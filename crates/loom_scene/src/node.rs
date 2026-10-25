use crate::param::Param;
use glam::{Mat4, Quat, Vec3};
use loom_mesh::Mesh;
use loom_mesh::uvmap::AutoMap;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u64);

/// Position / Rotation / Scale, as the default PRS controllers:
/// rotation is an Euler XYZ triple in degrees, so it can accumulate past
/// 360° and reads back exactly as typed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Transform {
    pub position: Param<Vec3>,
    /// Euler XYZ rotation in degrees.
    pub rotation: Param<Vec3>,
    pub scale: Param<Vec3>,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            position: Param::new(Vec3::ZERO),
            rotation: Param::new(Vec3::ZERO),
            scale: Param::new(Vec3::ONE),
        }
    }
}

impl Transform {
    pub fn rotation_quat(&self) -> Quat {
        let r = self.rotation.get();
        Quat::from_euler(glam::EulerRot::XYZ, r.x.to_radians(), r.y.to_radians(), r.z.to_radians())
    }

    pub fn set_rotation_quat(&mut self, q: Quat) {
        let (x, y, z) = q.to_euler(glam::EulerRot::XYZ);
        self.rotation.set(Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees()));
    }

    pub fn matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale.get(), self.rotation_quat(), self.position.get())
    }

    /// A constant transform from a matrix (commits merge it onto the node's
    /// controllers, so keys survive).
    pub fn from_matrix(m: Mat4) -> Self {
        let (scale, rotation, position) = m.to_scale_rotation_translation();
        let (x, y, z) = rotation.to_euler(glam::EulerRot::XYZ);
        Self { position: Param::new(position), rotation: Param::new(Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees())), scale: Param::new(scale) }
    }

    /// `after` written onto this transform's controllers: a constant
    /// component is set through the controller (keyed under Auto Key, or
    /// editing / offsetting existing keys), any other controller replaces it.
    pub fn merged(&self, after: &Transform) -> Transform {
        fn merge<T: crate::param::Animatable>(cur: &Param<T>, new: &Param<T>) -> Param<T> {
            match &new.controller {
                crate::param::Controller::Constant(v) => {
                    let mut p = cur.clone();
                    if cur.get() != *v {
                        p.set(v.clone());
                    }
                    p
                }
                _ => new.clone(),
            }
        }
        Transform { position: merge(&self.position, &after.position), rotation: merge(&self.rotation, &after.rotation), scale: merge(&self.scale, &after.scale) }
    }

    /// Like `from_matrix`, but each Euler component takes the 360° wrap
    /// nearest to `prev + hint_deg` (the Euler change the caller intends),
    /// so interactive rotation keeps counting past 360 like an Euler XYZ
    /// controller.
    pub fn from_matrix_near(m: Mat4, prev: &Transform, hint_deg: Vec3) -> Self {
        let mut t = Self::from_matrix(m);
        let p = prev.rotation.get() + hint_deg;
        let mut r = t.rotation.get();
        for i in 0..3 {
            let turns = ((p[i] - r[i]) / 360.0).round();
            r[i] += turns * 360.0;
        }
        // The Euler decomposition may flip to the (x+180, 180-y, z+180)
        // representation; prefer the one closer to prev.
        let alt = Vec3::new(r.x + 180.0, 180.0 - r.y, r.z + 180.0);
        let mut alt_n = alt;
        for i in 0..3 {
            let turns = ((p[i] - alt_n[i]) / 360.0).round();
            alt_n[i] += turns * 360.0;
        }
        let pick = if (alt_n - p).length() + 1e-3 < (r - p).length() { alt_n } else { r };
        t.rotation = Param::new(pick);
        t
    }
}

/// A parameter value as shown in a rollout spinner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParamValue {
    Float(f32),
    Int(u32),
    Bool(bool),
}

impl ParamValue {
    pub fn as_f64(&self) -> f64 {
        match self {
            ParamValue::Float(f) => *f as f64,
            ParamValue::Int(i) => *i as f64,
            ParamValue::Bool(b) => if *b { 1.0 } else { 0.0 },
        }
    }
}

/// Editable Poly > Subdivision Surface rollout.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubdivSurface {
    pub use_nurms: Param<bool>,
    pub smooth_result: bool,
    pub isoline_display: bool,
    pub show_cage: bool,
    pub iterations: Param<u32>,
    pub smoothness: Param<f32>,
    pub separate_by_smoothing: bool,
    pub separate_by_materials: bool,
}

impl Default for SubdivSurface {
    fn default() -> Self {
        Self {
            use_nurms: Param::new(false),
            smooth_result: true,
            isoline_display: true,
            show_cage: true,
            iterations: Param::new(1),
            smoothness: Param::new(1.0),
            separate_by_smoothing: false,
            separate_by_materials: false,
        }
    }
}

/// Editable Poly > Soft Selection rollout.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SoftSelection {
    pub on: bool,
    /// Edge Distance (None = off), in edges.
    pub edge_distance: Option<u32>,
    pub affect_backfacing: bool,
    pub falloff: Param<f32>,
    pub pinch: Param<f32>,
    pub bubble: Param<f32>,
    pub lock: bool,
    /// Painted weights per vertex (Paint Soft Selection), when any.
    #[serde(default)]
    pub painted: Option<Vec<f32>>,
}

impl Default for SoftSelection {
    fn default() -> Self {
        Self { on: false, edge_distance: None, affect_backfacing: true, falloff: Param::new(20.0), pinch: Param::new(0.0), bubble: Param::new(0.0), lock: false, painted: None }
    }
}

/// Editable Poly > Paint Deformation rollout.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum PaintDirection {
    OriginalNormals,
    DeformedNormals,
    TransformAxis(u8),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PaintDeform {
    pub push_pull_value: f32,
    pub brush_size: f32,
    pub brush_strength: f32,
    pub direction: PaintDirection,
    /// Vertex positions before the first stroke, until Commit / Cancel.
    #[serde(default)]
    pub original: Option<Vec<glam::Vec3>>,
}

impl Default for PaintDeform {
    fn default() -> Self {
        Self { push_pull_value: 10.0, brush_size: 20.0, brush_strength: 1.0, direction: PaintDirection::OriginalNormals, original: None }
    }
}

/// An Editable Poly: the mesh plus the rollout settings that are not mesh data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolyObject {
    pub mesh: Mesh,
    #[serde(default)]
    pub subdiv: SubdivSurface,
    #[serde(default)]
    pub soft: SoftSelection,
    #[serde(default)]
    pub paint: PaintDeform,
    /// The sub-object selection passed up the stack while a sub-object
    /// level is active on the base object (the Editable Poly's
    /// selection flows into the modifiers above).
    #[serde(default, skip)]
    pub passes_up: Option<(loom_mesh::select_ops::SubLevel, Vec<u32>)>,
}

impl PolyObject {
    pub fn from_mesh(mesh: Mesh) -> Self {
        Self { mesh, subdiv: SubdivSurface::default(), soft: SoftSelection::default(), paint: PaintDeform::default(), passes_up: None }
    }

    /// The same settings with a new mesh (every mesh edit goes through here).
    pub fn with_mesh(&self, mesh: Mesh) -> Self {
        Self { mesh, subdiv: self.subdiv.clone(), soft: self.soft.clone(), paint: self.paint.clone(), passes_up: self.passes_up.clone() }
    }

    /// The mesh the viewports show: the NURMS subdivision result when Use
    /// NURMS Subdivision is on (display iterations), else the base mesh.
    pub fn display_mesh(&self) -> std::borrow::Cow<'_, Mesh> {
        let s = &self.subdiv;
        if !s.use_nurms.get() || s.iterations.get() == 0 {
            return std::borrow::Cow::Borrowed(&self.mesh);
        }
        let mut m = self.mesh.clone();
        for _ in 0..s.iterations.get().min(4) {
            match loom_mesh::geom_ops::msmooth(&m, s.smoothness.get(), s.separate_by_smoothing, s.separate_by_materials) {
                Ok(n) => m = n,
                Err(_) => break,
            }
        }
        std::borrow::Cow::Owned(m)
    }
}

/// An Editable Patch: the patch lattice plus its Surface rollout settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatchObject {
    pub patch: loom_mesh::patch::PatchMesh,
    /// Surface > View Steps: segments per patch edge are Steps + 1.
    pub steps: Param<u32>,
    /// Surface > Show Interior Edges.
    #[serde(default)]
    pub show_interior: bool,
}

impl PatchObject {
    pub fn from_patch(patch: loom_mesh::patch::PatchMesh) -> Self {
        Self { patch, steps: Param::new(5), show_interior: false }
    }

    /// The same settings with a new lattice (every patch edit goes through here).
    pub fn with_patch(&self, patch: loom_mesh::patch::PatchMesh) -> Self {
        Self { patch, steps: self.steps.clone(), show_interior: self.show_interior }
    }

    /// The tessellated surface the viewports show.
    pub fn surface(&self) -> Mesh {
        self.patch.tessellate(self.steps.get(), self.show_interior)
    }
}

/// What a node is. Parametric kinds mirror the Create panel
/// rollouts (same parameter names) and rebuild their mesh from the
/// parameters; `EditablePoly` owns its mesh.
/// An Editable Spline (or a Line, which is one from birth).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShapeObject {
    pub shape: loom_shape::Shape,
    #[serde(default)]
    pub soft: SoftSelection,
    /// Created by the Line tool: named and listed as "Line".
    #[serde(default)]
    pub is_line: bool,
    /// Hidden splines (Editable Spline > Hide), by spline index.
    #[serde(default)]
    pub hidden: std::collections::BTreeSet<usize>,
}

impl ShapeObject {
    pub fn from_shape(shape: loom_shape::Shape) -> Self {
        Self { shape, soft: SoftSelection::default(), is_line: false, hidden: Default::default() }
    }
}

/// The Rendering and Interpolation rollouts every parametric shape carries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShapeCommon {
    pub steps: Param<u32>,
    pub optimize: Param<bool>,
    pub adaptive: Param<bool>,
    pub render: loom_shape::RenderParams,
}

impl Default for ShapeCommon {
    fn default() -> Self {
        Self { steps: Param::new(6), optimize: Param::new(true), adaptive: Param::new(false), render: loom_shape::RenderParams::default() }
    }
}

impl ShapeCommon {
    fn apply(&self, mut shape: loom_shape::Shape) -> loom_shape::Shape {
        shape.steps = self.steps.get();
        shape.optimize = self.optimize.get();
        shape.adaptive = self.adaptive.get();
        shape.render = self.render.clone();
        shape
    }
}

/// The mesh a shape shows and picks by: the rendering tube when Enable In
/// Viewport, else isolated vertices — the knots (flat order) for an
/// editable spline so vertex picking and transforms address them, the
/// interpolated points for a parametric shape.
pub fn shape_cage_mesh(sh: &loom_shape::Shape, editable: bool) -> Mesh {
    if sh.render.enable_in_viewport {
        return sh.tube_mesh();
    }
    let pts: Vec<Vec3> = if editable { sh.knot_positions() } else { sh.polylines().into_iter().flatten().collect() };
    let mut im = loom_mesh::indexed::IndexedMesh::default();
    for p in pts {
        let id = im.add_vertex(p, None);
        im.keep.insert(id);
    }
    im.build().unwrap_or_default()
}

/// Rendering / Interpolation rollout parameter names shared by every shape.
pub const SHAPE_COMMON_NAMES: [&str; 14] = [
    "Steps", "Optimize", "Adaptive", "Enable In Renderer", "Enable In Viewport", "Use Viewport Settings", "Generate Mapping Coords", "Rectangular", "Thickness", "Sides", "Angle",
    "Rect Length", "Rect Width", "Auto Smooth",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ObjectKind {
    EditablePoly(PolyObject),
    EditableSpline(ShapeObject),
    /// An Editable Mesh: its mesh is all triangles, polygons are triangles
    /// joined by invisible edges (the mesh's edge flags).
    EditableMesh(PolyObject),
    /// An Editable Patch.
    EditablePatch(PatchObject),
    /// Create > Patch Grids > Quad Patch: `width` × `length` in quad patches.
    QuadPatch { length: Param<f32>, width: Param<f32>, length_segs: Param<u32>, width_segs: Param<u32> },
    /// Create > Patch Grids > Tri Patch: a rectangle of two tri patches.
    TriPatch { length: Param<f32>, width: Param<f32> },
    Rectangle { length: Param<f32>, width: Param<f32>, corner_radius: Param<f32>, common: ShapeCommon },
    Circle { radius: Param<f32>, common: ShapeCommon },
    Ellipse { length: Param<f32>, width: Param<f32>, common: ShapeCommon },
    Arc { radius: Param<f32>, from: Param<f32>, to: Param<f32>, pie_slice: Param<bool>, reverse: Param<bool>, common: ShapeCommon },
    Donut { radius1: Param<f32>, radius2: Param<f32>, common: ShapeCommon },
    NGon { radius: Param<f32>, inscribed: Param<bool>, sides: Param<u32>, corner_radius: Param<f32>, circular: Param<bool>, common: ShapeCommon },
    Star { radius1: Param<f32>, radius2: Param<f32>, points: Param<u32>, distortion: Param<f32>, fillet1: Param<f32>, fillet2: Param<f32>, common: ShapeCommon },
    Helix { radius1: Param<f32>, radius2: Param<f32>, height: Param<f32>, turns: Param<f32>, bias: Param<f32>, cw: Param<bool>, common: ShapeCommon },
    /// Compound Objects > Boolean: operands as copies in this object's space.
    Boolean { a: Mesh, b: Mesh, op: Param<u32>, display: Param<u32>, #[serde(skip)] cache: CompoundCache },
    /// Compound Objects > ProBoolean: the base and its (operation, operand) list.
    ProBoolean { base: Mesh, operands: Vec<(u32, Mesh)>, #[serde(skip)] cache: CompoundCache },
    /// Compound Objects > Loft: shapes at path percents along a path (object space).
    Loft { path: loom_shape::Shape, shapes: Vec<(f32, loom_shape::Shape)>, shape_steps: Param<u32>, path_steps: Param<u32>, cap_start: Param<bool>, cap_end: Param<bool>, contour: Param<bool>, flip_normals: Param<bool>, #[serde(skip)] cache: CompoundCache },
    Box {
        length: Param<f32>,
        width: Param<f32>,
        height: Param<f32>,
        length_segs: Param<u32>,
        width_segs: Param<u32>,
        height_segs: Param<u32>,
        /// Generate Mapping Coords.: channel 1 from the object's own projection.
        #[serde(default = "yes")]
        gen_mapping: Param<bool>,
    },
    Cone {
        radius1: Param<f32>,
        radius2: Param<f32>,
        height: Param<f32>,
        height_segs: Param<u32>,
        cap_segs: Param<u32>,
        sides: Param<u32>,
        slice_on: Param<bool>,
        slice_from: Param<f32>,
        slice_to: Param<f32>,
        /// Smooth: blends the faces into a smooth surface (off: faceted).
        #[serde(default = "yes")]
        smooth: Param<bool>,
        /// Generate Mapping Coords.: channel 1 from the object's own projection.
        #[serde(default = "yes")]
        gen_mapping: Param<bool>,
    },
    Sphere {
        radius: Param<f32>,
        segments: Param<u32>,
        hemisphere: Param<f32>,
        /// Hemisphere cut: 0 = Chop (removes the cut part), 1 = Squash (keeps every vertex and face).
        #[serde(default = "zero")]
        chop_squash: Param<u32>,
        #[serde(default = "no")]
        slice_on: Param<bool>,
        #[serde(default = "zero_f")]
        slice_from: Param<f32>,
        #[serde(default = "zero_f")]
        slice_to: Param<f32>,
        /// Base To Pivot: moves the sphere up its local Z so the pivot is at its base.
        #[serde(default = "no")]
        base_to_pivot: Param<bool>,
        /// Smooth: blends the faces into a smooth surface (off: faceted).
        #[serde(default = "yes")]
        smooth: Param<bool>,
        /// Generate Mapping Coords.: channel 1 from the object's own projection.
        #[serde(default = "yes")]
        gen_mapping: Param<bool>,
    },
    GeoSphere {
        radius: Param<f32>,
        segments: Param<u32>,
        /// 0 = Tetra, 1 = Octa, 2 = Icosa (Geodesic Base Type)
        base_type: Param<u32>,
        /// Base To Pivot: the pivot at the bottom of the geosphere (off: at its centre).
        #[serde(default = "no")]
        base_to_pivot: Param<bool>,
        /// Smooth: blends the faces into a smooth surface (off: faceted).
        #[serde(default = "yes")]
        smooth: Param<bool>,
        /// Generate Mapping Coords.: channel 1 from the object's own projection.
        #[serde(default = "yes")]
        gen_mapping: Param<bool>,
    },
    Cylinder {
        radius: Param<f32>,
        height: Param<f32>,
        height_segs: Param<u32>,
        cap_segs: Param<u32>,
        sides: Param<u32>,
        slice_on: Param<bool>,
        slice_from: Param<f32>,
        slice_to: Param<f32>,
        /// Smooth: blends the faces into a smooth surface (off: faceted).
        #[serde(default = "yes")]
        smooth: Param<bool>,
        /// Generate Mapping Coords.: channel 1 from the object's own projection.
        #[serde(default = "yes")]
        gen_mapping: Param<bool>,
    },
    Tube {
        radius1: Param<f32>,
        radius2: Param<f32>,
        height: Param<f32>,
        height_segs: Param<u32>,
        cap_segs: Param<u32>,
        sides: Param<u32>,
        slice_on: Param<bool>,
        slice_from: Param<f32>,
        slice_to: Param<f32>,
        /// Smooth: blends the faces into a smooth surface (off: faceted).
        #[serde(default = "yes")]
        smooth: Param<bool>,
        /// Generate Mapping Coords.: channel 1 from the object's own projection.
        #[serde(default = "yes")]
        gen_mapping: Param<bool>,
    },
    Torus {
        radius1: Param<f32>,
        radius2: Param<f32>,
        rotation: Param<f32>,
        twist: Param<f32>,
        segments: Param<u32>,
        sides: Param<u32>,
        slice_on: Param<bool>,
        slice_from: Param<f32>,
        slice_to: Param<f32>,
        /// Smooth: 0 All, 1 Sides, 2 None, 3 Segments.
        #[serde(default = "zero")]
        smooth: Param<u32>,
        /// Generate Mapping Coords.: channel 1 from the object's own projection.
        #[serde(default = "yes")]
        gen_mapping: Param<bool>,
    },
    Pyramid {
        width: Param<f32>,
        depth: Param<f32>,
        height: Param<f32>,
        width_segs: Param<u32>,
        depth_segs: Param<u32>,
        height_segs: Param<u32>,
        /// Generate Mapping Coords.: channel 1 from the object's own projection.
        #[serde(default = "yes")]
        gen_mapping: Param<bool>,
    },
    Teapot {
        radius: Param<f32>,
        segments: Param<u32>,
        body: Param<bool>,
        handle: Param<bool>,
        spout: Param<bool>,
        lid: Param<bool>,
        /// Smooth: blends the faces into a smooth surface (off: faceted).
        #[serde(default = "yes")]
        smooth: Param<bool>,
        /// Generate Mapping Coords.: channel 1 from the object's own projection.
        #[serde(default = "yes")]
        gen_mapping: Param<bool>,
    },
    Plane {
        length: Param<f32>,
        width: Param<f32>,
        length_segs: Param<u32>,
        width_segs: Param<u32>,
        /// Generate Mapping Coords.: channel 1 from the object's own projection.
        #[serde(default = "yes")]
        gen_mapping: Param<bool>,
    },
    /// A camera object (Create > Cameras), looking down its local -Z.
    Camera {
        /// Horizontal field of view in degrees (Lens follows it, 36 mm film).
        fov: Param<f32>,
        /// Target camera: the target node it looks at (Free camera: None).
        target: Option<NodeId>,
        /// FOV Direction the FOV spinner shows: 0 horizontal, 1 vertical, 2 diagonal.
        #[serde(default = "zero")]
        fov_type: Param<u32>,
        #[serde(default = "no")]
        ortho: Param<bool>,
        #[serde(default = "no")]
        show_cone: Param<bool>,
        #[serde(default = "no")]
        show_horizon: Param<bool>,
        /// Environment Ranges > Show.
        #[serde(default = "no")]
        show_ranges: Param<bool>,
        #[serde(default = "zero_f")]
        near_range: Param<f32>,
        #[serde(default = "thousand")]
        far_range: Param<f32>,
        #[serde(default = "no")]
        clip_manually: Param<bool>,
        #[serde(default = "one_f")]
        near_clip: Param<f32>,
        #[serde(default = "thousand")]
        far_clip: Param<f32>,
        /// Free camera: distance to its invisible target.
        #[serde(default = "cam_target_distance")]
        target_distance: Param<f32>,
    },
    /// A group head (Group menu): a dummy whose children are the members.
    Group {
        open: bool,
    },
    /// Systems > Bones: pivot at the base, local +X along the bone.
    Bone {
        length: Param<f32>,
        width: Param<f32>,
        height: Param<f32>,
        taper: Param<f32>,
        fins: BoneFins,
        /// Bone Tools > Object Properties.
        #[serde(default = "yes")]
        bone_on: Param<bool>,
        #[serde(default = "no")]
        freeze_length: Param<bool>,
        /// Stretch: 0 None, 1 Scale, 2 Squash.
        #[serde(default = "zero")]
        stretch: Param<u32>,
        /// Stretch axis: 0 X, 1 Y, 2 Z.
        #[serde(default = "zero")]
        stretch_axis: Param<u32>,
        /// The local axis the bone points along: 0 +X (native bones), 1 +Y (FBX / Mixamo
        /// skeletons), 2 +Z, 3 −X, 4 −Y, 5 −Z.
        #[serde(default = "zero")]
        axis: Param<u32>,
    },
    /// Helpers > Dummy: a wireframe cube.
    Dummy {
        size: Param<f32>,
    },
    /// Helpers > Point.
    Point {
        size: Param<f32>,
        axis_tripod: Param<bool>,
        cross: Param<bool>,
        box_: Param<bool>,
    },
    /// The goal of an IK chain (Hierarchy > IK), drawn like a Point.
    IkGoal {
        chain: u32,
    },
    /// Systems > Biped: the centre of mass, root of the biped's parts; holds the Structure.
    Biped(Box<crate::biped::BipedStructure>),
    /// Create > Lights > Standard: a light proxy the renderers build their lights from.
    Light(Box<crate::light::LightObject>),
}

/// Bone fins (Bone Tools > Fin Adjustment Tools).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BoneFins {
    pub side: Param<bool>,
    pub side_size: Param<f32>,
    pub side_start_taper: Param<f32>,
    pub side_end_taper: Param<f32>,
    pub front: Param<bool>,
    pub front_size: Param<f32>,
    pub front_start_taper: Param<f32>,
    pub front_end_taper: Param<f32>,
    pub back: Param<bool>,
    pub back_size: Param<f32>,
    pub back_start_taper: Param<f32>,
    pub back_end_taper: Param<f32>,
}

impl Default for BoneFins {
    fn default() -> Self {
        Self {
            side: p(false), side_size: p(2.0), side_start_taper: p(0.1), side_end_taper: p(0.1),
            front: p(false), front_size: p(2.0), front_start_taper: p(0.1), front_end_taper: p(0.1),
            back: p(false), back_size: p(2.0), back_start_taper: p(0.1), back_end_taper: p(0.1),
        }
    }
}

/// 0 +X, 1 +Y, 2 +Z, 3 −X, 4 −Y, 5 −Z.
pub fn bone_axis_dir(axis: u32) -> Vec3 {
    match axis {
        1 => Vec3::Y,
        2 => Vec3::Z,
        3 => Vec3::NEG_X,
        4 => Vec3::NEG_Y,
        5 => Vec3::NEG_Z,
        _ => Vec3::X,
    }
}

/// The axis code nearest to a local direction.
pub fn bone_axis_of(dir: Vec3) -> u32 {
    let a = dir.abs();
    if a.x >= a.y && a.x >= a.z {
        if dir.x >= 0.0 { 0 } else { 3 }
    } else if a.y >= a.z {
        if dir.y >= 0.0 { 1 } else { 4 }
    } else if dir.z >= 0.0 {
        2
    } else {
        5
    }
}

fn no() -> Param<bool> {
    p(false)
}
fn zero() -> Param<u32> {
    p(0)
}
fn zero_f() -> Param<f32> {
    p(0.0)
}
fn one_f() -> Param<f32> {
    p(1.0)
}
fn thousand() -> Param<f32> {
    p(1000.0)
}
fn cam_target_distance() -> Param<f32> {
    p(crate::camera::DEFAULT_TARGET_DISTANCE)
}

fn p<T: crate::param::Animatable>(v: T) -> Param<T> {
    Param::new(v)
}

/// Memo of a compound object's result keyed by its parameters (operands
/// only change by replacing the object, so the key needs the spinners only).
#[derive(Debug, Default)]
pub struct CompoundCache(std::cell::RefCell<Option<(u64, Result<Mesh, String>)>>);

impl Clone for CompoundCache {
    fn clone(&self) -> Self {
        CompoundCache(std::cell::RefCell::new(self.0.borrow().clone()))
    }
}

impl PartialEq for CompoundCache {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl CompoundCache {
    fn get_or(&self, key: u64, build: impl FnOnce() -> Result<Mesh, String>) -> Result<Mesh, String> {
        if let Some((k, r)) = &*self.0.borrow() {
            if *k == key {
                return r.clone();
            }
        }
        let r = build();
        *self.0.borrow_mut() = Some((key, r.clone()));
        r
    }
}

fn param_key(kind: &ObjectKind) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    format!("{:?}", kind.parameters()).hash(&mut h);
    h.finish()
}

/// Boolean > Operation index to the CSG operation.
pub fn csg_op(i: u32) -> loom_mesh::csg::CsgOp {
    use loom_mesh::csg::CsgOp::*;
    match i { 0 => Union, 1 => Intersection, 2 => Subtraction, 3 => SubtractionBA, 4 => CutRefine, 5 => CutSplit, 6 => CutRemoveInside, _ => CutRemoveOutside }
}

/// ProBoolean operations: 0 Union, 1 Intersection, 2 Subtraction, 3 Merge
/// (both surfaces cut by each other), 4 Attach, 5 Insert (the operand
/// replaces the base's interior: union).
pub const PROBOOLEAN_OPS: [&str; 6] = ["Union", "Intersection", "Subtraction", "Merge", "Attach", "Insert"];

pub fn probool_step(base: &Mesh, op: u32, operand: &Mesh) -> Result<Mesh, String> {
    use loom_mesh::csg::{boolean, CsgOp};
    match op {
        0 | 5 => boolean(base, operand, CsgOp::Union),
        1 => boolean(base, operand, CsgOp::Intersection),
        2 => boolean(base, operand, CsgOp::Subtraction),
        3 => {
            let mut a = boolean(base, operand, CsgOp::CutRefine)?;
            let b = boolean(operand, base, CsgOp::CutRefine)?;
            loom_mesh::geom_ops::attach(&mut a, &b, Mat4::IDENTITY)?;
            Ok(a)
        }
        _ => {
            let mut a = base.clone();
            loom_mesh::geom_ops::attach(&mut a, operand, Mat4::IDENTITY)?;
            Ok(a)
        }
    }
}

fn loft_params(kind: &ObjectKind) -> loom_shape::loft::LoftParams {
    match kind {
        ObjectKind::Loft { shape_steps, path_steps, cap_start, cap_end, contour, flip_normals, .. } => loom_shape::loft::LoftParams {
            cap_start: cap_start.get(),
            cap_end: cap_end.get(),
            shape_steps: shape_steps.get(),
            path_steps: path_steps.get(),
            contour: contour.get(),
            flip_normals: flip_normals.get(),
        },
        _ => loom_shape::loft::LoftParams::default(),
    }
}

/// Channel 1 from the primitive's own projection when Generate Mapping Coords. is on.
fn mapped(mut m: Mesh, on: bool, kind: AutoMap) -> Mesh {
    if on {
        loom_mesh::uvmap::auto_map(&mut m, kind, 1);
    }
    m
}

fn yes() -> Param<bool> {
    p(true)
}


/// Moves every vertex up local Z (Base To Pivot).
fn lift(m: &mut Mesh, dz: f32) {
    for v in m.vertices_mut() {
        v.position.z += dz;
    }
}

/// Smooth off: every face loses its smoothing groups.
fn smoothed(mut m: Mesh, on: bool) -> Mesh {
    if !on {
        for f in m.faces_mut() {
            f.smoothing_groups = 0;
        }
    }
    m
}

fn slice(on: bool, from: f32, to: f32) -> loom_mesh::Slice {
    on.then_some((from, to))
}

impl ObjectKind {
    pub fn box_object(length: f32, width: f32, height: f32) -> Self {
        Self::Box { length: p(length), width: p(width), height: p(height), length_segs: p(1), width_segs: p(1), height_segs: p(1), gen_mapping: p(true) }
    }
    pub fn cone_object(radius1: f32, radius2: f32, height: f32) -> Self {
        Self::Cone { radius1: p(radius1), radius2: p(radius2), height: p(height), height_segs: p(5), cap_segs: p(1), sides: p(24), slice_on: p(false), slice_from: p(0.0), slice_to: p(0.0), smooth: p(true), gen_mapping: p(true) }
    }
    pub fn sphere_object(radius: f32) -> Self {
        Self::Sphere { radius: p(radius), segments: p(32), hemisphere: p(0.0), chop_squash: p(0), slice_on: p(false), slice_from: p(0.0), slice_to: p(0.0), base_to_pivot: p(false), smooth: p(true), gen_mapping: p(true) }
    }
    pub fn geosphere_object(radius: f32) -> Self {
        Self::GeoSphere { radius: p(radius), segments: p(4), base_type: p(2), base_to_pivot: p(false), smooth: p(true), gen_mapping: p(true) }
    }
    pub fn cylinder_object(radius: f32, height: f32) -> Self {
        Self::Cylinder { radius: p(radius), height: p(height), height_segs: p(5), cap_segs: p(1), sides: p(18), slice_on: p(false), slice_from: p(0.0), slice_to: p(0.0), smooth: p(true), gen_mapping: p(true) }
    }
    pub fn tube_object(radius1: f32, radius2: f32, height: f32) -> Self {
        Self::Tube { radius1: p(radius1), radius2: p(radius2), height: p(height), height_segs: p(5), cap_segs: p(1), sides: p(18), slice_on: p(false), slice_from: p(0.0), slice_to: p(0.0), smooth: p(true), gen_mapping: p(true) }
    }
    pub fn torus_object(radius1: f32, radius2: f32) -> Self {
        Self::Torus { radius1: p(radius1), radius2: p(radius2), rotation: p(0.0), twist: p(0.0), segments: p(24), sides: p(12), slice_on: p(false), slice_from: p(0.0), slice_to: p(0.0), smooth: p(0), gen_mapping: p(true) }
    }
    pub fn pyramid_object(width: f32, depth: f32, height: f32) -> Self {
        Self::Pyramid { width: p(width), depth: p(depth), height: p(height), width_segs: p(1), depth_segs: p(1), height_segs: p(1), gen_mapping: p(true) }
    }
    pub fn teapot_object(radius: f32) -> Self {
        Self::Teapot { radius: p(radius), segments: p(4), body: p(true), handle: p(true), spout: p(true), lid: p(true), smooth: p(true), gen_mapping: p(true) }
    }
    pub fn plane_object(length: f32, width: f32) -> Self {
        Self::Plane { length: p(length), width: p(width), length_segs: p(4), width_segs: p(4), gen_mapping: p(true) }
    }
    pub fn camera_object(fov_deg: f32) -> Self {
        Self::Camera {
            fov: p(fov_deg),
            target: None,
            fov_type: zero(),
            ortho: no(),
            show_cone: no(),
            show_horizon: no(),
            show_ranges: no(),
            near_range: zero_f(),
            far_range: thousand(),
            clip_manually: no(),
            near_clip: one_f(),
            far_clip: thousand(),
            target_distance: cam_target_distance(),
        }
    }
    /// A target camera aiming at `target`.
    pub fn target_camera_object(fov_deg: f32, target: NodeId) -> Self {
        let mut o = Self::camera_object(fov_deg);
        if let Self::Camera { target: t, .. } = &mut o {
            *t = Some(target);
        }
        o
    }
    /// A standard light of type `t` (free; a target light gets its target set by the caller).
    pub fn light_object(t: crate::light::LightType) -> Self {
        Self::Light(Box::new(crate::light::LightObject::new(t)))
    }
    pub fn light(&self) -> Option<&crate::light::LightObject> {
        match self {
            Self::Light(l) => Some(l),
            _ => None,
        }
    }
    /// A bone of `length` with the default size (width 4, height 4, taper 90 %).
    pub fn bone(length: f32) -> Self {
        Self::Bone { length: p(length), width: p(4.0), height: p(4.0), taper: p(0.9), fins: BoneFins::default(), bone_on: p(true), freeze_length: p(false), stretch: p(0), stretch_axis: p(0), axis: p(0) }
    }
    pub fn dummy(size: f32) -> Self {
        Self::Dummy { size: p(size) }
    }
    pub fn point(size: f32) -> Self {
        Self::Point { size: p(size), axis_tripod: p(true), cross: p(true), box_: p(false) }
    }
    /// Helpers and other non-geometry nodes (Dummy, Point, IK goal, Group, Camera).
    pub fn is_helper(&self) -> bool {
        matches!(self, Self::Dummy { .. } | Self::Point { .. } | Self::IkGoal { .. } | Self::Group { .. } | Self::Camera { .. } | Self::Biped(_))
    }
    pub fn is_bone(&self) -> bool {
        matches!(self, Self::Bone { .. })
    }
    /// The bone's length (the tip sits at `bone_tip_local`).
    pub fn bone_length(&self) -> Option<f32> {
        match self {
            Self::Bone { length, .. } => Some(length.get()),
            _ => None,
        }
    }
    /// The unit direction a bone points along in its own frame.
    pub fn bone_axis(&self) -> Option<Vec3> {
        match self {
            Self::Bone { axis, .. } => Some(bone_axis_dir(axis.get())),
            _ => None,
        }
    }
    /// The bone's tip in its own frame (its child joint).
    pub fn bone_tip_local(&self) -> Option<Vec3> {
        Some(self.bone_axis()? * self.bone_length()?)
    }
    pub fn rectangle_object(length: f32, width: f32) -> Self {
        Self::Rectangle { length: p(length), width: p(width), corner_radius: p(0.0), common: ShapeCommon::default() }
    }
    pub fn circle_object(radius: f32) -> Self {
        Self::Circle { radius: p(radius), common: ShapeCommon::default() }
    }
    pub fn ellipse_object(length: f32, width: f32) -> Self {
        Self::Ellipse { length: p(length), width: p(width), common: ShapeCommon::default() }
    }
    pub fn arc_object(radius: f32) -> Self {
        Self::Arc { radius: p(radius), from: p(0.0), to: p(90.0), pie_slice: p(false), reverse: p(false), common: ShapeCommon::default() }
    }
    pub fn donut_object(radius1: f32, radius2: f32) -> Self {
        Self::Donut { radius1: p(radius1), radius2: p(radius2), common: ShapeCommon::default() }
    }
    pub fn ngon_object(radius: f32) -> Self {
        Self::NGon { radius: p(radius), inscribed: p(false), sides: p(6), corner_radius: p(0.0), circular: p(false), common: ShapeCommon::default() }
    }
    pub fn star_object(radius1: f32, radius2: f32) -> Self {
        Self::Star { radius1: p(radius1), radius2: p(radius2), points: p(6), distortion: p(0.0), fillet1: p(0.0), fillet2: p(0.0), common: ShapeCommon::default() }
    }
    pub fn helix_object(radius1: f32, radius2: f32, height: f32) -> Self {
        Self::Helix { radius1: p(radius1), radius2: p(radius2), height: p(height), turns: p(1.0), bias: p(0.0), cw: p(false), common: ShapeCommon::default() }
    }
    pub fn boolean_object(a: Mesh, b: Mesh, op: u32) -> Self {
        Self::Boolean { a, b, op: p(op.min(7)), display: p(0), cache: CompoundCache::default() }
    }
    pub fn proboolean_object(base: Mesh) -> Self {
        Self::ProBoolean { base, operands: Vec::new(), cache: CompoundCache::default() }
    }
    pub fn loft_object(path: loom_shape::Shape, shapes: Vec<(f32, loom_shape::Shape)>) -> Self {
        Self::Loft { path, shapes, shape_steps: p(5), path_steps: p(5), cap_start: p(true), cap_end: p(true), contour: p(true), flip_normals: p(false), cache: CompoundCache::default() }
    }
    pub fn quad_patch_object(length: f32, width: f32) -> Self {
        Self::QuadPatch { length: p(length), width: p(width), length_segs: p(1), width_segs: p(1) }
    }
    pub fn tri_patch_object(length: f32, width: f32) -> Self {
        Self::TriPatch { length: p(length), width: p(width) }
    }

    /// Steps the Patch Grids tessellate with.
    pub const PATCH_GRID_STEPS: u32 = 5;

    /// The patch lattice of a patch object (Patch Grids build theirs).
    pub fn patch_mesh(&self) -> Option<loom_mesh::patch::PatchMesh> {
        use loom_mesh::patch::PatchMesh;
        match self {
            Self::EditablePatch(p) => Some(p.patch.clone()),
            Self::QuadPatch { length, width, length_segs, width_segs } => Some(PatchMesh::quad_grid(length.get(), width.get(), length_segs.get(), width_segs.get())),
            Self::TriPatch { length, width } => Some(PatchMesh::tri_grid(length.get(), width.get())),
            _ => None,
        }
    }

    pub fn patch(&self) -> Option<&PatchObject> {
        match self {
            Self::EditablePatch(p) => Some(p),
            _ => None,
        }
    }

    pub fn patch_mut(&mut self) -> Option<&mut PatchObject> {
        match self {
            Self::EditablePatch(p) => Some(p),
            _ => None,
        }
    }

    /// This object as an Editable Patch with `patch` (a patch keeps its settings).
    pub fn with_patch(&self, patch: loom_mesh::patch::PatchMesh) -> ObjectKind {
        match self {
            Self::EditablePatch(p) => Self::EditablePatch(p.with_patch(patch)),
            _ => Self::EditablePatch(PatchObject::from_patch(patch)),
        }
    }

    /// The Editable Mesh data (the triangle mesh and its rollout settings).
    pub fn tri_mesh(&self) -> Option<&PolyObject> {
        match self {
            Self::EditableMesh(p) => Some(p),
            _ => None,
        }
    }

    pub fn line_object(shape: loom_shape::Shape) -> Self {
        Self::EditableSpline(ShapeObject { shape, soft: SoftSelection::default(), is_line: true, hidden: Default::default() })
    }

    /// The shape of a spline object (parametric ones are built from their parameters).
    pub fn shape(&self) -> Option<loom_shape::Shape> {
        use loom_shape::parametric as sp;
        use loom_shape::Shape;
        Some(match self {
            Self::EditableSpline(s) => s.shape.clone(),
            Self::Rectangle { length, width, corner_radius, common } => common.apply(Shape::from_splines(vec![sp::rectangle(length.get(), width.get(), corner_radius.get())])),
            Self::Circle { radius, common } => common.apply(Shape::from_splines(vec![sp::circle(radius.get())])),
            Self::Ellipse { length, width, common } => common.apply(Shape::from_splines(vec![sp::ellipse(length.get(), width.get())])),
            Self::Arc { radius, from, to, pie_slice, reverse, common } => common.apply(Shape::from_splines(vec![sp::arc(radius.get(), from.get(), to.get(), pie_slice.get(), reverse.get())])),
            Self::Donut { radius1, radius2, common } => common.apply(Shape::from_splines(sp::donut(radius1.get(), radius2.get()))),
            Self::NGon { radius, inscribed, sides, corner_radius, circular, common } => common.apply(Shape::from_splines(vec![sp::ngon(radius.get(), inscribed.get(), sides.get(), corner_radius.get(), circular.get())])),
            Self::Star { radius1, radius2, points, distortion, fillet1, fillet2, common } => common.apply(Shape::from_splines(vec![sp::star(radius1.get(), radius2.get(), points.get(), distortion.get(), fillet1.get(), fillet2.get())])),
            Self::Helix { radius1, radius2, height, turns, bias, cw, common } => common.apply(Shape::from_splines(vec![sp::helix(radius1.get(), radius2.get(), height.get(), turns.get(), bias.get(), cw.get())])),
            _ => return None,
        })
    }

    pub fn is_shape(&self) -> bool {
        matches!(self, Self::EditableSpline(_) | Self::Rectangle { .. } | Self::Circle { .. } | Self::Ellipse { .. } | Self::Arc { .. } | Self::Donut { .. } | Self::NGon { .. } | Self::Star { .. } | Self::Helix { .. })
    }

    pub fn spline(&self) -> Option<&ShapeObject> {
        match self {
            Self::EditableSpline(s) => Some(s),
            _ => None,
        }
    }

    pub fn spline_mut(&mut self) -> Option<&mut ShapeObject> {
        match self {
            Self::EditableSpline(s) => Some(s),
            _ => None,
        }
    }

    /// This object as an Editable Spline carrying `shape` (a spline keeps its settings).
    pub fn with_shape(&self, shape: loom_shape::Shape) -> ObjectKind {
        match self {
            Self::EditableSpline(s) => Self::EditableSpline(ShapeObject { shape, soft: s.soft.clone(), is_line: s.is_line, hidden: s.hidden.clone() }),
            _ => Self::EditableSpline(ShapeObject::from_shape(shape)),
        }
    }

    /// Rendering / Interpolation settings of a shape.
    fn shape_common(&self) -> Option<ShapeCommon> {
        match self {
            Self::EditableSpline(s) => Some(ShapeCommon { steps: p(s.shape.steps), optimize: p(s.shape.optimize), adaptive: p(s.shape.adaptive), render: s.shape.render.clone() }),
            Self::Rectangle { common, .. } | Self::Circle { common, .. } | Self::Ellipse { common, .. } | Self::Arc { common, .. } | Self::Donut { common, .. } | Self::NGon { common, .. } | Self::Star { common, .. } | Self::Helix { common, .. } => Some(common.clone()),
            _ => None,
        }
    }

    fn set_shape_common(&mut self, name: &str, value: f64) -> Option<Result<(), String>> {
        let v = value as f32;
        let b = value != 0.0;
        let u = value.round().max(0.0) as u32;
        let mut c = self.shape_common()?;
        let r = &mut c.render;
        match name {
            "Steps" => c.steps.set(u.min(100)),
            "Optimize" => c.optimize.set(b),
            "Adaptive" => c.adaptive.set(b),
            "Enable In Renderer" => r.enable_in_renderer = b,
            "Enable In Viewport" => r.enable_in_viewport = b,
            "Use Viewport Settings" => r.use_viewport_settings = b,
            "Generate Mapping Coords" => r.generate_mapping = b,
            "Rectangular" => r.rectangular = b,
            "Thickness" => r.thickness = v.max(0.0),
            "Sides" => r.sides = u.clamp(3, 100),
            "Angle" => r.angle = v,
            "Rect Length" => r.length = v.max(0.0),
            "Rect Width" => r.width = v.max(0.0),
            "Auto Smooth" => r.auto_smooth = b,
            _ => return None,
        }
        match self {
            Self::EditableSpline(s) => {
                s.shape.steps = c.steps.get();
                s.shape.optimize = c.optimize.get();
                s.shape.adaptive = c.adaptive.get();
                s.shape.render = c.render;
            }
            Self::Rectangle { common, .. } | Self::Circle { common, .. } | Self::Ellipse { common, .. } | Self::Arc { common, .. } | Self::Donut { common, .. } | Self::NGon { common, .. } | Self::Star { common, .. } | Self::Helix { common, .. } => *common = c,
            _ => {}
        }
        Some(Ok(()))
    }

    fn shape_common_params(&self) -> Vec<(&'static str, ParamValue)> {
        use ParamValue::*;
        let Some(c) = self.shape_common() else { return Vec::new() };
        let r = &c.render;
        vec![
            ("Steps", Int(c.steps.get())), ("Optimize", Bool(c.optimize.get())), ("Adaptive", Bool(c.adaptive.get())),
            ("Enable In Renderer", Bool(r.enable_in_renderer)), ("Enable In Viewport", Bool(r.enable_in_viewport)), ("Use Viewport Settings", Bool(r.use_viewport_settings)),
            ("Generate Mapping Coords", Bool(r.generate_mapping)), ("Rectangular", Bool(r.rectangular)), ("Thickness", Float(r.thickness)), ("Sides", Int(r.sides)), ("Angle", Float(r.angle)),
            ("Rect Length", Float(r.length)), ("Rect Width", Float(r.width)), ("Auto Smooth", Bool(r.auto_smooth)),
        ]
    }

    /// Base name for the Create panel object type ("Box", "Sphere", ...).
    pub fn base_name(&self) -> &'static str {
        match self {
            Self::EditablePoly(_) | Self::EditableMesh(_) | Self::EditablePatch(_) => "Object",
            Self::QuadPatch { .. } => "QuadPatch",
            Self::TriPatch { .. } => "TriPatch",
            Self::Box { .. } => "Box",
            Self::Cone { .. } => "Cone",
            Self::Sphere { .. } => "Sphere",
            Self::GeoSphere { .. } => "GeoSphere",
            Self::Cylinder { .. } => "Cylinder",
            Self::Tube { .. } => "Tube",
            Self::Torus { .. } => "Torus",
            Self::Pyramid { .. } => "Pyramid",
            Self::Teapot { .. } => "Teapot",
            Self::Plane { .. } => "Plane",
            Self::Camera { .. } => "Camera",
            Self::Group { .. } => "Group",
            Self::Bone { .. } => "Bone",
            Self::Dummy { .. } => "Dummy",
            Self::Point { .. } => "Point",
            Self::IkGoal { .. } => "IK Chain",
            Self::Biped(_) => "Biped",
            Self::Light(l) => l.base_name(),
            Self::EditableSpline(s) => if s.is_line { "Line" } else { "Shape" },
            Self::Rectangle { .. } => "Rectangle",
            Self::Circle { .. } => "Circle",
            Self::Ellipse { .. } => "Ellipse",
            Self::Arc { .. } => "Arc",
            Self::Donut { .. } => "Donut",
            Self::NGon { .. } => "NGon",
            Self::Star { .. } => "Star",
            Self::Helix { .. } => "Helix",
            Self::Boolean { .. } => "Boolean",
            Self::ProBoolean { .. } => "ProBoolean",
            Self::Loft { .. } => "Loft",
        }
    }

    /// Compound objects whose result could not be built: the message the
    /// rollout shows (the viewport then shows operand A).
    pub fn compound_error(&self) -> Option<String> {
        match self {
            Self::Boolean { display, .. } if display.get() == 0 => self.compound_result().err(),
            Self::ProBoolean { .. } | Self::Loft { .. } => self.compound_result().err(),
            _ => None,
        }
    }

    /// The memoised result of a compound object.
    fn compound_result(&self) -> Result<Mesh, String> {
        let key = param_key(self);
        match self {
            Self::Boolean { a, b, op, cache, .. } => cache.get_or(key, || loom_mesh::csg::boolean(a, b, csg_op(op.get()))),
            Self::ProBoolean { base, operands, cache } => cache.get_or(key.wrapping_add(operands.len() as u64), || {
                let mut cur = base.clone();
                for (op, m) in operands {
                    cur = probool_step(&cur, *op, m).map_err(|e| format!("operand: {e}"))?;
                }
                Ok(cur)
            }),
            Self::Loft { path, shapes, cache, .. } => cache.get_or(key.wrapping_add(shapes.len() as u64 * 7 + path.knot_count() as u64), || loom_shape::loft::loft(path, shapes, loft_params(self))),
            _ => Err("not a compound object".into()),
        }
    }

    pub fn is_compound(&self) -> bool {
        matches!(self, Self::Boolean { .. } | Self::ProBoolean { .. } | Self::Loft { .. })
    }

    /// Choice labels for enumerated Int parameters (radios in the rollout).
    pub fn choices(&self, name: &str) -> Option<&'static [&'static str]> {
        match (self, name) {
            (Self::Boolean { .. }, "Operation") => Some(&["Union", "Intersection", "Subtraction (A-B)", "Subtraction (B-A)", "Cut Refine", "Cut Split", "Cut Remove Inside", "Cut Remove Outside"]),
            (Self::Boolean { .. }, "Display") => Some(&["Result", "Operands"]),
            (Self::GeoSphere { .. }, "Geodesic Base Type") => Some(&["Tetra", "Octa", "Icosa"]),
            (Self::Torus { .. }, "Smooth") => Some(&["All", "Sides", "None", "Segments"]),
            (Self::Sphere { .. }, "Chop/Squash") => Some(&["Chop", "Squash"]),
            (Self::Camera { .. }, "FOV Direction") => Some(&crate::camera::FOV_DIRECTIONS),
            (Self::Light(l), name) => l.choices(name),
            _ => None,
        }
    }

    /// The base object's output into the modifier stack: the NURMS result
    /// for a subdivided poly, else the plain mesh.
    pub fn display_output(&self) -> Mesh {
        match self {
            Self::EditablePoly(p) => p.display_mesh().into_owned(),
            other => other.mesh().into_owned(),
        }
    }

    /// Name shown in the Modify panel's stack for the base object.
    pub fn stack_name(&self) -> &'static str {
        match self {
            Self::EditablePoly(_) => "Editable Poly",
            Self::EditableMesh(_) => "Editable Mesh",
            Self::EditablePatch(_) => "Editable Patch",
            Self::EditableSpline(s) => if s.is_line { "Line" } else { "Editable Spline" },
            Self::Light(l) => l.stack_name(),
            other => other.base_name(),
        }
    }

    /// The rollout parameters, in rollout order, with rollout names.
    pub fn parameters(&self) -> Vec<(&'static str, ParamValue)> {
        use ParamValue::*;
        let f = |x: &Param<f32>| Float(x.get());
        let i = |x: &Param<u32>| Int(x.get());
        let b = |x: &Param<bool>| Bool(x.get());
        match self {
            Self::EditablePoly(p) => vec![("Iterations", i(&p.subdiv.iterations)), ("Smoothness", f(&p.subdiv.smoothness))],
            Self::EditableMesh(_) => Vec::new(),
            Self::EditablePatch(p) => vec![("View Steps", i(&p.steps)), ("Show Interior Edges", Bool(p.show_interior))],
            Self::QuadPatch { length, width, length_segs, width_segs } => vec![("Length", f(length)), ("Width", f(width)), ("Length Segs", i(length_segs)), ("Width Segs", i(width_segs))],
            Self::TriPatch { length, width } => vec![("Length", f(length)), ("Width", f(width))],
            Self::Box { length, width, height, length_segs, width_segs, height_segs, gen_mapping } => vec![
                ("Length", f(length)), ("Width", f(width)), ("Height", f(height)),
                ("Length Segs", i(length_segs)), ("Width Segs", i(width_segs)), ("Height Segs", i(height_segs)),
            ("Generate Mapping Coords.", b(gen_mapping)),
            ],
            Self::Cone { radius1, radius2, height, height_segs, cap_segs, sides, slice_on, slice_from, slice_to, smooth, gen_mapping } => vec![
                ("Radius 1", f(radius1)), ("Radius 2", f(radius2)), ("Height", f(height)), ("Height Segments", i(height_segs)),
                ("Cap Segments", i(cap_segs)), ("Sides", i(sides)), ("Smooth", b(smooth)), ("Slice On", b(slice_on)), ("Slice From", f(slice_from)), ("Slice To", f(slice_to)),
            ("Generate Mapping Coords.", b(gen_mapping)),
            ],
            Self::Sphere { radius, segments, hemisphere, chop_squash, slice_on, slice_from, slice_to, base_to_pivot, smooth, gen_mapping } => vec![
                ("Radius", f(radius)), ("Segments", i(segments)), ("Smooth", b(smooth)), ("Hemisphere", f(hemisphere)), ("Chop/Squash", i(chop_squash)),
                ("Slice On", b(slice_on)), ("Slice From", f(slice_from)), ("Slice To", f(slice_to)), ("Base To Pivot", b(base_to_pivot)),
                ("Generate Mapping Coords.", b(gen_mapping)),
            ],
            Self::GeoSphere { radius, segments, base_type, base_to_pivot, smooth, gen_mapping } => vec![
                ("Radius", f(radius)), ("Segments", i(segments)), ("Geodesic Base Type", i(base_type)), ("Smooth", b(smooth)),
                ("Base To Pivot", b(base_to_pivot)), ("Generate Mapping Coords.", b(gen_mapping)),
            ],
            Self::Cylinder { radius, height, height_segs, cap_segs, sides, slice_on, slice_from, slice_to, smooth, gen_mapping } => vec![
                ("Radius", f(radius)), ("Height", f(height)), ("Height Segments", i(height_segs)), ("Cap Segments", i(cap_segs)),
                ("Sides", i(sides)), ("Smooth", b(smooth)), ("Slice On", b(slice_on)), ("Slice From", f(slice_from)), ("Slice To", f(slice_to)),
            ("Generate Mapping Coords.", b(gen_mapping)),
            ],
            Self::Tube { radius1, radius2, height, height_segs, cap_segs, sides, slice_on, slice_from, slice_to, smooth, gen_mapping } => vec![
                ("Radius 1", f(radius1)), ("Radius 2", f(radius2)), ("Height", f(height)), ("Height Segments", i(height_segs)),
                ("Cap Segments", i(cap_segs)), ("Sides", i(sides)), ("Smooth", b(smooth)), ("Slice On", b(slice_on)), ("Slice From", f(slice_from)), ("Slice To", f(slice_to)),
            ("Generate Mapping Coords.", b(gen_mapping)),
            ],
            Self::Torus { radius1, radius2, rotation, twist, segments, sides, slice_on, slice_from, slice_to, smooth, gen_mapping } => vec![
                ("Radius 1", f(radius1)), ("Radius 2", f(radius2)), ("Rotation", f(rotation)), ("Twist", f(twist)),
                ("Segments", i(segments)), ("Sides", i(sides)), ("Smooth", i(smooth)), ("Slice On", b(slice_on)), ("Slice From", f(slice_from)), ("Slice To", f(slice_to)),
            ("Generate Mapping Coords.", b(gen_mapping)),
            ],
            Self::Pyramid { width, depth, height, width_segs, depth_segs, height_segs, gen_mapping } => vec![
                ("Width", f(width)), ("Depth", f(depth)), ("Height", f(height)),
                ("Width Segs", i(width_segs)), ("Depth Segs", i(depth_segs)), ("Height Segs", i(height_segs)),
            ("Generate Mapping Coords.", b(gen_mapping)),
            ],
            Self::Teapot { radius, segments, body, handle, spout, lid, smooth, gen_mapping } => vec![
                ("Radius", f(radius)), ("Segments", i(segments)), ("Smooth", b(smooth)), ("Body", b(body)), ("Handle", b(handle)), ("Spout", b(spout)), ("Lid", b(lid)),
            ("Generate Mapping Coords.", b(gen_mapping)),
            ],
            Self::Plane { length, width, length_segs, width_segs, gen_mapping } => vec![
                ("Length", f(length)), ("Width", f(width)), ("Length Segs", i(length_segs)), ("Width Segs", i(width_segs)),
            ("Generate Mapping Coords.", b(gen_mapping)),
            ],
            Self::Camera { fov, fov_type, ortho, show_cone, show_horizon, show_ranges, near_range, far_range, clip_manually, near_clip, far_clip, target_distance, .. } => vec![
                ("Lens", Float(crate::camera::fov_to_lens(fov.get()))), ("FOV", f(fov)), ("FOV Direction", i(fov_type)), ("Orthographic Projection", b(ortho)),
                ("Show Cone", b(show_cone)), ("Show Horizon", b(show_horizon)),
                ("Show Ranges", b(show_ranges)), ("Near Range", f(near_range)), ("Far Range", f(far_range)),
                ("Clip Manually", b(clip_manually)), ("Near Clip", f(near_clip)), ("Far Clip", f(far_clip)),
                ("Target Distance", f(target_distance)),
            ],
            Self::Group { .. } => Vec::new(),
            Self::Bone { length, width, height, taper, fins, bone_on, freeze_length, stretch, stretch_axis, axis } => vec![
                ("Bone Axis", i(axis)),
                ("Length", f(length)), ("Width", f(width)), ("Height", f(height)), ("Taper", f(taper)),
                ("Side Fins", b(&fins.side)), ("Side Fin Size", f(&fins.side_size)), ("Side Fin Start Taper", f(&fins.side_start_taper)), ("Side Fin End Taper", f(&fins.side_end_taper)),
                ("Front Fins", b(&fins.front)), ("Front Fin Size", f(&fins.front_size)), ("Front Fin Start Taper", f(&fins.front_start_taper)), ("Front Fin End Taper", f(&fins.front_end_taper)),
                ("Back Fins", b(&fins.back)), ("Back Fin Size", f(&fins.back_size)), ("Back Fin Start Taper", f(&fins.back_start_taper)), ("Back Fin End Taper", f(&fins.back_end_taper)),
                ("Bone On", b(bone_on)), ("Freeze Length", b(freeze_length)), ("Stretch", i(stretch)), ("Stretch Axis", i(stretch_axis)),
            ],
            Self::Dummy { size } => vec![("Size", f(size))],
            Self::Point { size, axis_tripod, cross, box_ } => vec![("Size", f(size)), ("Axis Tripod", b(axis_tripod)), ("Cross", b(cross)), ("Box", b(box_))],
            Self::IkGoal { .. } => Vec::new(),
            Self::Biped(bs) => {
                let mut v = vec![("Arms", b(&bs.arms))];
                v.extend(crate::biped::BipedStructure::LINKS.iter().map(|(n, _, _)| (*n, Int(bs.count(n).unwrap_or(0)))));
                v.push(("Height", f(&bs.height)));
                v
            }
            Self::Light(l) => l.parameters(),
            Self::EditableSpline(_) => self.shape_common_params(),
            Self::Rectangle { length, width, corner_radius, .. } => [vec![("Length", f(length)), ("Width", f(width)), ("Corner Radius", f(corner_radius))], self.shape_common_params()].concat(),
            Self::Circle { radius, .. } => [vec![("Radius", f(radius))], self.shape_common_params()].concat(),
            Self::Ellipse { length, width, .. } => [vec![("Length", f(length)), ("Width", f(width))], self.shape_common_params()].concat(),
            Self::Arc { radius, from, to, pie_slice, reverse, .. } => [vec![("Radius", f(radius)), ("From", f(from)), ("To", f(to)), ("Pie Slice", b(pie_slice)), ("Reverse", b(reverse))], self.shape_common_params()].concat(),
            Self::Donut { radius1, radius2, .. } => [vec![("Radius 1", f(radius1)), ("Radius 2", f(radius2))], self.shape_common_params()].concat(),
            Self::NGon { radius, inscribed, sides, corner_radius, circular, .. } => [vec![("Radius", f(radius)), ("Inscribed", b(inscribed)), ("Sides", i(sides)), ("Corner Radius", f(corner_radius)), ("Circular", b(circular))], self.shape_common_params()].concat(),
            Self::Star { radius1, radius2, points, distortion, fillet1, fillet2, .. } => [vec![("Radius 1", f(radius1)), ("Radius 2", f(radius2)), ("Points", i(points)), ("Distortion", f(distortion)), ("Fillet Radius 1", f(fillet1)), ("Fillet Radius 2", f(fillet2))], self.shape_common_params()].concat(),
            Self::Helix { radius1, radius2, height, turns, bias, cw, .. } => [vec![("Radius 1", f(radius1)), ("Radius 2", f(radius2)), ("Height", f(height)), ("Turns", f(turns)), ("Bias", f(bias)), ("CW", b(cw))], self.shape_common_params()].concat(),
            Self::Boolean { op, display, .. } => vec![("Operation", i(op)), ("Display", i(display))],
            Self::ProBoolean { .. } => Vec::new(),
            Self::Loft { shape_steps, path_steps, cap_start, cap_end, contour, flip_normals, .. } => vec![
                ("Cap Start", b(cap_start)), ("Cap End", b(cap_end)), ("Shape Steps", i(shape_steps)), ("Path Steps", i(path_steps)),
                ("Contour", b(contour)), ("Flip Normals", b(flip_normals)),
            ],
        }
    }

    /// Sets a parameter by its rollout name, clamping to the minimums.
    pub fn set_parameter(&mut self, name: &str, value: f64) -> Result<(), String> {
        if let Self::EditablePoly(p) = self {
            return match name {
                "Iterations" => { p.subdiv.iterations.set((value.round().max(0.0) as u32).min(10)); Ok(()) }
                "Smoothness" => { p.subdiv.smoothness.set((value as f32).clamp(0.0, 1.0)); Ok(()) }
                _ => Err(format!("Editable Poly has no parameter {name:?}")),
            };
        }
        if let Self::EditablePatch(p) = self {
            return match name {
                "View Steps" => { p.steps.set((value.round().max(0.0) as u32).min(100)); Ok(()) }
                "Show Interior Edges" => { p.show_interior = value != 0.0; Ok(()) }
                _ => Err(format!("Editable Patch has no parameter {name:?}")),
            };
        }
        if let Self::Biped(b) = self {
            return b.set(name, value);
        }
        if let Self::Light(l) = self {
            return l.set_parameter(name, value);
        }
        if let Some(r) = self.set_shape_common(name, value) {
            return r;
        }
        let v = value as f32;
        let fl = |x: &mut Param<f32>, min: f32| x.set(v.max(min));
        let int = |x: &mut Param<u32>, min: u32| x.set((value.round().max(0.0) as u32).max(min));
        let bl = |x: &mut Param<bool>| x.set(value != 0.0);
        let ok = Ok(());
        match (self, name) {
            (Self::Box { gen_mapping, .. }, "Generate Mapping Coords.") => { bl(gen_mapping); ok }
            (Self::Cone { gen_mapping, .. }, "Generate Mapping Coords.") => { bl(gen_mapping); ok }
            (Self::Sphere { gen_mapping, .. }, "Generate Mapping Coords.") => { bl(gen_mapping); ok }
            (Self::GeoSphere { gen_mapping, .. }, "Generate Mapping Coords.") => { bl(gen_mapping); ok }
            (Self::Cylinder { gen_mapping, .. }, "Generate Mapping Coords.") => { bl(gen_mapping); ok }
            (Self::Tube { gen_mapping, .. }, "Generate Mapping Coords.") => { bl(gen_mapping); ok }
            (Self::Torus { gen_mapping, .. }, "Generate Mapping Coords.") => { bl(gen_mapping); ok }
            (Self::Pyramid { gen_mapping, .. }, "Generate Mapping Coords.") => { bl(gen_mapping); ok }
            (Self::Teapot { gen_mapping, .. }, "Generate Mapping Coords.") => { bl(gen_mapping); ok }
            (Self::Plane { gen_mapping, .. }, "Generate Mapping Coords.") => { bl(gen_mapping); ok }
            (Self::Box { length, .. }, "Length") => { fl(length, 0.0); ok }
            (Self::Box { width, .. }, "Width") => { fl(width, 0.0); ok }
            (Self::Box { height, .. }, "Height") => { fl(height, 0.0); ok }
            (Self::Box { length_segs, .. }, "Length Segs") => { int(length_segs, 1); ok }
            (Self::Box { width_segs, .. }, "Width Segs") => { int(width_segs, 1); ok }
            (Self::Box { height_segs, .. }, "Height Segs") => { int(height_segs, 1); ok }
            (Self::Cone { radius1, .. }, "Radius 1") | (Self::Tube { radius1, .. }, "Radius 1") | (Self::Torus { radius1, .. }, "Radius 1") => { fl(radius1, 0.0); ok }
            (Self::Cone { radius2, .. }, "Radius 2") | (Self::Tube { radius2, .. }, "Radius 2") | (Self::Torus { radius2, .. }, "Radius 2") => { fl(radius2, 0.0); ok }
            // Negative heights build below the construction plane.
            (Self::Cone { height, .. }, "Height") | (Self::Cylinder { height, .. }, "Height") | (Self::Tube { height, .. }, "Height") => { height.set(v); ok }
            (Self::Pyramid { height, .. }, "Height") => { fl(height, 0.0); ok }
            (Self::Cone { smooth, .. }, "Smooth") | (Self::Sphere { smooth, .. }, "Smooth") | (Self::GeoSphere { smooth, .. }, "Smooth") | (Self::Cylinder { smooth, .. }, "Smooth") | (Self::Tube { smooth, .. }, "Smooth") | (Self::Teapot { smooth, .. }, "Smooth") => { bl(smooth); ok }
            (Self::Torus { smooth, .. }, "Smooth") => { smooth.set((value.round().max(0.0) as u32).min(3)); ok }
            (Self::Sphere { chop_squash, .. }, "Chop/Squash") => { chop_squash.set((value.round().max(0.0) as u32).min(1)); ok }
            (Self::Sphere { base_to_pivot, .. }, "Base To Pivot") | (Self::GeoSphere { base_to_pivot, .. }, "Base To Pivot") => { bl(base_to_pivot); ok }
            (Self::Cone { height_segs, .. }, "Height Segments") | (Self::Cylinder { height_segs, .. }, "Height Segments") | (Self::Tube { height_segs, .. }, "Height Segments") => { int(height_segs, 1); ok }
            (Self::Cone { cap_segs, .. }, "Cap Segments") | (Self::Cylinder { cap_segs, .. }, "Cap Segments") | (Self::Tube { cap_segs, .. }, "Cap Segments") => { int(cap_segs, 1); ok }
            (Self::Cone { sides, .. }, "Sides") | (Self::Cylinder { sides, .. }, "Sides") | (Self::Tube { sides, .. }, "Sides") | (Self::Torus { sides, .. }, "Sides") => { int(sides, 3); ok }
            (Self::Cone { slice_on, .. }, "Slice On") | (Self::Cylinder { slice_on, .. }, "Slice On") | (Self::Tube { slice_on, .. }, "Slice On") | (Self::Torus { slice_on, .. }, "Slice On") | (Self::Sphere { slice_on, .. }, "Slice On") => { bl(slice_on); ok }
            (Self::Cone { slice_from, .. }, "Slice From") | (Self::Cylinder { slice_from, .. }, "Slice From") | (Self::Tube { slice_from, .. }, "Slice From") | (Self::Torus { slice_from, .. }, "Slice From") | (Self::Sphere { slice_from, .. }, "Slice From") => { slice_from.set(v); ok }
            (Self::Cone { slice_to, .. }, "Slice To") | (Self::Cylinder { slice_to, .. }, "Slice To") | (Self::Tube { slice_to, .. }, "Slice To") | (Self::Torus { slice_to, .. }, "Slice To") | (Self::Sphere { slice_to, .. }, "Slice To") => { slice_to.set(v); ok }
            (Self::Sphere { radius, .. }, "Radius") | (Self::GeoSphere { radius, .. }, "Radius") | (Self::Cylinder { radius, .. }, "Radius") | (Self::Teapot { radius, .. }, "Radius") => { fl(radius, 0.0); ok }
            (Self::Sphere { segments, .. }, "Segments") => { int(segments, 4); ok }
            (Self::Sphere { hemisphere, .. }, "Hemisphere") => { hemisphere.set(v.clamp(0.0, 1.0)); ok }
            (Self::GeoSphere { segments, .. }, "Segments") => { segments.set((value.round().max(1.0) as u32).min(200)); ok }
            (Self::Teapot { segments, .. }, "Segments") => { int(segments, 1); ok }
            (Self::GeoSphere { base_type, .. }, "Geodesic Base Type") => { base_type.set((value.round().max(0.0) as u32).min(2)); ok }
            (Self::Torus { rotation, .. }, "Rotation") => { rotation.set(v); ok }
            (Self::Torus { twist, .. }, "Twist") => { twist.set(v); ok }
            (Self::Torus { segments, .. }, "Segments") => { int(segments, 3); ok }
            (Self::Pyramid { width, .. }, "Width") | (Self::Plane { width, .. }, "Width") => { fl(width, 0.0); ok }
            (Self::Pyramid { depth, .. }, "Depth") => { fl(depth, 0.0); ok }
            (Self::Pyramid { width_segs, .. }, "Width Segs") | (Self::Plane { width_segs, .. }, "Width Segs") => { int(width_segs, 1); ok }
            (Self::Pyramid { depth_segs, .. }, "Depth Segs") => { int(depth_segs, 1); ok }
            (Self::Pyramid { height_segs, .. }, "Height Segs") => { int(height_segs, 1); ok }
            (Self::Plane { length, .. }, "Length") => { fl(length, 0.0); ok }
            (Self::QuadPatch { length, .. }, "Length") | (Self::TriPatch { length, .. }, "Length") => { fl(length, 0.0); ok }
            (Self::QuadPatch { width, .. }, "Width") | (Self::TriPatch { width, .. }, "Width") => { fl(width, 0.0); ok }
            (Self::QuadPatch { length_segs, .. }, "Length Segs") => { length_segs.set((value.round().max(1.0) as u32).min(100)); ok }
            (Self::QuadPatch { width_segs, .. }, "Width Segs") => { width_segs.set((value.round().max(1.0) as u32).min(100)); ok }
            (Self::Plane { length_segs, .. }, "Length Segs") => { int(length_segs, 1); ok }
            (Self::Teapot { body, .. }, "Body") => { bl(body); ok }
            (Self::Teapot { handle, .. }, "Handle") => { bl(handle); ok }
            (Self::Teapot { spout, .. }, "Spout") => { bl(spout); ok }
            (Self::Teapot { lid, .. }, "Lid") => { bl(lid); ok }
            (Self::Camera { fov, .. }, "FOV") => { fov.set(v.clamp(1.0, 175.0)); ok }
            (Self::Camera { fov, .. }, "Lens") => { fov.set(crate::camera::lens_to_fov(v.max(1e-3)).clamp(1.0, 175.0)); ok }
            (Self::Camera { fov_type, .. }, "FOV Direction") => { fov_type.set((value.round().max(0.0) as u32).min(2)); ok }
            (Self::Camera { ortho, .. }, "Orthographic Projection") => { bl(ortho); ok }
            (Self::Camera { show_cone, .. }, "Show Cone") => { bl(show_cone); ok }
            (Self::Camera { show_horizon, .. }, "Show Horizon") => { bl(show_horizon); ok }
            (Self::Camera { show_ranges, .. }, "Show Ranges") => { bl(show_ranges); ok }
            (Self::Camera { near_range, .. }, "Near Range") => { fl(near_range, 0.0); ok }
            (Self::Camera { far_range, .. }, "Far Range") => { fl(far_range, 0.0); ok }
            (Self::Camera { clip_manually, .. }, "Clip Manually") => { bl(clip_manually); ok }
            (Self::Camera { near_clip, .. }, "Near Clip") => { fl(near_clip, crate::camera::MIN_NEAR_CLIP); ok }
            (Self::Camera { far_clip, .. }, "Far Clip") => { far_clip.set(v.clamp(crate::camera::MIN_NEAR_CLIP, 1e32)); ok }
            (Self::Camera { target_distance, .. }, "Target Distance") => { fl(target_distance, 0.0); ok }
            (Self::Rectangle { length, .. }, "Length") | (Self::Ellipse { length, .. }, "Length") => { fl(length, 0.0); ok }
            (Self::Rectangle { width, .. }, "Width") | (Self::Ellipse { width, .. }, "Width") => { fl(width, 0.0); ok }
            (Self::Rectangle { corner_radius, .. }, "Corner Radius") | (Self::NGon { corner_radius, .. }, "Corner Radius") => { fl(corner_radius, 0.0); ok }
            (Self::Circle { radius, .. }, "Radius") | (Self::Arc { radius, .. }, "Radius") | (Self::NGon { radius, .. }, "Radius") => { fl(radius, 0.0); ok }
            (Self::Arc { from, .. }, "From") => { from.set(v); ok }
            (Self::Arc { to, .. }, "To") => { to.set(v); ok }
            (Self::Arc { pie_slice, .. }, "Pie Slice") => { bl(pie_slice); ok }
            (Self::Arc { reverse, .. }, "Reverse") => { bl(reverse); ok }
            (Self::Donut { radius1, .. }, "Radius 1") | (Self::Star { radius1, .. }, "Radius 1") | (Self::Helix { radius1, .. }, "Radius 1") => { fl(radius1, 0.0); ok }
            (Self::Donut { radius2, .. }, "Radius 2") | (Self::Star { radius2, .. }, "Radius 2") | (Self::Helix { radius2, .. }, "Radius 2") => { fl(radius2, 0.0); ok }
            (Self::NGon { inscribed, .. }, "Inscribed") => { bl(inscribed); ok }
            (Self::NGon { sides, .. }, "Sides") => { int(sides, 3); ok }
            (Self::NGon { circular, .. }, "Circular") => { bl(circular); ok }
            (Self::Star { points, .. }, "Points") => { int(points, 3); ok }
            (Self::Star { distortion, .. }, "Distortion") => { distortion.set(v); ok }
            (Self::Star { fillet1, .. }, "Fillet Radius 1") => { fl(fillet1, 0.0); ok }
            (Self::Star { fillet2, .. }, "Fillet Radius 2") => { fl(fillet2, 0.0); ok }
            (Self::Helix { height, .. }, "Height") => { height.set(v); ok }
            (Self::Helix { turns, .. }, "Turns") => { fl(turns, 0.01); ok }
            (Self::Helix { bias, .. }, "Bias") => { bias.set(v.clamp(-1.0, 1.0)); ok }
            (Self::Helix { cw, .. }, "CW") => { bl(cw); ok }
            (Self::Boolean { op, .. }, "Operation") => { op.set((value.round().max(0.0) as u32).min(7)); ok }
            (Self::Boolean { display, .. }, "Display") => { display.set((value.round().max(0.0) as u32).min(1)); ok }
            (Self::Loft { cap_start, .. }, "Cap Start") => { bl(cap_start); ok }
            (Self::Loft { cap_end, .. }, "Cap End") => { bl(cap_end); ok }
            (Self::Loft { shape_steps, .. }, "Shape Steps") => { int(shape_steps, 0); ok }
            (Self::Loft { path_steps, .. }, "Path Steps") => { int(path_steps, 0); ok }
            (Self::Loft { contour, .. }, "Contour") => { bl(contour); ok }
            (Self::Loft { flip_normals, .. }, "Flip Normals") => { bl(flip_normals); ok }
            (Self::Bone { length, .. }, "Length") => { fl(length, 0.001); ok }
            (Self::Bone { width, .. }, "Width") => { fl(width, 0.0); ok }
            (Self::Bone { height, .. }, "Height") => { fl(height, 0.0); ok }
            (Self::Bone { taper, .. }, "Taper") => { taper.set(v.clamp(0.0, 1.0)); ok }
            (Self::Bone { fins, .. }, "Side Fins") => { bl(&mut fins.side); ok }
            (Self::Bone { fins, .. }, "Side Fin Size") => { fl(&mut fins.side_size, 0.0); ok }
            (Self::Bone { fins, .. }, "Side Fin Start Taper") => { fins.side_start_taper.set(v.clamp(0.0, 1.0)); ok }
            (Self::Bone { fins, .. }, "Side Fin End Taper") => { fins.side_end_taper.set(v.clamp(0.0, 1.0)); ok }
            (Self::Bone { fins, .. }, "Front Fins") => { bl(&mut fins.front); ok }
            (Self::Bone { fins, .. }, "Front Fin Size") => { fl(&mut fins.front_size, 0.0); ok }
            (Self::Bone { fins, .. }, "Front Fin Start Taper") => { fins.front_start_taper.set(v.clamp(0.0, 1.0)); ok }
            (Self::Bone { fins, .. }, "Front Fin End Taper") => { fins.front_end_taper.set(v.clamp(0.0, 1.0)); ok }
            (Self::Bone { fins, .. }, "Back Fins") => { bl(&mut fins.back); ok }
            (Self::Bone { fins, .. }, "Back Fin Size") => { fl(&mut fins.back_size, 0.0); ok }
            (Self::Bone { fins, .. }, "Back Fin Start Taper") => { fins.back_start_taper.set(v.clamp(0.0, 1.0)); ok }
            (Self::Bone { fins, .. }, "Back Fin End Taper") => { fins.back_end_taper.set(v.clamp(0.0, 1.0)); ok }
            (Self::Bone { bone_on, .. }, "Bone On") => { bl(bone_on); ok }
            (Self::Bone { freeze_length, .. }, "Freeze Length") => { bl(freeze_length); ok }
            (Self::Bone { stretch, .. }, "Stretch") => { stretch.set((value.round().max(0.0) as u32).min(2)); ok }
            (Self::Bone { axis, .. }, "Bone Axis") => { axis.set((value.round().max(0.0) as u32).min(5)); ok }
            (Self::Bone { stretch_axis, .. }, "Stretch Axis") => { stretch_axis.set((value.round().max(0.0) as u32).min(2)); ok }
            (Self::Dummy { size }, "Size") | (Self::Point { size, .. }, "Size") => { fl(size, 0.001); ok }
            (Self::Point { axis_tripod, .. }, "Axis Tripod") => { bl(axis_tripod); ok }
            (Self::Point { cross, .. }, "Cross") => { bl(cross); ok }
            (Self::Point { box_, .. }, "Box") => { bl(box_); ok }
            (kind, name) => Err(format!("{} has no parameter `{name}`", kind.base_name())),
        }
    }

    pub fn poly(&self) -> Option<&PolyObject> {
        match self {
            Self::EditablePoly(p) => Some(p),
            _ => None,
        }
    }

    pub fn poly_mut(&mut self) -> Option<&mut PolyObject> {
        match self {
            Self::EditablePoly(p) => Some(p),
            _ => None,
        }
    }

    /// This object as an Editable Poly carrying `mesh`: a poly keeps its
    /// rollout settings, anything else becomes a fresh poly.
    pub fn with_mesh(&self, mesh: Mesh) -> ObjectKind {
        match self {
            Self::EditablePoly(p) => Self::EditablePoly(p.with_mesh(mesh)),
            // An Editable Mesh stays triangles: polygons a tool made are cut
            // again, edges keep their visibility.
            Self::EditableMesh(p) => Self::EditableMesh(p.with_mesh(loom_mesh::tri_mesh::retriangulate_like(&p.mesh, mesh.clone()).unwrap_or(mesh))),
            // An Editable Patch: the edited cage moves the lattice.
            Self::EditablePatch(p) => Self::EditablePatch(p.with_patch(p.patch.with_cage(&mesh))),
            _ => Self::EditablePoly(PolyObject::from_mesh(mesh)),
        }
    }

    /// Local-space mesh.
    pub fn mesh(&self) -> Cow<'_, Mesh> {
        use loom_mesh::*;
        match self {
            Self::EditablePoly(p) | Self::EditableMesh(p) => Cow::Borrowed(&p.mesh),
            Self::EditablePatch(p) => Cow::Owned(p.surface()),
            Self::QuadPatch { .. } | Self::TriPatch { .. } => Cow::Owned(self.patch_mesh().unwrap_or_default().tessellate(Self::PATCH_GRID_STEPS, false)),
            Self::Box { length, width, height, length_segs, width_segs, height_segs, gen_mapping } => {
                Cow::Owned(mapped(box_segmented(length.get(), width.get(), height.get(), [length_segs.get(), width_segs.get(), height_segs.get()]), gen_mapping.get(), AutoMap::Box))
            }
            Self::Cone { radius1, radius2, height, height_segs, cap_segs, sides, slice_on, slice_from, slice_to, smooth, gen_mapping } => Cow::Owned(smoothed(mapped(cone_primitive(
                radius1.get(), radius2.get(), height.get(), height_segs.get(), cap_segs.get(), sides.get(), slice(slice_on.get(), slice_from.get(), slice_to.get()),
            ), gen_mapping.get(), AutoMap::Cylindrical { cap: true }), smooth.get())),
            Self::Sphere { radius, segments, hemisphere, chop_squash, slice_on, slice_from, slice_to, base_to_pivot, smooth, gen_mapping } => {
                let mut m = sphere_full(radius.get(), segments.get(), hemisphere.get(), chop_squash.get() == 1, slice(slice_on.get(), slice_from.get(), slice_to.get()));
                if base_to_pivot.get() {
                    lift(&mut m, radius.get().max(0.0));
                }
                Cow::Owned(smoothed(mapped(m, gen_mapping.get(), AutoMap::Spherical), smooth.get()))
            }
            Self::GeoSphere { radius, segments, base_type, base_to_pivot, smooth, gen_mapping } => {
                let base = match base_type.get() { 0 => GeoBase::Tetra, 1 => GeoBase::Octa, _ => GeoBase::Icosa };
                let mut m = geosphere_primitive(radius.get(), segments.get(), base);
                if base_to_pivot.get() {
                    lift(&mut m, radius.get().max(0.0));
                }
                Cow::Owned(smoothed(mapped(m, gen_mapping.get(), AutoMap::Spherical), smooth.get()))
            }
            Self::Cylinder { radius, height, height_segs, cap_segs, sides, slice_on, slice_from, slice_to, smooth, gen_mapping } => Cow::Owned(smoothed(mapped(cylinder_segmented(
                radius.get(), height.get(), height_segs.get(), cap_segs.get(), sides.get(), slice(slice_on.get(), slice_from.get(), slice_to.get()),
            ), gen_mapping.get(), AutoMap::Cylindrical { cap: true }), smooth.get())),
            Self::Tube { radius1, radius2, height, height_segs, cap_segs, sides, slice_on, slice_from, slice_to, smooth, gen_mapping } => Cow::Owned(smoothed(mapped(tube_primitive(
                radius1.get(), radius2.get(), height.get(), height_segs.get(), cap_segs.get(), sides.get(), slice(slice_on.get(), slice_from.get(), slice_to.get()),
            ), gen_mapping.get(), AutoMap::Cylindrical { cap: true }), smooth.get())),
            Self::Torus { radius1, radius2, rotation, twist, segments, sides, slice_on, slice_from, slice_to, smooth, gen_mapping } => {
                let mode = match smooth.get() { 1 => TorusSmooth::Sides, 2 => TorusSmooth::None, 3 => TorusSmooth::Segments, _ => TorusSmooth::All };
                Cow::Owned(mapped(torus_smoothed(
                    radius1.get(), radius2.get(), rotation.get(), twist.get(), segments.get(), sides.get(), slice(slice_on.get(), slice_from.get(), slice_to.get()), mode,
                ), gen_mapping.get(), AutoMap::Torus))
            }
            Self::Pyramid { width, depth, height, width_segs, depth_segs, height_segs, gen_mapping } => {
                Cow::Owned(mapped(pyramid_primitive(width.get(), depth.get(), height.get(), [width_segs.get(), depth_segs.get(), height_segs.get()]), gen_mapping.get(), AutoMap::Box))
            }
            Self::Teapot { radius, segments, body, handle, spout, lid, smooth, gen_mapping } => Cow::Owned(smoothed(mapped(teapot_primitive(
                radius.get(), segments.get(), TeapotParts { body: body.get(), handle: handle.get(), spout: spout.get(), lid: lid.get() },
            ), gen_mapping.get(), AutoMap::Spherical), smooth.get())),
            Self::Plane { length, width, length_segs, width_segs, gen_mapping } => {
                Cow::Owned(mapped(plane_segmented(length.get(), width.get(), length_segs.get(), width_segs.get()), gen_mapping.get(), AutoMap::Planar))
            }
            Self::Camera { .. } => Cow::Owned(crate::camera::icon_mesh()),
            Self::Light(l) => Cow::Owned(l.icon_mesh()),
            Self::Group { .. } => Cow::Owned(Mesh::default()),
            Self::Bone { length, width, height, taper, fins, axis, .. } => {
                let mut m = loom_mesh::bone_mesh(
                    length.get(),
                    width.get(),
                    height.get(),
                    taper.get(),
                    &loom_mesh::BoneFinParams {
                        side: fins.side.get(),
                        front: fins.front.get(),
                        back: fins.back.get(),
                        size: fins.side_size.get().max(fins.front_size.get()).max(fins.back_size.get()),
                        start_taper: fins.side_start_taper.get(),
                        end_taper: fins.side_end_taper.get(),
                    },
                );
                // Built along +X; turned onto the bone's axis.
                let dir = bone_axis_dir(axis.get());
                if (dir - Vec3::X).length_squared() > 1e-8 {
                    let q = if (dir + Vec3::X).length_squared() < 1e-8 { glam::Quat::from_rotation_z(std::f32::consts::PI) } else { glam::Quat::from_rotation_arc(Vec3::X, dir) };
                    let _ = loom_mesh::edit_ops::transform_mesh(&mut m, Mat4::from_quat(q));
                }
                Cow::Owned(m)
            }
            Self::Dummy { size } => Cow::Owned(loom_mesh::dummy_mesh(size.get())),
            // Point: a small cube a tenth of its size; the tripod/cross draw as an overlay.
            Self::Point { size, .. } => Cow::Owned(box_primitive(Vec3::splat(size.get() * 0.1))),
            Self::IkGoal { .. } => Cow::Owned(box_primitive(Vec3::splat(1.0))),
            // The centre of mass: an octahedron.
            Self::Biped(b) => Cow::Owned(loom_mesh::standard_primitives::geosphere_primitive(crate::biped::root_size(b.height.get()), 1, loom_mesh::standard_primitives::GeoBase::Octa)),
            Self::Boolean { a, b, display, .. } => {
                if display.get() == 1 {
                    let mut both = a.clone();
                    let _ = loom_mesh::geom_ops::attach(&mut both, b, Mat4::IDENTITY);
                    return Cow::Owned(both);
                }
                // A result that cannot be built shows operand A (the rollout reports why).
                Cow::Owned(self.compound_result().unwrap_or_else(|_| a.clone()))
            }
            Self::ProBoolean { base, .. } => Cow::Owned(self.compound_result().unwrap_or_else(|_| base.clone())),
            Self::Loft { .. } => Cow::Owned(self.compound_result().unwrap_or_default()),
            // Shapes: the rendering tube when Enable In Viewport, else the
            // interpolated points as isolated vertices (bounds and picking).
            shape => {
                let sh = shape.shape().expect("shape kinds have a shape");
                Cow::Owned(shape_cage_mesh(&sh, matches!(shape, Self::EditableSpline(_))))
            }
        }
    }
}

/// Per-axis transform locks (Hierarchy > Link Info > Locks) or inheritance
/// flags (Link Info > Inherit). `true` in `Locks` means locked; `true` in
/// `inherit` means the component is inherited from the parent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Axes3 {
    pub move_axes: [bool; 3],
    pub rotate_axes: [bool; 3],
    pub scale_axes: [bool; 3],
}

impl Axes3 {
    pub const NONE: Axes3 = Axes3 { move_axes: [false; 3], rotate_axes: [false; 3], scale_axes: [false; 3] };
    pub const ALL: Axes3 = Axes3 { move_axes: [true; 3], rotate_axes: [true; 3], scale_axes: [true; 3] };
}

/// Display panel > Display Properties rollout, per object.
/// What the viewport paints on an object over its colour (the spec's
/// checker in the viewport).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TextureDisplay {
    #[default]
    None,
    Checker,
    /// A bitmap file, sampled with map channel 1.
    Bitmap(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayProps {
    pub see_through: bool,
    pub display_as_box: bool,
    pub backface_cull: bool,
    pub vertex_ticks: bool,
    pub ignore_extents: bool,
    pub show_frozen_in_gray: bool,
    /// Vertex Channel Display: shade by vertex colour.
    #[serde(default)]
    pub vertex_colors: bool,
    /// Show Map in Viewport: none, the checker, or a bitmap.
    #[serde(default)]
    pub texture: TextureDisplay,
    /// Object Color By Layer: the viewport shows the layer's colour instead of the object's.
    #[serde(default)]
    pub color_by_layer: bool,
    /// Object Properties > Display Properties By Layer: the layer's display properties apply.
    #[serde(default)]
    pub by_layer: bool,
    /// Edges Only: wireframes show polygon edges only; off adds the
    /// triangulation diagonals as dashed lines.
    #[serde(default = "default_true")]
    pub edges_only: bool,
    /// Trajectory: the viewports draw this object's motion path.
    #[serde(default)]
    pub trajectory: bool,
    /// Vertex Channel Display > Shaded: vertex colours lit by the viewport
    /// lighting; off shows the raw colours.
    #[serde(default)]
    pub vertex_colors_shaded: bool,
    /// Link Display > Display Links: the link to the parent is drawn.
    #[serde(default)]
    pub display_links: bool,
    /// Link Display > Link Replaces Object: only the link is drawn.
    #[serde(default)]
    pub link_replaces_object: bool,
}

fn default_true() -> bool {
    true
}

impl Default for DisplayProps {
    fn default() -> Self {
        Self {
            see_through: false,
            display_as_box: false,
            backface_cull: false,
            vertex_ticks: false,
            ignore_extents: false,
            show_frozen_in_gray: true,
            vertex_colors: false,
            texture: TextureDisplay::None,
            edges_only: true,
            trajectory: false,
            vertex_colors_shaded: false,
            display_links: false,
            link_replaces_object: false,
            color_by_layer: false,
            by_layer: false,
        }
    }
}

impl DisplayProps {
    /// Display Properties checkbox names, in rollout order.
    pub const NAMES: [&'static str; 10] = ["See-Through", "Display as Box", "Backface Cull", "Edges Only", "Vertex Ticks", "Trajectory", "Ignore Extents", "Show Frozen in Gray", "Vertex Color", "Vertex Color Shaded"];
    /// Link Display rollout checkbox names.
    pub const LINK_NAMES: [&'static str; 2] = ["Display Links", "Link Replaces Object"];

    pub fn get(&self, name: &str) -> Option<bool> {
        Some(match name {
            "See-Through" => self.see_through,
            "Display as Box" => self.display_as_box,
            "Backface Cull" => self.backface_cull,
            "Edges Only" => self.edges_only,
            "Vertex Ticks" => self.vertex_ticks,
            "Trajectory" => self.trajectory,
            "Ignore Extents" => self.ignore_extents,
            "Show Frozen in Gray" => self.show_frozen_in_gray,
            "Vertex Color" => self.vertex_colors,
            "Vertex Color Shaded" => self.vertex_colors_shaded,
            "Display Links" => self.display_links,
            "Link Replaces Object" => self.link_replaces_object,
            _ => return None,
        })
    }

    pub fn set(&mut self, name: &str, value: bool) -> bool {
        match name {
            "See-Through" => self.see_through = value,
            "Display as Box" => self.display_as_box = value,
            "Backface Cull" => self.backface_cull = value,
            "Edges Only" => self.edges_only = value,
            "Vertex Ticks" => self.vertex_ticks = value,
            "Trajectory" => self.trajectory = value,
            "Ignore Extents" => self.ignore_extents = value,
            "Show Frozen in Gray" => self.show_frozen_in_gray = value,
            "Vertex Color" => self.vertex_colors = value,
            "Vertex Color Shaded" => self.vertex_colors_shaded = value,
            "Display Links" => self.display_links = value,
            "Link Replaces Object" => self.link_replaces_object = value,
            _ => return false,
        }
        true
    }
}

/// Object Properties > Rendering Control, per object: what renders see of
/// the object. The viewports draw it regardless.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderProps {
    /// Off: the object is left out of renders (and casts no shadows there).
    pub renderable: bool,
    /// The rendered visibility is multiplied by the parent's.
    pub inherit_visibility: bool,
    /// Off: cameras do not see the object; its shadows still render.
    pub visible_to_camera: bool,
    pub receive_shadows: bool,
    pub cast_shadows: bool,
    /// Rendering Control By Layer: the layer's rendering control applies.
    pub by_layer: bool,
}

impl Default for RenderProps {
    fn default() -> Self {
        Self { renderable: true, inherit_visibility: true, visible_to_camera: true, receive_shadows: true, cast_shadows: true, by_layer: false }
    }
}

impl RenderProps {
    /// Rendering Control checkbox names, in dialog order.
    pub const NAMES: [&'static str; 5] = ["Renderable", "Inherit Visibility", "Visible to Camera", "Receive Shadows", "Cast Shadows"];

    pub fn get(&self, name: &str) -> Option<bool> {
        Some(match name {
            "Renderable" => self.renderable,
            "Inherit Visibility" => self.inherit_visibility,
            "Visible to Camera" => self.visible_to_camera,
            "Receive Shadows" => self.receive_shadows,
            "Cast Shadows" => self.cast_shadows,
            _ => return None,
        })
    }

    pub fn set(&mut self, name: &str, value: bool) -> bool {
        match name {
            "Renderable" => self.renderable = value,
            "Inherit Visibility" => self.inherit_visibility = value,
            "Visible to Camera" => self.visible_to_camera = value,
            "Receive Shadows" => self.receive_shadows = value,
            "Cast Shadows" => self.cast_shadows = value,
            _ => return false,
        }
        true
    }
}

/// The node settings that Hierarchy and Display panels edit, undone as one unit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeProps {
    pub object_offset: Transform,
    pub locks: Axes3,
    pub inherit: Axes3,
    pub display: DisplayProps,
    pub instance_of: Option<NodeId>,
    pub render: RenderProps,
    /// Object Properties > User Defined: free text kept with the scene.
    pub user_props: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub name: String,
    pub parent: Option<NodeId>,
    /// The node transform: the pivot.
    pub transform: Transform,
    /// Object-offset transform: where the mesh sits relative to the pivot
    /// (Affect Pivot Only / Affect Object Only edit this).
    #[serde(default)]
    pub object_offset: Transform,
    pub object: ObjectKind,
    pub hidden: bool,
    pub frozen: bool,
    /// Object colour (Name and Color rollout), one of the palette.
    #[serde(default = "default_wire_color")]
    pub wire_color: [u8; 3],
    #[serde(default = "Axes3::none")]
    pub locks: Axes3,
    #[serde(default = "Axes3::all")]
    pub inherit: Axes3,
    #[serde(default)]
    pub display: DisplayProps,
    /// The Layer Manager layer (0 = default).
    #[serde(default)]
    pub layer: u32,
    /// The modifier stack, bottom to top.
    #[serde(default)]
    pub modifiers: Vec<crate::modifier::ModifierSlot>,
    /// The stack entry the Modify panel edits: None = the base object.
    #[serde(default)]
    pub active_entry: Option<usize>,
    /// Material names by material id (`materials[i]` is id i+1), as FBX
    /// carries them; empty until a file brings some in.
    #[serde(default)]
    pub materials: Vec<String>,
    /// The assigned material (Material Editor > Assign Material to Selection).
    #[serde(default)]
    pub material: Option<crate::material::MaterialId>,
    /// Bumped on every change that can alter the evaluated result (cache key).
    #[serde(skip)]
    pub version: u64,
    /// Bumped on object / stack changes only (the stack cache key).
    #[serde(skip)]
    pub geom_version: u64,
    /// Clone Options > Instance/Reference: this node shares its object with
    /// the given source node (parameter edits apply to the whole group).
    #[serde(default)]
    pub instance_of: Option<NodeId>,
    /// Path / Position / Attachment, LookAt / Orientation and Link constraints.
    #[serde(default)]
    pub constraints: crate::constraint::Constraints,
    /// Track View > Visibility Track: 0 invisible … 1 fully visible.
    #[serde(default)]
    pub visibility: Option<Param<f32>>,
    /// Object Properties > Rendering Control.
    #[serde(default)]
    pub render: RenderProps,
    /// Object Properties > User Defined.
    #[serde(default)]
    pub user_props: String,
}

impl Node {
    /// Rewrites every node reference this node holds (own id, parent,
    /// instance source, instanced modifier sources, picked nodes, camera
    /// target). `f` returning `None` drops the reference: a parent or
    /// picked node that is not part of a Merge.
    pub fn remap_ids(&mut self, f: &dyn Fn(NodeId) -> Option<NodeId>) {
        if let Some(id) = f(self.id) {
            self.id = id;
        }
        self.parent = self.parent.and_then(f);
        self.instance_of = self.instance_of.and_then(f);
        for slot in &mut self.modifiers {
            slot.instance_of = slot.instance_of.and_then(|(id, i)| f(id).map(|id| (id, i)));
            slot.modifier.remap_ids(f);
        }
        if let ObjectKind::Camera { target, .. } = &mut self.object {
            *target = target.and_then(f);
        }
        if let ObjectKind::Light(l) = &mut self.object {
            l.target = l.target.and_then(f);
            l.exclusion.nodes = l.exclusion.nodes.iter().filter_map(|n| f(*n)).collect();
        }
        self.constraints.remap_ids(f);
    }
}

/// Equality of what a file stores: everything but the in-memory versions.
impl PartialEq for Node {
    fn eq(&self, o: &Self) -> bool {
        self.id == o.id
            && self.name == o.name
            && self.parent == o.parent
            && self.transform == o.transform
            && self.object_offset == o.object_offset
            && self.object == o.object
            && self.hidden == o.hidden
            && self.frozen == o.frozen
            && self.wire_color == o.wire_color
            && self.locks == o.locks
            && self.inherit == o.inherit
            && self.display == o.display
            && self.layer == o.layer
            && self.modifiers == o.modifiers
            && self.active_entry == o.active_entry
            && self.instance_of == o.instance_of
            && self.constraints == o.constraints
            && self.visibility == o.visibility
            && self.render == o.render
            && self.user_props == o.user_props
    }
}

impl Axes3 {
    fn none() -> Self {
        Self::NONE
    }
    fn all() -> Self {
        Self::ALL
    }
}

impl Node {
    /// The stack entry being edited as an Edit Poly (or Edit Mesh), when the active entry is one.
    pub fn editing_entry(&self) -> Option<usize> {
        let a = self.active_entry?;
        matches!(self.modifiers.get(a)?.modifier, crate::modifier::Modifier::EditPoly { .. } | crate::modifier::Modifier::EditMesh { .. }).then_some(a)
    }

    /// The Editable Poly the user edits: the active Edit Poly / Edit Mesh
    /// entry's, else the base object's (an Editable Poly or Editable Mesh).
    pub fn edit_poly(&self) -> Option<&PolyObject> {
        match self.editing_entry() {
            Some(a) => self.modifiers[a].modifier.edit_poly(),
            None => self.object.poly().or_else(|| self.object.tri_mesh()),
        }
    }

    pub fn edit_poly_mut(&mut self) -> Option<&mut PolyObject> {
        match self.editing_entry() {
            Some(a) => self.modifiers[a].modifier.edit_poly_mut(),
            None => match &mut self.object {
                ObjectKind::EditablePoly(p) | ObjectKind::EditableMesh(p) => Some(p),
                _ => None,
            },
        }
    }

    /// What the user edits is an Editable Mesh (or an Edit Mesh entry):
    /// triangles with edge visibility.
    pub fn edits_tri_mesh(&self) -> bool {
        match self.editing_entry() {
            Some(a) => matches!(self.modifiers[a].modifier, crate::modifier::Modifier::EditMesh { .. }),
            None => matches!(self.object, ObjectKind::EditableMesh(_)) && self.editing_spline_entry().is_none() && self.editing_patch_entry().is_none(),
        }
    }

    /// The stack entry being edited as an Edit Patch, when the active entry is one.
    pub fn editing_patch_entry(&self) -> Option<usize> {
        let a = self.active_entry?;
        matches!(self.modifiers.get(a)?.modifier, crate::modifier::Modifier::EditPatch { .. }).then_some(a)
    }

    /// The Editable Patch the user edits: the active Edit Patch entry's, else the base object's.
    pub fn edit_patch(&self) -> Option<&PatchObject> {
        match self.editing_patch_entry() {
            Some(a) => self.modifiers[a].modifier.edit_patch(),
            None => self.object.patch(),
        }
    }

    pub fn edit_patch_mut(&mut self) -> Option<&mut PatchObject> {
        match self.editing_patch_entry() {
            Some(a) => self.modifiers[a].modifier.edit_patch_mut(),
            None => self.object.patch_mut(),
        }
    }

    /// The mesh sub-object editing and picking work on: the active Edit
    /// Poly's mesh, the active Edit Spline's knots, else the base object's.
    pub fn cage_mesh(&self) -> Cow<'_, Mesh> {
        // A patch: its lattice (vertices, then handles, one polygon per patch).
        if let Some(p) = self.edit_patch() {
            return Cow::Owned(p.patch.cage());
        }
        if let Some(a) = self.editing_spline_entry() {
            if let Some(so) = self.modifiers[a].modifier.edit_spline() {
                return Cow::Owned(shape_cage_mesh(&so.shape, true));
            }
        }
        if let Some(a) = self.active_entry {
            // HSDS at a sub-object level: its finest level is picked.
            if let Some(crate::modifier::Modifier::Hsds(d)) = self.modifiers.get(a).filter(|m| m.sub_level > 0).map(|m| &m.modifier) {
                if d.cage.vertex_count() > 0 {
                    return Cow::Borrowed(&d.cage);
                }
            }
        }
        match self.edit_poly().filter(|_| self.editing_entry().is_some()) {
            Some(p) => Cow::Borrowed(&p.mesh),
            None => self.object.mesh(),
        }
    }

    /// The stack entry being edited as an Edit Spline, when the active entry is one.
    pub fn editing_spline_entry(&self) -> Option<usize> {
        let a = self.active_entry?;
        matches!(self.modifiers.get(a)?.modifier, crate::modifier::Modifier::EditSpline { .. }).then_some(a)
    }

    /// The Editable Spline the user edits: the active Edit Spline entry's, else the base object's.
    pub fn edit_spline(&self) -> Option<&ShapeObject> {
        match self.editing_spline_entry() {
            Some(a) => self.modifiers[a].modifier.edit_spline(),
            None => self.object.spline(),
        }
    }

    pub fn edit_spline_mut(&mut self) -> Option<&mut ShapeObject> {
        match self.editing_spline_entry() {
            Some(a) => self.modifiers[a].modifier.edit_spline_mut(),
            None => self.object.spline_mut(),
        }
    }

    /// The shape the user edits (an Edit Spline entry's, else the object's own).
    pub fn edit_shape(&self) -> Option<loom_shape::Shape> {
        match self.editing_spline_entry() {
            Some(a) => self.modifiers[a].modifier.edit_spline().map(|s| s.shape.clone()),
            None => self.object.shape(),
        }
    }
}

impl Node {
    /// Euler XYZ rotation of the node transform in degrees (Motion panel values).
    pub fn euler_deg(&self) -> Vec3 {
        self.transform.rotation.get()
    }

    pub fn props(&self) -> NodeProps {
        NodeProps { object_offset: self.object_offset.clone(), locks: self.locks, inherit: self.inherit, display: self.display.clone(), instance_of: self.instance_of, render: self.render.clone(), user_props: self.user_props.clone() }
    }

    pub fn set_props(&mut self, p: NodeProps) {
        self.object_offset = p.object_offset;
        self.locks = p.locks;
        self.inherit = p.inherit;
        self.display = p.display.clone();
        self.instance_of = p.instance_of;
        self.render = p.render;
        self.user_props = p.user_props;
    }
}

fn default_wire_color() -> [u8; 3] {
    [110, 150, 210]
}

impl Node {
    /// A node with no id yet; `Scene::add_node` assigns one.
    pub fn new(name: &str, object: ObjectKind) -> Self {
        Self {
            id: NodeId(0),
            name: name.to_owned(),
            parent: None,
            transform: Transform::default(),
            object_offset: Transform::default(),
            object,
            hidden: false,
            frozen: false,
            wire_color: default_wire_color(),
            locks: Axes3::NONE,
            inherit: Axes3::ALL,
            display: DisplayProps::default(),
            layer: 0,
            modifiers: Vec::new(),
            active_entry: None,
            materials: Vec::new(),
            material: None,
            constraints: Default::default(),
            visibility: None,
            render: RenderProps::default(),
            user_props: String::new(),
            version: 0,
            geom_version: 0,
            instance_of: None,
        }
    }
}

/// The default object colour palette (Name and Color rollout picks one
/// at random; we cycle by id so colours are stable and testable).
pub const OBJECT_COLORS: [[u8; 3]; 16] = [
    [225, 143, 87], [223, 87, 87], [87, 223, 87], [87, 87, 223], [223, 223, 87], [87, 223, 223], [223, 87, 223], [180, 180, 180],
    [255, 128, 0], [128, 255, 0], [0, 255, 128], [0, 128, 255], [128, 0, 255], [255, 0, 128], [200, 160, 110], [110, 160, 200],
];
