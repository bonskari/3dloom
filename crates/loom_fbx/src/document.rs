//! The neutral FBX document: what the scene gives the writer and what the
//! reader gives the scene. Positions are in `unit_metres` units, Z up,
//! right-handed (the scene's own frame).

use glam::{Mat4, Vec3};
use loom_mesh::Mesh;

#[derive(Debug, Clone, Default)]
pub struct FbxDocument {
    /// Parents before children.
    pub nodes: Vec<FbxNode>,
    /// Metres per unit of the positions in `nodes`.
    pub unit_metres: f64,
    /// The animation take (the file's first anim stack), baked per frame.
    pub anim: Option<FbxTake>,
    /// Surface descriptions of the materials the meshes name (by name).
    pub materials: Vec<FbxMaterial>,
    /// (node, earlier node): the node shares the earlier node's geometry (an instance).
    pub instances: Vec<(usize, usize)>,
    /// Named selection sets of whole nodes.
    pub selection_sets: Vec<(String, Vec<usize>)>,
    /// (node, preview levels, render levels): a mesh written as the cage of a smooth surface.
    pub subdivision: Vec<(usize, u32, u32)>,
}

/// A material as FBX carries it (a Phong or Lambert surface). Colours 0–1.
#[derive(Debug, Clone, PartialEq)]
pub struct FbxMaterial {
    pub name: String,
    /// `phong` or `lambert`.
    pub shading: String,
    pub ambient: [f32; 3],
    pub diffuse: [f32; 3],
    pub specular: [f32; 3],
    /// Specular strength (1 = full).
    pub specular_factor: f32,
    /// Phong exponent.
    pub shininess: f32,
    /// 0 transparent … 1 opaque.
    pub opacity: f32,
    /// Self-illumination: the diffuse colour emitted, 0–1.
    pub emissive_factor: f32,
    pub textures: Vec<FbxTexture>,
}

impl FbxMaterial {
    pub fn new(name: &str) -> Self {
        Self { name: name.into(), shading: "phong".into(), ambient: [0.2; 3], diffuse: [0.8; 3], specular: [0.2; 3], specular_factor: 0.0, shininess: 20.0, opacity: 1.0, emissive_factor: 0.0, textures: Vec::new() }
    }
}

/// A file texture on one material property.
#[derive(Debug, Clone, PartialEq)]
pub struct FbxTexture {
    /// The FBX material property it drives: `DiffuseColor`, `SpecularFactor`,
    /// `TransparentColor`, `Bump`, `NormalMap`, …
    pub property: String,
    /// The image file as written (absolute) or as found when reading.
    pub path: String,
    /// The path relative to the FBX file, when there is one.
    pub relative: String,
    /// Map strength, 1 = full (Bump: the bump factor).
    pub amount: f32,
    /// The image bytes embedded in the file (Embed Media).
    pub content: Option<Vec<u8>>,
}

/// One animation take: per-frame local transforms of the nodes that move.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FbxTake {
    pub name: String,
    pub fps: f32,
    pub start: i32,
    pub end: i32,
    /// (index into `FbxDocument::nodes`, its samples).
    pub tracks: Vec<(usize, FbxAnim)>,
}

/// A node's local transform per frame, in document units and Euler XYZ degrees.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FbxAnim {
    pub frames: Vec<f32>,
    pub translation: Vec<Vec3>,
    pub rotation_deg: Vec<Vec3>,
    pub scale: Vec<Vec3>,
}

#[derive(Debug, Clone)]
pub struct FbxNode {
    pub name: String,
    /// Index into `FbxDocument::nodes`.
    pub parent: Option<usize>,
    /// Local, in document units.
    pub translation: Vec3,
    /// Local Euler XYZ in degrees (XYZ order).
    pub rotation_deg: Vec3,
    pub scale: Vec3,
    pub kind: FbxKind,
    /// A skin deformer on a mesh node.
    pub skin: Option<FbxSkin>,
    /// Blend shape channels on a mesh node.
    pub blend_shapes: Vec<FbxBlendShape>,
}

impl FbxNode {
    pub fn new(name: &str, kind: FbxKind) -> Self {
        Self { name: name.into(), parent: None, translation: Vec3::ZERO, rotation_deg: Vec3::ZERO, scale: Vec3::ONE, kind, skin: None, blend_shapes: Vec::new() }
    }
}

#[derive(Debug, Clone)]
pub enum FbxKind {
    /// `materials[i]` names material id `i + 1`.
    Mesh { mesh: Mesh, materials: Vec<String> },
    Camera { fov_deg: f32 },
    /// A skeleton bone (FBX LimbNode): `length` to its child joint, `size` its drawn width.
    Bone { length: f32, size: f32 },
    Empty,
}

/// A skin deformer: per bone its `geometry_to_bone` binding (mesh-local
/// vertices → bone space at bind) and per vertex its (bone index, weight).
#[derive(Debug, Clone, PartialEq)]
pub struct FbxSkin {
    /// (index into `FbxDocument::nodes`, geometry_to_bone at bind).
    pub bones: Vec<(usize, Mat4)>,
    /// The mesh node's world matrix at bind.
    pub mesh_bind: Mat4,
    pub weights: Vec<Vec<(u32, f32)>>,
}

/// One blend shape channel: sparse vertex offsets (document units) and its weight in percent.
#[derive(Debug, Clone, PartialEq)]
pub struct FbxBlendShape {
    pub name: String,
    pub offsets: Vec<(u32, Vec3)>,
    pub weight: f32,
}

/// Which way the written file says is up. The document is always Z-up;
/// `YUp` converts on the way out (x, z, -y), the frame Unity-style
/// readers expect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WriteAxes {
    #[default]
    ZUp,
    YUp,
}

#[derive(Debug, Clone)]
pub struct WriteOpts {
    /// Written into `FBXHeaderExtension/Creator`.
    pub creator: String,
    pub axes: WriteAxes,
    /// Write `LayerElementSmoothing` (off: readers must recover groups from normals).
    pub smoothing_layer: bool,
    /// Split control points where their normals differ (hard edges become seams).
    pub split_normals: bool,
    /// Write `LayerElementTangent` / `LayerElementBinormal` from map channel 1.
    pub tangents: bool,
    /// Every polygon written as triangles.
    pub triangulate: bool,
    /// Texture images go into the file (`Video/Content`).
    pub embed_media: bool,
    /// ASCII text instead of binary.
    pub ascii: bool,
    /// The FBX file version: 7400 or 7500 (7.5 has 64-bit record offsets).
    pub version: u32,
    /// Centimetres per unit of the written file (`UnitScaleFactor`); 1 = centimetres.
    pub file_unit_cm: f64,
    /// The folder the file goes in: texture paths are also written relative to it.
    pub file_dir: Option<std::path::PathBuf>,
    pub curve_filters: CurveFilters,
}

/// Animation curve filters (Constant Key Reducer, Key Reducer, Remove single key).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CurveFilters {
    /// Constant Key Reducer thresholds for translation, rotation, scale and other curves; None = off.
    pub constant: Option<[f32; 4]>,
    /// Key Reducer precision and XYZ curve synchronization; None = off.
    pub key_reducer: Option<(f32, bool)>,
    /// A curve left with a single key is not written (the node keeps that value).
    pub remove_single_key: bool,
}

impl Default for WriteOpts {
    fn default() -> Self {
        Self {
            creator: format!("3dloom {}", env!("CARGO_PKG_VERSION")),
            axes: WriteAxes::ZUp,
            smoothing_layer: true,
            split_normals: false,
            tangents: false,
            triangulate: false,
            embed_media: false,
            ascii: false,
            version: 7400,
            file_unit_cm: 1.0,
            file_dir: None,
            curve_filters: CurveFilters::default(),
        }
    }
}

/// What `read_with` brings in.
#[derive(Debug, Clone)]
pub struct ReadOpts {
    /// Metres per unit of the returned document.
    pub unit_metres: f64,
    /// Use the file's smoothing-group bits (off: groups come from the normals).
    pub smoothing_groups: bool,
    pub animation: bool,
    /// The take to read (by name); None = the first.
    pub take: Option<String>,
    /// Evaluate every animation layer (off: the base layer only).
    pub bake_layers: bool,
    pub skins: bool,
    pub morphs: bool,
}

impl ReadOpts {
    pub fn new(unit_metres: f64) -> Self {
        Self { unit_metres, smoothing_groups: true, animation: true, take: None, bake_layers: true, skins: true, morphs: true }
    }
}

/// What the Import dialog's Statistics show about a file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FbxInfo {
    pub version: u32,
    pub ascii: bool,
    pub creator: String,
    /// `CreationTime` as written ("YYYY-MM-DD hh:mm:ss:mmm"), empty if absent.
    pub creation_time: String,
    /// "Y-up", "Z-up" or "X-up".
    pub up_axis: String,
    /// Metres per file unit.
    pub unit_metres: f64,
    pub fps: f64,
    pub takes: Vec<String>,
    pub nodes: usize,
    pub meshes: usize,
    pub materials: usize,
    pub textures: usize,
    pub cameras: usize,
    pub lights: usize,
    pub caches: usize,
}
