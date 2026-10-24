//! Realistic shading with Bevy: physically based materials, a key light with
//! shadows and ambient light. Bevy runs headless on the UI's own wgpu device
//! and renders each Realistic viewport into an image the UI draws as the
//! viewport background, and renders the active view to a PNG for Render.

pub mod convert;

use std::collections::{HashMap, HashSet};

use bevy::app::{App, PluginsState, TerminalCtrlCHandlerPlugin};
use bevy::asset::{Assets, Handle, RenderAssetUsages};
use bevy::core_pipeline::core_3d::Camera3d;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::ecs::entity::Entity;
use bevy::image::{Image, ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Vec3;
use bevy::pbr::{AmbientLight, CascadeShadowConfigBuilder, DirectionalLight, DirectionalLightShadowMap, MeshMaterial3d, StandardMaterial};
use bevy::render::camera::{Camera, ClearColorConfig, RenderTarget};
use bevy::render::mesh::{Mesh, Mesh3d};
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::render::renderer::{RenderAdapter, RenderAdapterInfo, RenderDevice, RenderInstance, RenderQueue, WgpuWrapper};
use bevy::render::settings::{RenderCreation, RenderResources};
use bevy::render::texture::GpuImage;
use bevy::render::view::Msaa;
use bevy::render::{RenderApp, RenderPlugin};
use bevy::transform::components::Transform;
use bevy::window::{ExitCondition, WindowPlugin};
use bevy::DefaultPlugins;
use bevy::app::PluginGroup;
use bevy::color::{Alpha, Color};

pub use convert::{diffuse_map_path, material_from_loom, mesh_data, BucketProjection, MeshData, ViewCamera};

use bevy::render::camera::Projection;
use bevy::render::view::RenderLayers;

/// Render layers: the viewports see layer 0, a render sees the renderable
/// objects (1), only the selected ones (2) or hidden ones too (3).
const VIEWPORT_LAYER: usize = 0;
const RENDER_LAYER: usize = 1;
const SELECTED_LAYER: usize = 2;
const HIDDEN_LAYER: usize = 3;

/// Render Setup values a render uses (the Bevy renderer panel and the
/// Common options that reach the renderer).
#[derive(Debug, Clone, PartialEq)]
pub struct RenderOptions {
    pub msaa: bool,
    pub shadows: bool,
    pub shadow_map_size: u32,
    /// sRGB 0–1; the background's alpha is 0.
    pub background: [f32; 3],
    pub ambient: [f32; 3],
    pub force_two_sided: bool,
    /// Area to Render > Selected: only these nodes.
    pub selected_only: Option<Vec<u64>>,
    /// Render Hidden Geometry: the objects given to `sync_hidden` render too.
    pub hidden: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self { msaa: true, shadows: true, shadow_map_size: 4096, background: [0.0; 3], ambient: [1.0; 3], force_two_sided: false, selected_only: None, hidden: false }
    }
}

/// One visible object: its world-space mesh and how it is coloured.
pub struct SceneObject<'a> {
    pub id: u64,
    pub mesh: &'a loom_mesh::Mesh,
    pub wire_color: [u8; 3],
    pub material: Option<&'a loom_scene::material::StandardMaterial>,
    /// Shows in renders (geometry, not helpers or camera icons).
    pub renderable: bool,
}

struct Object {
    entity: Entity,
    mesh_hash: u64,
    material: Option<loom_scene::material::StandardMaterial>,
    wire_color: [u8; 3],
    material_handle: Handle<StandardMaterial>,
    renderable: bool,
}

struct View {
    camera: Entity,
    image: Handle<Image>,
    size: [u32; 2],
    msaa: bool,
    used: bool,
    /// Bumped whenever the view gets a new target image.
    generation: u64,
}

/// Slot of the offline Render camera.
const RENDER_SLOT: u32 = u32::MAX;
/// Key light: from the front right and above (world Z up), so the default
/// Perspective view (from the front left) sees the shadows fall beside objects.
const KEY_LIGHT_FROM: Vec3 = Vec3::new(0.8, -0.45, 1.0);

/// The device a shared Bevy renderer needs: the adapter's features (less
/// the ones Bevy itself leaves off) and its limits, as Bevy asks for them
/// when it creates its own device.
pub fn device_descriptor(adapter: &wgpu::Adapter) -> wgpu::DeviceDescriptor<'static> {
    let mut features = adapter.features();
    if adapter.get_info().device_type == wgpu::DeviceType::DiscreteGpu {
        features -= wgpu::Features::MAPPABLE_PRIMARY_BUFFERS;
    }
    features -= wgpu::Features::EXPERIMENTAL_RAY_QUERY;
    features -= wgpu::Features::EXPERIMENTAL_RAY_TRACING_ACCELERATION_STRUCTURE;
    wgpu::DeviceDescriptor { label: Some("3dLoom device"), required_features: features, required_limits: adapter.limits(), memory_hints: wgpu::MemoryHints::default() }
}

/// Whether Bevy can run on `adapter` (not on the GL fallback).
pub fn supported(adapter: &wgpu::Adapter) -> bool {
    adapter.get_info().backend != wgpu::Backend::Gl
}

/// A headless Bevy app on a shared wgpu device.
pub struct BevyRenderer {
    app: App,
    device: wgpu::Device,
    queue: wgpu::Queue,
    objects: HashMap<u64, Object>,
    views: HashMap<u32, View>,
    textures: HashMap<String, Option<Handle<Image>>>,
    light: Entity,
    /// Scene bounds (centre, radius) for the shadow range.
    bounds: Option<(Vec3, f32)>,
    /// Updates still wanted after a change (pipelines compiling, first frames).
    settle: u32,
    /// Hidden objects a Render Hidden Geometry render adds (render layer only).
    hidden: HashMap<u64, Object>,
    /// The render in progress: its options and what it changed, to restore.
    render: Option<ActiveRender>,
}

struct ActiveRender {
    options: RenderOptions,
    /// Materials made 2-sided by Force 2-Sided, with their own setting.
    two_sided: Vec<(Handle<StandardMaterial>, bool, Option<bevy::render::render_resource::Face>)>,
}

impl BevyRenderer {
    /// Builds the Bevy app on `device` / `queue` (the UI's). The instance is
    /// only used by Bevy to make window surfaces, which a headless app never does.
    pub fn new(adapter: wgpu::Adapter, device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let info = adapter.get_info();
        let resources = RenderResources(
            RenderDevice::from(device.clone()),
            RenderQueue(std::sync::Arc::new(WgpuWrapper::new(queue.clone()))),
            RenderAdapterInfo(WgpuWrapper::new(info)),
            RenderAdapter(std::sync::Arc::new(WgpuWrapper::new(adapter))),
            RenderInstance(std::sync::Arc::new(WgpuWrapper::new(wgpu::Instance::new(&wgpu::InstanceDescriptor { backends: wgpu::Backends::empty(), ..Default::default() })))),
        );
        let mut app = App::new();
        app.add_plugins(
            DefaultPlugins
                .build()
                // The host program owns Ctrl+C.
                .disable::<TerminalCtrlCHandlerPlugin>()
                .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, close_when_requested: false })
                .set(RenderPlugin { render_creation: RenderCreation::Manual(resources), synchronous_pipeline_compilation: true, ..Default::default() }),
        );
        while app.plugins_state() == PluginsState::Adding {
            bevy::tasks::tick_global_task_pools_on_main_thread();
        }
        app.finish();
        app.cleanup();
        app.insert_resource(AmbientLight { color: Color::WHITE, brightness: 250.0, affects_lightmapped_meshes: true });
        app.insert_resource(DirectionalLightShadowMap { size: 2048 });
        let light = app
            .world_mut()
            .spawn((
                DirectionalLight { illuminance: 3000.0, shadows_enabled: true, ..Default::default() },
                Transform::from_translation(KEY_LIGHT_FROM).looking_at(Vec3::ZERO, Vec3::Z),
                // The key light lights (and shadows) every layer.
                RenderLayers::from_layers(&[VIEWPORT_LAYER, RENDER_LAYER, SELECTED_LAYER, HIDDEN_LAYER]),
            ))
            .id();
        Self { app, device, queue, objects: HashMap::new(), views: HashMap::new(), textures: HashMap::new(), light, bounds: None, settle: 3, hidden: HashMap::new(), render: None }
    }

    /// The layers an object is on: viewports, renders, and the selection's
    /// layer while a Selected render runs.
    fn object_layers(&self, id: u64, renderable: bool, hidden: bool) -> RenderLayers {
        if hidden {
            return RenderLayers::layer(HIDDEN_LAYER);
        }
        let mut l = vec![VIEWPORT_LAYER];
        if renderable {
            l.push(RENDER_LAYER);
            if self.render.as_ref().and_then(|r| r.options.selected_only.as_ref()).is_some_and(|s| s.contains(&id)) {
                l.push(SELECTED_LAYER);
            }
        }
        RenderLayers::from_layers(&l)
    }

    /// Brings the Bevy scene in line with `objects`: new nodes are spawned,
    /// changed meshes or materials rebuilt, removed nodes despawned.
    pub fn sync(&mut self, objects: &[SceneObject]) {
        let mut set = std::mem::take(&mut self.objects);
        self.bounds = self.sync_set(&mut set, objects, false);
        self.objects = set;
    }

    /// The hidden objects a Render Hidden Geometry render draws (seen by
    /// renders only); an empty list removes them.
    pub fn sync_hidden(&mut self, objects: &[SceneObject]) {
        let mut set = std::mem::take(&mut self.hidden);
        self.sync_set(&mut set, objects, true);
        self.hidden = set;
    }

    /// Brings `set` in line with `objects`; returns their bounds.
    fn sync_set(&mut self, set: &mut HashMap<u64, Object>, objects: &[SceneObject], hidden: bool) -> Option<(Vec3, f32)> {
        let mut seen = HashSet::new();
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        for o in objects {
            seen.insert(o.id);
            let data = mesh_data(o.mesh);
            if let Some(b) = data.aabb() {
                lo = lo.min(Vec3::from(b.min()));
                hi = hi.max(Vec3::from(b.max()));
            }
            let hash = data.content_hash();
            let material_changed = |obj: &Object| obj.material.as_ref() != o.material || obj.wire_color != o.wire_color;
            let layers = self.object_layers(o.id, o.renderable, hidden);
            match set.get(&o.id) {
                Some(obj) if obj.mesh_hash == hash && !material_changed(obj) && obj.renderable == o.renderable => continue,
                Some(obj) if obj.mesh_hash == hash && !material_changed(obj) => {
                    let entity = obj.entity;
                    self.app.world_mut().entity_mut(entity).insert(layers);
                    set.get_mut(&o.id).expect("present").renderable = o.renderable;
                }
                Some(obj) if obj.mesh_hash == hash => {
                    let handle = obj.material_handle.clone();
                    let mat = self.material(o);
                    self.app.world_mut().resource_mut::<Assets<StandardMaterial>>().insert(&handle, mat);
                    let obj = set.get_mut(&o.id).expect("present");
                    obj.material = o.material.cloned();
                    obj.wire_color = o.wire_color;
                    obj.renderable = o.renderable;
                    let entity = obj.entity;
                    self.app.world_mut().entity_mut(entity).insert(layers);
                }
                _ => {
                    let mat = self.material(o);
                    let world = self.app.world_mut();
                    let mesh = world.resource_mut::<Assets<Mesh>>().add(data.to_bevy());
                    let material_handle = world.resource_mut::<Assets<StandardMaterial>>().add(mat);
                    let mut e = match set.get(&o.id) {
                        Some(obj) => world.entity_mut(obj.entity),
                        None => world.spawn(Transform::IDENTITY),
                    };
                    e.insert((Mesh3d(mesh), MeshMaterial3d(material_handle.clone()), layers));
                    // Bevy computes bounds only once per entity: keep them current.
                    match data.aabb() {
                        Some(b) => {
                            e.insert(b);
                        }
                        None => {
                            e.remove::<bevy::render::primitives::Aabb>();
                        }
                    }
                    let entity = e.id();
                    if let Some(r) = self.render.as_mut().filter(|r| r.options.force_two_sided) {
                        // A render in progress with Force 2-Sided: new materials too.
                        let mut mats = self.app.world_mut().resource_mut::<Assets<StandardMaterial>>();
                        if let Some(m) = mats.get_mut(&material_handle) {
                            r.two_sided.push((material_handle.clone(), m.double_sided, m.cull_mode));
                            m.double_sided = true;
                            m.cull_mode = None;
                        }
                    }
                    set.insert(o.id, Object { entity, mesh_hash: hash, material: o.material.cloned(), wire_color: o.wire_color, material_handle, renderable: o.renderable });
                }
            }
            self.settle = self.settle.max(3);
        }
        let gone: Vec<u64> = set.keys().filter(|id| !seen.contains(id)).copied().collect();
        for id in gone {
            if let Some(obj) = set.remove(&id) {
                self.app.world_mut().despawn(obj.entity);
            }
            self.settle = self.settle.max(3);
        }
        (lo.x <= hi.x).then(|| ((lo + hi) * 0.5, ((hi - lo) * 0.5).length().max(1e-3)))
    }

    fn material(&mut self, o: &SceneObject) -> StandardMaterial {
        let mut m = material_from_loom(o.material, o.wire_color);
        if let Some(path) = diffuse_map_path(o.material) {
            m.base_color_texture = self.texture(path);
            if m.base_color_texture.is_some() && o.material.is_some() {
                // The map replaces the diffuse colour, as in the shaded viewport.
                m.base_color = Color::srgba(1.0, 1.0, 1.0, m.base_color.alpha());
            }
        }
        m
    }

    /// A bitmap loaded once per path (None when it cannot be read).
    fn texture(&mut self, path: &str) -> Option<Handle<Image>> {
        if let Some(t) = self.textures.get(path) {
            return t.clone();
        }
        let handle = image::open(path).ok().map(|img| {
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            let mut image = Image::new(Extent3d { width: w, height: h, depth_or_array_layers: 1 }, TextureDimension::D2, rgba.into_raw(), TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD);
            image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                address_mode_v: ImageAddressMode::Repeat,
                ..ImageSamplerDescriptor::linear()
            });
            self.app.world_mut().resource_mut::<Assets<Image>>().add(image)
        });
        self.textures.insert(path.to_owned(), handle.clone());
        handle
    }

    fn target_image(size: [u32; 2]) -> Image {
        let mut image = Image::new_uninit(
            Extent3d { width: size[0].max(1), height: size[1].max(1), depth_or_array_layers: 1 },
            TextureDimension::D2,
            TextureFormat::Rgba8UnormSrgb,
            // Kept in the main world too: Bevy reads the target's size from it
            // whenever the projection changes (it holds no pixel data).
            RenderAssetUsages::default(),
        );
        image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_SRC | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
        image
    }

    /// Points view `slot`'s camera at `camera` and renders it at `size`
    /// pixels on the next `update`, cleared to `clear` (sRGB, alpha 0 lets
    /// the background behind the image show). Views not set before an update stop rendering.
    pub fn set_view(&mut self, slot: u32, size: [u32; 2], camera: &ViewCamera, clear: [f32; 4], msaa: bool) {
        let aspect = size[0].max(1) as f32 / size[1].max(1) as f32;
        self.place_view(slot, size, camera.bevy_projection(aspect), camera.view, clear, msaa, RenderLayers::layer(VIEWPORT_LAYER));
    }

    #[allow(clippy::too_many_arguments)]
    fn place_view(&mut self, slot: u32, size: [u32; 2], projection: Projection, view: bevy::math::Mat4, clear: [f32; 4], msaa: bool, layers: RenderLayers) {
        let clear = Color::srgba(clear[0], clear[1], clear[2], clear[3]);
        let size = [size[0].max(1), size[1].max(1)];
        let changed = match self.views.get(&slot) {
            Some(v) => v.size != size || v.msaa != msaa,
            None => true,
        };
        if changed {
            let image = self.app.world_mut().resource_mut::<Assets<Image>>().add(Self::target_image(size));
            match self.views.get_mut(&slot) {
                Some(v) => {
                    v.image = image.clone();
                    v.size = size;
                    v.msaa = msaa;
                    v.generation += 1;
                }
                None => {
                    let entity = self
                        .app
                        .world_mut()
                        .spawn((Camera3d::default(), Tonemapping::AcesFitted, Camera { order: slot.min(1 << 20) as isize, ..Default::default() }))
                        .id();
                    self.views.insert(slot, View { camera: entity, image: image.clone(), size, msaa, used: false, generation: 0 });
                }
            }
            self.settle = self.settle.max(3);
        }
        let v = self.views.get_mut(&slot).expect("inserted above");
        v.used = true;
        let (entity, image) = (v.camera, v.image.clone());
        let mut e = self.app.world_mut().entity_mut(entity);
        // Camera is changed in place: inserting it anew would drop the
        // render target size Bevy computed, and the view would stop rendering.
        if let Some(mut cam) = e.get_mut::<Camera>() {
            if !matches!(&cam.target, RenderTarget::Image(t) if t.handle == image) {
                cam.target = RenderTarget::Image(image.into());
            }
            cam.clear_color = ClearColorConfig::Custom(clear);
            cam.is_active = true;
        }
        e.insert((projection, Transform::from_matrix(view.inverse()), if msaa { Msaa::Sample4 } else { Msaa::Off }, layers));
    }

    /// Starts a render with `options`: shadows, shadow map size, ambient
    /// light, Force 2-Sided and the Selected layer apply until `end_render`
    /// (the Realistic viewports see them meanwhile too).
    pub fn begin_render(&mut self, options: RenderOptions) {
        self.end_render();
        let world = self.app.world_mut();
        world.insert_resource(DirectionalLightShadowMap { size: options.shadow_map_size.clamp(512, 8192) as usize });
        world.insert_resource(AmbientLight { color: Color::srgb(options.ambient[0], options.ambient[1], options.ambient[2]), brightness: 250.0, affects_lightmapped_meshes: true });
        if let Some(mut l) = world.get_mut::<DirectionalLight>(self.light) {
            l.shadows_enabled = options.shadows;
        }
        let mut two_sided = Vec::new();
        if options.force_two_sided {
            let handles: Vec<Handle<StandardMaterial>> = self.objects.values().chain(self.hidden.values()).map(|o| o.material_handle.clone()).collect();
            let mut mats = world.resource_mut::<Assets<StandardMaterial>>();
            for h in handles {
                if let Some(m) = mats.get_mut(&h) {
                    two_sided.push((h, m.double_sided, m.cull_mode));
                    m.double_sided = true;
                    m.cull_mode = None;
                }
            }
        }
        self.render = Some(ActiveRender { options, two_sided });
        self.relayer();
        self.settle = self.settle.max(3);
    }

    /// Puts every object on its layers again (the Selected layer changed).
    fn relayer(&mut self) {
        let list: Vec<(Entity, RenderLayers)> = self.objects.iter().map(|(id, o)| (o.entity, self.object_layers(*id, o.renderable, false))).collect();
        for (e, l) in list {
            self.app.world_mut().entity_mut(e).insert(l);
        }
    }

    /// Renders bucket `sub` = [u0, v0, u1, v1] (fractions of the frame, v
    /// down) of `camera` at image aspect `aspect` into a `size` image on the
    /// next `update`; read it back with `read_render`. Needs `begin_render`.
    pub fn set_render_bucket(&mut self, camera: &ViewCamera, aspect: f32, sub: [f32; 4], size: [u32; 2]) -> Result<(), String> {
        let Some(r) = self.render.as_ref() else { return Err("no render started".into()) };
        let o = &r.options;
        let layers = match &o.selected_only {
            Some(_) => RenderLayers::layer(SELECTED_LAYER),
            None if o.hidden => RenderLayers::from_layers(&[RENDER_LAYER, HIDDEN_LAYER]),
            None => RenderLayers::layer(RENDER_LAYER),
        };
        let clear = [o.background[0], o.background[1], o.background[2], 0.0];
        let msaa = o.msaa;
        self.place_view(RENDER_SLOT, size, Projection::custom(BucketProjection::new(camera, aspect, sub)), camera.view, clear, msaa, layers);
        Ok(())
    }

    /// The last bucket rendered (sRGB RGBA, background alpha 0).
    pub fn read_render(&self) -> Result<image::RgbaImage, String> {
        self.read_back(RENDER_SLOT)
    }

    /// Ends the render: its camera goes, the scene's own settings return.
    pub fn end_render(&mut self) {
        if let Some(v) = self.views.remove(&RENDER_SLOT) {
            self.app.world_mut().despawn(v.camera);
        }
        let Some(r) = self.render.take() else { return };
        let world = self.app.world_mut();
        world.insert_resource(DirectionalLightShadowMap { size: 2048 });
        world.insert_resource(AmbientLight { color: Color::WHITE, brightness: 250.0, affects_lightmapped_meshes: true });
        if let Some(mut l) = world.get_mut::<DirectionalLight>(self.light) {
            l.shadows_enabled = true;
        }
        let mut mats = world.resource_mut::<Assets<StandardMaterial>>();
        for (h, double_sided, cull) in r.two_sided {
            if let Some(m) = mats.get_mut(&h) {
                m.double_sided = double_sided;
                m.cull_mode = cull;
            }
        }
        if r.options.selected_only.is_some() {
            self.relayer();
        }
        self.settle = self.settle.max(3);
    }

    /// Fits the key light's shadow range around the scene as seen from the active views.
    fn fit_shadows(&mut self) {
        let Some((centre, radius)) = self.bounds else { return };
        let mut range: Option<(f32, f32)> = None;
        for v in self.views.values().filter(|v| v.used) {
            let Some(t) = self.app.world().get::<Transform>(v.camera) else { continue };
            // Distance along the view direction to the scene centre.
            let d = (centre - t.translation).dot(t.forward().as_vec3());
            let (a, b) = ((d - radius).max(0.0), (d + radius).max(1e-3));
            range = Some(range.map_or((a, b), |(lo, hi)| (lo.min(a), hi.max(b))));
        }
        let Some((min, max)) = range else { return };
        let config = CascadeShadowConfigBuilder { num_cascades: 1, minimum_distance: min, maximum_distance: max.max(min + 1e-3), first_cascade_far_bound: max.max(min + 1e-3), overlap_proportion: 0.2 }.build();
        self.app.world_mut().entity_mut(self.light).insert(config);
    }

    /// Renders every view set since the last update.
    pub fn update(&mut self) {
        let world = self.app.world_mut();
        for v in self.views.values_mut() {
            if !v.used {
                if let Some(mut cam) = world.get_mut::<Camera>(v.camera) {
                    cam.is_active = false;
                }
            }
        }
        self.fit_shadows();
        self.app.update();
        for v in self.views.values_mut() {
            v.used = false;
        }
        // Pipelines compile synchronously (no `multi_threaded`), so a change
        // shows within a frame or two; a pipeline cache "waiting" list is no
        // signal (pipelines whose shaders never load stay in it).
        self.settle = self.settle.saturating_sub(1);
    }

    /// True while pipelines or new assets still need frames to appear.
    pub fn needs_more_frames(&self) -> bool {
        self.settle > 0
    }

    /// The GPU texture view `slot` rendered into, once it exists, with the
    /// view's generation (it changes when the view gets a new image).
    pub fn view_texture(&self, slot: u32) -> Option<(u64, wgpu::TextureView)> {
        let v = self.views.get(&slot)?;
        let images = self.app.sub_app(RenderApp).world().get_resource::<RenderAssets<GpuImage>>()?;
        let gpu = images.get(&v.image)?;
        (gpu.size.width == v.size[0] && gpu.size.height == v.size[1]).then(|| (v.generation, (*gpu.texture_view).clone()))
    }

    /// Renders `camera` at `width` x `height` in one go with the default
    /// render options (4x MSAA, 4096 px shadows) and reads the pixels back
    /// (sRGB RGBA, black background with alpha 0).
    pub fn render_image(&mut self, width: u32, height: u32, camera: &ViewCamera) -> Result<image::RgbaImage, String> {
        if width == 0 || height == 0 {
            return Err(format!("render size {width}x{height}"));
        }
        self.begin_render(RenderOptions::default());
        // A few frames: new pipelines, the target image and the shadow map settle first.
        let aspect = width as f32 / height as f32;
        for _ in 0..4 {
            self.set_render_bucket(camera, aspect, [0.0, 0.0, 1.0, 1.0], [width, height])?;
            self.update();
        }
        let result = self.read_render();
        self.end_render();
        result
    }

    /// Reads back what view `slot` rendered last (sRGB RGBA).
    pub fn read_view(&self, slot: u32) -> Result<image::RgbaImage, String> {
        self.read_back(slot)
    }

    fn read_back(&self, slot: u32) -> Result<image::RgbaImage, String> {
        let v = self.views.get(&slot).ok_or("no render view")?;
        let images = self.app.sub_app(RenderApp).world().get_resource::<RenderAssets<GpuImage>>().ok_or("no GPU images")?;
        let gpu = images.get(&v.image).ok_or("render target not on the GPU yet")?;
        let (w, h) = (gpu.size.width, gpu.size.height);
        let row = w * 4;
        let padded = row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("loom_bevy readback"), size: (padded * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("loom_bevy readback") });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &gpu.texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo { buffer: &buffer, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(h) } },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.queue.submit([encoder.finish()]);
        let slice = buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let _ = self.device.poll(wgpu::Maintain::Wait);
        rx.recv().map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
        let data = slice.get_mapped_range();
        let mut pixels = Vec::with_capacity((row * h) as usize);
        for y in 0..h as usize {
            pixels.extend_from_slice(&data[y * padded as usize..y * padded as usize + row as usize]);
        }
        drop(data);
        buffer.unmap();
        image::RgbaImage::from_raw(w, h, pixels).ok_or_else(|| "readback size mismatch".to_owned())
    }
}
