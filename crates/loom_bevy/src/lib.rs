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

pub use convert::{diffuse_map_path, material_from_loom, mesh_data, MeshData, ViewCamera};

/// One visible object: its world-space mesh and how it is coloured.
pub struct SceneObject<'a> {
    pub id: u64,
    pub mesh: &'a loom_mesh::Mesh,
    pub wire_color: [u8; 3],
    pub material: Option<&'a loom_scene::material::StandardMaterial>,
}

struct Object {
    entity: Entity,
    mesh_hash: u64,
    material: Option<loom_scene::material::StandardMaterial>,
    wire_color: [u8; 3],
    material_handle: Handle<StandardMaterial>,
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
            ))
            .id();
        Self { app, device, queue, objects: HashMap::new(), views: HashMap::new(), textures: HashMap::new(), light, bounds: None, settle: 3 }
    }

    /// Brings the Bevy scene in line with `objects`: new nodes are spawned,
    /// changed meshes or materials rebuilt, removed nodes despawned.
    pub fn sync(&mut self, objects: &[SceneObject]) {
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
            match self.objects.get(&o.id) {
                Some(obj) if obj.mesh_hash == hash && !material_changed(obj) => continue,
                Some(obj) if obj.mesh_hash == hash => {
                    let handle = obj.material_handle.clone();
                    let mat = self.material(o);
                    self.app.world_mut().resource_mut::<Assets<StandardMaterial>>().insert(&handle, mat);
                    let obj = self.objects.get_mut(&o.id).expect("present");
                    obj.material = o.material.cloned();
                    obj.wire_color = o.wire_color;
                }
                _ => {
                    let mat = self.material(o);
                    let world = self.app.world_mut();
                    let mesh = world.resource_mut::<Assets<Mesh>>().add(data.to_bevy());
                    let material_handle = world.resource_mut::<Assets<StandardMaterial>>().add(mat);
                    let mut e = match self.objects.get(&o.id) {
                        Some(obj) => world.entity_mut(obj.entity),
                        None => world.spawn(Transform::IDENTITY),
                    };
                    e.insert((Mesh3d(mesh), MeshMaterial3d(material_handle.clone())));
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
                    self.objects.insert(o.id, Object { entity, mesh_hash: hash, material: o.material.cloned(), wire_color: o.wire_color, material_handle });
                }
            }
            self.settle = self.settle.max(3);
        }
        let gone: Vec<u64> = self.objects.keys().filter(|id| !seen.contains(id)).copied().collect();
        for id in gone {
            if let Some(obj) = self.objects.remove(&id) {
                self.app.world_mut().despawn(obj.entity);
            }
            self.settle = self.settle.max(3);
        }
        self.bounds = (lo.x <= hi.x).then(|| ((lo + hi) * 0.5, ((hi - lo) * 0.5).length().max(1e-3)));
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
        let aspect = size[0] as f32 / size[1] as f32;
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
        e.insert((camera.bevy_projection(aspect), Transform::from_matrix(camera.view.inverse()), if msaa { Msaa::Sample4 } else { Msaa::Off }));
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

    /// Renders `camera` at `width` x `height` with 4x MSAA and 4096 px
    /// shadows, and reads the pixels back (sRGB RGBA, opaque black background).
    pub fn render_image(&mut self, width: u32, height: u32, camera: &ViewCamera) -> Result<image::RgbaImage, String> {
        if width == 0 || height == 0 {
            return Err(format!("render size {width}x{height}"));
        }
        self.app.insert_resource(DirectionalLightShadowMap { size: 4096 });
        // A few frames: new pipelines, the target image and the shadow map settle first.
        for _ in 0..4 {
            self.set_view(RENDER_SLOT, [width, height], camera, [0.0, 0.0, 0.0, 1.0], true);
            self.update();
        }
        let result = self.read_back(RENDER_SLOT);
        // The offline camera and its image are not kept.
        if let Some(v) = self.views.remove(&RENDER_SLOT) {
            self.app.world_mut().despawn(v.camera);
        }
        self.app.insert_resource(DirectionalLightShadowMap { size: 2048 });
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
