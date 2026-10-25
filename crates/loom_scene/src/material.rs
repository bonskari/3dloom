//! Materials: the Standard material with the Blinn shader, the compound
//! Multi/Sub-Object and Blend materials, the 2D maps (Bitmap, Checker,
//! Gradient, Noise, Mix), the scene's material library and the Material
//! Editor's 24 sample slots with the level being edited.

use crate::material_types::{composite, shellac, Architectural, Composite, DoubleSided, InkPaint, MatteShadow, Raytrace, Shellac, TopBottom};
use crate::param::Param;
use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MaterialId(pub u32);

/// The map types the Material/Map Browser offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum MapType {
    #[default]
    Bitmap,
    Checker,
    Gradient,
    Noise,
    Mix,
    /// Normal Bump: a tangent-space normal map in its Normal component.
    NormalBump,
    Swirl,
    Tiles,
    GradientRamp,
    Cellular,
    Dent,
    Marble,
    PerlinMarble,
    Planet,
    Smoke,
    Speckle,
    Splat,
    Stucco,
    Waves,
    Wood,
    Composite,
    Mask,
    RgbMultiply,
    RgbTint,
    Output,
    ColorCorrection,
    VertexColor,
    Falloff,
}

impl MapType {
    pub const ALL: [MapType; 28] = [
        MapType::Bitmap,
        MapType::Cellular,
        MapType::Checker,
        MapType::ColorCorrection,
        MapType::Composite,
        MapType::Dent,
        MapType::Falloff,
        MapType::Gradient,
        MapType::GradientRamp,
        MapType::Marble,
        MapType::Mask,
        MapType::Mix,
        MapType::Noise,
        MapType::NormalBump,
        MapType::Output,
        MapType::PerlinMarble,
        MapType::Planet,
        MapType::RgbMultiply,
        MapType::RgbTint,
        MapType::Smoke,
        MapType::Speckle,
        MapType::Splat,
        MapType::Stucco,
        MapType::Swirl,
        MapType::Tiles,
        MapType::VertexColor,
        MapType::Waves,
        MapType::Wood,
    ];

    pub fn label(self) -> &'static str {
        match self {
            MapType::Bitmap => "Bitmap",
            MapType::Checker => "Checker",
            MapType::Gradient => "Gradient",
            MapType::Noise => "Noise",
            MapType::Mix => "Mix",
            MapType::NormalBump => "Normal Bump",
            MapType::Swirl => "Swirl",
            MapType::Tiles => "Tiles",
            MapType::GradientRamp => "Gradient Ramp",
            MapType::Cellular => "Cellular",
            MapType::Dent => "Dent",
            MapType::Marble => "Marble",
            MapType::PerlinMarble => "Perlin Marble",
            MapType::Planet => "Planet",
            MapType::Smoke => "Smoke",
            MapType::Speckle => "Speckle",
            MapType::Splat => "Splat",
            MapType::Stucco => "Stucco",
            MapType::Waves => "Waves",
            MapType::Wood => "Wood",
            MapType::Composite => "Composite",
            MapType::Mask => "Mask",
            MapType::RgbMultiply => "RGB Multiply",
            MapType::RgbTint => "RGB Tint",
            MapType::Output => "Output",
            MapType::ColorCorrection => "Color Correction",
            MapType::VertexColor => "Vertex Color",
            MapType::Falloff => "Falloff",
        }
    }

    pub fn from_label(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.label() == s)
    }

    /// The names of the map's colour components (each can hold a sub-map);
    /// a Composite's and a Gradient Ramp's grow (`Map::slot_names`).
    pub fn color_slots(self) -> &'static [&'static str] {
        match self {
            MapType::Bitmap | MapType::VertexColor => &[],
            MapType::Checker | MapType::Noise => &["Color #1", "Color #2"],
            MapType::Dent | MapType::Marble | MapType::PerlinMarble | MapType::Smoke | MapType::Speckle | MapType::Splat | MapType::Stucco | MapType::Waves | MapType::Wood | MapType::RgbMultiply => &["Color #1", "Color #2"],
            MapType::Swirl => &["Base", "Swirl"],
            MapType::Tiles => &["Tiles", "Grout"],
            MapType::GradientRamp => &["Source Map", "Flag #1", "Flag #2", "Flag #3"],
            MapType::Cellular => &["Cell Color", "Division #1", "Division #2"],
            MapType::Planet => &["Color #1", "Color #2", "Color #3", "Color #4", "Color #5", "Color #6", "Color #7", "Color #8"],
            MapType::Composite => &["Layer 1", "Mask 1"],
            MapType::Mask => &["Map", "Mask"],
            MapType::RgbTint => &["R", "G", "B", "Map"],
            MapType::Output => &["Map"],
            MapType::ColorCorrection => &["Map", "Hue Tint"],
            MapType::Falloff => &["Front", "Side"],
            MapType::Gradient => &["Color #1", "Color #2", "Color #3"],
            MapType::Mix => &["Color #1", "Color #2", "Mix Amount"],
            MapType::NormalBump => &["Normal"],
        }
    }
}

/// Coordinates rollout (2D maps): Offset, Tiling, Mirror, Tile, Angle W, Map Channel, Blur.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MapCoords {
    pub offset: [f32; 2],
    pub tiling: [f32; 2],
    pub mirror: [bool; 2],
    pub tile: [bool; 2],
    /// Angle W in degrees.
    pub angle: f32,
    pub map_channel: u32,
    pub blur: f32,
}

impl Default for MapCoords {
    fn default() -> Self {
        Self { offset: [0.0; 2], tiling: [1.0; 2], mirror: [false; 2], tile: [true; 2], angle: 0.0, map_channel: 1, blur: 1.0 }
    }
}

impl MapCoords {
    /// Map-space UV of a surface UV: offset, rotate about the centre by
    /// Angle W, then tile. A mirrored axis's baked texture holds the image
    /// and its mirror image, so Tiling 1 shows two copies as in the reference.
    pub fn apply(&self, uv: [f32; 2]) -> [f32; 2] {
        let (s, c) = (-self.angle.to_radians()).sin_cos();
        let u = uv[0] - 0.5 - self.offset[0];
        let v = uv[1] - 0.5 - self.offset[1];
        let (ru, rv) = (u * c - v * s, u * s + v * c);
        [ru * self.tiling[0] + 0.5, rv * self.tiling[1] + 0.5]
    }

    /// As `apply`, as a 2×3 matrix [a, b, c, d, e, f]: u' = a u + b v + c, v' = d u + e v + f.
    pub fn matrix(&self) -> [f32; 6] {
        let o = self.apply([0.0, 0.0]);
        let x = self.apply([1.0, 0.0]);
        let y = self.apply([0.0, 1.0]);
        [x[0] - o[0], y[0] - o[0], o[0], x[1] - o[1], y[1] - o[1], o[1]]
    }

    /// Tile off on an axis: the map shows once (a decal), the material colour around it.
    pub fn decal(&self) -> [bool; 2] {
        [!self.tile[0] && !self.mirror[0], !self.tile[1] && !self.mirror[1]]
    }
}

/// Bitmap Parameters > Cropping/Placement: U/V from the image's top left, W/H its size, 0–1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Cropping {
    pub apply: bool,
    /// Place (scale the whole image into the region) instead of Crop.
    pub place: bool,
    pub u: f32,
    pub v: f32,
    pub w: f32,
    pub h: f32,
}

impl Default for Cropping {
    fn default() -> Self {
        Self { apply: false, place: false, u: 0.0, v: 0.0, w: 1.0, h: 1.0 }
    }
}

/// Checker / Gradient / Noise / Mix parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MapParams {
    /// Checker Soften.
    pub soften: f32,
    /// Gradient Color 2 Position.
    pub position: f32,
    /// Gradient Type: Radial (else Linear).
    pub radial: bool,
    /// Noise Type: 0 Regular, 1 Fractal, 2 Turbulence.
    pub noise_type: u32,
    /// Noise Size, Levels, Phase, High / Low thresholds.
    pub size: f32,
    pub levels: f32,
    pub phase: f32,
    pub high: f32,
    pub low: f32,
    /// Mix Amount 0–100.
    pub mix_amount: f32,
    /// Normal Bump: Flip Red (X), Flip Green (Y) of the normal map.
    pub flip_red: bool,
    pub flip_green: bool,
}

impl Default for MapParams {
    fn default() -> Self {
        Self { soften: 0.0, position: 0.5, radial: false, noise_type: 0, size: 25.0, levels: 3.0, phase: 0.0, high: 1.0, low: 0.0, mix_amount: 0.0, flip_red: false, flip_green: false }
    }
}

/// A map in a material component (Maps rollout channel, Blend Mask) or in
/// another map's colour component.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Map {
    /// The Bitmap's image file.
    pub path: String,
    /// The Maps rollout Amount, 0–100 (Bump: -999–999).
    pub amount: f32,
    /// The Maps rollout check box.
    pub enabled: bool,
    #[serde(default)]
    pub kind: MapType,
    /// The name field at the map's level ("Map #1").
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub coords: MapCoords,
    #[serde(default)]
    pub crop: Cropping,
    #[serde(default = "default_map_colors")]
    pub colors: [Vec3; 3],
    /// Maps in the colour components (`MapType::color_slots` order), each with its check box.
    #[serde(default)]
    pub sub_maps: [Option<Box<Map>>; 3],
    #[serde(default)]
    pub params: MapParams,
    /// Coordinates rollout of a 3D map.
    #[serde(default)]
    pub coords3: crate::map_params::Coords3,
    #[serde(default)]
    pub output: crate::map_params::MapOutput,
    #[serde(default)]
    pub extra: crate::map_params::MoreParams,
    /// Composite layers.
    #[serde(default)]
    pub layers: Vec<crate::map_params::CompositeLayer>,
    /// Gradient Ramp flags.
    #[serde(default)]
    pub flags: Vec<crate::map_params::RampFlag>,
    /// Colour components past the third.
    #[serde(default)]
    pub extra_colors: Vec<Vec3>,
    #[serde(default)]
    pub extra_maps: Vec<Option<Box<Map>>>,
}

fn default_map_colors() -> [Vec3; 3] {
    [Vec3::ZERO, Vec3::splat(0.5), Vec3::ONE]
}

impl Map {
    /// A new map of `kind` with the reference's defaults: Checker black/white,
    /// Gradient black / grey / white, Noise and Mix black and white.
    pub fn new(kind: MapType, amount: f32) -> Self {
        let colors = match kind {
            MapType::Gradient => default_map_colors(),
            // A flat normal until a normal map is in the Normal component.
            MapType::NormalBump => [Vec3::new(0.5, 0.5, 1.0), Vec3::ONE, Vec3::ONE],
            _ => [Vec3::ZERO, Vec3::ONE, Vec3::ONE],
        };
        let mut map = Self {
            path: String::new(),
            amount,
            enabled: true,
            kind,
            name: String::new(),
            coords: MapCoords::default(),
            crop: Cropping::default(),
            colors,
            sub_maps: [None, None, None],
            params: MapParams::default(),
            coords3: Default::default(),
            output: Default::default(),
            extra: Default::default(),
            layers: Vec::new(),
            flags: Vec::new(),
            extra_colors: Vec::new(),
            extra_maps: Vec::new(),
        };
        map.init_more();
        map
    }

    pub fn bitmap(path: impl Into<String>, amount: f32) -> Self {
        Self { path: path.into(), ..Self::new(MapType::Bitmap, amount) }
    }

    /// The map button's label: the bitmap's file name, else the name and type.
    pub fn label(&self) -> String {
        if self.kind == MapType::Bitmap && !self.path.is_empty() {
            return std::path::Path::new(&self.path).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_else(|| self.path.clone());
        }
        let name = if self.name.is_empty() { "Map" } else { &self.name };
        format!("{name} ({})", self.kind.label())
    }

    pub fn sub_map_index(&self, slot: &str) -> Option<usize> {
        self.slot_names().iter().position(|s| s == slot)
    }

    /// Sets a parameter by its rollout label (booleans as 0 / 1), clamped to its range.
    pub fn set_parameter(&mut self, name: &str, value: f64) -> Result<(), String> {
        let v = value as f32;
        let on = value != 0.0;
        let c = &mut self.coords;
        let k = &mut self.crop;
        let p = &mut self.params;
        match name {
            "Offset U" => c.offset[0] = v,
            "Offset V" => c.offset[1] = v,
            "Tiling U" => c.tiling[0] = v,
            "Tiling V" => c.tiling[1] = v,
            // Mirror and Tile exclude each other on an axis.
            "Mirror U" => {
                c.mirror[0] = on;
                c.tile[0] &= !on;
            }
            "Mirror V" => {
                c.mirror[1] = on;
                c.tile[1] &= !on;
            }
            "Tile U" => {
                c.tile[0] = on;
                c.mirror[0] &= !on;
            }
            "Tile V" => {
                c.tile[1] = on;
                c.mirror[1] &= !on;
            }
            "Angle W" => c.angle = v,
            "Map Channel" => c.map_channel = (value.round().max(0.0) as u32).clamp(if self.kind == MapType::VertexColor { 0 } else { 1 }, 99),
            "Blur" => c.blur = v.clamp(0.01, 100.0),
            "Apply" => k.apply = on,
            "Place" => k.place = on,
            "Crop" => k.place = !on,
            "U" => k.u = v.clamp(0.0, 1.0),
            "V" => k.v = v.clamp(0.0, 1.0),
            "W" => k.w = v.clamp(0.0, 1.0),
            "H" => k.h = v.clamp(0.0, 1.0),
            "Soften" => p.soften = v.clamp(0.0, 1.0),
            "Color 2 Position" => p.position = v.clamp(0.0, 1.0),
            "Radial" => p.radial = on,
            "Noise Type" => p.noise_type = (value.round() as u32).min(2),
            "Size" => p.size = v.clamp(0.001, 1_000_000.0),
            "Levels" => p.levels = v.clamp(1.0, 10.0),
            "Phase" => p.phase = v,
            "High" => p.high = v.clamp(0.0, 1.0),
            "Low" => p.low = v.clamp(0.0, 1.0),
            "Mix Amount" => p.mix_amount = v.clamp(0.0, 100.0),
            "Flip Red (X)" => p.flip_red = on,
            "Flip Green (Y)" => p.flip_green = on,
            "Amount" => self.amount = v,
            "Enabled" => self.enabled = on,
            other => {
                if let Some(slot) = other.strip_suffix(" Map On") {
                    let i = self.sub_map_index(slot).ok_or_else(|| format!("no map component \"{slot}\""))?;
                    if let Some(m) = self.sub_map_slot_mut(i).as_mut() {
                        m.enabled = on;
                    }
                    return Ok(());
                }
                if self.set_more_parameter(other, value)? {
                    return Ok(());
                }
                return Err(format!("no map parameter \"{other}\""));
            }
        }
        Ok(())
    }

    /// A colour component's swatch (Color #1, Base, Flag #2, R...) from 0–255 RGB.
    pub fn set_color(&mut self, which: &str, rgb: [u8; 3]) -> Result<(), String> {
        let names = self.slot_names();
        let i = names.iter().position(|s| s == which).filter(|i| self.slot_has_color(*i)).ok_or_else(|| {
            let with: Vec<&str> = names.iter().enumerate().filter(|(i, _)| self.slot_has_color(*i)).map(|(_, s)| s.as_str()).collect();
            format!("no map colour \"{which}\" ({})", with.join(", "))
        })?;
        *self.color_mut(i) = Vec3::new(rgb[0] as f32, rgb[1] as f32, rgb[2] as f32) / 255.0;
        Ok(())
    }

    /// Swap: Color #1 and #2 (and their maps) trade places.
    pub fn swap(&mut self) {
        self.colors.swap(0, 1);
        self.sub_maps.swap(0, 1);
    }
}

/// One row of the Multi/Sub-Object Basic Parameters list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubMaterial {
    /// The material ID (1-based) faces use to show this sub-material.
    pub id: u16,
    /// The Name column.
    pub name: String,
    pub material: Option<MaterialId>,
    /// The On/Off toggle: off renders black.
    pub enabled: bool,
}

/// Blend Basic Parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Blend {
    pub materials: [Option<MaterialId>; 2],
    pub enabled: [bool; 2],
    pub mask: Option<Map>,
    /// 0–100: 0 shows Material 1, 100 Material 2 (unused while a mask is on).
    pub mix_amount: Param<f32>,
    /// Interactive: 0 Material 1, 1 Material 2, 2 the Mask, shown in viewports.
    pub interactive: u8,
    pub use_curve: bool,
    pub upper: f32,
    pub lower: f32,
}

/// Shell Material Parameters: the original material and the baked one
/// (Render To Texture), and which of them viewports and renders show.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shell {
    /// Original Material, Baked Material.
    pub materials: [Option<MaterialId>; 2],
    /// Viewport / Render: 0 the original, 1 the baked material.
    pub viewport: u8,
    pub render: u8,
}

/// Which material of a Shell resolves: the viewport's, the render's, or
/// the original (what Render To Texture bakes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellView {
    Viewport,
    Render,
    Original,
}

/// Material type (the Type button).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum MaterialKind {
    #[default]
    Standard,
    MultiSub(Vec<SubMaterial>),
    Blend(Box<Blend>),
    Shell(Box<Shell>),
    Raytrace(Box<Raytrace>),
    Architectural(Box<Architectural>),
    DoubleSided(Box<DoubleSided>),
    TopBottom(Box<TopBottom>),
    Composite(Box<Composite>),
    Shellac(Box<Shellac>),
    MatteShadow(Box<MatteShadow>),
    InkPaint(Box<InkPaint>),
}

impl MaterialKind {
    /// The Material/Map Browser's material types.
    pub const TYPES: [&'static str; 12] = ["Architectural", "Blend", "Composite", "Double Sided", "Ink 'n Paint", "Matte/Shadow", "Multi/Sub-Object", "Raytrace", "Shell", "Shellac", "Standard", "Top/Bottom"];

    pub fn label(&self) -> &'static str {
        match self {
            MaterialKind::Standard => "Standard",
            MaterialKind::MultiSub(_) => "Multi/Sub-Object",
            MaterialKind::Blend(_) => "Blend",
            MaterialKind::Shell(_) => "Shell",
            MaterialKind::Raytrace(_) => "Raytrace",
            MaterialKind::Architectural(_) => "Architectural",
            MaterialKind::DoubleSided(_) => "Double Sided",
            MaterialKind::TopBottom(_) => "Top/Bottom",
            MaterialKind::Composite(_) => "Composite",
            MaterialKind::Shellac(_) => "Shellac",
            MaterialKind::MatteShadow(_) => "Matte/Shadow",
            MaterialKind::InkPaint(_) => "Ink 'n Paint",
        }
    }

    /// Kinds that draw with their own Standard values (shader, colours, maps).
    pub fn uses_standard_values(&self) -> bool {
        matches!(self, MaterialKind::Standard | MaterialKind::Raytrace(_) | MaterialKind::Architectural(_))
    }
}

/// Shading type: the Shader Basic Parameters drop-down of a Standard material.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Shader {
    Anisotropic,
    #[default]
    Blinn,
    Metal,
    MultiLayer,
    OrenNayarBlinn,
    Phong,
    Strauss,
    Translucent,
}

impl Shader {
    pub const ALL: [Shader; 8] = [Shader::Anisotropic, Shader::Blinn, Shader::Metal, Shader::MultiLayer, Shader::OrenNayarBlinn, Shader::Phong, Shader::Strauss, Shader::Translucent];

    /// The drop-down's text (the Basic Parameters rollout is "<name> Basic Parameters").
    pub fn label(self) -> &'static str {
        match self {
            Shader::Anisotropic => "Anisotropic",
            Shader::Blinn => "Blinn",
            Shader::Metal => "Metal",
            Shader::MultiLayer => "Multi-Layer",
            Shader::OrenNayarBlinn => "Oren-Nayar-Blinn",
            Shader::Phong => "Phong",
            Shader::Strauss => "Strauss",
            Shader::Translucent => "Translucent Shader",
        }
    }

    pub fn from_label(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.label() == s || (*t == Shader::Translucent && s == "Translucent"))
    }

    /// Diffuse Level: Anisotropic, Multi-Layer, Oren-Nayar-Blinn.
    pub fn has_diffuse_level(self) -> bool {
        matches!(self, Shader::Anisotropic | Shader::MultiLayer | Shader::OrenNayarBlinn)
    }

    /// Roughness: Multi-Layer, Oren-Nayar-Blinn.
    pub fn has_roughness(self) -> bool {
        matches!(self, Shader::MultiLayer | Shader::OrenNayarBlinn)
    }
}

/// The Basic Parameters beyond Blinn's: the other shaders' values, the
/// Self-Illumination Color mode and the Diffuse/Specular lock.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShaderParams {
    pub shader: Shader,
    /// 0–400 (Anisotropic, Multi-Layer, Oren-Nayar-Blinn).
    pub diffuse_level: f32,
    /// 0–100 (Multi-Layer, Oren-Nayar-Blinn).
    pub roughness: f32,
    /// Anisotropic highlight (Multi-Layer: the first layer's): 0–100 and degrees 0–9999.
    pub anisotropy: f32,
    pub orientation: f32,
    /// Strauss 0–100.
    pub metalness: f32,
    /// Multi-Layer Second Specular Layer: Color, Level, Glossiness, Anisotropy, Orientation.
    pub specular2: Vec3,
    pub level2: f32,
    pub glossiness2: f32,
    pub anisotropy2: f32,
    pub orientation2: f32,
    /// Translucent shader: Translucent Clr (its Filter Color is the Extended Parameters' one).
    pub translucent_color: Vec3,
    pub backside_specular: bool,
    /// Self-Illumination Color check box and swatch.
    pub self_illum_color_on: bool,
    pub self_illum_color: Vec3,
    /// The lock between Diffuse and Specular.
    pub diffuse_specular_locked: bool,
}

impl Default for ShaderParams {
    fn default() -> Self {
        Self {
            shader: Shader::Blinn,
            diffuse_level: 100.0,
            roughness: 0.0,
            anisotropy: 50.0,
            orientation: 0.0,
            metalness: 0.0,
            specular2: Vec3::splat(229.0 / 255.0),
            level2: 0.0,
            glossiness2: 25.0,
            anisotropy2: 0.0,
            orientation2: 0.0,
            translucent_color: Vec3::ZERO,
            backside_specular: true,
            self_illum_color_on: false,
            self_illum_color: Vec3::ZERO,
            diffuse_specular_locked: false,
        }
    }
}

/// Extended Parameters: Advanced Transparency, Wire, Reflection Dimming.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExtendedParams {
    /// Falloff Out (else In) and its Amt 0–100.
    pub falloff_out: bool,
    pub falloff_amount: f32,
    /// Type: 0 Filter, 1 Subtractive, 2 Additive.
    pub transparency_type: u8,
    pub filter_color: Vec3,
    /// Index of Refraction 0.01–10.
    pub ior: f32,
    pub wire_size: f32,
    /// Wire size In: Units (else Pixels).
    pub wire_units: bool,
    pub reflection_dimming: bool,
    pub dim_level: f32,
    pub refl_level: f32,
}

impl Default for ExtendedParams {
    fn default() -> Self {
        Self { falloff_out: false, falloff_amount: 0.0, transparency_type: 0, filter_color: Vec3::splat(0.5), ior: 1.5, wire_size: 1.0, wire_units: false, reflection_dimming: false, dim_level: 0.0, refl_level: 3.0 }
    }
}

/// Standard material, Blinn Basic Parameters plus the maps; `kind` makes it
/// a compound material (its own Standard values are then unused).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StandardMaterial {
    pub name: String,
    /// Colours 0–1.
    pub ambient: Param<Vec3>,
    pub diffuse: Param<Vec3>,
    pub specular: Param<Vec3>,
    /// The lock between Ambient and Diffuse (on by default).
    pub ambient_locked: bool,
    /// 0–999.
    pub specular_level: Param<f32>,
    /// 0–100.
    pub glossiness: Param<f32>,
    /// 0–1.
    pub soften: Param<f32>,
    /// Self-Illumination 0–100.
    pub self_illum: Param<f32>,
    /// 0–100.
    pub opacity: Param<f32>,
    pub two_sided: bool,
    pub wire: bool,
    pub faceted: bool,
    pub diffuse_map: Option<Map>,
    pub bump_map: Option<Map>,
    pub opacity_map: Option<Map>,
    pub specular_level_map: Option<Map>,
    #[serde(default)]
    pub ambient_map: Option<Map>,
    #[serde(default)]
    pub specular_color_map: Option<Map>,
    #[serde(default)]
    pub glossiness_map: Option<Map>,
    #[serde(default)]
    pub self_illum_map: Option<Map>,
    #[serde(default)]
    pub filter_map: Option<Map>,
    #[serde(default)]
    pub reflection_map: Option<Map>,
    #[serde(default)]
    pub refraction_map: Option<Map>,
    #[serde(default)]
    pub displacement_map: Option<Map>,
    /// Show Standard Map in Viewport (the diffuse map in shaded views).
    pub show_map_in_viewport: bool,
    #[serde(default)]
    pub kind: MaterialKind,
    #[serde(default)]
    pub shading: ShaderParams,
    #[serde(default)]
    pub extended: ExtendedParams,
}

/// The Maps rollout channels in rollout order, with their default Amount.
pub const MAP_CHANNELS: [(&str, f32); 12] = [
    ("Ambient Color", 100.0),
    ("Diffuse Color", 100.0),
    ("Specular Color", 100.0),
    ("Specular Level", 100.0),
    ("Glossiness", 100.0),
    ("Self-Illumination", 100.0),
    ("Opacity", 100.0),
    ("Filter Color", 100.0),
    ("Bump", 30.0),
    ("Reflection", 100.0),
    ("Refraction", 100.0),
    ("Displacement", 100.0),
];

/// Every map slot name a level can have: the Maps channels, a Blend's Mask
/// and a map's colour components (indexes for dialogs that must be `Copy`).
pub const MAP_SLOTS: [&str; 16] = [
    "Ambient Color", "Diffuse Color", "Specular Color", "Specular Level", "Glossiness", "Self-Illumination", "Opacity", "Filter Color", "Bump", "Reflection", "Refraction", "Displacement", "Mask", "Color #1", "Color #2", "Color #3",
];

/// A map slot's default Amount.
pub fn default_amount(slot: &str) -> f32 {
    MAP_CHANNELS.iter().find(|(n, _)| *n == slot).map_or(100.0, |(_, a)| *a)
}

impl StandardMaterial {
    /// A fresh Standard material as a Material Editor slot shows it
    /// ("01 - Default" in slot 0): grey diffuse, near-white specular,
    /// Specular Level 0, Glossiness 10, Soften 0.1, fully opaque.
    pub fn new(name: impl Into<String>) -> Self {
        let grey = Vec3::splat(150.0 / 255.0);
        Self {
            name: name.into(),
            ambient: Param::new(grey),
            diffuse: Param::new(grey),
            specular: Param::new(Vec3::splat(229.0 / 255.0)),
            ambient_locked: true,
            specular_level: Param::new(0.0),
            glossiness: Param::new(10.0),
            soften: Param::new(0.1),
            self_illum: Param::new(0.0),
            opacity: Param::new(100.0),
            two_sided: false,
            wire: false,
            faceted: false,
            diffuse_map: None,
            bump_map: None,
            opacity_map: None,
            specular_level_map: None,
            ambient_map: None,
            specular_color_map: None,
            glossiness_map: None,
            self_illum_map: None,
            filter_map: None,
            reflection_map: None,
            refraction_map: None,
            displacement_map: None,
            show_map_in_viewport: false,
            kind: MaterialKind::Standard,
            shading: ShaderParams::default(),
            extended: ExtendedParams::default(),
        }
    }

    /// The default material of Material Editor slot `slot` (0-based).
    pub fn slot_default(slot: usize) -> Self {
        Self::new(format!("{:02} - Default", slot + 1))
    }

    pub fn map(&self, channel: &str) -> Option<&Map> {
        self.map_slot(channel).ok().and_then(|m| m.as_ref())
    }

    /// The Option holding `channel`'s map, by its Maps rollout name (a Blend's "Mask" too).
    pub fn map_slot_mut(&mut self, channel: &str) -> Result<&mut Option<Map>, String> {
        if let MaterialKind::Blend(b) = &mut self.kind {
            return match channel {
                "Mask" => Ok(&mut b.mask),
                other => Err(format!("a Blend material has no map \"{other}\" (Mask)")),
            };
        }
        Ok(match channel {
            "Ambient Color" => &mut self.ambient_map,
            "Diffuse Color" => &mut self.diffuse_map,
            "Specular Color" => &mut self.specular_color_map,
            "Specular Level" => &mut self.specular_level_map,
            "Glossiness" => &mut self.glossiness_map,
            "Self-Illumination" => &mut self.self_illum_map,
            "Opacity" => &mut self.opacity_map,
            "Filter Color" => &mut self.filter_map,
            "Bump" => &mut self.bump_map,
            "Reflection" => &mut self.reflection_map,
            "Refraction" => &mut self.refraction_map,
            "Displacement" => &mut self.displacement_map,
            other => return Err(format!("no map channel \"{other}\" ({})", MAP_CHANNELS.map(|c| c.0).join(", "))),
        })
    }

    fn map_slot(&self, channel: &str) -> Result<&Option<Map>, String> {
        if let MaterialKind::Blend(b) = &self.kind {
            return if channel == "Mask" { Ok(&b.mask) } else { Err(format!("no map \"{channel}\"")) };
        }
        Ok(match channel {
            "Ambient Color" => &self.ambient_map,
            "Diffuse Color" => &self.diffuse_map,
            "Specular Color" => &self.specular_color_map,
            "Specular Level" => &self.specular_level_map,
            "Glossiness" => &self.glossiness_map,
            "Self-Illumination" => &self.self_illum_map,
            "Opacity" => &self.opacity_map,
            "Filter Color" => &self.filter_map,
            "Bump" => &self.bump_map,
            "Reflection" => &self.reflection_map,
            "Refraction" => &self.refraction_map,
            "Displacement" => &self.displacement_map,
            other => return Err(format!("no map channel \"{other}\"")),
        })
    }

    /// The map slots this level shows, in rollout order.
    pub fn map_slots(&self) -> Vec<&'static str> {
        match &self.kind {
            MaterialKind::Standard | MaterialKind::Raytrace(_) => MAP_CHANNELS.iter().map(|c| c.0).collect(),
            // Physical Qualities (Diffuse, Shininess, Transparency, Translucency, IOR, Luminance) and Special Effects.
            MaterialKind::Architectural(_) => vec!["Diffuse Color", "Glossiness", "Opacity", "Filter Color", "Refraction", "Self-Illumination", "Bump", "Displacement"],
            MaterialKind::MatteShadow(_) => vec!["Reflection"],
            MaterialKind::InkPaint(_) => vec!["Diffuse Color", "Bump", "Displacement"],
            MaterialKind::Blend(_) => vec!["Mask"],
            _ => Vec::new(),
        }
    }

    /// The sub-materials this level holds, in list order (Blend: Material 1, 2).
    pub fn children(&self) -> Vec<Option<MaterialId>> {
        match &self.kind {
            MaterialKind::Standard => Vec::new(),
            MaterialKind::MultiSub(list) => list.iter().map(|s| s.material).collect(),
            MaterialKind::Blend(b) => b.materials.to_vec(),
            MaterialKind::Shell(sh) => sh.materials.to_vec(),
            MaterialKind::DoubleSided(d) => d.materials.to_vec(),
            MaterialKind::TopBottom(t) => t.materials.to_vec(),
            MaterialKind::Shellac(sh) => sh.materials.to_vec(),
            MaterialKind::Composite(c) => std::iter::once(c.base).chain(c.layers.iter().map(|l| l.material)).collect(),
            MaterialKind::Raytrace(_) | MaterialKind::Architectural(_) | MaterialKind::MatteShadow(_) | MaterialKind::InkPaint(_) => Vec::new(),
        }
    }

    /// The sub-material buttons, as `children` orders them.
    pub fn children_mut(&mut self) -> Vec<&mut Option<MaterialId>> {
        match &mut self.kind {
            MaterialKind::MultiSub(list) => list.iter_mut().map(|s| &mut s.material).collect(),
            MaterialKind::Blend(b) => b.materials.iter_mut().collect(),
            MaterialKind::Shell(sh) => sh.materials.iter_mut().collect(),
            MaterialKind::DoubleSided(d) => d.materials.iter_mut().collect(),
            MaterialKind::TopBottom(t) => t.materials.iter_mut().collect(),
            MaterialKind::Shellac(sh) => sh.materials.iter_mut().collect(),
            MaterialKind::Composite(c) => std::iter::once(&mut c.base).chain(c.layers.iter_mut().map(|l| &mut l.material)).collect(),
            _ => Vec::new(),
        }
    }

    /// Sets a Blinn Basic Parameters / Shader Basic Parameters value by its
    /// label, clamped to its spinner range (booleans as 0 / 1). Blend values
    /// (Mix Amount, Interactive, Use Curve, Upper, Lower, Material 1/2 On,
    /// Mask On) apply to a Blend material.
    pub fn set_parameter(&mut self, name: &str, value: f64) -> Result<(), String> {
        let v = value as f32;
        let on = value != 0.0;
        let on_off = |i: usize, enabled: &mut [bool; 2]| enabled[i] = on;
        match &mut self.kind {
            MaterialKind::Raytrace(r) => {
                if r.set_parameter(name, value)? {
                    return Ok(());
                }
            }
            MaterialKind::Architectural(a) => {
                if name == "Template" {
                    let i = (value.round() as usize).min(crate::material_types::ARCH_TEMPLATES.len() - 1);
                    let t = crate::material_types::ARCH_TEMPLATES[i];
                    a.template = i as u8;
                    // A template sets the physical values it describes.
                    if i > 0 {
                        (a.shininess, a.transparency, a.translucency, a.ior) = (t.2, t.3, t.4, t.5);
                        let c = Vec3::new(t.1[0] as f32, t.1[1] as f32, t.1[2] as f32) / 255.0;
                        self.diffuse.set(c);
                        self.ambient.set(c);
                    }
                    return Ok(());
                }
                if a.set_parameter(name, value)? {
                    return Ok(());
                }
                if !matches!(name, "2-Sided") {
                    return Err(format!("no Architectural parameter \"{name}\""));
                }
            }
            MaterialKind::DoubleSided(d) => {
                match name {
                    "Translucency" => d.translucency = v.clamp(0.0, 100.0),
                    "Facing Material On" => on_off(0, &mut d.enabled),
                    "Back Material On" => on_off(1, &mut d.enabled),
                    other => return Err(format!("no Double Sided parameter \"{other}\"")),
                }
                return Ok(());
            }
            MaterialKind::TopBottom(t) => {
                match name {
                    "Local" => t.local = on,
                    "Swap" => {
                        t.materials.swap(0, 1);
                        t.enabled.swap(0, 1);
                    }
                    "Blend" => t.blend = v.clamp(0.0, 100.0),
                    "Position" => t.position = v.clamp(0.0, 100.0),
                    "Top Material On" => on_off(0, &mut t.enabled),
                    "Bottom Material On" => on_off(1, &mut t.enabled),
                    other => return Err(format!("no Top/Bottom parameter \"{other}\"")),
                }
                return Ok(());
            }
            MaterialKind::Shellac(sh) => {
                match name {
                    "Shellac Color Blend" => sh.blend = v.clamp(0.0, 1000.0),
                    other => return Err(format!("no Shellac parameter \"{other}\"")),
                }
                return Ok(());
            }
            MaterialKind::Composite(c) => {
                // "Mat 3 On", "Mat 3 Mode" (0 A, 1 S, 2 M), "Mat 3 Amount".
                let parsed = name.strip_prefix("Mat ").and_then(|r| r.split_once(' ')).and_then(|(n, what)| n.parse::<usize>().ok().map(|n| (n, what)));
                let Some((n, what)) = parsed.filter(|(n, _)| (1..=c.layers.len()).contains(n)) else { return Err(format!("no Composite parameter \"{name}\" (Mat 1-9 On / Mode / Amount)")) };
                let l = &mut c.layers[n - 1];
                match what {
                    "On" => l.enabled = on,
                    "Mode" => {
                        l.mode = (value.round() as u8).min(2);
                        l.amount = l.amount.min(if l.mode == 2 { 100.0 } else { 200.0 });
                    }
                    "Amount" => l.amount = v.clamp(0.0, if l.mode == 2 { 100.0 } else { 200.0 }),
                    other => return Err(format!("no Composite parameter \"Mat {n} {other}\"")),
                }
                return Ok(());
            }
            MaterialKind::MatteShadow(m) => {
                match name {
                    "Opaque Alpha" => m.opaque_alpha = on,
                    "Apply Atmosphere" => m.apply_atmosphere = on,
                    "Receive Shadows" => m.receive_shadows = on,
                    "Affect Alpha" => m.affect_alpha = on,
                    "Shadow Brightness" => m.shadow_brightness = v.clamp(0.0, 1.0),
                    "Amount" => m.reflection_amount = v.clamp(0.0, 100.0),
                    other => return Err(format!("no Matte/Shadow parameter \"{other}\"")),
                }
                return Ok(());
            }
            MaterialKind::InkPaint(ink) => return ink.set_parameter(name, value),
            _ => {}
        }
        if let MaterialKind::Shell(sh) = &mut self.kind {
            let which = (value.round() as u8).min(1);
            match name {
                "Viewport" => sh.viewport = which,
                "Render" => sh.render = which,
                other => return Err(format!("no Shell parameter \"{other}\" (Viewport, Render)")),
            }
            return Ok(());
        }
        if let MaterialKind::Blend(b) = &mut self.kind {
            let on = value != 0.0;
            match name {
                "Mix Amount" => b.mix_amount.set(v.clamp(0.0, 100.0)),
                "Interactive" => b.interactive = (value.round() as u8).min(2),
                "Use Curve" => b.use_curve = on,
                "Upper" => b.upper = v.clamp(0.0, 1.0),
                "Lower" => b.lower = v.clamp(0.0, 1.0),
                "Material 1 On" => b.enabled[0] = on,
                "Material 2 On" => b.enabled[1] = on,
                "Mask On" => {
                    if let Some(m) = b.mask.as_mut() {
                        m.enabled = on;
                    }
                }
                other => return Err(format!("no Blend parameter \"{other}\"")),
            }
            return Ok(());
        }
        match name {
            "Specular Level" => self.specular_level.set(v.clamp(0.0, 999.0)),
            "Glossiness" => self.glossiness.set(v.clamp(0.0, 100.0)),
            "Soften" => self.soften.set(v.clamp(0.0, 1.0)),
            "Self-Illumination" => self.self_illum.set(v.clamp(0.0, 100.0)),
            "Opacity" => self.opacity.set(v.clamp(0.0, 100.0)),
            "2-Sided" => self.two_sided = value != 0.0,
            "Wire" => self.wire = value != 0.0,
            "Faceted" => self.faceted = value != 0.0,
            "Ambient/Diffuse Lock" => self.ambient_locked = value != 0.0,
            "Show Map in Viewport" => self.show_map_in_viewport = value != 0.0,
            other => return self.set_shader_parameter(other, value),
        }
        Ok(())
    }

    /// The other shaders' Basic Parameters and the Extended Parameters.
    fn set_shader_parameter(&mut self, name: &str, value: f64) -> Result<(), String> {
        let v = value as f32;
        let on = value != 0.0;
        let (sh, ex) = (&mut self.shading, &mut self.extended);
        match name {
            "Diffuse Level" => sh.diffuse_level = v.clamp(0.0, 400.0),
            "Roughness" => sh.roughness = v.clamp(0.0, 100.0),
            "Anisotropy" => sh.anisotropy = v.clamp(0.0, 100.0),
            "Orientation" => sh.orientation = v.clamp(0.0, 9999.0),
            "Metalness" => sh.metalness = v.clamp(0.0, 100.0),
            "Level 2" => sh.level2 = v.clamp(0.0, 999.0),
            "Glossiness 2" => sh.glossiness2 = v.clamp(0.0, 100.0),
            "Anisotropy 2" => sh.anisotropy2 = v.clamp(0.0, 100.0),
            "Orientation 2" => sh.orientation2 = v.clamp(0.0, 9999.0),
            "Backside Specular" => sh.backside_specular = on,
            "Self-Illum Color On" => sh.self_illum_color_on = on,
            "Diffuse/Specular Lock" => sh.diffuse_specular_locked = on,
            "Falloff Out" => ex.falloff_out = on,
            "Falloff Amount" => ex.falloff_amount = v.clamp(0.0, 100.0),
            "Transparency Type" => ex.transparency_type = (value.round() as u8).min(2),
            "Index of Refraction" => ex.ior = v.clamp(0.01, 10.0),
            "Wire Size" => ex.wire_size = v.clamp(0.0, 10000.0),
            "Wire Units" => ex.wire_units = on,
            "Reflection Dimming" => ex.reflection_dimming = on,
            "Dim Level" => ex.dim_level = v.clamp(0.0, 1.0),
            "Refl. Level" => ex.refl_level = v.clamp(0.1, 10.0),
            other => return Err(format!("no material parameter \"{other}\"")),
        }
        Ok(())
    }

    /// Sets Ambient, Diffuse or Specular from 0–255 RGB; Ambient follows
    /// Diffuse while they are locked.
    pub fn set_color(&mut self, which: &str, rgb: [u8; 3]) -> Result<(), String> {
        let c = Vec3::new(rgb[0] as f32, rgb[1] as f32, rgb[2] as f32) / 255.0;
        match &mut self.kind {
            MaterialKind::Raytrace(r) => {
                if r.set_color(which, rgb) {
                    return Ok(());
                }
            }
            MaterialKind::InkPaint(ink) => return if ink.set_color(which, rgb) { Ok(()) } else { Err(format!("no Ink 'n Paint colour \"{which}\" (Lighted, Shaded, Highlight, Ink Color)")) },
            MaterialKind::MatteShadow(m) if which == "Color" => {
                m.color = c;
                return Ok(());
            }
            _ => {}
        }
        match which {
            "Ambient" => {
                self.ambient.set(c);
                if self.ambient_locked {
                    self.diffuse.set(c);
                }
            }
            "Diffuse" => {
                self.diffuse.set(c);
                if self.ambient_locked {
                    self.ambient.set(c);
                }
                if self.shading.diffuse_specular_locked {
                    self.specular.set(c);
                }
            }
            "Specular" => {
                self.specular.set(c);
                if self.shading.diffuse_specular_locked {
                    self.diffuse.set(c);
                    if self.ambient_locked {
                        self.ambient.set(c);
                    }
                }
            }
            "Specular 2" => self.shading.specular2 = c,
            "Translucent Color" => self.shading.translucent_color = c,
            "Self-Illum Color" => self.shading.self_illum_color = c,
            "Filter Color" => self.extended.filter_color = c,
            other => return Err(format!("no colour \"{other}\" (Ambient, Diffuse, Specular, Specular 2, Translucent Color, Self-Illum Color, Filter Color)")),
        }
        Ok(())
    }
}

/// A step down the material tree from a slot's top level: into the
/// sub-material at list position `Sub(i)` (Blend: 0 = Material 1, 1 =
/// Material 2) or into the map in slot `Map(name)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NavStep {
    Sub(u32),
    Map(String),
}

/// Sample Type flyout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SampleType {
    #[default]
    Sphere,
    Cylinder,
    Cube,
}

/// Material Editor Options and the toolbar toggles beside the slots.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorOptions {
    pub top_light: Vec3,
    pub top_multiplier: f32,
    pub back_light: Vec3,
    pub back_multiplier: f32,
    pub ambient: Vec3,
    pub background_intensity: f32,
    /// Backlight toolbar toggle.
    pub backlight: bool,
    /// Background toolbar toggle (the checker behind the sample).
    pub background: bool,
    /// Sample UV Tiling 1–4.
    pub uv_tiling: u32,
    /// Slots: 6 (3 × 2), 15 (5 × 3) or 24 (6 × 4) shown at a time.
    pub slots_shown: u32,
    pub antialias: bool,
    /// Simple Multi Display Below Top Level.
    pub simple_multi: bool,
    /// Display Maps as 2D.
    pub maps_as_2d: bool,
}

impl Default for EditorOptions {
    fn default() -> Self {
        Self {
            top_light: Vec3::ONE,
            top_multiplier: 1.0,
            back_light: Vec3::ONE,
            back_multiplier: 1.0,
            ambient: Vec3::splat(0.0),
            background_intensity: 0.2,
            backlight: true,
            background: false,
            uv_tiling: 1,
            slots_shown: 6,
            antialias: false,
            simple_multi: true,
            maps_as_2d: true,
        }
    }
}

impl EditorOptions {
    /// Sets an option by its dialog / toolbar label.
    pub fn set(&mut self, name: &str, value: f64) -> Result<(), String> {
        let v = value as f32;
        let on = value != 0.0;
        match name {
            "Top Light Multiplier" => self.top_multiplier = v.clamp(0.0, 10.0),
            "Back Light Multiplier" => self.back_multiplier = v.clamp(0.0, 10.0),
            "Background Intensity" => self.background_intensity = v.clamp(0.0, 1.0),
            "Backlight" => self.backlight = on,
            "Background" => self.background = on,
            "Sample UV Tiling" => self.uv_tiling = (value.round() as u32).clamp(1, 4),
            "Slots" => {
                let n = value.round() as u32;
                if ![6, 15, 24].contains(&n) {
                    return Err("Slots is 6 (3 X 2), 15 (5 X 3) or 24 (6 X 4)".into());
                }
                self.slots_shown = n;
            }
            "Antialias" => self.antialias = on,
            "Simple Multi Display Below Top Level" => self.simple_multi = on,
            "Display Maps as 2D" => self.maps_as_2d = on,
            other => return Err(format!("no Material Editor option \"{other}\"")),
        }
        Ok(())
    }
}

/// The Material Editor's 24 sample slots, the active one and the level of
/// its material being edited.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialEditorSlots {
    /// None = the slot still shows its untouched default material.
    pub slots: Vec<Option<MaterialId>>,
    pub active: usize,
    /// The active slot's level below its top material (empty = top level).
    #[serde(default)]
    pub path: Vec<NavStep>,
    /// Sample Type per slot (absent = Sphere).
    #[serde(default)]
    pub sample_types: BTreeMap<u32, SampleType>,
    #[serde(default)]
    pub options: EditorOptions,
}

pub const SLOT_COUNT: usize = 24;

impl Default for MaterialEditorSlots {
    fn default() -> Self {
        Self { slots: vec![None; SLOT_COUNT], active: 0, path: Vec::new(), sample_types: BTreeMap::new(), options: EditorOptions::default() }
    }
}

impl MaterialEditorSlots {
    pub fn sample_type(&self, slot: usize) -> SampleType {
        self.sample_types.get(&(slot as u32)).copied().unwrap_or_default()
    }
}

/// The scene's material table: top-level materials and sub-materials alike.
pub type MaterialLibrary = BTreeMap<MaterialId, StandardMaterial>;

/// The id a new material gets in `lib`.
pub fn next_material_id(lib: &MaterialLibrary) -> MaterialId {
    MaterialId(lib.keys().next_back().map_or(1, |k| k.0 + 1))
}

/// Where a level path leads: a material (by id), or a map below the
/// material `owner` reached by the map slots `maps` (first in the material,
/// then down colour components).
#[derive(Debug, Clone, PartialEq)]
pub enum Level {
    Material(MaterialId),
    Map { owner: MaterialId, maps: Vec<String> },
}

/// Follows `path` from `top`; stops at the deepest step that exists.
/// Returns the level and how many steps were valid.
pub fn resolve_level(lib: &MaterialLibrary, top: MaterialId, path: &[NavStep]) -> (Level, usize) {
    let mut id = top;
    let mut maps: Vec<String> = Vec::new();
    for (n, step) in path.iter().enumerate() {
        let level = if maps.is_empty() { Level::Material(id) } else { Level::Map { owner: id, maps: maps.clone() } };
        match step {
            NavStep::Sub(i) if maps.is_empty() => match lib.get(&id).and_then(|m| m.children().get(*i as usize).copied().flatten()) {
                Some(child) if lib.contains_key(&child) => id = child,
                _ => return (level, n),
            },
            NavStep::Map(slot) => {
                let exists = if maps.is_empty() {
                    lib.get(&id).and_then(|m| m.map(slot)).is_some()
                } else {
                    lib.get(&id).and_then(|m| map_at(m, &maps)).and_then(|m| m.sub_map_index(slot).and_then(|i| m.sub_map(i))).is_some()
                };
                if !exists {
                    return (level, n);
                }
                maps.push(slot.clone());
            }
            NavStep::Sub(_) => return (level, n),
        }
    }
    let level = if maps.is_empty() { Level::Material(id) } else { Level::Map { owner: id, maps } };
    (level, path.len())
}

/// The map reached from material `m` by the slot names `maps`.
pub fn map_at<'a>(m: &'a StandardMaterial, maps: &[String]) -> Option<&'a Map> {
    let (first, rest) = maps.split_first()?;
    let mut map = m.map(first)?;
    for slot in rest {
        map = map.sub_map(map.sub_map_index(slot)?)?;
    }
    Some(map)
}

pub fn map_at_mut<'a>(m: &'a mut StandardMaterial, maps: &[String]) -> Option<&'a mut Map> {
    let (first, rest) = maps.split_first()?;
    let mut map = m.map_slot_mut(first).ok()?.as_mut()?;
    for slot in rest {
        let i = map.sub_map_index(slot)?;
        map = map.sub_map_slot_mut(i).as_deref_mut()?;
    }
    Some(map)
}

/// The Option holding the map in slot `slot` of the map reached by `maps`
/// (or of the material itself when `maps` is empty).
pub fn map_slot_at_mut<'a>(m: &'a mut StandardMaterial, maps: &[String], slot: &str) -> Result<&'a mut Option<Box<Map>>, String> {
    let parent = map_at_mut(m, maps).ok_or("no such map level")?;
    let i = parent.sub_map_index(slot).filter(|i| parent.slot_has_map(*i)).ok_or_else(|| format!("a {} map has no map component \"{slot}\"", parent.kind.label()))?;
    Ok(parent.sub_map_slot_mut(i))
}

/// What one surface draws with, resolved from a (possibly compound)
/// material: the viewport and the Realistic renderer both read this.
#[derive(Debug, Clone, PartialEq)]
pub struct Surface {
    pub diffuse: Vec3,
    pub ambient: Vec3,
    /// Specular colour × Specular Level / 100.
    pub specular: Vec3,
    pub specular_level: f32,
    pub glossiness: f32,
    /// Self-Illumination 0–1.
    pub self_illum: f32,
    /// Opacity 0–1.
    pub opacity: f32,
    pub two_sided: bool,
    pub wire: bool,
    pub faceted: bool,
    /// Enabled maps only.
    pub diffuse_map: Option<Map>,
    pub opacity_map: Option<Map>,
    pub bump_map: Option<Map>,
    pub specular_level_map: Option<Map>,
    pub self_illum_map: Option<Map>,
    pub show_map_in_viewport: bool,
    /// The shader and its values.
    pub shading: ShaderParams,
    pub extended: ExtendedParams,
    /// Self-Illumination Color mode's colour (black in the Diffuse mode).
    pub self_illum_color: Vec3,
    /// Metalness 0–1 beyond the shader's (Architectural metal templates).
    pub metallic: f32,
    /// Reflection colour (Raytrace Reflect); its value is the amount.
    pub reflect: Vec3,
    pub fresnel: bool,
    /// Refracted share 0–1 (Raytrace Transparency, Architectural Transparency).
    pub transmission: f32,
    /// Light scattered through 0–1 (Architectural / Raytrace translucency).
    pub translucency: f32,
    /// A clear coat over the surface (Shellac) 0–1 and its glossiness.
    pub coat: f32,
    pub coat_glossiness: f32,
    /// Raw Diffuse Texture: no lighting.
    pub unlit: bool,
    pub matte: Option<MatteShadow>,
    pub toon: Option<InkPaint>,
    /// A Falloff map in Diffuse Color: the renderer mixes the diffuse (its
    /// Front) toward the Side colour by the view or light direction.
    pub falloff: Option<FalloffLook>,
    /// Faces of this surface split further (Top/Bottom, Double Sided).
    pub split: Option<Box<Split>>,
}

/// A Falloff map as the renderer draws it per pixel.
#[derive(Debug, Clone, PartialEq)]
pub struct FalloffLook {
    /// 0 Towards/Away, 1 Perpendicular/Parallel, 2 Fresnel, 3 Shadow/Light, 4 Distance Blend.
    pub kind: u8,
    /// The Side colour (sRGB 0–1).
    pub side: Vec3,
    /// A fixed world direction (None: the viewing direction).
    pub direction: Option<Vec3>,
    /// Distance Blend Near / Far, Fresnel IOR.
    pub near: f32,
    pub far: f32,
    pub ior: f32,
}

/// How a surface's faces divide: by their normal (Top/Bottom) or into the
/// front and the back (Double Sided). The surface itself is the top /
/// facing one.
#[derive(Debug, Clone, PartialEq)]
pub enum Split {
    TopBottom { bottom: Surface, local: bool, blend: f32, position: f32 },
    DoubleSided { back: Surface },
}

impl Surface {
    /// A disabled or missing sub-material: black, as the reference renders it.
    pub fn black() -> Self {
        let mut s = Self::from_standard(&StandardMaterial::new(""));
        s.diffuse = Vec3::ZERO;
        s.ambient = Vec3::ZERO;
        s.specular = Vec3::ZERO;
        s.specular_level = 0.0;
        s
    }

    pub fn from_standard(m: &StandardMaterial) -> Self {
        let on = |m: &Option<Map>| m.as_ref().filter(|b| b.enabled).cloned();
        Self {
            diffuse: m.diffuse.get(),
            ambient: m.ambient.get(),
            specular: m.specular.get() * (m.specular_level.get() / 100.0),
            specular_level: m.specular_level.get(),
            glossiness: m.glossiness.get(),
            // The Color mode replaces the amount spinner.
            self_illum: if m.shading.self_illum_color_on { 0.0 } else { (m.self_illum.get() / 100.0).clamp(0.0, 1.0) },
            opacity: (m.opacity.get() / 100.0).clamp(0.0, 1.0),
            two_sided: m.two_sided,
            wire: m.wire,
            faceted: m.faceted,
            diffuse_map: on(&m.diffuse_map),
            opacity_map: on(&m.opacity_map),
            bump_map: on(&m.bump_map),
            specular_level_map: on(&m.specular_level_map),
            self_illum_map: on(&m.self_illum_map),
            show_map_in_viewport: m.show_map_in_viewport,
            shading: m.shading.clone(),
            extended: m.extended.clone(),
            self_illum_color: if m.shading.self_illum_color_on { m.shading.self_illum_color } else { Vec3::ZERO },
            metallic: 0.0,
            reflect: Vec3::ZERO,
            fresnel: false,
            transmission: 0.0,
            translucency: 0.0,
            coat: 0.0,
            coat_glossiness: 0.0,
            unlit: false,
            matte: None,
            toon: None,
            falloff: None,
            split: None,
        }
    }

    /// A surface of a material that draws with its own values: Standard,
    /// Raytrace, Architectural, Matte/Shadow, Ink 'n Paint.
    pub fn from_material(m: &StandardMaterial) -> Self {
        let mut s = Self::from_standard(m);
        match &m.kind {
            MaterialKind::Raytrace(r) => r.apply(&mut s),
            MaterialKind::Architectural(a) => a.apply(&mut s),
            MaterialKind::MatteShadow(ms) => {
                s.matte = Some((**ms).clone());
                s.specular_level = 0.0;
                s.specular = Vec3::ZERO;
            }
            MaterialKind::InkPaint(ink) => {
                s.diffuse = ink.lighted;
                s.ambient = ink.lighted;
                s.glossiness = ink.glossiness;
                s.specular = if ink.highlight_on { ink.highlight } else { Vec3::ZERO };
                s.specular_level = if ink.highlight_on { 100.0 } else { 0.0 };
                s.toon = Some((**ink).clone());
            }
            _ => {}
        }
        s
    }

    /// The diffuse map the viewport shows (Show Map in Viewport on).
    pub fn viewport_map(&self) -> Option<&Map> {
        self.diffuse_map.as_ref().filter(|_| self.show_map_in_viewport)
    }

    /// How the shaded viewports draw this surface with their one Blinn
    /// lobe per vertex: each shader's diffuse and highlight folded in.
    pub fn viewport_look(&self) -> ViewportLook {
        let sh = &self.shading;
        let blinn = |gloss: f32| 2.0 * 2f32.powf(gloss / 10.0);
        let level = self.specular_level / 100.0;
        let mut color = self.diffuse;
        let mut specular = self.specular;
        let mut exponent = blinn(self.glossiness);
        let mut two_sided = self.two_sided;
        if sh.shader.has_diffuse_level() {
            color *= sh.diffuse_level / 100.0;
        }
        if sh.shader.has_roughness() {
            color *= oren_nayar_dim(sh.roughness);
        }
        match sh.shader {
            Shader::Blinn | Shader::OrenNayarBlinn => {}
            // Phong's highlight is about four times wider at the same Glossiness.
            Shader::Phong => exponent /= 4.0,
            // An anisotropic highlight is long one way and narrow the other: on average wider.
            Shader::Anisotropic => exponent *= 1.0 - 0.75 * sh.anisotropy / 100.0,
            Shader::MultiLayer => {
                let second = sh.specular2 * (sh.level2 / 100.0);
                if sh.level2 > self.specular_level {
                    exponent = blinn(sh.glossiness2);
                }
                specular = (specular + second).min(Vec3::ONE);
            }
            // Metal makes its own highlight from the diffuse colour, which dims as the level rises.
            Shader::Metal => {
                specular = self.diffuse * level;
                color *= 1.0 - 0.75 * level.min(1.0);
            }
            Shader::Strauss => {
                let (g, m) = (self.glossiness / 100.0, sh.metalness / 100.0);
                color *= (1.0 - m * g) * (1.0 - g * g * g);
                specular = Vec3::ONE.lerp(self.diffuse, m) * (0.1 + 0.9 * g);
            }
            // Light passes through: both sides show.
            Shader::Translucent => two_sided = true,
        }
        let mut lit = matches!(sh.shader, Shader::Strauss) || self.specular_level > 0.0 || (sh.shader == Shader::MultiLayer && sh.level2 > 0.0);
        // A metal (Architectural templates) or a Raytrace Reflect colour tints the highlight.
        if self.metallic > 0.0 {
            specular = specular.lerp(self.diffuse, self.metallic);
            color *= 1.0 - 0.75 * self.metallic;
        }
        let reflect = self.reflect.max_element();
        if reflect > 0.0 {
            specular = specular.max(self.reflect);
            color *= 1.0 - 0.5 * reflect;
            lit = true;
        }
        if self.coat > 0.0 {
            specular = (specular + Vec3::splat(self.coat)).min(Vec3::ONE);
            lit = true;
        }
        // The Color mode's glow: toward the colour, unlit by its brightness.
        let glow = self.self_illum_color.max_element();
        let (color, mut self_illum) = if glow > 0.0 { (color.lerp(self.self_illum_color, glow), glow) } else { (color, self.self_illum) };
        if self.unlit {
            self_illum = 1.0;
        }
        // Ink 'n Paint: the flat fill mostly unlit.
        if self.toon.is_some() {
            self_illum = self_illum.max(0.6);
        }
        // Refracting surfaces show through in the viewports.
        let alpha = self.opacity * (1.0 - 0.8 * self.transmission.clamp(0.0, 1.0));
        ViewportLook { color, specular: if lit { specular } else { Vec3::ZERO }, exponent: exponent.max(1.0), self_illum, two_sided, alpha }
    }
}

/// Oren-Nayar's darkening of a facing surface at Roughness 0–100 (σ up to 1 radian).
pub fn oren_nayar_dim(roughness: f32) -> f32 {
    let s2 = (roughness / 100.0).powi(2);
    1.0 - 0.5 * s2 / (s2 + 0.33)
}

/// A surface as the shaded viewports draw it (sRGB 0–1 colours).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportLook {
    pub color: Vec3,
    /// The highlight colour (zero: no highlight).
    pub specular: Vec3,
    /// The Blinn exponent.
    pub exponent: f32,
    pub self_illum: f32,
    pub two_sided: bool,
    /// Opacity 0–1 (refracting surfaces show through).
    pub alpha: f32,
}

/// A node's material resolved for drawing: one surface, or one per face
/// material ID (0-based, as faces store it) for a Multi/Sub-Object material.
#[derive(Debug, Clone, PartialEq)]
pub enum Surfaces {
    Single(Box<Surface>),
    /// Face IDs without an entry (or with the sub-material off) draw black.
    PerId(BTreeMap<u16, Surface>),
}

impl Surfaces {
    pub fn for_face(&self, material_id: u16) -> std::borrow::Cow<'_, Surface> {
        match self {
            Surfaces::Single(s) => std::borrow::Cow::Borrowed(s.as_ref()),
            Surfaces::PerId(m) => m.get(&material_id).map_or_else(|| std::borrow::Cow::Owned(Surface::black()), std::borrow::Cow::Borrowed),
        }
    }
}

/// Resolves material `id` for drawing. A nested Multi/Sub uses the same face
/// ID; a Blend mixes its two materials by Mix Amount, its maps through a Mix
/// map with the Blend's mask.
pub fn surfaces(lib: &MaterialLibrary, id: MaterialId) -> Option<Surfaces> {
    surfaces_for(lib, id, ShellView::Viewport)
}

/// As `surfaces`, a Shell material resolving to the material `view` picks.
pub fn surfaces_for(lib: &MaterialLibrary, id: MaterialId, view: ShellView) -> Option<Surfaces> {
    // The material a Shell shows, followed through nested Shells.
    let pick = |sh: &Shell| match view {
        ShellView::Viewport => sh.materials[sh.viewport.min(1) as usize],
        ShellView::Render => sh.materials[sh.render.min(1) as usize],
        ShellView::Original => sh.materials[0],
    };
    let mut id = id;
    for _ in 0..16 {
        match &lib.get(&id)?.kind {
            MaterialKind::Shell(sh) => id = pick(sh)?,
            _ => break,
        }
    }
    fn single(lib: &MaterialLibrary, id: Option<MaterialId>, face: Option<u16>, depth: u32, view: ShellView) -> Surface {
        let Some(m) = id.and_then(|i| lib.get(&i)).filter(|_| depth < 16) else { return Surface::black() };
        let sub = |i: Option<MaterialId>, on: bool| if on { single(lib, i, face, depth + 1, view) } else { Surface::black() };
        match &m.kind {
            MaterialKind::Standard | MaterialKind::Raytrace(_) | MaterialKind::Architectural(_) | MaterialKind::MatteShadow(_) | MaterialKind::InkPaint(_) => Surface::from_material(m),
            MaterialKind::DoubleSided(d) => {
                let (mut front, mut back) = (sub(d.materials[0], d.enabled[0]), sub(d.materials[1], d.enabled[1]));
                front.split = None;
                back.split = None;
                // Translucency: each side shows that much of the other.
                let t = (d.translucency / 100.0).clamp(0.0, 1.0);
                let (f, b) = (blend_surfaces(&front, &back, t), blend_surfaces(&back, &front, t));
                let mut out = f;
                out.two_sided = false;
                let mut back = b;
                back.two_sided = false;
                out.split = Some(Box::new(Split::DoubleSided { back }));
                out
            }
            MaterialKind::TopBottom(t) => {
                let mut top = sub(t.materials[0], t.enabled[0]);
                let mut bottom = sub(t.materials[1], t.enabled[1]);
                top.split = None;
                bottom.split = None;
                top.split = Some(Box::new(Split::TopBottom { bottom, local: t.local, blend: t.blend, position: t.position }));
                top
            }
            MaterialKind::Composite(c) => {
                let base = single(lib, c.base, face, depth + 1, view);
                let layers: Vec<(Surface, u8, f32)> = c.layers.iter().filter(|l| l.enabled && l.material.is_some()).map(|l| (single(lib, l.material, face, depth + 1, view), l.mode, l.amount)).collect();
                composite(base, &layers)
            }
            MaterialKind::Shellac(sh) => {
                let base = single(lib, sh.materials[0], face, depth + 1, view);
                let coat = single(lib, sh.materials[1], face, depth + 1, view);
                shellac(base, &coat, sh.blend)
            }
            MaterialKind::MultiSub(list) => match list.iter().find(|s| Some(s.id) == face.map(|f| f + 1)) {
                Some(s) if s.enabled => single(lib, s.material, face, depth + 1, view),
                _ => Surface::black(),
            },
            MaterialKind::Blend(b) => {
                let a = if b.enabled[0] { single(lib, b.materials[0], face, depth + 1, view) } else { Surface::black() };
                let c = if b.enabled[1] { single(lib, b.materials[1], face, depth + 1, view) } else { Surface::black() };
                blend(&a, &c, b)
            }
            MaterialKind::Shell(sh) => {
                let i = match view {
                    ShellView::Viewport => sh.viewport,
                    ShellView::Render => sh.render,
                    ShellView::Original => 0,
                };
                single(lib, sh.materials[i.min(1) as usize], face, depth + 1, view)
            }
        }
    }
    let m = lib.get(&id)?;
    Some(match &m.kind {
        MaterialKind::MultiSub(list) => Surfaces::PerId(list.iter().map(|s| (s.id.saturating_sub(1), single(lib, Some(id), Some(s.id.saturating_sub(1)), 0, view))).collect()),
        _ => Surfaces::Single(Box::new(single(lib, Some(id), None, 0, view))),
    })
}

/// Top/Bottom bands a blended boundary is drawn with, bottom to top.
pub const TOP_BOTTOM_BANDS: usize = 4;

/// Top/Bottom surfaces turned into per-face-ID ones: what `split_surfaces`
/// returns with them, for `apply_split` to give each face its virtual ID.
#[derive(Debug, Clone, PartialEq)]
pub struct FaceSplit {
    /// Per original face material ID (None: one surface for every face):
    /// (Local, Blend, Position, the virtual IDs of the bands).
    entries: Vec<(Option<u16>, bool, f32, f32, [u16; TOP_BOTTOM_BANDS])>,
}

/// The ID faces get when their original ID has no surface: drawn black.
const NO_SURFACE: u16 = u16::MAX;

/// Top/Bottom surfaces resolved without the mesh: a per-ID surface for
/// every band of every Top/Bottom surface (blends of its top and bottom),
/// or None when there is no Top/Bottom surface. `apply_split` then
/// renumbers the mesh's faces by their normals, so both renderers draw
/// Top/Bottom through their per-material-ID path.
pub fn split_surfaces(surfaces: &Surfaces) -> Option<(Surfaces, FaceSplit)> {
    let list: Vec<(Option<u16>, &Surface)> = match surfaces {
        Surfaces::Single(s) => vec![(None, s.as_ref())],
        Surfaces::PerId(m) => m.iter().map(|(k, s)| (Some(*k), s)).collect(),
    };
    if !list.iter().any(|(_, s)| matches!(s.split.as_deref(), Some(Split::TopBottom { .. }))) {
        return None;
    }
    let mut out = BTreeMap::new();
    let mut entries = Vec::new();
    for (key, s) in list {
        let next = out.len() as u16;
        match s.split.as_deref() {
            Some(Split::TopBottom { bottom, local, blend, position }) => {
                let mut top = s.clone();
                top.split = None;
                let mut ids = [0u16; TOP_BOTTOM_BANDS];
                for (k, id) in ids.iter_mut().enumerate() {
                    *id = next + k as u16;
                    let t = k as f32 / (TOP_BOTTOM_BANDS - 1) as f32;
                    out.insert(*id, blend_surfaces(bottom, &top, t));
                }
                entries.push((key, *local, *blend, *position, ids));
            }
            _ => {
                out.insert(next, s.clone());
                entries.push((key, false, 0.0, 0.0, [next; TOP_BOTTOM_BANDS]));
            }
        }
    }
    Some((Surfaces::PerId(out), FaceSplit { entries }))
}

/// Gives each face of `mesh` its virtual ID from `split`: a Top/Bottom
/// face's band comes from its normal against `world_up` (World) or
/// `local_up` (Local, the object's Z), both in the mesh's space.
pub fn apply_split(mesh: &mut loom_mesh::Mesh, split: &FaceSplit, world_up: Vec3, local_up: Vec3) {
    let single = split.entries.first().is_some_and(|e| e.0.is_none());
    let ids: Vec<u16> = (0..mesh.faces().len())
        .map(|i| {
            let key = if single { None } else { Some(mesh.faces()[i].material_id) };
            let Some((_, local, blend, position, bands)) = split.entries.iter().find(|e| e.0 == key) else { return NO_SURFACE };
            if bands.iter().all(|b| *b == bands[0]) {
                return bands[0];
            }
            let up = if *local { local_up } else { world_up }.normalize_or_zero();
            let nz = mesh.face_normal(loom_mesh::FaceId(i as u32)).dot(up);
            // Position 0: all top, 100: all bottom; Blend widens the boundary.
            let z0 = position / 50.0 - 1.0;
            let w = blend / 100.0;
            let t = if w <= 0.0 {
                if nz > z0 || *position <= 0.0 { 1.0 } else { 0.0 }
            } else {
                ((nz - (z0 - w)) / (2.0 * w)).clamp(0.0, 1.0)
            };
            bands[(t * (TOP_BOTTOM_BANDS - 1) as f32).round() as usize]
        })
        .collect();
    for (f, id) in mesh.faces_mut().iter_mut().zip(ids) {
        f.material_id = id;
    }
}

/// `a` and `b` mixed by `t` (colours and values; maps and switches from the nearer one).
pub fn blend_surfaces(a: &Surface, b: &Surface, t: f32) -> Surface {
    if t <= 0.0 {
        return a.clone();
    }
    let p = Blend { materials: [None; 2], enabled: [true; 2], mask: None, mix_amount: Param::new(t * 100.0), interactive: 0, use_curve: false, upper: 0.75, lower: 0.25 };
    blend(a, b, &p)
}

fn blend(a: &Surface, b: &Surface, p: &Blend) -> Surface {
    let mask = p.mask.as_ref().filter(|m| m.enabled);
    let t = (p.mix_amount.get() / 100.0).clamp(0.0, 1.0);
    // Light parts of the mask show Material 1: averaged values use half and half.
    let t = if mask.is_some() { 0.5 } else { t };
    let lerp = |x: f32, y: f32| x + (y - x) * t;
    let lerp3 = |x: Vec3, y: Vec3| x.lerp(y, t);
    let diffuse_map = if a.diffuse_map.is_some() || b.diffuse_map.is_some() || mask.is_some() {
        let mut mix = Map::new(MapType::Mix, 100.0);
        mix.colors = [a.diffuse, b.diffuse, Vec3::ONE];
        mix.sub_maps = [a.diffuse_map.clone().map(Box::new), b.diffuse_map.clone().map(Box::new), None];
        match mask {
            // Mix shows Color #2 where its amount map is white; the Blend mask shows Material 1 there.
            Some(m) => {
                mix.colors.swap(0, 1);
                mix.sub_maps.swap(0, 1);
                mix.sub_maps[2] = Some(Box::new(m.clone()));
            }
            None => mix.params.mix_amount = t * 100.0,
        }
        // A sub-map's Amount: the Bitmap fills its colour component.
        for s in mix.sub_maps.iter_mut().take(2).flatten() {
            s.amount = 100.0;
        }
        Some(mix)
    } else {
        None
    };
    Surface {
        diffuse: lerp3(a.diffuse, b.diffuse),
        ambient: lerp3(a.ambient, b.ambient),
        specular: lerp3(a.specular, b.specular),
        specular_level: lerp(a.specular_level, b.specular_level),
        glossiness: lerp(a.glossiness, b.glossiness),
        self_illum: lerp(a.self_illum, b.self_illum),
        opacity: lerp(a.opacity, b.opacity),
        two_sided: a.two_sided || b.two_sided,
        // One wire sub-material makes the whole Blend wire.
        wire: a.wire || b.wire,
        faceted: a.faceted && b.faceted,
        show_map_in_viewport: diffuse_map.is_some() && (a.show_map_in_viewport || b.show_map_in_viewport || mask.is_some()),
        diffuse_map,
        opacity_map: if t < 0.5 { a.opacity_map.clone() } else { b.opacity_map.clone() },
        bump_map: if t < 0.5 { a.bump_map.clone() } else { b.bump_map.clone() },
        specular_level_map: if t < 0.5 { a.specular_level_map.clone() } else { b.specular_level_map.clone() },
        self_illum_map: if t < 0.5 { a.self_illum_map.clone() } else { b.self_illum_map.clone() },
        shading: if t < 0.5 { a.shading.clone() } else { b.shading.clone() },
        extended: if t < 0.5 { a.extended.clone() } else { b.extended.clone() },
        self_illum_color: lerp3(a.self_illum_color, b.self_illum_color),
        metallic: lerp(a.metallic, b.metallic),
        reflect: lerp3(a.reflect, b.reflect),
        fresnel: if t < 0.5 { a.fresnel } else { b.fresnel },
        transmission: lerp(a.transmission, b.transmission),
        translucency: lerp(a.translucency, b.translucency),
        coat: lerp(a.coat, b.coat),
        coat_glossiness: lerp(a.coat_glossiness, b.coat_glossiness),
        unlit: if t < 0.5 { a.unlit } else { b.unlit },
        matte: if t < 0.5 { a.matte.clone() } else { b.matte.clone() },
        toon: if t < 0.5 { a.toon.clone() } else { b.toon.clone() },
        falloff: if t < 0.5 { a.falloff.clone() } else { b.falloff.clone() },
        split: if t < 0.5 { a.split.clone() } else { b.split.clone() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib_with(ms: Vec<StandardMaterial>) -> MaterialLibrary {
        ms.into_iter().enumerate().map(|(i, m)| (MaterialId(i as u32 + 1), m)).collect()
    }

    #[test]
    fn multi_sub_resolves_per_face_id_and_missing_ids_are_black() {
        let mut red = StandardMaterial::new("red");
        red.diffuse.set(Vec3::X);
        let mut green = StandardMaterial::new("green");
        green.diffuse.set(Vec3::Y);
        let mut multi = StandardMaterial::new("multi");
        multi.kind = MaterialKind::MultiSub(vec![
            SubMaterial { id: 1, name: String::new(), material: Some(MaterialId(1)), enabled: true },
            SubMaterial { id: 3, name: String::new(), material: Some(MaterialId(2)), enabled: true },
            SubMaterial { id: 4, name: String::new(), material: Some(MaterialId(2)), enabled: false },
        ]);
        let lib = lib_with(vec![red, green, multi]);
        let s = surfaces(&lib, MaterialId(3)).unwrap();
        assert_eq!(s.for_face(0).diffuse, Vec3::X, "face ID 1 → sub-material ID 1");
        assert_eq!(s.for_face(2).diffuse, Vec3::Y, "face ID 3 → sub-material ID 3");
        assert_eq!(s.for_face(1).diffuse, Vec3::ZERO, "no ID 2: black");
        assert_eq!(s.for_face(3).diffuse, Vec3::ZERO, "ID 4 is off: black");
    }

    #[test]
    fn blend_mixes_by_amount_and_a_mask_goes_through_a_mix_map() {
        let mut a = StandardMaterial::new("a");
        a.diffuse.set(Vec3::X);
        let mut b = StandardMaterial::new("b");
        b.diffuse.set(Vec3::Z);
        b.diffuse_map = Some(Map::bitmap("/x/sign.png", 100.0));
        let mut blend_m = StandardMaterial::new("blend");
        blend_m.kind = MaterialKind::Blend(Box::new(Blend { materials: [Some(MaterialId(1)), Some(MaterialId(2))], enabled: [true; 2], mask: None, mix_amount: Param::new(25.0), interactive: 0, use_curve: false, upper: 0.75, lower: 0.25 }));
        let mut lib = lib_with(vec![a, b, blend_m]);
        let Surfaces::Single(s) = surfaces(&lib, MaterialId(3)).unwrap() else { panic!() };
        assert!((s.diffuse - Vec3::new(0.75, 0.0, 0.25)).length() < 1e-5);
        let mix = s.diffuse_map.expect("material 2's map shows through a Mix map");
        assert_eq!(mix.kind, MapType::Mix);
        assert_eq!(mix.params.mix_amount, 25.0);
        assert_eq!(mix.sub_maps[1].as_ref().unwrap().path, "/x/sign.png");
        if let MaterialKind::Blend(bl) = &mut lib.get_mut(&MaterialId(3)).unwrap().kind {
            bl.mask = Some(Map::bitmap("/x/mask.png", 100.0));
        }
        let Surfaces::Single(s) = surfaces(&lib, MaterialId(3)).unwrap() else { panic!() };
        let mix = s.diffuse_map.unwrap();
        assert_eq!(mix.sub_maps[2].as_ref().unwrap().path, "/x/mask.png");
        assert_eq!(mix.colors[1], Vec3::X, "white in the mask shows Material 1");
    }

    #[test]
    fn a_shell_shows_its_viewport_or_render_choice() {
        let mut orig = StandardMaterial::new("orig");
        orig.diffuse.set(Vec3::X);
        let mut baked = StandardMaterial::new("baked");
        baked.diffuse.set(Vec3::Y);
        let mut shell = StandardMaterial::new("shell");
        shell.kind = MaterialKind::Shell(Box::new(Shell { materials: [Some(MaterialId(1)), Some(MaterialId(2))], viewport: 1, render: 0 }));
        let mut lib = lib_with(vec![orig, baked, shell]);
        let diffuse = |lib: &MaterialLibrary, v: ShellView| match surfaces_for(lib, MaterialId(3), v).unwrap() {
            Surfaces::Single(s) => s.diffuse,
            Surfaces::PerId(_) => panic!("single"),
        };
        assert_eq!(diffuse(&lib, ShellView::Viewport), Vec3::Y, "viewports show the baked material");
        assert_eq!(diffuse(&lib, ShellView::Render), Vec3::X, "renders the original");
        assert_eq!(diffuse(&lib, ShellView::Original), Vec3::X);
        lib.get_mut(&MaterialId(3)).unwrap().set_parameter("Viewport", 0.0).unwrap();
        assert_eq!(diffuse(&lib, ShellView::Viewport), Vec3::X);
        assert_eq!(lib[&MaterialId(3)].children(), vec![Some(MaterialId(1)), Some(MaterialId(2))]);
    }

    fn single_of(lib: &MaterialLibrary, id: u32) -> Surface {
        match surfaces(lib, MaterialId(id)).unwrap() {
            Surfaces::Single(s) => *s,
            Surfaces::PerId(_) => panic!("single"),
        }
    }

    fn coloured(name: &str, c: Vec3) -> StandardMaterial {
        let mut m = StandardMaterial::new(name);
        m.diffuse.set(c);
        m.ambient.set(c);
        m
    }

    #[test]
    fn raytrace_and_architectural_resolve_to_pbr_hints() {
        let mut rt = StandardMaterial::new("rt");
        rt.kind = MaterialKind::Raytrace(Box::default());
        rt.set_color("Reflect", [255, 0, 0]).unwrap();
        rt.set_color("Transparency", [128, 128, 128]).unwrap();
        rt.set_color("Luminosity", [0, 0, 255]).unwrap();
        rt.set_parameter("Index of Refr.", 2.0).unwrap();
        rt.set_color("Diffuse", [0, 255, 0]).unwrap();
        let mut arch = StandardMaterial::new("arch");
        arch.kind = MaterialKind::Architectural(Box::default());
        arch.set_parameter("Template", 11.0).unwrap();
        let lib = lib_with(vec![rt, arch]);
        let s = single_of(&lib, 1);
        assert_eq!(s.reflect, Vec3::X);
        assert!((s.transmission - 128.0 / 255.0).abs() < 1e-5 && s.viewport_look().alpha < 0.6, "transparency refracts and the viewports show through it");
        assert_eq!(s.self_illum_color, Vec3::Z, "luminosity glows");
        assert_eq!(s.extended.ior, 2.0);
        assert_eq!(s.diffuse, Vec3::Y, "Diffuse is the Standard value");
        let a = single_of(&lib, 2);
        assert_eq!(crate::material_types::ARCH_TEMPLATES[11].0, "Mirror");
        assert_eq!((a.metallic, a.glossiness), (1.0, 100.0), "the Mirror template is a polished metal");
    }

    #[test]
    fn composite_and_shellac_combine_colours() {
        let mut comp = StandardMaterial::new("comp");
        comp.kind = MaterialKind::Composite(Box::new(Composite {
            base: Some(MaterialId(1)),
            layers: vec![crate::material_types::CompositeLayer { material: Some(MaterialId(2)), enabled: true, mode: 0, amount: 100.0 }, crate::material_types::CompositeLayer { material: None, enabled: true, mode: 0, amount: 100.0 }],
        }));
        let mut sh = StandardMaterial::new("shellac");
        sh.kind = MaterialKind::Shellac(Box::new(Shellac { materials: [Some(MaterialId(1)), Some(MaterialId(2))], blend: 50.0 }));
        let lib = lib_with(vec![coloured("red", Vec3::new(0.5, 0.0, 0.0)), coloured("blue", Vec3::new(0.0, 0.0, 0.5)), comp, sh]);
        assert_eq!(single_of(&lib, 3).diffuse, Vec3::new(0.5, 0.0, 0.5), "A adds");
        let mut lib2 = lib.clone();
        lib2.get_mut(&MaterialId(3)).unwrap().set_parameter("Mat 1 Mode", 1.0).unwrap();
        assert_eq!(single_of(&lib2, 3).diffuse, Vec3::new(0.5, 0.0, 0.0), "S subtracts (clamped at black)");
        lib2.get_mut(&MaterialId(3)).unwrap().set_parameter("Mat 1 Mode", 2.0).unwrap();
        lib2.get_mut(&MaterialId(3)).unwrap().set_parameter("Mat 1 Amount", 150.0).unwrap();
        let MaterialKind::Composite(c) = &lib2[&MaterialId(3)].kind else { panic!() };
        assert_eq!(c.layers[0].amount, 100.0, "Mix amounts stop at 100");
        assert_eq!(single_of(&lib, 4).diffuse, Vec3::new(0.5, 0.0, 0.25), "Shellac Color Blend 50 adds half the shellac colour");
    }

    #[test]
    fn double_sided_and_top_bottom_split_the_faces() {
        let mut ds = StandardMaterial::new("ds");
        ds.kind = MaterialKind::DoubleSided(Box::new(DoubleSided { materials: [Some(MaterialId(1)), Some(MaterialId(2))], enabled: [true; 2], translucency: 0.0 }));
        let mut tb = StandardMaterial::new("tb");
        tb.kind = MaterialKind::TopBottom(Box::new(TopBottom { materials: [Some(MaterialId(1)), Some(MaterialId(2))], enabled: [true; 2], local: false, blend: 0.0, position: 50.0 }));
        let mut lib = lib_with(vec![coloured("red", Vec3::X), coloured("blue", Vec3::Z), ds, tb]);
        let front = single_of(&lib, 3);
        assert_eq!(front.diffuse, Vec3::X);
        let Some(Split::DoubleSided { back }) = front.split.as_deref() else { panic!("a back surface") };
        assert_eq!(back.diffuse, Vec3::Z);
        lib.get_mut(&MaterialId(3)).unwrap().set_parameter("Translucency", 50.0).unwrap();
        assert!((single_of(&lib, 3).diffuse - Vec3::new(0.5, 0.0, 0.5)).length() < 1e-5, "translucency shows half the back");

        // Top/Bottom on a box: the top faces red, the bottom and (Position 50) the sides... by their normals.
        let s = surfaces(&lib, MaterialId(4)).unwrap();
        let (per_id, split) = split_surfaces(&s).expect("a Top/Bottom split");
        let mut mesh = loom_mesh::box_primitive(Vec3::splat(5.0));
        apply_split(&mut mesh, &split, Vec3::Z, Vec3::Z);
        let colour_of = |mesh: &loom_mesh::Mesh, n: Vec3| {
            let i = (0..mesh.faces().len()).find(|i| mesh.face_normal(loom_mesh::FaceId(*i as u32)).dot(n) > 0.99).unwrap();
            per_id.for_face(mesh.faces()[i].material_id).diffuse
        };
        assert_eq!(colour_of(&mesh, Vec3::Z), Vec3::X, "the top face shows the top material");
        assert_eq!(colour_of(&mesh, -Vec3::Z), Vec3::Z, "the bottom face the bottom one");
        assert_eq!(colour_of(&mesh, Vec3::X), Vec3::Z, "at Position 50 a side (normal z 0) is not above the boundary");
        // Position 100: everything bottom; Local follows the object's up.
        lib.get_mut(&MaterialId(4)).unwrap().set_parameter("Position", 100.0).unwrap();
        let (per_id, split) = split_surfaces(&surfaces(&lib, MaterialId(4)).unwrap()).unwrap();
        let mut mesh = loom_mesh::box_primitive(Vec3::splat(5.0));
        apply_split(&mut mesh, &split, Vec3::Z, Vec3::Z);
        assert!(mesh.faces().iter().all(|f| per_id.for_face(f.material_id).diffuse == Vec3::Z));
        lib.get_mut(&MaterialId(4)).unwrap().set_parameter("Position", 50.0).unwrap();
        lib.get_mut(&MaterialId(4)).unwrap().set_parameter("Local", 1.0).unwrap();
        let (per_id, split) = split_surfaces(&surfaces(&lib, MaterialId(4)).unwrap()).unwrap();
        let mut mesh = loom_mesh::box_primitive(Vec3::splat(5.0));
        apply_split(&mut mesh, &split, Vec3::Z, -Vec3::Z);
        assert_eq!(colour_of(&mesh, -Vec3::Z), Vec3::X, "an upside-down object's local top is world down");
        let _ = per_id;
    }

    #[test]
    fn ink_n_paint_bands_and_matte_shadow_flags() {
        let ink = InkPaint::default();
        assert_eq!(ink.paint(1.0), ink.lighted);
        assert_eq!(ink.paint(0.2), ink.lighted * 0.7, "two Paint Levels: the shaded band is 70% of Lighted");
        let mut m = StandardMaterial::new("ink");
        m.kind = MaterialKind::InkPaint(Box::default());
        m.set_color("Lighted", [255, 0, 0]).unwrap();
        m.set_parameter("Paint Levels", 4.0).unwrap();
        let mut matte = StandardMaterial::new("matte");
        matte.kind = MaterialKind::MatteShadow(Box::default());
        matte.set_parameter("Shadow Brightness", 0.3).unwrap();
        let lib = lib_with(vec![m, matte]);
        let s = single_of(&lib, 1);
        assert_eq!((s.diffuse, s.toon.as_ref().unwrap().paint_levels), (Vec3::X, 4));
        let s = single_of(&lib, 2);
        assert_eq!(s.matte.as_ref().unwrap().shadow_brightness, 0.3);
    }

    #[test]
    fn viewport_looks_follow_the_shader() {
        let mut m = StandardMaterial::new("m");
        m.diffuse.set(Vec3::new(0.8, 0.4, 0.2));
        m.specular_level.set(100.0);
        m.glossiness.set(40.0);
        let look = |m: &StandardMaterial| Surface::from_standard(m).viewport_look();
        let blinn = look(&m);
        assert_eq!(blinn.color, m.diffuse.get());
        assert_eq!(blinn.exponent, 2.0 * 2f32.powf(4.0));
        m.shading.shader = Shader::Phong;
        assert!(look(&m).exponent < blinn.exponent, "Phong's highlight is wider");
        m.shading.shader = Shader::Metal;
        let metal = look(&m);
        assert!((metal.specular - m.diffuse.get()).length() < 1e-5, "Metal's highlight takes the diffuse colour");
        assert!(metal.color.x < blinn.color.x, "and the diffuse dims");
        m.shading.shader = Shader::OrenNayarBlinn;
        m.shading.diffuse_level = 200.0;
        m.shading.roughness = 100.0;
        let onb = look(&m);
        assert!((onb.color.x - 1.6 * oren_nayar_dim(100.0)).abs() < 1e-5, "{onb:?}");
        m.shading.shader = Shader::Strauss;
        m.specular_level.set(0.0);
        m.shading.metalness = 100.0;
        let strauss = look(&m);
        assert!(strauss.specular.length() > 0.0, "Strauss always has a highlight");
        assert!(strauss.specular.x > strauss.specular.z, "a metal Strauss highlight is tinted by the colour");
        m.shading.shader = Shader::Translucent;
        assert!(look(&m).two_sided);
        m.shading.shader = Shader::MultiLayer;
        m.shading.level2 = 50.0;
        m.shading.specular2 = Vec3::Z;
        assert!(look(&m).specular.z > 0.4, "the second layer adds its colour");
        m.shading.shader = Shader::Blinn;
        m.shading.self_illum_color_on = true;
        m.shading.self_illum_color = Vec3::new(0.0, 1.0, 0.0);
        let glow = look(&m);
        assert_eq!((glow.color, glow.self_illum), (Vec3::Y, 1.0), "a full-value glow colour shows unlit");
    }

    #[test]
    fn coordinates_offset_tile_and_rotate() {
        let mut c = MapCoords::default();
        assert_eq!(c.apply([0.25, 0.75]), [0.25, 0.75]);
        c.tiling = [2.0, 3.0];
        let p = c.apply([0.75, 0.5]);
        assert!((p[0] - 1.0).abs() < 1e-6 && (p[1] - 0.5).abs() < 1e-6, "{p:?}");
        c = MapCoords { offset: [-1.0, 0.5], ..Default::default() };
        let p = c.apply([0.0, 0.0]);
        assert!((p[0] - 1.0).abs() < 1e-6 && (p[1] + 0.5).abs() < 1e-6, "a -1 U offset moves the map a full width left: {p:?}");
        c = MapCoords { angle: 90.0, ..Default::default() };
        let p = c.apply([1.0, 0.5]);
        assert!((p[0] - 0.5).abs() < 1e-5 && (p[1] - 0.0).abs() < 1e-5, "{p:?}");
        let m = c.matrix();
        let q = [m[0] + 0.5 * m[1] + m[2], m[3] + 0.5 * m[4] + m[5]];
        assert!((q[0] - p[0]).abs() < 1e-5 && (q[1] - p[1]).abs() < 1e-5);
        let mut map = Map::new(MapType::Bitmap, 100.0);
        map.set_parameter("Mirror U", 1.0).unwrap();
        assert!(map.coords.mirror[0] && !map.coords.tile[0], "Mirror turns Tile off");
        map.set_parameter("Tile U", 0.0).unwrap();
        map.set_parameter("Mirror U", 0.0).unwrap();
        assert_eq!(map.coords.decal(), [true, false]);
    }

    #[test]
    fn levels_resolve_through_sub_materials_and_maps() {
        let mut sub = StandardMaterial::new("sub");
        let mut mix = Map::new(MapType::Mix, 100.0);
        mix.sub_maps[0] = Some(Box::new(Map::bitmap("/x/a.png", 100.0)));
        sub.diffuse_map = Some(mix);
        let mut multi = StandardMaterial::new("multi");
        multi.kind = MaterialKind::MultiSub(vec![SubMaterial { id: 1, name: String::new(), material: Some(MaterialId(1)), enabled: true }]);
        let lib = lib_with(vec![sub, multi]);
        let path = vec![NavStep::Sub(0), NavStep::Map("Diffuse Color".into()), NavStep::Map("Color #1".into())];
        let (level, n) = resolve_level(&lib, MaterialId(2), &path);
        assert_eq!(n, 3);
        assert_eq!(level, Level::Map { owner: MaterialId(1), maps: vec!["Diffuse Color".into(), "Color #1".into()] });
        assert_eq!(map_at(&lib[&MaterialId(1)], &["Diffuse Color".into(), "Color #1".into()]).unwrap().path, "/x/a.png");
        // A step that no longer exists stops the walk there.
        let (level, n) = resolve_level(&lib, MaterialId(2), &[NavStep::Sub(4)]);
        assert_eq!((level, n), (Level::Material(MaterialId(2)), 0));
    }
}
