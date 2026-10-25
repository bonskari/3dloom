//! Realistic shading with Bevy: physically based materials, a key light with
//! shadows and ambient light. Bevy runs headless on the UI's own wgpu device
//! and renders each Realistic viewport into an image the UI draws as the
//! viewport background, and renders the active view to a PNG for Render.
//! The scene's Environment (background, global lighting, exposure control,
//! atmospheres, Depth of Field) applies to both; see [`environment`].

pub mod convert;
pub mod effects;
pub mod environment;
pub mod lights;
mod post;

use std::collections::{HashMap, HashSet};

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
    pub force_two_sided: bool,
    /// Area to Render > Selected: only these nodes.
    pub selected_only: Option<Vec<u64>>,
    /// Render Hidden Geometry: the objects given to `sync_hidden` render too.
    pub hidden: bool,
    /// Options > Atmospherics / Effects: the environment's atmospheres and
    /// Depth of Field apply to the render.
    pub atmospherics: bool,
    pub effects: bool,
    /// The rendered view's environment ranges and focus.
    pub view: ViewEnv,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self { msaa: true, shadows: true, shadow_map_size: 4096, force_two_sided: false, selected_only: None, hidden: false, atmospherics: true, effects: true, view: ViewEnv::default() }
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
    entity: Entity,
    mesh_hash: u64,
    material: Option<loom_scene::material::StandardMaterial>,
    wire_color: [u8; 3],
    material_handle: Handle<StandardMaterial>,
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
/// Slot of the camera that meters a view for Automatic / Linear exposure.
const METER_SLOT: u32 = u32::MAX - 1;
/// Metering image size, and the stops it renders under so highlights do not clip.
const METER_SIZE: u32 = 64;
const METER_STOPS: f32 = 4.0;
/// Key light: from the front right and above (world Z up), so the default
/// Perspective view (from the front left) sees the shadows fall beside objects.
const KEY_LIGHT_FROM: Vec3 = Vec3::new(0.8, -0.45, 1.0);
/// The default key light's illuminance (lux) at Global Lighting Level 1.
const KEY_LIGHT_LUX: f32 = 3000.0;

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
    two_sided: Vec<(Handle<StandardMaterial>, bool, Option<bevy::render::render_resource::Face>)>,
    /// The part of the frame the current bucket shows (u0, v0, u1, v1).
    frame: [f32; 4],
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
        app.add_plugins(post::EnvPassPlugin);
        while app.plugins_state() == PluginsState::Adding {
            bevy::tasks::tick_global_task_pools_on_main_thread();
        }
        app.finish();
        app.cleanup();
        app.insert_resource(AmbientLight { color: Color::WHITE, brightness: AMBIENT_BRIGHTNESS, affects_lightmapped_meshes: true });
        app.insert_resource(DirectionalLightShadowMap { size: 2048 });
        let light = Some(Self::spawn_key_light(&mut app));
        Self {
            app,
            device,
            queue,
            objects: HashMap::new(),
            views: HashMap::new(),
            textures: HashMap::new(),
            light,
            samples: Vec::new(),
            scene_lights: HashMap::new(),
            bounds: None,
            settle: 3,
            hidden: HashMap::new(),
            render: None,
            env: EnvSettings::default(),
            view_envs: HashMap::new(),
            env_map: None,
            volumes: Vec::new(),
            metered: HashMap::new(),
            meter: None,
            meter_next: 0,
        }
    }

    /// Default lighting: a key light with shadows.
    fn spawn_key_light(app: &mut App) -> Entity {
        app.world_mut()
            .spawn((
                DirectionalLight { illuminance: KEY_LIGHT_LUX, shadows_enabled: true, ..Default::default() },
                Transform::from_translation(KEY_LIGHT_FROM).looking_at(Vec3::ZERO, Vec3::Z),
                // The key light lights (and shadows) every layer.
                RenderLayers::from_layers(&[VIEWPORT_LAYER, RENDER_LAYER, SELECTED_LAYER, HIDDEN_LAYER, SHADOW_ONLY_LAYER]),
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
        // Global Lighting: Ambient is the ambient light's colour; Tint and
        // Level multiply every other light.
        let tint = self.env.ambient;
        let light_tint = Color::srgb(self.env.tint[0], self.env.tint[1], self.env.tint[2]).to_linear();
        let light_tint = glam::Vec3::new(light_tint.red, light_tint.green, light_tint.blue) * self.env.level.max(0.0);
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
                let b = light_tint.max_element();
                l.illuminance = KEY_LIGHT_LUX * b;
                l.color = if b > 0.0 { Color::linear_rgb(light_tint.x / b, light_tint.y / b, light_tint.z / b) } else { Color::BLACK };
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
            s.color *= light_tint;
            let mut e = world.entity_mut(entity);
            e.insert((lights::transform(&s), RenderLayers::from_layers(&[LIGHT_LAYER, LIGHT_LAYER + 1 + i])));
            match kind {
                lights::Kind::Point => {
                    e.insert(lights::point(&s, centre_distance, radius));
                }
                lights::Kind::Spot => {
                    e.insert(lights::spot(&s, centre_distance, radius));
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
            self.env_map = env.map.as_ref().and_then(|(p, _)| self.texture(p));
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
        let (tonemapping, grading, hdr) = if slot == METER_SLOT {
            (Tonemapping::None, ColorGrading { global: ColorGradingGlobal { exposure: -METER_STOPS, ..Default::default() }, ..Default::default() }, false)
        } else {
            exposure_components(env.exposure.as_ref(), metered)
        };
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
            let hash = data.content_hash();
            let material_changed = |obj: &Object| obj.material.as_ref() != o.material || obj.wire_color != o.wire_color || obj.flags.visibility != o.flags.visibility;
            let layers = self.object_layers(o.id, o.renderable, o.flags, hidden);
            match set.get(&o.id) {
                Some(obj) if obj.mesh_hash == hash && !material_changed(obj) && obj.renderable == o.renderable && obj.flags == o.flags => continue,
                Some(obj) if obj.mesh_hash == hash && !material_changed(obj) => {
                    let entity = obj.entity;
                    Self::shadow_flags(self.app.world_mut().entity_mut(entity), Self::cast_flags(o)).insert(layers);
                    let obj = set.get_mut(&o.id).expect("present");
                    obj.renderable = o.renderable;
                    obj.flags = o.flags;
                }
                Some(obj) if obj.mesh_hash == hash => {
                    let handle = obj.material_handle.clone();
                    let mat = self.material(o);
                    self.app.world_mut().resource_mut::<Assets<StandardMaterial>>().insert(&handle, mat);
                    let obj = set.get_mut(&o.id).expect("present");
                    obj.material = o.material.cloned();
                    obj.wire_color = o.wire_color;
                    obj.renderable = o.renderable;
                    obj.flags = o.flags;
                    let entity = obj.entity;
                    Self::shadow_flags(self.app.world_mut().entity_mut(entity), Self::cast_flags(o)).insert(layers);
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
                    let mut e = Self::shadow_flags(e, Self::cast_flags(o));
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
                    set.insert(o.id, Object { entity, mesh_hash: hash, material: o.material.cloned(), wire_color: o.wire_color, material_handle, renderable: o.renderable, flags: o.flags });
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

    fn material(&mut self, o: &SceneObject) -> StandardMaterial {
        let mut m = material_from_loom(o.material, o.wire_color);
        let v = o.flags.visibility.clamp(0.0, 1.0);
        if v < 1.0 {
            // Visibility fades the object like its material's opacity.
            m.base_color = m.base_color.with_alpha(m.base_color.alpha() * v);
            m.alpha_mode = bevy::render::alpha::AlphaMode::Blend;
        }
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
        // Every view sees the scene lights' layer.
        e.insert((projection, Transform::from_matrix(view.inverse()), if msaa { Msaa::Sample4 } else { Msaa::Off }, layers.with(LIGHT_LAYER)));
        self.apply_view_env(slot);
    }

    /// Starts a render with `options`: shadows, shadow map size, ambient
    /// light, Force 2-Sided and the Selected layer apply until `end_render`
    /// (the Realistic viewports see them meanwhile too).
    pub fn begin_render(&mut self, options: RenderOptions) {
        self.end_render();
        let world = self.app.world_mut();
        world.insert_resource(DirectionalLightShadowMap { size: options.shadow_map_size.clamp(512, 8192) as usize });
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
        self.render = Some(ActiveRender { options, two_sided, frame: [0.0, 0.0, 1.0, 1.0] });
        self.metered.remove(&RENDER_SLOT);
        self.meter = None;
        self.apply_lights();
        self.relayer();
        self.settle = self.settle.max(3);
    }

    /// Puts every object on its layers again (the Selected layer or a light's exclusions changed).
    fn relayer(&mut self) {
        let shown = self.objects.iter().map(|(id, o)| (o.entity, self.object_layers(*id, o.renderable, o.flags, false)));
        let hidden = self.hidden.iter().map(|(id, o)| (o.entity, self.object_layers(*id, o.renderable, o.flags, true)));
        let list: Vec<(Entity, RenderLayers)> = shown.chain(hidden).collect();
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
        let msaa = o.msaa;
        if let Some(r) = self.render.as_mut() {
            r.frame = sub;
        }
        // Cleared to transparent black: the environment pass draws the background.
        self.place_view(RENDER_SLOT, size, Projection::custom(BucketProjection::new(camera, aspect, sub)), camera.view, [0.0; 4], msaa, layers.clone());
        // Automatic / Linear exposure meters the whole frame once per render.
        if self.env.exposure.is_some_and(|x| x.meters()) && !self.metered.contains_key(&RENDER_SLOT) && self.meter.is_none() {
            self.place_meter(RENDER_SLOT, camera.bevy_projection(aspect), camera.view, layers);
        }
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
        self.metered.remove(&RENDER_SLOT);
        let Some(r) = self.render.take() else { return };
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
        self.read_meter();
        if !self.views.iter().any(|(s, v)| v.used && *s >= self.meter_next && *s < METER_SLOT) {
            self.meter_next = 0;
        }
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
