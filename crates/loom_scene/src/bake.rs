//! Render To Texture settings saved with the scene: the dialog's General
//! Settings, Objects to Bake, Output, Baked Material and Automatic Mapping
//! rollouts and the Projection Options dialog. One set for the session,
//! applied to every object baked.

use serde::{Deserialize, Serialize};

/// Baked texture element types (the Add Texture Elements list).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ElementKind {
    #[default]
    Complete,
    Specular,
    Diffuse,
    Shadows,
    Lighting,
    Normals,
    Alpha,
    Height,
}

impl ElementKind {
    pub const ALL: [ElementKind; 8] = [ElementKind::Complete, ElementKind::Specular, ElementKind::Diffuse, ElementKind::Shadows, ElementKind::Lighting, ElementKind::Normals, ElementKind::Alpha, ElementKind::Height];

    /// The element type name (also the default element name).
    pub fn label(self) -> &'static str {
        match self {
            ElementKind::Complete => "CompleteMap",
            ElementKind::Specular => "SpecularMap",
            ElementKind::Diffuse => "DiffuseMap",
            ElementKind::Shadows => "ShadowsMap",
            ElementKind::Lighting => "LightingMap",
            ElementKind::Normals => "NormalsMap",
            ElementKind::Alpha => "AlphaMap",
            ElementKind::Height => "HeightMap",
        }
    }

    pub fn from_label(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.label().eq_ignore_ascii_case(s) || k.label().trim_end_matches("Map").eq_ignore_ascii_case(s))
    }

    /// Selected Element Unique Settings: the component toggles this type shows.
    pub fn components(self) -> &'static [&'static str] {
        match self {
            ElementKind::Complete => &["Shadows"],
            ElementKind::Specular | ElementKind::Diffuse => &["Lighting", "Shadows"],
            ElementKind::Lighting => &["Shadows", "Direct Light On", "Indirect Light On"],
            ElementKind::Normals => &["Output into Normal Bump", "Render Height Map into Alpha Channel"],
            ElementKind::Shadows | ElementKind::Alpha | ElementKind::Height => &[],
        }
    }

    /// Rendered by the GPU renderer (lit by the scene); the others are geometry.
    pub fn lit(self) -> bool {
        matches!(self, ElementKind::Complete | ElementKind::Specular | ElementKind::Diffuse | ElementKind::Shadows | ElementKind::Lighting)
    }
}

/// One row of the Output rollout's list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BakeElement {
    pub kind: ElementKind,
    pub enabled: bool,
    /// Name: the element part of the file name.
    pub name: String,
    /// File Name and Type: a file name of its own (empty: object name +
    /// element name), its extension the type.
    pub file: String,
    /// The file type when `file` has none (tga, png, ...).
    pub format: String,
    pub width: u32,
    pub height: u32,
    /// Target Map Slot (a Maps rollout channel), empty for none.
    pub target_slot: String,
    /// Element Background, sRGB 0–1.
    pub background: [f32; 3],
    /// Unique settings, by component name (see `ElementKind::components`); all on by default.
    pub shadows: bool,
    pub lighting: bool,
    pub direct: bool,
    pub indirect: bool,
    pub normal_bump: bool,
    pub height_in_alpha: bool,
}

impl Default for BakeElement {
    fn default() -> Self {
        Self::new(ElementKind::Complete)
    }
}

impl BakeElement {
    pub fn new(kind: ElementKind) -> Self {
        Self {
            kind,
            enabled: true,
            name: kind.label().to_owned(),
            file: String::new(),
            format: "tga".into(),
            width: 256,
            height: 256,
            target_slot: String::new(),
            background: [0.0; 3],
            shadows: true,
            lighting: true,
            direct: true,
            indirect: true,
            normal_bump: false,
            height_in_alpha: false,
        }
    }

    /// A component toggle by its Unique Settings name.
    pub fn component(&self, name: &str) -> Option<bool> {
        Some(match name {
            "Shadows" => self.shadows,
            "Lighting" => self.lighting,
            "Direct Light On" => self.direct,
            "Indirect Light On" => self.indirect,
            "Output into Normal Bump" => self.normal_bump,
            "Render Height Map into Alpha Channel" => self.height_in_alpha,
            _ => return None,
        })
    }

    pub fn set_component(&mut self, name: &str, on: bool) -> Result<(), String> {
        let c = match name {
            "Shadows" => &mut self.shadows,
            "Lighting" => &mut self.lighting,
            "Direct Light On" => &mut self.direct,
            "Indirect Light On" => &mut self.indirect,
            "Output into Normal Bump" => &mut self.normal_bump,
            "Render Height Map into Alpha Channel" => &mut self.height_in_alpha,
            other => return Err(format!("no component \"{other}\"")),
        };
        *c = on;
        Ok(())
    }

    /// The file this element writes for object `object` in folder `dir`.
    pub fn path(&self, dir: &std::path::Path, object: &str) -> std::path::PathBuf {
        if !self.file.is_empty() {
            let f = std::path::Path::new(&self.file);
            return if f.is_absolute() { f.to_path_buf() } else { dir.join(f) };
        }
        let ext = if self.format.is_empty() { "tga" } else { self.format.as_str() };
        dir.join(format!("{object}{}.{ext}", self.name))
    }
}

/// Objects to Bake: which objects a Render bakes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BakeObjects {
    Individual,
    #[default]
    AllSelected,
    /// Every visible, unfrozen object that has the mapping to bake into.
    AllPrepared,
}

impl BakeObjects {
    pub fn label(self) -> &'static str {
        match self {
            BakeObjects::Individual => "Individual",
            BakeObjects::AllSelected => "All Selected",
            BakeObjects::AllPrepared => "All Prepared",
        }
    }

    pub fn from_label(s: &str) -> Option<Self> {
        [BakeObjects::Individual, BakeObjects::AllSelected, BakeObjects::AllPrepared].into_iter().find(|o| o.label().eq_ignore_ascii_case(s))
    }
}

/// Projection Options > Normal Map Space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum NormalSpace {
    World,
    Screen,
    #[serde(rename = "Local XYZ")]
    LocalXyz,
    #[default]
    Tangent,
}

impl NormalSpace {
    pub const ALL: [NormalSpace; 4] = [NormalSpace::World, NormalSpace::Screen, NormalSpace::LocalXyz, NormalSpace::Tangent];

    pub fn label(self) -> &'static str {
        match self {
            NormalSpace::World => "World",
            NormalSpace::Screen => "Screen",
            NormalSpace::LocalXyz => "Local XYZ",
            NormalSpace::Tangent => "Tangent",
        }
    }

    pub fn from_label(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|n| n.label().eq_ignore_ascii_case(s))
    }
}

/// The Projection Options dialog.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectionOptions {
    /// Use Cage (the Projection modifier's cage); off: Offset.
    pub use_cage: bool,
    pub offset: f32,
    /// Resolve Hit: Closest (off: Furthest).
    pub closest: bool,
    pub match_material_id: bool,
    pub include_working_model: bool,
    pub ray_miss_check: bool,
    /// Ray miss colour, sRGB 0–1.
    pub ray_miss_color: [f32; 3],
    pub space: NormalSpace,
    /// Tangent: Red Left (off: Right); World / Screen / Local XYZ: Red -X (off: +X).
    pub red_flip: bool,
    /// Tangent: Green Up (off: Down); World / Screen / Local XYZ: Green -Y (off: +Y).
    pub green_flip: bool,
    pub min_height: f32,
    pub max_height: f32,
    /// Buffer min / max Height: the heights the last projection found.
    pub buffer_min: f32,
    pub buffer_max: f32,
}

impl Default for ProjectionOptions {
    fn default() -> Self {
        Self {
            use_cage: true,
            offset: 10.0,
            closest: false,
            match_material_id: false,
            include_working_model: false,
            ray_miss_check: true,
            ray_miss_color: [1.0, 0.0, 0.0],
            space: NormalSpace::Tangent,
            red_flip: false,
            green_flip: false,
            min_height: 0.0,
            max_height: 10.0,
            buffer_min: 0.0,
            buffer_max: 0.0,
        }
    }
}

/// The Render To Texture dialog's values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BakeSettings {
    /// General Settings > Output path (empty: the `images` folder of the
    /// program's data directory).
    pub output_dir: String,
    pub skip_existing: bool,
    /// Show each baked map in the Rendered Frame Window.
    pub rendered_frame_window: bool,
    pub objects: BakeObjects,
    /// Selected Object Settings > Enable and Padding (texels of edge padding).
    pub enable: bool,
    pub padding: u32,
    /// Projection Mapping > Enabled (the objects' Projection modifiers).
    pub projection: bool,
    pub options: ProjectionOptions,
    /// Mapping Coordinates: Use Automatic Unwrap (off: Use Existing Channel) and the channel.
    pub auto_unwrap: bool,
    pub auto_channel: u32,
    pub existing_channel: u32,
    pub elements: Vec<BakeElement>,
    /// Baked Material: Output Into Source, or Save Source (Create Shell)
    /// with Duplicate Source To Baked or Create New Baked (Standard).
    pub output_into_source: bool,
    pub create_new_baked: bool,
    pub render_to_files_only: bool,
    /// Clear Shell Materials keeps the baked (off: the source) materials.
    pub keep_baked: bool,
    /// Original / Baked: what the Shell materials show in viewports and renders.
    pub views_baked: bool,
    pub render_baked: bool,
    /// Automatic Mapping rollout.
    pub rotate_clusters: bool,
    pub threshold_angle: f32,
    pub fill_holes: bool,
    pub spacing: f32,
}

impl Default for BakeSettings {
    fn default() -> Self {
        Self {
            output_dir: String::new(),
            skip_existing: false,
            rendered_frame_window: true,
            objects: BakeObjects::AllSelected,
            enable: false,
            padding: 2,
            projection: false,
            options: ProjectionOptions::default(),
            auto_unwrap: true,
            auto_channel: 3,
            existing_channel: 1,
            elements: Vec::new(),
            output_into_source: false,
            create_new_baked: true,
            render_to_files_only: false,
            keep_baked: false,
            views_baked: true,
            render_baked: true,
            rotate_clusters: true,
            threshold_angle: 45.0,
            fill_holes: true,
            spacing: 0.02,
        }
    }
}

impl BakeSettings {
    /// The map channel the bake renders into.
    pub fn channel(&self) -> u32 {
        if self.auto_unwrap {
            self.auto_channel
        } else {
            self.existing_channel
        }
    }
}

/// The name of the Unwrap UVW entry Use Automatic Unwrap adds.
pub const AUTO_UNWRAP_NAME: &str = "Automatic Flatten UVs";

/// The Projection modifier: Reference Geometry (the high-resolution
/// objects) and the cage's Push amount (world units along the normals).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectionData {
    pub targets: Vec<crate::NodeId>,
    pub push: crate::Param<f32>,
}

impl Default for ProjectionData {
    fn default() -> Self {
        Self { targets: Vec::new(), push: crate::Param::new(0.0) }
    }
}
