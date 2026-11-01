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
    /// Volume Fog, density 0–20: in its gizmos (`in_gizmos`, the bits of
    /// [`EnvSettings::gizmos`] in `gizmos`), else the whole scene.
    Volume { color: [f32; 3], density: f32, fog_background: bool, in_gizmos: bool, gizmos: u32 },
    /// Fire Effect in the gizmos `gizmos` (bits of [`EnvSettings::gizmos`]).
    Fire(Fire),
}

/// A Fire Effect's values at one time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fire {
    pub inner: [f32; 3],
    pub outer: [f32; 3],
    pub smoke: [f32; 3],
    pub tendril: bool,
    pub stretch: f32,
    pub regularity: f32,
    pub flame_size: f32,
    pub flame_detail: f32,
    pub density: f32,
    pub samples: u32,
    pub phase: f32,
    pub drift: f32,
    pub explosion: bool,
    pub smoke_on: bool,
    pub fury: f32,
    pub gizmos: u32,
}

/// An atmospheric apparatus the environment pass fills: world to local,
/// shape (0 box, 1 sphere, 2 cylinder), size (box width, length, height;
/// sphere radius, hemisphere 0/1; cylinder radius, height) and seed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gizmo {
    pub world_to_local: glam::Mat4,
    pub shape: u32,
    pub size: [f32; 3],
    pub seed: u32,
}

/// Apparatus the environment pass handles at once.
pub const MAX_GIZMOS: usize = 8;

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
    /// Lens Effects Star / Streak at the centre of each area brighter than `bright`.
    Star(Star),
    /// Lens Effects Ring round the centre of each area brighter than `bright`.
    Ring(Ring),
    /// Lens Effects Manual / Auto Secondary flares.
    Secondary(Secondary),
    Blur(Blur),
    BrightnessContrast { brightness: f32, contrast: f32, ignore_background: bool },
    ColorBalance { cyan_red: f32, magenta_green: f32, yellow_blue: f32, preserve_luminosity: bool, ignore_background: bool },
    FilmGrain { grain: f32, ignore_background: bool },
    /// File Output: writes the image (channel 0 whole, 1 luminance, 2 alpha)
    /// to `path` (None: no file, as in previews); `affect_source` passes
    /// the channel on as the image.
    FileOutput { path: Option<String>, channel: u32, affect_source: bool },
    /// Motion Blur: the renderer smears the Image-blurred objects over
    /// `duration` frames here (it needs the scene, so `effects::apply` skips it).
    MotionBlur { duration: f32 },
}

/// Lens Effects secondary flares: discs (`plane` % of the way from each
/// bright area's centre to the image centre, radius % of the image diagonal).
#[derive(Debug, Clone, PartialEq)]
pub struct Secondary {
    pub discs: Vec<(f32, f32)>,
    pub intensity: f32,
    /// 0 circular, else 3 to 8 sides.
    pub sides: u32,
    pub use_source_color: f32,
    pub inner: [f32; 3],
    pub outer: [f32; 3],
    pub bright: f32,
}

/// Auto Secondary: `qty` discs spread evenly along 0..200 × `axis` % of the
/// source-to-centre line, each nudged and sized between `min` and `max` by
/// a hash of `seed`, so a frame renders the same each time.
pub fn auto_secondary_discs(qty: u32, min: f32, max: f32, axis: f32, seed: u32) -> Vec<(f32, f32)> {
    let hash = |i: u32, salt: u32| ((i.wrapping_add(seed).wrapping_mul(2_654_435_761) ^ salt.wrapping_mul(40_503)).wrapping_mul(2_246_822_519) >> 8) as f32 / (1u32 << 24) as f32;
    let (lo, hi) = (min.min(max).max(0.0), max.max(min).max(0.0));
    (0..qty)
        .map(|i| {
            let step = 200.0 * axis.max(0.0) / qty as f32;
            ((i as f32 + 0.5 + (hash(i, 3) - 0.5) * 0.8) * step, lo + (hi - lo) * hash(i, 4))
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ring {
    pub size: f32,
    pub intensity: f32,
    pub plane: f32,
    pub thickness: f32,
    pub use_source_color: f32,
    pub inner: [f32; 3],
    pub outer: [f32; 3],
    pub bright: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Star {
    /// Ray: the spokes are randomly spaced and of random lengths.
    pub scatter: bool,
    /// Lens Effects Globals > Seed (varies the Ray's spokes).
    pub seed: u32,
    pub qty: u32,
    pub size: f32,
    pub intensity: f32,
    pub width: f32,
    pub angle: f32,
    pub taper: f32,
    pub sharp: f32,
    pub use_source_color: f32,
    pub inner: [f32; 3],
    pub outer: [f32; 3],
    pub bright: f32,
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
    /// Per entry of `fogs`: the apparatus node ids it fills (Volume Fog, Fire).
    pub fog_gizmo_ids: Vec<Vec<u64>>,
    /// The apparatus [`EnvSettings::resolve_gizmos`] found, at most [`MAX_GIZMOS`].
    pub gizmos: Vec<Gizmo>,
    pub volume_lights: Vec<VolumeLight>,
    pub depth_of_field: Option<DepthOfField>,
    pub effects: Vec<ImageEffect>,
    /// Preferences > Use Environment Alpha: a render's background takes
    /// the map's alpha (off: alpha 0).
    pub use_alpha: bool,
    /// Preferences > Filter Background: a render's antialiasing filter also
    /// runs over the background map (off: the background stays unfiltered).
    pub filter_background: bool,
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
        let mut fog_gizmo_ids = Vec::new();
        let mut volume_lights = Vec::new();
        for a in e.atmospheres.iter().filter(|a| a.active) {
            if !matches!(a.kind, AtmosphereKind::VolumeLight(_)) {
                fog_gizmo_ids.push(a.kind.gizmos().map(|g| g.iter().map(|n| n.0).collect()).unwrap_or_default());
            }
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
                AtmosphereKind::VolumeFog(v) => fogs.push(Fog::Volume { color: rgb(v.color.get_at(t)), density: v.density.get_at(t), fog_background: v.fog_background, in_gizmos: !v.gizmos.is_empty(), gizmos: 0 }),
                AtmosphereKind::Fire(f) => fogs.push(Fog::Fire(Fire {
                    inner: rgb(f.inner_color.get_at(t)),
                    outer: rgb(f.outer_color.get_at(t)),
                    smoke: rgb(f.smoke_color.get_at(t)),
                    tendril: f.tendril,
                    stretch: f.stretch.get_at(t),
                    regularity: f.regularity.get_at(t),
                    flame_size: f.flame_size.get_at(t),
                    flame_detail: f.flame_detail.get_at(t),
                    density: f.density.get_at(t),
                    samples: f.samples.get_at(t),
                    phase: f.phase.get_at(t),
                    drift: f.drift.get_at(t),
                    explosion: f.explosion,
                    smoke_on: f.smoke,
                    fury: f.fury.get_at(t),
                    gizmos: 0,
                })),
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
                EffectKind::Glow(g) => {
                    // Lens Effects Globals scale every element.
                    let gs = g.globals.size.get_at(t).max(0.0) / 100.0;
                    let gi = g.globals.intensity.get_at(t).max(0.0) / 100.0;
                    let ga = g.globals.angle.get_at(t);
                    effects.push(ImageEffect::Glow {
                        size: g.size.get_at(t) * gs,
                        intensity: g.intensity.get_at(t) * gi,
                        use_source_color: g.use_source_color.get_at(t),
                        inner: rgb(g.inner_color.get_at(t)),
                        outer: rgb(g.outer_color.get_at(t)),
                        bright: g.bright.get_at(t),
                    });
                    if g.ring.on {
                        let r = &g.ring;
                        effects.push(ImageEffect::Ring(Ring {
                            size: r.size.get_at(t) * gs,
                            intensity: r.intensity.get_at(t) * gi,
                            plane: r.plane.get_at(t),
                            thickness: r.thickness.get_at(t),
                            use_source_color: r.use_source_color.get_at(t),
                            inner: rgb(r.inner_color.get_at(t)),
                            outer: rgb(r.outer_color.get_at(t)),
                            bright: g.bright.get_at(t),
                        }));
                    }
                    for (el, scatter) in [(&g.star, false), (&g.streak, false), (&g.ray, true)].into_iter().filter(|(el, _)| el.on) {
                        effects.push(ImageEffect::Star(Star {
                            scatter,
                            seed: g.globals.seed,
                            qty: el.qty,
                            size: el.size.get_at(t) * gs,
                            intensity: el.intensity.get_at(t) * gi,
                            width: el.width.get_at(t),
                            angle: el.angle.get_at(t) + ga,
                            taper: el.taper.get_at(t),
                            sharp: el.sharp.get_at(t),
                            use_source_color: el.use_source_color.get_at(t),
                            inner: rgb(el.inner_color.get_at(t)),
                            outer: rgb(el.outer_color.get_at(t)),
                            bright: g.bright.get_at(t),
                        }));
                    }
                    let m = &g.manual_secondary;
                    if m.on {
                        effects.push(ImageEffect::Secondary(Secondary {
                            discs: vec![(m.plane.get_at(t), m.size.get_at(t) * gs)],
                            intensity: m.intensity.get_at(t) * gi,
                            sides: m.sides,
                            use_source_color: m.use_source_color.get_at(t),
                            inner: rgb(m.inner_color.get_at(t)),
                            outer: rgb(m.outer_color.get_at(t)),
                            bright: g.bright.get_at(t),
                        }));
                    }
                    let a = &g.auto_secondary;
                    if a.on && a.qty > 0 {
                        effects.push(ImageEffect::Secondary(Secondary {
                            discs: auto_secondary_discs(a.qty, a.min.get_at(t) * gs, a.max.get_at(t) * gs, a.axis.get_at(t), g.globals.seed),
                            intensity: a.intensity.get_at(t) * gi,
                            sides: a.sides,
                            use_source_color: a.use_source_color.get_at(t),
                            inner: rgb(a.inner_color.get_at(t)),
                            outer: rgb(a.outer_color.get_at(t)),
                            bright: g.bright.get_at(t),
                        }));
                    }
                }
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
                EffectKind::FileOutput(f) => effects.push(ImageEffect::FileOutput { path: f.path.clone(), channel: f.channel, affect_source: f.affect_source }),
                EffectKind::MotionBlur(m) => effects.push(ImageEffect::MotionBlur { duration: m.duration.get_at(t) }),
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
            fog_gizmo_ids,
            gizmos: Vec::new(),
            volume_lights,
            depth_of_field,
            effects,
            use_alpha: false,
            filter_background: false,
        }
    }

    /// Without atmospheres (Render Setup > Atmospherics off).
    pub fn without_atmospherics(&self) -> Self {
        Self { fogs: Vec::new(), fog_gizmo_ids: Vec::new(), gizmos: Vec::new(), volume_lights: Vec::new(), ..self.clone() }
    }

    /// Looks up the atmospheric apparatus the atmospheres fill in `scene`
    /// (their world matrices now) and sets each atmosphere's gizmo bits;
    /// deleted or other nodes are left out.
    pub fn resolve_gizmos(&mut self, scene: &loom_scene::Scene) {
        self.gizmos.clear();
        let mut index: Vec<u64> = Vec::new();
        for (f, ids) in self.fogs.iter_mut().zip(&self.fog_gizmo_ids) {
            let mut mask = 0u32;
            for id in ids {
                let i = match index.iter().position(|x| x == id) {
                    Some(i) => i,
                    None => {
                        let Some(n) = scene.node(loom_scene::NodeId(*id)) else { continue };
                        let loom_scene::ObjectKind::AtmosApparatus { shape, length, width, height, radius, hemisphere, seed } = &n.object else { continue };
                        if self.gizmos.len() == MAX_GIZMOS {
                            continue;
                        }
                        let size = match shape {
                            0 => [width.get(), length.get(), height.get()],
                            1 => [radius.get(), if hemisphere.get() { 1.0 } else { 0.0 }, 0.0],
                            _ => [radius.get(), height.get(), 0.0],
                        };
                        let world = scene.world_matrix(n.id);
                        let world_to_local = if world.determinant().abs() > 1e-12 { world.inverse() } else { continue };
                        self.gizmos.push(Gizmo { world_to_local, shape: *shape, size, seed: seed.get() });
                        index.push(*id);
                        index.len() - 1
                    }
                };
                mask |= 1 << i;
            }
            match f {
                Fog::Volume { gizmos, .. } => *gizmos = mask,
                Fog::Fire(fire) => fire.gizmos = mask,
                _ => {}
            }
        }
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

/// Volume Light density (percent) as the density of the fog volume a
/// directional light scatters in, `length` deep: at 100% the glow seen
/// through the whole volume is about as bright as a white surface the
/// light lights (Bevy scatters directional light with an isotropic phase
/// of 1 / 4π; the fog scatters 0.5 and absorbs nothing).
pub fn volume_light_density(density: f32, length: f32) -> f32 {
    density.max(0.0) / 100.0 * 4.0 * std::f32::consts::PI / (std::f32::consts::PI * 0.5) / length.max(1e-3)
}

/// Layered fog density (0–1) as extinction per world unit: looking straight
/// through a layer at 100% leaves e^-4 of what is behind it.
pub fn layer_extinction(density: f32, bottom: f32, top: f32) -> f32 {
    density.max(0.0) * 4.0 / (top - bottom).max(1e-3)
}
