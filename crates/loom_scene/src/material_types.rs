//! The material types besides Standard, Multi/Sub-Object, Blend and Shell:
//! Raytrace, Architectural, Double Sided, Top/Bottom, Composite, Shellac,
//! Matte/Shadow and Ink 'n Paint, with their parameters (set by their
//! rollout labels) and how they resolve to what the viewports and the
//! renderer draw.

use crate::material::{MaterialId, Surface};
use glam::Vec3;
use serde::{Deserialize, Serialize};

fn color(rgb: [u8; 3]) -> Vec3 {
    Vec3::new(rgb[0] as f32, rgb[1] as f32, rgb[2] as f32) / 255.0
}

/// Raytrace Basic and Extended Parameters. The shader, Diffuse, Specular
/// Level, Glossiness, Soften, 2-Sided, Wire, Faceted and the maps are the
/// material's own Standard values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Raytrace {
    /// Ambient absorption (white = Ambient locked to Diffuse).
    pub ambient: Vec3,
    /// Reflect: the colour reflections are filtered through; its value is the amount.
    pub reflect: Vec3,
    pub fresnel: bool,
    /// Luminosity: a glow independent of the diffuse colour.
    pub luminosity: Vec3,
    /// Transparency: black opaque, white clear.
    pub transparency: Vec3,
    /// Index of Refr. 0.001–10.
    pub ior: f32,
    /// Extended: Extra Lighting, Translucency, Fluorescence and its Bias.
    pub extra_lighting: Vec3,
    pub translucency: Vec3,
    pub fluorescence: Vec3,
    pub fluorescence_bias: f32,
}

impl Default for Raytrace {
    fn default() -> Self {
        Self { ambient: Vec3::ZERO, reflect: Vec3::ZERO, fresnel: false, luminosity: Vec3::ZERO, transparency: Vec3::ZERO, ior: 1.55, extra_lighting: Vec3::ZERO, translucency: Vec3::ZERO, fluorescence: Vec3::ZERO, fluorescence_bias: 0.5 }
    }
}

impl Raytrace {
    /// Ok(false): not a Raytrace value (the Standard ones apply).
    pub fn set_parameter(&mut self, name: &str, value: f64) -> Result<bool, String> {
        match name {
            "Index of Refr." => self.ior = (value as f32).clamp(0.001, 10.0),
            "Fresnel" => self.fresnel = value != 0.0,
            "Fluorescence Bias" => self.fluorescence_bias = (value as f32).clamp(0.0, 1.0),
            _ => return Ok(false),
        }
        Ok(true)
    }

    pub fn set_color(&mut self, which: &str, rgb: [u8; 3]) -> bool {
        let c = color(rgb);
        match which {
            "Ambient" => self.ambient = c,
            "Reflect" => self.reflect = c,
            "Luminosity" => self.luminosity = c,
            "Transparency" => self.transparency = c,
            "Extra Lighting" => self.extra_lighting = c,
            "Translucency" => self.translucency = c,
            "Fluorescence" => self.fluorescence = c,
            _ => return false,
        }
        true
    }

    /// The Raytrace values on a surface resolved from the Standard ones.
    pub fn apply(&self, s: &mut Surface) {
        let avg = |c: Vec3| (c.x + c.y + c.z) / 3.0;
        s.ambient = self.ambient;
        s.reflect = self.reflect;
        s.fresnel = self.fresnel;
        s.self_illum = 0.0;
        s.self_illum_color = self.luminosity + self.extra_lighting * s.diffuse;
        // Transparency both shows what is behind and refracts it (the renderer).
        let t = avg(self.transparency);
        s.transmission = t;
        s.extended.ior = self.ior;
        s.extended.filter_color = self.transparency;
        s.translucency = avg(self.translucency);
    }
}

/// An Architectural template: (name, diffuse, Shininess, Transparency, Translucency, IOR, metal).
pub type ArchTemplate = (&'static str, [u8; 3], f32, f32, f32, f32, bool);

/// Architectural material templates.
pub const ARCH_TEMPLATES: [ArchTemplate; 24] = [
    ("User Defined", [178, 178, 178], 0.0, 0.0, 0.0, 1.5, false),
    ("Ceramic Tile, Glazed", [230, 230, 220], 80.0, 0.0, 0.0, 2.0, false),
    ("Fabric", [160, 130, 110], 0.0, 0.0, 0.0, 1.0, false),
    ("Glass - Clear", [255, 255, 255], 100.0, 95.0, 0.0, 1.52, false),
    ("Glass - Translucent", [235, 240, 245], 50.0, 30.0, 60.0, 1.52, false),
    ("Ideal Diffuse", [204, 204, 204], 0.0, 0.0, 0.0, 1.0, false),
    ("Masonry", [150, 120, 100], 0.0, 0.0, 0.0, 1.0, false),
    ("Metal", [190, 190, 195], 60.0, 0.0, 0.0, 2.5, true),
    ("Metal - Brushed", [180, 180, 185], 40.0, 0.0, 0.0, 2.5, true),
    ("Metal - Flat", [170, 170, 170], 20.0, 0.0, 0.0, 2.5, true),
    ("Metal - Polished", [210, 210, 215], 90.0, 0.0, 0.0, 2.5, true),
    ("Mirror", [255, 255, 255], 100.0, 0.0, 0.0, 2.5, true),
    ("Paint Flat", [200, 200, 200], 0.0, 0.0, 0.0, 1.0, false),
    ("Paint Gloss", [200, 200, 200], 90.0, 0.0, 0.0, 1.5, false),
    ("Paint Semi-gloss", [200, 200, 200], 50.0, 0.0, 0.0, 1.5, false),
    ("Paper", [235, 235, 230], 0.0, 0.0, 0.0, 1.0, false),
    ("Paper Translucent", [235, 235, 230], 0.0, 0.0, 50.0, 1.0, false),
    ("Plastic", [200, 60, 60], 60.0, 0.0, 0.0, 1.5, false),
    ("Stone Polished", [140, 140, 135], 80.0, 0.0, 0.0, 1.6, false),
    ("Stone Rough", [140, 135, 125], 0.0, 0.0, 0.0, 1.0, false),
    ("Water", [200, 220, 230], 100.0, 90.0, 0.0, 1.33, false),
    ("Wood Unfinished", [170, 120, 80], 0.0, 0.0, 0.0, 1.0, false),
    ("Wood Varnished", [150, 95, 55], 80.0, 0.0, 0.0, 1.5, false),
    ("Leather", [110, 70, 45], 30.0, 0.0, 0.0, 1.4, false),
];

/// Architectural material: Templates and Physical Qualities. Diffuse Color,
/// 2-Sided and the maps are the material's own Standard values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Architectural {
    /// Index into `ARCH_TEMPLATES`.
    pub template: u8,
    /// 0–100 each.
    pub shininess: f32,
    pub transparency: f32,
    pub translucency: f32,
    /// 1–2.5.
    pub ior: f32,
    /// Luminance cd/m².
    pub luminance: f32,
    pub raw_diffuse: bool,
}

impl Default for Architectural {
    fn default() -> Self {
        Self { template: 0, shininess: 0.0, transparency: 0.0, translucency: 0.0, ior: 1.5, luminance: 0.0, raw_diffuse: false }
    }
}

impl Architectural {
    pub fn set_parameter(&mut self, name: &str, value: f64) -> Result<bool, String> {
        let v = value as f32;
        match name {
            "Shininess" => self.shininess = v.clamp(0.0, 100.0),
            "Transparency" => self.transparency = v.clamp(0.0, 100.0),
            "Translucency" => self.translucency = v.clamp(0.0, 100.0),
            "Index of Refraction" => self.ior = v.clamp(1.0, 2.5),
            "Luminance cd/m2" => self.luminance = v.clamp(0.0, 1.0e7),
            "Raw Diffuse Texture" => self.raw_diffuse = value != 0.0,
            _ => return Ok(false),
        }
        Ok(true)
    }

    pub fn is_metal(&self) -> bool {
        ARCH_TEMPLATES.get(self.template as usize).is_some_and(|t| t.6)
    }

    pub fn apply(&self, s: &mut Surface) {
        s.glossiness = self.shininess;
        // Shininess and IOR give the highlight: an IOR of 1 shows none.
        s.specular_level = self.shininess * ((self.ior - 1.0) / 0.5).clamp(0.0, 1.0);
        s.specular = Vec3::ONE * (s.specular_level / 100.0);
        s.transmission = self.transparency / 100.0;
        s.translucency = self.translucency / 100.0;
        s.extended.ior = self.ior;
        // 1000 cd/m² glows fully.
        s.self_illum = 0.0;
        s.self_illum_color = s.diffuse * (self.luminance / 1000.0).clamp(0.0, 1.0);
        if self.is_metal() {
            s.metallic = 1.0;
        }
        s.unlit = self.raw_diffuse;
    }
}

/// Double Sided Basic Parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DoubleSided {
    /// Facing Material, Back Material.
    pub materials: [Option<MaterialId>; 2],
    pub enabled: [bool; 2],
    /// 0–100: how much of each side shows on the other.
    pub translucency: f32,
}

/// Top/Bottom Basic Parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopBottom {
    /// Top Material, Bottom Material.
    pub materials: [Option<MaterialId>; 2],
    pub enabled: [bool; 2],
    /// Coordinates: Local (else World).
    pub local: bool,
    /// 0–100 each; Position 0 shows only the top material, 100 only the bottom.
    pub blend: f32,
    pub position: f32,
}

/// One of Composite's Mat 1–9.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompositeLayer {
    pub material: Option<MaterialId>,
    pub enabled: bool,
    /// 0 A (additive), 1 S (subtractive), 2 M (mix).
    pub mode: u8,
    /// 0–200 (Mix 0–100).
    pub amount: f32,
}

/// Composite Basic Parameters: the Base Material and Mat 1–9 over it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Composite {
    pub base: Option<MaterialId>,
    pub layers: Vec<CompositeLayer>,
}

/// Shellac Basic Parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shellac {
    /// Base Material, Shellac Material.
    pub materials: [Option<MaterialId>; 2],
    /// Shellac Color Blend, 0 and up.
    pub blend: f32,
}

/// Matte/Shadow Basic Parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MatteShadow {
    pub opaque_alpha: bool,
    pub apply_atmosphere: bool,
    pub receive_shadows: bool,
    pub affect_alpha: bool,
    /// 0–1: 0 black shadows, 1 none.
    pub shadow_brightness: f32,
    pub color: Vec3,
    /// Reflection Amount 0–100 (with a Reflection map).
    pub reflection_amount: f32,
}

impl Default for MatteShadow {
    fn default() -> Self {
        Self { opaque_alpha: false, apply_atmosphere: false, receive_shadows: true, affect_alpha: true, shadow_brightness: 0.0, color: Vec3::ZERO, reflection_amount: 50.0 }
    }
}

/// Ink 'n Paint: Paint Controls and Ink Controls.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InkPaint {
    pub lighted_on: bool,
    pub lighted: Vec3,
    /// Shaded: the percent of Lighted on the unlit side, or its own colour when off.
    pub shaded_on: bool,
    pub shaded_percent: f32,
    pub shaded: Vec3,
    pub paint_levels: u32,
    pub highlight_on: bool,
    pub highlight: Vec3,
    pub glossiness: f32,
    pub ink: bool,
    pub ink_quality: u32,
    pub ink_width: f32,
    pub variable_width: bool,
    pub ink_min: f32,
    pub ink_max: f32,
    pub ink_color: Vec3,
    pub outline: bool,
    pub overlap: bool,
    pub underlap: bool,
    pub smgroup: bool,
    pub mat_id: bool,
}

impl Default for InkPaint {
    fn default() -> Self {
        Self {
            lighted_on: true,
            lighted: Vec3::new(0.27, 0.53, 0.82),
            shaded_on: true,
            shaded_percent: 70.0,
            shaded: Vec3::new(0.19, 0.37, 0.57),
            paint_levels: 2,
            highlight_on: false,
            highlight: Vec3::ONE,
            glossiness: 50.0,
            ink: true,
            ink_quality: 1,
            ink_width: 2.0,
            variable_width: false,
            ink_min: 2.0,
            ink_max: 4.0,
            ink_color: Vec3::ZERO,
            outline: true,
            overlap: true,
            underlap: false,
            smgroup: true,
            mat_id: true,
        }
    }
}

impl InkPaint {
    pub fn set_parameter(&mut self, name: &str, value: f64) -> Result<(), String> {
        let v = value as f32;
        let on = value != 0.0;
        match name {
            "Lighted On" => self.lighted_on = on,
            "Shaded On" => self.shaded_on = on,
            "Shaded Percent" => self.shaded_percent = v.clamp(0.0, 100.0),
            "Paint Levels" => self.paint_levels = (value.round() as u32).clamp(1, 255),
            "Highlight On" => self.highlight_on = on,
            "Glossiness" => self.glossiness = v.clamp(0.0, 100.0),
            "Ink" => self.ink = on,
            "Ink Quality" => self.ink_quality = (value.round() as u32).clamp(1, 3),
            "Ink Width" => self.ink_width = v.clamp(0.0, 1000.0),
            "Variable Width" => self.variable_width = on,
            "Ink Min" => self.ink_min = v.clamp(0.0, 1000.0),
            "Ink Max" => self.ink_max = v.clamp(0.0, 1000.0),
            "Outline" => self.outline = on,
            "Overlap" => self.overlap = on,
            "Underlap" => self.underlap = on,
            "SmGroup" => self.smgroup = on,
            "Mat ID" => self.mat_id = on,
            other => return Err(format!("no Ink 'n Paint parameter \"{other}\"")),
        }
        Ok(())
    }

    pub fn set_color(&mut self, which: &str, rgb: [u8; 3]) -> bool {
        let c = color(rgb);
        match which {
            "Lighted" => self.lighted = c,
            "Shaded" => self.shaded = c,
            "Highlight" => self.highlight = c,
            "Ink Color" => self.ink_color = c,
            _ => return false,
        }
        true
    }

    /// The paint colour at diffuse lighting `light` 0–1: Paint Levels flat bands
    /// from the Shaded colour to the Lighted one.
    pub fn paint(&self, light: f32) -> Vec3 {
        let shaded = if self.shaded_on { self.lighted * (self.shaded_percent / 100.0) } else { self.shaded };
        let n = self.paint_levels.max(1);
        let band = if n == 1 { 1.0 } else { ((light.clamp(0.0, 1.0) * n as f32).floor().min(n as f32 - 1.0)) / (n as f32 - 1.0) };
        if self.lighted_on {
            shaded.lerp(self.lighted, band)
        } else {
            Vec3::ZERO
        }
    }
}

/// Composite's colour arithmetic: each enabled layer over the result so far.
pub fn composite(base: Surface, layers: &[(Surface, u8, f32)]) -> Surface {
    let mut out = base;
    for (s, mode, amount) in layers {
        let a = (amount / 100.0).max(0.0);
        // A layer's opacity (overloaded past 100 for A and S) scales what it adds.
        let k = (s.opacity * a).min(1.0);
        match mode {
            0 => out.diffuse += s.diffuse * k,
            1 => out.diffuse = (out.diffuse - s.diffuse * k).max(Vec3::ZERO),
            _ => {
                let t = a.min(1.0);
                out.diffuse = out.diffuse.lerp(s.diffuse, 1.0 - t);
                out.opacity += (s.opacity - out.opacity) * (1.0 - t);
            }
        }
        out.ambient = out.diffuse;
        out.specular = (out.specular + s.specular * k).min(Vec3::splat(2.0));
        out.self_illum_color += s.self_illum_color * k;
    }
    out.diffuse = out.diffuse.min(Vec3::ONE);
    out
}

/// Shellac: the shellac material's colour added to the base by Shellac
/// Color Blend; its highlight becomes a clear coat over the base.
pub fn shellac(base: Surface, coat: &Surface, blend: f32) -> Surface {
    let k = blend / 100.0;
    let mut out = base;
    out.diffuse = (out.diffuse + coat.diffuse * k).min(Vec3::ONE);
    out.ambient = out.diffuse;
    out.specular = (out.specular + coat.specular * k).min(Vec3::splat(2.0));
    out.coat = (coat.specular_level / 100.0 * k).clamp(0.0, 1.0);
    out.coat_glossiness = coat.glossiness;
    out
}
