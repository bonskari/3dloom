//! Standard lights (Create > Lights > Standard): Omni, Target / Free Spot,
//! Target / Free Direct and Skylight. A light node is only a proxy: its
//! transform, parameters and viewport icon live here, and each renderer
//! (the shaded viewport, Bevy) builds its own light from [`scene_lights`],
//! the renderer-independent evaluation of every light that is on.
//!
//! A spot or directional light shines down its local -Z, like a camera.
//! Colours are stored as the swatch shows them (sRGB, 0..1).

use crate::node::{Node, NodeId, ObjectKind, ParamValue};
use crate::param::Param;
use crate::scene::Scene;
use glam::{Mat4, Vec3};
use loom_mesh::Mesh;
use serde::{Deserialize, Serialize};

/// Light Type list of the General Parameters rollout (Skylight has none).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LightType {
    #[default]
    Omni,
    Spot,
    Directional,
    Skylight,
}

impl LightType {
    /// The names the Light Type list shows, in order.
    pub const LIST: [LightType; 3] = [LightType::Omni, LightType::Spot, LightType::Directional];

    pub fn label(self) -> &'static str {
        match self {
            LightType::Omni => "Omni",
            LightType::Spot => "Spot",
            LightType::Directional => "Directional",
            LightType::Skylight => "Skylight",
        }
    }

    /// Spot and directional lights aim, so they can have a target.
    pub fn aims(self) -> bool {
        matches!(self, LightType::Spot | LightType::Directional)
    }
}

/// Exclude/Include dialog: which objects a light leaves out (or the only
/// ones it affects, with `include`), for illumination, shadow casting or both.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Exclusion {
    pub include: bool,
    /// 0 Illumination, 1 Shadow Casting, 2 Both.
    pub scope: u32,
    pub nodes: Vec<NodeId>,
}

impl Exclusion {
    pub const SCOPES: [&'static str; 3] = ["Illumination", "Shadow Casting", "Both"];

    /// Whether the light leaves `id` out of `illumination` (else shadow casting).
    pub fn excludes(&self, id: NodeId, illumination: bool) -> bool {
        let applies = match self.scope {
            0 => illumination,
            1 => !illumination,
            _ => true,
        };
        if !applies {
            return false;
        }
        self.nodes.contains(&id) != self.include
    }
}

/// Default (sRGB) sky colour of a new Skylight.
pub const DEFAULT_SKY_COLOR: [f32; 3] = [0.459, 0.549, 0.698];
/// Hotspot / Falloff of a new spot (degrees) or directional light (units).
pub const DEFAULT_HOTSPOT: f32 = 43.0;
pub const DEFAULT_FALLOFF: f32 = 45.0;
/// Falloff stays at least this much wider than the hotspot.
pub const HOTSPOT_GAP: f32 = 2.0;
/// The widest spot cone (degrees).
pub const MAX_SPOT_ANGLE: f32 = 179.5;
/// Distance to the invisible target of a new free light.
pub const DEFAULT_TARGET_DISTANCE: f32 = 240.0;
/// Shadow types of the General Parameters list.
pub const SHADOW_TYPES: [&str; 4] = ["Shadow Map", "Ray Traced Shadows", "Adv. Ray Traced", "Area Shadows"];
pub const SHADOW_MAP: u32 = 0;
pub const RAY_TRACED: u32 = 1;
pub const ADV_RAY_TRACED: u32 = 2;
pub const AREA_SHADOWS: u32 = 3;
/// Area Shadows > Basic Options > Mode.
pub const AREA_MODES: [&str; 5] = ["Simple", "Rectangle Light", "Disc Light", "Box Light", "Sphere Light"];
/// Advanced Ray-Traced Params > Basic Options > Mode.
pub const ADV_RT_MODES: [&str; 3] = ["Simple", "1-Pass Antialias", "2-Pass Antialias"];
pub const DECAY_TYPES: [&str; 3] = ["None", "Inverse", "Inverse Square"];

fn p<T: crate::param::Animatable>(v: T) -> Param<T> {
    Param::new(v)
}

/// A standard light's parameters, by rollout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LightObject {
    pub light_type: LightType,
    /// Target light: the node it aims at (free light: None).
    pub target: Option<NodeId>,
    /// Free light: distance to its invisible target (Targ. Dist.).
    pub target_distance: Param<f32>,
    // General Parameters
    pub on: Param<bool>,
    pub shadows_on: Param<bool>,
    /// Index into [`SHADOW_TYPES`].
    pub shadow_type: Param<u32>,
    pub exclusion: Exclusion,
    // Intensity/Color/Attenuation (Skylight: Multiplier and Sky Color)
    pub multiplier: Param<f32>,
    pub color: Param<Vec3>,
    /// Index into [`DECAY_TYPES`].
    pub decay: Param<u32>,
    pub decay_start: Param<f32>,
    pub near_use: Param<bool>,
    pub near_show: Param<bool>,
    pub near_start: Param<f32>,
    pub near_end: Param<f32>,
    pub far_use: Param<bool>,
    pub far_show: Param<bool>,
    pub far_start: Param<f32>,
    pub far_end: Param<f32>,
    // Spotlight / Directional Parameters
    pub show_cone: Param<bool>,
    pub overshoot: Param<bool>,
    pub hotspot: Param<f32>,
    pub falloff: Param<f32>,
    /// Circle (false) / Rectangle (true).
    pub rectangle: Param<bool>,
    pub aspect: Param<f32>,
    // Advanced Effects
    pub contrast: Param<f32>,
    pub soften_diffuse: Param<f32>,
    pub affect_diffuse: Param<bool>,
    pub affect_specular: Param<bool>,
    pub ambient_only: Param<bool>,
    pub projector: Param<bool>,
    pub projector_map: Option<String>,
    // Shadow Parameters / Shadow Map Params
    pub shadow_color: Param<Vec3>,
    pub shadow_density: Param<f32>,
    pub map_bias: Param<f32>,
    pub map_size: Param<u32>,
    pub sample_range: Param<f32>,
    // Skylight
    pub use_scene_environment: Param<bool>,
    pub rays_per_sample: Param<u32>,
    pub ray_bias: Param<f32>,
    // Renderer (Bevy) light properties without a standard-light equivalent
    /// Size of the emitting sphere: highlight size and shadow softness.
    pub source_radius: Param<f32>,
    pub shadow_normal_bias: Param<f32>,
    // Shadow Parameters (rest)
    /// Blend the light's colour into the shadow colour.
    pub light_affects_shadow_color: Param<bool>,
    /// Shadow Parameters > Map: the bitmap the shadow colour comes from (kept; no renderer reads it).
    pub shadow_map_on: Param<bool>,
    pub shadow_map_path: Option<String>,
    pub atmosphere_shadows: Param<bool>,
    pub atmosphere_opacity: Param<f32>,
    pub atmosphere_color_amount: Param<f32>,
    // Shadow Map Params (rest)
    /// Bias in scene units (else normalised to the scene's extents).
    pub absolute_bias: Param<bool>,
    pub two_sided_shadows: Param<bool>,
    // Ray-Traced Shadow Params
    pub max_quadtree_depth: Param<u32>,
    // Advanced Ray-Traced / Area Shadows
    /// Index into [`ADV_RT_MODES`].
    pub adv_rt_mode: Param<u32>,
    /// Index into [`AREA_MODES`].
    pub area_mode: Param<u32>,
    pub area_length: Param<f32>,
    pub area_width: Param<f32>,
    pub area_height: Param<f32>,
    pub shadow_integrity: Param<u32>,
    pub shadow_quality: Param<u32>,
    pub shadow_spread: Param<f32>,
    pub area_bias: Param<f32>,
    pub jitter: Param<f32>,
    /// Create > Lights > Photometric (Target / Free Light): physical intensity,
    /// colour temperature and distribution. `light_type` then follows the
    /// distribution (Spot for Spotlight, Omni otherwise).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub photometric: Option<Photometric>,
    /// The sun of a Sunlight / Daylight system: placed around its parent
    /// compass by date, time and location.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sun: Option<crate::sun::Sunlight>,
}

impl Default for LightObject {
    fn default() -> Self {
        Self::new(LightType::Omni)
    }
}

impl LightObject {
    pub fn new(light_type: LightType) -> Self {
        let sky = light_type == LightType::Skylight;
        Self {
            light_type,
            target: None,
            target_distance: p(DEFAULT_TARGET_DISTANCE),
            on: p(true),
            // Skylight: Cast Shadows defaults to off.
            shadows_on: p(!sky),
            shadow_type: p(0),
            exclusion: Exclusion::default(),
            multiplier: p(1.0),
            color: p(if sky { Vec3::from(DEFAULT_SKY_COLOR) } else { Vec3::ONE }),
            decay: p(0),
            decay_start: p(40.0),
            near_use: p(false),
            near_show: p(false),
            near_start: p(0.0),
            near_end: p(40.0),
            far_use: p(false),
            far_show: p(false),
            far_start: p(80.0),
            far_end: p(200.0),
            show_cone: p(false),
            overshoot: p(false),
            hotspot: p(DEFAULT_HOTSPOT),
            falloff: p(DEFAULT_FALLOFF),
            rectangle: p(false),
            aspect: p(1.0),
            contrast: p(0.0),
            soften_diffuse: p(50.0),
            affect_diffuse: p(true),
            affect_specular: p(true),
            ambient_only: p(false),
            projector: p(false),
            projector_map: None,
            shadow_color: p(Vec3::ZERO),
            shadow_density: p(1.0),
            map_bias: p(1.0),
            map_size: p(512),
            sample_range: p(4.0),
            use_scene_environment: p(false),
            rays_per_sample: p(20),
            ray_bias: p(0.005),
            source_radius: p(0.0),
            shadow_normal_bias: p(1.8),
            light_affects_shadow_color: p(false),
            shadow_map_on: p(false),
            shadow_map_path: None,
            atmosphere_shadows: p(false),
            atmosphere_opacity: p(100.0),
            atmosphere_color_amount: p(100.0),
            absolute_bias: p(false),
            two_sided_shadows: p(true),
            max_quadtree_depth: p(7),
            adv_rt_mode: p(2),
            area_mode: p(1),
            area_length: p(10.0),
            area_width: p(10.0),
            area_height: p(10.0),
            shadow_integrity: p(2),
            shadow_quality: p(5),
            shadow_spread: p(1.0),
            area_bias: p(0.5),
            jitter: p(1.0),
            photometric: None,
            sun: None,
        }
    }

    /// A photometric light (Target Light / Free Light) with the reference's
    /// defaults: Uniform Spherical, D65 white, 1500 cd, shadows off;
    /// `unit_metres` is the scene's system unit in metres.
    pub fn photometric(unit_metres: f32) -> Self {
        let mut l = Self::new(LightType::Omni);
        l.shadows_on.set(false);
        l.hotspot.set(30.0);
        l.falloff.set(60.0);
        l.photometric = Some(Photometric::new(unit_metres));
        l
    }

    /// Spot and directional lights, and every photometric light, aim at a
    /// target (a photometric light's distribution can change later).
    pub fn aims(&self) -> bool {
        self.light_type.aims() || self.photometric.is_some()
    }

    /// Base name of a new light ("Omni001", "Spot001", "FSpot001", ...).
    pub fn base_name(&self) -> &'static str {
        if self.photometric.is_some() {
            return if self.target.is_some() { "TPhotometricLight" } else { "PhotometricLight" };
        }
        if self.sun.is_some() {
            return "Sun";
        }
        match (self.light_type, self.target.is_some()) {
            (LightType::Omni, _) => "Omni",
            (LightType::Spot, true) => "Spot",
            (LightType::Spot, false) => "FSpot",
            (LightType::Directional, true) => "Direct",
            (LightType::Directional, false) => "FDirect",
            (LightType::Skylight, _) => "Sky",
        }
    }

    /// The Modify panel's stack name ("Target Spotlight", "Omni", ...).
    pub fn stack_name(&self) -> &'static str {
        if self.photometric.is_some() {
            return if self.target.is_some() { "Target Light" } else { "Free Light" };
        }
        match (self.light_type, self.target.is_some()) {
            (LightType::Omni, _) => "Omni",
            (LightType::Spot, true) => "Target Spotlight",
            (LightType::Spot, false) => "Free Spot",
            (LightType::Directional, true) => "Target Directional",
            (LightType::Directional, false) => "Free Directional",
            (LightType::Skylight, _) => "Skylight",
        }
    }

    /// The rollout parameters by name (colours as three 0..1 channels).
    pub fn parameters(&self) -> Vec<(&'static str, ParamValue)> {
        use ParamValue::*;
        let f = |x: &Param<f32>| Float(x.get());
        let i = |x: &Param<u32>| Int(x.get());
        let b = |x: &Param<bool>| Bool(x.get());
        let c = self.color.get();
        if self.light_type == LightType::Skylight {
            return vec![
                ("On", b(&self.on)), ("Multiplier", f(&self.multiplier)),
                ("Color R", Float(c.x)), ("Color G", Float(c.y)), ("Color B", Float(c.z)),
                ("Use Scene Environment", b(&self.use_scene_environment)),
                ("Cast Shadows", b(&self.shadows_on)), ("Rays per Sample", i(&self.rays_per_sample)), ("Ray Bias", f(&self.ray_bias)),
            ];
        }
        if let Some(ph) = &self.photometric {
            return self.photometric_parameters(ph);
        }
        let s = self.shadow_color.get();
        let mut v = vec![
            ("On", b(&self.on)), ("Shadows On", b(&self.shadows_on)), ("Shadow Type", i(&self.shadow_type)),
            ("Multiplier", f(&self.multiplier)), ("Color R", Float(c.x)), ("Color G", Float(c.y)), ("Color B", Float(c.z)),
            ("Decay Type", i(&self.decay)), ("Decay Start", f(&self.decay_start)),
            ("Near Use", b(&self.near_use)), ("Near Show", b(&self.near_show)), ("Near Start", f(&self.near_start)), ("Near End", f(&self.near_end)),
            ("Far Use", b(&self.far_use)), ("Far Show", b(&self.far_show)), ("Far Start", f(&self.far_start)), ("Far End", f(&self.far_end)),
        ];
        if self.light_type.aims() {
            v.extend([
                ("Show Cone", b(&self.show_cone)), ("Overshoot", b(&self.overshoot)), ("Hotspot", f(&self.hotspot)), ("Falloff", f(&self.falloff)),
                ("Rectangle", b(&self.rectangle)), ("Aspect", f(&self.aspect)),
            ]);
        }
        v.extend([
            ("Contrast", f(&self.contrast)), ("Soften Diffuse Edge", f(&self.soften_diffuse)),
            ("Affect Diffuse", b(&self.affect_diffuse)), ("Affect Specular", b(&self.affect_specular)), ("Ambient Only", b(&self.ambient_only)),
            ("Projector", b(&self.projector)),
            ("Shadow Color R", Float(s.x)), ("Shadow Color G", Float(s.y)), ("Shadow Color B", Float(s.z)), ("Shadow Density", f(&self.shadow_density)),
            ("Map Bias", f(&self.map_bias)), ("Map Size", i(&self.map_size)), ("Sample Range", f(&self.sample_range)),
            ("Source Radius", f(&self.source_radius)), ("Shadow Normal Bias", f(&self.shadow_normal_bias)),
            ("Target Distance", f(&self.target_distance)),
        ]);
        v.extend(self.shadow_parameters());
        v
    }

    /// The shadow rollouts' parameters every shadow-casting light shares.
    fn shadow_parameters(&self) -> Vec<(&'static str, ParamValue)> {
        use ParamValue::*;
        let f = |x: &Param<f32>| Float(x.get());
        let i = |x: &Param<u32>| Int(x.get());
        let b = |x: &Param<bool>| Bool(x.get());
        vec![
            ("Light Affects Shadow Color", b(&self.light_affects_shadow_color)), ("Shadow Map On", b(&self.shadow_map_on)),
            ("Atmosphere Shadows", b(&self.atmosphere_shadows)), ("Atmosphere Opacity", f(&self.atmosphere_opacity)), ("Atmosphere Color Amount", f(&self.atmosphere_color_amount)),
            ("Absolute Map Bias", b(&self.absolute_bias)), ("2-Sided Shadows", b(&self.two_sided_shadows)), ("Max Quadtree Depth", i(&self.max_quadtree_depth)),
            ("Adv RT Mode", i(&self.adv_rt_mode)), ("Area Mode", i(&self.area_mode)),
            ("Area Length", f(&self.area_length)), ("Area Width", f(&self.area_width)), ("Area Height", f(&self.area_height)),
            ("Shadow Integrity", i(&self.shadow_integrity)), ("Shadow Quality", i(&self.shadow_quality)), ("Shadow Spread", f(&self.shadow_spread)),
            ("Shadow Bias", f(&self.area_bias)), ("Jitter Amount", f(&self.jitter)),
        ]
    }

    /// Area Shadows: the radius of a sphere the size of the area light
    /// (the renderers' soft-shadow source); 0 for Simple and the other shadow types.
    pub fn area_radius(&self) -> f32 {
        if self.shadow_type.get() != AREA_SHADOWS {
            return 0.0;
        }
        let (l, w, h) = (self.area_length.get().max(0.0), self.area_width.get().max(0.0), self.area_height.get().max(0.0));
        match self.area_mode.get() {
            1 => (l * w / std::f32::consts::PI).sqrt(),
            2 | 4 => l * 0.5,
            3 => (3.0 * l * w * h / (4.0 * std::f32::consts::PI)).cbrt(),
            _ => 0.0,
        }
    }

    /// What a shadow lets through of the light, per channel: 1 - density ×
    /// (1 - shadow colour), the colour times the light's with Light Affects
    /// Shadow Color (density 1 and black: nothing). Linear.
    pub fn shadow_transmission(&self) -> Vec3 {
        let mut c = srgb_to_linear(self.shadow_color.get());
        if self.light_affects_shadow_color.get() {
            c *= srgb_to_linear(self.color.get());
        }
        let d = self.shadow_density.get().clamp(0.0, 1.0);
        (Vec3::ONE - (Vec3::ONE - c) * d).clamp(Vec3::ZERO, Vec3::ONE)
    }

    /// Sets a parameter by its rollout name, keeping the reference's limits
    /// (Falloff stays at least 2 wider than Hotspot, and so on).
    pub fn set_parameter(&mut self, name: &str, value: f64) -> Result<(), String> {
        let v = value as f32;
        let b = value != 0.0;
        let u = value.round().max(0.0) as u32;
        let channel = |c: &mut Param<Vec3>, i: usize| {
            let mut x = c.get();
            x[i] = v.clamp(0.0, 1.0);
            c.set(x);
        };
        let cone_max = if self.light_type == LightType::Spot { MAX_SPOT_ANGLE } else { 1e6 };
        if self.photometric.is_some() && self.set_photometric(name, value) {
            return Ok(());
        }
        match name {
            "On" => self.on.set(b),
            "Shadows On" | "Cast Shadows" => self.shadows_on.set(b),
            "Shadow Type" => self.shadow_type.set(u.min(SHADOW_TYPES.len() as u32 - 1)),
            "Multiplier" => self.multiplier.set(v),
            "Color R" => channel(&mut self.color, 0),
            "Color G" => channel(&mut self.color, 1),
            "Color B" => channel(&mut self.color, 2),
            "Decay Type" => self.decay.set(u.min(2)),
            "Decay Start" => self.decay_start.set(v.max(0.0)),
            "Near Use" => self.near_use.set(b),
            "Near Show" => self.near_show.set(b),
            "Near Start" => self.near_start.set(v.max(0.0)),
            "Near End" => self.near_end.set(v.max(0.0)),
            "Far Use" => self.far_use.set(b),
            "Far Show" => self.far_show.set(b),
            "Far Start" => self.far_start.set(v.max(0.0)),
            "Far End" => self.far_end.set(v.max(0.0)),
            "Show Cone" => self.show_cone.set(b),
            "Overshoot" => self.overshoot.set(b),
            "Hotspot" => {
                let h = v.clamp(0.5, cone_max - HOTSPOT_GAP);
                self.hotspot.set(h);
                if self.falloff.get() < h + HOTSPOT_GAP {
                    self.falloff.set(h + HOTSPOT_GAP);
                }
            }
            "Falloff" => {
                let f = v.clamp(0.5 + HOTSPOT_GAP, cone_max);
                self.falloff.set(f);
                if self.hotspot.get() > f - HOTSPOT_GAP {
                    self.hotspot.set(f - HOTSPOT_GAP);
                }
            }
            "Rectangle" => self.rectangle.set(b),
            "Aspect" => self.aspect.set(v.clamp(0.001, 100.0)),
            "Contrast" => self.contrast.set(v.clamp(0.0, 100.0)),
            "Soften Diffuse Edge" => self.soften_diffuse.set(v.clamp(0.0, 100.0)),
            "Affect Diffuse" => self.affect_diffuse.set(b),
            "Affect Specular" => self.affect_specular.set(b),
            "Ambient Only" => self.ambient_only.set(b),
            "Projector" => self.projector.set(b),
            "Shadow Color R" => channel(&mut self.shadow_color, 0),
            "Shadow Color G" => channel(&mut self.shadow_color, 1),
            "Shadow Color B" => channel(&mut self.shadow_color, 2),
            "Shadow Density" => self.shadow_density.set(v),
            "Map Bias" => self.map_bias.set(v.max(0.0)),
            "Map Size" => self.map_size.set(u.clamp(1, 10_000)),
            "Sample Range" => self.sample_range.set(v.clamp(0.01, 50.0)),
            "Use Scene Environment" => self.use_scene_environment.set(b),
            "Rays per Sample" => self.rays_per_sample.set(u.clamp(1, 32)),
            "Ray Bias" => self.ray_bias.set(v.max(0.0)),
            "Source Radius" => self.source_radius.set(v.max(0.0)),
            "Shadow Normal Bias" => self.shadow_normal_bias.set(v.max(0.0)),
            "Target Distance" => self.target_distance.set(v.max(0.001)),
            "Light Affects Shadow Color" => self.light_affects_shadow_color.set(b),
            "Shadow Map On" => self.shadow_map_on.set(b),
            "Atmosphere Shadows" => self.atmosphere_shadows.set(b),
            "Atmosphere Opacity" => self.atmosphere_opacity.set(v.clamp(0.0, 100.0)),
            "Atmosphere Color Amount" => self.atmosphere_color_amount.set(v.clamp(0.0, 100.0)),
            "Absolute Map Bias" => self.absolute_bias.set(b),
            "2-Sided Shadows" => self.two_sided_shadows.set(b),
            "Max Quadtree Depth" => self.max_quadtree_depth.set(u.clamp(1, 10)),
            "Adv RT Mode" => self.adv_rt_mode.set(u.min(2)),
            "Area Mode" => self.area_mode.set(u.min(4)),
            "Area Length" => self.area_length.set(v.max(0.0)),
            "Area Width" => self.area_width.set(v.max(0.0)),
            "Area Height" => self.area_height.set(v.max(0.0)),
            "Shadow Integrity" => self.shadow_integrity.set(u.clamp(1, 15)),
            "Shadow Quality" => self.shadow_quality.set(u.clamp(1, 15)),
            "Shadow Spread" => self.shadow_spread.set(v.max(0.0)),
            "Shadow Bias" => self.area_bias.set(v.max(0.0)),
            "Jitter Amount" => self.jitter.set(v.max(0.0)),
            _ => return Err(format!("{} has no parameter `{name}`", self.stack_name())),
        }
        Ok(())
    }

    pub fn choices(&self, name: &str) -> Option<&'static [&'static str]> {
        match name {
            "Shadow Type" => Some(&SHADOW_TYPES),
            "Decay Type" => Some(&DECAY_TYPES),
            "Distribution" => Some(&DISTRIBUTIONS),
            "Area Mode" => Some(&AREA_MODES),
            "Adv RT Mode" => Some(&ADV_RT_MODES),
            "Lamp" => Some(&LAMP_NAMES),
            "Intensity Unit" => Some(&INTENSITY_UNITS),
            "Light Shape" => Some(&LIGHT_SHAPES),
            _ => None,
        }
    }

    /// The viewport icon in the light's frame (it shines down local -Z).
    pub fn icon_mesh(&self) -> Mesh {
        if let Some(ph) = &self.photometric {
            return match ph.distribution.get() {
                DIST_DIFFUSE => diffuse_icon(),
                DIST_SPOT => spot_icon(),
                _ => bulb_icon(),
            };
        }
        match self.light_type {
            LightType::Omni => omni_icon(),
            LightType::Spot => spot_icon(),
            LightType::Directional => direct_icon(),
            LightType::Skylight => sky_icon(),
        }
    }
}

/// Distribution list of a photometric light's General Parameters.
pub const DISTRIBUTIONS: [&str; 4] = ["Uniform Spherical", "Uniform Diffuse", "Spotlight", "Photometric Web"];
pub const DIST_SPHERICAL: u32 = 0;
pub const DIST_DIFFUSE: u32 = 1;
pub const DIST_SPOT: u32 = 2;
pub const DIST_WEB: u32 = 3;
/// Uniform Diffuse lights the hemisphere in front of the light, fading
/// towards its edge: drawn as a spot this wide with no hotspot.
pub const DIFFUSE_FALLOFF: f32 = 178.0;
/// Intensity units: lumen, candela, lux at a distance.
pub const INTENSITY_UNITS: [&str; 3] = ["lm", "cd", "lx"];
/// Shape/Area Shadows emitter shapes.
pub const LIGHT_SHAPES: [&str; 6] = ["Point", "Line", "Rectangle", "Disc", "Sphere", "Cylinder"];
/// The Color group's lamp list, each with the correlated colour temperature
/// (kelvin) it is drawn with here: the CIE illuminants' own temperatures and
/// typical catalogue temperatures of the lamp types.
pub const LAMPS: [(&str, f32); 21] = [
    ("D50 Illuminant (Reference White)", 5003.0),
    ("D65 Illuminant (Reference White)", 6504.0),
    ("Fluorescent (Cool White)", 4100.0),
    ("Fluorescent (Daylight)", 6500.0),
    ("Fluorescent (Lite White)", 4100.0),
    ("Fluorescent (Warm White)", 3000.0),
    ("Fluorescent (White)", 3500.0),
    ("Halogen", 3000.0),
    ("Halogen (Cool)", 3200.0),
    ("Halogen (Warm)", 2800.0),
    ("HID Ceramic Metal Halide (Cool)", 4200.0),
    ("HID Ceramic Metal Halide (Warm)", 3000.0),
    ("HID High Pressure Sodium", 2100.0),
    ("HID Low Pressure Sodium", 1800.0),
    ("HID Mercury", 5700.0),
    ("HID Phosphor Mercury", 4000.0),
    ("HID Quartz Metal Halide", 4000.0),
    ("HID Quartz Metal Halide (Cool)", 4300.0),
    ("HID Quartz Metal Halide (Warm)", 3200.0),
    ("HID Xenon", 6000.0),
    ("Incandescent filament lamp", 2800.0),
];
pub const LAMP_NAMES: [&str; 21] = {
    let mut n = [""; 21];
    let mut i = 0;
    while i < 21 {
        n[i] = LAMPS[i].0;
        i += 1;
    }
    n
};
/// Index of D65 in [`LAMPS`], the default.
pub const LAMP_D65: u32 = 1;
/// Illuminance (lux) a photometric light must give to light a white surface
/// it faces as a standard light of multiplier 1 does: the default 1500 cd
/// lamp seen from 2 m (the viewports and the Bevy view have no exposure
/// control for photometric light, so this fixes the brightness scale).
pub const PHOTOMETRIC_LUX_PER_MULTIPLIER: f32 = 375.0;

/// Templates rollout: (name, distribution, candela, lamp, beam, field).
/// Bulbs are Uniform Spherical at the luminous intensity of common
/// incandescent A-19 lamps; the halogen spot is the reference's M16 50 W
/// medium beam. Web templates need their IES files and are not listed.
pub const TEMPLATES: [(&str, u32, f32, u32, f32, f32); 5] = [
    ("40W Bulb", DIST_SPHERICAL, 36.0, 20, 30.0, 60.0),
    ("60W Bulb", DIST_SPHERICAL, 70.0, 20, 30.0, 60.0),
    ("75W Bulb", DIST_SPHERICAL, 95.0, 20, 30.0, 60.0),
    ("100W Bulb", DIST_SPHERICAL, 139.0, 20, 30.0, 60.0),
    ("Halogen Spotlight", DIST_SPOT, 3000.0, 7, 25.0, 50.0),
];

/// sRGB colour (0..1, brightest channel 1) of black-body light at `kelvin`
/// (a fit of the Planckian locus valid from 1000 K to 40000 K).
pub fn kelvin_to_srgb(kelvin: f32) -> Vec3 {
    let t = kelvin.clamp(1000.0, 40000.0) / 100.0;
    let r = if t <= 66.0 { 255.0 } else { 329.698_73 * (t - 60.0).powf(-0.133_204_76) };
    let g = if t <= 66.0 { 99.470_8 * t.ln() - 161.119_57 } else { 288.122_17 * (t - 60.0).powf(-0.075_514_85) };
    let b = if t >= 66.0 {
        255.0
    } else if t <= 19.0 {
        0.0
    } else {
        138.517_73 * (t - 10.0).ln() - 305.044_8
    };
    let c = Vec3::new(r, g, b).clamp(Vec3::ZERO, Vec3::splat(255.0)) / 255.0;
    c / c.max_element().max(1e-6)
}

/// A photometric light's own parameters (Intensity/Color/Attenuation,
/// distribution, Shape/Area Shadows). Intensity is kept in candela.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Photometric {
    /// Index into [`DISTRIBUTIONS`].
    pub distribution: Param<u32>,
    /// Photometric Web: the IES file (drawn as Uniform Spherical).
    pub web_file: Option<String>,
    /// Kelvin (true) or the lamp list (false).
    pub use_kelvin: Param<bool>,
    /// Index into [`LAMPS`].
    pub lamp: Param<u32>,
    pub kelvin: Param<f32>,
    pub filter_color: Param<Vec3>,
    pub candela: Param<f32>,
    /// Index into [`INTENSITY_UNITS`]: the unit the rollout shows.
    pub intensity_unit: Param<u32>,
    /// "lx at": the distance (scene units) the illuminance is given for.
    pub lux_distance: Param<f32>,
    pub dimming_on: Param<bool>,
    /// Percent of the full intensity.
    pub dimming: Param<f32>,
    pub incandescent_shift: Param<bool>,
    /// Index into [`LIGHT_SHAPES`].
    pub shape: Param<u32>,
    pub shape_length: Param<f32>,
    pub shape_width: Param<f32>,
    pub shape_radius: Param<f32>,
    pub render_shape: Param<bool>,
    /// The scene's system unit in metres when the light was made (intensity
    /// falls off with the square of the distance in metres).
    pub unit_metres: f32,
}

impl Default for Photometric {
    fn default() -> Self {
        Self::new(0.0254)
    }
}

impl Photometric {
    pub fn new(unit_metres: f32) -> Self {
        let m = 1.0 / unit_metres.max(1e-9);
        Self {
            distribution: p(DIST_SPHERICAL),
            web_file: None,
            use_kelvin: p(false),
            lamp: p(LAMP_D65),
            kelvin: p(3600.0),
            filter_color: p(Vec3::ONE),
            candela: p(1500.0),
            intensity_unit: p(1),
            lux_distance: p(m),
            dimming_on: p(false),
            dimming: p(100.0),
            incandescent_shift: p(false),
            shape: p(0),
            shape_length: p(1.2 * m),
            shape_width: p(0.6 * m),
            shape_radius: p(0.1 * m),
            render_shape: p(false),
            unit_metres: unit_metres.max(1e-9),
        }
    }

    /// Solid angle (sr) the light fills: lumens = candela × this.
    pub fn solid_angle(&self, falloff_deg: f32) -> f32 {
        use std::f32::consts::PI;
        match self.distribution.get() {
            DIST_DIFFUSE => PI,
            DIST_SPOT => 2.0 * PI * (1.0 - (falloff_deg.min(MAX_SPOT_ANGLE) * 0.5).to_radians().cos()),
            _ => 4.0 * PI,
        }
    }

    /// `cd` in `unit` (0 lm, 1 cd, 2 lx at Lux Distance).
    pub fn from_candela(&self, cd: f32, unit: u32, falloff_deg: f32) -> f32 {
        match unit {
            0 => cd * self.solid_angle(falloff_deg),
            2 => {
                let d = self.lux_distance.get() * self.unit_metres;
                cd / (d * d).max(1e-9)
            }
            _ => cd,
        }
    }

    /// A value in `unit` as candela.
    pub fn to_candela(&self, v: f32, unit: u32, falloff_deg: f32) -> f32 {
        let one = self.from_candela(1.0, unit, falloff_deg);
        v / one.max(1e-9)
    }

    /// Dimming's fraction of the full intensity.
    pub fn dim(&self) -> f32 {
        if self.dimming_on.get() { (self.dimming.get() / 100.0).max(0.0) } else { 1.0 }
    }

    /// The colour temperature drawn: Kelvin or the lamp's, lowered as an
    /// incandescent lamp's is when Incandescent Lamp Color Shift dims it
    /// (temperature ∝ light output^⅛).
    pub fn temperature(&self) -> f32 {
        let k = if self.use_kelvin.get() { self.kelvin.get() } else { LAMPS[(self.lamp.get() as usize).min(LAMPS.len() - 1)].1 };
        if self.incandescent_shift.get() && self.dimming_on.get() {
            k * self.dim().clamp(1e-4, 1.0).powf(0.125)
        } else {
            k
        }
    }

    /// Linear colour: the temperature's colour through the filter.
    pub fn linear_color(&self) -> Vec3 {
        srgb_to_linear(kelvin_to_srgb(self.temperature())) * srgb_to_linear(self.filter_color.get())
    }

    /// Radius of a sphere with the emitter's size (the renderers' soft
    /// shadow source): half a line's length, the radius of a rectangle's
    /// equal-area disc, a disc / sphere / cylinder's radius.
    pub fn source_radius(&self) -> f32 {
        match self.shape.get() {
            1 => self.shape_length.get() * 0.5,
            2 => (self.shape_length.get() * self.shape_width.get() / std::f32::consts::PI).sqrt(),
            3..=5 => self.shape_radius.get(),
            _ => 0.0,
        }
    }
}

impl LightObject {
    fn photometric_parameters(&self, ph: &Photometric) -> Vec<(&'static str, ParamValue)> {
        use ParamValue::*;
        let f = |x: &Param<f32>| Float(x.get());
        let i = |x: &Param<u32>| Int(x.get());
        let b = |x: &Param<bool>| Bool(x.get());
        let c = ph.filter_color.get();
        let mut v = vec![
            ("On", b(&self.on)), ("Shadows On", b(&self.shadows_on)), ("Shadow Type", i(&self.shadow_type)),
            ("Distribution", i(&ph.distribution)),
            ("Use Kelvin", b(&ph.use_kelvin)), ("Lamp", i(&ph.lamp)), ("Kelvin", f(&ph.kelvin)),
            ("Filter Color R", Float(c.x)), ("Filter Color G", Float(c.y)), ("Filter Color B", Float(c.z)),
            ("Intensity Unit", i(&ph.intensity_unit)),
            ("Intensity", Float(ph.from_candela(ph.candela.get(), ph.intensity_unit.get(), self.falloff.get()))),
            ("Lux Distance", f(&ph.lux_distance)),
            ("Dimming On", b(&ph.dimming_on)), ("Dimming", f(&ph.dimming)), ("Incandescent Shift", b(&ph.incandescent_shift)),
            ("Far Use", b(&self.far_use)), ("Far Show", b(&self.far_show)), ("Far Start", f(&self.far_start)), ("Far End", f(&self.far_end)),
        ];
        if ph.distribution.get() == DIST_SPOT {
            v.extend([("Hotspot", f(&self.hotspot)), ("Falloff", f(&self.falloff))]);
        }
        v.extend([
            ("Light Shape", i(&ph.shape)), ("Shape Length", f(&ph.shape_length)), ("Shape Width", f(&ph.shape_width)), ("Shape Radius", f(&ph.shape_radius)),
            ("Render Shape", b(&ph.render_shape)),
            ("Map Bias", f(&self.map_bias)), ("Map Size", i(&self.map_size)), ("Sample Range", f(&self.sample_range)),
            ("Shadow Normal Bias", f(&self.shadow_normal_bias)), ("Target Distance", f(&self.target_distance)),
            ("Shadow Color R", Float(self.shadow_color.get().x)), ("Shadow Color G", Float(self.shadow_color.get().y)), ("Shadow Color B", Float(self.shadow_color.get().z)),
            ("Shadow Density", f(&self.shadow_density)),
        ]);
        v.extend(self.shadow_parameters());
        v
    }

    /// A photometric parameter by name; false when `name` is a parameter
    /// photometric and standard lights share (On, Far Use, Hotspot...).
    fn set_photometric(&mut self, name: &str, value: f64) -> bool {
        let falloff = self.falloff.get();
        let Some(ph) = self.photometric.as_mut() else { return false };
        let v = value as f32;
        let b = value != 0.0;
        let u = value.round().max(0.0) as u32;
        let channel = |c: &mut Param<Vec3>, i: usize| {
            let mut x = c.get();
            x[i] = v.clamp(0.0, 1.0);
            c.set(x);
        };
        match name {
            "Distribution" => {
                let d = u.min(DIST_WEB);
                ph.distribution.set(d);
                self.light_type = if d == DIST_SPOT { LightType::Spot } else { LightType::Omni };
            }
            "Use Kelvin" => ph.use_kelvin.set(b),
            "Lamp" => ph.lamp.set(u.min(LAMPS.len() as u32 - 1)),
            "Kelvin" => ph.kelvin.set(v.clamp(1000.0, 20000.0)),
            "Filter Color R" => channel(&mut ph.filter_color, 0),
            "Filter Color G" => channel(&mut ph.filter_color, 1),
            "Filter Color B" => channel(&mut ph.filter_color, 2),
            // Switching the unit keeps the light as bright as it was.
            "Intensity Unit" => ph.intensity_unit.set(u.min(2)),
            "Intensity" => {
                let cd = ph.to_candela(v.max(0.0), ph.intensity_unit.get(), falloff);
                ph.candela.set(cd);
            }
            "Candela" => ph.candela.set(v.max(0.0)),
            // The lux value stays as typed: the light at the new distance is that bright.
            "Lux Distance" => {
                let lx = ph.from_candela(ph.candela.get(), 2, falloff);
                ph.lux_distance.set(v.max(1e-4));
                if ph.intensity_unit.get() == 2 {
                    let cd = ph.to_candela(lx, 2, falloff);
                    ph.candela.set(cd);
                }
            }
            "Dimming On" => ph.dimming_on.set(b),
            "Dimming" => ph.dimming.set(v.max(0.0)),
            "Incandescent Shift" => ph.incandescent_shift.set(b),
            "Light Shape" => ph.shape.set(u.min(LIGHT_SHAPES.len() as u32 - 1)),
            "Shape Length" => ph.shape_length.set(v.max(0.0)),
            "Shape Width" => ph.shape_width.set(v.max(0.0)),
            "Shape Radius" => ph.shape_radius.set(v.max(0.0)),
            "Render Shape" => ph.render_shape.set(b),
            _ => return false,
        }
        true
    }

    /// Templates rollout: the template's distribution, intensity, lamp and beam.
    pub fn apply_template(&mut self, index: usize) -> Result<(), String> {
        let (name, dist, cd, lamp, beam, field) = *TEMPLATES.get(index).ok_or_else(|| format!("no template {index}"))?;
        if self.photometric.is_none() {
            return Err(format!("{name}: only photometric lights take a template"));
        }
        self.set_photometric("Distribution", dist as f64);
        let ph = self.photometric.as_mut().expect("photometric");
        ph.candela.set(cd);
        ph.use_kelvin.set(false);
        ph.lamp.set(lamp);
        self.hotspot.set(beam);
        self.falloff.set(field);
        Ok(())
    }
}

/// Photometric Uniform Spherical / Web: a small sphere around the pivot.
pub fn bulb_icon() -> Mesh {
    let rings: Vec<Vec<Vec3>> = (1..6).map(|k| {
        let a = k as f32 / 6.0 * std::f32::consts::PI;
        ring(4.0 * a.sin(), 4.0 * a.cos(), 8)
    }).collect();
    let mut polys = Vec::new();
    for k in 0..rings.len() - 1 {
        band(&mut polys, &rings[k], &rings[k + 1]);
    }
    let (top, bottom) = (Vec3::new(0.0, 0.0, 4.0), Vec3::new(0.0, 0.0, -4.0));
    let (first, last) = (&rings[0], &rings[rings.len() - 1]);
    for i in 0..8 {
        polys.push(vec![top, first[i], first[(i + 1) % 8]]);
        polys.push(vec![bottom, last[(i + 1) % 8], last[i]]);
    }
    loom_mesh::build_merged(&wind_outward(polys, Vec3::ZERO))
}

/// Photometric Uniform Diffuse: a hemisphere behind the pivot (the light
/// leaves down -Z), so its own view does not look into it.
pub fn diffuse_icon() -> Mesh {
    let rings: Vec<Vec<Vec3>> = (0..3).map(|k| {
        let a = k as f32 / 3.0 * std::f32::consts::FRAC_PI_2;
        ring(4.0 * a.cos(), 4.0 * a.sin(), 8)
    }).collect();
    let mut polys = Vec::new();
    for k in 0..2 {
        band(&mut polys, &rings[k], &rings[k + 1]);
    }
    polys.push(rings[2].clone());
    polys.push(rings[0].clone());
    loom_mesh::build_merged(&wind_outward(polys, Vec3::new(0.0, 0.0, 1.0)))
}

fn ring(r: f32, z: f32, n: usize) -> Vec<Vec3> {
    (0..n).map(|i| {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        Vec3::new(r * a.cos(), r * a.sin(), z)
    }).collect()
}

/// Side faces between two rings (wound outward) plus optional caps.
fn band(polys: &mut Vec<Vec<Vec3>>, a: &[Vec3], b: &[Vec3]) {
    let n = a.len();
    for i in 0..n {
        let j = (i + 1) % n;
        polys.push(vec![a[i], a[j], b[j], b[i]]);
    }
}

fn wind_outward(polys: Vec<Vec<Vec3>>, centre: Vec3) -> Vec<Vec<Vec3>> {
    polys
        .into_iter()
        .map(|mut f| {
            let n = (f[1] - f[0]).cross(f[2] - f[0]);
            let c = f.iter().sum::<Vec3>() / f.len() as f32;
            if n.dot(c - centre) < 0.0 {
                f.reverse();
            }
            f
        })
        .collect()
}

/// Omni: an octahedron around the pivot.
pub fn omni_icon() -> Mesh {
    let r = 5.0;
    let (x, y, z) = (Vec3::X * r, Vec3::Y * r, Vec3::Z * r);
    let mut polys = Vec::new();
    for sx in [x, -x] {
        for sy in [y, -y] {
            for sz in [z, -z] {
                polys.push(vec![sx, sy, sz]);
            }
        }
    }
    loom_mesh::build_merged(&wind_outward(polys, Vec3::ZERO))
}

/// Spot: a cone opening down -Z at the pivot; like a camera icon it lies
/// behind the light (local z >= 0), so a light view does not look into it.
pub fn spot_icon() -> Mesh {
    let base = ring(3.5, 0.0, 8);
    let apex = Vec3::new(0.0, 0.0, 7.0);
    let mut polys: Vec<Vec<Vec3>> = (0..8).map(|i| vec![apex, base[(i + 1) % 8], base[i]]).collect();
    polys.push(base.clone());
    loom_mesh::build_merged(&wind_outward(polys, Vec3::new(0.0, 0.0, 3.0)))
}

/// Directional: a short cylinder with an arrow head whose tip is the pivot,
/// pointing down -Z (all of it behind the light).
pub fn direct_icon() -> Mesh {
    let (top, bottom) = (ring(2.5, 10.0, 8), ring(2.5, 4.0, 8));
    let mut polys = Vec::new();
    band(&mut polys, &top, &bottom);
    polys.push(top.clone());
    polys.push(bottom.clone());
    let mut polys = wind_outward(polys, Vec3::new(0.0, 0.0, 7.0));
    let head = ring(4.0, 4.0, 8);
    let tip = Vec3::ZERO;
    let mut arrow: Vec<Vec<Vec3>> = (0..8).map(|i| vec![tip, head[i], head[(i + 1) % 8]]).collect();
    arrow.push(head);
    polys.extend(wind_outward(arrow, Vec3::new(0.0, 0.0, 2.5)));
    loom_mesh::build_merged(&polys)
}

/// Skylight: a dome over the pivot.
pub fn sky_icon() -> Mesh {
    let rings: Vec<Vec<Vec3>> = (0..3).map(|k| {
        let a = k as f32 / 3.0 * std::f32::consts::FRAC_PI_2;
        ring(6.0 * a.cos(), 6.0 * a.sin(), 8)
    }).collect();
    let mut polys = Vec::new();
    for k in 0..2 {
        band(&mut polys, &rings[k], &rings[k + 1]);
    }
    polys.push(rings[2].clone());
    polys.push(rings[0].clone());
    loom_mesh::build_merged(&wind_outward(polys, Vec3::new(0.0, 0.0, 1.5)))
}

/// sRGB (swatch) to linear.
pub fn srgb_to_linear(c: Vec3) -> Vec3 {
    let f = |x: f32| if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) };
    Vec3::new(f(c.x), f(c.y), f(c.z))
}

/// How the light reaches a point, renderer-independent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Emitter {
    /// An omni, or a spot / directional light with Overshoot on, in all directions.
    Point,
    /// Hotspot and falloff as full cone angles in degrees.
    Spot { hotspot: f32, falloff: f32 },
    /// Parallel light down `direction` (beam width is display only).
    Directional,
    /// Uniform light from the sky dome and Ambient Only lights.
    Ambient,
}

/// One light that is on, evaluated in world space: the mapping layer each
/// renderer reads. Parameters no renderer uses (Contrast, Soften Diffuse
/// Edge, Projector Map, shadow colour) are not in it.
#[derive(Debug, Clone, PartialEq)]
pub struct LightSample {
    pub id: NodeId,
    pub emitter: Emitter,
    pub position: Vec3,
    /// Unit direction the light travels (its local -Z in world space).
    pub direction: Vec3,
    /// Linear colour times the multiplier.
    pub color: Vec3,
    /// 0 none, 1 inverse, 2 inverse square.
    pub decay: u32,
    /// Distance where decay starts (Near End when near attenuation is used).
    pub decay_start: f32,
    pub near: Option<(f32, f32)>,
    pub far: Option<(f32, f32)>,
    pub shadows: bool,
    pub map_size: u32,
    pub map_bias: f32,
    pub sample_range: f32,
    pub source_radius: f32,
    pub shadow_normal_bias: f32,
    /// Map Bias in scene units (Absolute Map Bias), else relative.
    pub absolute_bias: bool,
    /// What the light's shadows let through, per channel (1 = no shadow,
    /// 0 = full shadow): shadow colour and density.
    pub shadow_transmission: Vec3,
    /// Distance to the target (or Target Distance of a free light).
    pub target_distance: f32,
    pub exclusion: Exclusion,
}

impl LightSample {
    /// The fraction of the light that reaches `distance` from the light at
    /// an angle with cosine `cos_axis` to its direction: decay, near / far
    /// attenuation and the spot cone, as the reference computes them.
    pub fn falloff(&self, distance: f32, cos_axis: f32) -> f32 {
        if self.emitter == Emitter::Ambient {
            return 1.0;
        }
        let mut k = 1.0;
        if self.emitter != Emitter::Directional {
            let r0 = self.decay_start.max(1e-4);
            if self.decay > 0 && distance > r0 {
                k *= (r0 / distance).powi(self.decay as i32);
            }
        }
        if let Some((s, e)) = self.near {
            k *= ramp(distance, s, e);
        }
        if let Some((s, e)) = self.far {
            k *= 1.0 - ramp(distance, s, e);
        }
        if let Emitter::Spot { hotspot, falloff } = self.emitter {
            let (ch, cf) = ((hotspot * 0.5).to_radians().cos(), (falloff * 0.5).to_radians().cos());
            k *= smooth((cos_axis - cf) / (ch - cf).max(1e-6));
        }
        k
    }
}

fn ramp(x: f32, start: f32, end: f32) -> f32 {
    if end <= start {
        return if x >= end { 1.0 } else { 0.0 };
    }
    ((x - start) / (end - start)).clamp(0.0, 1.0)
}

fn smooth(s: f32) -> f32 {
    let s = s.clamp(0.0, 1.0);
    s * s * (3.0 - 2.0 * s)
}

/// Distance from a light to the point it aims at.
pub fn target_distance(scene: &Scene, id: NodeId) -> Option<f32> {
    let ObjectKind::Light(l) = &scene.node(id)?.object else { return None };
    match l.target.filter(|t| scene.node(*t).is_some()) {
        Some(t) => Some((scene.world_matrix(t).w_axis.truncate() - scene.world_matrix(id).w_axis.truncate()).length()),
        None => Some(l.target_distance.get()),
    }
}

/// Every light that is on among the nodes `visible` accepts (hidden lights
/// still light the scene in the reference, so callers usually pass all).
pub fn scene_lights(scene: &Scene, visible: impl Fn(&Node) -> bool) -> Vec<LightSample> {
    let mut out = Vec::new();
    for n in scene.nodes() {
        let ObjectKind::Light(l) = &n.object else { continue };
        if !l.on.get() || !visible(n) {
            continue;
        }
        let world: Mat4 = scene.world_matrix(n.id);
        let position = world.w_axis.truncate();
        let direction = (-world.z_axis.truncate()).normalize_or(Vec3::NEG_Z);
        let emitter = if l.light_type == LightType::Skylight || l.ambient_only.get() {
            Emitter::Ambient
        } else {
            match l.light_type {
                LightType::Spot if !l.overshoot.get() => Emitter::Spot { hotspot: l.hotspot.get(), falloff: l.falloff.get() },
                LightType::Directional => Emitter::Directional,
                _ => Emitter::Point,
            }
        };
        let near = l.near_use.get().then(|| (l.near_start.get(), l.near_end.get()));
        if let Some(ph) = &l.photometric {
            // Physical light: inverse square from 1 m on, the distribution's
            // cone, the emitter's size as the source radius.
            let emitter = match ph.distribution.get() {
                DIST_SPOT => Emitter::Spot { hotspot: l.hotspot.get(), falloff: l.falloff.get() },
                DIST_DIFFUSE => Emitter::Spot { hotspot: 0.0, falloff: DIFFUSE_FALLOFF },
                _ => Emitter::Point,
            };
            let shape_radius = ph.source_radius();
            out.push(LightSample {
                id: n.id,
                emitter,
                position,
                direction,
                color: ph.linear_color() * ph.candela.get() * ph.dim() / PHOTOMETRIC_LUX_PER_MULTIPLIER,
                decay: 2,
                decay_start: 1.0 / ph.unit_metres,
                near: None,
                far: l.far_use.get().then(|| (l.far_start.get(), l.far_end.get())),
                shadows: l.shadows_on.get(),
                map_size: l.map_size.get(),
                map_bias: l.map_bias.get(),
                sample_range: l.sample_range.get(),
                source_radius: if shape_radius > 0.0 { shape_radius } else { l.source_radius.get() },
                shadow_normal_bias: l.shadow_normal_bias.get(),
                absolute_bias: l.absolute_bias.get(),
                shadow_transmission: l.shadow_transmission(),
                target_distance: target_distance(scene, n.id).unwrap_or(DEFAULT_TARGET_DISTANCE),
                exclusion: l.exclusion.clone(),
            });
            continue;
        }
        out.push(LightSample {
            id: n.id,
            emitter,
            position,
            direction,
            color: srgb_to_linear(l.color.get()) * l.multiplier.get(),
            decay: l.decay.get(),
            decay_start: near.map_or(l.decay_start.get(), |(_, e)| e),
            near,
            far: l.far_use.get().then(|| (l.far_start.get(), l.far_end.get())),
            shadows: l.shadows_on.get() && emitter != Emitter::Ambient,
            map_size: l.map_size.get(),
            map_bias: l.map_bias.get(),
            sample_range: l.sample_range.get(),
            // Area Shadows: the area light's size softens the shadows.
            source_radius: if l.area_radius() > 0.0 { l.area_radius() } else { l.source_radius.get() },
            shadow_normal_bias: l.shadow_normal_bias.get(),
            absolute_bias: l.absolute_bias.get(),
            shadow_transmission: l.shadow_transmission(),
            target_distance: target_distance(scene, n.id).unwrap_or(DEFAULT_TARGET_DISTANCE),
            exclusion: l.exclusion.clone(),
        });
    }
    out
}

/// Default lighting for a view whose eye frame is `eye_to_world` (looking
/// down its -Z): one light from the viewer, or a key light above-left of
/// the viewer plus a dimmer fill from below-right behind the objects.
pub fn default_lights(count: u32, eye_to_world: Mat4) -> Vec<LightSample> {
    let dir = |v: Vec3| eye_to_world.transform_vector3(v).normalize_or(Vec3::NEG_Z);
    let base = |direction: Vec3, intensity: f32| LightSample {
        id: NodeId(0),
        emitter: Emitter::Directional,
        position: eye_to_world.w_axis.truncate(),
        direction,
        color: Vec3::splat(intensity),
        decay: 0,
        decay_start: 0.0,
        near: None,
        far: None,
        shadows: false,
        map_size: 512,
        map_bias: 1.0,
        sample_range: 4.0,
        source_radius: 0.0,
        shadow_normal_bias: 1.8,
        absolute_bias: false,
        shadow_transmission: Vec3::ZERO,
        target_distance: DEFAULT_TARGET_DISTANCE,
        exclusion: Exclusion::default(),
    };
    if count >= 2 {
        vec![base(dir(Vec3::new(0.5, -0.5, -1.0)), 0.75), base(dir(Vec3::new(-0.5, 0.6, 1.0)), 0.35)]
    } else {
        vec![base(dir(Vec3::NEG_Z), 1.0)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falloff_follows_hotspot_and_keeps_its_gap() {
        let mut l = LightObject::new(LightType::Spot);
        l.set_parameter("Hotspot", 60.0).unwrap();
        assert_eq!((l.hotspot.get(), l.falloff.get()), (60.0, 62.0));
        l.set_parameter("Falloff", 30.0).unwrap();
        assert_eq!((l.hotspot.get(), l.falloff.get()), (28.0, 30.0));
        l.set_parameter("Falloff", 500.0).unwrap();
        assert_eq!(l.falloff.get(), MAX_SPOT_ANGLE);
    }

    #[test]
    fn decay_and_attenuation_match_the_reference_formulas() {
        let mut s = LightSample { emitter: Emitter::Point, decay: 2, decay_start: 10.0, ..default_lights(1, Mat4::IDENTITY)[0].clone() };
        assert!((s.falloff(5.0, 1.0) - 1.0).abs() < 1e-6, "full strength before the decay start");
        assert!((s.falloff(20.0, 1.0) - 0.25).abs() < 1e-6, "inverse square");
        s.decay = 1;
        assert!((s.falloff(20.0, 1.0) - 0.5).abs() < 1e-6, "inverse");
        s.decay = 0;
        s.far = Some((80.0, 200.0));
        assert_eq!(s.falloff(50.0, 1.0), 1.0);
        assert!((s.falloff(140.0, 1.0) - 0.5).abs() < 1e-6);
        assert_eq!(s.falloff(250.0, 1.0), 0.0);
        s.far = None;
        s.near = Some((0.0, 40.0));
        assert!((s.falloff(20.0, 1.0) - 0.5).abs() < 1e-6, "near attenuation fades in");
    }

    #[test]
    fn spot_cone_is_full_inside_the_hotspot_and_dark_outside_the_falloff() {
        let s = LightSample { emitter: Emitter::Spot { hotspot: 40.0, falloff: 60.0 }, ..default_lights(1, Mat4::IDENTITY)[0].clone() };
        assert_eq!(s.falloff(10.0, 15f32.to_radians().cos()), 1.0);
        assert_eq!(s.falloff(10.0, 35f32.to_radians().cos()), 0.0);
        let mid = s.falloff(10.0, 25f32.to_radians().cos());
        assert!(mid > 0.0 && mid < 1.0);
    }

    #[test]
    fn kelvin_colours_run_from_warm_to_white() {
        let d65 = kelvin_to_srgb(6504.0);
        assert!(d65.min_element() > 0.95, "D65 is white: {d65}");
        let warm = kelvin_to_srgb(2800.0);
        assert!(warm.x == 1.0 && warm.z < 0.6, "incandescent is warm: {warm}");
    }

    #[test]
    fn photometric_intensity_units_convert_through_candela() {
        let mut l = LightObject::photometric(1.0);
        assert_eq!(l.stack_name(), "Free Light");
        l.set_parameter("Intensity Unit", 0.0).unwrap();
        let lm = 1500.0 * 4.0 * std::f32::consts::PI;
        let ParamValue::Float(v) = l.parameters().into_iter().find(|p| p.0 == "Intensity").unwrap().1 else { panic!() };
        assert!((v - lm).abs() < 0.5, "{v}");
        l.set_parameter("Intensity", 1000.0).unwrap();
        assert!((l.photometric.as_ref().unwrap().candela.get() - 1000.0 / (4.0 * std::f32::consts::PI)).abs() < 1e-3);
        // lx at 2 m: cd / 4.
        l.set_parameter("Candela", 1500.0).unwrap();
        l.set_parameter("Intensity Unit", 2.0).unwrap();
        l.set_parameter("Lux Distance", 2.0).unwrap();
        let ParamValue::Float(v) = l.parameters().into_iter().find(|p| p.0 == "Intensity").unwrap().1 else { panic!() };
        assert!((v - 1500.0).abs() < 1e-2, "the lux value is kept when the distance changes: {v}");
        l.set_parameter("Distribution", 2.0).unwrap();
        assert_eq!(l.light_type, LightType::Spot);
    }

    #[test]
    fn shadow_colour_density_and_area_lights_reach_the_sample() {
        let mut l = LightObject::new(LightType::Spot);
        assert_eq!(l.shadow_transmission(), Vec3::ZERO, "black shadows at density 1 stop all light");
        l.set_parameter("Shadow Density", 0.25).unwrap();
        assert!((l.shadow_transmission() - Vec3::splat(0.75)).length() < 1e-6);
        l.set_parameter("Shadow Density", 1.0).unwrap();
        l.set_parameter("Shadow Color G", 1.0).unwrap();
        assert_eq!(l.shadow_transmission(), Vec3::new(0.0, 1.0, 0.0), "a green shadow lets green through");
        l.set_parameter("Color R", 0.0).unwrap();
        l.set_parameter("Light Affects Shadow Color", 1.0).unwrap();
        assert_eq!(l.shadow_transmission(), Vec3::new(0.0, 1.0, 0.0));
        l.set_parameter("Color G", 0.0).unwrap();
        assert_eq!(l.shadow_transmission(), Vec3::ZERO, "the light's colour multiplies the shadow colour");
        assert_eq!(l.area_radius(), 0.0, "Shadow Map: a point source");
        l.set_parameter("Shadow Type", AREA_SHADOWS as f64).unwrap();
        l.set_parameter("Area Length", 20.0).unwrap();
        l.set_parameter("Area Width", 20.0).unwrap();
        assert!((l.area_radius() - (400.0 / std::f32::consts::PI).sqrt()).abs() < 1e-4, "Rectangle Light: equal area");
        l.set_parameter("Area Mode", 4.0).unwrap();
        assert_eq!(l.area_radius(), 10.0, "Sphere Light: half its length");
        l.set_parameter("Area Mode", 0.0).unwrap();
        assert_eq!(l.area_radius(), 0.0, "Simple: a point");
        assert_eq!(l.choices("Area Mode").unwrap().len(), 5);
        assert_eq!(SHADOW_TYPES.len(), 4);
    }

    #[test]
    fn exclusion_scopes() {
        let e = Exclusion { include: false, scope: 1, nodes: vec![NodeId(3)] };
        assert!(e.excludes(NodeId(3), false) && !e.excludes(NodeId(3), true) && !e.excludes(NodeId(4), false));
        let i = Exclusion { include: true, scope: 2, nodes: vec![NodeId(3)] };
        assert!(!i.excludes(NodeId(3), true) && i.excludes(NodeId(4), true));
    }

    #[test]
    fn icons_are_closed_meshes_around_the_pivot() {
        for t in [LightType::Omni, LightType::Spot, LightType::Directional, LightType::Skylight] {
            let m = LightObject::new(t).icon_mesh();
            assert!(m.face_count() > 0, "{t:?}");
            let (min, max) = m.aabb();
            assert!(min.length() < 12.0 && max.length() < 12.0, "{t:?} icon is small: {min} {max}");
            if t.aims() {
                assert!(min.z >= -1e-5, "{t:?}: nothing in front of the light, where its own view looks");
            }
        }
    }
}
