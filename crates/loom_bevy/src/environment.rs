//! The scene's Environment and Effects as the renderer applies them: every
//! animated value evaluated at one time, colours still sRGB 0–1.
//! [`EnvSettings::from_scene`] builds it; `BevyRenderer::set_environment`
//! applies it to the lights, the cameras (exposure, the environment pass)
//! and the fog volumes; [`crate::effects`] applies the image effects to a
//! finished render.

use loom_scene::environment::{AtmosphereKind, BlurType, EffectKind, EnvMapping, Environment, ExposureKind, LayerFalloff};

/// The exposure control in use (Active, and not "no exposure control").
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Exposure {
    pub kind: ExposureKind,
    pub process_background: bool,
    pub brightness: f32,
    pub contrast: f32,
    pub exposure_value: f32,
    pub mid_tones: f32,
}

impl Exposure {
    /// Automatic and Linear sample the rendered image's brightness.
    pub fn meters(&self) -> bool {
        matches!(self.kind, ExposureKind::Automatic | ExposureKind::Linear)
    }

    /// Stops added to the exposure, before metering: Brightness 50 is
    /// neutral (each 25 is a stop; Logarithmic's 0–200 each 50), plus the
    /// Exposure Value of Automatic / Linear.
    pub fn stops(&self) -> f32 {
        match self.kind {
            ExposureKind::Logarithmic => (self.brightness - 50.0) / 50.0,
            ExposureKind::None => 0.0,
            _ => (self.brightness - 50.0) / 25.0 + self.exposure_value,
        }
    }
}

/// One atmosphere the environment pass draws, in list order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fog {
    /// Standard fog: density (0–1) at the near and far environment range.
    Standard { color: [f32; 3], near: f32, far: f32, exponential: bool, fog_background: bool },
    /// Layered fog between `bottom` and `top` (world Z).
    Layered { color: [f32; 3], bottom: f32, top: f32, density: f32, falloff: LayerFalloff, fog_background: bool },
    /// Volume Fog without gizmos: the whole scene, density 0–20.
    Volume { color: [f32; 3], density: f32, fog_background: bool },
}

/// A Volume Light atmosphere.
#[derive(Debug, Clone, PartialEq)]
pub struct VolumeLight {
    pub lights: Vec<u64>,
    pub color: [f32; 3],
    /// Percent.
    pub density: f32,
}

/// The Depth of Field effect (Custom focal parameters).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DepthOfField {
    /// Pick Node: the focal node's id (its distance is the view's focus).
    pub focal_node: Option<u64>,
    pub horiz_loss: f32,
    pub vert_loss: f32,
    pub focal_range: f32,
    pub focal_limit: f32,
}

/// The image effects applied to a finished render, in list order.
#[derive(Debug, Clone, PartialEq)]
pub enum ImageEffect {
    Glow { size: f32, intensity: f32, use_source_color: f32, inner: [f32; 3], outer: [f32; 3], bright: f32 },
    Blur(Blur),
    BrightnessContrast { brightness: f32, contrast: f32, ignore_background: bool },
    ColorBalance { cyan_red: f32, magenta_green: f32, yellow_blue: f32, preserve_luminosity: bool, ignore_background: bool },
    FilmGrain { grain: f32, ignore_background: bool },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Blur {
    pub blur_type: BlurType,
    pub radius: f32,
    pub u_radius: f32,
    pub v_radius: f32,
    pub u_trail: f32,
    pub v_trail: f32,
    pub rotation: f32,
    pub radial_trail: f32,
    pub origin: [f32; 2],
    pub affect_alpha: bool,
    pub non_background: bool,
    pub brighten: f32,
    pub blend: f32,
}

/// The environment the renderer uses.
#[derive(Debug, Clone, PartialEq)]
pub struct EnvSettings {
    pub background: [f32; 3],
    /// Use Map's bitmap file and its mapping.
    pub map: Option<(String, EnvMapping)>,
    pub tint: [f32; 3],
    pub level: f32,
    pub ambient: [f32; 3],
    pub exposure: Option<Exposure>,
    /// At most [`MAX_FOGS`] are drawn.
    pub fogs: Vec<Fog>,
    pub volume_lights: Vec<VolumeLight>,
    pub depth_of_field: Option<DepthOfField>,
    pub effects: Vec<ImageEffect>,
}

/// Atmospheres the environment pass draws at once.
pub const MAX_FOGS: usize = 4;

impl Default for EnvSettings {
    fn default() -> Self {
        Self::from_scene(&Environment::default(), 0.0)
    }
}

fn rgb(v: glam::Vec3) -> [f32; 3] {
    v.to_array()
}

impl EnvSettings {
    /// The environment at frame `t` (inactive entries left out).
    pub fn from_scene(e: &Environment, t: f32) -> Self {
        let x = &e.exposure;
        let exposure = (x.active && x.kind != ExposureKind::None).then(|| Exposure {
            kind: x.kind,
            process_background: x.process_background,
            brightness: x.brightness.get_at(t),
            contrast: x.contrast.get_at(t),
            exposure_value: x.exposure_value.get_at(t),
            mid_tones: x.mid_tones.get_at(t),
        });
        let mut fogs = Vec::new();
        let mut volume_lights = Vec::new();
        for a in e.atmospheres.iter().filter(|a| a.active) {
            match &a.kind {
                AtmosphereKind::Fog(f) if f.layered => fogs.push(Fog::Layered {
                    color: rgb(f.color.get_at(t)),
                    bottom: f.bottom.get_at(t),
                    top: f.top.get_at(t),
                    density: f.density.get_at(t) / 100.0,
                    falloff: f.falloff,
                    fog_background: f.fog_background,
                }),
                AtmosphereKind::Fog(f) => fogs.push(Fog::Standard { color: rgb(f.color.get_at(t)), near: f.near.get_at(t) / 100.0, far: f.far.get_at(t) / 100.0, exponential: f.exponential, fog_background: f.fog_background }),
                AtmosphereKind::VolumeFog(v) => fogs.push(Fog::Volume { color: rgb(v.color.get_at(t)), density: v.density.get_at(t), fog_background: v.fog_background }),
                AtmosphereKind::VolumeLight(l) => volume_lights.push(VolumeLight { lights: l.lights.iter().map(|n| n.0).collect(), color: rgb(l.fog_color.get_at(t)), density: l.density.get_at(t) }),
            }
        }
        let mut depth_of_field = None;
        let mut effects = Vec::new();
        for fx in e.effects.iter().filter(|f| f.active) {
            match &fx.kind {
                EffectKind::DepthOfField(d) => {
                    depth_of_field.get_or_insert(DepthOfField {
                        focal_node: d.focal_node.map(|n| n.0),
                        horiz_loss: d.horiz_loss.get_at(t),
                        vert_loss: d.vert_loss.get_at(t),
                        focal_range: d.focal_range.get_at(t),
                        focal_limit: d.focal_limit.get_at(t),
                    });
                }
                EffectKind::Glow(g) => effects.push(ImageEffect::Glow {
                    size: g.size.get_at(t),
                    intensity: g.intensity.get_at(t),
                    use_source_color: g.use_source_color.get_at(t),
                    inner: rgb(g.inner_color.get_at(t)),
                    outer: rgb(g.outer_color.get_at(t)),
                    bright: g.bright.get_at(t),
                }),
                EffectKind::Blur(b) => effects.push(ImageEffect::Blur(Blur {
                    blur_type: b.blur_type,
                    radius: b.radius.get_at(t),
                    u_radius: b.u_radius.get_at(t),
                    v_radius: b.v_radius.get_at(t),
                    u_trail: b.u_trail.get_at(t),
                    v_trail: b.v_trail.get_at(t),
                    rotation: b.rotation.get_at(t),
                    radial_trail: b.radial_trail.get_at(t),
                    origin: b.origin,
                    affect_alpha: b.affect_alpha,
                    non_background: b.non_background,
                    brighten: b.brighten.get_at(t),
                    blend: b.blend.get_at(t),
                })),
                EffectKind::BrightnessContrast(b) => effects.push(ImageEffect::BrightnessContrast { brightness: b.brightness.get_at(t), contrast: b.contrast.get_at(t), ignore_background: b.ignore_background }),
                EffectKind::ColorBalance(c) => effects.push(ImageEffect::ColorBalance {
                    cyan_red: c.cyan_red.get_at(t),
                    magenta_green: c.magenta_green.get_at(t),
                    yellow_blue: c.yellow_blue.get_at(t),
                    preserve_luminosity: c.preserve_luminosity,
                    ignore_background: c.ignore_background,
                }),
                EffectKind::FilmGrain(f) => effects.push(ImageEffect::FilmGrain { grain: f.grain.get_at(t), ignore_background: f.ignore_background }),
            }
        }
        Self {
            background: rgb(e.background.get_at(t)),
            map: e.map().map(|p| (p.to_owned(), e.map_mapping)),
            tint: rgb(e.tint.get_at(t)),
            level: e.level.get_at(t),
            ambient: rgb(e.ambient.get_at(t)),
            exposure,
            fogs,
            volume_lights,
            depth_of_field,
            effects,
        }
    }

    /// Without atmospheres (Render Setup > Atmospherics off).
    pub fn without_atmospherics(&self) -> Self {
        Self { fogs: Vec::new(), volume_lights: Vec::new(), ..self.clone() }
    }

    /// Without rendering effects (Render Setup > Effects off).
    pub fn without_effects(&self) -> Self {
        Self { depth_of_field: None, effects: Vec::new(), ..self.clone() }
    }
}

/// What one view adds to the environment: the camera's Environment Ranges
/// (standard fog; None: the scene's near and far side) and the distance in
/// focus (Depth of Field; None: the focal node's distance or no blur).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ViewEnv {
    pub ranges: Option<[f32; 2]>,
    pub focus: Option<f32>,
}

/// Linear-light value of an sRGB component.
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub fn linear_to_srgb(c: f32) -> f32 {
    let c = c.max(0.0);
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Volume Fog density (0–20) as extinction per world unit for a scene of
/// radius `radius`: density 20 hides the far side of the scene (optical
/// depth 6 across it), 5 lets about a fifth through.
pub fn volume_extinction(density: f32, radius: f32) -> f32 {
    density.max(0.0) * 0.15 / radius.max(1e-3)
}

/// Volume Light density (percent) as the density of a fog volume `length`
/// deep: at 100% the glow seen through the whole volume is about as bright
/// as a white surface lit by the light there. Bevy scatters omni and spot
/// light without the view's exposure (Bevy's default EV100 9.7 makes that
/// 1.2 · 2^9.7 brighter) and with a factor 0.1; directional light with
/// both and an isotropic phase of 1 / 4π. The fog scatters (0.5) and
/// absorbs nothing.
pub fn volume_light_density(density: f32, length: f32, directional: bool) -> f32 {
    let per_length = if directional { 4.0 * std::f32::consts::PI / (std::f32::consts::PI * 0.5) } else { 1.0 / (1.2 * 831.746_5 * std::f32::consts::PI * 0.1 * 0.5) };
    density.max(0.0) / 100.0 * per_length / length.max(1e-3)
}

/// Layered fog density (0–1) as extinction per world unit: looking straight
/// through a layer at 100% leaves e^-4 of what is behind it.
pub fn layer_extinction(density: f32, bottom: f32, top: f32) -> f32 {
    density.max(0.0) * 4.0 / (top - bottom).max(1e-3)
}
