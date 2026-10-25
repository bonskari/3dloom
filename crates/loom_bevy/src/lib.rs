//! Realistic shading with Bevy: physically based materials, a key light with
//! shadows and ambient light. Bevy runs headless on the UI's own wgpu device
//! and renders each Realistic viewport into an image the UI draws as the
//! viewport background, and renders the active view to a PNG for Render.

pub mod bake;
pub mod convert;
pub mod lights;
pub mod quality;

use std::collections::{HashMap, HashSet};

use bevy::app::{App, PluginsState, TerminalCtrlCHandlerPlugin};
use bevy::asset::{Assets, Handle, RenderAssetUsages};
use bevy::core_pipeline::core_3d::Camera3d;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::ecs::entity::Entity;
use bevy::image::{Image, ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Vec3;
use bevy::pbr::{AmbientLight, CascadeShadowConfigBuilder, DirectionalLight, DirectionalLightShadowMap, MeshMaterial3d, PointLightShadowMap, StandardMaterial};
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

pub use convert::{diffuse_map_path, material_from_loom, material_from_surface, mesh_data, mesh_data_by_id, BucketProjection, MeshData, ViewCamera};
use loom_scene::material::{Map, Surface, Surfaces};

use bevy::render::camera::Projection;
use bevy::render::view::RenderLayers;

/// Render layers: the viewports see layer 0, a render sees the renderable
/// objects (1), only the selected ones (2) or hidden ones too (3).
const VIEWPORT_LAYER: usize = 0;
const RENDER_LAYER: usize = 1;
const SELECTED_LAYER: usize = 2;
const HIDDEN_LAYER: usize = 3;
/// Scene lights are on this layer, which every camera sees and no object is
/// on, plus `LIGHT_LAYER + 1 + i` for light i: an object is on a light's own
/// layer unless the light excludes it from shadow casting.
const LIGHT_LAYER: usize = 4;
/// Objects that renders do not see but whose shadows still render (Visible
/// to Camera off) with the default key light: only that light is on this
/// layer. Kept far above the per-light layers.
const SHADOW_ONLY_LAYER: usize = 1023;
/// Brightness of the ambient light that the Render Setup colour tints.
const AMBIENT_BRIGHTNESS: f32 = 250.0;

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
    /// The panel's quality controls: antialiasing filter, supersampling,
    /// passes, area shadows, ambient occlusion, reflections, sky lighting
    /// and exposure control (its shadows, map size and colours are the fields above).
    pub quality: loom_scene::render::BevyRenderSettings,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self { msaa: true, shadows: true, shadow_map_size: 4096, background: [0.0; 3], ambient: [1.0; 3], force_two_sided: false, selected_only: None, hidden: false, quality: Default::default() }
    }
}

/// The world scale a render with ambient occlusion uses: Bevy's ambient
/// occlusion reaches a fixed 0.73 world units, so the world shrinks until
/// that is the AO Radius.
const AO_REACH: f32 = 0.5 * 1.457;

impl RenderOptions {
    /// Whether the render camera needs the screen-space passes (and so no MSAA).
    fn screen_space(&self) -> bool {
        self.quality.ambient_occlusion || self.quality.reflections
    }

    fn world_scale(&self) -> f32 {
        if self.quality.ambient_occlusion {
            AO_REACH / self.quality.ao_radius.max(1e-3)
        } else {
            1.0
        }
    }

    /// Passes per bucket.
    fn passes(&self) -> u32 {
        self.quality.passes.clamp(1, 64)
    }
}

/// One visible object: its world-space mesh and how it is coloured.
pub struct SceneObject<'a> {
    pub id: u64,
    pub mesh: &'a loom_mesh::Mesh,
    pub wire_color: [u8; 3],
    /// Its material resolved per face material ID (`Scene::node_surfaces`).
    pub surfaces: Option<Surfaces>,
    /// Shows in renders (geometry, not helpers or camera icons).
    pub renderable: bool,
    /// Object Properties > Rendering Control values.
    pub flags: ObjectFlags,
}

/// Object Properties > Rendering Control as the renderer applies it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObjectFlags {
    pub cast_shadows: bool,
    pub receive_shadows: bool,
    /// Off: renders leave the object out but keep its shadows.
    pub visible_to_camera: bool,
    /// Rendered visibility 0–1 (multiplies the material's opacity; 0 leaves
    /// the object and its shadows out of renders).
    pub visibility: f32,
}

impl Default for ObjectFlags {
    fn default() -> Self {
        Self { cast_shadows: true, receive_shadows: true, visible_to_camera: true, visibility: 1.0 }
    }
}

struct Object {
    /// One entity (and material) per face material ID drawn; one for a single material.
    parts: Vec<(Entity, Handle<StandardMaterial>)>,
    mesh_hash: u64,
    surfaces: Option<Surfaces>,
    wire_color: [u8; 3],
    renderable: bool,
    flags: ObjectFlags,
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
    /// Image files as maps read them, by path.
    images: loom_scene::maps::Images,
    /// The default key light, while the scene has no light that is on.
    light: Option<Entity>,
    /// The scene's lights (from `sync_lights`) and their Bevy entities by node id.
    samples: Vec<loom_scene::light::LightSample>,
    scene_lights: HashMap<u64, lights::Synced>,
    /// Scene bounds (centre, radius) for the shadow range.
    bounds: Option<(Vec3, f32)>,
    /// Updates still wanted after a change (pipelines compiling, first frames).
    settle: u32,
    /// Hidden objects a Render Hidden Geometry render adds (render layer only).
    hidden: HashMap<u64, Object>,
    /// The render in progress: its options and what it changed, to restore.
    render: Option<ActiveRender>,
    /// World units per scene unit (see `AO_REACH`); 1 outside renders.
    scale: f32,
    /// The pass the lights are placed for (their Source Radius jitter).
    light_pass: u32,
}

struct ActiveRender {
    options: RenderOptions,
    /// Materials made 2-sided by Force 2-Sided, with their own setting.
    two_sided: Vec<(Handle<StandardMaterial>, bool, Option<bevy::render::render_resource::Face>)>,
    /// Materials drawn deferred for the reflections, with their own method.
    deferred: Vec<(Handle<StandardMaterial>, bevy::pbr::OpaqueRendererMethod)>,
    /// The render camera has its quality components.
    camera_ready: bool,
    /// The bucket's passes so far.
    accum: Option<quality::Accum>,
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
        app.add_plugins(bake::BakePlugin);
        while app.plugins_state() == PluginsState::Adding {
            bevy::tasks::tick_global_task_pools_on_main_thread();
        }
        app.finish();
        app.cleanup();
        app.insert_resource(AmbientLight { color: Color::WHITE, brightness: AMBIENT_BRIGHTNESS, affects_lightmapped_meshes: true });
        app.insert_resource(DirectionalLightShadowMap { size: 2048 });
        let light = Some(Self::spawn_key_light(&mut app));
        Self { app, device, queue, objects: HashMap::new(), views: HashMap::new(), textures: HashMap::new(), images: Default::default(), light, samples: Vec::new(), scene_lights: HashMap::new(), bounds: None, settle: 3, hidden: HashMap::new(), render: None, scale: 1.0, light_pass: 0 }
    }

    /// Default lighting: a key light with shadows.
    fn spawn_key_light(app: &mut App) -> Entity {
        app.world_mut()
            .spawn((
                DirectionalLight { illuminance: 3000.0, shadows_enabled: true, ..Default::default() },
                Transform::from_translation(KEY_LIGHT_FROM).looking_at(Vec3::ZERO, Vec3::Z),
                // The key light lights (and shadows) every layer.
                RenderLayers::from_layers(&[VIEWPORT_LAYER, RENDER_LAYER, SELECTED_LAYER, HIDDEN_LAYER, SHADOW_ONLY_LAYER, bake::BAKE_KEY_LAYER]),
            ))
            .id()
    }

    /// Brings the Bevy lights in line with the scene's lights (call after
    /// `sync`, which measures the scene). With none, the default key light
    /// lights the scene; otherwise the scene lights replace it. A render's
    /// Render Setup options (shadows, shadow map size, ambient) apply on top.
    pub fn sync_lights(&mut self, samples: &[loom_scene::light::LightSample]) {
        let relayer = samples.iter().map(|s| (s.id, &s.exclusion)).ne(self.samples.iter().map(|s| (s.id, &s.exclusion)));
        self.samples = samples.to_vec();
        self.apply_lights();
        if relayer {
            self.relayer();
        }
        self.settle = self.settle.max(3);
    }

    /// Builds the Bevy light entities and the ambient light from the scene
    /// lights and the render in progress.
    fn apply_lights(&mut self) {
        let (centre, radius) = self.bounds.unwrap_or((Vec3::ZERO, 1.0));
        let options = self.render.as_ref().map(|r| r.options.clone());
        let shadows = options.as_ref().is_none_or(|o| o.shadows);
        let tint = options.as_ref().map_or([1.0; 3], |o| o.ambient);
        if self.samples.is_empty() {
            if self.light.is_none() {
                self.light = Some(Self::spawn_key_light(&mut self.app));
            }
        } else if let Some(e) = self.light.take() {
            self.app.world_mut().despawn(e);
        }
        if let Some(e) = self.light {
            if let Some(mut l) = self.app.world_mut().get_mut::<DirectionalLight>(e) {
                l.shadows_enabled = shadows;
            }
        }
        let (color, b) = self.ambient_light(tint);
        self.app.insert_resource(AmbientLight { color, brightness: b, affects_lightmapped_meshes: true });
        let mut seen = HashSet::new();
        let mut map_size = 0usize;
        let samples = self.samples.clone();
        let scale = self.scale;
        // Area Shadows: each pass moves the lights over their Source Radius
        // (a point light then has no radius of its own: the passes are its size).
        let area = options.as_ref().filter(|o| o.quality.area_shadows && o.passes() > 1).map(|o| quality::disk(self.light_pass, o.passes()));
        for (i, s) in samples.iter().enumerate() {
            let Some(kind) = lights::kind(s) else { continue };
            seen.insert(s.id.0);
            let centre_distance = (Vec3::from(s.position.to_array()) - centre).length();
            let world = self.app.world_mut();
            let entity = match self.scene_lights.get(&s.id.0) {
                Some(l) if l.kind == kind => l.entity,
                other => {
                    if let Some(l) = other {
                        world.despawn(l.entity);
                    }
                    let e = world.spawn(Transform::IDENTITY).id();
                    self.scene_lights.insert(s.id.0, lights::Synced { entity: e, kind });
                    e
                }
            };
            let mut s = s.clone();
            s.shadows &= shadows;
            if let (Some(d), true) = (area, s.source_radius > 0.0) {
                lights::move_over_source(&mut s, d, centre);
                s.source_radius = 0.0;
            }
            let mut transform = lights::transform(&s);
            transform.translation *= scale;
            let mut e = world.entity_mut(entity);
            e.insert((transform, RenderLayers::from_layers(&[LIGHT_LAYER, LIGHT_LAYER + 1 + i])));
            match kind {
                lights::Kind::Point => {
                    e.insert(lights::scaled_point(lights::point(&s, centre_distance, radius), scale));
                }
                lights::Kind::Spot => {
                    e.insert(lights::scaled_spot(lights::spot(&s, centre_distance, radius), scale));
                }
                lights::Kind::Directional => {
                    e.insert(lights::directional(&s));
                }
            }
            if s.shadows && kind != lights::Kind::Directional {
                map_size = map_size.max(s.map_size as usize);
            }
        }
        let gone: Vec<u64> = self.scene_lights.keys().filter(|id| !seen.contains(id)).copied().collect();
        for id in gone {
            if let Some(l) = self.scene_lights.remove(&id) {
                self.app.world_mut().despawn(l.entity);
            }
        }
        if let Some(o) = options.as_ref() {
            // Renders give no light a smaller map than Render Setup's.
            map_size = map_size.max(o.shadow_map_size as usize);
        }
        if map_size > 0 {
            self.app.insert_resource(PointLightShadowMap { size: map_size.clamp(128, 8192) });
        }
    }

    /// The ambient light's colour (largest channel 1) and brightness: the
    /// Render Setup ambient colour (sRGB `tint`), plus the skylights and Ambient Only lights.
    fn ambient_light(&self, tint: [f32; 3]) -> (Color, f32) {
        let t = Color::srgb(tint[0], tint[1], tint[2]).to_linear();
        let (sky_color, sky) = lights::ambient(&self.samples);
        let sky_color = sky_color.to_linear();
        let sum = Vec3::new(t.red, t.green, t.blue) * AMBIENT_BRIGHTNESS + Vec3::new(sky_color.red, sky_color.green, sky_color.blue) * sky;
        let b = sum.max_element();
        let color = if b > 0.0 { Color::linear_rgb(sum.x / b, sum.y / b, sum.z / b) } else { Color::BLACK };
        (color, b.max(0.0))
    }

    /// The Bevy light entity of scene light `id` (tests and debugging).
    pub fn light_entity(&self, id: u64) -> Option<Entity> {
        self.scene_lights.get(&id).map(|l| l.entity)
    }

    /// Whether the default key light is lighting the scene.
    pub fn default_lighting(&self) -> bool {
        self.light.is_some()
    }

    /// The layers an object is on: viewports, renders, and the selection's
    /// layer while a Selected render runs. Visible to Camera off puts the
    /// render part on the shadow-only layer instead (scene lights keep
    /// shadowing it through their own layers).
    fn object_layers(&self, id: u64, renderable: bool, flags: ObjectFlags, hidden: bool) -> RenderLayers {
        let renderable = renderable && flags.visibility > 0.0;
        let mut l = match (hidden, flags.visible_to_camera) {
            (true, true) => vec![HIDDEN_LAYER],
            (true, false) => vec![SHADOW_ONLY_LAYER],
            (false, _) => vec![VIEWPORT_LAYER],
        };
        if renderable && !hidden && !flags.visible_to_camera {
            l.push(SHADOW_ONLY_LAYER);
        } else if renderable && !hidden {
            l.push(RENDER_LAYER);
            if self.render.as_ref().and_then(|r| r.options.selected_only.as_ref()).is_some_and(|s| s.contains(&id)) {
                l.push(SELECTED_LAYER);
            }
        }
        // The layers of the scene lights whose shadows the object casts.
        for (i, s) in self.samples.iter().enumerate() {
            if !s.exclusion.excludes(loom_scene::NodeId(id), false) {
                l.push(LIGHT_LAYER + 1 + i);
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
            let hash = data.content_hash() ^ convert::material_ids_hash(o.mesh).rotate_left(1);
            let layers = self.object_layers(o.id, o.renderable, o.flags, hidden);
            if let Some(obj) = set.get_mut(&o.id) {
                if obj.mesh_hash == hash && obj.surfaces == o.surfaces && obj.wire_color == o.wire_color && obj.flags.visibility == o.flags.visibility {
                    if obj.renderable != o.renderable || obj.flags != o.flags {
                        obj.renderable = o.renderable;
                        obj.flags = o.flags;
                        for (e, _) in obj.parts.clone() {
                            Self::shadow_flags(self.app.world_mut().entity_mut(e), Self::cast_flags(o)).insert(layers.clone());
                        }
                        self.settle = self.settle.max(3);
                    }
                    continue;
                }
            }
            // (material ID drawn or all faces, its surface)
            let parts: Vec<(Option<u16>, Option<Surface>)> = match &o.surfaces {
                Some(Surfaces::PerId(_)) => mesh_data_by_id(o.mesh).into_iter().map(|(id, _)| (Some(id), o.surfaces.as_ref().map(|s| s.for_face(id).into_owned()))).collect(),
                Some(Surfaces::Single(s)) => vec![(None, Some((**s).clone()))],
                None => vec![(None, None)],
            };
            for (e, _) in set.remove(&o.id).map(|obj| obj.parts).unwrap_or_default() {
                self.app.world_mut().despawn(e);
            }
            let mut entities = Vec::new();
            for (id, surface) in parts {
                let mat = self.material(surface.as_ref(), o.wire_color, o.flags.visibility);
                let faceted = surface.as_ref().is_some_and(|s| s.faceted);
                let d = if id.is_none() && !faceted {
                    data.clone()
                } else {
                    let mut part = o.mesh.clone();
                    for f in part.faces_mut() {
                        f.hidden |= id.is_some_and(|id| f.material_id != id);
                        // Faceted: flat normals whatever the smoothing groups.
                        if faceted {
                            f.smoothing_groups = 0;
                        }
                    }
                    mesh_data(&part)
                };
                let mut mesh = d.to_bevy();
                if mat.normal_map_texture.is_some() {
                    // A normal map needs tangents (from the UVs).
                    let _ = mesh.generate_tangents();
                }
                let world = self.app.world_mut();
                let mesh = world.resource_mut::<Assets<Mesh>>().add(mesh);
                let material_handle = world.resource_mut::<Assets<StandardMaterial>>().add(mat);
                let e = world.spawn((Transform::from_scale(Vec3::splat(self.scale)), Mesh3d(mesh), MeshMaterial3d(material_handle.clone()), layers.clone()));
                let mut e = Self::shadow_flags(e, Self::cast_flags(o));
                // Bevy computes bounds only once per entity: keep them current.
                if let Some(b) = d.aabb() {
                    e.insert(b);
                }
                let entity = e.id();
                if let Some(r) = self.render.as_mut() {
                    // A render in progress: new materials get its changes too.
                    let mut mats = self.app.world_mut().resource_mut::<Assets<StandardMaterial>>();
                    if let Some(m) = mats.get_mut(&material_handle) {
                        if r.options.force_two_sided {
                            r.two_sided.push((material_handle.clone(), m.double_sided, m.cull_mode));
                            m.double_sided = true;
                            m.cull_mode = None;
                        }
                        if r.options.quality.reflections {
                            r.deferred.push((material_handle.clone(), m.opaque_render_method));
                            m.opaque_render_method = bevy::pbr::OpaqueRendererMethod::Deferred;
                        }
                    }
                }
                entities.push((entity, material_handle));
            }
            set.insert(o.id, Object { parts: entities, mesh_hash: hash, surfaces: o.surfaces.clone(), wire_color: o.wire_color, renderable: o.renderable, flags: o.flags });
            self.settle = self.settle.max(3);
        }
        let gone: Vec<u64> = set.keys().filter(|id| !seen.contains(id)).copied().collect();
        for id in gone {
            if let Some(obj) = set.remove(&id) {
                for (e, _) in obj.parts {
                    self.app.world_mut().despawn(e);
                }
            }
            self.settle = self.settle.max(3);
        }
        (lo.x <= hi.x).then(|| ((lo + hi) * 0.5, ((hi - lo) * 0.5).length().max(1e-3)))
    }

    /// The flags whose shadows apply: shadow maps belong to the lights, not
    /// to a camera, so objects renders leave out (not renderable, visibility
    /// 0) cast no shadows at all.
    fn cast_flags(o: &SceneObject) -> ObjectFlags {
        let mut f = o.flags;
        f.cast_shadows &= o.renderable && f.visibility > 0.0;
        f
    }

    /// Cast Shadows / Receive Shadows off: Bevy's shadow opt-out markers.
    fn shadow_flags(mut e: bevy::ecs::world::EntityWorldMut<'_>, flags: ObjectFlags) -> bevy::ecs::world::EntityWorldMut<'_> {
        use bevy::pbr::{NotShadowCaster, NotShadowReceiver};
        let removed = (flags.cast_shadows && e.contains::<NotShadowCaster>()) || (flags.receive_shadows && e.contains::<NotShadowReceiver>());
        if flags.cast_shadows {
            e.remove::<NotShadowCaster>();
        } else {
            e.insert(NotShadowCaster);
        }
        if flags.receive_shadows {
            e.remove::<NotShadowReceiver>();
        } else {
            e.insert(NotShadowReceiver);
        }
        if removed {
            // Bevy re-extracts a mesh's shadow flags when a marker is added,
            // not when one is removed: mark the mesh changed so it is.
            if let Some(mut m) = e.get_mut::<Mesh3d>() {
                bevy::ecs::change_detection::DetectChangesMut::set_changed(&mut m);
            }
        }
        e
    }

    /// Reads the image files again (Bitmap > Reload): textures are rebuilt on the next sync.
    pub fn reload_bitmaps(&mut self) {
        self.images.clear();
        self.textures.clear();
        for obj in self.objects.values_mut() {
            obj.mesh_hash = 0;
        }
    }

    /// A surface as a PBR material with its maps: Diffuse (over the colour,
    /// with the Opacity map as alpha), Bump as a normal map, Specular Level
    /// and Self-Illumination maps. All maps use the Diffuse (else first) map's
    /// Coordinates, as Bevy has one UV transform per material.
    fn material(&mut self, surface: Option<&Surface>, wire_color: [u8; 3], visibility: f32) -> StandardMaterial {
        let mut m = self.surface_material(surface, wire_color);
        let v = visibility.clamp(0.0, 1.0);
        if v < 1.0 {
            // Visibility fades the whole object, highlights too.
            if m.alpha_mode == bevy::render::alpha::AlphaMode::Premultiplied {
                // Back to a straight colour: blending fades all of it.
                let c = m.base_color.to_linear();
                let a = c.alpha.max(1e-6);
                m.base_color = Color::linear_rgba(c.red / a, c.green / a, c.blue / a, c.alpha * v);
            } else {
                m.base_color = m.base_color.with_alpha(m.base_color.alpha() * v);
            }
            m.alpha_mode = bevy::render::alpha::AlphaMode::Blend;
        }
        m
    }

    fn surface_material(&mut self, surface: Option<&Surface>, wire_color: [u8; 3]) -> StandardMaterial {
        let mut m = material_from_surface(surface, wire_color);
        let Some(s) = surface else { return m };
        let diffuse = s.diffuse_map.as_ref().and_then(|map| self.baked(map).map(|img| (img, map.clone())));
        let opacity = s.opacity_map.as_ref().and_then(|map| self.baked(map).map(|img| (img, map.clone())));
        if diffuse.is_some() || opacity.is_some() {
            let key = format!("base:{:?}:{}:{}:{}", s.diffuse, s.opacity, diffuse.as_ref().map(|(_, m)| loom_scene::maps::texture_key(m) + &m.amount.to_string()).unwrap_or_default(), opacity.as_ref().map(|(_, m)| loom_scene::maps::texture_key(m) + &m.amount.to_string()).unwrap_or_default());
            let decal = diffuse.as_ref().map(|(_, m)| m.coords.decal()).unwrap_or([false; 2]);
            m.base_color_texture = self.texture_from(&key, TextureFormat::Rgba8UnormSrgb, decal, || convert::compose_base(s.diffuse, diffuse.as_ref().map(|(i, m)| (i.as_ref(), m)), s.opacity, opacity.as_ref().map(|(i, m)| (i.as_ref(), m))));
            if m.base_color_texture.is_some() {
                // The texture carries the colour (and the alpha); only the self-lit share is taken off.
                let k = 1.0 - s.self_illum;
                m.base_color = Color::srgba(k, k, k, 1.0);
                if opacity.is_some() {
                    m.alpha_mode = bevy::render::alpha::AlphaMode::Blend;
                } else if m.alpha_mode == bevy::render::alpha::AlphaMode::Premultiplied {
                    // The texture's alpha is the Opacity: premultiply its colour by it.
                    let k = Color::srgb(k, k, k).to_linear().red * s.opacity;
                    m.base_color = Color::linear_rgba(k, k, k, 1.0);
                }
            }
            let coords = diffuse.as_ref().or(opacity.as_ref()).map(|(_, m)| m);
            if let Some(c) = coords {
                m.uv_transform = convert::uv_transform(c);
            }
        }
        if let Some(map) = s.bump_map.as_ref() {
            if let Some(img) = self.baked(map) {
                let key = format!("bump:{}:{}:{}{}", loom_scene::maps::texture_key(map), map.amount, map.params.flip_red, map.params.flip_green);
                m.normal_map_texture = if map.kind == loom_scene::material::MapType::NormalBump {
                    self.texture_from(&key, TextureFormat::Rgba8Unorm, [false; 2], || convert::normal_bump(&img, map.params.flip_red, map.params.flip_green))
                } else {
                    self.texture_from(&key, TextureFormat::Rgba8Unorm, [false; 2], || loom_scene::maps::bump_to_normal(&img, map.amount / 100.0))
                };
            }
        }
        if let Some(map) = s.specular_level_map.as_ref() {
            if let Some(img) = self.baked(map) {
                let key = format!("spec:{}:{}", loom_scene::maps::texture_key(map), map.amount);
                m.specular_texture = self.texture_from(&key, TextureFormat::Rgba8Unorm, [false; 2], || convert::luminance(&img, (map.amount / 100.0).clamp(0.0, 1.0)));
                // The texture's alpha 0–1 maps to reflectance 0–0.5 times
                // this: white in the map gives the Specular Level's own.
                m.reflectance *= 2.0;
            }
        }
        if let Some(map) = s.self_illum_map.as_ref() {
            if let Some(img) = self.baked(map) {
                let key = format!("illum:{}:{}", loom_scene::maps::texture_key(map), map.amount);
                m.emissive_texture = self.texture_from(&key, TextureFormat::Rgba8Unorm, [false; 2], || convert::luminance(&img, (map.amount / 100.0).clamp(0.0, 1.0)));
                let d = Color::srgb(s.diffuse.x, s.diffuse.y, s.diffuse.z).to_linear();
                m.emissive = bevy::color::LinearRgba::rgb(d.red, d.green, d.blue);
            }
        }
        m
    }

    /// A map baked to pixels (one map-space tile), its bitmaps read once per path.
    fn baked(&mut self, map: &Map) -> Option<std::sync::Arc<loom_scene::maps::Image>> {
        loom_scene::maps::load_images(map, &mut self.images, &mut |path| {
            image::open(path).ok().map(|img| {
                let rgba = img.to_rgba8();
                std::sync::Arc::new(loom_scene::maps::Image { width: rgba.width(), height: rgba.height(), rgba: rgba.into_raw() })
            })
        });
        let plain = map.kind == loom_scene::material::MapType::Bitmap && !map.crop.apply && !map.coords.mirror[0] && !map.coords.mirror[1];
        if plain {
            return self.images.get(&map.path).cloned().flatten();
        }
        loom_scene::maps::bake(map, &self.images).map(std::sync::Arc::new)
    }

    /// An image as a Bevy texture, made once per key; decal axes clamp.
    fn texture_from(&mut self, key: &str, format: TextureFormat, decal: [bool; 2], make: impl FnOnce() -> loom_scene::maps::Image) -> Option<Handle<Image>> {
        if let Some(t) = self.textures.get(key) {
            return t.clone();
        }
        let img = make();
        let mut image = Image::new(Extent3d { width: img.width.max(1), height: img.height.max(1), depth_or_array_layers: 1 }, TextureDimension::D2, img.rgba.clone(), format, RenderAssetUsages::RENDER_WORLD);
        let mode = |d: bool| if d { ImageAddressMode::ClampToEdge } else { ImageAddressMode::Repeat };
        image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor { address_mode_u: mode(decal[0]), address_mode_v: mode(decal[1]), ..ImageSamplerDescriptor::linear() });
        let handle = Some(self.app.world_mut().resource_mut::<Assets<Image>>().add(image));
        self.textures.insert(key.to_owned(), handle.clone());
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
        let camera = camera.scaled(self.scale);
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
                    // The render camera works in HDR: its exposure control tone maps.
                    let entity = self
                        .app
                        .world_mut()
                        .spawn((Camera3d::default(), Tonemapping::AcesFitted, Camera { order: slot.min(1 << 20) as isize, hdr: slot == RENDER_SLOT, ..Default::default() }))
                        .id();
                    self.views.insert(slot, View { camera: entity, image: image.clone(), size, msaa, used: false, generation: 0 });
                }
            }
            self.settle = self.settle.max(3);
        }
        // A render drawing deferred (for its reflections) holds the
        // viewports: they draw forward and would miss the deferred objects.
        let held = slot != RENDER_SLOT && self.render.as_ref().is_some_and(|r| r.options.quality.reflections);
        // The screen-space passes need the camera without MSAA.
        let msaa = msaa && !(slot == RENDER_SLOT && self.render.as_ref().is_some_and(|r| r.options.screen_space()));
        let v = self.views.get_mut(&slot).expect("inserted above");
        v.used = !held;
        let (entity, image) = (v.camera, v.image.clone());
        let mut e = self.app.world_mut().entity_mut(entity);
        // Camera is changed in place: inserting it anew would drop the
        // render target size Bevy computed, and the view would stop rendering.
        if let Some(mut cam) = e.get_mut::<Camera>() {
            if !matches!(&cam.target, RenderTarget::Image(t) if t.handle == image) {
                cam.target = RenderTarget::Image(image.into());
            }
            cam.clear_color = ClearColorConfig::Custom(clear);
            cam.is_active = !held;
        }
        // Every view sees the scene lights' layer.
        e.insert((projection, Transform::from_matrix(view.inverse()), if msaa { Msaa::Sample4 } else { Msaa::Off }, layers.with(LIGHT_LAYER)));
    }

    /// Gives the render camera the render's quality settings: exposure
    /// control, ambient occlusion, reflections and the sky dome.
    fn setup_render_camera(&mut self) {
        use bevy::core_pipeline::tonemapping::DebandDither;
        use bevy::pbr::{EnvironmentMapLight, ScreenSpaceAmbientOcclusion, ScreenSpaceAmbientOcclusionQualityLevel, ScreenSpaceReflections, ShadowFilteringMethod};
        use bevy::render::camera::Exposure;
        use loom_scene::render::{AoQuality, Tonemapper};
        let Some(r) = self.render.as_ref() else { return };
        let Some(v) = self.views.get(&RENDER_SLOT) else { return };
        let o = r.options.clone();
        let q = &o.quality;
        let entity = v.camera;
        let tonemapping = match q.tonemapper {
            Tonemapper::None => Tonemapping::None,
            Tonemapper::Reinhard => Tonemapping::Reinhard,
            Tonemapper::Aces => Tonemapping::AcesFitted,
            Tonemapper::AgX => Tonemapping::AgX,
            Tonemapper::TonyMcMapface => Tonemapping::TonyMcMapface,
            Tonemapper::BlenderFilmic => Tonemapping::BlenderFilmic,
        };
        // The sky dome: the ambient light's colour and brightness, from a dome.
        let sky = q.sky_lighting.then(|| {
            let (color, brightness) = self.ambient_light(o.ambient);
            let c = color.to_linear();
            let (d, s) = quality::sky_maps([c.red, c.green, c.blue], q.ground);
            let mut images = self.app.world_mut().resource_mut::<Assets<Image>>();
            (images.add(d), images.add(s), brightness * q.sky_intensity * quality::SKY_MAP_SCALE)
        });
        let world = self.app.world_mut();
        let mut e = world.entity_mut(entity);
        e.insert((
            tonemapping,
            if q.dither { DebandDither::Enabled } else { DebandDither::Disabled },
            Exposure { ev100: Exposure::default().ev100 - q.exposure },
            // A filter that changes with every frame adds up over the passes.
            if o.passes() > 1 { ShadowFilteringMethod::Temporal } else { ShadowFilteringMethod::Gaussian },
        ));
        if q.ambient_occlusion {
            let quality_level = match q.ao_quality {
                AoQuality::Low => ScreenSpaceAmbientOcclusionQualityLevel::Low,
                AoQuality::Medium => ScreenSpaceAmbientOcclusionQualityLevel::Medium,
                AoQuality::High => ScreenSpaceAmbientOcclusionQualityLevel::High,
                AoQuality::Ultra => ScreenSpaceAmbientOcclusionQualityLevel::Ultra,
            };
            e.insert(ScreenSpaceAmbientOcclusion { quality_level, ..Default::default() });
        }
        if q.reflections {
            // Glossier than this reflects the view (rougher: only the sky dome).
            e.insert(ScreenSpaceReflections { perceptual_roughness_threshold: 0.35, ..Default::default() });
        }
        if let Some((diffuse_map, specular_map, intensity)) = sky {
            // The dome replaces the flat ambient light for this camera.
            e.insert((EnvironmentMapLight { diffuse_map, specular_map, intensity, ..Default::default() }, AmbientLight { color: Color::BLACK, brightness: 0.0, affects_lightmapped_meshes: true }));
        }
    }

    /// Starts a render with `options`: shadows, shadow map size, ambient
    /// light, Force 2-Sided and the Selected layer apply until `end_render`
    /// (the Realistic viewports see them meanwhile too).
    pub fn begin_render(&mut self, options: RenderOptions) {
        self.end_render();
        let handles: Vec<Handle<StandardMaterial>> = self.objects.values().chain(self.hidden.values()).flat_map(|o| o.parts.iter().map(|(_, h)| h.clone())).collect();
        let world = self.app.world_mut();
        world.insert_resource(DirectionalLightShadowMap { size: options.shadow_map_size.clamp(512, 8192) as usize });
        let (mut two_sided, mut deferred) = (Vec::new(), Vec::new());
        let mut mats = world.resource_mut::<Assets<StandardMaterial>>();
        for h in handles {
            let Some(m) = mats.get_mut(&h) else { continue };
            if options.force_two_sided {
                two_sided.push((h.clone(), m.double_sided, m.cull_mode));
                m.double_sided = true;
                m.cull_mode = None;
            }
            if options.quality.reflections {
                // The reflections read the deferred G-buffer.
                deferred.push((h, m.opaque_render_method));
                m.opaque_render_method = bevy::pbr::OpaqueRendererMethod::Deferred;
            }
        }
        self.scale = options.world_scale();
        self.light_pass = 0;
        self.render = Some(ActiveRender { options, two_sided, deferred, camera_ready: false, accum: None });
        self.rescale();
        self.apply_lights();
        self.relayer();
        self.settle = self.settle.max(3);
    }

    /// Puts every object at the current world scale.
    fn rescale(&mut self) {
        let t = Transform::from_scale(Vec3::splat(self.scale));
        let parts: Vec<Entity> = self.objects.values().chain(self.hidden.values()).flat_map(|o| o.parts.iter().map(|(e, _)| *e)).collect();
        for e in parts {
            self.app.world_mut().entity_mut(e).insert(t);
        }
    }

    /// Puts every object on its layers again (the Selected layer or a light's exclusions changed).
    fn relayer(&mut self) {
        let shown = self.objects.iter().flat_map(|(id, o)| o.parts.iter().map(|(e, _)| (*e, self.object_layers(*id, o.renderable, o.flags, false))));
        let hidden = self.hidden.iter().flat_map(|(id, o)| o.parts.iter().map(|(e, _)| (*e, self.object_layers(*id, o.renderable, o.flags, true))));
        let list: Vec<(Entity, RenderLayers)> = shown.chain(hidden).collect();
        for (e, l) in list {
            self.app.world_mut().entity_mut(e).insert(l);
        }
    }

    /// Renders the next pass of bucket `sub` = [u0, v0, u1, v1] (fractions
    /// of the frame, v down) of `camera` at image aspect `aspect`, `size`
    /// output pixels, on the next `update`; add it with `read_render`. The
    /// pass covers a margin around the bucket (the filter's reach, the
    /// screen-space effects' view), at the supersampling's size, its samples
    /// and lights moved for the pass. Needs `begin_render`.
    pub fn set_render_bucket(&mut self, camera: &ViewCamera, aspect: f32, sub: [f32; 4], size: [u32; 2]) -> Result<(), String> {
        let Some(r) = self.render.as_mut() else { return Err("no render started".into()) };
        let o = r.options.clone();
        let q = &o.quality;
        let size = [size[0].max(1), size[1].max(1)];
        let radius = if q.antialiasing { q.filter_size.clamp(1.0, 10.0) * 0.5 } else { 0.5 };
        let margin = radius.ceil() as u32 + 1 + if o.screen_space() { 16 } else { 0 };
        if r.accum.as_ref().is_none_or(|a| a.key != (sub, size)) {
            r.accum = Some(quality::Accum::new((sub, size), margin, q.supersampling.clamp(1, 4)));
        }
        let a = r.accum.as_ref().expect("set above");
        let (pass, ss, internal) = (a.pass, a.ss as f32, a.internal_size());
        let j = if q.antialiasing { quality::jitter(pass, o.passes()) } else { [0.0; 2] };
        // Frame fractions per output pixel.
        let (fx, fy) = ((sub[2] - sub[0]) / size[0] as f32, (sub[3] - sub[1]) / size[1] as f32);
        let (m, dx, dy) = (margin as f32, j[0] / ss * fx, j[1] / ss * fy);
        let sub = [sub[0] - m * fx + dx, sub[1] - m * fy + dy, sub[2] + m * fx + dx, sub[3] + m * fy + dy];
        if o.quality.area_shadows && pass != self.light_pass {
            self.light_pass = pass;
            self.apply_lights();
        }
        let layers = match &o.selected_only {
            Some(_) => RenderLayers::layer(SELECTED_LAYER),
            None if o.hidden => RenderLayers::from_layers(&[RENDER_LAYER, HIDDEN_LAYER]),
            None => RenderLayers::layer(RENDER_LAYER),
        };
        // Cleared to nothing: the background goes under the passes once they
        // are added up, so tone mapping never touches its colour.
        let clear = [0.0; 4];
        let camera = camera.scaled(self.scale);
        self.place_view(RENDER_SLOT, internal, Projection::custom(BucketProjection::new(&camera, aspect, sub)), camera.view, clear, o.msaa, layers);
        if self.render.as_ref().is_some_and(|r| !r.camera_ready) {
            self.setup_render_camera();
            if let Some(r) = self.render.as_mut() {
                r.camera_ready = true;
            }
        }
        Ok(())
    }

    /// Adds the pass rendered last to its bucket and returns the bucket so
    /// far (sRGB RGBA, background alpha 0); `bucket_done` once all passes are in.
    pub fn read_render(&mut self) -> Result<image::RgbaImage, String> {
        let img = self.read_back(RENDER_SLOT)?;
        let r = self.render.as_mut().ok_or("no render started")?;
        let (q, passes, bg) = (r.options.quality.clone(), r.options.passes(), r.options.background);
        let a = r.accum.as_mut().ok_or("no bucket set")?;
        let (j, filter, size) = if q.antialiasing { (quality::jitter(a.pass, passes), q.filter, q.filter_size.clamp(1.0, 10.0)) } else { ([0.0; 2], loom_scene::render::AaFilter::Area, 1.0) };
        a.add(&img, j, filter, size)?;
        Ok(a.image(bg))
    }

    /// Whether the bucket has all its passes.
    pub fn bucket_done(&self) -> bool {
        self.render.as_ref().is_some_and(|r| r.accum.as_ref().is_some_and(|a| a.pass >= r.options.passes()))
    }

    /// Ends the render: its camera goes, the scene's own settings return.
    pub fn end_render(&mut self) {
        if let Some(v) = self.views.remove(&RENDER_SLOT) {
            self.app.world_mut().despawn(v.camera);
        }
        let Some(r) = self.render.take() else { return };
        self.scale = 1.0;
        self.light_pass = 0;
        self.rescale();
        self.apply_lights();
        let world = self.app.world_mut();
        world.insert_resource(DirectionalLightShadowMap { size: 2048 });
        let mut mats = world.resource_mut::<Assets<StandardMaterial>>();
        for (h, double_sided, cull) in r.two_sided {
            if let Some(m) = mats.get_mut(&h) {
                m.double_sided = double_sided;
                m.cull_mode = cull;
            }
        }
        for (h, method) in r.deferred {
            if let Some(m) = mats.get_mut(&h) {
                m.opaque_render_method = method;
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
        let (centre, radius) = (centre * self.scale, radius * self.scale);
        let mut range: Option<(f32, f32)> = None;
        for v in self.views.values().filter(|v| v.used) {
            let Some(t) = self.app.world().get::<Transform>(v.camera) else { continue };
            // Distance along the view direction to the scene centre.
            let d = (centre - t.translation).dot(t.forward().as_vec3());
            let (a, b) = ((d - radius).max(0.0), (d + radius).max(1e-3));
            range = Some(range.map_or((a, b), |(lo, hi)| (lo.min(a), hi.max(b))));
        }
        let Some((min, max)) = range else { return };
        let max = max.max(min + 1e-3);
        // A render splits the range into cascades: sharper shadows near the camera.
        let config = if self.render.is_some() {
            CascadeShadowConfigBuilder { num_cascades: 4, minimum_distance: min, maximum_distance: max, first_cascade_far_bound: min + (max - min) * 0.1, overlap_proportion: 0.2 }.build()
        } else {
            CascadeShadowConfigBuilder { num_cascades: 1, minimum_distance: min, maximum_distance: max, first_cascade_far_bound: max, overlap_proportion: 0.2 }.build()
        };
        let directional = self.scene_lights.values().filter(|l| l.kind == lights::Kind::Directional).map(|l| l.entity);
        let entities: Vec<Entity> = self.light.into_iter().chain(directional).collect();
        for e in entities {
            self.app.world_mut().entity_mut(e).insert(config.clone());
        }
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
    /// render options and reads the pixels back (sRGB RGBA, black
    /// background with alpha 0).
    pub fn render_image(&mut self, width: u32, height: u32, camera: &ViewCamera) -> Result<image::RgbaImage, String> {
        self.render_with(width, height, camera, RenderOptions::default())
    }

    /// Renders `camera` at `width` x `height` with `options`, every pass.
    pub fn render_with(&mut self, width: u32, height: u32, camera: &ViewCamera, options: RenderOptions) -> Result<image::RgbaImage, String> {
        if width == 0 || height == 0 {
            return Err(format!("render size {width}x{height}"));
        }
        self.begin_render(options);
        let aspect = width as f32 / height as f32;
        let whole = [0.0, 0.0, 1.0, 1.0];
        // A few frames: new pipelines, the target image and the shadow map settle first.
        for _ in 0..3 {
            self.set_render_bucket(camera, aspect, whole, [width, height])?;
            self.update();
        }
        let result = loop {
            self.set_render_bucket(camera, aspect, whole, [width, height])?;
            self.update();
            match self.read_render() {
                Ok(img) if self.bucket_done() => break Ok(img),
                Ok(_) => {}
                Err(e) => break Err(e),
            }
        };
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
