//! Materials: the Standard material with the Blinn shader, the
//! scene's material library and the Material Editor's 24 sample slots.

use crate::param::Param;
use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MaterialId(pub u32);

/// A Bitmap map in one of the Maps rollout's channels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BitmapMap {
    pub path: String,
    /// The Maps rollout Amount, 0–100 (Bump: -999–999).
    pub amount: f32,
    pub enabled: bool,
}

/// Standard material, Blinn Basic Parameters plus the maps 3dLoom shows.
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
    pub diffuse_map: Option<BitmapMap>,
    pub bump_map: Option<BitmapMap>,
    pub opacity_map: Option<BitmapMap>,
    pub specular_level_map: Option<BitmapMap>,
    /// Show Standard Map in Viewport (the diffuse map in shaded views).
    pub show_map_in_viewport: bool,
}

/// The Maps rollout channels 3dLoom stores, with their default Amount.
pub const MAP_CHANNELS: [(&str, f32); 4] = [("Diffuse Color", 100.0), ("Specular Level", 100.0), ("Opacity", 100.0), ("Bump", 30.0)];

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
            show_map_in_viewport: false,
        }
    }

    /// The default material of Material Editor slot `slot` (0-based).
    pub fn slot_default(slot: usize) -> Self {
        Self::new(format!("{:02} - Default", slot + 1))
    }

    pub fn map(&self, channel: &str) -> Option<&BitmapMap> {
        self.map_slot(channel).ok().and_then(|m| m.as_ref())
    }

    /// The Option holding `channel`'s map, by its Maps rollout name.
    pub fn map_slot_mut(&mut self, channel: &str) -> Result<&mut Option<BitmapMap>, String> {
        Ok(match channel {
            "Diffuse Color" => &mut self.diffuse_map,
            "Specular Level" => &mut self.specular_level_map,
            "Opacity" => &mut self.opacity_map,
            "Bump" => &mut self.bump_map,
            other => return Err(format!("no map channel \"{other}\" (Diffuse Color, Specular Level, Opacity, Bump)")),
        })
    }

    fn map_slot(&self, channel: &str) -> Result<&Option<BitmapMap>, String> {
        Ok(match channel {
            "Diffuse Color" => &self.diffuse_map,
            "Specular Level" => &self.specular_level_map,
            "Opacity" => &self.opacity_map,
            "Bump" => &self.bump_map,
            other => return Err(format!("no map channel \"{other}\"")),
        })
    }

    /// Sets a Blinn Basic Parameters / Shader Basic Parameters value by its
    /// label, clamped to its spinner range (booleans as 0 / 1).
    pub fn set_parameter(&mut self, name: &str, value: f64) -> Result<(), String> {
        let v = value as f32;
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

/// The Material Editor's 24 sample slots and the active one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialEditorSlots {
    /// None = the slot still shows its untouched default material.
    pub slots: Vec<Option<MaterialId>>,
    pub active: usize,
}

pub const SLOT_COUNT: usize = 24;

impl Default for MaterialEditorSlots {
    fn default() -> Self {
        Self { slots: vec![None; SLOT_COUNT], active: 0 }
    }
}

/// The scene's material table.
pub type MaterialLibrary = BTreeMap<MaterialId, StandardMaterial>;

/// The id a new material gets in `lib`.
pub fn next_material_id(lib: &MaterialLibrary) -> MaterialId {
    MaterialId(lib.keys().next_back().map_or(1, |k| k.0 + 1))
}
