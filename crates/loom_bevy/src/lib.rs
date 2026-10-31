//! Realistic shading with Bevy: physically based materials, a key light with
//! shadows and ambient light. Bevy runs headless on the UI's own wgpu device
//! and renders each Realistic viewport into an image the UI draws as the
//! viewport background, and renders the active view to a PNG for Render.
//! The scene's Environment (background, global lighting, exposure control,
//! atmospheres, Depth of Field) applies to both; see [`environment`].

pub mod bake;
pub mod convert;
mod prepared_geometry;
mod gpu_pose_commit;
pub mod effects;
pub mod environment;
pub mod lights;
pub mod look;
pub mod quality;
mod post;
mod mikktspace;
#[cfg(test)]
mod gpu_skin;
pub mod dynamic_geometry;
mod gpu_pose_geometry;
mod gpu_pose_runtime;
mod gpu_mikk_precision;
mod gpu_pose_renderer;
pub use gpu_pose_renderer::{GpuPoseSource, GpuPoseStatus};

use std::collections::{HashMap, HashSet};
use dynamic_geometry::{DynamicGeometryBuffer, DynamicGeometryPlugin, DynamicMaterial};

use bevy::app::{App, PluginsState, TerminalCtrlCHandlerPlugin};
use bevy::asset::{Assets, Handle, RenderAssetUsages};
use bevy::core_pipeline::core_3d::Camera3d;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::ecs::entity::Entity;
use bevy::image::{Image, ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Vec3;
use bevy::pbr::{AmbientLight, CascadeShadowConfigBuilder, DirectionalLight, DirectionalLightShadowMap, FogVolume, MeshMaterial3d, PointLightShadowMap, StandardMaterial, VolumetricFog, VolumetricLight};
use bevy::render::view::{ColorGrading, ColorGradingGlobal, ColorGradingSection};
use environment::{EnvSettings, Fog, ViewEnv};
use loom_scene::environment::{EnvMapping, ExposureKind, LayerFalloff};
use bevy::render::camera::{Camera, ClearColorConfig, RenderTarget};
use bevy::render::mesh::{Mesh, Mesh3d};
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::render::renderer::{RenderAdapter, RenderAdapterInfo, RenderDevice, RenderInstance, RenderQueue, WgpuWrapper};
use bevy::render::settings::{RenderCreation, RenderResources};
use bevy::render::texture::GpuImage;
use bevy::render::view::Msaa;
use bevy::render::{RenderApp, RenderPlugin};
use bevy::transform::components::{GlobalTransform, Transform};
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
/// Brightness of the ambient light at the Environment's Ambient white.
const AMBIENT_BRIGHTNESS: f32 = 250.0;

/// Render Setup values a render uses (the Bevy renderer panel and the
/// Common options that reach the renderer).
#[derive(Debug, Clone, PartialEq)]
pub struct RenderOptions {
    pub msaa: bool,
    pub shadows: bool,
    pub shadow_map_size: u32,
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
    /// Options > Atmospherics / Effects: the environment's atmospheres and
    /// Depth of Field apply to the render.
    pub atmospherics: bool,
    pub effects: bool,
    /// The rendered view's environment ranges and focus.
    pub view: ViewEnv,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            msaa: true,
            shadows: true,
            shadow_map_size: 4096,
            background: [0.0; 3],
            ambient: [0.0; 3],
            force_two_sided: false,
            selected_only: None,
            hidden: false,
            quality: loom_scene::render::BevyRenderSettings {
                passes: 1,
                area_shadows: false,
                ambient_occlusion: false,
                reflections: false,
                sky_lighting: false,
                ..Default::default()
            },
            atmospherics: true,
            effects: true,
            view: ViewEnv::default(),
        }
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

/// One visible object: its mesh, affine transform and how it is coloured.
pub struct SceneObject<'a> {
    pub id: u64,
    pub mesh: &'a loom_mesh::Mesh,
    /// Local-to-world matrix; identity when mesh positions are already in world space.
    pub transform: glam::Mat4,
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
    /// The object's own Z axis in world space (a Top/Bottom material's Local coordinates).
    pub local_up: [f32; 3],
}

impl Default for ObjectFlags {
    fn default() -> Self {
        Self { cast_shadows: true, receive_shadows: true, visible_to_camera: true, visibility: 1.0, local_up: [0.0, 0.0, 1.0] }
    }
}

struct Object {
    /// One entity (and material) per face material ID drawn; one for a single material.
    parts: Vec<(Entity, ObjectMaterial)>,
    dynamic: bool,
    /// The current storage contents came from GPU compute, not mesh_hash.
    gpu_pose_current: bool,
    material_dirty: bool,
    buffers: Vec<DynamicGeometryBuffer>,
    part_ids: Vec<Option<u16>>,
    tangent_caches: Vec<mikktspace::NeighborCache>,
    mesh_hash: u64,
    transform: glam::Mat4,
    surfaces: Option<Surfaces>,
    wire_color: [u8; 3],
    renderable: bool,
    flags: ObjectFlags,
}

#[derive(Clone, Debug)]
enum ObjectMaterial {
    Standard(Handle<StandardMaterial>),
    Dynamic(Handle<DynamicMaterial>),
    Look(Handle<look::LoomMaterial>),
}

impl ObjectMaterial {
    fn edit(&self, world: &mut bevy::ecs::world::World, edit: impl FnOnce(&mut StandardMaterial)) {
        match self {
            Self::Standard(h) => {
                if let Some(m) = world.resource_mut::<Assets<StandardMaterial>>().get_mut(h) { edit(m); }
            }
            Self::Dynamic(h) => {
                if let Some(m) = world.resource_mut::<Assets<DynamicMaterial>>().get_mut(h) { edit(&mut m.base); }
            }
            Self::Look(h) => {
                if let Some(m) = world.resource_mut::<Assets<look::LoomMaterial>>().get_mut(h) { edit(&mut m.base); }
            }
        }
    }

    #[cfg(test)]
    fn id(&self) -> bevy::asset::UntypedAssetId {
        match self { Self::Standard(h) => h.id().untyped(), Self::Dynamic(h) => h.id().untyped(), Self::Look(h) => h.id().untyped() }
    }
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
/// Slot of the camera that meters a view for Automatic / Linear exposure.
const METER_SLOT: u32 = u32::MAX - 2;
/// Metering image size, and the stops it renders under so highlights do not clip.
const METER_SIZE: u32 = 64;
const METER_STOPS: f32 = 4.0;
/// Key light: from the front right and above (world Z up), so the default
/// Perspective view (from the front left) sees the shadows fall beside objects.
const KEY_LIGHT_FROM: Vec3 = Vec3::new(0.8, -0.45, 1.0);
/// The default key light's illuminance (lux) at Global Lighting Level 1.
const KEY_LIGHT_LUX: f32 = 3000.0;
/// The reference's default lighting is two lights: the key above the scene
/// and a fill from below on the other side, so with the default black
/// ambient nothing is left unlit. The fill is as much weaker as loom's
/// two-light viewport default (0.35 against 0.75) and casts no shadows.
const FILL_LIGHT_FROM: Vec3 = Vec3::new(-0.8, 0.45, -1.0);
const FILL_LIGHT_LUX: f32 = KEY_LIGHT_LUX * 0.35 / 0.75;

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

/// For test binaries: ends the process with a failure (exit 101), naming
/// its threads, once it has run `limit` (`LOOM_TEST_WATCHDOG_SECS`
/// overrides it), so a test stuck in the GPU driver fails instead of
/// holding its build slot. Armed once per process; later calls do nothing.
#[doc(hidden)]
pub fn test_watchdog(limit: std::time::Duration) {
    static ARMED: std::sync::Once = std::sync::Once::new();
    ARMED.call_once(|| {
        let limit = std::env::var("LOOM_TEST_WATCHDOG_SECS").ok().and_then(|s| s.parse().ok()).map(std::time::Duration::from_secs).unwrap_or(limit);
        let _ = std::thread::Builder::new().name("test watchdog".into()).spawn(move || {
            std::thread::sleep(limit);
            let threads = std::fs::read_dir("/proc/self/task")
                .map(|d| d.filter_map(|e| e.ok()).filter_map(|e| std::fs::read_to_string(e.path().join("comm")).ok()).map(|s| s.trim().to_owned()).collect::<Vec<_>>().join(", "))
                .unwrap_or_default();
            eprintln!("test watchdog: the test binary still runs after {limit:?}; threads: {threads}");
            std::process::exit(101);
        });
    });
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
    geometry: HashMap<(bool,u64),prepared_geometry::GeometryCache>,
    gpu_poses: Option<gpu_pose_renderer::GpuPoseState>,
    views: HashMap<u32, View>,
    textures: HashMap<String, Option<Handle<Image>>>,
    /// Image files as maps read them, by path.
    images: loom_scene::maps::Images,
    /// The default key light, while the scene has no light that is on.
    light: Option<Entity>,
    /// The default fill light, with the key light.
    fill: Option<Entity>,
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
    /// The scene's environment, the views' own parts of it and the map.
    env: EnvSettings,
    view_envs: HashMap<u32, ViewEnv>,
    env_map: Option<Handle<Image>>,
    /// The fog volumes Volume Light scatters in.
    volumes: Vec<Entity>,
    /// Automatic / Linear exposure: stops each view's metering found.
    metered: HashMap<u32, f32>,
    /// The view the metering camera looks through this frame.
    meter: Option<u32>,
    /// The next viewport to meter (they take turns).
    meter_next: u32,
}

struct ActiveRender {
    options: RenderOptions,
    /// Materials made 2-sided by Force 2-Sided, with their own setting.
    two_sided: Vec<(ObjectMaterial, bool, Option<bevy::render::render_resource::Face>)>,
    /// Materials drawn deferred for the reflections, with their own method.
    deferred: Vec<(ObjectMaterial, bevy::pbr::OpaqueRendererMethod)>,
    /// The render camera has its quality components.
    camera_ready: bool,
    /// The bucket's passes so far.
    accum: Option<quality::Accum>,
    /// The part of the frame the current bucket shows (u0, v0, u1, v1).
    frame: [f32; 4],
}

// Bevy 0.16.1's light specialization system appends every removed
// NotShadowCaster entity, including despawned entities and entities carrying a
// different material type. Extraction removes their old ticks first, then
// reinserts those invalid requests, retaining every dead generation forever.
// Enforce the request's live-mesh/material precondition before extraction.
fn retain_live_material_specializations<M: bevy::pbr::Material>(
    meshes: bevy::ecs::system::Query<(), (bevy::ecs::query::With<Mesh3d>, bevy::ecs::query::With<MeshMaterial3d<M>>)>,
    mut requests: bevy::ecs::system::ResMut<bevy::pbr::EntitiesNeedingSpecialization<M>>,
) {
    requests.entities.retain(|entity| meshes.contains(*entity));
}

fn material_lifecycle<M: bevy::pbr::Material>(app: &mut App) {
    use bevy::ecs::schedule::IntoScheduleConfigs;
    app.add_systems(bevy::app::PostUpdate,
        retain_live_material_specializations::<M>
            .after(bevy::pbr::check_entities_needing_specialization::<M>)
            .after(bevy::pbr::check_light_entities_needing_specialization::<M>));
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
        app.add_plugins((post::EnvPassPlugin, bake::BakePlugin, look::LookPlugin, DynamicGeometryPlugin));
        material_lifecycle::<StandardMaterial>(&mut app);
        material_lifecycle::<DynamicMaterial>(&mut app);
        material_lifecycle::<look::LoomMaterial>(&mut app);
        material_lifecycle::<bevy::pbr::ExtendedMaterial<StandardMaterial, bake::UvBake>>(&mut app);
        while app.plugins_state() == PluginsState::Adding {
            bevy::tasks::tick_global_task_pools_on_main_thread();
        }
        app.finish();
        app.cleanup();
        // The default Environment's ambient is black.
        app.insert_resource(AmbientLight { color: Color::WHITE, brightness: 0.0, affects_lightmapped_meshes: true });
        app.insert_resource(DirectionalLightShadowMap { size: 2048 });
        let light = Some(Self::spawn_key_light(&mut app));
        let fill = Some(Self::spawn_fill_light(&mut app));
        Self {
            gpu_poses: gpu_pose_renderer::GpuPoseState::selected(&device, std::env::var_os("LOOM_EXPERIMENTAL_GPU_POSES").as_deref()),
            app,
            device,
            queue,
            objects: HashMap::new(),
            geometry: HashMap::new(),
            views: HashMap::new(),
            textures: HashMap::new(),
            images: Default::default(),
            light,
            fill,
            samples: Vec::new(),
            scene_lights: HashMap::new(),
            bounds: None,
            settle: 3,
            hidden: HashMap::new(),
            render: None,
            scale: 1.0,
            light_pass: 0,
            env: EnvSettings::default(),
            view_envs: HashMap::new(),
            env_map: None,
            volumes: Vec::new(),
            metered: HashMap::new(),
            meter: None,
            meter_next: 0,
        }
    }

    /// Default lighting: the key light, with shadows.
    fn spawn_key_light(app: &mut App) -> Entity {
        app.world_mut()
            .spawn((
                DirectionalLight { illuminance: KEY_LIGHT_LUX, shadows_enabled: true, ..Default::default() },
                Transform::from_translation(KEY_LIGHT_FROM).looking_at(Vec3::ZERO, Vec3::Z),
                // The key light lights (and shadows) every layer.
                RenderLayers::from_layers(&[VIEWPORT_LAYER, RENDER_LAYER, SELECTED_LAYER, HIDDEN_LAYER, SHADOW_ONLY_LAYER, bake::BAKE_KEY_LAYER]),
            ))
            .id()
    }

    /// Default lighting: the fill light, from below on the other side.
    fn spawn_fill_light(app: &mut App) -> Entity {
        app.world_mut()
            .spawn((
                DirectionalLight { illuminance: FILL_LIGHT_LUX, shadows_enabled: false, ..Default::default() },
                Transform::from_translation(FILL_LIGHT_FROM).looking_at(Vec3::ZERO, Vec3::Z),
                RenderLayers::from_layers(&[VIEWPORT_LAYER, RENDER_LAYER, SELECTED_LAYER, HIDDEN_LAYER, SHADOW_ONLY_LAYER, bake::BAKE_KEY_LAYER]),
            ))
            .id()
    }

    /// Brings the Bevy lights in line with the scene's lights (call after
    /// `sync`, which measures the scene). With none, the default key and fill
    /// lights light the scene; otherwise the scene lights replace it. A render's
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
        // Global Lighting: Ambient is the ambient light's colour; Tint and
        // Level multiply every other light.
        let tint = if self.env.ambient != [0.0; 3] { self.env.ambient } else { options.as_ref().map_or(self.env.ambient, |o| o.ambient) };
        let light_tint = Color::srgb(self.env.tint[0], self.env.tint[1], self.env.tint[2]).to_linear();
        let light_tint = glam::Vec3::new(light_tint.red, light_tint.green, light_tint.blue) * self.env.level.max(0.0);
        if self.samples.is_empty() {
            if self.light.is_none() {
                self.light = Some(Self::spawn_key_light(&mut self.app));
            }
            if self.fill.is_none() {
                self.fill = Some(Self::spawn_fill_light(&mut self.app));
            }
        } else {
            for e in [self.light.take(), self.fill.take()].into_iter().flatten() {
                self.app.world_mut().despawn(e);
            }
        }
        let b = light_tint.max_element();
        let color = if b > 0.0 { Color::linear_rgb(light_tint.x / b, light_tint.y / b, light_tint.z / b) } else { Color::BLACK };
        for (e, lux, key) in [(self.light, KEY_LIGHT_LUX, true), (self.fill, FILL_LIGHT_LUX, false)] {
            let Some(e) = e else { continue };
            if let Some(mut l) = self.app.world_mut().get_mut::<DirectionalLight>(e) {
                l.shadows_enabled = key && shadows;
                l.illuminance = lux * b;
                l.color = color;
            }
        }
        // The Environment's ambient colour (sRGB), plus the skylights and Ambient Only lights.
        let t = Color::srgb(tint[0], tint[1], tint[2]).to_linear();
        let (sky_color, sky) = lights::ambient(&self.samples);
        let sky_color = sky_color.to_linear();
        // Skylights are lights: Tint and Level apply to them too.
        let sum = Vec3::new(t.red, t.green, t.blue) * AMBIENT_BRIGHTNESS + Vec3::new(sky_color.red, sky_color.green, sky_color.blue) * Vec3::from(light_tint.to_array()) * sky;
        let b = sum.max_element();
        let color = if b > 0.0 { Color::linear_rgb(sum.x / b, sum.y / b, sum.z / b) } else { Color::BLACK };
        self.app.insert_resource(AmbientLight { color, brightness: b.max(0.0), affects_lightmapped_meshes: true });
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
            let (entity, old_fill) = match self.scene_lights.get(&s.id.0) {
                Some(l) if l.kind == kind => (l.entity, l.fill),
                other => {
                    if let Some(l) = other {
                        world.despawn(l.entity);
                        if let Some(f) = l.fill {
                            world.despawn(f);
                        }
                    }
                    let e = world.spawn(Transform::IDENTITY).id();
                    self.scene_lights.insert(s.id.0, lights::Synced { entity: e, kind, fill: None });
                    (e, None)
                }
            };
            let mut s = s.clone();
            s.shadows &= shadows;
            s.color *= light_tint;
            if let (Some(d), true) = (area, s.source_radius > 0.0) {
                lights::move_over_source(&mut s, d, centre);
                s.source_radius = 0.0;
            }
            // Shadow colour / density: the part shadows let through is a second, unshadowed light.
            let (main, fill) = match lights::shadow_split(&s) {
                Some((stopped, through)) => (stopped, Some(through)),
                None => (s.clone(), None),
            };
            let layers = RenderLayers::from_layers(&[LIGHT_LAYER, LIGHT_LAYER + 1 + i]);
            let fill_entity = match (fill.is_some(), old_fill) {
                (true, Some(f)) => Some(f),
                (true, None) => Some(world.spawn(Transform::IDENTITY).id()),
                (false, Some(f)) => {
                    world.despawn(f);
                    None
                }
                (false, None) => None,
            };
            if let Some(l) = self.scene_lights.get_mut(&s.id.0) {
                l.fill = fill_entity;
            }
            for (entity, part) in std::iter::once((entity, &main)).chain(fill_entity.zip(fill.as_ref())) {
                let mut transform = lights::transform(part);
                transform.translation *= scale;
                let mut e = world.entity_mut(entity);
                e.insert((transform, layers.clone()));
                match kind {
                    lights::Kind::Point => {
                        e.insert(lights::scaled_point(lights::point(part, centre_distance, radius), scale));
                    }
                    lights::Kind::Spot => {
                        e.insert(lights::scaled_spot(lights::spot(part, centre_distance, radius), scale));
                    }
                    lights::Kind::Directional => {
                        e.insert(lights::directional(part));
                    }
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
                if let Some(f) = l.fill {
                    self.app.world_mut().despawn(f);
                }
            }
        }
        if let Some(o) = options.as_ref() {
            // Renders give no light a smaller map than Render Setup's.
            map_size = map_size.max(o.shadow_map_size as usize);
        }
        if map_size > 0 {
            self.app.insert_resource(PointLightShadowMap { size: map_size.clamp(128, 8192) });
        }
        self.apply_volume_lights();
    }

    /// Volume Light of directional lights: Bevy's volumetric fog in a fog
    /// volume around the scene (it follows their shadow maps). Omni and spot
    /// lights are marched by the environment pass ([`volume_light_uniform`]):
    /// Bevy looks those up by where a ray enters the fog, which misses a
    /// cone seen from the side.
    fn apply_volume_lights(&mut self) {
        let atmospherics = self.render.as_ref().is_none_or(|r| r.options.atmospherics);
        let lights: Vec<environment::VolumeLight> = if atmospherics { self.env.volume_lights.clone() } else { Vec::new() };
        let (centre, radius) = self.bounds.unwrap_or((Vec3::ZERO, 1.0));
        let mut volumes = Vec::new();
        let mut lit = HashSet::new();
        for l in &lights {
            for id in &l.lights {
                let Some(s) = self.samples.iter().find(|s| s.id.0 == *id) else { continue };
                if lights::kind(s) != Some(lights::Kind::Directional) {
                    continue;
                }
                lit.insert(*id);
                let fog = FogVolume {
                    fog_color: Color::srgb(l.color[0], l.color[1], l.color[2]),
                    density_factor: environment::volume_light_density(l.density, radius * 2.0),
                    absorption: 0.0,
                    scattering: 0.5,
                    scattering_asymmetry: 0.0,
                    ..Default::default()
                };
                volumes.push((fog, Transform::from_translation(centre).with_scale(Vec3::splat(radius * 2.2))));
            }
        }
        let entities: Vec<(u64, Entity)> = self.scene_lights.iter().map(|(id, l)| (*id, l.entity)).collect();
        for (id, e) in entities {
            let mut e = self.app.world_mut().entity_mut(e);
            if lit.contains(&id) {
                e.insert(VolumetricLight);
            } else {
                e.remove::<VolumetricLight>();
            }
        }
        for e in self.volumes.drain(..) {
            self.app.world_mut().despawn(e);
        }
        // Every camera's layers see the fog.
        let layers = RenderLayers::from_layers(&[VIEWPORT_LAYER, RENDER_LAYER, SELECTED_LAYER, HIDDEN_LAYER, LIGHT_LAYER]);
        for (fog, t) in volumes {
            let e = self.app.world_mut().spawn((fog, t, layers.clone())).id();
            self.volumes.push(e);
        }
    }

    /// The environment pass's Volume Light entries for `env`: up to four
    /// omni / spot lights, their in-scattering colour in the units the
    /// main pass lights surfaces with (Bevy's exposure and 1/π), so a
    /// density of 100% seen through a tenth of the scene's diameter glows
    /// about as bright as a white surface the light lights there.
    fn volume_light_uniform(&self, env: &EnvSettings) -> [[bevy::math::Vec4; 4]; 4] {
        use bevy::math::Vec4;
        let mut out = [[Vec4::ZERO; 4]; 4];
        let (centre, radius) = self.bounds.unwrap_or((Vec3::ZERO, 1.0));
        let tint = Color::srgb(env.tint[0], env.tint[1], env.tint[2]).to_linear();
        let tint = glam::Vec3::new(tint.red, tint.green, tint.blue) * env.level.max(0.0);
        let mut n = 0;
        for l in &env.volume_lights {
            let fog = linear(l.color);
            let sigma = l.density.max(0.0) / 100.0 * 5.0 / radius.max(1e-3);
            for id in &l.lights {
                let Some(s) = self.samples.iter().find(|s| s.id.0 == *id) else { continue };
                let kind = match lights::kind(s) {
                    Some(lights::Kind::Point) => 1.0,
                    Some(lights::Kind::Spot) => 2.0,
                    _ => continue,
                };
                if n == 4 {
                    return out;
                }
                let mut s = s.clone();
                s.color *= tint;
                let pos = Vec3::from(s.position.to_array());
                let d = (pos - centre).length();
                let c = s.color.max(glam::Vec3::ZERO);
                let b = c.max_element();
                let intensity = lights::lumens(&s, d) / (4.0 * std::f32::consts::PI);
                let k = if b > 0.0 { sigma * intensity / std::f32::consts::PI * lights::EXPOSURE / b } else { 0.0 };
                let (cos_outer, cos_inner) = match s.emitter {
                    loom_scene::light::Emitter::Spot { hotspot, falloff } if kind == 2.0 => ((falloff * 0.5).to_radians().cos(), (hotspot.min(falloff) * 0.5).to_radians().cos()),
                    _ => (-1.0, -1.0),
                };
                out[0][n] = pos.extend(kind);
                out[1][n] = Vec3::from(s.direction.to_array()).normalize_or(Vec3::NEG_Z).extend(cos_outer);
                out[2][n] = Vec4::new(fog.x * c.x * k, fog.y * c.y * k, fog.z * c.z * k, cos_inner.max(cos_outer + 1e-4));
                // Marched as far as the light reaches, within the scene.
                out[3][n] = Vec4::new(lights::range(&s, d, radius).min(d + radius), 0.0, 0.0, 0.0);
                n += 1;
            }
        }
        out
    }

    /// Applies the scene's environment (Environment and Effects at the
    /// current time) to the lights and every view.
    pub fn set_environment(&mut self, env: &EnvSettings) {
        if *env == self.env {
            return;
        }
        let map_changed = env.map != self.env.map;
        self.env = env.clone();
        if map_changed {
            self.env_map = env.map.as_ref().and_then(|(p, _)| self.env_texture(p));
        }
        if !env.exposure.is_some_and(|x| x.meters()) {
            self.metered.clear();
        }
        self.apply_lights();
        let slots: Vec<u32> = self.views.keys().copied().collect();
        for slot in slots {
            self.apply_view_env(slot);
        }
        self.settle = self.settle.max(3);
    }

    /// The environment ranges and focus of viewport `slot`.
    pub fn set_view_environment(&mut self, slot: u32, view: ViewEnv) {
        if self.view_envs.insert(slot, view) != Some(view) && self.views.contains_key(&slot) {
            self.apply_view_env(slot);
        }
    }

    /// The environment as view `slot` uses it: a render leaves out what
    /// Render Setup turned off; the metering camera has no Depth of Field.
    fn env_for(&self, slot: u32) -> EnvSettings {
        let mut env = self.env.clone();
        if slot == RENDER_SLOT || slot == METER_SLOT {
            if let Some(o) = self.render.as_ref().map(|r| &r.options) {
                if env.background == [0.0; 3] && o.background != [0.0; 3] {
                    env.background = o.background;
                }
                if !o.atmospherics {
                    env = env.without_atmospherics();
                }
                if !o.effects {
                    env = env.without_effects();
                }
            }
        }
        if slot == METER_SLOT {
            env.depth_of_field = None;
        }
        env
    }

    /// Exposure, the environment pass and volumetric fog on view `slot`'s camera.
    fn apply_view_env(&mut self, slot: u32) {
        // Render To Texture bakes in texture space: no environment there.
        if slot == bake::BAKE_SLOT {
            return;
        }
        let Some(v) = self.views.get(&slot) else { return };
        let entity = v.camera;
        let env = self.env_for(slot);
        let metered_slot = if slot == METER_SLOT { self.meter.unwrap_or(slot) } else { slot };
        let view = if slot == RENDER_SLOT || (slot == METER_SLOT && self.meter == Some(RENDER_SLOT)) {
            self.render.as_ref().map(|r| r.options.view).unwrap_or_default()
        } else {
            self.view_envs.get(&metered_slot).copied().unwrap_or_default()
        };
        let render = slot == RENDER_SLOT;
        let frame = if render { self.render.as_ref().map_or([0.0, 0.0, 1.0, 1.0], |r| r.frame) } else { [0.0, 0.0, 1.0, 1.0] };
        let world = self.app.world();
        let Some(t) = world.get::<Transform>(entity).copied() else { return };
        let bounds = self.bounds.unwrap_or((Vec3::ZERO, 1.0));
        let mut uniform = env_uniform(&env, view, &t, bounds, render, frame);
        [uniform.vlight_pos, uniform.vlight_dir, uniform.vlight_color, uniform.vlight_range] = self.volume_light_uniform(&env);
        let metered = self.metered.get(&slot).copied().unwrap_or(0.0);
        let (mut tonemapping, grading, mut hdr) = if slot == METER_SLOT {
            (Tonemapping::None, ColorGrading { global: ColorGradingGlobal { exposure: -METER_STOPS, ..Default::default() }, ..Default::default() }, false)
        } else {
            exposure_components(env.exposure.as_ref(), metered)
        };
        if render {
            if let Some(r) = self.render.as_ref() {
                if env.exposure.is_none() {
                    use loom_scene::render::Tonemapper;
                    tonemapping = match r.options.quality.tonemapper {
                        Tonemapper::None => Tonemapping::None,
                        Tonemapper::Reinhard => Tonemapping::Reinhard,
                        Tonemapper::Aces => Tonemapping::AcesFitted,
                        Tonemapper::AgX => Tonemapping::AgX,
                        Tonemapper::TonyMcMapface => Tonemapping::TonyMcMapface,
                        Tonemapper::BlenderFilmic => Tonemapping::BlenderFilmic,
                    };
                }
                if r.options.screen_space() || r.options.quality.exposure != 0.0 {
                    hdr = true;
                }
            }
        }
        // HDR with no tonemapping curve (Linear, Process Background): Bevy
        // skips tonemapping and its grading, so the pass applies the exposure.
        if let (true, Tonemapping::None, Some(x)) = (hdr, tonemapping, env.exposure.as_ref()) {
            let stops = x.stops() + if x.meters() { metered } else { 0.0 };
            uniform.grade = bevy::math::Vec4::new(2f32.powf(stops), x.contrast / 50.0, 1.0, 0.0);
        }
        let map = (render && env.map.is_some()).then(|| self.env_map.clone()).flatten();
        let volumetric = !env.volume_lights.is_empty() && !self.volumes.is_empty();
        let mut e = self.app.world_mut().entity_mut(entity);
        e.insert((tonemapping, grading, post::EnvPass { uniform, map }));
        if volumetric {
            e.insert(VolumetricFog { ambient_intensity: 0.0, step_count: 64, ..Default::default() });
        } else {
            e.remove::<VolumetricFog>();
        }
        if let Some(mut cam) = e.get_mut::<Camera>() {
            if cam.hdr != hdr {
                cam.hdr = hdr;
            }
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
        self.sync_deforming(objects, &HashSet::new());
    }

    /// CPU-deformed nodes keep immutable Bevy mesh/material/entity handles;
    /// only their final posed attributes and current bounds are uploaded.
    pub fn sync_deforming(&mut self, objects: &[SceneObject], deforming: &HashSet<u64>) {
        let _scope = loom_render::profile::Scope::new("bevy.sync");
        let mut set = std::mem::take(&mut self.objects);
        self.bounds = self.sync_set(&mut set, objects, false, deforming, &self.device.limits());
        self.objects = set;
    }

    /// The hidden objects a Render Hidden Geometry render draws.
    pub fn sync_hidden(&mut self, objects: &[SceneObject]) {
        self.sync_hidden_deforming(objects, &HashSet::new());
    }

    pub fn sync_hidden_deforming(&mut self, objects: &[SceneObject], deforming: &HashSet<u64>) {
        let mut set = std::mem::take(&mut self.hidden);
        self.sync_set(&mut set, objects, true, deforming, &self.device.limits());
        self.hidden = set;
    }

    /// Brings `set` in line with `objects`; returns their bounds.
    fn sync_set(&mut self, set: &mut HashMap<u64, Object>, objects: &[SceneObject], hidden: bool, deforming: &HashSet<u64>, limits: &wgpu::Limits) -> Option<(Vec3, f32)> {
        let (gpu_updated, gpu_bounds) = self.try_gpu_poses(set, objects, hidden, deforming);
        let pending: Vec<_> = objects.iter().filter(|o| !gpu_updated.contains(&o.id)).collect();
        let mut seen = gpu_updated;
        let (mut lo, mut hi) = gpu_bounds.unwrap_or((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)));
        let mut batches = Vec::new();
        let mut materials = Vec::new();
        let mut material_data = Vec::new();
        let mut tangent_caches = Vec::new();
        let geometry = {
            let _scope = loom_render::profile::Scope::new("bevy.geometry_data");
            let jobs = pending.iter().map(|o| {
                let key = (hidden, o.id);
                (key, self.geometry.remove(&key).unwrap_or_default(), o.mesh)
            }).collect();
            prepared_geometry::prepare_batch(bevy_tasks::ComputeTaskPool::get(), jobs)
        };
        for (o, (key, cache, data)) in pending.into_iter().zip(geometry) {
            self.geometry.insert(key, cache);
            seen.insert(o.id);
            let dynamic = deforming.contains(&o.id) && DynamicGeometryBuffer::supported(limits, data.positions.len());
            let mirrored = o.transform.determinant() < 0.0;
            if let Some(b) = data.aabb_transformed(o.transform) {
                lo = lo.min(Vec3::from(b.min()));
                hi = hi.max(Vec3::from(b.max()));
            }
            let hash = {
                let _scope = loom_render::profile::Scope::new("bevy.geometry_hash");
                data.content_hash() ^ convert::material_ids_hash(o.mesh).rotate_left(1) ^ (mirrored as u64).rotate_left(2)
            };
            let layers = self.object_layers(o.id, o.renderable, o.flags, hidden);
            if let Some(obj) = set.get_mut(&o.id) {
                if !obj.gpu_pose_current && !obj.material_dirty && obj.dynamic == dynamic && obj.mesh_hash == hash && obj.surfaces == o.surfaces && obj.wire_color == o.wire_color && obj.flags.visibility == o.flags.visibility {
                    if obj.transform != o.transform {
                        obj.transform = o.transform;
                        for (e, _) in &obj.parts {
                            let mut entity = self.app.world_mut().entity_mut(*e);
                            entity.insert(GlobalTransform::from(o.transform));
                            if o.transform == glam::Mat4::IDENTITY { entity.insert(Transform::IDENTITY); }
                            else { entity.remove::<Transform>(); }
                        }
                        self.settle = self.settle.max(3);
                    }
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
            // Top/Bottom: its bands become per-ID surfaces, the faces get their IDs by their normals.
            let split_mesh;
            let split_data;
            let (src, src_data, surfaces) = match o.surfaces.as_ref().and_then(loom_scene::material::split_surfaces) {
                Some((s, split)) => {
                    let mut m = o.mesh.clone();
                    loom_scene::material::apply_split(&mut m, &split, Vec3::Z, Vec3::from_array(o.flags.local_up));
                    split_data = mesh_data(&m);
                    split_mesh = m;
                    (&split_mesh, &split_data, Some(s))
                }
                None => (o.mesh, &data, o.surfaces.clone()),
            };
            // (material ID drawn or all faces, its surface, drawn as the back faces)
            let mut parts: Vec<(Option<u16>, Option<Surface>, bool)> = match &surfaces {
                Some(Surfaces::PerId(_)) => {
                    let ids: std::collections::BTreeSet<_> = src.faces().iter().filter(|f| !f.hidden).map(|f| f.material_id).collect();
                    ids.into_iter().map(|id| (Some(id), surfaces.as_ref().map(|s| s.for_face(id).into_owned()), false)).collect()
                }
                Some(Surfaces::Single(s)) => vec![(None, Some((**s).clone()), false)],
                None => vec![(None, None, false)],
            };
            // Double Sided: the back material on the faces turned around.
            let backs: Vec<(Option<u16>, Option<Surface>, bool)> = parts
                .iter()
                .filter_map(|(id, s, _)| match s.as_ref().and_then(|s| s.split.as_deref()) {
                    Some(loom_scene::material::Split::DoubleSided { back }) => Some((*id, Some(back.clone()), true)),
                    _ => None,
                })
                .collect();
            let has_backs = !backs.is_empty();
            parts.extend(backs);
            let mut old = set.remove(&o.id);
            let mut previous_caches: HashMap<_, _> = old.as_mut().map(|obj| {
                obj.part_ids.iter().copied().zip(std::mem::take(&mut obj.tangent_caches)).collect()
            }).unwrap_or_default();
            let part_ids = parts.iter().map(|(id, _, _)| *id).collect::<Vec<_>>();
            let count = parts.len();
            for (id, surface, back) in parts {
                let _scope = loom_render::profile::Scope::new("bevy.material_partition");
                let mat = self.material(surface.as_ref(), o.wire_color, o.flags.visibility);
                let faceted = surface.as_ref().is_some_and(|s| s.faceted);
                let mut d = if id.is_none() && !faceted {
                    src_data.clone()
                } else {
                    convert::mesh_data_part(src, src_data, id, faceted)
                };
                if back {
                    d = d.flipped();
                }
                // Match world-baked polygon normals when reflection reverses winding.
                if mirrored { for n in &mut d.normals { for c in n { *c = -*c; } } }
                let vc = surface.as_ref().is_some_and(|s| s.shows_vertex_colors()).then(|| {
                    let mut part = src.clone();
                    for f in part.faces_mut() {
                        f.hidden |= id.is_some_and(|id| f.material_id != id);
                    }
                    convert::vertex_colors(&part)
                });
                tangent_caches.push(if back { Default::default() } else { previous_caches.remove(&id).unwrap_or_default() });
                material_data.push((d, mat.normal_map_texture.is_some()));
                materials.push((mat, surface, vc));
            }
            batches.push((o, hash, layers, count, dynamic && !has_backs, part_ids, old));
        }
        // Mesh/tangent work is independent across objects as well as material
        // IDs. Submit one batch so Body, Face and clothing can overlap.
        let meshes = convert::material_meshes_cached(bevy_tasks::ComputeTaskPool::get(), &material_data, &mut tangent_caches);
        let mut prepared = materials.into_iter().zip(material_data).zip(meshes).zip(tangent_caches);
        for (o, hash, layers, count, dynamic, part_ids, mut old) in batches {
            let mut caches = Vec::with_capacity(count);
            let prepared: Vec<_> = prepared.by_ref().take(count).map(|((((mat, surface, vc), data), mut mesh), cache)| {
                caches.push(cache);
                if let Some(colors) = vc {
                    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
                }
                if mat.normal_map_texture.is_none() && mat.anisotropy_strength > 0.0 {
                    let _ = mesh.generate_tangents();
                }
                ((mat, surface, data), mesh)
            }).collect();
            let can_reuse = old.as_ref().is_some_and(|obj| dynamic && obj.dynamic
                && !obj.material_dirty && obj.surfaces == o.surfaces && obj.wire_color == o.wire_color
                && obj.flags.visibility == o.flags.visibility && obj.part_ids == part_ids
                && obj.buffers.len() == count);
            if can_reuse {
                let obj = old.as_mut().unwrap();
                let mut bounds = Vec::with_capacity(count);
                let update_scope = loom_render::profile::Scope::new("bevy.posed_upload");
                for (buffer, (_, mesh)) in obj.buffers.iter_mut().zip(&prepared) {
                    match buffer.update(&self.queue, mesh) { Ok(b) => bounds.push(b), Err(_) => break }
                }
                drop(update_scope);
                if bounds.len() == count {
                    let _scope = loom_render::profile::Scope::new("bevy.entity_upload");
                    for ((entity, _), bounds) in obj.parts.iter().zip(bounds) {
                        let mut e = self.app.world_mut().entity_mut(*entity);
                        let world_tx = glam::Mat4::from_scale(glam::Vec3::splat(self.scale)) * o.transform;
                        e.insert((GlobalTransform::from(world_tx), bounds, layers.clone()));
                        if o.transform == glam::Mat4::IDENTITY { e.insert(Transform::from_scale(Vec3::splat(self.scale))); } else { e.remove::<Transform>(); }
                        Self::shadow_flags(e, Self::cast_flags(o));
                    }
                    obj.tangent_caches = caches;
                    obj.gpu_pose_current = false;
                    obj.mesh_hash = hash;
                    obj.transform = o.transform;
                    obj.renderable = o.renderable;
                    obj.flags = o.flags;
                    set.insert(o.id, old.take().unwrap());
                    self.settle = self.settle.max(3);
                    continue;
                }
            }
            if let Some(obj) = old {
                for (e, _) in obj.parts { self.app.world_mut().despawn(e); }
            }
            let mut entities = Vec::with_capacity(count);
            let mut buffers = Vec::with_capacity(if dynamic { count } else { 0 });
            for ((mat, surface, (d, _)), mesh) in prepared {
                let _scope = loom_render::profile::Scope::new("bevy.entity_upload");
                if deforming.contains(&o.id) && !dynamic {
                    // Capacity/empty eligibility is separate from validity: the
                    // full-fidelity ordinary path must not conceal bad data.
                    dynamic_geometry::validate_geometry(&mesh).expect("invalid final posed geometry");
                }
                let buffer = if dynamic {
                    Some(DynamicGeometryBuffer::new(&self.device, &mesh)
                        .expect("eligible final posed geometry must have valid attributes"))
                } else { None };
                let world = self.app.world_mut();
                let mesh = world.resource_mut::<Assets<Mesh>>().add(mesh);
                let look_params = look::params(surface.as_ref());
                let material_handle = if let Some((buffer, _)) = &buffer {
                    ObjectMaterial::Dynamic(world.resource_mut::<Assets<DynamicMaterial>>().add(DynamicMaterial { base: mat, extension: buffer.extension() }))
                } else if look_params.flags != 0 {
                    ObjectMaterial::Look(world.resource_mut::<Assets<look::LoomMaterial>>().add(look::LoomMaterial { base: mat, extension: look::Look { params: look_params } }))
                } else {
                    ObjectMaterial::Standard(world.resource_mut::<Assets<StandardMaterial>>().add(mat))
                };
                let world_tx = glam::Mat4::from_scale(glam::Vec3::splat(self.scale)) * o.transform;
                let mut e = world.spawn((Transform::from_scale(Vec3::splat(self.scale)), GlobalTransform::from(world_tx), Mesh3d(mesh), layers.clone()));
                match &material_handle {
                    ObjectMaterial::Standard(h) => { e.insert(MeshMaterial3d(h.clone())); }
                    ObjectMaterial::Dynamic(h) => { e.insert(MeshMaterial3d(h.clone())); }
                    ObjectMaterial::Look(h) => { e.insert(MeshMaterial3d(h.clone())); }
                }
                if o.transform != glam::Mat4::IDENTITY { e.remove::<Transform>(); }
                let mut e = Self::shadow_flags(e, Self::cast_flags(o));
                if let Some((buffer, bounds)) = buffer { e.insert(bounds); buffers.push(buffer); }
                else if let Some(b) = d.aabb() { e.insert(b); }
                let entity = e.id();
                if let Some(r) = self.render.as_mut() {
                    if r.options.force_two_sided {
                        material_handle.edit(self.app.world_mut(), |m| {
                            r.two_sided.push((material_handle.clone(), m.double_sided, m.cull_mode));
                            m.double_sided = true;
                            m.cull_mode = None;
                        });
                    }
                    if r.options.quality.reflections {
                        material_handle.edit(self.app.world_mut(), |m| {
                            r.deferred.push((material_handle.clone(), m.opaque_render_method));
                            m.opaque_render_method = bevy::pbr::OpaqueRendererMethod::Deferred;
                        });
                    }
                }
                entities.push((entity, material_handle));
            }
            set.insert(o.id, Object { parts: entities, dynamic, gpu_pose_current: false, material_dirty: false, buffers, part_ids, tangent_caches: caches, mesh_hash: hash, transform: o.transform, surfaces: o.surfaces.clone(), wire_color: o.wire_color, renderable: o.renderable, flags: o.flags });
            self.settle = self.settle.max(3);
        }
        let gone: Vec<u64> = set.keys().filter(|id| !seen.contains(id)).copied().collect();
        for id in gone {
            self.geometry.remove(&(hidden,id));
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
        for obj in self.objects.values_mut().chain(self.hidden.values_mut()) {
            obj.material_dirty = true;
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
        // Bevy has one UV transform per material: the first map's Coordinates
        // (Diffuse, else Opacity, Bump, Specular Level, Self-Illumination);
        // a map with other Coordinates is baked into that map's frame.
        let frame = [&s.diffuse_map, &s.opacity_map, &s.bump_map, &s.specular_level_map, &s.self_illum_map].into_iter().flatten().next().cloned();
        if let Some(f) = &frame {
            m.uv_transform = convert::uv_transform(f);
        }
        let frame = frame.as_ref();
        let diffuse = s.diffuse_map.as_ref().and_then(|map| self.in_frame(map, frame).map(|(img, k)| (img, map.clone(), k)));
        let opacity = s.opacity_map.as_ref().and_then(|map| self.in_frame(map, frame).map(|(img, k)| (img, map.clone(), k)));
        let diffuse = diffuse.map(|(i, m, k)| ((i, m), k));
        let opacity = opacity.map(|(i, m, k)| ((i, m), k));
        let (diffuse, diffuse_key) = (diffuse.as_ref().map(|d| d.0.clone()), diffuse.as_ref().map(|d| d.1.clone()).unwrap_or_default());
        let (opacity, opacity_key) = (opacity.as_ref().map(|d| d.0.clone()), opacity.as_ref().map(|d| d.1.clone()).unwrap_or_default());
        if diffuse.is_some() || opacity.is_some() {
            let key = format!("base:{:?}:{}:{}:{}:{}:{}", s.diffuse, s.opacity, diffuse.as_ref().map(|(_, m)| m.amount.to_string()).unwrap_or_default(), diffuse_key, opacity.as_ref().map(|(_, m)| m.amount.to_string()).unwrap_or_default(), opacity_key);
            let decal = frame.map(|m| m.coords.decal()).unwrap_or([false; 2]);
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
        }
        if let Some(map) = s.bump_map.as_ref() {
            if let Some((img, k)) = self.in_frame(map, frame) {
                let key = format!("bump:{k}:{}:{}{}", map.amount, map.params.flip_red, map.params.flip_green);
                m.normal_map_texture = if map.kind == loom_scene::material::MapType::NormalBump {
                    self.texture_from(&key, TextureFormat::Rgba8Unorm, [false; 2], || convert::normal_bump(&img, map.params.flip_red, map.params.flip_green))
                } else {
                    self.texture_from(&key, TextureFormat::Rgba8Unorm, [false; 2], || loom_scene::maps::bump_to_normal(&img, map.amount / 100.0))
                };
            }
        }
        if let Some(map) = s.specular_level_map.as_ref() {
            if let Some((img, k)) = self.in_frame(map, frame) {
                let key = format!("spec:{k}:{}", map.amount);
                m.specular_texture = self.texture_from(&key, TextureFormat::Rgba8Unorm, [false; 2], || convert::luminance(&img, (map.amount / 100.0).clamp(0.0, 1.0)));
                // The texture's alpha 0–1 maps to reflectance 0–0.5 times
                // this: white in the map gives the Specular Level's own.
                m.reflectance *= 2.0;
            }
        }
        if let Some(map) = s.self_illum_map.as_ref() {
            if let Some((img, k)) = self.in_frame(map, frame) {
                let key = format!("illum:{k}:{}", map.amount);
                m.emissive_texture = self.texture_from(&key, TextureFormat::Rgba8Unorm, [false; 2], || convert::luminance(&img, (map.amount / 100.0).clamp(0.0, 1.0)));
                let d = Color::srgb(s.diffuse.x, s.diffuse.y, s.diffuse.z).to_linear();
                m.emissive = bevy::color::LinearRgba::rgb(d.red, d.green, d.blue);
            }
        }
        m
    }

    /// `map` as a texture under the material's UV transform (the Coordinates
    /// of `frame`) and a key for its pixels: its own tile when its Coordinates
    /// are the frame's, else baked through the frame (exact when the two
    /// tilings repeat together, as integer ratios do).
    fn in_frame(&mut self, map: &Map, frame: Option<&Map>) -> Option<(std::sync::Arc<loom_scene::maps::Image>, String)> {
        let key = loom_scene::maps::texture_key(map);
        let same = frame.is_none_or(|f| f.coords.matrix() == map.coords.matrix() && f.coords.mirror == map.coords.mirror);
        let img = self.baked(map)?;
        if same {
            return Some((img, key));
        }
        let f = frame.expect("checked");
        let fm = f.coords.matrix();
        let fkey = format!("{key}@{fm:?}{:?}", f.coords.mirror);
        let size = img.width.max(img.height).clamp(256, 2048);
        let images = self.images.clone();
        Some((std::sync::Arc::new(convert::bake_in_frame(map, &fm, size, &images)), fkey))
    }

    /// A map baked to pixels (one map-space tile), its bitmaps read once per path.
    fn baked(&mut self, map: &Map) -> Option<std::sync::Arc<loom_scene::maps::Image>> {
        loom_scene::maps::load_images(map, &mut self.images, &mut |path| {
            image::open(path).ok().map(|img| {
                let rgba = img.to_rgba8();
                std::sync::Arc::new(loom_scene::maps::Image { width: rgba.width(), height: rgba.height(), rgba: rgba.into_raw() })
            })
        });
        let plain = loom_scene::maps::is_plain_bitmap(map);
        if plain {
            return self.images.get(&map.path).cloned().flatten();
        }
        loom_scene::maps::bake(map, &self.images).map(std::sync::Arc::new)
    }

    /// The environment map's bitmap (None when it cannot be read).
    fn env_texture(&mut self, path: &str) -> Option<Handle<Image>> {
        let img = image::open(path).ok()?.to_rgba8();
        let (width, height) = img.dimensions();
        self.texture_from(&format!("env:{path}"), TextureFormat::Rgba8UnormSrgb, [false; 2], || loom_scene::maps::Image { width, height, rgba: img.into_raw() })
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
        // Automatic / Linear exposure: the viewports take turns being metered.
        if self.meter.is_none() && self.env.exposure.is_some_and(|x| x.meters()) && slot >= self.meter_next {
            self.meter_next = slot + 1;
            self.place_meter(slot, camera.bevy_projection(aspect), camera.view, RenderLayers::layer(VIEWPORT_LAYER));
        }
    }

    /// Points the metering camera at view `slot`'s camera for this frame.
    fn place_meter(&mut self, slot: u32, projection: Projection, view: bevy::math::Mat4, layers: RenderLayers) {
        self.meter = Some(slot);
        self.place_view(METER_SLOT, [METER_SIZE; 2], projection, view, [0.0; 4], false, layers);
    }

    /// After an update: the metered view's exposure from the metering image.
    fn read_meter(&mut self) {
        let Some(slot) = self.meter.take() else { return };
        let Ok(img) = self.read_back(METER_SLOT) else { return };
        let kind = self.env.exposure.map_or(ExposureKind::None, |x| x.kind);
        let scale = 2f32.powf(METER_STOPS);
        let lum: Vec<f32> = img
            .pixels()
            .filter(|p| p.0[3] > 0)
            .map(|p| {
                let c = [0, 1, 2].map(|i| environment::srgb_to_linear(p.0[i] as f32 / 255.0) * scale);
                0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
            })
            .collect();
        let stops = metering_stops(&lum, kind);
        if self.metered.insert(slot, stops) != Some(stops) {
            self.apply_view_env(slot);
            self.settle = self.settle.max(2);
        }
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
                    // The environment pass reads the depth buffer.
                    let camera_3d = Camera3d { depth_texture_usages: (TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING).into(), ..Default::default() };
                    let entity = self.app.world_mut().spawn((camera_3d, Tonemapping::AcesFitted, Camera { order: slot.min(1 << 20) as isize, ..Default::default() })).id();
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
        self.apply_view_env(slot);
    }

    /// Gives the render camera the render's quality settings: exposure
    /// control, ambient occlusion, reflections and the sky dome.
    fn setup_render_camera(&mut self) {
        use bevy::core_pipeline::tonemapping::DebandDither;
        use bevy::pbr::{prelude::EnvironmentMapLight, ScreenSpaceAmbientOcclusion, ScreenSpaceAmbientOcclusionQualityLevel, ScreenSpaceReflections, ShadowFilteringMethod};
        use bevy::render::camera::Exposure;
        use loom_scene::render::{AoQuality, Tonemapper};
        let Some(r) = self.render.as_ref() else { return };
        let Some(v) = self.views.get(&RENDER_SLOT) else { return };
        let o = r.options.clone();
        let q = &o.quality;
        let entity = v.camera;
        let has_env_exposure = self.env.exposure.is_some();
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
            let tint = if self.env.ambient != [0.0; 3] { self.env.ambient } else { o.ambient };
            let (color, brightness) = self.ambient_light(tint);
            let c = color.to_linear();
            let (d, s) = quality::sky_maps([c.red, c.green, c.blue], q.ground);
            let mut images = self.app.world_mut().resource_mut::<Assets<Image>>();
            (images.add(d), images.add(s), brightness * q.sky_intensity * quality::SKY_MAP_SCALE)
        });
        let world = self.app.world_mut();
        let mut e = world.entity_mut(entity);
        if !has_env_exposure {
            e.insert((
                tonemapping,
                if q.dither { DebandDither::Enabled } else { DebandDither::Disabled },
                Exposure { ev100: Exposure::default().ev100 - q.exposure },
            ));
        }
        // A filter that changes with every frame adds up over the passes.
        e.insert(if o.passes() > 1 { ShadowFilteringMethod::Temporal } else { ShadowFilteringMethod::Gaussian });
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
        let handles: Vec<ObjectMaterial> = self.objects.values().chain(self.hidden.values()).flat_map(|o| o.parts.iter().map(|(_, h)| h.clone())).collect();
        let world = self.app.world_mut();
        world.insert_resource(DirectionalLightShadowMap { size: options.shadow_map_size.clamp(512, 8192) as usize });
        let (mut two_sided, mut deferred) = (Vec::new(), Vec::new());
        for h in handles {
            if options.force_two_sided {
                h.edit(world, |m| {
                    two_sided.push((h.clone(), m.double_sided, m.cull_mode));
                    m.double_sided = true;
                    m.cull_mode = None;
                });
            }
            if options.quality.reflections {
                // The reflections read the deferred G-buffer.
                h.edit(world, |m| {
                    deferred.push((h.clone(), m.opaque_render_method));
                    m.opaque_render_method = bevy::pbr::OpaqueRendererMethod::Deferred;
                });
            }
        }
        self.scale = options.world_scale();
        self.light_pass = 0;
        self.render = Some(ActiveRender { options, two_sided, deferred, camera_ready: false, accum: None, frame: [0.0, 0.0, 1.0, 1.0] });
        self.metered.remove(&RENDER_SLOT);
        self.meter = None;
        self.rescale();
        self.apply_lights();
        self.relayer();
        self.settle = self.settle.max(3);
    }

    /// Puts every object at the current world scale.
    fn rescale(&mut self) {
        let s = glam::Mat4::from_scale(glam::Vec3::splat(self.scale));
        let parts: Vec<(Entity, glam::Mat4)> = self.objects.values().chain(self.hidden.values()).flat_map(|o| o.parts.iter().map(move |(e, _)| (*e, o.transform))).collect();
        for (e, transform) in parts {
            let scaled = s * transform;
            let mut entity = self.app.world_mut().entity_mut(e);
            entity.insert(GlobalTransform::from(scaled));
            if transform == glam::Mat4::IDENTITY {
                entity.insert(Transform::from_scale(Vec3::splat(self.scale)));
            } else {
                entity.remove::<Transform>();
            }
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
        let margin = if o.passes() > 1 || o.screen_space() || q.supersampling > 1 {
            radius.ceil() as u32 + 1 + if o.screen_space() { 16 } else { 0 }
        } else {
            0
        };
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
        if let Some(r) = self.render.as_mut() {
            r.frame = sub;
        }
        // Cleared to transparent black: the environment pass draws the background.
        let clear = [0.0; 4];
        let camera = camera.scaled(self.scale);
        self.place_view(RENDER_SLOT, internal, Projection::custom(BucketProjection::new(&camera, aspect, sub)), camera.view, clear, o.msaa, layers.clone());
        if self.render.as_ref().is_some_and(|r| !r.camera_ready) {
            self.setup_render_camera();
            if let Some(r) = self.render.as_mut() {
                r.camera_ready = true;
            }
        }
        // Automatic / Linear exposure meters the whole frame once per render.
        if self.env.exposure.is_some_and(|x| x.meters()) && !self.metered.contains_key(&RENDER_SLOT) && self.meter.is_none() {
            self.place_meter(RENDER_SLOT, camera.bevy_projection(aspect), camera.view, layers);
        }
        Ok(())
    }

    /// Adds the pass rendered last to its bucket and returns the bucket so
    /// far (sRGB RGBA, background alpha 0); `bucket_done` once all passes are in.
    pub fn read_render(&mut self) -> Result<image::RgbaImage, String> {
        let img = self.read_back(RENDER_SLOT)?;
        let r = self.render.as_mut().ok_or("no render started")?;
        let (q, passes) = (r.options.quality.clone(), r.options.passes());
        let a = r.accum.as_mut().ok_or("no bucket set")?;
        let (j, filter, size) = if q.antialiasing { (quality::jitter(a.pass, passes), q.filter, q.filter_size.clamp(1.0, 10.0)) } else { ([0.0; 2], loom_scene::render::AaFilter::Area, 1.0) };
        a.add(&img, j, filter, size)?;
        Ok(a.image([0.0; 3]))
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
        self.metered.remove(&RENDER_SLOT);
        let Some(r) = self.render.take() else { return };
        self.scale = 1.0;
        self.light_pass = 0;
        self.rescale();
        self.apply_lights();
        let world = self.app.world_mut();
        world.insert_resource(DirectionalLightShadowMap { size: 2048 });
        for (h, double_sided, cull) in r.two_sided {
            h.edit(world, |m| { m.double_sided = double_sided; m.cull_mode = cull; });
        }
        for (h, method) in r.deferred {
            h.edit(world, |m| { m.opaque_render_method = method; });
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
        let _scope = loom_render::profile::Scope::new("bevy.update");
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
        for obj in self.objects.values_mut().chain(self.hidden.values_mut()) {
            for buffer in &mut obj.buffers { buffer.advance_history(&self.queue); }
        }
        self.read_meter();
        if !self.views.iter().any(|(s, v)| v.used && *s >= self.meter_next && *s < METER_SLOT) {
            self.meter_next = 0;
        }
        for v in self.views.values_mut() {
            v.used = false;
        }
        // Pipelines compile synchronously, so a change
        // shows within a frame or two; a pipeline cache "waiting" list is no
        // signal (pipelines whose shaders never load stay in it).
        self.settle = self.settle.saturating_sub(1);
    }

    /// Whether a viewport or render camera was requested for this frame.
    pub fn has_requested_views(&self) -> bool {
        self.views.values().any(|view| view.used)
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

/// Stops that bring the metered luminances (linear, of the geometry only)
/// to mid grey: Linear uses their average, Automatic their log average.
pub fn metering_stops(lum: &[f32], kind: ExposureKind) -> f32 {
    if lum.is_empty() {
        return 0.0;
    }
    let avg = match kind {
        ExposureKind::Automatic => (lum.iter().map(|l| (l + 1e-4).ln()).sum::<f32>() / lum.len() as f32).exp(),
        _ => lum.iter().sum::<f32>() / lum.len() as f32,
    };
    (0.18 / avg.max(1e-5)).log2().clamp(-10.0, 10.0)
}

/// Tonemapping, colour grading and HDR of a view under `exposure` (None:
/// no exposure control) with `metered` stops from metering.
fn exposure_components(exposure: Option<&environment::Exposure>, metered: f32) -> (Tonemapping, ColorGrading, bool) {
    let Some(x) = exposure else { return (Tonemapping::AcesFitted, ColorGrading::default(), false) };
    let tonemapping = match x.kind {
        // Linear maps light evenly to the display range.
        ExposureKind::Linear => Tonemapping::None,
        // Logarithmic compresses the highlights by luminance.
        ExposureKind::Logarithmic => Tonemapping::ReinhardLuminance,
        _ => Tonemapping::AcesFitted,
    };
    let stops = x.stops() + if x.meters() { metered } else { 0.0 };
    let section = |gamma: f32| ColorGradingSection { contrast: x.contrast / 50.0, gamma, ..Default::default() };
    let mid_gamma = if x.kind == ExposureKind::Logarithmic { x.mid_tones.max(0.01) } else { 1.0 };
    let grading = ColorGrading { global: ColorGradingGlobal { exposure: stops, ..Default::default() }, shadows: section(1.0), midtones: section(mid_gamma), highlights: section(1.0) };
    (tonemapping, grading, x.process_background)
}

fn linear(c: [f32; 3]) -> bevy::math::Vec4 {
    bevy::math::Vec4::new(environment::srgb_to_linear(c[0]), environment::srgb_to_linear(c[1]), environment::srgb_to_linear(c[2]), 0.0)
}

/// The environment pass uniform of a view: its camera `cam`, the scene
/// `bounds` (centre, radius) for fog ranges and volume fog, whether it draws
/// the background (renders; viewports show their own), the frame part.
fn env_uniform(env: &EnvSettings, view: ViewEnv, cam: &Transform, (centre, radius): (Vec3, f32), draw_background: bool, frame: [f32; 4]) -> post::EnvUniform {
    use bevy::math::Vec4;
    let mut u = post::EnvUniform { frame: Vec4::from_array(frame), ..Default::default() };
    if draw_background {
        u.background = linear(env.background).with_w(1.0);
        if let Some((_, mapping)) = &env.map {
            let m = match mapping {
                EnvMapping::Screen => 0.0,
                EnvMapping::Spherical => 1.0,
                EnvMapping::Cylindrical => 2.0,
                EnvMapping::ShrinkWrap => 3.0,
            };
            u.map = Vec4::new(1.0, m, 0.0, 0.0);
        }
    }
    // Standard fog without the camera's Environment Ranges: the scene's near and far side.
    let d = (centre - cam.translation).dot(cam.forward().as_vec3());
    let [near_d, far_d] = view.ranges.unwrap_or([(d - radius).max(0.0), (d + radius).max(1e-3)]);
    for (i, f) in env.fogs.iter().take(environment::MAX_FOGS).enumerate() {
        let flag = |b: bool| if b { 1.0 } else { 0.0 };
        match *f {
            Fog::Standard { color, near, far, exponential, fog_background } => {
                u.fog_color[i] = linear(color).with_w(1.0);
                u.fog_a[i] = Vec4::new(near_d, far_d.max(near_d + 1e-3), near.clamp(0.0, 1.0), far.clamp(0.0, 1.0));
                u.fog_b[i] = Vec4::new(flag(fog_background), flag(exponential), 0.0, 0.0);
            }
            Fog::Layered { color, bottom, top, density, falloff, fog_background } => {
                let fall = match falloff {
                    LayerFalloff::None => 0.0,
                    LayerFalloff::Top => 1.0,
                    LayerFalloff::Bottom => 2.0,
                };
                u.fog_color[i] = linear(color).with_w(2.0);
                u.fog_a[i] = Vec4::new(bottom, top, environment::layer_extinction(density, bottom, top), fall);
                u.fog_b[i] = Vec4::new(flag(fog_background), 0.0, 0.0, 0.0);
            }
            Fog::Volume { color, density, fog_background } => {
                u.fog_color[i] = linear(color).with_w(3.0);
                // The fog fills a sphere a little larger than the scene's bounds.
                u.fog_a[i] = Vec4::new(environment::volume_extinction(density, radius), centre.x, centre.y, centre.z);
                u.fog_b[i] = Vec4::new(flag(fog_background), 0.0, radius * 1.1, 0.0);
            }
        }
    }
    if let (Some(dof), Some(focus)) = (env.depth_of_field, view.focus) {
        u.dof = Vec4::new(1.0, focus, dof.focal_range.max(0.0), dof.focal_limit.max(dof.focal_range + 1e-3));
        u.dof_loss = Vec4::new(dof.horiz_loss.max(0.0), dof.vert_loss.max(0.0), 0.0, 0.0);
    }
    u
}

#[cfg(test)]
mod affine_tests {
    use super::*;

    #[test]
    fn view_requests_are_consumed_and_can_resume_after_idle_frames() {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let Some(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else { return };
        if !supported(&adapter) { return; }
        let (device, queue) = pollster::block_on(adapter.request_device(&device_descriptor(&adapter), None)).unwrap();
        let mut renderer = BevyRenderer::new(adapter, device, queue);
        let camera = ViewCamera { view: glam::Mat4::IDENTITY, projection: glam::Mat4::IDENTITY };
        assert!(!renderer.has_requested_views());
        renderer.set_view(0, [64, 64], &camera, [0.0; 4], true);
        assert!(renderer.has_requested_views());
        renderer.update();
        assert!(!renderer.has_requested_views());
        renderer.set_view(0, [64, 64], &camera, [0.0; 4], true);
        assert!(renderer.has_requested_views(), "an existing view must request rendering again");
        renderer.update();
        assert!(!renderer.has_requested_views());
    }

    #[test]
    #[ignore = "actual renderer cache integration requires LOOM_PERF_SCENE"]
    fn actual_ray_renderer_preserves_tangent_neighbor_caches() {
        let path = std::env::var("LOOM_PERF_SCENE").expect("real saved scene path");
        let mut value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let scene: loom_scene::Scene = serde_json::from_value(value["scene"].take()).unwrap();
        let ids: Vec<_> = scene.nodes().filter(|n| n.modifiers.iter().any(|m| matches!(m.modifier, loom_scene::Modifier::Skin(_)))).map(|n| n.id).collect();
        let dynamic: HashSet<_> = ids.iter().map(|id| id.0).collect();
        let instance = wgpu::Instance::new(&Default::default());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).expect("real GPU required");
        let (device, queue) = pollster::block_on(adapter.request_device(&device_descriptor(&adapter), None)).unwrap();
        let mut renderer = BevyRenderer::new(adapter, device, queue);
        for time in [0.0, 39.25, 149.0] {
            scene.set_time(time);
            let meshes: Vec<_> = ids.iter().map(|id| {
                let mut mesh = scene.eval_mesh(*id).value.mesh.clone();
                loom_mesh::edit_ops::transform_mesh(&mut mesh, scene.object_matrix(*id)).unwrap();
                mesh
            }).collect();
            let objects: Vec<_> = ids.iter().zip(&meshes).map(|(id,mesh)| SceneObject {
                id:id.0, mesh, transform:glam::Mat4::IDENTITY, wire_color:[200,200,200],
                surfaces:scene.node_surfaces_for(*id,loom_scene::material::ShellView::Viewport),
                renderable:true,flags:ObjectFlags::default(),
            }).collect();
            renderer.sync_deforming(&objects,&dynamic);
            renderer.update();
        }
        let hits: usize = renderer.objects.values().flat_map(|o|&o.tangent_caches).map(|c|c.hits).sum();
        let misses: usize = renderer.objects.values().flat_map(|o|&o.tangent_caches).map(|c|c.misses).sum();
        eprintln!("ACTUAL_RAY_RENDERER tangent_neighbor_hits={hits} misses={misses}");
        assert!(hits > 0, "production sync must retain prepared neighbors between poses");
        assert!(misses > 0, "first pose must prepare from current geometry");
        renderer.sync_deforming(&[],&dynamic);
        assert!(renderer.objects.is_empty(), "removing objects releases their tangent caches");
    }

    #[test]
    fn specialization_ticks_drop_despawned_renderer_entities() {
        let instance = wgpu::Instance::new(&Default::default());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).expect("real GPU required");
        let (device, queue) = pollster::block_on(adapter.request_device(&device_descriptor(&adapter), None)).unwrap();
        let mut renderer = BevyRenderer::new(adapter, device, queue);
        let mut mesh = loom_mesh::box_primitive(Vec3::ONE);
        for frame in 0..128 {
            loom_mesh::edit_ops::translate_mesh(&mut mesh, Vec3::new(0.01,0.0,0.0)).unwrap();
            renderer.sync(&[SceneObject { id: 1, mesh: &mesh, transform: glam::Mat4::IDENTITY, wire_color: [220,30,30], surfaces: None, renderable: true, flags: ObjectFlags { cast_shadows: false, ..Default::default() } }]);
            let camera = ViewCamera { view: glam::Mat4::look_at_rh(Vec3::new(-4.0,-6.0,4.0), Vec3::ZERO, Vec3::Z), projection: glam::Mat4::perspective_rh(0.8,1.0,0.1,100.0) };
            renderer.set_view(0, [8,8], &camera, [0.0;4], false);
            renderer.update();
            let ticks = renderer.app.sub_app(RenderApp).world().resource::<bevy::pbr::EntitySpecializationTicks<StandardMaterial>>();
            assert_eq!(ticks.len(), 1, "despawned material entities must not accumulate in specialization ticks at frame {frame}");
            assert!(ticks.capacity() <= 3, "specialization capacity must remain bounded");
            let world = renderer.app.sub_app(RenderApp).world();
            assert_eq!(world.resource::<bevy::pbr::EntitySpecializationTicks<DynamicMaterial>>().len(), 0, "wrong-material removals must not create requests");
            assert_eq!(world.resource::<bevy::pbr::EntitySpecializationTicks<bevy::pbr::ExtendedMaterial<StandardMaterial,bake::UvBake>>>().len(), 0);
        }
        // Drain new-asset notifications so the next request is specifically
        // the live NotShadowCaster removal, not an unrelated asset event.
        renderer.update();
        renderer.update();
        assert!(renderer.app.world().resource::<bevy::pbr::EntitiesNeedingSpecialization<StandardMaterial>>().entities.is_empty());
        let entity = renderer.objects[&1].parts[0].0;
        let before = renderer.app.sub_app(RenderApp).world().resource::<bevy::pbr::EntitySpecializationTicks<StandardMaterial>>()[&bevy::render::sync_world::MainEntity::from(entity)];
        renderer.sync(&[SceneObject { id: 1, mesh: &mesh, transform: glam::Mat4::IDENTITY, wire_color: [220,30,30], surfaces: None, renderable: true, flags: ObjectFlags::default() }]);
        renderer.update();
        let ticks = renderer.app.sub_app(RenderApp).world().resource::<bevy::pbr::EntitySpecializationTicks<StandardMaterial>>();
        assert_ne!(ticks[&bevy::render::sync_world::MainEntity::from(entity)], before, "live NotShadowCaster removal must still trigger respecialization");
        assert_eq!(renderer.objects[&1].parts[0].0, entity);
    }

    #[test]
    fn rigid_pose_changes_keep_entities_meshes_and_materials() {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let Some(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else { return };
        if !supported(&adapter) { return; }
        let (device, queue) = pollster::block_on(adapter.request_device(&device_descriptor(&adapter), None)).unwrap();
        let mut renderer = BevyRenderer::new(adapter, device, queue);
        let mesh = loom_mesh::box_primitive(Vec3::ONE);
        let shear = glam::Mat4::from_cols(glam::Vec4::new(1.2, 0.1, 0.0, 0.0), glam::Vec4::new(0.4, 0.8, 0.2, 0.0), glam::Vec4::new(0.0, 0.3, 1.1, 0.0), glam::Vec4::new(1.0, -1.0, 0.5, 1.0));
        let mut previous = None;
        for transform in [glam::Mat4::IDENTITY, glam::Mat4::from_rotation_z(0.4), shear, glam::Mat4::IDENTITY] {
            renderer.sync(&[SceneObject { id: 1, mesh: &mesh, transform, wire_color: [220, 30, 30], surfaces: None, renderable: true, flags: ObjectFlags::default() }]);
            let (entity, material) = &renderer.objects[&1].parts[0];
            let handles = (*entity, renderer.app.world().get::<Mesh3d>(*entity).unwrap().0.id(), material.id());
            if let Some(previous) = previous { assert_eq!(handles, previous); }
            previous = Some(handles);
            renderer.update();
            let matrix = renderer.app.world().get::<GlobalTransform>(handles.0).unwrap().compute_matrix();
            assert!(matrix.abs_diff_eq(transform, 1e-6), "the full affine transform survives Bevy propagation");
        }
    }
}
