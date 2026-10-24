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
    /// Display layers (the Layer Manager's layers).
    pub layers: Vec<FbxLayer>,
}

/// A display layer: name, colour (0-1), Show / Freeze and its nodes.
#[derive(Debug, Clone, PartialEq)]
pub struct FbxLayer {
    pub name: String,
    pub color: Vec3,
    pub visible: bool,
    pub frozen: bool,
    /// Indices into `FbxDocument::nodes`.
    pub nodes: Vec<usize>,
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
}

impl Default for WriteOpts {
    fn default() -> Self {
        Self { creator: format!("3dloom {}", env!("CARGO_PKG_VERSION")), axes: WriteAxes::ZUp, smoothing_layer: true }
    }
}
