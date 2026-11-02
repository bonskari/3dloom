//! Rendering > Environment and Effects, saved with the scene: the
//! background, global lighting, exposure control, the atmosphere list and
//! the rendering effects list. Colours are sRGB 0–1; the values the
//! reference animates are `Param`s.

use crate::node::NodeId;
use crate::param::Param;
use glam::Vec3;
use serde::{Deserialize, Serialize};

fn p<T: crate::param::Animatable>(v: T) -> Param<T> {
    Param::new(v)
}

/// The coordinates an environment map uses (Bitmap > Coordinates > Environ).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum EnvMapping {
    /// Stretched over the frame.
    #[default]
    Screen,
    /// Latitude-longitude around the world Z axis.
    Spherical,
    Cylindrical,
    ShrinkWrap,
}

impl EnvMapping {
    pub const ALL: [EnvMapping; 4] = [EnvMapping::Screen, EnvMapping::Spherical, EnvMapping::Cylindrical, EnvMapping::ShrinkWrap];

    pub fn label(self) -> &'static str {
        match self {
            EnvMapping::Screen => "Screen",
            EnvMapping::Spherical => "Spherical Environment",
            EnvMapping::Cylindrical => "Cylindrical Environment",
            EnvMapping::ShrinkWrap => "Shrink-wrap Environment",
        }
    }
}

/// Exposure Control drop-down.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ExposureKind {
    /// <no exposure control>
    #[default]
    None,
    Automatic,
    Linear,
    Logarithmic,
}

impl ExposureKind {
    pub const ALL: [ExposureKind; 4] = [ExposureKind::None, ExposureKind::Automatic, ExposureKind::Linear, ExposureKind::Logarithmic];

    pub fn label(self) -> &'static str {
        match self {
            ExposureKind::None => "<no exposure control>",
            ExposureKind::Automatic => "Automatic Exposure Control",
            ExposureKind::Linear => "Linear Exposure Control",
            ExposureKind::Logarithmic => "Logarithmic Exposure Control",
        }
    }
}

/// Exposure Control rollout and the chosen control's parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExposureControl {
    pub kind: ExposureKind,
    pub active: bool,
    /// Process Background and Environment Maps.
    pub process_background: bool,
    /// 0–100 (Logarithmic 0–200), default 50.
    pub brightness: Param<f32>,
    /// 0–100, default 50.
    pub contrast: Param<f32>,
    /// Automatic / Linear: −5 to 5 stops.
    pub exposure_value: Param<f32>,
    /// Logarithmic: 0.01–20, default 1.
    pub mid_tones: Param<f32>,
}

impl Default for ExposureControl {
    fn default() -> Self {
        Self { kind: ExposureKind::None, active: true, process_background: false, brightness: p(50.0), contrast: p(50.0), exposure_value: p(0.0), mid_tones: p(1.0) }
    }
}

/// Fog > Layered > Falloff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LayerFalloff {
    Top,
    Bottom,
    #[default]
    None,
}

/// Fog Parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Fog {
    pub color: Param<Vec3>,
    pub fog_background: bool,
    /// Type: Standard (false) or Layered.
    pub layered: bool,
    /// Standard: density grows exponentially with distance instead of linearly.
    pub exponential: bool,
    /// Standard: density (%) at the near and far environment range.
    pub near: Param<f32>,
    pub far: Param<f32>,
    /// Layered: the layer's extent in world units and its density (%).
    pub top: Param<f32>,
    pub bottom: Param<f32>,
    pub density: Param<f32>,
    pub falloff: LayerFalloff,
}

impl Default for Fog {
    fn default() -> Self {
        Self { color: p(Vec3::ONE), fog_background: true, layered: false, exponential: false, near: p(0.0), far: p(100.0), top: p(50.0), bottom: p(0.0), density: p(50.0), falloff: LayerFalloff::None }
    }
}

/// Volume Fog Parameters: without gizmos the fog fills the scene; with
/// atmospheric apparatus picked it fills them. Without noise its density is
/// even, so it is drawn exactly, with no steps to set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VolumeFog {
    pub color: Param<Vec3>,
    /// 0–20.
    pub density: Param<f32>,
    pub fog_background: bool,
    /// Gizmos > Pick Gizmo: atmospheric apparatus nodes.
    pub gizmos: Vec<NodeId>,
}

impl Default for VolumeFog {
    fn default() -> Self {
        Self { color: p(Vec3::ONE), density: p(5.0), fog_background: true, gizmos: Vec::new() }
    }
}

/// Fire Effect Parameters: flames, fireballs or an explosion inside the
/// picked atmospheric apparatus (nothing renders without one).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FireEffect {
    pub gizmos: Vec<NodeId>,
    pub inner_color: Param<Vec3>,
    pub outer_color: Param<Vec3>,
    pub smoke_color: Param<Vec3>,
    /// Shape: Tendril (true) or Fireball.
    pub tendril: bool,
    pub stretch: Param<f32>,
    /// 0–1.
    pub regularity: Param<f32>,
    pub flame_size: Param<f32>,
    /// 0–10.
    pub flame_detail: Param<f32>,
    pub density: Param<f32>,
    pub samples: Param<u32>,
    pub phase: Param<f32>,
    pub drift: Param<f32>,
    pub explosion: bool,
    pub smoke: bool,
    pub fury: Param<f32>,
}

impl Default for FireEffect {
    fn default() -> Self {
        let c = |r: f32, g: f32, b: f32| p(Vec3::new(r, g, b) / 255.0);
        Self {
            gizmos: Vec::new(),
            inner_color: c(252.0, 202.0, 0.0),
            outer_color: c(225.0, 30.0, 30.0),
            smoke_color: c(26.0, 26.0, 26.0),
            // Flame Type: Fireball.
            tendril: false,
            stretch: p(1.0),
            regularity: p(0.2),
            flame_size: p(35.0),
            flame_detail: p(3.0),
            density: p(15.0),
            samples: p(15),
            phase: p(0.0),
            drift: p(0.0),
            explosion: false,
            smoke: true,
            fury: p(1.0),
        }
    }
}

/// Volume Light Parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VolumeLight {
    /// Pick Light: the lights whose volume glows.
    pub lights: Vec<NodeId>,
    pub fog_color: Param<Vec3>,
    /// Density in percent (2–6 looks natural).
    pub density: Param<f32>,
}

impl Default for VolumeLight {
    fn default() -> Self {
        Self { lights: Vec::new(), fog_color: p(Vec3::ONE), density: p(5.0) }
    }
}

/// File Output Parameters: the image as it is at this point of the
/// effects list goes to a file. Channel: 0 Whole Image, 1 Luminance,
/// 2 Alpha; Affect Source Bitmap passes that channel on as the image.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileOutput {
    pub path: Option<String>,
    pub channel: u32,
    pub affect_source: bool,
}

impl FileOutput {
    pub const CHANNELS: [&'static str; 3] = ["Whole Image", "Luminance", "Alpha"];
}

// A handful of entries in a list: boxing the larger kinds buys nothing.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AtmosphereKind {
    Fire(FireEffect),
    Fog(Fog),
    VolumeFog(VolumeFog),
    VolumeLight(VolumeLight),
}

impl AtmosphereKind {
    /// Add Atmospheric Effect list names.
    pub const NAMES: [&'static str; 4] = ["Fire Effect", "Fog", "Volume Fog", "Volume Light"];
    /// The kinds an atmospheric apparatus takes (Add Atmosphere dialog).
    pub const GIZMO_NAMES: [&'static str; 2] = ["Fire Effect", "Volume Fog"];

    pub fn new(name: &str) -> Option<Self> {
        match name {
            "Fire Effect" => Some(Self::Fire(FireEffect::default())),
            "Fog" => Some(Self::Fog(Fog::default())),
            "Volume Fog" => Some(Self::VolumeFog(VolumeFog::default())),
            "Volume Light" => Some(Self::VolumeLight(VolumeLight::default())),
            _ => None,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Fire(_) => "Fire Effect",
            Self::Fog(_) => "Fog",
            Self::VolumeFog(_) => "Volume Fog",
            Self::VolumeLight(_) => "Volume Light",
        }
    }

    /// The atmospheric apparatus a Fire Effect or Volume Fog fills.
    pub fn gizmos(&self) -> Option<&Vec<NodeId>> {
        match self {
            Self::Fire(f) => Some(&f.gizmos),
            Self::VolumeFog(v) => Some(&v.gizmos),
            _ => None,
        }
    }

    pub fn gizmos_mut(&mut self) -> Option<&mut Vec<NodeId>> {
        match self {
            Self::Fire(f) => Some(&mut f.gizmos),
            Self::VolumeFog(v) => Some(&mut v.gizmos),
            _ => None,
        }
    }
}

/// One entry of the Atmosphere list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Atmosphere {
    pub name: String,
    pub active: bool,
    pub kind: AtmosphereKind,
}

/// Lens Effects Glow (Image source Whole, Bright filter).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Glow {
    /// Size as a percentage of the image's diagonal.
    pub size: Param<f32>,
    /// Brightness and opacity, percent.
    pub intensity: Param<f32>,
    /// 0: the radial colours only; 100: the glowing pixels' own colour.
    pub use_source_color: Param<f32>,
    /// Radial Color: inner (at the source) and outer.
    pub inner_color: Param<Vec3>,
    pub outer_color: Param<Vec3>,
    /// Image Filters > Bright: pixels brighter than this (0–255) glow.
    pub bright: Param<f32>,
    /// The Star and Streak elements of the same Lens Effects entry (Image
    /// Centers: one per bright area, at its centre).
    pub star: StarElement,
    pub streak: StarElement,
    /// Ray element: `qty` (Num) thin rays, randomly spaced, of random lengths.
    pub ray: StarElement,
    pub ring: RingElement,
    /// Lens Effects Globals > Parameters: Size and Intensity (%) scale every
    /// element, Angle turns the Star / Streak / Ray spokes, Seed varies the
    /// random elements (Ray, Auto Secondary).
    pub globals: LensGlobals,
    pub manual_secondary: ManualSecondary,
    pub auto_secondary: AutoSecondary,
}

impl Default for Glow {
    fn default() -> Self {
        Self {
            size: p(5.0),
            intensity: p(100.0),
            use_source_color: p(100.0),
            inner_color: p(Vec3::ONE),
            outer_color: p(Vec3::ONE),
            bright: p(200.0),
            star: StarElement::star(),
            streak: StarElement::streak(),
            ray: StarElement::ray(),
            ring: RingElement::default(),
            globals: LensGlobals::default(),
            manual_secondary: ManualSecondary::default(),
            auto_secondary: AutoSecondary::default(),
        }
    }
}

/// Lens Effects Globals > Parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LensGlobals {
    /// % applied to every element's size.
    pub size: Param<f32>,
    /// % applied to every element's intensity.
    pub intensity: Param<f32>,
    pub seed: u32,
    /// Degrees added to the elements' angles.
    pub angle: Param<f32>,
}

impl Default for LensGlobals {
    fn default() -> Self {
        Self { size: p(100.0), intensity: p(100.0), seed: 1357, angle: p(0.0) }
    }
}

/// Manual Secondary element: one flare disc on the line from the source
/// through the image centre.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ManualSecondary {
    pub on: bool,
    /// Radius, % of the image diagonal.
    pub size: Param<f32>,
    pub intensity: Param<f32>,
    /// % of the way from the source to the image centre (200: mirrored through it).
    pub plane: Param<f32>,
    pub use_source_color: Param<f32>,
    /// 0 circular, else 3 to 8 sides.
    pub sides: u32,
    pub inner_color: Param<Vec3>,
    pub outer_color: Param<Vec3>,
}

impl Default for ManualSecondary {
    /// Intensity, Plane, Use Source Color and Sides as the reference's
    /// rollout; Size is this build's (its scale differs from the reference's).
    fn default() -> Self {
        Self { on: false, size: p(5.0), intensity: p(30.0), plane: p(150.0), use_source_color: p(20.0), sides: 0, inner_color: p(Vec3::ONE), outer_color: p(Vec3::ONE) }
    }
}

/// Auto Secondary element: Qty flare discs of sizes between Min and Max
/// spread along the line from the source through the image centre; Axis
/// sets how far (1: to the source mirrored through the centre).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoSecondary {
    pub on: bool,
    /// Smallest and largest radius, % of the image diagonal.
    pub min: Param<f32>,
    pub max: Param<f32>,
    /// 0 to 10.
    pub axis: Param<f32>,
    pub intensity: Param<f32>,
    pub qty: u32,
    pub use_source_color: Param<f32>,
    /// 0 circular, else 3 to 8 sides.
    pub sides: u32,
    pub inner_color: Param<Vec3>,
    pub outer_color: Param<Vec3>,
}

impl Default for AutoSecondary {
    /// Intensity, Qty, Use Source Color and Sides as the reference's rollout;
    /// Min, Max and Axis are this build's (their scale differs from the
    /// reference's).
    fn default() -> Self {
        Self { on: false, min: p(1.0), max: p(5.0), axis: p(1.0), intensity: p(30.0), qty: 12, use_source_color: p(20.0), sides: 0, inner_color: p(Vec3::ONE), outer_color: p(Vec3::ONE) }
    }
}

/// Lens Effects Star / Streak element: spokes from each bright area's centre
/// (a Streak is a band through it: two opposite spokes).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StarElement {
    pub on: bool,
    /// Spoke length, % of the image diagonal.
    pub size: Param<f32>,
    /// Brightness and opacity, percent.
    pub intensity: Param<f32>,
    /// Spoke width, % of the frame (its diagonal).
    pub width: Param<f32>,
    /// Starting angle of the spokes, degrees.
    pub angle: Param<f32>,
    /// Tip width as % of the base width: 0 a sharp point, over 100 flared.
    pub taper: Param<f32>,
    /// Sharpness 0-10: higher is crisper, lower glows.
    pub sharp: Param<f32>,
    /// Number of spokes (Star only; 6 by default).
    pub qty: u32,
    pub use_source_color: Param<f32>,
    pub inner_color: Param<Vec3>,
    pub outer_color: Param<Vec3>,
}

impl StarElement {
    /// This build's starting values (the reference documents only Qty 6).
    pub fn star() -> Self {
        Self { on: false, size: p(10.0), intensity: p(100.0), width: p(1.0), angle: p(0.0), taper: p(0.0), sharp: p(8.0), qty: 6, use_source_color: p(0.0), inner_color: p(Vec3::ONE), outer_color: p(Vec3::ONE) }
    }

    pub fn streak() -> Self {
        Self { size: p(30.0), width: p(0.5), taper: p(100.0), qty: 2, ..Self::star() }
    }

    pub fn ray() -> Self {
        Self { size: p(20.0), width: p(0.15), qty: 100, ..Self::star() }
    }
}

/// Lens Effects Ring element: a colour band round each bright area's centre.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RingElement {
    pub on: bool,
    /// Radius, % of the image diagonal.
    pub size: Param<f32>,
    pub intensity: Param<f32>,
    /// Where the ring sits on the line from the source through the image
    /// centre: % of that distance (0 at the source; this build's unit).
    pub plane: Param<f32>,
    /// Band thickness, pixels.
    pub thickness: Param<f32>,
    pub use_source_color: Param<f32>,
    pub inner_color: Param<Vec3>,
    pub outer_color: Param<Vec3>,
}

impl Default for RingElement {
    /// This build's starting values.
    fn default() -> Self {
        Self { on: false, size: p(5.0), intensity: p(100.0), plane: p(0.0), thickness: p(4.0), use_source_color: p(0.0), inner_color: p(Vec3::ONE), outer_color: p(Vec3::ONE) }
    }
}

impl Default for StarElement {
    fn default() -> Self {
        Self::star()
    }
}

/// Blur > Blur Type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BlurType {
    #[default]
    Uniform,
    Directional,
    Radial,
}

/// Blur Parameters (Pixel Selections: Whole Image or Non-Background).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Blur {
    pub blur_type: BlurType,
    /// Uniform and Radial Pixel Radius, in pixels.
    pub radius: Param<f32>,
    /// Directional: U / V Pixel Radius (pixels), Trail (−100 to 100), Rotation (degrees).
    pub u_radius: Param<f32>,
    pub v_radius: Param<f32>,
    pub u_trail: Param<f32>,
    pub v_trail: Param<f32>,
    pub rotation: Param<f32>,
    /// Radial: Trail (−100 to 100) and the X / Y Origin in output pixels.
    pub radial_trail: Param<f32>,
    pub origin: [f32; 2],
    pub affect_alpha: bool,
    /// Pixel Selections: Non-Background instead of Whole Image.
    pub non_background: bool,
    /// Brighten and Blend (percent) of the selection.
    pub brighten: Param<f32>,
    pub blend: Param<f32>,
}

impl Default for Blur {
    fn default() -> Self {
        Self {
            blur_type: BlurType::Uniform,
            radius: p(2.0),
            u_radius: p(2.0),
            v_radius: p(2.0),
            u_trail: p(0.0),
            v_trail: p(0.0),
            rotation: p(0.0),
            radial_trail: p(0.0),
            origin: [320.0, 240.0],
            affect_alpha: true,
            non_background: false,
            brighten: p(0.0),
            blend: p(0.0),
        }
    }
}

/// Brightness and Contrast Parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BrightnessContrast {
    /// 0–1, 0.5 leaves the image as it is.
    pub brightness: Param<f32>,
    pub contrast: Param<f32>,
    pub ignore_background: bool,
}

impl Default for BrightnessContrast {
    fn default() -> Self {
        Self { brightness: p(0.5), contrast: p(0.5), ignore_background: false }
    }
}

/// Color Balance Parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorBalance {
    /// −100 to 100 each.
    pub cyan_red: Param<f32>,
    pub magenta_green: Param<f32>,
    pub yellow_blue: Param<f32>,
    pub preserve_luminosity: bool,
    pub ignore_background: bool,
}

impl Default for ColorBalance {
    fn default() -> Self {
        Self { cyan_red: p(0.0), magenta_green: p(0.0), yellow_blue: p(0.0), preserve_luminosity: false, ignore_background: false }
    }
}

/// Film Grain Parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FilmGrain {
    /// 0–10.
    pub grain: Param<f32>,
    pub ignore_background: bool,
}

impl Default for FilmGrain {
    fn default() -> Self {
        Self { grain: p(0.2), ignore_background: false }
    }
}

/// Motion Blur Parameters: the objects with Image Motion Blur smeared over
/// Duration frames (the virtual shutter) at this place in the effects list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MotionBlurEffect {
    pub duration: Param<f32>,
}

impl Default for MotionBlurEffect {
    fn default() -> Self {
        Self { duration: p(1.0) }
    }
}

/// Depth of Field Parameters (Focal Parameters: Custom).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DepthOfField {
    /// Focal Point: Pick Node; None uses the camera's target distance.
    pub focal_node: Option<NodeId>,
    /// Horiz / Vert Focal Loss: the largest blur, in pixels.
    pub horiz_loss: Param<f32>,
    pub vert_loss: Param<f32>,
    /// Focal Range: in focus this far either side of the focal point (world units).
    pub focal_range: Param<f32>,
    /// Focal Limit: the blur is at its largest this far either side.
    pub focal_limit: Param<f32>,
}

impl Default for DepthOfField {
    fn default() -> Self {
        Self { focal_node: None, horiz_loss: p(10.0), vert_loss: p(10.0), focal_range: p(100.0), focal_limit: p(200.0) }
    }
}

// A handful of entries in a list: boxing the larger kinds buys nothing.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EffectKind {
    Glow(Glow),
    Blur(Blur),
    BrightnessContrast(BrightnessContrast),
    ColorBalance(ColorBalance),
    FilmGrain(FilmGrain),
    DepthOfField(DepthOfField),
    FileOutput(FileOutput),
    MotionBlur(MotionBlurEffect),
}

impl EffectKind {
    /// Add Effect list names.
    pub const NAMES: [&'static str; 8] = ["Lens Effects", "Blur", "Brightness and Contrast", "Color Balance", "File Output", "Film Grain", "Motion Blur", "Depth of Field"];

    pub fn new(name: &str) -> Option<Self> {
        match name {
            "Lens Effects" | "Glow" => Some(Self::Glow(Glow::default())),
            "Blur" => Some(Self::Blur(Blur::default())),
            "Brightness and Contrast" => Some(Self::BrightnessContrast(BrightnessContrast::default())),
            "Color Balance" => Some(Self::ColorBalance(ColorBalance::default())),
            "Film Grain" => Some(Self::FilmGrain(FilmGrain::default())),
            "Depth of Field" => Some(Self::DepthOfField(DepthOfField::default())),
            "File Output" => Some(Self::FileOutput(FileOutput::default())),
            "Motion Blur" => Some(Self::MotionBlur(MotionBlurEffect::default())),
            _ => None,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Glow(_) => "Lens Effects",
            Self::Blur(_) => "Blur",
            Self::BrightnessContrast(_) => "Brightness and Contrast",
            Self::ColorBalance(_) => "Color Balance",
            Self::FilmGrain(_) => "Film Grain",
            Self::DepthOfField(_) => "Depth of Field",
            Self::FileOutput(_) => "File Output",
            Self::MotionBlur(_) => "Motion Blur",
        }
    }
}

/// One entry of the Effects list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderEffect {
    pub name: String,
    pub active: bool,
    pub kind: EffectKind,
}

/// The Environment and Effects dialog's values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Environment {
    /// Background > Color.
    pub background: Param<Vec3>,
    /// Background > Environment Map (a bitmap file) and Use Map.
    pub background_map: Option<String>,
    pub use_map: bool,
    pub map_mapping: EnvMapping,
    /// Global Lighting: Tint and Level multiply every light except ambient.
    pub tint: Param<Vec3>,
    pub level: Param<f32>,
    /// Global Lighting > Ambient.
    pub ambient: Param<Vec3>,
    pub exposure: ExposureControl,
    pub atmospheres: Vec<Atmosphere>,
    pub effects: Vec<RenderEffect>,
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            background: p(Vec3::ZERO),
            background_map: None,
            use_map: false,
            map_mapping: EnvMapping::Screen,
            tint: p(Vec3::ONE),
            level: p(1.0),
            // Black, as the reference: what no light reaches renders black.
            ambient: p(Vec3::ZERO),
            exposure: ExposureControl::default(),
            atmospheres: Vec::new(),
            effects: Vec::new(),
        }
    }
}

impl Environment {
    /// The environment map a render draws behind the scene (Use Map on and a file chosen).
    pub fn map(&self) -> Option<&str> {
        self.background_map.as_deref().filter(|_| self.use_map)
    }

    /// A list name not used yet: `base`, then "base 1", "base 2", ...
    pub fn unique_name(names: &[&str], base: &str) -> String {
        if !names.contains(&base) {
            return base.to_owned();
        }
        (1..).map(|i| format!("{base} {i}")).find(|n| !names.contains(&n.as_str())).expect("unbounded")
    }
}
