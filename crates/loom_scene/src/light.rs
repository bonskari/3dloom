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
pub const SHADOW_TYPES: [&str; 2] = ["Shadow Map", "Ray Traced Shadows"];
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
        }
    }

    /// Base name of a new light ("Omni001", "Spot001", "FSpot001", ...).
    pub fn base_name(&self) -> &'static str {
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
        v
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
            _ => return Err(format!("{} has no parameter `{name}`", self.stack_name())),
        }
        Ok(())
    }

    pub fn choices(&self, name: &str) -> Option<&'static [&'static str]> {
        match name {
            "Shadow Type" => Some(&SHADOW_TYPES),
            "Decay Type" => Some(&DECAY_TYPES),
            _ => None,
        }
    }

    /// The viewport icon in the light's frame (it shines down local -Z).
    pub fn icon_mesh(&self) -> Mesh {
        match self.light_type {
            LightType::Omni => omni_icon(),
            LightType::Spot => spot_icon(),
            LightType::Directional => direct_icon(),
            LightType::Skylight => sky_icon(),
        }
    }
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
            source_radius: l.source_radius.get(),
            shadow_normal_bias: l.shadow_normal_bias.get(),
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
