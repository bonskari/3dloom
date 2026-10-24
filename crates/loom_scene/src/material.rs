//! Materials: the Standard material with the Blinn shader, the compound
//! Multi/Sub-Object and Blend materials, the 2D maps (Bitmap, Checker,
//! Gradient, Noise, Mix), the scene's material library and the Material
//! Editor's 24 sample slots with the level being edited.

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
}

impl MapType {
    pub const ALL: [MapType; 5] = [MapType::Bitmap, MapType::Checker, MapType::Gradient, MapType::Mix, MapType::Noise];

    pub fn label(self) -> &'static str {
        match self {
            MapType::Bitmap => "Bitmap",
            MapType::Checker => "Checker",
            MapType::Gradient => "Gradient",
            MapType::Noise => "Noise",
            MapType::Mix => "Mix",
        }
    }

    pub fn from_label(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.label() == s)
    }

    /// The names of the map's colour components (each can hold a sub-map).
    pub fn color_slots(self) -> &'static [&'static str] {
        match self {
            MapType::Bitmap => &[],
            MapType::Checker | MapType::Noise => &["Color #1", "Color #2"],
            MapType::Gradient => &["Color #1", "Color #2", "Color #3"],
            MapType::Mix => &["Color #1", "Color #2", "Mix Amount"],
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
}

impl Default for MapParams {
    fn default() -> Self {
        Self { soften: 0.0, position: 0.5, radial: false, noise_type: 0, size: 25.0, levels: 3.0, phase: 0.0, high: 1.0, low: 0.0, mix_amount: 0.0 }
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
            _ => [Vec3::ZERO, Vec3::ONE, Vec3::ONE],
        };
        Self { path: String::new(), amount, enabled: true, kind, name: String::new(), coords: MapCoords::default(), crop: Cropping::default(), colors, sub_maps: [None, None, None], params: MapParams::default() }
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
        self.kind.color_slots().iter().position(|s| *s == slot)
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
            "Map Channel" => c.map_channel = (value.round() as u32).clamp(1, 99),
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
            "Amount" => self.amount = v,
            "Enabled" => self.enabled = on,
            other => {
                if let Some(slot) = other.strip_suffix(" Map On") {
                    let i = self.sub_map_index(slot).ok_or_else(|| format!("no map component \"{slot}\""))?;
                    if let Some(m) = self.sub_maps[i].as_mut() {
                        m.enabled = on;
                    }
                    return Ok(());
                }
                return Err(format!("no map parameter \"{other}\""));
            }
        }
        Ok(())
    }

    /// Color #1–#3 from 0–255 RGB.
    pub fn set_color(&mut self, which: &str, rgb: [u8; 3]) -> Result<(), String> {
        let i = match which {
            "Color #1" => 0,
            "Color #2" => 1,
            "Color #3" => 2,
            other => return Err(format!("no map colour \"{other}\" (Color #1, Color #2, Color #3)")),
        };
        self.colors[i] = Vec3::new(rgb[0] as f32, rgb[1] as f32, rgb[2] as f32) / 255.0;
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

/// Material type (the Type button).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum MaterialKind {
    #[default]
    Standard,
    MultiSub(Vec<SubMaterial>),
    Blend(Box<Blend>),
}

impl MaterialKind {
    pub const TYPES: [&'static str; 3] = ["Blend", "Multi/Sub-Object", "Standard"];

    pub fn label(&self) -> &'static str {
        match self {
            MaterialKind::Standard => "Standard",
            MaterialKind::MultiSub(_) => "Multi/Sub-Object",
            MaterialKind::Blend(_) => "Blend",
        }
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
            MaterialKind::Standard => MAP_CHANNELS.iter().map(|c| c.0).collect(),
            MaterialKind::Blend(_) => vec!["Mask"],
            MaterialKind::MultiSub(_) => Vec::new(),
        }
    }

    /// The sub-materials this level holds, in list order (Blend: Material 1, 2).
    pub fn children(&self) -> Vec<Option<MaterialId>> {
        match &self.kind {
            MaterialKind::Standard => Vec::new(),
            MaterialKind::MultiSub(list) => list.iter().map(|s| s.material).collect(),
            MaterialKind::Blend(b) => b.materials.to_vec(),
        }
    }

    /// Sets a Blinn Basic Parameters / Shader Basic Parameters value by its
    /// label, clamped to its spinner range (booleans as 0 / 1). Blend values
    /// (Mix Amount, Interactive, Use Curve, Upper, Lower, Material 1/2 On,
    /// Mask On) apply to a Blend material.
    pub fn set_parameter(&mut self, name: &str, value: f64) -> Result<(), String> {
        let v = value as f32;
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
            other => return Err(format!("no material parameter \"{other}\"")),
        }
        Ok(())
    }

    /// Sets Ambient, Diffuse or Specular from 0–255 RGB; Ambient follows
    /// Diffuse while they are locked.
    pub fn set_color(&mut self, which: &str, rgb: [u8; 3]) -> Result<(), String> {
        let c = Vec3::new(rgb[0] as f32, rgb[1] as f32, rgb[2] as f32) / 255.0;
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
            }
            "Specular" => self.specular.set(c),
            other => return Err(format!("no colour \"{other}\" (Ambient, Diffuse, Specular)")),
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
                    lib.get(&id).and_then(|m| map_at(m, &maps)).and_then(|m| m.sub_map_index(slot).and_then(|i| m.sub_maps[i].as_ref())).is_some()
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
        map = map.sub_maps[map.sub_map_index(slot)?].as_deref()?;
    }
    Some(map)
}

pub fn map_at_mut<'a>(m: &'a mut StandardMaterial, maps: &[String]) -> Option<&'a mut Map> {
    let (first, rest) = maps.split_first()?;
    let mut map = m.map_slot_mut(first).ok()?.as_mut()?;
    for slot in rest {
        let i = map.sub_map_index(slot)?;
        map = map.sub_maps[i].as_deref_mut()?;
    }
    Some(map)
}

/// The Option holding the map in slot `slot` of the map reached by `maps`
/// (or of the material itself when `maps` is empty).
pub fn map_slot_at_mut<'a>(m: &'a mut StandardMaterial, maps: &[String], slot: &str) -> Result<&'a mut Option<Box<Map>>, String> {
    let parent = map_at_mut(m, maps).ok_or("no such map level")?;
    let i = parent.sub_map_index(slot).ok_or_else(|| format!("a {} map has no component \"{slot}\"", parent.kind.label()))?;
    Ok(&mut parent.sub_maps[i])
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
            self_illum: (m.self_illum.get() / 100.0).clamp(0.0, 1.0),
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
        }
    }

    /// The diffuse map the viewport shows (Show Map in Viewport on).
    pub fn viewport_map(&self) -> Option<&Map> {
        self.diffuse_map.as_ref().filter(|_| self.show_map_in_viewport)
    }
}

/// A node's material resolved for drawing: one surface, or one per face
/// material ID (0-based, as faces store it) for a Multi/Sub-Object material.
#[derive(Debug, Clone, PartialEq)]
pub enum Surfaces {
    Single(Surface),
    /// Face IDs without an entry (or with the sub-material off) draw black.
    PerId(BTreeMap<u16, Surface>),
}

impl Surfaces {
    pub fn for_face(&self, material_id: u16) -> std::borrow::Cow<'_, Surface> {
        match self {
            Surfaces::Single(s) => std::borrow::Cow::Borrowed(s),
            Surfaces::PerId(m) => m.get(&material_id).map_or_else(|| std::borrow::Cow::Owned(Surface::black()), std::borrow::Cow::Borrowed),
        }
    }
}

/// Resolves material `id` for drawing. A nested Multi/Sub uses the same face
/// ID; a Blend mixes its two materials by Mix Amount, its maps through a Mix
/// map with the Blend's mask.
pub fn surfaces(lib: &MaterialLibrary, id: MaterialId) -> Option<Surfaces> {
    fn single(lib: &MaterialLibrary, id: Option<MaterialId>, face: Option<u16>, depth: u32) -> Surface {
        let Some(m) = id.and_then(|i| lib.get(&i)).filter(|_| depth < 16) else { return Surface::black() };
        match &m.kind {
            MaterialKind::Standard => Surface::from_standard(m),
            MaterialKind::MultiSub(list) => match list.iter().find(|s| Some(s.id) == face.map(|f| f + 1)) {
                Some(s) if s.enabled => single(lib, s.material, face, depth + 1),
                _ => Surface::black(),
            },
            MaterialKind::Blend(b) => {
                let a = if b.enabled[0] { single(lib, b.materials[0], face, depth + 1) } else { Surface::black() };
                let c = if b.enabled[1] { single(lib, b.materials[1], face, depth + 1) } else { Surface::black() };
                blend(&a, &c, b)
            }
        }
    }
    let m = lib.get(&id)?;
    Some(match &m.kind {
        MaterialKind::MultiSub(list) => Surfaces::PerId(list.iter().map(|s| (s.id.saturating_sub(1), single(lib, Some(id), Some(s.id.saturating_sub(1)), 0))).collect()),
        _ => Surfaces::Single(single(lib, Some(id), None, 0)),
    })
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
