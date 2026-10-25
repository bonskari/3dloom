//! The maps' parameters beyond the basic 2D ones: the 3D Coordinates
//! rollout, the Output rollout, the procedural maps' settings, Composite
//! layers and Gradient Ramp flags, and the colour components past the
//! first three (Planet's eight colours, RGB Tint's swatches and map, a
//! Composite's layer and mask maps, a Gradient Ramp's flag maps).
//! `Map::set_parameter` hands every name it does not know to
//! `Map::set_more_parameter` here.

use crate::material::{Map, MapType, MAP_SLOTS};
use glam::Vec3;
use serde::{Deserialize, Serialize};

/// Coordinates rollout of a 3D map: Source, Offset / Tiling / Angle X Y Z,
/// Blur Offset (Blur and Map Channel are the 2D rollout's fields).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Coords3 {
    /// 0 Object XYZ, 1 World XYZ, 2 Explicit Map Channel, 3 Vertex Color Channel.
    pub source: u8,
    pub offset: [f32; 3],
    pub tiling: [f32; 3],
    /// Degrees about X, Y, Z.
    pub angle: [f32; 3],
    pub blur_offset: f32,
}

impl Default for Coords3 {
    fn default() -> Self {
        Self { source: 0, offset: [0.0; 3], tiling: [1.0; 3], angle: [0.0; 3], blur_offset: 0.0 }
    }
}

impl Coords3 {
    pub const SOURCES: [&'static str; 4] = ["Object XYZ", "World XYZ", "Explicit Map Channel", "Vertex Color Channel"];

    /// The pattern-space point of `p`: offset, rotated by Angle X Y Z, tiled.
    pub fn apply(&self, p: Vec3) -> Vec3 {
        let q = p - Vec3::from(self.offset);
        let [ax, ay, az] = self.angle.map(|a| -a.to_radians());
        let r = glam::Mat3::from_rotation_z(az) * glam::Mat3::from_rotation_y(ay) * glam::Mat3::from_rotation_x(ax);
        (r * q) * Vec3::from(self.tiling)
    }
}

/// Output rollout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MapOutput {
    pub invert: bool,
    pub clamp: bool,
    pub alpha_from_rgb: bool,
    pub amount: f32,
    pub rgb_offset: f32,
    pub rgb_level: f32,
    /// Only for the map's use as a bump map.
    pub bump_amount: f32,
}

impl Default for MapOutput {
    fn default() -> Self {
        Self { invert: false, clamp: false, alpha_from_rgb: false, amount: 1.0, rgb_offset: 0.0, rgb_level: 1.0, bump_amount: 1.0 }
    }
}

impl MapOutput {
    /// Invert, RGB Level and Offset, Output Amount, Clamp, Alpha from RGB Intensity.
    pub fn apply(&self, c: [f32; 4]) -> [f32; 4] {
        let mut rgb = [c[0], c[1], c[2]];
        for v in &mut rgb {
            if self.invert {
                *v = 1.0 - *v;
            }
            *v = (*v * self.rgb_level + self.rgb_offset) * self.amount;
            if self.clamp {
                *v = v.clamp(0.0, 1.0);
            }
        }
        let a = if self.alpha_from_rgb { (rgb[0] + rgb[1] + rgb[2]) / 3.0 } else { c[3] * self.amount.min(1.0) };
        [rgb[0], rgb[1], rgb[2], a.clamp(0.0, 1.0)]
    }
}

/// One layer of a Composite map: its map is component `2 i`, its mask `2 i + 1`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompositeLayer {
    /// Hide this layer (off) / Hide the mask of this layer (off).
    pub visible: bool,
    pub mask_visible: bool,
    /// 0–100.
    pub opacity: f32,
    /// Index into `BLEND_MODES`.
    pub blend: u8,
}

impl Default for CompositeLayer {
    fn default() -> Self {
        Self { visible: true, mask_visible: true, opacity: 100.0, blend: 0 }
    }
}

pub const BLEND_MODES: [&str; 25] = [
    "Normal", "Average", "Addition", "Subtract", "Darken", "Multiply", "Color Burn", "Linear Burn", "Lighten", "Screen", "Color Dodge", "Linear Dodge", "Spotlight", "Spotlight Blend", "Overlay", "Soft Light", "Hard Light", "Pin Light", "Hard Mix", "Difference", "Exclusion", "Hue", "Saturation", "Color", "Value",
];

/// One flag of a Gradient Ramp: its colour and map are component `1 + i`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RampFlag {
    /// 0–100 along the gradient.
    pub position: f32,
    /// Flag Properties interpolation (Custom interpolation): index into `INTERPOLATIONS`.
    pub interpolation: u8,
}

impl Default for RampFlag {
    fn default() -> Self {
        Self { position: 0.0, interpolation: 4 }
    }
}

pub const RAMP_TYPES: [&str; 12] = ["4 Corner", "Box", "Diagonal", "Lighting", "Linear", "Mapped", "Normal", "Pong", "Radial", "Spiral", "Sweep", "Tartan"];
pub const INTERPOLATIONS: [&str; 6] = ["Custom", "Ease In", "Ease In Out", "Ease Out", "Linear", "Solid"];
pub const TILE_PRESETS: [&str; 8] = ["Custom Tiles", "Running Bond", "Common Flemish Bond", "English Bond", "1/2 Running Bond", "Stack Bond", "Fine Running Bond", "Fine Stack Bond"];
pub const FALLOFF_TYPES: [&str; 5] = ["Towards / Away", "Perpendicular / Parallel", "Fresnel", "Shadow / Light", "Distance Blend"];
pub const FALLOFF_DIRECTIONS: [&str; 10] = ["Viewing Direction (Camera Z-Axis)", "Camera X-Axis", "Camera Y-Axis", "Object", "Local X-Axis", "Local Y-Axis", "Local Z-Axis", "World X-Axis", "World Y-Axis", "World Z-Axis"];
pub const CHANNEL_OPS: [&str; 4] = ["Normal", "Monochrome", "Invert", "Custom"];
pub const CHANNEL_SOURCES: [&str; 11] = ["Red", "Green", "Blue", "Alpha", "Red (Inverse)", "Green (Inverse)", "Blue (Inverse)", "Alpha (Inverse)", "Monochrome", "One", "Zero"];
pub const RGB_ALPHA_FROM: [&str; 3] = ["Map #1", "Map #2", "Multiply Alphas"];
pub const SUB_CHANNELS: [&str; 4] = ["RGB", "Red", "Green", "Blue"];

/// The procedural maps' settings (Size, Levels, Phase, High / Low are `MapParams`').
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MoreParams {
    pub seed: u32,
    // Swirl.
    pub color_contrast: f32,
    pub swirl_intensity: f32,
    pub swirl_amount: f32,
    pub twist: f32,
    pub constant_detail: f32,
    pub center: [f32; 2],
    pub center_lock: bool,
    // Tiles.
    pub preset: u8,
    pub horiz_count: f32,
    pub vert_count: f32,
    pub color_variance: f32,
    pub fade_variance: f32,
    pub gap: [f32; 2],
    pub gap_lock: bool,
    pub holes: f32,
    pub rough: f32,
    pub line_shift: f32,
    pub random_shift: f32,
    pub row_modify: bool,
    pub per_row: u32,
    pub row_change: f32,
    pub column_modify: bool,
    pub per_column: u32,
    pub column_change: f32,
    // Gradient Ramp.
    pub ramp_type: u8,
    pub interpolation: u8,
    pub noise_amount: f32,
    pub smooth: f32,
    // Cellular.
    pub variation: f32,
    pub chips: bool,
    pub spread: f32,
    pub bump_smoothing: f32,
    pub fractal: bool,
    pub adaptive: bool,
    pub roughness: f32,
    pub mid: f32,
    // Dent, Smoke, Splat, Cellular.
    pub iterations: f32,
    pub strength: f32,
    // Marble.
    pub vein_width: f32,
    // Perlin Marble: Color 1 / Color 2 Saturation.
    pub saturation: [f32; 2],
    // Planet.
    pub continent_size: f32,
    pub island_factor: f32,
    pub ocean: f32,
    pub blend_water_land: bool,
    // Smoke.
    pub exponent: f32,
    // Splat, Stucco.
    pub threshold: f32,
    pub thickness: f32,
    // Waves.
    pub wave_sets: u32,
    pub wave_radius: f32,
    pub wave_len_max: f32,
    pub wave_len_min: f32,
    pub amplitude: f32,
    pub distribution_2d: bool,
    // Wood.
    pub grain_thickness: f32,
    pub radial_noise: f32,
    pub axial_noise: f32,
    // Mask.
    pub invert_mask: bool,
    // RGB Multiply.
    pub alpha_from: u8,
    // Falloff.
    pub falloff_type: u8,
    pub falloff_direction: u8,
    pub front_amount: f32,
    pub side_amount: f32,
    pub override_ior: bool,
    pub ior: f32,
    pub near_distance: f32,
    pub far_distance: f32,
    pub extrapolate: bool,
    // Mix: Mixing Curve.
    pub use_curve: bool,
    pub upper: f32,
    pub lower: f32,
    // Color Correction.
    pub channels: u8,
    pub custom_channels: [u8; 4],
    pub hue_shift: f32,
    pub cc_saturation: f32,
    pub tint_strength: f32,
    pub brightness: f32,
    pub contrast: f32,
    // Vertex Color.
    pub sub_channel: u8,
}

impl Default for MoreParams {
    fn default() -> Self {
        Self {
            seed: 12345,
            color_contrast: 0.4,
            swirl_intensity: 2.0,
            swirl_amount: 1.0,
            twist: 1.0,
            constant_detail: 4.0,
            center: [0.5, 0.5],
            center_lock: true,
            preset: 5,
            horiz_count: 4.0,
            vert_count: 4.0,
            color_variance: 0.0,
            fade_variance: 0.05,
            gap: [0.5, 0.5],
            gap_lock: true,
            holes: 0.0,
            rough: 0.0,
            line_shift: 0.5,
            random_shift: 0.0,
            row_modify: false,
            per_row: 2,
            row_change: 1.0,
            column_modify: false,
            per_column: 2,
            column_change: 1.0,
            ramp_type: 4,
            interpolation: 4,
            noise_amount: 0.0,
            smooth: 0.0,
            variation: 0.0,
            chips: false,
            spread: 0.5,
            bump_smoothing: 0.1,
            fractal: false,
            adaptive: true,
            roughness: 0.0,
            mid: 0.5,
            iterations: 3.0,
            strength: 20.0,
            vein_width: 0.025,
            saturation: [85.0, 70.0],
            continent_size: 40.0,
            island_factor: 0.5,
            ocean: 60.0,
            blend_water_land: true,
            exponent: 1.5,
            threshold: 0.2,
            thickness: 0.15,
            wave_sets: 10,
            wave_radius: 800.0,
            wave_len_max: 50.0,
            wave_len_min: 5.0,
            amplitude: 1.0,
            distribution_2d: false,
            grain_thickness: 7.0,
            radial_noise: 1.0,
            axial_noise: 1.0,
            invert_mask: false,
            alpha_from: 2,
            falloff_type: 1,
            falloff_direction: 0,
            front_amount: 100.0,
            side_amount: 100.0,
            override_ior: false,
            ior: 1.6,
            near_distance: 0.0,
            far_distance: 100.0,
            extrapolate: false,
            use_curve: false,
            upper: 0.7,
            lower: 0.3,
            channels: 0,
            custom_channels: [0, 1, 2, 3],
            hue_shift: 0.0,
            cc_saturation: 0.0,
            tint_strength: 0.0,
            brightness: 0.0,
            contrast: 0.0,
            sub_channel: 0,
        }
    }
}

impl MapType {
    /// Evaluated at a 3D point (the 3D Coordinates rollout) rather than a UV.
    pub fn is_3d(self) -> bool {
        matches!(self, MapType::Cellular | MapType::Dent | MapType::Marble | MapType::Noise | MapType::PerlinMarble | MapType::Planet | MapType::Smoke | MapType::Speckle | MapType::Splat | MapType::Stucco | MapType::Waves | MapType::Wood)
    }

    /// Has the 2D Coordinates rollout.
    pub fn has_2d_coords(self) -> bool {
        matches!(self, MapType::Bitmap | MapType::Checker | MapType::Gradient | MapType::GradientRamp | MapType::Swirl | MapType::Tiles | MapType::NormalBump)
    }

    /// Has the Output rollout.
    pub fn has_output(self) -> bool {
        matches!(self, MapType::Bitmap | MapType::Cellular | MapType::Falloff | MapType::Gradient | MapType::GradientRamp | MapType::Mix | MapType::Noise | MapType::Output)
    }
}

/// Map button names outside `MAP_SLOTS` that dialogs pass as an index.
const MORE_SLOTS: [&str; 22] = ["Base", "Swirl", "Tiles", "Grout", "Source Map", "Cell Color", "Division #1", "Division #2", "Color #4", "Color #5", "Color #6", "Color #7", "Color #8", "Map", "R", "G", "B", "Hue Tint", "Front", "Side", "Normal", "Mix Amount"];

/// A map slot name as a small index (for dialogs that must be `Copy`):
/// `MAP_SLOTS`, the other fixed component names, then Layer N (100 + N),
/// Mask N (150 + N) and Flag #N (200 + N).
pub fn slot_index(name: &str) -> u8 {
    if let Some(i) = MAP_SLOTS.iter().position(|s| *s == name) {
        return i as u8;
    }
    if let Some(i) = MORE_SLOTS.iter().position(|s| *s == name) {
        return (MAP_SLOTS.len() + i) as u8;
    }
    let num = |prefix: &str, max: u32| name.strip_prefix(prefix).and_then(|n| n.parse::<u32>().ok()).filter(|n| (1..=max).contains(n));
    if let Some(n) = num("Layer ", 50) {
        return (100 + n) as u8;
    }
    if let Some(n) = num("Mask ", 50) {
        return (150 + n) as u8;
    }
    if let Some(n) = num("Flag #", 55) {
        return (200 + n) as u8;
    }
    0
}

/// The name of a `slot_index`.
pub fn slot_name(i: u8) -> String {
    let i = i as usize;
    let fixed = MAP_SLOTS.len() + MORE_SLOTS.len();
    match i {
        _ if i < MAP_SLOTS.len() => MAP_SLOTS[i].to_owned(),
        _ if i < fixed => MORE_SLOTS[i - MAP_SLOTS.len()].to_owned(),
        101..=150 => format!("Layer {}", i - 100),
        151..=200 => format!("Mask {}", i - 150),
        201..=255 => format!("Flag #{}", i - 200),
        _ => String::new(),
    }
}

fn rgb(r: u8, g: u8, b: u8) -> Vec3 {
    Vec3::new(r as f32, g as f32, b as f32) / 255.0
}

impl Map {
    /// The new map's type-specific defaults (`Map::new` calls this).
    pub(crate) fn init_more(&mut self) {
        let p = &mut self.params;
        let e = &mut self.extra;
        match self.kind {
            MapType::Swirl => {
                self.colors[0] = rgb(25, 76, 127);
                self.colors[1] = rgb(229, 229, 229);
                e.seed = 0;
            }
            MapType::Tiles => {
                self.colors[0] = rgb(152, 72, 44);
                self.colors[1] = rgb(204, 204, 204);
                e.seed = 0;
            }
            MapType::GradientRamp => {
                // Flag #1 at 0, Flag #2 at 100, Flag #3 between: red, blue, green.
                self.flags = vec![RampFlag { position: 0.0, interpolation: 4 }, RampFlag { position: 100.0, interpolation: 4 }, RampFlag { position: 50.0, interpolation: 4 }];
                self.colors = [Vec3::ZERO, Vec3::X, Vec3::Z];
                self.extra_colors = vec![Vec3::Y];
                p.size = 1.0;
                p.levels = 4.0;
            }
            MapType::Cellular => {
                self.colors = [Vec3::ONE, Vec3::splat(0.5), Vec3::ONE];
                p.size = 5.0;
            }
            MapType::Dent => {
                p.size = 200.0;
                e.iterations = 2.0;
            }
            MapType::Marble => {
                self.colors[0] = rgb(229, 229, 229);
                self.colors[1] = rgb(115, 76, 51);
                p.size = 100.0;
            }
            MapType::PerlinMarble => {
                self.colors[0] = rgb(184, 140, 64);
                self.colors[1] = rgb(102, 61, 36);
                p.size = 50.0;
                p.levels = 8.0;
            }
            MapType::Planet => {
                self.colors = [rgb(0, 0, 76), rgb(0, 25, 127), rgb(25, 76, 178)];
                self.extra_colors = vec![rgb(25, 102, 25), rgb(76, 127, 38), rgb(127, 102, 51), rgb(153, 128, 102), rgb(242, 242, 242)];
            }
            MapType::Smoke => {
                p.size = 40.0;
                e.iterations = 5.0;
            }
            MapType::Speckle => p.size = 0.1,
            MapType::Splat => {
                p.size = 40.0;
                e.iterations = 4.0;
                e.threshold = 0.2;
            }
            MapType::Stucco => {
                p.size = 20.0;
                e.threshold = 0.57;
            }
            MapType::Waves => e.seed = 30159,
            MapType::Wood => {
                self.colors[0] = rgb(214, 163, 74);
                self.colors[1] = rgb(92, 51, 13);
            }
            MapType::Composite => self.layers = vec![CompositeLayer::default()],
            MapType::Mask => self.colors = [Vec3::ONE; 3],
            MapType::RgbMultiply => self.colors = [Vec3::ONE; 3],
            MapType::RgbTint => {
                self.colors = [Vec3::X, Vec3::Y, Vec3::Z];
                self.extra_colors = vec![Vec3::ONE];
            }
            MapType::ColorCorrection => self.colors = [Vec3::splat(0.5), Vec3::ONE, Vec3::ONE],
            MapType::Output => self.colors = [Vec3::ZERO; 3],
            MapType::VertexColor => self.coords.map_channel = 0,
            _ => {}
        }
    }

    /// The colour component names (with their maps) at this map: the
    /// type's fixed ones, a Composite's Layer / Mask pairs, a Gradient
    /// Ramp's Source Map and flags.
    pub fn slot_names(&self) -> Vec<String> {
        match self.kind {
            MapType::Composite => (1..=self.layers.len()).flat_map(|i| [format!("Layer {i}"), format!("Mask {i}")]).collect(),
            MapType::GradientRamp => std::iter::once("Source Map".to_owned()).chain((1..=self.flags.len()).map(|i| format!("Flag #{i}"))).collect(),
            k => k.color_slots().iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    /// Component `i` has a colour swatch.
    pub fn slot_has_color(&self, i: usize) -> bool {
        match self.kind {
            MapType::Mix => i < 2,
            MapType::Composite | MapType::Mask | MapType::Output | MapType::NormalBump => false,
            MapType::GradientRamp => i > 0,
            MapType::RgbTint => i < 3,
            _ => true,
        }
    }

    /// Component `i` takes a map.
    pub fn slot_has_map(&self, i: usize) -> bool {
        match self.kind {
            MapType::Planet => false,
            MapType::RgbTint => i == 3,
            MapType::ColorCorrection => i == 0,
            _ => true,
        }
    }

    /// The component an old map goes into when Replace Map keeps it.
    pub fn keep_old_slot(&self) -> Option<usize> {
        if self.kind == MapType::GradientRamp {
            return Some(1);
        }
        (0..self.slot_names().len()).find(|i| self.slot_has_map(*i))
    }

    /// Component `i`'s colour.
    pub fn color(&self, i: usize) -> Vec3 {
        if i < 3 { self.colors[i] } else { self.extra_colors.get(i - 3).copied().unwrap_or(Vec3::ONE) }
    }

    pub fn color_mut(&mut self, i: usize) -> &mut Vec3 {
        if i < 3 {
            return &mut self.colors[i];
        }
        if self.extra_colors.len() <= i - 3 {
            self.extra_colors.resize(i - 2, Vec3::ONE);
        }
        &mut self.extra_colors[i - 3]
    }

    /// Component `i`'s map.
    pub fn sub_map(&self, i: usize) -> Option<&Map> {
        if i < 3 { self.sub_maps[i].as_deref() } else { self.extra_maps.get(i - 3).and_then(|m| m.as_deref()) }
    }

    pub fn sub_map_slot_mut(&mut self, i: usize) -> &mut Option<Box<Map>> {
        if i < 3 {
            return &mut self.sub_maps[i];
        }
        if self.extra_maps.len() <= i - 3 {
            self.extra_maps.resize(i - 2, None);
        }
        &mut self.extra_maps[i - 3]
    }

    /// Every map in this map's components.
    pub fn components(&self) -> impl Iterator<Item = &Map> {
        self.sub_maps.iter().chain(self.extra_maps.iter()).filter_map(|m| m.as_deref())
    }

    fn take_components(&mut self) -> (Vec<Vec3>, Vec<Option<Box<Map>>>) {
        let n = self.slot_names().len().max(3 + self.extra_colors.len()).max(3 + self.extra_maps.len());
        let colors = (0..n).map(|i| self.color(i)).collect();
        let maps = (0..n).map(|i| self.sub_map_slot_mut(i).take()).collect();
        (colors, maps)
    }

    fn put_components(&mut self, colors: Vec<Vec3>, maps: Vec<Option<Box<Map>>>) {
        self.extra_colors.clear();
        self.extra_maps.clear();
        for (i, c) in colors.into_iter().enumerate() {
            *self.color_mut(i) = c;
        }
        for (i, m) in maps.into_iter().enumerate() {
            *self.sub_map_slot_mut(i) = m;
        }
        while self.extra_maps.last().is_some_and(|m| m.is_none()) {
            self.extra_maps.pop();
        }
    }

    /// Inserts `n` empty components at `at` (white, no map).
    fn insert_components(&mut self, at: usize, n: usize) {
        let (mut colors, mut maps) = self.take_components();
        for _ in 0..n {
            colors.insert(at.min(colors.len()), Vec3::ONE);
            maps.insert(at.min(maps.len()), None);
        }
        self.put_components(colors, maps);
    }

    fn remove_components(&mut self, at: usize, n: usize) {
        let (mut colors, mut maps) = self.take_components();
        for _ in 0..n {
            if at < colors.len() {
                colors.remove(at);
                maps.remove(at);
            }
        }
        self.put_components(colors, maps);
    }

    /// A Gradient Ramp's colour at `t` (0–100) between its flags, by Interpolation.
    pub fn ramp_color_at(&self, t: f32) -> Vec3 {
        self.ramp_sample(t, &|i| self.color(1 + i))
    }

    /// As `ramp_color_at` with flag colours from `flag`.
    pub fn ramp_sample(&self, t: f32, flag: &dyn Fn(usize) -> Vec3) -> Vec3 {
        let mut order: Vec<usize> = (0..self.flags.len()).collect();
        order.sort_by(|a, b| self.flags[*a].position.total_cmp(&self.flags[*b].position));
        let Some(&first) = order.first() else { return Vec3::ZERO };
        if t <= self.flags[first].position {
            return flag(first);
        }
        for w in order.windows(2) {
            let (a, b) = (w[0], w[1]);
            let (pa, pb) = (self.flags[a].position, self.flags[b].position);
            if t <= pb {
                let f = if pb - pa <= 1e-6 { 1.0 } else { (t - pa) / (pb - pa) };
                let interp = if self.extra.interpolation == 0 { self.flags[a].interpolation } else { self.extra.interpolation };
                let f = match interp {
                    1 => f * f,
                    2 => f * f * (3.0 - 2.0 * f),
                    3 => 1.0 - (1.0 - f) * (1.0 - f),
                    5 => 0.0,
                    _ => f,
                };
                return flag(a).lerp(flag(b), f);
            }
        }
        flag(*order.last().expect("non-empty"))
    }

    /// `set_parameter`'s names for the 3D Coordinates and Output rollouts,
    /// the procedural maps, Composite layers and Gradient Ramp flags.
    pub(crate) fn set_more_parameter(&mut self, name: &str, value: f64) -> Result<bool, String> {
        let v = value as f32;
        let on = value != 0.0;
        let idx = |max: usize| (value.round().max(0.0) as usize).min(max - 1) as u8;
        let c3 = &mut self.coords3;
        let o = &mut self.output;
        let e = &mut self.extra;
        let axis = |s: &str| ["X", "Y", "Z"].iter().position(|a| *a == s);
        if let Some((what, ax)) = name.rsplit_once(' ') {
            if let Some(i) = axis(ax) {
                match what {
                    "Offset" => c3.offset[i] = v,
                    "Tiling" => c3.tiling[i] = v,
                    "Angle" => c3.angle[i] = v,
                    _ => {}
                }
                if matches!(what, "Offset" | "Tiling" | "Angle") {
                    return Ok(true);
                }
            }
        }
        match name {
            "Source" => c3.source = idx(4),
            "Blur Offset" => c3.blur_offset = v.clamp(0.0, 1.0),
            "Invert" => o.invert = on,
            "Clamp" => o.clamp = on,
            "Alpha from RGB Intensity" => o.alpha_from_rgb = on,
            "Output Amount" => o.amount = v.clamp(0.0, 100.0),
            "RGB Offset" => o.rgb_offset = v.clamp(-100.0, 100.0),
            "RGB Level" => o.rgb_level = v.clamp(0.0, 100.0),
            "Bump Amount" => o.bump_amount = v.clamp(-1000.0, 1000.0),
            "Random Seed" => e.seed = value.round().clamp(0.0, 65535.0) as u32,
            "Color Contrast" => e.color_contrast = v.clamp(0.0, 4.0),
            "Swirl Intensity" => e.swirl_intensity = v.clamp(-10.0, 10.0),
            "Swirl Amount" => e.swirl_amount = v.clamp(0.0, 3.0),
            "Twist" => e.twist = v.clamp(-20.0, 20.0),
            "Constant Detail" => e.constant_detail = v.round().clamp(0.0, 10.0),
            "Center Position X" | "Center Position Y" => {
                let i = (name == "Center Position Y") as usize;
                e.center[i] = v.clamp(-100.0, 100.0);
                if e.center_lock {
                    e.center[1 - i] = e.center[i];
                }
            }
            "Lock" => e.center_lock = on,
            "Preset Type" => e.preset = idx(TILE_PRESETS.len()),
            "Horiz. Count" => e.horiz_count = v.clamp(0.0, 100.0),
            "Vert. Count" => e.vert_count = v.clamp(0.0, 100.0),
            "Color Variance" => e.color_variance = v.clamp(0.0, 100.0),
            "Fade Variance" => e.fade_variance = v.clamp(0.0, 1.0),
            "Horizontal Gap" | "Vertical Gap" => {
                let i = (name == "Vertical Gap") as usize;
                e.gap[i] = v.clamp(0.0, 100.0);
                if e.gap_lock {
                    e.gap[1 - i] = e.gap[i];
                }
            }
            "Gap Lock" => e.gap_lock = on,
            "% Holes" => e.holes = v.clamp(0.0, 100.0),
            "Rough" => e.rough = v.clamp(0.0, 200.0),
            "Line Shift" => e.line_shift = v.clamp(0.0, 100.0),
            "Random Shift" => e.random_shift = v.clamp(0.0, 100.0),
            "Row Modify" => e.row_modify = on,
            "Per Row" => e.per_row = value.round().clamp(0.0, 50.0) as u32,
            "Row Change" => e.row_change = v.clamp(0.0, 5.0),
            "Column Modify" => e.column_modify = on,
            "Per Column" => e.per_column = value.round().clamp(0.0, 50.0) as u32,
            "Column Change" => e.column_change = v.clamp(0.0, 5.0),
            "Gradient Type" => e.ramp_type = idx(RAMP_TYPES.len()),
            "Interpolation" => e.interpolation = idx(INTERPOLATIONS.len()),
            "Noise Amount" => e.noise_amount = v.clamp(0.0, 1.0),
            "Smooth" => e.smooth = v.clamp(0.0, 1.0),
            "Variation" => e.variation = v.clamp(0.0, 100.0),
            "Chips" => e.chips = on,
            "Circular" => e.chips = !on,
            "Spread" => e.spread = v.clamp(0.001, 100.0),
            "Bump Smoothing" => e.bump_smoothing = v.clamp(0.0, 10.0),
            "Fractal" => e.fractal = on,
            "Adaptive" => e.adaptive = on,
            "Roughness" => e.roughness = v.clamp(0.0, 1.0),
            "Mid" => e.mid = v.clamp(0.0, 1.0),
            "Iterations" => e.iterations = v.clamp(1.0, if self.kind == MapType::Smoke || self.kind == MapType::Splat { 20.0 } else { 10.0 }),
            "Strength" => e.strength = v.clamp(0.0, 1000.0),
            "Vein Width" => e.vein_width = v.clamp(0.0, 100.0),
            "Color 1 Saturation" => e.saturation[0] = v.clamp(1.0, 100.0),
            "Color 2 Saturation" => e.saturation[1] = v.clamp(1.0, 100.0),
            "Continent Size" => e.continent_size = v.clamp(0.001, 1_000_000.0),
            "Island Factor" => e.island_factor = v.clamp(0.0, 100.0),
            "Ocean %" => e.ocean = v.clamp(0.0, 100.0),
            "Blend Water/Land" => e.blend_water_land = on,
            "Exponent" => e.exponent = v.clamp(0.0, 100.0),
            "Threshold" => e.threshold = v.clamp(0.0, 1.0),
            "Thickness" => e.thickness = v.clamp(0.0, 1.0),
            "Num Wave Sets" => e.wave_sets = value.round().clamp(1.0, 50.0) as u32,
            "Wave Radius" => e.wave_radius = v.clamp(0.0, 1_000_000.0),
            "Wave Len Max" => e.wave_len_max = v.clamp(0.0, 1_000_000.0),
            "Wave Len Min" => e.wave_len_min = v.clamp(0.0, 1_000_000.0),
            "Amplitude" => e.amplitude = v.clamp(0.0, 10_000.0),
            "Distribution 2D" => e.distribution_2d = on,
            "Distribution 3D" => e.distribution_2d = !on,
            "Grain Thickness" => e.grain_thickness = v.clamp(0.0, 1_000_000.0),
            "Radial Noise" => e.radial_noise = v.clamp(0.0, 100.0),
            "Axial Noise" => e.axial_noise = v.clamp(0.0, 100.0),
            "Invert Mask" => e.invert_mask = on,
            "Alpha From" => e.alpha_from = idx(RGB_ALPHA_FROM.len()),
            "Falloff Type" => e.falloff_type = idx(FALLOFF_TYPES.len()),
            "Falloff Direction" => e.falloff_direction = idx(FALLOFF_DIRECTIONS.len()),
            "Front Amount" => e.front_amount = v.clamp(0.0, 100.0),
            "Side Amount" => e.side_amount = v.clamp(0.0, 100.0),
            "Override Material IOR" => e.override_ior = on,
            "Index of Refraction" => e.ior = v.clamp(1.0, 50.0),
            "Near Distance" => e.near_distance = v.max(0.0),
            "Far Distance" => e.far_distance = v.max(0.0),
            "Extrapolate" => e.extrapolate = on,
            "Use Curve" => e.use_curve = on,
            "Upper" => e.upper = v.clamp(0.0, 1.0),
            "Lower" => e.lower = v.clamp(0.0, 1.0),
            "Channels" => e.channels = idx(CHANNEL_OPS.len()),
            "Red" | "Green" | "Blue" | "Alpha" => {
                let i = ["Red", "Green", "Blue", "Alpha"].iter().position(|c| *c == name).expect("listed");
                e.custom_channels[i] = idx(CHANNEL_SOURCES.len());
            }
            "Hue Shift" => e.hue_shift = v.clamp(-180.0, 180.0),
            "Saturation" => e.cc_saturation = v.clamp(-100.0, 100.0),
            "Hue Tint Strength" => e.tint_strength = v.clamp(0.0, 100.0),
            "Brightness" => e.brightness = v.clamp(-100.0, 100.0),
            "Contrast" => e.contrast = v.clamp(-100.0, 100.0),
            "Sub Channel" => e.sub_channel = idx(SUB_CHANNELS.len()),
            _ => return self.set_layer_or_flag(name, value),
        }
        Ok(true)
    }

    /// Composite: Add Layer, Delete / Duplicate Layer (value: the layer
    /// number), Layer N Opacity / Blend Mode / Visible, Mask N Visible,
    /// Layer N Color Correct / Mask N Color Correct. Gradient Ramp: Add Flag
    /// (value: the position), Delete Flag (the flag number), Flag #N Position
    /// / Interpolation.
    fn set_layer_or_flag(&mut self, name: &str, value: f64) -> Result<bool, String> {
        let v = value as f32;
        let n = value.round().max(0.0) as usize;
        match (self.kind, name) {
            (MapType::Composite, "Add Layer") => {
                let at = self.layers.len() * 2;
                self.insert_components(at, 2);
                self.layers.push(CompositeLayer::default());
            }
            (MapType::Composite, "Delete Layer") => {
                if self.layers.len() < 2 {
                    return Err("a Composite map keeps at least one layer".into());
                }
                let i = n.checked_sub(1).filter(|i| *i < self.layers.len()).ok_or_else(|| format!("no layer {n}"))?;
                self.remove_components(2 * i, 2);
                self.layers.remove(i);
            }
            (MapType::Composite, "Duplicate Layer") => {
                let i = n.checked_sub(1).filter(|i| *i < self.layers.len()).ok_or_else(|| format!("no layer {n}"))?;
                let (map, mask) = (self.sub_map(2 * i).cloned(), self.sub_map(2 * i + 1).cloned());
                self.insert_components(2 * i + 2, 2);
                *self.sub_map_slot_mut(2 * i + 2) = map.map(Box::new);
                *self.sub_map_slot_mut(2 * i + 3) = mask.map(Box::new);
                let layer = self.layers[i].clone();
                self.layers.insert(i + 1, layer);
            }
            (MapType::GradientRamp, "Add Flag") => {
                let pos = v.clamp(0.0, 100.0);
                let c = self.ramp_color_at(pos);
                let at = 1 + self.flags.len();
                self.insert_components(at, 1);
                *self.color_mut(at) = c;
                self.flags.push(RampFlag { position: pos, interpolation: 4 });
            }
            (MapType::GradientRamp, "Delete Flag") => {
                if n <= 2 || n > self.flags.len() {
                    return Err(format!("Flag #{n} cannot be deleted (Flag #1 and #2 are the ends)"));
                }
                self.remove_components(n, 1);
                self.flags.remove(n - 1);
            }
            (MapType::Composite, _) => {
                let Some((head, what)) = ["Opacity", "Blend Mode", "Visible", "Color Correct"].into_iter().find_map(|w| name.strip_suffix(w).and_then(|h| h.strip_suffix(' ')).map(|h| (h, w))) else { return Ok(false) };
                let (is_mask, num) = if let Some(k) = head.strip_prefix("Layer ") { (false, k) } else if let Some(k) = head.strip_prefix("Mask ") { (true, k) } else { return Ok(false) };
                let i = num.parse::<usize>().ok().and_then(|k| k.checked_sub(1)).filter(|i| *i < self.layers.len()).ok_or_else(|| format!("no layer {num}"))?;
                match (is_mask, what) {
                    (false, "Opacity") => self.layers[i].opacity = v.clamp(0.0, 100.0),
                    (false, "Blend Mode") => self.layers[i].blend = (n).min(BLEND_MODES.len() - 1) as u8,
                    (false, "Visible") => self.layers[i].visible = value != 0.0,
                    (true, "Visible") => self.layers[i].mask_visible = value != 0.0,
                    (_, "Color Correct") => {
                        // Color Correct This Texture / Mask: the map moves into a Color Correction map.
                        let slot = 2 * i + is_mask as usize;
                        let old = self.sub_map_slot_mut(slot).take();
                        let mut cc = Map::new(MapType::ColorCorrection, 100.0);
                        cc.sub_maps[0] = old;
                        *self.sub_map_slot_mut(slot) = Some(Box::new(cc));
                    }
                    _ => return Ok(false),
                }
            }
            (MapType::GradientRamp, _) => {
                let Some(rest) = name.strip_prefix("Flag #") else { return Ok(false) };
                let (num, what) = rest.split_once(' ').ok_or_else(|| format!("no map parameter \"{name}\""))?;
                let i = num.parse::<usize>().ok().and_then(|k| k.checked_sub(1)).filter(|i| *i < self.flags.len()).ok_or_else(|| format!("no Flag #{num}"))?;
                match what {
                    // The end flags stay at 0 and 100.
                    "Position" if i >= 2 => self.flags[i].position = v.clamp(0.0, 100.0),
                    "Position" => return Err("Flag #1 and #2 stay at the ends".into()),
                    "Interpolation" => self.flags[i].interpolation = n.min(INTERPOLATIONS.len() - 1) as u8,
                    _ => return Ok(false),
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_indexes_round_trip() {
        for name in ["Diffuse Color", "Color #3", "Base", "Mix Amount", "Layer 3", "Mask 12", "Flag #7", "Hue Tint"] {
            assert_eq!(slot_name(slot_index(name)), name);
        }
    }

    #[test]
    fn composite_layers_and_ramp_flags_keep_their_maps() {
        let mut c = Map::new(MapType::Composite, 100.0);
        assert_eq!(c.slot_names(), ["Layer 1", "Mask 1"]);
        *c.sub_map_slot_mut(0) = Some(Box::new(Map::new(MapType::Checker, 100.0)));
        c.set_parameter("Add Layer", 1.0).unwrap();
        c.set_parameter("Add Layer", 1.0).unwrap();
        *c.sub_map_slot_mut(5) = Some(Box::new(Map::new(MapType::Noise, 100.0)));
        assert_eq!(c.slot_names().len(), 6);
        c.set_parameter("Delete Layer", 2.0).unwrap();
        assert_eq!(c.layers.len(), 2);
        assert_eq!(c.sub_map(0).map(|m| m.kind), Some(MapType::Checker));
        assert_eq!(c.sub_map(3).map(|m| m.kind), Some(MapType::Noise), "layer 3's mask is layer 2's now");
        c.set_parameter("Layer 2 Opacity", 40.0).unwrap();
        c.set_parameter("Layer 2 Blend Mode", 5.0).unwrap();
        assert_eq!((c.layers[1].opacity, BLEND_MODES[c.layers[1].blend as usize]), (40.0, "Multiply"));
        assert!(c.set_parameter("Layer 9 Opacity", 1.0).is_err());
        // Color Correct This Texture wraps the layer's map.
        c.set_parameter("Layer 1 Color Correct", 1.0).unwrap();
        let cc = c.sub_map(0).unwrap();
        assert_eq!((cc.kind, cc.sub_map(0).map(|m| m.kind)), (MapType::ColorCorrection, Some(MapType::Checker)));
        // A 3D axis name and a two-word name ending in an axis letter.
        c.set_parameter("Center Position X", 0.25).unwrap();
        assert_eq!(c.extra.center, [0.25, 0.25], "locked X and Y");
        c.set_parameter("Tiling Z", 3.0).unwrap();
        assert_eq!(c.coords3.tiling[2], 3.0);

        let mut g = Map::new(MapType::GradientRamp, 100.0);
        assert_eq!(g.slot_names(), ["Source Map", "Flag #1", "Flag #2", "Flag #3"]);
        assert_eq!(g.ramp_color_at(0.0), Vec3::X);
        assert_eq!(g.ramp_color_at(50.0), Vec3::Y);
        assert_eq!(g.ramp_color_at(100.0), Vec3::Z);
        g.set_parameter("Add Flag", 25.0).unwrap();
        assert!((g.color(4) - Vec3::new(0.5, 0.5, 0.0)).length() < 1e-5, "a new flag takes the colour under it");
        assert!(g.set_parameter("Flag #1 Position", 10.0).is_err());
        g.set_parameter("Flag #3 Position", 75.0).unwrap();
        g.set_parameter("Delete Flag", 4.0).unwrap();
        assert_eq!(g.flags.len(), 3);
        assert!(g.set_parameter("Delete Flag", 2.0).is_err());
    }
}
