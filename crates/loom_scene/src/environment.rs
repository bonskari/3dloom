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

/// Volume Fog Parameters (no gizmo: the fog fills the scene; without noise
/// its density is even, so it is drawn exactly, with no steps to set).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VolumeFog {
    pub color: Param<Vec3>,
    /// 0–20.
    pub density: Param<f32>,
    pub fog_background: bool,
}

impl Default for VolumeFog {
    fn default() -> Self {
        Self { color: p(Vec3::ONE), density: p(5.0), fog_background: true }
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AtmosphereKind {
    Fog(Fog),
    VolumeFog(VolumeFog),
    VolumeLight(VolumeLight),
}

impl AtmosphereKind {
    /// Add Atmospheric Effect list names.
    pub const NAMES: [&'static str; 3] = ["Fog", "Volume Fog", "Volume Light"];

    pub fn new(name: &str) -> Option<Self> {
        match name {
            "Fog" => Some(Self::Fog(Fog::default())),
            "Volume Fog" => Some(Self::VolumeFog(VolumeFog::default())),
            "Volume Light" => Some(Self::VolumeLight(VolumeLight::default())),
            _ => None,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Fog(_) => "Fog",
            Self::VolumeFog(_) => "Volume Fog",
            Self::VolumeLight(_) => "Volume Light",
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
}

impl Default for Glow {
    fn default() -> Self {
        Self { size: p(5.0), intensity: p(100.0), use_source_color: p(100.0), inner_color: p(Vec3::ONE), outer_color: p(Vec3::ONE), bright: p(200.0) }
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EffectKind {
    Glow(Glow),
    Blur(Blur),
    BrightnessContrast(BrightnessContrast),
    ColorBalance(ColorBalance),
    FilmGrain(FilmGrain),
    DepthOfField(DepthOfField),
}

impl EffectKind {
    /// Add Effect list names.
    pub const NAMES: [&'static str; 6] = ["Lens Effects", "Blur", "Brightness and Contrast", "Color Balance", "Film Grain", "Depth of Field"];

    pub fn new(name: &str) -> Option<Self> {
        match name {
            "Lens Effects" | "Glow" => Some(Self::Glow(Glow::default())),
            "Blur" => Some(Self::Blur(Blur::default())),
            "Brightness and Contrast" => Some(Self::BrightnessContrast(BrightnessContrast::default())),
            "Color Balance" => Some(Self::ColorBalance(ColorBalance::default())),
            "Film Grain" => Some(Self::FilmGrain(FilmGrain::default())),
            "Depth of Field" => Some(Self::DepthOfField(DepthOfField::default())),
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
            // White, as the Bevy renderer's ambient light always was here
            // (the reference defaults to black next to its default lights).
            ambient: p(Vec3::ONE),
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
