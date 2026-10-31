//! Render Setup settings saved with the scene: the Common panel (output
//! size, options, render output, area to render) and the Bevy renderer's own
//! panel. The background and ambient colours are the Environment's.

use serde::{Deserialize, Serialize};

/// Rendered Frame Window > Area to Render.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RenderArea {
    /// The whole view.
    #[default]
    View,
    /// Only the selected objects.
    Selected,
    /// A rectangle of the view; the rest of the frame is left alone.
    Region,
    /// A rectangle of the view, output at its own size.
    Crop,
    /// A rectangle of the view enlarged to the output size.
    Blowup,
}

impl RenderArea {
    pub const ALL: [RenderArea; 5] = [RenderArea::View, RenderArea::Selected, RenderArea::Region, RenderArea::Crop, RenderArea::Blowup];

    pub fn label(self) -> &'static str {
        match self {
            RenderArea::View => "View",
            RenderArea::Selected => "Selected",
            RenderArea::Region => "Region",
            RenderArea::Crop => "Crop",
            RenderArea::Blowup => "Blowup",
        }
    }

    /// Whether the area is a rectangle of the view (Region, Crop, Blowup).
    pub fn uses_region(self) -> bool {
        matches!(self, RenderArea::Region | RenderArea::Crop | RenderArea::Blowup)
    }
}

/// Antialiasing > Filter: how the samples around a pixel are weighted into
/// it (a reconstruction filter Filter Size pixels wide).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AaFilter {
    /// A box: every sample under the filter counts the same.
    #[default]
    Area,
    /// Blackman window: sharp, little ringing.
    Blackman,
    /// Catmull-Rom cubic: sharpening.
    CatmullRom,
    /// Cubic B-spline: smooth, slightly blurring.
    Cubic,
    /// Mitchell-Netravali cubic (B = C = 1/3): balanced.
    MitchellNetravali,
    /// Quadratic B-spline: smooth.
    Quadratic,
    /// Gaussian: soft.
    Soften,
}

impl AaFilter {
    pub const ALL: [AaFilter; 7] = [AaFilter::Area, AaFilter::Blackman, AaFilter::CatmullRom, AaFilter::Cubic, AaFilter::MitchellNetravali, AaFilter::Quadratic, AaFilter::Soften];

    pub fn label(self) -> &'static str {
        match self {
            AaFilter::Area => "Area",
            AaFilter::Blackman => "Blackman",
            AaFilter::CatmullRom => "Catmull-Rom",
            AaFilter::Cubic => "Cubic",
            AaFilter::MitchellNetravali => "Mitchell-Netravali",
            AaFilter::Quadratic => "Quadratic",
            AaFilter::Soften => "Soften",
        }
    }

    pub fn from_label(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.label() == s)
    }

    /// The filter's weight at `x`, the distance from the pixel centre over
    /// the filter's radius (0 at the centre, 1 at the edge, 0 beyond).
    pub fn weight(self, x: f32) -> f32 {
        let x = x.abs();
        if x >= 1.0 {
            return 0.0;
        }
        // The cubics at their natural size span 2 pixels each side.
        let cubic = |b: f32, c: f32| {
            let t = 2.0 * x;
            if t < 1.0 {
                ((12.0 - 9.0 * b - 6.0 * c) * t * t * t + (-18.0 + 12.0 * b + 6.0 * c) * t * t + (6.0 - 2.0 * b)) / 6.0
            } else {
                ((-b - 6.0 * c) * t * t * t + (6.0 * b + 30.0 * c) * t * t + (-12.0 * b - 48.0 * c) * t + (8.0 * b + 24.0 * c)) / 6.0
            }
        };
        match self {
            AaFilter::Area => 1.0,
            AaFilter::Blackman => {
                let a = std::f32::consts::PI * (x + 1.0);
                0.42 - 0.5 * a.cos() + 0.08 * (2.0 * a).cos()
            }
            AaFilter::CatmullRom => cubic(0.0, 0.5),
            AaFilter::Cubic => cubic(1.0, 0.0),
            AaFilter::MitchellNetravali => cubic(1.0 / 3.0, 1.0 / 3.0),
            AaFilter::Quadratic => {
                let t = 1.5 * x;
                if t < 0.5 {
                    0.75 - t * t
                } else {
                    0.5 * (1.5 - t) * (1.5 - t)
                }
            }
            AaFilter::Soften => (-8.0 * x * x).exp(),
        }
    }
}

/// Exposure Control > the tone mapping curve from scene light to the image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Tonemapper {
    /// Clamp only (the reference's default: no exposure control).
    #[default]
    None,
    Reinhard,
    /// ACES filmic (fitted).
    Aces,
    /// AgX filmic: soft highlight roll-off.
    AgX,
    /// Tony McMapface: neutral filmic.
    TonyMcMapface,
    BlenderFilmic,
}

impl Tonemapper {
    pub const ALL: [Tonemapper; 6] = [Tonemapper::None, Tonemapper::Reinhard, Tonemapper::Aces, Tonemapper::AgX, Tonemapper::TonyMcMapface, Tonemapper::BlenderFilmic];

    pub fn label(self) -> &'static str {
        match self {
            Tonemapper::None => "None",
            Tonemapper::Reinhard => "Reinhard",
            Tonemapper::Aces => "ACES Filmic",
            Tonemapper::AgX => "AgX",
            Tonemapper::TonyMcMapface => "Tony McMapface",
            Tonemapper::BlenderFilmic => "Blender Filmic",
        }
    }

    pub fn from_label(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.label() == s)
    }
}

/// Ambient Occlusion quality: samples per pixel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AoQuality {
    Low,
    Medium,
    #[default]
    High,
    Ultra,
}

impl AoQuality {
    pub const ALL: [AoQuality; 4] = [AoQuality::Low, AoQuality::Medium, AoQuality::High, AoQuality::Ultra];

    pub fn label(self) -> &'static str {
        match self {
            AoQuality::Low => "Low",
            AoQuality::Medium => "Medium",
            AoQuality::High => "High",
            AoQuality::Ultra => "Ultra",
        }
    }

    pub fn from_label(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|q| q.label() == s)
    }
}

/// Render Setup > Renderer panel of the Bevy renderer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BevyRenderSettings {
    /// Antialiasing: `filter` over samples spread through each pixel (off:
    /// one sample at each pixel centre).
    pub antialiasing: bool,
    pub filter: AaFilter,
    /// Filter width in pixels (1–10).
    pub filter_size: f32,
    /// Each output pixel takes `supersampling`² samples per pass (1–4).
    pub supersampling: u32,
    /// Passes per bucket (1–64): each pass moves the samples within their
    /// pixel (and the lights over their Source Radius); the passes add up.
    pub passes: u32,
    /// The lights cast shadows.
    pub shadows: bool,
    /// Shadow map size in pixels (512–8192); renders give no light a smaller map.
    pub shadow_map_size: u32,
    /// Area Shadows: soft shadows from each light's Source Radius (over the passes).
    pub area_shadows: bool,
    /// Background colour, sRGB 0–1 (its alpha is 0: Display Alpha shows it black).
    pub background: [f32; 3],
    /// Ambient (environment) light colour, sRGB 0–1.
    pub ambient: [f32; 3],
    /// Ambient Occlusion: the ambient light darkens in creases and corners.
    pub ambient_occlusion: bool,
    pub ao_quality: AoQuality,
    /// How far around a point occluders darken it, in scene units.
    pub ao_radius: f32,
    /// Reflections: glossy surfaces reflect what the camera sees.
    pub reflections: bool,
    /// Sky Lighting: the ambient light comes from a sky dome (`ground` × as
    /// bright below the horizon) that glossy surfaces reflect, times `sky_intensity`.
    pub sky_lighting: bool,
    pub sky_intensity: f32,
    pub ground: f32,
    /// Exposure Control: tone mapping, exposure in stops, dithering.
    pub tonemapper: Tonemapper,
    pub exposure: f32,
    pub dither: bool,
    /// Options > Auto Reflect/Refract and Mirrors: Reflect/Refract and Flat
    /// Mirror maps render (off: they are left out).
    pub auto_reflect: bool,
    /// Auto Reflect/Refract Maps > Rendering Iterations (1-10): how many
    /// reflections deep a Reflect/Refract map sees.
    pub reflect_iterations: u32,
    /// Antialiasing > Filter Maps: bitmaps are filtered (off: nearest texel).
    pub filter_maps: bool,
    /// Global SuperSampling: Disable all Samplers, and Enable Global
    /// Supersampler (the Supersampling factor applies only with it on).
    pub disable_all_samplers: bool,
    pub global_supersampler: bool,
    /// Object Motion Blur: Apply, Duration (frames the shutter is open),
    /// Samples and Duration Subdivisions (1-32; Samples of the
    /// Subdivisions' time slices render).
    pub object_motion_blur: bool,
    pub object_blur_duration: f32,
    pub object_blur_samples: u32,
    pub object_blur_subdivisions: u32,
    /// Image Motion Blur: Apply, Duration, Apply to Environment Map.
    pub image_motion_blur: bool,
    pub image_blur_duration: f32,
    pub image_blur_environment: bool,
    /// Color Range Limiting: Clamp or Scale over-bright colours.
    pub color_range: ColorRange,
}

/// Color Range Limiting: what happens to colour components above 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ColorRange {
    /// Each component above 1 becomes 1 (bright colours go white).
    #[default]
    Clamp,
    /// All three scale down so the largest is 1 (the hue stays).
    Scale,
}

impl ColorRange {
    /// Linear colour `c` brought into 0-1.
    pub fn limit(self, c: [f32; 3]) -> [f32; 3] {
        match self {
            ColorRange::Clamp => c.map(|v| v.clamp(0.0, 1.0)),
            ColorRange::Scale => {
                let m = c[0].max(c[1]).max(c[2]);
                let k = if m > 1.0 { 1.0 / m } else { 1.0 };
                c.map(|v| (v * k).max(0.0))
            }
        }
    }
}

impl BevyRenderSettings {
    /// Samples per output pixel side: the Supersampling factor while Global
    /// SuperSampling is enabled and not disabled, else 1.
    pub fn samples_per_side(&self) -> u32 {
        if self.disable_all_samplers || !self.global_supersampler {
            1
        } else {
            self.supersampling.clamp(1, 4)
        }
    }

    /// The shutter times of Object Motion Blur around frame `frame`: Samples
    /// of the Duration Subdivisions' slices (spread evenly when fewer), each
    /// at its slice's centre, the shutter centred on the frame.
    pub fn object_blur_times(&self, frame: f32) -> Vec<f32> {
        let subs = self.object_blur_subdivisions.clamp(1, 32);
        let samples = self.object_blur_samples.clamp(1, 32).min(subs);
        let d = self.object_blur_duration.max(0.0);
        (0..samples)
            .map(|i| {
                let k = (i as u64 * subs as u64 / samples as u64) as u32;
                frame + d * ((k as f32 + 0.5) / subs as f32 - 0.5)
            })
            .collect()
    }
}

/// Raytracer Global Parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RaytracerSettings {
    /// Global Raytrace Engine Options > Enable Raytracing (off: the
    /// renderer's screen-space approximations instead).
    pub enabled: bool,
    /// Ray Depth Control: Maximum Depth (0-100), Cutoff Threshold (0-1),
    /// and the colour a ray returns at the depth (None: the background).
    pub max_depth: u32,
    pub cutoff: f32,
    pub max_depth_color: Option<[f32; 3]>,
    /// Global Ray Antialiaser: On, its rays per sample (2-64) and the blur
    /// spread of Raytrace reflections and refractions in degrees (0-45).
    pub antialias: bool,
    pub samples: u32,
    pub blur: f32,
    /// Enable Self Reflect/Refract.
    pub self_reflect: bool,
    /// Exclude: objects the rays do not see (node ids).
    pub exclude: Vec<u64>,
}

impl Default for RaytracerSettings {
    fn default() -> Self {
        Self { enabled: true, max_depth: 9, cutoff: 0.05, max_depth_color: None, antialias: false, samples: 8, blur: 0.0, self_reflect: true, exclude: Vec::new() }
    }
}

/// Advanced Lighting > Select Advanced Lighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AdvancedPlugin {
    #[default]
    None,
    LightTracer,
}

impl AdvancedPlugin {
    pub const ALL: [AdvancedPlugin; 2] = [AdvancedPlugin::None, AdvancedPlugin::LightTracer];

    pub fn label(self) -> &'static str {
        match self {
            AdvancedPlugin::None => "<no lighting plug-in>",
            AdvancedPlugin::LightTracer => "Light Tracer",
        }
    }
}

/// Advanced Lighting panel: the plug-in, Active and the Light Tracer's
/// Parameters; Common > Options > Use Advanced Lighting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AdvancedLighting {
    pub plugin: AdvancedPlugin,
    /// Select Advanced Lighting > Active.
    pub active: bool,
    /// Common > Options > Use Advanced Lighting.
    pub use_advanced: bool,
    /// Light Tracer General Settings.
    pub global_multiplier: f32,
    pub object_multiplier: f32,
    pub sky_lights_on: bool,
    pub sky_lights: f32,
    pub color_bleed: f32,
    pub rays_per_sample: u32,
    /// Color Filter and Extra Ambient (sRGB 0-1).
    pub color_filter: [f32; 3],
    pub extra_ambient: [f32; 3],
    /// Filter Size in pixels: the traced light is blurred this much.
    pub filter_size: f32,
    pub ray_bias: f32,
    pub bounces: u32,
    /// Cone Angle (33-90 degrees) the gathering rays stay within.
    pub cone_angle: f32,
}

impl Default for AdvancedLighting {
    fn default() -> Self {
        Self {
            plugin: AdvancedPlugin::None,
            active: true,
            use_advanced: true,
            global_multiplier: 1.0,
            object_multiplier: 1.0,
            sky_lights_on: true,
            sky_lights: 1.0,
            color_bleed: 1.0,
            rays_per_sample: 250,
            color_filter: [1.0; 3],
            extra_ambient: [0.0; 3],
            filter_size: 0.5,
            ray_bias: 0.03,
            bounces: 0,
            cone_angle: 88.0,
        }
    }
}

impl AdvancedLighting {
    /// Whether a render traces the Light Tracer's light.
    pub fn light_tracer(&self) -> bool {
        self.plugin == AdvancedPlugin::LightTracer && self.active && self.use_advanced
    }
}

impl Default for BevyRenderSettings {
    fn default() -> Self {
        Self {
            antialiasing: true,
            filter: AaFilter::Area,
            filter_size: 1.5,
            supersampling: 1,
            passes: 16,
            shadows: true,
            shadow_map_size: 4096,
            // The reference's defaults: shadow maps, black ambient (the
            // Environment's), no AO, reflections, sky light or exposure
            // control; each is a Renderer panel option.
            area_shadows: false,
            background: [0.0; 3],
            ambient: [0.0; 3],
            ambient_occlusion: false,
            ao_quality: AoQuality::High,
            ao_radius: 10.0,
            reflections: false,
            sky_lighting: false,
            sky_intensity: 1.0,
            ground: 0.5,
            tonemapper: Tonemapper::None,
            exposure: 0.0,
            dither: true,
            auto_reflect: true,
            reflect_iterations: 1,
            filter_maps: true,
            disable_all_samplers: false,
            global_supersampler: true,
            // The reference's defaults: both Apply on (only objects whose
            // Motion Blur property asks for it blur), Duration 0.5, 10 / 10.
            object_motion_blur: true,
            object_blur_duration: 0.5,
            object_blur_samples: 10,
            object_blur_subdivisions: 10,
            image_motion_blur: true,
            image_blur_duration: 0.5,
            image_blur_environment: false,
            color_range: ColorRange::Clamp,
        }
    }
}

#[cfg(test)]
mod time_output_tests {
    use super::*;

    #[test]
    fn frames_lists_take_frames_and_ranges() {
        assert_eq!(parse_frames("2,5").unwrap(), vec![2, 5]);
        assert_eq!(parse_frames("0-3, 7").unwrap(), vec![0, 1, 2, 3, 7]);
        assert_eq!(parse_frames("3-1").unwrap(), vec![3, 2, 1]);
        assert_eq!(parse_frames("-2--1").unwrap(), vec![-2, -1]);
        assert!(parse_frames("a").is_err());
        assert!(parse_frames(" , ").is_err());
    }

    #[test]
    fn time_output_frames_and_file_numbers() {
        let mut s = RenderSettings::default();
        assert_eq!(s.output_frames((0, 100), 7).unwrap(), vec![7]);
        s.time_output = TimeOutput::ActiveSegment;
        s.every_nth = 25;
        assert_eq!(s.output_frames((0, 100), 7).unwrap(), vec![0, 25, 50, 75, 100]);
        s.time_output = TimeOutput::Range;
        s.range = [0, 3];
        s.every_nth = 1;
        assert_eq!(s.output_frames((0, 100), 7).unwrap(), vec![0, 1, 2, 3]);
        // The reference's example: Range 0-3, base 15 gives file0015..file0018.
        s.file_number_base = 15;
        assert_eq!((0..=3).map(|f| s.file_number(f, 0)).collect::<Vec<_>>(), vec![15, 16, 17, 18]);
        s.file_number_base = 0;
        assert_eq!(s.file_number(52, 50), 52);
        s.range = [5, 1];
        assert!(s.output_frames((0, 100), 0).is_err());
        assert_eq!(numbered_path("/tmp/a/out.png", 7), "/tmp/a/out0007.png");
        assert_eq!(numbered_path("out.png", -50), "out-050.png");
    }
}

#[cfg(test)]
mod quality_tests {
    use super::*;

    #[test]
    fn filters_peak_at_the_centre_and_end_at_the_edge() {
        for f in AaFilter::ALL {
            assert!(f.weight(0.0) > 0.5, "{f:?}");
            assert_eq!(f.weight(1.0), 0.0, "{f:?}");
            assert!(f.weight(0.0) >= f.weight(0.5), "{f:?}");
            assert_eq!(AaFilter::from_label(f.label()), Some(f));
        }
        // Catmull-Rom sharpens with a negative lobe; the B-spline never goes negative.
        assert!(AaFilter::CatmullRom.weight(0.7) < 0.0);
        assert!((0..100).all(|i| AaFilter::Cubic.weight(i as f32 / 100.0) >= 0.0));
    }

    #[test]
    fn color_range_clamps_or_scales_over_bright_colours() {
        assert_eq!(ColorRange::Clamp.limit([2.0, 1.0, 0.5]), [1.0, 1.0, 0.5]);
        assert_eq!(ColorRange::Scale.limit([2.0, 1.0, 0.5]), [1.0, 0.5, 0.25]);
        assert_eq!(ColorRange::Scale.limit([0.5, 0.2, -0.1]), [0.5, 0.2, 0.0]);
    }

    #[test]
    fn old_scenes_load_with_the_new_quality_defaults() {
        let s: BevyRenderSettings = serde_json::from_str(r#"{"antialiasing":false,"supersampling":2}"#).unwrap();
        assert!(!s.antialiasing);
        assert_eq!(s.supersampling, 2);
        assert_eq!((s.passes, s.tonemapper, s.filter), (16, Tonemapper::None, AaFilter::Area));
    }
}

/// Render Elements: the kinds of image a render can also write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ElementKind {
    Alpha,
    Background,
    Diffuse,
    Lighting,
    SelfIllumination,
    Shadow,
    Specular,
    ZDepth,
    Reflection,
    Refraction,
    Velocity,
    /// A chosen combination of the shading parts.
    Blend,
    /// The light falling on the surfaces (material ignored), scaled.
    IlluminanceHdr,
    /// The rendered brightness (material included), scaled.
    LuminanceHdr,
    /// White where the chosen objects or G-buffer ID are.
    Matte,
    /// Each object's G-buffer Object Channel in its colour.
    ObjectId,
}

impl ElementKind {
    pub const ALL: [ElementKind; 16] = [
        ElementKind::Alpha,
        ElementKind::Background,
        ElementKind::Blend,
        ElementKind::Diffuse,
        ElementKind::IlluminanceHdr,
        ElementKind::Lighting,
        ElementKind::LuminanceHdr,
        ElementKind::Matte,
        ElementKind::ObjectId,
        ElementKind::Reflection,
        ElementKind::Refraction,
        ElementKind::SelfIllumination,
        ElementKind::Shadow,
        ElementKind::Specular,
        ElementKind::Velocity,
        ElementKind::ZDepth,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ElementKind::Alpha => "Alpha",
            ElementKind::Background => "Background",
            ElementKind::Diffuse => "Diffuse",
            ElementKind::Lighting => "Lighting",
            ElementKind::SelfIllumination => "Self-Illumination",
            ElementKind::Shadow => "Shadow",
            ElementKind::Specular => "Specular",
            ElementKind::ZDepth => "Z Depth",
            ElementKind::Reflection => "Reflection",
            ElementKind::Refraction => "Refraction",
            ElementKind::Velocity => "Velocity",
            ElementKind::Blend => "Blend",
            ElementKind::IlluminanceHdr => "Illuminance HDR Data",
            ElementKind::LuminanceHdr => "Luminance HDR Data",
            ElementKind::Matte => "Matte",
            ElementKind::ObjectId => "Object ID",
        }
    }
}

/// Blend element: the parts of the shading it adds up.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BlendParts {
    /// The lit diffuse colour (the ambient light included).
    pub diffuse: bool,
    pub specular: bool,
    pub self_illumination: bool,
    pub reflection: bool,
    pub refraction: bool,
    /// Apply Atmosphere / Apply Shadows.
    pub atmosphere: bool,
    pub shadows: bool,
}

impl Default for BlendParts {
    fn default() -> Self {
        Self { diffuse: true, specular: true, self_illumination: true, reflection: true, refraction: true, atmosphere: true, shadows: true }
    }
}

impl BlendParts {
    pub const NAMES: [&'static str; 7] = ["Diffuse", "Specular", "Self-Illumination", "Reflection", "Refraction", "Atmosphere", "Shadows"];

    pub fn get_mut(&mut self, name: &str) -> Option<&mut bool> {
        Some(match name {
            "Diffuse" => &mut self.diffuse,
            "Specular" => &mut self.specular,
            "Self-Illumination" => &mut self.self_illumination,
            "Reflection" => &mut self.reflection,
            "Refraction" => &mut self.refraction,
            "Atmosphere" => &mut self.atmosphere,
            "Shadows" => &mut self.shadows,
            _ => return None,
        })
    }
}

/// One entry of the Render Elements list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderElement {
    pub kind: ElementKind,
    pub name: String,
    pub enabled: bool,
    /// Its output file; None: the Render Output file's name with `_<name>`.
    pub path: Option<String>,
    /// Z Depth: the depth that is white and the one that is black.
    pub z_min: f32,
    pub z_max: f32,
    /// Diffuse: Lighting (off: the diffuse colour and maps before lighting).
    pub lighting: bool,
    /// Lighting: Direct Light On, Indirect Light On, Shadows On.
    pub direct: bool,
    pub indirect: bool,
    pub shadows: bool,
    /// Illuminance / Luminance HDR Data: Scale Factor.
    pub scale: f32,
    /// Matte: G-Buffer ID (None: off), the Include list (or with
    /// `matte_exclude` the Exclude list) of object ids.
    pub matte_gbuffer: Option<u32>,
    pub matte_objects: Vec<u64>,
    pub matte_exclude: bool,
    /// Blend: the parts included.
    pub blend: BlendParts,
}

impl Default for RenderElement {
    fn default() -> Self {
        Self {
            kind: ElementKind::Diffuse,
            name: "Diffuse".into(),
            enabled: true,
            path: None,
            z_min: 100.0,
            z_max: 300.0,
            lighting: true,
            direct: true,
            indirect: true,
            shadows: true,
            scale: 1.0,
            matte_gbuffer: None,
            matte_objects: Vec::new(),
            matte_exclude: false,
            blend: BlendParts::default(),
        }
    }
}

impl RenderElement {
    /// The file the element writes for a render saved to `output`.
    pub fn output_path(&self, output: Option<&str>) -> Option<String> {
        if let Some(p) = &self.path {
            return Some(p.clone());
        }
        let out = std::path::Path::new(output?);
        let stem = out.file_stem()?.to_string_lossy();
        let name = self.name.replace([' ', '/', '\\'], "");
        let file = match out.extension() {
            Some(e) => format!("{stem}_{name}.{}", e.to_string_lossy()),
            None => format!("{stem}_{name}"),
        };
        Some(out.with_file_name(file).to_string_lossy().into_owned())
    }
}

/// Common > Time Output: which frames a Render renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TimeOutput {
    /// The current frame.
    #[default]
    Single,
    /// The time slider's range.
    ActiveSegment,
    /// `RenderSettings::range`.
    Range,
    /// `RenderSettings::frames`: "1,3,5-12".
    Frames,
}

impl TimeOutput {
    pub const ALL: [TimeOutput; 4] = [TimeOutput::Single, TimeOutput::ActiveSegment, TimeOutput::Range, TimeOutput::Frames];

    pub fn label(self) -> &'static str {
        match self {
            TimeOutput::Single => "Single",
            TimeOutput::ActiveSegment => "Active Time Segment",
            TimeOutput::Range => "Range",
            TimeOutput::Frames => "Frames",
        }
    }
}

/// Frames list "2,5,7-9" (ranges either way round) in the order written.
pub fn parse_frames(s: &str) -> Result<Vec<i32>, String> {
    let mut out = Vec::new();
    for part in s.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        // A leading minus is a negative frame, not a range.
        let split = part.char_indices().skip(1).find(|(_, c)| *c == '-').map(|(i, _)| i);
        let num = |t: &str| t.trim().parse::<i32>().map_err(|_| format!("frames \"{s}\": \"{part}\" is not a frame or a range a-b"));
        match split {
            Some(i) => {
                let (a, b) = (num(&part[..i])?, num(&part[i + 1..])?);
                if a <= b {
                    out.extend(a..=b);
                } else {
                    out.extend((b..=a).rev());
                }
            }
            None => out.push(num(part)?),
        }
    }
    if out.is_empty() {
        return Err(format!("frames \"{s}\": no frame"));
    }
    Ok(out)
}

/// `path` with file number `n` before its extension: out.png -> out0007.png.
pub fn numbered_path(path: &str, n: i32) -> String {
    let p = std::path::Path::new(path);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let name = match p.extension().and_then(|e| e.to_str()) {
        Some(ext) => format!("{stem}{n:04}.{ext}"),
        None => format!("{stem}{n:04}"),
    };
    p.with_file_name(name).to_string_lossy().into_owned()
}

/// Render Setup dialog values (Common and Renderer panels).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderSettings {
    /// Output Size in pixels.
    pub width: u32,
    pub height: u32,
    /// Pixel Aspect: the shape of the output device's pixels.
    pub pixel_aspect: f32,
    /// Image Aspect lock: Width and Height change together.
    pub lock_image_aspect: bool,
    /// Pixel Aspect lock.
    pub lock_pixel_aspect: bool,
    /// Options > Render Hidden Geometry.
    pub render_hidden: bool,
    /// Options > Force 2-Sided.
    pub force_two_sided: bool,
    /// Options > Atmospherics: the Environment's atmosphere list renders.
    pub atmospherics: bool,
    /// Options > Effects: the rendering effects list applies.
    pub effects: bool,
    /// Render Output > Save File, and the file Files... chose.
    pub save_file: bool,
    pub output_path: Option<String>,
    /// Rendered Frame Window > Area to Render.
    pub area: RenderArea,
    /// Region and Crop rectangle: x0, y0, x1, y1 as fractions of the frame (y down).
    pub region: [f32; 4],
    /// Blowup rectangle (kept apart from Region/Crop, as the reference does).
    pub blowup: [f32; 4],
    pub bevy: BevyRenderSettings,
    /// Time Output: the frames, Range from-to, Frames list, Every Nth
    /// Frame (Active Time Segment and Range) and File Number Base (0: a
    /// file is numbered by its frame, else the first file has this number).
    pub time_output: TimeOutput,
    pub range: [i32; 2],
    pub frames: String,
    pub every_nth: u32,
    pub file_number_base: i32,
    /// Render Output > Skip Existing Images (with Save File, a sequence
    /// leaves out the frames whose file exists).
    pub skip_existing: bool,
    /// Render Output > Put Image File List in Output Path: a sequence also
    /// writes a .ifl next to its frames, listing them.
    pub image_file_list: bool,
    /// Options > Video Color Check: pixels beyond the NTSC limit render black.
    pub video_color_check: bool,
    /// Options > Super Black: rendered geometry is no darker than the
    /// threshold (0-255; the background stays black for keying).
    pub super_black: bool,
    pub super_black_threshold: u8,
    /// Renderer > Options: Force Wireframe draws every surface as its
    /// polygon edges, Wire Thickness pixels wide.
    pub force_wireframe: bool,
    pub wire_thickness: f32,
    /// Render Elements: the list, Elements Active (they render) and
    /// Display Elements (each shows in its own window).
    pub elements: Vec<RenderElement>,
    pub elements_active: bool,
    pub display_elements: bool,
    /// Render To Texture.
    pub bake: crate::bake::BakeSettings,
    /// Raytracer panel > Raytracer Global Parameters.
    pub raytracer: RaytracerSettings,
    /// Advanced Lighting panel, and Common > Options > Use Advanced Lighting.
    pub advanced: AdvancedLighting,
    /// Common > Options > Area Lights/Shadows as Points: area and soft
    /// ray-traced shadows render sharp, from the light's centre.
    pub area_as_points: bool,
    /// Render Output File > Setup: the formats' options.
    pub output_format: OutputFormat,
    /// Gamma and LUT, and the output's Gamma group.
    pub gamma: GammaSettings,
    /// Output Size format (None: Custom) and Aperture Width in millimetres.
    pub film_format: Option<String>,
    pub aperture_width: f32,
    /// Rendering > Batch Render: the task queue (saved with the scene).
    pub batch: Vec<BatchJob>,
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            width: 640,
            height: 480,
            pixel_aspect: 1.0,
            lock_image_aspect: false,
            lock_pixel_aspect: false,
            render_hidden: false,
            force_two_sided: false,
            atmospherics: true,
            effects: true,
            save_file: false,
            output_path: None,
            area: RenderArea::View,
            region: [0.25, 0.25, 0.75, 0.75],
            blowup: [0.25, 0.25, 0.75, 0.75],
            bevy: BevyRenderSettings::default(),
            time_output: TimeOutput::Single,
            range: [0, 100],
            frames: "1,3,5-12".into(),
            every_nth: 1,
            file_number_base: 0,
            skip_existing: false,
            image_file_list: false,
            video_color_check: false,
            super_black: false,
            super_black_threshold: 15,
            force_wireframe: false,
            wire_thickness: 1.0,
            elements: Vec::new(),
            elements_active: true,
            display_elements: true,
            bake: crate::bake::BakeSettings::default(),
            raytracer: RaytracerSettings::default(),
            advanced: AdvancedLighting::default(),
            area_as_points: false,
            output_format: OutputFormat::default(),
            gamma: GammaSettings::default(),
            film_format: None,
            aperture_width: DEFAULT_APERTURE,
            batch: Vec::new(),
        }
    }
}

impl RenderSettings {
    /// Image Aspect: width / height of the picture on the output device.
    pub fn image_aspect(&self) -> f32 {
        self.width as f32 * self.pixel_aspect / self.height.max(1) as f32
    }

    /// The frames Time Output renders, in order: `active` is the time
    /// slider's range, `current` the current frame.
    pub fn output_frames(&self, active: (i32, i32), current: i32) -> Result<Vec<i32>, String> {
        let nth = self.every_nth.max(1) as usize;
        match self.time_output {
            TimeOutput::Single => Ok(vec![current]),
            TimeOutput::ActiveSegment => Ok((active.0..=active.1).step_by(nth).collect()),
            TimeOutput::Range => {
                let [a, b] = self.range;
                if a > b {
                    return Err(format!("range {a} to {b}: the first frame is after the last"));
                }
                Ok((a..=b).step_by(nth).collect())
            }
            TimeOutput::Frames => parse_frames(&self.frames),
        }
    }

    /// File number of `frame` in a sequence whose first frame is `first`
    /// (File Number Base applies to Active Time Segment and Range).
    pub fn file_number(&self, frame: i32, first: i32) -> i32 {
        match self.time_output {
            TimeOutput::ActiveSegment | TimeOutput::Range if self.file_number_base != 0 => self.file_number_base + (frame - first),
            _ => frame,
        }
    }

    /// A render preset's categories taken from `from`: Common (output, time,
    /// options, area), Renderer and Render Elements; Render To Texture stays.
    pub fn take_categories(&mut self, from: &RenderSettings, common: bool, renderer: bool, elements: bool) {
        if common {
            let (bevy, wire, thickness, list, active, display, bake) = (self.bevy.clone(), self.force_wireframe, self.wire_thickness, self.elements.clone(), self.elements_active, self.display_elements, self.bake.clone());
            let (raytracer, advanced) = (self.raytracer.clone(), self.advanced.clone());
            // The batch queue and the gamma preferences are not a preset's.
            let (batch, gamma) = (std::mem::take(&mut self.batch), self.gamma.clone());
            *self = from.clone();
            (self.batch, self.gamma) = (batch, gamma);
            (self.bevy, self.force_wireframe, self.wire_thickness, self.elements, self.elements_active, self.display_elements, self.bake) = (bevy, wire, thickness, list, active, display, bake);
            (self.raytracer, self.advanced) = (raytracer, advanced);
        }
        if renderer {
            // The renderer's own panels: Renderer, Raytracer, Advanced Lighting.
            self.raytracer = from.raytracer.clone();
            self.advanced = from.advanced.clone();
            self.bevy = from.bevy.clone();
            self.force_wireframe = from.force_wireframe;
            self.wire_thickness = from.wire_thickness;
        }
        if elements {
            self.elements = from.elements.clone();
            self.elements_active = from.elements_active;
            self.display_elements = from.display_elements;
        }
    }

    /// The rectangle the current area uses (Blowup has its own).
    pub fn area_rect(&self) -> [f32; 4] {
        if self.area == RenderArea::Blowup {
            self.blowup
        } else {
            self.region
        }
    }
}

/// PNG Configuration: the colour type a .png is written with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PngType {
    /// RGB 24 bit.
    #[default]
    Rgb24,
    /// RGB 48 bit.
    Rgb48,
    /// Grayscale 8 bit.
    Gray8,
    /// Grayscale 16 bit.
    Gray16,
}

/// Render Output File > Setup: each file format's own options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputFormat {
    /// JPEG Image Control > Quality, 1 (smallest file) to 100 (best).
    pub jpeg_quality: u8,
    /// PNG Configuration: colour type and Alpha Channel.
    pub png_type: PngType,
    pub png_alpha: bool,
    /// Targa Image Control: Bits-Per-Pixel 24 or 32 (with alpha), Compress
    /// (run-length), Alpha Split (the alpha also to a_<name>.tga) and
    /// Pre-Multiplied Alpha.
    pub tga_bits: u32,
    pub tga_compress: bool,
    pub tga_alpha_split: bool,
    pub tga_premultiplied: bool,
    /// OpenEXR Configuration: Format Half Float (16 bits, the default) or
    /// Full Float (32 bits); Type RGBA (off: RGB, no alpha).
    pub exr_full_float: bool,
    pub exr_alpha: bool,
}

impl Default for OutputFormat {
    fn default() -> Self {
        Self { jpeg_quality: 75, png_type: PngType::Rgb24, png_alpha: true, tga_bits: 32, tga_compress: true, tga_alpha_split: false, tga_premultiplied: false, exr_full_float: false, exr_alpha: true }
    }
}

/// The gamma the rendered image is encoded for: the renderer writes sRGB,
/// which a 2.2 display shows as intended.
pub const DISPLAY_GAMMA: f32 = 2.2;

/// Preferences > Gamma and LUT (saved with the scene, as the reference
/// does) and the Render Output File dialog's Gamma group.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GammaSettings {
    /// Enable Gamma/LUT Correction: off, files are written as rendered.
    pub enabled: bool,
    /// Bitmap Files > Output Gamma (the system default for saved renders).
    pub output: f32,
    /// Render Output File > Gamma > Override: this output's own gamma
    /// (None: Use System Default Gamma).
    pub override_output: Option<f32>,
}

impl Default for GammaSettings {
    fn default() -> Self {
        Self { enabled: false, output: DISPLAY_GAMMA, override_output: None }
    }
}

impl GammaSettings {
    /// The gamma a saved render is written with, None when no correction applies.
    pub fn output_gamma(&self) -> Option<f32> {
        self.enabled.then(|| self.override_output.unwrap_or(self.output))
    }

    /// One 8-bit channel re-encoded from the display gamma to `gamma`
    /// (output = intensity ^ (1 / gamma); 2.2 leaves it as rendered).
    pub fn encode(value: u8, gamma: f32) -> u8 {
        let linear = (value as f32 / 255.0).powf(DISPLAY_GAMMA);
        (linear.powf(1.0 / gamma.max(0.01)) * 255.0).round().clamp(0.0, 255.0) as u8
    }
}

/// Common > Output Size: a film or video format. Aspect ratios are the
/// ones the formats are named by; apertures are the formats' published
/// camera gate widths; video sizes are the standards' frame sizes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FilmFormat {
    pub name: &'static str,
    /// Aperture Width in millimetres (None: the format leaves it).
    pub aperture: Option<f32>,
    /// Image aspect (width / height of the picture).
    pub image_aspect: f32,
    pub pixel_aspect: f32,
    /// A fixed frame size (video); else the width is kept and the height follows.
    pub size: Option<[u32; 2]>,
}

pub const FILM_FORMATS: &[FilmFormat] = &[
    FilmFormat { name: "35mm 1.316:1 Full Aperture (cine)", aperture: Some(24.892), image_aspect: 1.316, pixel_aspect: 1.0, size: None },
    FilmFormat { name: "35mm 1.37:1 Academy (cine)", aperture: Some(21.946), image_aspect: 1.37, pixel_aspect: 1.0, size: None },
    FilmFormat { name: "35mm 1.66:1 (cine)", aperture: Some(21.946), image_aspect: 1.66, pixel_aspect: 1.0, size: None },
    FilmFormat { name: "35mm 1.75:1 (cine)", aperture: Some(21.946), image_aspect: 1.75, pixel_aspect: 1.0, size: None },
    FilmFormat { name: "35mm 1.85:1 (cine)", aperture: Some(21.946), image_aspect: 1.85, pixel_aspect: 1.0, size: None },
    FilmFormat { name: "35mm Anamorphic (2.35:1)", aperture: Some(21.946), image_aspect: 2.35, pixel_aspect: 1.0, size: None },
    FilmFormat { name: "35mm Anamorphic (2.35:1) (Squeezed)", aperture: Some(21.946), image_aspect: 2.35, pixel_aspect: 2.0, size: None },
    FilmFormat { name: "70mm Panavision (cine)", aperture: Some(52.476), image_aspect: 2.2, pixel_aspect: 1.0, size: None },
    FilmFormat { name: "70mm IMAX (cine)", aperture: Some(70.409), image_aspect: 1.43, pixel_aspect: 1.0, size: None },
    FilmFormat { name: "VistaVision", aperture: Some(37.719), image_aspect: 1.5, pixel_aspect: 1.0, size: None },
    FilmFormat { name: "35mm (24mm X 36mm) (slide)", aperture: Some(36.0), image_aspect: 1.5, pixel_aspect: 1.0, size: None },
    FilmFormat { name: "6cm X 6cm (2 1/4\" X 2 1/4\") (slide)", aperture: Some(56.0), image_aspect: 1.0, pixel_aspect: 1.0, size: None },
    FilmFormat { name: "NTSC D-1 (video)", aperture: None, image_aspect: 4.0 / 3.0, pixel_aspect: 0.9, size: Some([720, 486]) },
    FilmFormat { name: "NTSC DV (video)", aperture: None, image_aspect: 4.0 / 3.0, pixel_aspect: 8.0 / 9.0, size: Some([720, 480]) },
    FilmFormat { name: "PAL (video)", aperture: None, image_aspect: 4.0 / 3.0, pixel_aspect: 16.0 / 15.0, size: Some([720, 576]) },
    FilmFormat { name: "PAL DV (video)", aperture: None, image_aspect: 4.0 / 3.0, pixel_aspect: 16.0 / 15.0, size: Some([720, 576]) },
    FilmFormat { name: "HDTV (video)", aperture: None, image_aspect: 16.0 / 9.0, pixel_aspect: 1.0, size: Some([1920, 1080]) },
    FilmFormat { name: "HDTV 720 (video)", aperture: None, image_aspect: 16.0 / 9.0, pixel_aspect: 1.0, size: Some([1280, 720]) },
];

/// Aperture Width of Custom: the 36 mm film the Lens is measured against.
pub const DEFAULT_APERTURE: f32 = 36.0;

pub fn film_format(name: &str) -> Option<&'static FilmFormat> {
    FILM_FORMATS.iter().find(|f| f.name.eq_ignore_ascii_case(name.trim()))
}

/// One Batch Render task: what it renders (a camera, else the render
/// viewport), the render preset it loads, Override Preset's frames and size,
/// and the output file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BatchJob {
    pub name: String,
    pub enabled: bool,
    /// Camera node id (None: the viewport).
    pub camera: Option<u64>,
    /// A .rps render preset the task renders with.
    pub preset: Option<String>,
    /// Override Preset: Frame Start / End, Width, Height, Pixel Aspect.
    pub override_preset: bool,
    pub frames: [i32; 2],
    pub width: u32,
    pub height: u32,
    pub pixel_aspect: f32,
    /// Output Path (a sequence numbers its files); None: not saved.
    pub output: Option<String>,
}

impl Default for BatchJob {
    fn default() -> Self {
        Self { name: "View01".into(), enabled: true, camera: None, preset: None, override_preset: false, frames: [0, 0], width: 640, height: 480, pixel_aspect: 1.0, output: None }
    }
}
