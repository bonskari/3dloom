//! The modifier stack: `ModifierSlot`s on a node, evaluated in order over
//! the base object's output. Each `Modifier` variant carries its
//! parameters as `Param<T>` and a pure `apply`.

use crate::node::{NodeId, ParamValue, PolyObject, ShapeObject, Transform};
use std::collections::{BTreeMap, BTreeSet};
use crate::param::Param;
use glam::{Mat4, Vec3};
use loom_mesh::select_ops::SubLevel;
use loom_mesh::{deform, Mesh, VertexId};
use serde::{Deserialize, Serialize};

/// The gizmo and Limits every deformation modifier has (Gizmo and
/// Center sub-object levels, Limit Effect with Upper / Lower Limit).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Deformer {
    pub gizmo: Transform,
    /// 0 = X, 1 = Y, 2 = Z.
    pub axis: Param<u32>,
    pub limit: Param<bool>,
    pub upper: Param<f32>,
    pub lower: Param<f32>,
    /// The Center sub-object, in gizmo space: the deformation is measured from it.
    #[serde(default = "param_origin")]
    pub center: Param<Vec3>,
}

fn param_origin() -> Param<Vec3> {
    Param::new(Vec3::ZERO)
}

impl Default for Deformer {
    fn default() -> Self {
        Self { gizmo: Transform::default(), axis: Param::new(2), limit: Param::new(false), upper: Param::new(0.0), lower: Param::new(0.0), center: param_origin() }
    }
}

impl Deformer {
    fn limits(&self) -> deform::Limits {
        self.limit.get().then(|| (self.lower.get().min(self.upper.get()), self.upper.get().max(self.lower.get())))
    }
}

/// What flows up the stack: the mesh plus the sub-object selection a lower
/// entry passes up (Editable Poly at a sub-object level, Poly Select).
#[derive(Debug, Clone)]
pub struct StackValue {
    pub mesh: Mesh,
    pub selection: Option<(SubLevel, Vec<u32>)>,
    /// The shape flowing up from a spline object (consumed by Extrude /
    /// Lathe / Bevel / Sweep, edited by Edit Spline, dropped by anything else).
    pub shape: Option<loom_shape::Shape>,
    /// A shape sub-object selection flowing up (Spline Select): level 1
    /// vertex (flat knot index), 2 segment (flat segment index), 3 spline.
    pub shape_selection: Option<(u32, Vec<u32>)>,
    /// Warnings a modifier raises while still producing a result (the
    /// stack collects them next to its own).
    pub warnings: Vec<String>,
    /// The patch lattice flowing up from a patch object or Surface
    /// (edited by Edit Patch, dropped by anything else).
    pub patch: Option<loom_mesh::patch::PatchMesh>,
}

/// The Unwrap UVW modifier's data: the edited copy of its channel, the
/// topology it was edited on, seams, pins and the editor's selection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UnwrapUvw {
    /// A hash of the input's face loops (vertex ids), so a topology change
    /// that keeps the face and corner counts (Turn) is still noticed.
    pub topo_hash: u64,
    /// Map Channel (1–99) or 0 = Vertex Color Channel.
    pub channel: Param<u32>,
    /// The edited channel; None passes the input through untouched.
    pub edits: Option<loom_mesh::half_edge::MapChannel>,
    /// Face centroids of the input the edits were made on (topology carry).
    pub face_snapshot: Vec<Vec3>,
    /// The input's corner (half-edge) ids per face, so the edited channel's
    /// corners can be read back per face after a topology change below.
    pub face_loops: Vec<Vec<u32>>,
    pub input_faces: usize,
    pub input_corners: usize,
    /// Pelt seams as geometry vertex pairs (min, max).
    pub seams: BTreeSet<(u32, u32)>,
    /// Pinned texture vertices (Relax / Pelt keep them).
    pub pins: BTreeSet<u32>,
    pub tv_selection: BTreeSet<u32>,
    pub tv_edge_selection: BTreeSet<(u32, u32)>,
    /// Geometry face selection (the modifier's Face level).
    pub face_selection: BTreeSet<u32>,
    /// Map Parameters: 0 Planar, 1 Pelt, 2 Cylindrical, 3 Spherical, 4 Box.
    pub map_kind: Param<u32>,
    pub map_gizmo: Transform,
    pub map_size: Vec3,
    pub normalize_map: Param<bool>,
    pub show_seams: Param<bool>,
    pub thin_seams: Param<bool>,
    pub always_show_pelt_seam: Param<bool>,
    pub prevent_reflattening: Param<bool>,
    pub ignore_backfacing: Param<bool>,
    pub select_by_element: Param<bool>,
    pub planar_angle: Param<f32>,
    pub select_matid: Param<u32>,
    pub select_sg: Param<u32>,
    /// Hidden / frozen texture vertices of the editor.
    pub hidden_tvs: BTreeSet<u32>,
    pub frozen_tvs: BTreeSet<u32>,
}

impl Default for UnwrapUvw {
    fn default() -> Self {
        Self {
            topo_hash: 0,
            channel: Param::new(1),
            edits: None,
            face_snapshot: Vec::new(),
            face_loops: Vec::new(),
            input_faces: 0,
            input_corners: 0,
            seams: BTreeSet::new(),
            pins: BTreeSet::new(),
            tv_selection: BTreeSet::new(),
            tv_edge_selection: BTreeSet::new(),
            face_selection: BTreeSet::new(),
            map_kind: Param::new(0),
            map_gizmo: Transform::default(),
            map_size: Vec3::ONE,
            normalize_map: Param::new(true),
            show_seams: Param::new(true),
            thin_seams: Param::new(true),
            always_show_pelt_seam: Param::new(true),
            prevent_reflattening: Param::new(false),
            ignore_backfacing: Param::new(false),
            select_by_element: Param::new(false),
            planar_angle: Param::new(15.0),
            select_matid: Param::new(1),
            select_sg: Param::new(1),
            hidden_tvs: BTreeSet::new(),
            frozen_tvs: BTreeSet::new(),
        }
    }
}

impl UnwrapUvw {
    /// The input's channel with the edits applied (channel 0 = the vertex
    /// colours as a channel). After a topology change below, the edits are
    /// carried onto the faces that still exist (see `carry`).
    pub fn working_mesh(&self, input: &Mesh) -> Mesh {
        self.carry(input).0
    }

    /// True while the input still has the topology the edits were made on.
    pub fn topology_matches(&self, input: &Mesh) -> bool {
        input.face_count() == self.input_faces && input.half_edge_count() == self.input_corners && (self.topo_hash == 0 || topology_hash(input) == self.topo_hash)
    }

    /// The working mesh, and how many edited faces were carried when the
    /// topology below changed (None = no change).
    pub fn carry(&self, input: &Mesh) -> (Mesh, Option<usize>) {
        let mut m = input.clone();
        let ch = self.channel.get();
        if ch == 0 {
            let colours: Vec<Vec3> = m.vertices().iter().map(|v| Vec3::from(v.color)).collect();
            let corners: Vec<u32> = m.half_edges().iter().map(|h| h.origin.0).collect();
            m.set_map(0, loom_mesh::half_edge::MapChannel { verts: colours, corners });
        } else {
            m.ensure_map(ch);
        }
        let Some(edits) = &self.edits else { return (m, None) };
        if self.topology_matches(input) && edits.corners.len() == m.half_edge_count() {
            m.set_map(ch, edits.clone());
            return (m, None);
        }
        // Faces that still sit at a snapshot centroid keep their edited corners;
        // corners of one vertex with equal values stay welded.
        let key = |p: Vec3| (p.x.to_bits(), p.y.to_bits(), p.z.to_bits());
        let by_pos: std::collections::HashMap<(u32, u32, u32), usize> = self.face_snapshot.iter().enumerate().map(|(i, p)| (key(*p), i)).collect();
        let mut map = m.map(ch).cloned().expect("ensured");
        let mut welded: std::collections::HashMap<(u32, [u32; 3]), u32> = std::collections::HashMap::new();
        let mut carried = 0usize;
        for f in 0..m.face_count() {
            let fid = loom_mesh::FaceId(f as u32);
            let Some(old) = by_pos.get(&key(face_centroid(&m, fid))) else { continue };
            let Some(corners) = m.face_corners(fid) else { continue };
            let Some(old_loop) = self.face_loops.get(*old) else { continue };
            if old_loop.len() != corners.len() {
                continue;
            }
            for (he, old_he) in corners.iter().zip(old_loop) {
                let uv = edits.corners.get(*old_he as usize).and_then(|tv| edits.verts.get(*tv as usize)).copied().unwrap_or(Vec3::ZERO);
                let v = m.half_edges()[he.0 as usize].origin.0;
                let tv = *welded.entry((v, [uv.x.to_bits(), uv.y.to_bits(), uv.z.to_bits()])).or_insert_with(|| {
                    map.verts.push(uv);
                    (map.verts.len() - 1) as u32
                });
                map.corners[he.0 as usize] = tv;
            }
            carried += 1;
        }
        m.set_map(ch, map);
        (m, Some(carried))
    }

    /// Takes the channel of an edited working mesh as the entry's edits.
    pub fn take_edits(&mut self, working: &Mesh) {
        self.edits = working.map(self.channel.get()).cloned();
    }

    /// Starts from the input: topology snapshot, no edits.
    pub fn init(&mut self, input: &Mesh) {
        self.input_faces = input.face_count();
        self.input_corners = input.half_edge_count();
        self.topo_hash = topology_hash(input);
        self.face_snapshot = (0..input.face_count()).map(|f| face_centroid(input, loom_mesh::FaceId(f as u32))).collect();
        self.face_loops = (0..input.face_count()).map(|f| input.face_corners(loom_mesh::FaceId(f as u32)).unwrap_or_default().into_iter().map(|h| h.0).collect()).collect();
    }
}

/// A hash of the face loops (vertex ids in order): the topology an Unwrap
/// entry's edits belong to.
pub fn topology_hash(mesh: &Mesh) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for f in 0..mesh.face_count() {
        for he in mesh.face_corners(loom_mesh::FaceId(f as u32)).unwrap_or_default() {
            mesh.half_edges()[he.0 as usize].origin.0.hash(&mut h);
        }
        u32::MAX.hash(&mut h);
    }
    let v = h.finish();
    if v == 0 { 1 } else { v }
}

/// Channel 0 edits land on the vertex colours.
fn apply_vertex_colour_channel(out: &mut Mesh, edits: &loom_mesh::half_edge::MapChannel) {
    let mut colours: Vec<Option<[f32; 3]>> = vec![None; out.vertex_count()];
    for (i, he) in out.half_edges().iter().enumerate() {
        if he.face.is_none() {
            continue;
        }
        if let Some(uv) = edits.verts.get(edits.corners[i] as usize) {
            let c = &mut colours[he.origin.0 as usize];
            if c.is_none() {
                *c = Some([uv.x.clamp(0.0, 1.0), uv.y.clamp(0.0, 1.0), uv.z.clamp(0.0, 1.0)]);
            }
        }
    }
    for (v, c) in out.vertices_mut().iter_mut().zip(colours) {
        if let Some(c) = c {
            v.color = c;
        }
    }
}

fn param_off() -> Param<bool> {
    Param::new(false)
}

fn param_one() -> Param<f32> {
    Param::new(1.0)
}

fn param_zero() -> Param<u32> {
    Param::new(0)
}

/// VertexPaint layer blend modes (the Layer group's Mode list), by index.
pub const VERTEX_PAINT_MODES: [&str; 15] = [
    "Normal", "Add", "Subtract", "Multiply", "Screen", "Overlay", "Darken", "Lighten", "Color Burn", "Color Dodge", "Difference", "Exclusion", "Soft Light", "Hard Light", "Pin Light",
];

/// One channel of a VertexPaint layer colour `t` blended onto the colour below `b`.
pub fn blend_channel(mode: u32, b: f32, t: f32) -> f32 {
    let r = match mode {
        1 => b + t,
        2 => b - t,
        3 => b * t,
        4 => 1.0 - (1.0 - b) * (1.0 - t),
        5 => if b < 0.5 { 2.0 * b * t } else { 1.0 - 2.0 * (1.0 - b) * (1.0 - t) },
        6 => b.min(t),
        7 => b.max(t),
        8 => if t <= 0.0 { 0.0 } else { 1.0 - ((1.0 - b) / t).min(1.0) },
        9 => if t >= 1.0 { 1.0 } else { (b / (1.0 - t)).min(1.0) },
        10 => (b - t).abs(),
        11 => b + t - 2.0 * b * t,
        12 => (1.0 - 2.0 * t) * b * b + 2.0 * t * b,
        13 => if t < 0.5 { 2.0 * b * t } else { 1.0 - 2.0 * (1.0 - b) * (1.0 - t) },
        14 => if t < 0.5 { b.min(2.0 * t) } else { b.max(2.0 * t - 1.0) },
        _ => t,
    };
    r.clamp(0.0, 1.0)
}

fn face_centroid(m: &Mesh, f: loom_mesh::FaceId) -> Vec3 {
    let pts = m.face_positions(f).unwrap_or_default();
    if pts.is_empty() { Vec3::ZERO } else { pts.iter().sum::<Vec3>() / pts.len() as f32 }
}

/// A sub-object selection stored on a modifier, with the topology it was
/// made on so a change below can be detected (topology dependence).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StoredSelection {
    pub level: u32,
    pub ids: Vec<u32>,
    pub vertices: usize,
    pub faces: usize,
}

/// One Fillet/Chamfer operation: knots (flat ids at that point), fillet or chamfer, amount.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilletOp {
    pub knots: Vec<u32>,
    pub fillet: bool,
    pub amount: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FilletChamferData {
    pub ids: Vec<u32>,
    pub radius: f32,
    pub distance: f32,
    pub ops: Vec<FilletOp>,
    /// The input's knot count the operations were made on.
    pub knots: usize,
}

impl Default for FilletChamferData {
    fn default() -> Self {
        Self { ids: Vec::new(), radius: 0.0, distance: 0.0, ops: Vec::new(), knots: 0 }
    }
}

/// Vol. Select: a gizmo volume (fitted to the input's bounds) selects vertices or faces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VolSelectData {
    pub gizmo: Transform,
    /// Stack Selection Level: 0 Object, 1 Vertex, 2 Face.
    pub level: u32,
    /// Selection Method: 0 Replace, 1 Add, 2 Subtract.
    pub method: u32,
    pub invert: bool,
    /// Selection Type: false Window, true Crossing.
    pub crossing: bool,
    /// Volume: 0 Box, 1 Sphere, 2 Cylinder, 3 Mesh Object.
    pub volume: u32,
    pub node: Option<crate::node::NodeId>,
    /// Select By: 0 Volume, 1 Material ID, 2 Smoothing Group.
    pub select_by: u32,
    pub material_id: u32,
    pub smoothing_group: u32,
}

impl Default for VolSelectData {
    fn default() -> Self {
        Self { gizmo: Transform::default(), level: 0, method: 0, invert: false, crossing: false, volume: 0, node: None, select_by: 0, material_id: 1, smoothing_group: 1 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Modifier {
    /// XForm: the gizmo transform applied to the incoming selection (or all).
    XForm { gizmo: Transform },
    UnwrapUVW(Box<UnwrapUvw>),
    /// Skin: bones, envelopes and weights (phase 7).
    Skin(Box<crate::skin::SkinData>),
    Morpher(Box<crate::morph::MorpherData>),
    EditNormals(Box<crate::edit_normals::EditNormalsData>),
    SkinMorph(Box<crate::morph::SkinMorphData>),
    SkinWrap(Box<crate::morph::SkinWrapData>),
    /// UVW Map: a projection of `channel` from the gizmo. `length`/`width`/
    /// `height` are the gizmo extents (Y, X, Z); `alignment` 0 X, 1 Y, 2 Z.
    UVWMap {
        projection: Param<u32>,
        cap: Param<bool>,
        length: Param<f32>,
        width: Param<f32>,
        height: Param<f32>,
        u_tile: Param<f32>,
        v_tile: Param<f32>,
        w_tile: Param<f32>,
        u_flip: Param<bool>,
        v_flip: Param<bool>,
        w_flip: Param<bool>,
        real_world: Param<bool>,
        channel: Param<u32>,
        alignment: Param<u32>,
        gizmo: Transform,
    },
    /// UVW XForm: tile / offset / rotate / flip an existing channel.
    UVWXForm {
        u_tile: Param<f32>,
        v_tile: Param<f32>,
        w_tile: Param<f32>,
        u_offset: Param<f32>,
        v_offset: Param<f32>,
        w_offset: Param<f32>,
        rotation: Param<f32>,
        u_flip: Param<bool>,
        v_flip: Param<bool>,
        w_flip: Param<bool>,
        channel: Param<u32>,
    },
    Bend { d: Deformer, angle: Param<f32>, direction: Param<f32> },
    Taper { d: Deformer, amount: Param<f32>, curve: Param<f32>, effect: Param<u32>, symmetry: Param<bool> },
    Twist { d: Deformer, angle: Param<f32>, bias: Param<f32> },
    Noise { d: Deformer, seed: Param<u32>, scale: Param<f32>, fractal: Param<bool>, roughness: Param<f32>, iterations: Param<u32>, strength: [Param<f32>; 3] },
    Stretch { d: Deformer, stretch: Param<f32>, amplify: Param<f32> },
    Squeeze { d: Deformer, axial_amount: Param<f32>, axial_curve: Param<f32>, radial_amount: Param<f32>, radial_curve: Param<f32> },
    Spherify { gizmo: Transform, percent: Param<f32> },
    Wave { d: Deformer, amplitude1: Param<f32>, amplitude2: Param<f32>, wavelength: Param<f32>, phase: Param<f32>, decay: Param<f32> },
    Ripple { d: Deformer, amplitude1: Param<f32>, amplitude2: Param<f32>, wavelength: Param<f32>, phase: Param<f32>, decay: Param<f32> },
    Melt { d: Deformer, amount: Param<f32>, spread: Param<f32>, solidity: Param<f32>, flip: Param<bool> },
    Push { amount: Param<f32> },
    Displace { strength: Param<f32>, decay: Param<f32> },
    Skew { d: Deformer, amount: Param<f32>, direction: Param<f32> },
    /// Mirror Axis 0..5 = X, Y, Z, XY, YZ, ZX.
    Mirror { gizmo: Transform, axis: Param<u32>, offset: Param<f32>, copy: Param<bool> },
    Symmetry { gizmo: Transform, axis: Param<u32>, flip: Param<bool>, slice: Param<bool>, weld: Param<bool>, threshold: Param<f32> },
    /// Slice Type 0..3 = Refine Mesh, Split Mesh, Remove Top, Remove Bottom.
    Slice { gizmo: Transform, mode: Param<u32> },
    /// Geometry 0 = Joints Only, 1 = Struts Only, 2 = Both.
    Lattice { geometry: Param<u32>, strut_radius: Param<f32>, strut_sides: Param<u32>, joint_radius: Param<f32>, joint_segments: Param<u32> },
    /// Linked XForm: follows `control`'s world transform relative to when it was picked.
    LinkedXForm { control: Option<NodeId>, control_inverse_at_pick: Option<[[f32; 4]; 4]> },
    /// Preserve: relaxes toward `original`'s edge lengths.
    Preserve { original: Option<NodeId>, iterations: Param<u32>, weight: Param<f32> },
    /// Normal: Unify / Flip.
    Normal { flip: Param<bool> },
    /// Smooth: Auto Smooth by threshold, or explicit smoothing group bits.
    Smooth {
        auto_smooth: Param<bool>,
        threshold: Param<f32>,
        groups: Param<u32>,
        /// Auto Smooth: faces meeting only at a vertex get different groups too.
        #[serde(default = "param_off")]
        prevent_indirect: Param<bool>,
    },
    /// Poly Select: passes a stored sub-object selection up the stack.
    PolySelect { selection: StoredSelection },
    /// Mesh Select: Vertex / Edge / Face / Polygon / Element (Face and Polygon both select polygons).
    MeshSelect { selection: StoredSelection },
    VolSelect(Box<VolSelectData>),
    /// Spline Select: the stored selection (level 1 vertex, 2 segment, 3 spline) for the input's knot count.
    SplineSelect { level: u32, ids: Vec<u32>, knots: usize },
    DeleteSpline,
    NormalizeSpline { seg_length: f32 },
    RenderableSpline(Box<loom_shape::RenderParams>),
    /// Fillet/Chamfer: its vertex selection and the operations applied, replayed in order.
    FilletChamfer(Box<FilletChamferData>),
    /// Trim/Extend: picked locations (shape space) with Extend on or off, replayed in order.
    TrimExtend { picks: Vec<(Vec3, bool)> },
    /// CrossSection: Spline Options 0 Linear, 1 Smooth, 2 Bezier, 3 Bezier Corner.
    CrossSection { kind: u32 },
    /// Surface: patches over the spline cage.
    Surface { threshold: f32, flip: bool, remove_interior: bool, steps: u32 },
    /// Bevel Profile: the picked profile shape bevels the outline.
    BevelProfile { profile: Option<NodeId>, cap_start: bool, cap_end: bool },
    AffectRegion(Box<crate::deform9::AffectRegionData>),
    /// PathDeform and PathDeform (WSM) (`world`).
    PathDeform(Box<crate::deform9::PathDeformData>),
    VertexWeld { threshold: Param<f32> },
    Substitute { node: Option<NodeId> },
    Flex(Box<crate::deform9::FlexData>),
    SplineIkControl(Box<crate::deform9::SplineIkControlData>),
    /// Camera Map and Camera Map (WSM) (`world`).
    CameraMap(Box<crate::mapping9::CameraMapData>),
    /// MapScaler and MapScaler (WSM) (`world`).
    MapScaler(Box<crate::mapping9::MapScalerData>),
    UvwMappingAdd,
    UvwMappingClear { channel: u32 },
    Hsds(Box<crate::caches9::HsdsData>),
    /// Point Cache and Point Cache (WSM) (`world`).
    PointCache(Box<crate::caches9::PointCacheData>),
    /// FFD 2x2x2 / 3x3x3 / 4x4x4 / FFD(box) / FFD(cyl): `kind` 0–4. The
    /// control `points` are in gizmo space, their `rest` positions and the
    /// lattice volume `lo..hi` (the unit cube of `loom_mesh::ffd`) in object space.
    FFD { kind: u8, dims: [u32; 3], points: Vec<Vec3>, rest: Vec<Vec3>, lo: Vec3, hi: Vec3, gizmo: Transform, deform: Param<u32>, falloff: Param<f32> },
    TurboSmooth { iterations: Param<u32>, render_iters: Param<u32>, render_iters_on: Param<bool>, isoline: Param<bool>, explicit_normals: Param<bool>, sep_smoothing: Param<bool>, sep_materials: Param<bool> },
    MeshSmooth { method: Param<u32>, iterations: Param<u32>, smoothness: Param<f32>, sep_smoothing: Param<bool>, sep_materials: Param<bool> },
    Shell { inner: Param<f32>, outer: Param<f32>, segments: Param<u32>, override_inner: Param<bool>, inner_id: Param<u32>, override_outer: Param<bool>, outer_id: Param<u32>, override_edge: Param<bool>, edge_id: Param<u32>, auto_smooth: Param<bool>, angle: Param<f32> },
    Relax { amount: Param<f32>, iterations: Param<u32>, keep_boundary: Param<bool>, save_outer: Param<bool> },
    Optimize { face_threshold: Param<f32>, edge_threshold: Param<f32>, max_edge_length: Param<f32> },
    ProOptimizer { vertex_percent: Param<f32>, calculated: Param<bool> },
    MultiRes { vertex_percent: Param<f32>, generated: Param<bool> },
    Tessellate { operation: Param<u32>, tension: Param<f32>, iterations: Param<u32> },
    /// Subdivide and Subdivide (WSM) (`world`: Size in world units).
    Subdivide {
        size: Param<f32>,
        #[serde(default)]
        world: bool,
    },
    CapHoles,
    STLCheck { check: Param<bool>, test: Param<u32>, select_faces: Param<bool> },
    DeleteMesh,
    FaceExtrude { amount: Param<f32>, scale: Param<f32> },
    TurnToPoly { keep_convex: Param<bool>, limit_size: Param<bool>, max_size: Param<u32>, threshold: Param<f32> },
    TurnToMesh,
    Material { id: Param<u32> },
    MaterialByElement { random: Param<bool>, id_count: Param<u32>, seed: Param<u32> },
    /// VertexPaint: a colour (illumination / alpha) layer painted per vertex of the input.
    /// `paint` is the layer's colour per painted input vertex and `coverage`
    /// how much of it covers the colour below (a vertex painted before
    /// coverage existed covers fully); the layer blends by `mode`
    /// (`VERTEX_PAINT_MODES`) at `layer_opacity`. `faces`: the face
    /// selection that restricts the brush and Paint All (empty: every face).
    VertexPaint {
        channel: Param<u32>,
        brush_size: Param<f32>,
        opacity: Param<f32>,
        color: [Param<f32>; 3],
        paint: BTreeMap<u32, [f32; 3]>,
        #[serde(default)]
        coverage: BTreeMap<u32, f32>,
        #[serde(default = "param_one")]
        layer_opacity: Param<f32>,
        #[serde(default = "param_zero")]
        mode: Param<u32>,
        #[serde(default)]
        faces: std::collections::BTreeSet<u32>,
    },
    /// Edit Poly: an Editable Poly's editing applied to the stack input. The
    /// edits are kept as the edited mesh plus the input they were made on;
    /// position changes below carry through, topology changes drop them.
    EditPoly { poly: PolyObject, input_vertices: usize, input_faces: usize, snapshot: Vec<Vec3> },
    /// Edit Mesh: Editable Mesh editing (triangles with edge visibility)
    /// applied to the stack input, kept like Edit Poly's.
    EditMesh { poly: PolyObject, input_vertices: usize, input_faces: usize, snapshot: Vec<Vec3> },
    /// Edit Patch: Editable Patch editing applied to the patches below (a
    /// mesh below becomes patches); topology changes below drop the edits.
    EditPatch { patch: crate::node::PatchObject, input_vertices: usize, input_faces: usize },
    /// Shape modifiers (they need a spline below them).
    Extrude { amount: Param<f32>, segments: Param<u32>, cap_start: Param<bool>, cap_end: Param<bool> },
    Lathe { degrees: Param<f32>, segments: Param<u32>, cap_start: Param<bool>, cap_end: Param<bool>, weld_core: Param<bool>, flip_normals: Param<bool>, direction: Param<u32>, align: Param<u32> },
    Bevel { start_outline: Param<f32>, level1: [Param<f32>; 2], level2_on: Param<bool>, level2: [Param<f32>; 2], level3_on: Param<bool>, level3: [Param<f32>; 2], cap_start: Param<bool>, cap_end: Param<bool> },
    Sweep { section: Param<u32>, length: Param<f32>, width: Param<f32>, thickness: Param<f32>, radius: Param<f32>, custom: Option<NodeId>, mirror: Param<bool>, offset_x: Param<f32>, offset_y: Param<f32>, angle: Param<f32>, banking: Param<bool>, smooth_section: Param<bool>, smooth_path: Param<bool> },
    /// Edit Spline: Editable Spline editing applied to the shape below.
    EditSpline { spline: ShapeObject, input_knots: usize, snapshot: Vec<Vec3> },
}

/// Evaluation context: the node's own matrices and a lookup for the nodes
/// Linked XForm / Preserve point at.
#[derive(Default)]
pub struct EvalCtx<'a> {
    pub object_world: Mat4,
    pub object_inverse: Mat4,
    pub lookup: Option<&'a dyn Fn(NodeId) -> Option<(Mat4, Mesh)>>,
    /// Shape of another node (Sweep > Custom Section).
    pub shape_lookup: Option<&'a dyn Fn(NodeId) -> Option<loom_shape::Shape>>,
    /// Another node's evaluated shape, its stack's output (PathDeform path,
    /// Bevel Profile profile); `None` for a node already being evaluated (a cycle).
    pub eval_shape_lookup: Option<&'a dyn Fn(NodeId) -> Option<loom_shape::Shape>>,
    /// Another node's world matrix alone (bones of a Skin).
    pub world_lookup: Option<&'a dyn Fn(NodeId) -> Option<Mat4>>,
    /// Another node's evaluated stack output in world space (Skin Wrap base);
    /// `None` for a node already being evaluated (a cycle).
    pub eval_lookup: Option<&'a dyn Fn(NodeId) -> Option<(Mat4, Mesh)>>,
    /// This node's object matrix at another frame (Flex).
    pub self_world_at: Option<&'a dyn Fn(f32) -> Mat4>,
    /// A camera node's field of view in degrees (Camera Map (WSM)).
    pub camera_fov: Option<&'a dyn Fn(NodeId) -> Option<f32>>,
}

impl EvalCtx<'_> {
    /// World matrix of another node, cheaply (no mesh).
    pub fn node_world_matrix(&self, id: NodeId) -> Option<Mat4> {
        if let Some(l) = self.world_lookup {
            return l(id);
        }
        self.node_world(id)
    }
    /// Another node's evaluated world-space mesh.
    pub fn node_evaluated(&self, id: NodeId) -> Option<(Mat4, Mesh)> {
        self.eval_lookup.and_then(|l| l(id))
    }
    pub(crate) fn node_shape(&self, id: NodeId) -> Option<loom_shape::Shape> {
        self.shape_lookup.and_then(|l| l(id))
    }
    /// Another node's evaluated shape (its base shape without a scene to evaluate in).
    pub(crate) fn node_eval_shape(&self, id: NodeId) -> Option<loom_shape::Shape> {
        match self.eval_shape_lookup {
            Some(l) => l(id),
            None => self.node_shape(id),
        }
    }
    fn node_world(&self, id: NodeId) -> Option<Mat4> {
        self.lookup.and_then(|l| l(id)).map(|(m, _)| m)
    }
    pub(crate) fn node_mesh(&self, id: NodeId) -> Option<Mesh> {
        self.lookup.and_then(|l| l(id)).map(|(_, m)| m)
    }
}

impl Modifier {
    /// The Modifier List (object-space modifiers), in list order; grows
    /// with every task of phase 3.
    pub const LIST: &'static [&'static str] = &[
        "Bend", "Bevel", "Bevel Profile", "Cap Holes", "CrossSection", "Delete Spline", "DeleteMesh", "Displace", "Edit Mesh", "Edit Normals", "Edit Patch", "Edit Poly", "Edit Spline", "Extrude", "Face Extrude", "Fillet/Chamfer", "FFD 2x2x2", "FFD 3x3x3", "FFD 4x4x4", "FFD(box)", "FFD(cyl)", "Lathe", "Lattice", "Linked XForm",
        "Material", "Material By Element", "Melt", "Mesh Select", "MeshSmooth", "Mirror", "Morpher", "MultiRes", "Noise", "Normal", "Normalize Spline", "Optimize", "Poly Select", "Preserve", "ProOptimizer", "Push", "Relax", "Renderable Spline", "Ripple",
        "Shell", "Skew", "Skin", "Skin Morph", "Skin Wrap", "Slice", "Smooth", "Spherify", "Spline Select", "Squeeze", "STL Check", "Stretch", "Subdivide", "Surface", "Sweep", "Symmetry", "Taper", "Tessellate", "Trim/Extend", "TurboSmooth", "Turn To Mesh", "Turn To Poly",
        "Twist", "Unwrap UVW", "UVW Map", "UVW XForm", "VertexPaint", "Vol. Select", "Wave", "XForm",
        "Affect Region", "Flex", "PathDeform", "Spline IK Control", "Substitute", "Vertex Weld",
        "Camera Map", "MapScaler", "UVW Mapping Add", "UVW Mapping Clear", "HSDS", "Point Cache",
    ];

    /// The Modifier List, WORLD-SPACE MODIFIERS group (always applied
    /// above the object-space modifiers).
    pub const WSM_LIST: &'static [&'static str] = &["Camera Map (WSM)", "MapScaler (WSM)", "PathDeform (WSM)", "Point Cache (WSM)", "Subdivide (WSM)"];

    /// Whether the result depends on the node's own placement (world-space
    /// modifiers, Flex), so moving the node re-evaluates the stack.
    pub fn uses_own_transform(&self) -> bool {
        self.is_wsm() || matches!(self, Modifier::Flex(_))
    }

    /// Whether the entry is a world-space modifier.
    pub fn is_wsm(&self) -> bool {
        match self {
            Modifier::PathDeform(d) => d.world,
            Modifier::CameraMap(d) => d.world,
            Modifier::MapScaler(d) => d.world,
            Modifier::PointCache(d) => d.world,
            Modifier::Subdivide { world, .. } => *world,
            _ => false,
        }
    }

    pub fn by_name(name: &str) -> Option<Modifier> {
        let f = Param::new;
        Some(match name {
            "XForm" => Modifier::XForm { gizmo: Transform::default() },
            "Unwrap UVW" => Modifier::UnwrapUVW(Box::default()),
            "Skin" => Modifier::Skin(Box::new(crate::skin::SkinData { bone_affect_limit: 20, always_deform: true, weight_all_vertices: true, ..Default::default() })),
            "Morpher" => Modifier::Morpher(Box::default()),
            "Edit Normals" => Modifier::EditNormals(Box::default()),
            "Skin Morph" => Modifier::SkinMorph(Box::default()),
            "Skin Wrap" => Modifier::SkinWrap(Box::default()),
            "UVW Map" => Modifier::UVWMap {
                projection: Param::new(0), cap: Param::new(true), length: f(1.0), width: f(1.0), height: f(1.0),
                u_tile: f(1.0), v_tile: f(1.0), w_tile: f(1.0), u_flip: Param::new(false), v_flip: Param::new(false), w_flip: Param::new(false),
                real_world: Param::new(false), channel: Param::new(1), alignment: Param::new(2), gizmo: Transform::default(),
            },
            "UVW XForm" => Modifier::UVWXForm {
                u_tile: f(1.0), v_tile: f(1.0), w_tile: f(1.0), u_offset: f(0.0), v_offset: f(0.0), w_offset: f(0.0), rotation: f(0.0),
                u_flip: Param::new(false), v_flip: Param::new(false), w_flip: Param::new(false), channel: Param::new(1),
            },
            "Bend" => Modifier::Bend { d: Deformer::default(), angle: f(0.0), direction: f(0.0) },
            "Taper" => Modifier::Taper { d: Deformer::default(), amount: f(0.0), curve: f(0.0), effect: Param::new(0), symmetry: Param::new(false) },
            "Twist" => Modifier::Twist { d: Deformer::default(), angle: f(0.0), bias: f(0.0) },
            "Noise" => Modifier::Noise { d: Deformer::default(), seed: Param::new(0), scale: f(100.0), fractal: Param::new(false), roughness: f(0.0), iterations: Param::new(6), strength: [f(0.0), f(0.0), f(0.0)] },
            "Stretch" => Modifier::Stretch { d: Deformer::default(), stretch: f(0.0), amplify: f(0.0) },
            "Squeeze" => Modifier::Squeeze { d: Deformer::default(), axial_amount: f(0.0), axial_curve: f(1.0), radial_amount: f(0.0), radial_curve: f(1.0) },
            "Spherify" => Modifier::Spherify { gizmo: Transform::default(), percent: f(100.0) },
            "Wave" => Modifier::Wave { d: Deformer::default(), amplitude1: f(0.0), amplitude2: f(0.0), wavelength: f(10.0), phase: f(0.0), decay: f(0.0) },
            "Ripple" => Modifier::Ripple { d: Deformer::default(), amplitude1: f(0.0), amplitude2: f(0.0), wavelength: f(10.0), phase: f(0.0), decay: f(0.0) },
            "Melt" => Modifier::Melt { d: Deformer::default(), amount: f(0.0), spread: f(0.0), solidity: f(0.8), flip: Param::new(false) },
            "Push" => Modifier::Push { amount: f(0.0) },
            "Displace" => Modifier::Displace { strength: f(0.0), decay: f(0.0) },
            "Skew" => Modifier::Skew { d: Deformer::default(), amount: f(0.0), direction: f(0.0) },
            "Mirror" => Modifier::Mirror { gizmo: Transform::default(), axis: Param::new(0), offset: f(0.0), copy: Param::new(false) },
            "Symmetry" => Modifier::Symmetry { gizmo: Transform::default(), axis: Param::new(0), flip: Param::new(false), slice: Param::new(true), weld: Param::new(true), threshold: f(0.1) },
            "Slice" => Modifier::Slice { gizmo: Transform::default(), mode: Param::new(0) },
            "Lattice" => Modifier::Lattice { geometry: Param::new(2), strut_radius: f(0.2), strut_sides: Param::new(4), joint_radius: f(0.4), joint_segments: Param::new(1) },
            "Linked XForm" => Modifier::LinkedXForm { control: None, control_inverse_at_pick: None },
            "Preserve" => Modifier::Preserve { original: None, iterations: Param::new(10), weight: f(0.5) },
            "Normal" => Modifier::Normal { flip: Param::new(false) },
            "Smooth" => Modifier::Smooth { auto_smooth: Param::new(false), threshold: Param::new(30.0), groups: Param::new(0), prevent_indirect: Param::new(false) },
            "Poly Select" => Modifier::PolySelect { selection: StoredSelection::default() },
            "Mesh Select" => Modifier::MeshSelect { selection: StoredSelection::default() },
            "Vol. Select" => Modifier::VolSelect(Box::default()),
            "Spline Select" => Modifier::SplineSelect { level: 0, ids: Vec::new(), knots: 0 },
            "Delete Spline" => Modifier::DeleteSpline,
            "Fillet/Chamfer" => Modifier::FilletChamfer(Box::default()),
            "Trim/Extend" => Modifier::TrimExtend { picks: Vec::new() },
            "CrossSection" => Modifier::CrossSection { kind: 2 },
            "Surface" => Modifier::Surface { threshold: 1.0, flip: false, remove_interior: false, steps: 5 },
            "Bevel Profile" => Modifier::BevelProfile { profile: None, cap_start: true, cap_end: true },
            "Affect Region" => Modifier::AffectRegion(Box::default()),
            "PathDeform" => Modifier::PathDeform(Box::default()),
            "PathDeform (WSM)" => Modifier::PathDeform(Box::new(crate::deform9::PathDeformData { world: true, ..Default::default() })),
            "Vertex Weld" => Modifier::VertexWeld { threshold: f(0.1) },
            "Substitute" => Modifier::Substitute { node: None },
            "Flex" => Modifier::Flex(Box::default()),
            "Spline IK Control" => Modifier::SplineIkControl(Box::default()),
            "Camera Map" => Modifier::CameraMap(Box::default()),
            "Camera Map (WSM)" => Modifier::CameraMap(Box::new(crate::mapping9::CameraMapData { world: true, ..Default::default() })),
            "MapScaler" => Modifier::MapScaler(Box::default()),
            "MapScaler (WSM)" => Modifier::MapScaler(Box::new(crate::mapping9::MapScalerData { world: true, ..Default::default() })),
            "UVW Mapping Add" => Modifier::UvwMappingAdd,
            "UVW Mapping Clear" => Modifier::UvwMappingClear { channel: 1 },
            "Normalize Spline" => Modifier::NormalizeSpline { seg_length: 20.0 },
            "Renderable Spline" => Modifier::RenderableSpline(Box::new(loom_shape::RenderParams { enable_in_viewport: true, enable_in_renderer: true, ..Default::default() })),
            "FFD 2x2x2" => Modifier::ffd(0),
            "FFD 3x3x3" => Modifier::ffd(1),
            "FFD 4x4x4" => Modifier::ffd(2),
            "FFD(box)" => Modifier::ffd(3),
            "FFD(cyl)" => Modifier::ffd(4),
            "TurboSmooth" => Modifier::TurboSmooth { iterations: Param::new(1), render_iters: Param::new(1), render_iters_on: Param::new(false), isoline: Param::new(false), explicit_normals: Param::new(false), sep_smoothing: Param::new(false), sep_materials: Param::new(false) },
            "MeshSmooth" => Modifier::MeshSmooth { method: Param::new(1), iterations: Param::new(0), smoothness: f(1.0), sep_smoothing: Param::new(false), sep_materials: Param::new(false) },
            "Shell" => Modifier::Shell { inner: f(0.0), outer: f(1.0), segments: Param::new(1), override_inner: Param::new(false), inner_id: Param::new(1), override_outer: Param::new(false), outer_id: Param::new(1), override_edge: Param::new(false), edge_id: Param::new(1), auto_smooth: Param::new(true), angle: f(45.0) },
            "Relax" => Modifier::Relax { amount: f(0.5), iterations: Param::new(1), keep_boundary: Param::new(true), save_outer: Param::new(false) },
            "Optimize" => Modifier::Optimize { face_threshold: f(4.0), edge_threshold: f(1.0), max_edge_length: f(0.0) },
            "ProOptimizer" => Modifier::ProOptimizer { vertex_percent: f(100.0), calculated: Param::new(false) },
            "MultiRes" => Modifier::MultiRes { vertex_percent: f(100.0), generated: Param::new(false) },
            "Tessellate" => Modifier::Tessellate { operation: Param::new(0), tension: f(0.0), iterations: Param::new(1) },
            "Subdivide" => Modifier::Subdivide { size: f(10.0), world: false },
            "Subdivide (WSM)" => Modifier::Subdivide { size: f(10.0), world: true },
            "HSDS" => Modifier::Hsds(Box::default()),
            "Point Cache" => Modifier::PointCache(Box::default()),
            "Point Cache (WSM)" => Modifier::PointCache(Box::new(crate::caches9::PointCacheData { world: true, ..Default::default() })),
            "Cap Holes" => Modifier::CapHoles,
            "STL Check" => Modifier::STLCheck { check: Param::new(false), test: Param::new(4), select_faces: Param::new(false) },
            "DeleteMesh" => Modifier::DeleteMesh,
            "Face Extrude" => Modifier::FaceExtrude { amount: f(0.0), scale: f(100.0) },
            "Turn To Poly" => Modifier::TurnToPoly { keep_convex: Param::new(false), limit_size: Param::new(false), max_size: Param::new(4), threshold: f(15.0) },
            "Turn To Mesh" => Modifier::TurnToMesh,
            "Material" => Modifier::Material { id: Param::new(1) },
            "Material By Element" => Modifier::MaterialByElement { random: Param::new(true), id_count: Param::new(2), seed: Param::new(0) },
            "VertexPaint" => Modifier::VertexPaint { channel: Param::new(0), brush_size: f(20.0), opacity: f(1.0), color: [f(1.0), f(1.0), f(1.0)], paint: BTreeMap::new(), coverage: BTreeMap::new(), layer_opacity: f(1.0), mode: Param::new(0), faces: Default::default() },
            "Edit Poly" => Modifier::EditPoly { poly: PolyObject::from_mesh(Mesh::default()), input_vertices: 0, input_faces: 0, snapshot: Vec::new() },
            "Edit Mesh" => Modifier::EditMesh { poly: PolyObject::from_mesh(Mesh::default()), input_vertices: 0, input_faces: 0, snapshot: Vec::new() },
            "Edit Patch" => Modifier::EditPatch { patch: crate::node::PatchObject::from_patch(Default::default()), input_vertices: 0, input_faces: 0 },
            "Extrude" => Modifier::Extrude { amount: f(0.0), segments: Param::new(1), cap_start: Param::new(true), cap_end: Param::new(true) },
            "Lathe" => Modifier::Lathe { degrees: f(360.0), segments: Param::new(16), cap_start: Param::new(true), cap_end: Param::new(true), weld_core: Param::new(false), flip_normals: Param::new(false), direction: Param::new(1), align: Param::new(3) },
            "Bevel" => Modifier::Bevel { start_outline: f(0.0), level1: [f(0.0), f(0.0)], level2_on: Param::new(false), level2: [f(0.0), f(0.0)], level3_on: Param::new(false), level3: [f(0.0), f(0.0)], cap_start: Param::new(true), cap_end: Param::new(true) },
            "Sweep" => Modifier::Sweep { section: Param::new(0), length: f(3.0), width: f(3.0), thickness: f(0.5), radius: f(1.0), custom: None, mirror: Param::new(false), offset_x: f(0.0), offset_y: f(0.0), angle: f(0.0), banking: Param::new(true), smooth_section: Param::new(true), smooth_path: Param::new(true) },
            "Edit Spline" => Modifier::EditSpline { spline: ShapeObject::from_shape(loom_shape::Shape::default()), input_knots: 0, snapshot: Vec::new() },
            _ => return None,
        })
    }

    pub fn name(&self) -> &'static str {
        match self {
            Modifier::XForm { .. } => "XForm",
            Modifier::UnwrapUVW(_) => "Unwrap UVW",
            Modifier::Skin(_) => "Skin",
            Modifier::Morpher(_) => "Morpher",
            Modifier::EditNormals(_) => "Edit Normals",
            Modifier::SkinMorph(_) => "Skin Morph",
            Modifier::SkinWrap(_) => "Skin Wrap",
            Modifier::UVWMap { .. } => "UVW Map",
            Modifier::UVWXForm { .. } => "UVW XForm",
            Modifier::Bend { .. } => "Bend",
            Modifier::Taper { .. } => "Taper",
            Modifier::Twist { .. } => "Twist",
            Modifier::Noise { .. } => "Noise",
            Modifier::Stretch { .. } => "Stretch",
            Modifier::Squeeze { .. } => "Squeeze",
            Modifier::Spherify { .. } => "Spherify",
            Modifier::Wave { .. } => "Wave",
            Modifier::Ripple { .. } => "Ripple",
            Modifier::Melt { .. } => "Melt",
            Modifier::Push { .. } => "Push",
            Modifier::Displace { .. } => "Displace",
            Modifier::Skew { .. } => "Skew",
            Modifier::Mirror { .. } => "Mirror",
            Modifier::Symmetry { .. } => "Symmetry",
            Modifier::Slice { .. } => "Slice",
            Modifier::Lattice { .. } => "Lattice",
            Modifier::LinkedXForm { .. } => "Linked XForm",
            Modifier::Preserve { .. } => "Preserve",
            Modifier::Normal { .. } => "Normal",
            Modifier::Smooth { .. } => "Smooth",
            Modifier::PolySelect { .. } => "Poly Select",
            Modifier::MeshSelect { .. } => "Mesh Select",
            Modifier::VolSelect(_) => "Vol. Select",
            Modifier::SplineSelect { .. } => "Spline Select",
            Modifier::DeleteSpline => "Delete Spline",
            Modifier::FilletChamfer(_) => "Fillet/Chamfer",
            Modifier::TrimExtend { .. } => "Trim/Extend",
            Modifier::CrossSection { .. } => "CrossSection",
            Modifier::Surface { .. } => "Surface",
            Modifier::BevelProfile { .. } => "Bevel Profile",
            Modifier::AffectRegion(_) => "Affect Region",
            Modifier::PathDeform(d) => if d.world { "PathDeform (WSM)" } else { "PathDeform" },
            Modifier::VertexWeld { .. } => "Vertex Weld",
            Modifier::Substitute { .. } => "Substitute",
            Modifier::Flex(_) => "Flex",
            Modifier::SplineIkControl(_) => "Spline IK Control",
            Modifier::CameraMap(d) => if d.world { "Camera Map (WSM)" } else { "Camera Map" },
            Modifier::MapScaler(d) => if d.world { "MapScaler (WSM)" } else { "MapScaler" },
            Modifier::UvwMappingAdd => "UVW Mapping Add",
            Modifier::UvwMappingClear { .. } => "UVW Mapping Clear",
            Modifier::NormalizeSpline { .. } => "Normalize Spline",
            Modifier::RenderableSpline(_) => "Renderable Spline",
            Modifier::FFD { kind, .. } => Self::FFD_NAMES[(*kind as usize).min(4)],
            Modifier::TurboSmooth { .. } => "TurboSmooth",
            Modifier::MeshSmooth { .. } => "MeshSmooth",
            Modifier::Shell { .. } => "Shell",
            Modifier::Relax { .. } => "Relax",
            Modifier::Optimize { .. } => "Optimize",
            Modifier::ProOptimizer { .. } => "ProOptimizer",
            Modifier::MultiRes { .. } => "MultiRes",
            Modifier::Tessellate { .. } => "Tessellate",
            Modifier::Subdivide { world, .. } => if *world { "Subdivide (WSM)" } else { "Subdivide" },
            Modifier::Hsds(_) => "HSDS",
            Modifier::PointCache(d) => if d.world { "Point Cache (WSM)" } else { "Point Cache" },
            Modifier::CapHoles => "Cap Holes",
            Modifier::STLCheck { .. } => "STL Check",
            Modifier::DeleteMesh => "DeleteMesh",
            Modifier::FaceExtrude { .. } => "Face Extrude",
            Modifier::TurnToPoly { .. } => "Turn To Poly",
            Modifier::TurnToMesh => "Turn To Mesh",
            Modifier::Material { .. } => "Material",
            Modifier::MaterialByElement { .. } => "Material By Element",
            Modifier::VertexPaint { .. } => "VertexPaint",
            Modifier::EditPoly { .. } => "Edit Poly",
            Modifier::EditMesh { .. } => "Edit Mesh",
            Modifier::EditPatch { .. } => "Edit Patch",
            Modifier::Extrude { .. } => "Extrude",
            Modifier::Lathe { .. } => "Lathe",
            Modifier::Bevel { .. } => "Bevel",
            Modifier::Sweep { .. } => "Sweep",
            Modifier::EditSpline { .. } => "Edit Spline",
        }
    }

    /// Modifiers that need a shape (spline) flowing in (refused on meshes).
    pub fn needs_shape(&self) -> bool {
        matches!(self, Modifier::Extrude { .. } | Modifier::Lathe { .. } | Modifier::Bevel { .. } | Modifier::Sweep { .. } | Modifier::EditSpline { .. } | Modifier::Surface { .. } | Modifier::BevelProfile { .. }) || self.passes_shape()
    }

    /// Spline modifiers that pass a (changed) shape on to the entry above.
    pub fn passes_shape(&self) -> bool {
        matches!(self, Modifier::EditSpline { .. } | Modifier::SplineSelect { .. } | Modifier::DeleteSpline | Modifier::NormalizeSpline { .. } | Modifier::RenderableSpline(_) | Modifier::FilletChamfer(_) | Modifier::TrimExtend { .. } | Modifier::CrossSection { .. } | Modifier::SplineIkControl(_))
    }

    /// Modifiers holding sub-object data by index (edits, selections, maps),
    /// which a topology change below breaks.
    pub fn topology_dependent(&self) -> bool {
        matches!(self, Modifier::EditPoly { .. } | Modifier::EditMesh { .. } | Modifier::EditPatch { .. } | Modifier::EditSpline { .. } | Modifier::PolySelect { .. } | Modifier::MeshSelect { .. } | Modifier::SplineSelect { .. } | Modifier::UnwrapUVW(_) | Modifier::EditNormals(_) | Modifier::Hsds(_) | Modifier::VertexPaint { .. } | Modifier::FilletChamfer(_))
    }

    /// Edit Spline: the shape data the entry edits.
    pub fn edit_spline(&self) -> Option<&ShapeObject> {
        match self {
            Modifier::EditSpline { spline, .. } => Some(spline),
            _ => None,
        }
    }

    pub fn edit_spline_mut(&mut self) -> Option<&mut ShapeObject> {
        match self {
            Modifier::EditSpline { spline, .. } => Some(spline),
            _ => None,
        }
    }

    /// Edit Spline: starts editing from the shape below the entry.
    pub fn init_edit_spline(&mut self, input: &loom_shape::Shape) {
        if let Modifier::EditSpline { spline, input_knots, snapshot } = self {
            *spline = ShapeObject::from_shape(input.clone());
            *input_knots = input.knot_count();
            *snapshot = input.knot_positions();
        }
    }

    /// Edit Poly / Edit Mesh: the Editable Poly / Mesh data the entry edits.
    pub fn edit_poly(&self) -> Option<&PolyObject> {
        match self {
            Modifier::EditPoly { poly, .. } | Modifier::EditMesh { poly, .. } => Some(poly),
            _ => None,
        }
    }

    pub fn edit_poly_mut(&mut self) -> Option<&mut PolyObject> {
        match self {
            Modifier::EditPoly { poly, .. } | Modifier::EditMesh { poly, .. } => Some(poly),
            _ => None,
        }
    }

    /// Stores edited Editable Poly / Mesh data in the entry; an Edit Mesh
    /// keeps its mesh triangles (cut again, edges keep their visibility).
    pub fn set_edit_poly(&mut self, new: PolyObject) {
        match self {
            Modifier::EditPoly { poly, .. } => *poly = new,
            Modifier::EditMesh { poly, .. } => {
                let mesh = loom_mesh::tri_mesh::retriangulate_like(&poly.mesh, new.mesh.clone()).unwrap_or_else(|_| new.mesh.clone());
                *poly = new.with_mesh(mesh);
            }
            _ => {}
        }
    }

    /// Edit Patch: the Editable Patch data the entry edits.
    pub fn edit_patch(&self) -> Option<&crate::node::PatchObject> {
        match self {
            Modifier::EditPatch { patch, .. } => Some(patch),
            _ => None,
        }
    }

    pub fn edit_patch_mut(&mut self) -> Option<&mut crate::node::PatchObject> {
        match self {
            Modifier::EditPatch { patch, .. } => Some(patch),
            _ => None,
        }
    }

    /// Edit Patch: starts editing from the stack input (its patches, else
    /// the mesh made patches).
    pub fn init_edit_patch(&mut self, input: &Mesh, input_patch: Option<&loom_mesh::patch::PatchMesh>) {
        if let Modifier::EditPatch { patch, input_vertices, input_faces } = self {
            let lattice = input_patch.cloned().unwrap_or_else(|| loom_mesh::patch::PatchMesh::from_mesh(input));
            *patch = crate::node::PatchObject::from_patch(lattice);
            *input_vertices = input.vertex_count();
            *input_faces = input.face_count();
        }
    }

    /// Edit Poly / Edit Mesh: starts editing from the stack input below the
    /// entry. An Edit Mesh takes the input as triangles (polygons cut, cuts
    /// invisible); an Edit Poly takes an Editable Mesh's triangles merged
    /// into its polygons when the vertices stay the same.
    pub fn init_edit_poly(&mut self, input: &Mesh) {
        if let Modifier::EditPoly { poly, input_vertices, input_faces, snapshot } | Modifier::EditMesh { poly, input_vertices, input_faces, snapshot } = self {
            *poly = PolyObject::from_mesh(input.clone());
            *input_vertices = input.vertex_count();
            *input_faces = input.face_count();
            *snapshot = input.vertices().iter().map(|v| v.position).collect();
        }
        let edited = match self {
            Modifier::EditMesh { poly, .. } => loom_mesh::tri_mesh::from_poly(&poly.mesh).ok(),
            Modifier::EditPoly { poly, .. } if poly.mesh.has_invisible_edges() => loom_mesh::tri_mesh::to_poly(&poly.mesh).ok().filter(|m| m.vertex_count() == poly.mesh.vertex_count()),
            _ => None,
        };
        if let (Some(m), Some(p)) = (edited, self.edit_poly_mut()) {
            p.mesh = m;
        }
    }

    /// STL Check: the report of the entry's input (shown in its rollout).
    pub fn stl_report(&self, input: &Mesh) -> loom_mesh::mesh_mods::StlReport {
        loom_mesh::mesh_mods::stl_check(input)
    }

    pub const FFD_NAMES: [&'static str; 5] = ["FFD 2x2x2", "FFD 3x3x3", "FFD 4x4x4", "FFD(box)", "FFD(cyl)"];

    /// An unfitted FFD; `fit_ffd` sizes its lattice to the input.
    fn ffd(kind: u8) -> Modifier {
        let dims = match kind { 0 => [2, 2, 2], 1 => [3, 3, 3], 2 => [4, 4, 4], 3 => [4, 4, 4], _ => [8, 3, 4] };
        Modifier::FFD { kind, dims, points: Vec::new(), rest: Vec::new(), lo: Vec3::ZERO, hi: Vec3::ONE, gizmo: Transform::default(), deform: Param::new(0), falloff: Param::new(0.0) }
    }

    /// Fits an FFD lattice to the bounds of its input (object space): the
    /// control points go to their rest grid over the volume.
    /// UVW Map > Fit: the gizmo takes the alignment axis (its Z = object X /
    /// Y / Z), sits at the centre of the input bounds and spans them.
    pub fn fit_uvw_map(&mut self, input: &Mesh) {
        if let Modifier::UVWMap { length, width, height, alignment, gizmo, .. } = self {
            let rot = uvw_alignment_rotation(alignment.get());
            gizmo.set_rotation_quat(rot);
            let inv = glam::Mat4::from_quat(rot).inverse();
            let (lo, hi) = if input.vertex_count() == 0 { (Vec3::ZERO, Vec3::ONE) } else {
                let mut lo = Vec3::splat(f32::INFINITY);
                let mut hi = Vec3::splat(f32::NEG_INFINITY);
                for v in input.vertices() {
                    let p = inv.transform_point3(v.position);
                    lo = lo.min(p);
                    hi = hi.max(p);
                }
                (lo, hi)
            };
            let size = (hi - lo).max(Vec3::splat(1e-4));
            let centre = rot * ((lo + hi) * 0.5);
            gizmo.position.set(centre);
            gizmo.scale.set(Vec3::ONE);
            width.set(size.x);
            length.set(size.y);
            height.set(size.z);
        }
    }

    /// UVW Map > Center: the gizmo moves to the centre of the input bounds.
    pub fn center_uvw_map(&mut self, input: &Mesh) {
        if let Modifier::UVWMap { gizmo, .. } = self {
            if input.vertex_count() > 0 {
                let (lo, hi) = input.aabb();
                gizmo.position.set((lo + hi) * 0.5);
            }
        }
    }

    pub fn fit_ffd(&mut self, lo_in: Vec3, hi_in: Vec3) {
        if let Modifier::FFD { kind, dims, points, rest, lo, hi, .. } = self {
            *lo = lo_in;
            *hi = hi_in;
            let size = (hi_in - lo_in).max(Vec3::splat(1e-6));
            *rest = loom_mesh::ffd::rest_points(*dims, *kind == 4).into_iter().map(|u| lo_in + u * size).collect();
            *points = rest.clone();
        }
    }

    /// Set Number of Points: rebuilds the lattice grid over the same volume.
    pub fn set_ffd_dims(&mut self, new_dims: [u32; 3]) -> Result<(), String> {
        let Modifier::FFD { dims, lo, hi, .. } = self else { return Err("not an FFD".into()) };
        if new_dims.iter().any(|d| *d < 2 || *d > 32) {
            return Err("each dimension needs 2..=32 points".into());
        }
        *dims = new_dims;
        let (l, h) = (*lo, *hi);
        self.fit_ffd(l, h);
        Ok(())
    }

    /// Sub-object levels of the modifier in order ("" = none).
    pub fn sub_levels(&self) -> &'static [&'static str] {
        match self {
            Modifier::PolySelect { .. } | Modifier::EditPoly { .. } => &["Vertex", "Edge", "Border", "Polygon", "Element"],
            Modifier::MeshSelect { .. } | Modifier::EditMesh { .. } => &["Vertex", "Edge", "Face", "Polygon", "Element"],
            Modifier::EditPatch { .. } => &["Vertex", "Handle", "Edge", "Patch", "Element"],
            Modifier::SplineSelect { .. } => &["Vertex", "Segment", "Spline"],
            Modifier::FilletChamfer(_) => &["Vertex"],
            Modifier::EditSpline { .. } => &["Vertex", "Segment", "Spline"],
            Modifier::Slice { .. } => &["Slice Plane"],
            Modifier::Mirror { .. } => &["Mirror Center"],
            Modifier::Symmetry { .. } => &["Mirror"],
            Modifier::FFD { .. } => &["Control Points", "Lattice", "Set Volume"],
            Modifier::UVWMap { .. } => &["Gizmo"],
            Modifier::UnwrapUVW(_) => &["Vertex", "Edge", "Face"],
            Modifier::Skin(_) => &["Envelope"],
            Modifier::EditNormals(_) => &["Normal"],
            Modifier::AffectRegion(_) => &["Points"],
            Modifier::Hsds(_) => &["Vertex", "Edge", "Polygon", "Element"],
            m if m.gizmo().is_some() => &["Gizmo", "Center"],
            _ => &[],
        }
    }

    /// The gizmo transform of modifiers that have one.
    pub fn gizmo(&self) -> Option<&Transform> {
        match self {
            Modifier::XForm { gizmo } | Modifier::Spherify { gizmo, .. } | Modifier::Mirror { gizmo, .. } | Modifier::Symmetry { gizmo, .. } | Modifier::Slice { gizmo, .. } | Modifier::FFD { gizmo, .. } | Modifier::UVWMap { gizmo, .. } => Some(gizmo),
            Modifier::VolSelect(d) => Some(&d.gizmo),
            Modifier::AffectRegion(d) => Some(&d.gizmo),
            Modifier::Bend { d, .. } | Modifier::Taper { d, .. } | Modifier::Twist { d, .. } | Modifier::Noise { d, .. } | Modifier::Stretch { d, .. } | Modifier::Squeeze { d, .. }
            | Modifier::Wave { d, .. } | Modifier::Ripple { d, .. } | Modifier::Melt { d, .. } | Modifier::Skew { d, .. } => Some(&d.gizmo),
            _ => None,
        }
    }

    /// The gizmo, Limits and Center of the deformation modifiers.
    pub fn deformer(&self) -> Option<&Deformer> {
        match self {
            Modifier::Bend { d, .. } | Modifier::Taper { d, .. } | Modifier::Twist { d, .. } | Modifier::Noise { d, .. } | Modifier::Stretch { d, .. } | Modifier::Squeeze { d, .. }
            | Modifier::Wave { d, .. } | Modifier::Ripple { d, .. } | Modifier::Melt { d, .. } | Modifier::Skew { d, .. } => Some(d),
            _ => None,
        }
    }

    pub fn gizmo_mut(&mut self) -> Option<&mut Transform> {
        match self {
            Modifier::XForm { gizmo } | Modifier::Spherify { gizmo, .. } | Modifier::Mirror { gizmo, .. } | Modifier::Symmetry { gizmo, .. } | Modifier::Slice { gizmo, .. } | Modifier::FFD { gizmo, .. } | Modifier::UVWMap { gizmo, .. } => Some(gizmo),
            Modifier::VolSelect(d) => Some(&mut d.gizmo),
            Modifier::AffectRegion(d) => Some(&mut d.gizmo),
            Modifier::Bend { d, .. } | Modifier::Taper { d, .. } | Modifier::Twist { d, .. } | Modifier::Noise { d, .. } | Modifier::Stretch { d, .. } | Modifier::Squeeze { d, .. }
            | Modifier::Wave { d, .. } | Modifier::Ripple { d, .. } | Modifier::Melt { d, .. } | Modifier::Skew { d, .. } => Some(&mut d.gizmo),
            _ => None,
        }
    }

    /// Choice labels for enumerated Int parameters (radio buttons in the rollout).
    pub fn choices(&self, name: &str) -> Option<&'static [&'static str]> {
        match (self, name) {
            (Modifier::Taper { .. }, "Effect") => Some(&["XY", "X", "Y"]),
            (Modifier::PathDeform(_), "Path Deform Axis") => Some(&["X", "Y", "Z"]),
            (Modifier::PointCache(_), "Playback Type") => Some(&["Original Range", "Custom Start"]),
            (Modifier::SplineIkControl(_), "Link Types") => Some(&["Link All in Hierarchy", "Link All to Root", "No Linking"]),
            (Modifier::SplineIkControl(_), "Helper Display") => Some(&["Center Marker", "Axis Tripod", "Cross", "Box"]),
            (Modifier::Mirror { .. }, "Mirror Axis") => Some(&["X", "Y", "Z", "XY", "YZ", "ZX"]),
            (Modifier::UVWMap { .. }, "Mapping") => Some(&["Planar", "Cylindrical", "Spherical", "Shrink Wrap", "Box", "Face", "XYZ to UVW"]),
            (Modifier::UVWMap { .. }, "Alignment") => Some(&["X", "Y", "Z"]),
            (Modifier::UnwrapUVW(_), "Map Kind") => Some(&["Planar", "Pelt", "Cylindrical", "Spherical", "Box"]),
            (Modifier::Symmetry { .. }, "Mirror Axis") => Some(&["X", "Y", "Z"]),
            (Modifier::Slice { .. }, "Slice Type") => Some(&["Refine Mesh", "Split Mesh", "Remove Top", "Remove Bottom"]),
            (Modifier::FFD { .. }, "Deform") => Some(&["Only In Volume", "All Vertices"]),
            (Modifier::MeshSmooth { .. }, "Subdivision Method") => Some(&["Classic", "NURMS", "Quad Output"]),
            (Modifier::Tessellate { .. }, "Operation") => Some(&["Edge", "Face"]),
            (Modifier::STLCheck { .. }, "Test") => Some(&["Open Edge", "Double Face", "Spike", "Multiple Edge", "Everything"]),
            (Modifier::VertexPaint { .. }, "Channel") => Some(&["Vertex Color", "Vertex Illum", "Vertex Alpha"]),
            (Modifier::VertexPaint { .. }, "Mode") => Some(&VERTEX_PAINT_MODES),
            (Modifier::Lathe { .. }, "Direction") => Some(&["X", "Y", "Z"]),
            (Modifier::Lathe { .. }, "Align") => Some(&["Min", "Center", "Max"]),
            (Modifier::Sweep { .. }, "Section") => Some(&["Angle", "Bar", "Channel", "Cylinder", "Half Round", "Pipe", "Quarter Round", "Tee", "Tube", "Wide Flange", "Custom"]),
            (Modifier::Lattice { .. }, "Geometry") => Some(&["Joints Only from Vertices", "Struts Only from Edges", "Both"]),
            (_, n) if n.ends_with(" Axis") => Some(&["X", "Y", "Z"]),
            _ => None,
        }
    }

    /// Every other node whose transform or evaluated mesh this modifier reads
    /// (bones of a Skin, …): their versions key the evaluation cache.
    pub fn referenced_nodes(&self) -> Vec<NodeId> {
        match self {
            Modifier::Skin(d) => d.bones.iter().map(|b| b.node).collect(),
            Modifier::Morpher(d) => d.channels.iter().flat_map(|c| c.targets.iter().filter_map(|t| t.node)).collect(),
            Modifier::SkinMorph(d) => d.bones.iter().flat_map(|b| std::iter::once(b.node).chain(b.parent)).collect(),
            Modifier::SkinWrap(d) => d.bases.clone(),
            Modifier::SplineIkControl(d) => d.helpers.iter().flatten().copied().collect(),
            _ => self.picked_node().into_iter().collect(),
        }
    }

    /// The node reference some modifiers pick (Linked XForm control, Preserve original).
    pub fn picked_node(&self) -> Option<NodeId> {
        match self {
            Modifier::LinkedXForm { control, .. } => *control,
            Modifier::Preserve { original, .. } => *original,
            Modifier::Sweep { custom, .. } => *custom,
            Modifier::VolSelect(d) => d.node,
            Modifier::BevelProfile { profile, .. } => *profile,
            Modifier::PathDeform(d) => d.path,
            Modifier::Substitute { node } => *node,
            Modifier::CameraMap(d) => d.camera,
            _ => None,
        }
    }

    /// Rewrites every node reference (Merge gives nodes fresh ids; `None`
    /// drops a reference to a node that was not merged).
    pub fn remap_ids(&mut self, f: &dyn Fn(NodeId) -> Option<NodeId>) {
        match self {
            Modifier::Morpher(d) => {
                for ch in &mut d.channels {
                    for t in &mut ch.targets {
                        t.node = t.node.and_then(f);
                    }
                }
            }
            Modifier::SkinWrap(d) => {
                let keep: Vec<bool> = d.bases.iter().map(|b| f(*b).is_some()).collect();
                if keep.iter().any(|k| !k) {
                    d.bases.retain(|b| f(*b).is_some());
                    d.binds.clear();
                    d.base_counts.clear();
                }
                for b in &mut d.bases {
                    *b = f(*b).unwrap();
                }
            }
            Modifier::SkinMorph(d) => {
                d.bones.retain(|b| f(b.node).is_some());
                for b in &mut d.bones {
                    b.node = f(b.node).unwrap();
                    b.parent = b.parent.and_then(f);
                }
                let n = d.bones.len();
                d.morphs.retain(|m| m.bone < n);
            }
            Modifier::Skin(d) => {
                let keep: Vec<bool> = d.bones.iter().map(|b| f(b.node).is_some()).collect();
                for (i, b) in d.bones.iter_mut().enumerate() {
                    if keep[i] {
                        b.node = f(b.node).unwrap();
                    }
                }
                for i in (0..keep.len()).rev() {
                    if !keep[i] {
                        d.remove_bone(i);
                    }
                }
            }
            Modifier::LinkedXForm { control, control_inverse_at_pick } => {
                *control = control.and_then(f);
                if control.is_none() {
                    *control_inverse_at_pick = None;
                }
            }
            Modifier::Preserve { original, .. } => *original = original.and_then(f),
            Modifier::Sweep { custom, .. } => *custom = custom.and_then(f),
            Modifier::VolSelect(d) => d.node = d.node.and_then(f),
            Modifier::BevelProfile { profile, .. } => *profile = profile.and_then(f),
            Modifier::PathDeform(d) => d.path = d.path.and_then(f),
            Modifier::Substitute { node } => *node = node.and_then(f),
            Modifier::CameraMap(d) => d.camera = d.camera.and_then(f),
            Modifier::SplineIkControl(d) => {
                for h in &mut d.helpers {
                    *h = h.and_then(f);
                }
            }
            _ => {}
        }
    }

    /// The rollout spinners, rollout names, rollout order.
    pub fn parameters(&self) -> Vec<(&'static str, ParamValue)> {
        use ParamValue::*;
        let lim = |d: &Deformer, axis_name: &'static str| -> Vec<(&'static str, ParamValue)> {
            vec![(axis_name, Int(d.axis.get())), ("Limit Effect", Bool(d.limit.get())), ("Upper Limit", Float(d.upper.get())), ("Lower Limit", Float(d.lower.get()))]
        };
        match self {
            Modifier::XForm { .. } => Vec::new(),
            Modifier::Bend { d, angle, direction } => [vec![("Angle", Float(angle.get())), ("Direction", Float(direction.get()))], lim(d, "Bend Axis")].concat(),
            Modifier::Taper { d, amount, curve, effect, symmetry } => [vec![("Amount", Float(amount.get())), ("Curve", Float(curve.get()))], lim(d, "Primary Axis"), vec![("Effect", Int(effect.get())), ("Symmetry", Bool(symmetry.get()))]].concat(),
            Modifier::Twist { d, angle, bias } => [vec![("Angle", Float(angle.get())), ("Bias", Float(bias.get()))], lim(d, "Twist Axis")].concat(),
            Modifier::Noise { d, seed, scale, fractal, roughness, iterations, strength } => [vec![("Seed", Int(seed.get())), ("Scale", Float(scale.get())), ("Fractal", Bool(fractal.get())), ("Roughness", Float(roughness.get())), ("Iterations", Int(iterations.get())), ("Strength X", Float(strength[0].get())), ("Strength Y", Float(strength[1].get())), ("Strength Z", Float(strength[2].get()))], lim(d, "Noise Axis")].concat(),
            Modifier::Stretch { d, stretch, amplify } => [vec![("Stretch", Float(stretch.get())), ("Amplify", Float(amplify.get()))], lim(d, "Stretch Axis")].concat(),
            Modifier::Squeeze { d, axial_amount, axial_curve, radial_amount, radial_curve } => [vec![("Axial Bulge Amount", Float(axial_amount.get())), ("Axial Bulge Curve", Float(axial_curve.get())), ("Radial Squeeze Amount", Float(radial_amount.get())), ("Radial Squeeze Curve", Float(radial_curve.get()))], lim(d, "Squeeze Axis")].concat(),
            Modifier::Spherify { percent, .. } => vec![("Percent", Float(percent.get()))],
            Modifier::Wave { d, amplitude1, amplitude2, wavelength, phase, decay } | Modifier::Ripple { d, amplitude1, amplitude2, wavelength, phase, decay } => {
                let _ = d;
                vec![("Amplitude 1", Float(amplitude1.get())), ("Amplitude 2", Float(amplitude2.get())), ("Wave Length", Float(wavelength.get())), ("Phase", Float(phase.get())), ("Decay", Float(decay.get()))]
            }
            Modifier::Melt { d, amount, spread, solidity, flip } => [vec![("Amount", Float(amount.get())), ("Spread", Float(spread.get())), ("Solidity", Float(solidity.get()))], lim(d, "Melt Axis"), vec![("Flip Axis", Bool(flip.get()))]].concat(),
            Modifier::Push { amount } => vec![("Push Value", Float(amount.get()))],
            Modifier::Displace { strength, decay } => vec![("Strength", Float(strength.get())), ("Decay", Float(decay.get()))],
            Modifier::Skew { d, amount, direction } => [vec![("Amount", Float(amount.get())), ("Direction", Float(direction.get()))], lim(d, "Skew Axis")].concat(),
            Modifier::Mirror { axis, offset, copy, .. } => vec![("Mirror Axis", Int(axis.get())), ("Offset", Float(offset.get())), ("Copy", Bool(copy.get()))],
            Modifier::UVWMap { projection, cap, length, width, height, u_tile, v_tile, w_tile, u_flip, v_flip, w_flip, real_world, channel, alignment, .. } => vec![
                ("Mapping", Int(projection.get())), ("Cap", Bool(cap.get())), ("Length", Float(length.get())), ("Width", Float(width.get())), ("Height", Float(height.get())),
                ("U Tile", Float(u_tile.get())), ("V Tile", Float(v_tile.get())), ("W Tile", Float(w_tile.get())),
                ("U Flip", Bool(u_flip.get())), ("V Flip", Bool(v_flip.get())), ("W Flip", Bool(w_flip.get())), ("Real-World Map Size", Bool(real_world.get())),
                ("Channel", Int(channel.get())), ("Alignment", Int(alignment.get())),
            ],
            Modifier::Morpher(d) => vec![("Use Vertex Selection", Bool(d.use_vertex_selection)), ("Spinner Min", Float(d.spinner_min)), ("Spinner Max", Float(d.spinner_max))],
            Modifier::EditNormals(d) => vec![("Display Length", Float(d.display_length)), ("Ignore Backfacing", Bool(d.ignore_backfacing)), ("Use Threshold", Bool(d.use_threshold)), ("Threshold", Float(d.threshold))],
            Modifier::SkinMorph(_) => Vec::new(),
            Modifier::SkinWrap(d) => vec![("Deformation Engine", Int(d.engine)), ("Falloff", Float(d.falloff)), ("Distance Influence", Float(d.distance_influence)), ("Face Limit", Int(d.face_limit)), ("Threshold", Float(d.threshold)), ("Weight All Points", Bool(d.weight_all_points))],
            Modifier::Skin(d) => vec![
                ("Bone Affect Limit", Int(d.bone_affect_limit)), ("Always Deform", Bool(d.always_deform)), ("Weight All Vertices", Bool(d.weight_all_vertices)),
                ("Show Colored Vertices", Bool(d.display.colored_vertices)), ("Show Colored Faces", Bool(d.display.colored_faces)), ("Color All Weights", Bool(d.display.color_all_weights)),
                ("Show All Envelopes", Bool(d.display.all_envelopes)), ("Show All Gizmos", Bool(d.display.all_gizmos)), ("Show No Envelopes", Bool(d.display.no_envelopes)),
                ("Draw CrossSections On Top", Bool(d.display.cross_sections_on_top)), ("Draw Envelope On Top", Bool(d.display.envelope_on_top)),
                ("Mirror Mode", Bool(d.mirror.mode)), ("Mirror Plane", Int(d.mirror.plane)), ("Mirror Offset", Float(d.mirror.offset)), ("Mirror Threshold", Float(d.mirror.threshold)),
            ],
            Modifier::UnwrapUVW(u) => vec![
                ("Map Channel", Int(u.channel.get())), ("Ignore Backfacing", Bool(u.ignore_backfacing.get())), ("Select By Element", Bool(u.select_by_element.get())),
                ("Planar Angle", Float(u.planar_angle.get())), ("Select MatID", Int(u.select_matid.get())), ("Select SG", Int(u.select_sg.get())),
                ("Show Seams", Bool(u.show_seams.get())), ("Thin Seam Display", Bool(u.thin_seams.get())), ("Always Show Pelt Seam", Bool(u.always_show_pelt_seam.get())),
                ("Prevent Reflattening", Bool(u.prevent_reflattening.get())), ("Map Kind", Int(u.map_kind.get())), ("Normalize Map", Bool(u.normalize_map.get())),
            ],
            Modifier::UVWXForm { u_tile, v_tile, w_tile, u_offset, v_offset, w_offset, rotation, u_flip, v_flip, w_flip, channel } => vec![
                ("U Tile", Float(u_tile.get())), ("V Tile", Float(v_tile.get())), ("W Tile", Float(w_tile.get())),
                ("U Offset", Float(u_offset.get())), ("V Offset", Float(v_offset.get())), ("W Offset", Float(w_offset.get())), ("Rotation", Float(rotation.get())),
                ("U Flip", Bool(u_flip.get())), ("V Flip", Bool(v_flip.get())), ("W Flip", Bool(w_flip.get())), ("Channel", Int(channel.get())),
            ],
            Modifier::Symmetry { axis, flip, slice, weld, threshold, .. } => vec![("Mirror Axis", Int(axis.get())), ("Flip", Bool(flip.get())), ("Slice Along Mirror", Bool(slice.get())), ("Weld Seam", Bool(weld.get())), ("Threshold", Float(threshold.get()))],
            Modifier::Slice { mode, .. } => vec![("Slice Type", Int(mode.get()))],
            Modifier::Lattice { geometry, strut_radius, strut_sides, joint_radius, joint_segments } => vec![("Geometry", Int(geometry.get())), ("Strut Radius", Float(strut_radius.get())), ("Strut Sides", Int(strut_sides.get())), ("Joint Radius", Float(joint_radius.get())), ("Joint Segments", Int(joint_segments.get()))],
            Modifier::LinkedXForm { .. } => Vec::new(),
            Modifier::Preserve { iterations, weight, .. } => vec![("Iterations", Int(iterations.get())), ("Weight", Float(weight.get()))],
            Modifier::Normal { flip } => vec![("Flip Normals", Bool(flip.get()))],
            Modifier::Smooth { auto_smooth, threshold, groups, prevent_indirect } => vec![
                ("Auto Smooth", Bool(auto_smooth.get())),
                ("Prevent Indirect Smoothing", Bool(prevent_indirect.get())),
                ("Threshold", Float(threshold.get())),
                ("Smoothing Groups", Int(groups.get())),
            ],
            Modifier::PolySelect { .. } | Modifier::MeshSelect { .. } | Modifier::SplineSelect { .. } | Modifier::DeleteSpline => Vec::new(),
            Modifier::VolSelect(d) => vec![
                ("Stack Selection Level", Int(d.level)), ("Method", Int(d.method)), ("Invert", Bool(d.invert)), ("Selection Type", Int(d.crossing as u32)),
                ("Volume", Int(d.volume)), ("Select By", Int(d.select_by)), ("Material ID", Int(d.material_id)), ("Smoothing Group", Int(d.smoothing_group)),
            ],
            Modifier::NormalizeSpline { seg_length } => vec![("Seg Length", Float(*seg_length))],
            Modifier::FilletChamfer(d) => vec![("Radius", Float(d.radius)), ("Distance", Float(d.distance))],
            Modifier::TrimExtend { .. } => Vec::new(),
            Modifier::CrossSection { kind } => vec![("Spline Options", Int(*kind))],
            Modifier::Surface { threshold, flip, remove_interior, steps } => vec![("Threshold", Float(*threshold)), ("Flip Normals", Bool(*flip)), ("Remove Interior Patches", Bool(*remove_interior)), ("Steps", Int(*steps))],
            Modifier::BevelProfile { cap_start, cap_end, .. } => vec![("Cap Start", Bool(*cap_start)), ("Cap End", Bool(*cap_end))],
            Modifier::AffectRegion(d) => vec![("Falloff", Float(d.falloff.get())), ("Pinch", Float(d.pinch.get())), ("Bubble", Float(d.bubble.get())), ("Ignore Back Facing", Bool(d.ignore_back_facing.get()))],
            Modifier::PathDeform(d) => vec![("Percent", Float(d.percent.get())), ("Stretch", Float(d.stretch.get())), ("Rotation", Float(d.rotation.get())), ("Twist", Float(d.twist.get())), ("Path Deform Axis", Int(d.axis.get())), ("Flip", Bool(d.flip.get()))],
            Modifier::VertexWeld { threshold } => vec![("Threshold", Float(threshold.get()))],
            Modifier::Substitute { .. } => Vec::new(),
            Modifier::Flex(d) => vec![
                ("Flex", Float(d.flex.get())), ("Strength", Float(d.strength.get())), ("Sways", Float(d.sways.get())), ("Chase Springs", Bool(d.chase_springs)),
                ("Use Weights", Bool(d.use_weights)), ("Samples", Int(d.samples)), ("Reference Frame", Int(d.reference_frame.max(0) as u32)),
            ],
            Modifier::SplineIkControl(d) => vec![("Link Types", Int(d.link_type)), ("Helper Display", Int(d.display)), ("Helper Size", Float(d.helper_size))],
            Modifier::CameraMap(d) => vec![("Map Channel", Int(d.channel)), ("Vertex Color Channel", Bool(d.vertex_color))],
            Modifier::MapScaler(d) => vec![("Scale", Float(d.scale.get())), ("U Offset", Float(d.u_offset.get())), ("V Offset", Float(d.v_offset.get())), ("Wrap Texture", Bool(d.wrap)), ("Map Channel", Int(d.channel))],
            Modifier::UvwMappingAdd => Vec::new(),
            Modifier::UvwMappingClear { channel } => vec![("Map Channel", Int(*channel))],
            Modifier::RenderableSpline(r) => vec![
                ("Enable In Renderer", Bool(r.enable_in_renderer)), ("Enable In Viewport", Bool(r.enable_in_viewport)), ("Rectangular", Bool(r.rectangular)),
                ("Thickness", Float(r.thickness)), ("Sides", Int(r.sides)), ("Angle", Float(r.angle)), ("Length", Float(r.length)), ("Width", Float(r.width)),
                ("Auto Smooth", Bool(r.auto_smooth)), ("Threshold", Float(r.auto_smooth_threshold)),
            ],
            Modifier::FFD { deform, falloff, .. } => vec![("Deform", Int(deform.get())), ("Falloff", Float(falloff.get()))],
            Modifier::TurboSmooth { iterations, render_iters, render_iters_on, isoline, explicit_normals, sep_smoothing, sep_materials } => vec![
                ("Iterations", Int(iterations.get())), ("Render Iters On", Bool(render_iters_on.get())), ("Render Iters", Int(render_iters.get())), ("Isoline Display", Bool(isoline.get())),
                ("Explicit Normals", Bool(explicit_normals.get())), ("Smoothing Groups", Bool(sep_smoothing.get())), ("Materials", Bool(sep_materials.get())),
            ],
            Modifier::MeshSmooth { method, iterations, smoothness, sep_smoothing, sep_materials } => vec![
                ("Subdivision Method", Int(method.get())), ("Iterations", Int(iterations.get())), ("Smoothness", Float(smoothness.get())), ("Smoothing Groups", Bool(sep_smoothing.get())), ("Materials", Bool(sep_materials.get())),
            ],
            Modifier::Shell { inner, outer, segments, override_inner, inner_id, override_outer, outer_id, override_edge, edge_id, auto_smooth, angle } => vec![
                ("Inner Amount", Float(inner.get())), ("Outer Amount", Float(outer.get())), ("Segments", Int(segments.get())),
                ("Override Inner Mat ID", Bool(override_inner.get())), ("Inner Mat ID", Int(inner_id.get())), ("Override Outer Mat ID", Bool(override_outer.get())), ("Outer Mat ID", Int(outer_id.get())),
                ("Override Edge Mat ID", Bool(override_edge.get())), ("Edge Mat ID", Int(edge_id.get())), ("Auto Smooth Edge", Bool(auto_smooth.get())), ("Angle", Float(angle.get())),
            ],
            Modifier::Relax { amount, iterations, keep_boundary, save_outer } => vec![("Relax Value", Float(amount.get())), ("Iterations", Int(iterations.get())), ("Keep Boundary Pts Fixed", Bool(keep_boundary.get())), ("Save Outer Corners", Bool(save_outer.get()))],
            Modifier::Optimize { face_threshold, edge_threshold, max_edge_length } => vec![("Face Thresh", Float(face_threshold.get())), ("Edge Thresh", Float(edge_threshold.get())), ("Max Edge Len", Float(max_edge_length.get()))],
            Modifier::ProOptimizer { vertex_percent, calculated } => vec![("Vertex %", Float(vertex_percent.get())), ("Calculate", Bool(calculated.get()))],
            Modifier::MultiRes { vertex_percent, generated } => vec![("Vert Percent", Float(vertex_percent.get())), ("Generate", Bool(generated.get()))],
            Modifier::Tessellate { operation, tension, iterations } => vec![("Operation", Int(operation.get())), ("Tension", Float(tension.get())), ("Iterations", Int(iterations.get()))],
            Modifier::Subdivide { size, .. } => vec![("Size", Float(size.get()))],
            Modifier::Hsds(d) => vec![("Smooth Result", Bool(d.smooth_result))],
            Modifier::PointCache(d) => vec![("Strength", Float(d.strength.get())), ("Relative Offset", Bool(d.relative)), ("Playback Type", Int(d.playback)), ("Start Frame", Float(d.start_frame))],
            Modifier::CapHoles | Modifier::DeleteMesh | Modifier::TurnToMesh => Vec::new(),
            Modifier::STLCheck { check, test, select_faces } => vec![("Check", Bool(check.get())), ("Test", Int(test.get())), ("Select Faces", Bool(select_faces.get()))],
            Modifier::FaceExtrude { amount, scale } => vec![("Amount", Float(amount.get())), ("Scale", Float(scale.get()))],
            Modifier::TurnToPoly { keep_convex, limit_size, max_size, threshold } => vec![
                ("Keep Polygons Convex", Bool(keep_convex.get())), ("Limit Polygon Size", Bool(limit_size.get())), ("Max Size", Int(max_size.get())), ("Planar Thresh", Float(threshold.get())),
            ],
            Modifier::Material { id } => vec![("Material ID", Int(id.get()))],
            Modifier::MaterialByElement { random, id_count, seed } => vec![("Random Distribution", Bool(random.get())), ("ID Count", Int(id_count.get())), ("Seed", Int(seed.get()))],
            Modifier::VertexPaint { channel, brush_size, opacity, color, layer_opacity, mode, .. } => vec![
                ("Channel", Int(channel.get())), ("Brush Size", Float(brush_size.get())), ("Opacity", Float(opacity.get())), ("Color R", Float(color[0].get())), ("Color G", Float(color[1].get())), ("Color B", Float(color[2].get())),
                ("Layer Opacity", Float(layer_opacity.get())), ("Mode", Int(mode.get())),
            ],
            Modifier::EditPoly { .. } | Modifier::EditMesh { .. } | Modifier::EditSpline { .. } => Vec::new(),
            Modifier::EditPatch { patch, .. } => vec![("View Steps", Int(patch.steps.get())), ("Show Interior Edges", Bool(patch.show_interior))],
            Modifier::Extrude { amount, segments, cap_start, cap_end } => vec![
                ("Amount", Float(amount.get())), ("Segments", Int(segments.get())), ("Cap Start", Bool(cap_start.get())), ("Cap End", Bool(cap_end.get())),
            ],
            Modifier::Lathe { degrees, segments, cap_start, cap_end, weld_core, flip_normals, direction, align } => vec![
                ("Degrees", Float(degrees.get())), ("Weld Core", Bool(weld_core.get())), ("Flip Normals", Bool(flip_normals.get())), ("Segments", Int(segments.get())),
                ("Cap Start", Bool(cap_start.get())), ("Cap End", Bool(cap_end.get())), ("Direction", Int(direction.get())), ("Align", Int(align.get())),
            ],
            Modifier::Bevel { start_outline, level1, level2_on, level2, level3_on, level3, cap_start, cap_end } => vec![
                ("Cap Start", Bool(cap_start.get())), ("Cap End", Bool(cap_end.get())), ("Start Outline", Float(start_outline.get())),
                ("Level 1 Height", Float(level1[0].get())), ("Level 1 Outline", Float(level1[1].get())),
                ("Level 2", Bool(level2_on.get())), ("Level 2 Height", Float(level2[0].get())), ("Level 2 Outline", Float(level2[1].get())),
                ("Level 3", Bool(level3_on.get())), ("Level 3 Height", Float(level3[0].get())), ("Level 3 Outline", Float(level3[1].get())),
            ],
            Modifier::Sweep { section, length, width, thickness, radius, mirror, offset_x, offset_y, angle, banking, smooth_section, smooth_path, .. } => vec![
                ("Section", Int(section.get())), ("Length", Float(length.get())), ("Width", Float(width.get())), ("Thickness", Float(thickness.get())), ("Radius", Float(radius.get())),
                ("Mirror", Bool(mirror.get())), ("Offset X", Float(offset_x.get())), ("Offset Y", Float(offset_y.get())), ("Angle", Float(angle.get())),
                ("Banking", Bool(banking.get())), ("Smooth Section", Bool(smooth_section.get())), ("Smooth Path", Bool(smooth_path.get())),
            ],
        }
    }

    pub fn set_parameter(&mut self, name: &str, value: f64) -> Result<(), String> {
        let v = value as f32;
        let b = value != 0.0;
        let u = value.max(0.0) as u32;
        // Shared Limit / axis fields of the deformers.
        let axis_param = name.ends_with(" Axis") && name != "Flip Axis";
        if let Some(d) = self.deformer_mut() {
            match name {
                _ if axis_param => { d.axis.set(u.min(2)); return Ok(()); }
                "Limit Effect" => { d.limit.set(b); return Ok(()); }
                "Upper Limit" => { d.upper.set(v); return Ok(()); }
                "Lower Limit" => { d.lower.set(v); return Ok(()); }
                _ => {}
            }
        }
        match (self, name) {
            (Modifier::Bend { angle, .. }, "Angle") => angle.set(v),
            (Modifier::Bend { direction, .. }, "Direction") => direction.set(v),
            (Modifier::Taper { amount, .. }, "Amount") => amount.set(v),
            (Modifier::Taper { curve, .. }, "Curve") => curve.set(v),
            (Modifier::Taper { effect, .. }, "Effect") => effect.set(u.min(2)),
            (Modifier::Taper { symmetry, .. }, "Symmetry") => symmetry.set(b),
            (Modifier::Twist { angle, .. }, "Angle") => angle.set(v),
            (Modifier::Twist { bias, .. }, "Bias") => bias.set(v.clamp(-100.0, 100.0)),
            (Modifier::Noise { seed, .. }, "Seed") => seed.set(u),
            (Modifier::Noise { scale, .. }, "Scale") => scale.set(v.max(0.001)),
            (Modifier::Noise { fractal, .. }, "Fractal") => fractal.set(b),
            (Modifier::Noise { roughness, .. }, "Roughness") => roughness.set(v.clamp(0.0, 1.0)),
            (Modifier::Noise { iterations, .. }, "Iterations") => iterations.set(u.clamp(1, 10)),
            (Modifier::Noise { strength, .. }, "Strength X") => strength[0].set(v),
            (Modifier::Noise { strength, .. }, "Strength Y") => strength[1].set(v),
            (Modifier::Noise { strength, .. }, "Strength Z") => strength[2].set(v),
            (Modifier::Stretch { stretch, .. }, "Stretch") => stretch.set(v),
            (Modifier::Stretch { amplify, .. }, "Amplify") => amplify.set(v),
            (Modifier::Squeeze { axial_amount, .. }, "Axial Bulge Amount") => axial_amount.set(v),
            (Modifier::Squeeze { axial_curve, .. }, "Axial Bulge Curve") => axial_curve.set(v),
            (Modifier::Squeeze { radial_amount, .. }, "Radial Squeeze Amount") => radial_amount.set(v),
            (Modifier::Squeeze { radial_curve, .. }, "Radial Squeeze Curve") => radial_curve.set(v),
            (Modifier::Spherify { percent, .. }, "Percent") => percent.set(v.clamp(0.0, 100.0)),
            (Modifier::Wave { amplitude1, .. } | Modifier::Ripple { amplitude1, .. }, "Amplitude 1") => amplitude1.set(v),
            (Modifier::Wave { amplitude2, .. } | Modifier::Ripple { amplitude2, .. }, "Amplitude 2") => amplitude2.set(v),
            (Modifier::Wave { wavelength, .. } | Modifier::Ripple { wavelength, .. }, "Wave Length") => wavelength.set(v),
            (Modifier::Wave { phase, .. } | Modifier::Ripple { phase, .. }, "Phase") => phase.set(v),
            (Modifier::Wave { decay, .. } | Modifier::Ripple { decay, .. }, "Decay") => decay.set(v.max(0.0)),
            (Modifier::Melt { amount, .. }, "Amount") => amount.set(v.max(0.0)),
            (Modifier::Melt { spread, .. }, "Spread") => spread.set(v.max(0.0)),
            (Modifier::Melt { solidity, .. }, "Solidity") => solidity.set(v.clamp(0.0, 1.0)),
            (Modifier::Melt { flip, .. }, "Flip Axis") => flip.set(b),
            (Modifier::Push { amount }, "Push Value") => amount.set(v),
            (Modifier::Displace { strength, .. }, "Strength") => strength.set(v),
            (Modifier::Displace { decay, .. }, "Decay") => decay.set(v),
            (Modifier::Skew { amount, .. }, "Amount") => amount.set(v),
            (Modifier::Skew { direction, .. }, "Direction") => direction.set(v),
            (Modifier::Mirror { axis, .. }, "Mirror Axis") => axis.set(u.min(5)),
            (Modifier::UVWMap { projection, .. }, "Mapping") => projection.set(u.min(6)),
            (Modifier::UVWMap { cap, .. }, "Cap") => cap.set(b),
            (Modifier::UVWMap { length, .. }, "Length") => length.set(v.max(1e-4)),
            (Modifier::UVWMap { width, .. }, "Width") => width.set(v.max(1e-4)),
            (Modifier::UVWMap { height, .. }, "Height") => height.set(v.max(1e-4)),
            (Modifier::UVWMap { u_tile, .. } | Modifier::UVWXForm { u_tile, .. }, "U Tile") => u_tile.set(v),
            (Modifier::UVWMap { v_tile, .. } | Modifier::UVWXForm { v_tile, .. }, "V Tile") => v_tile.set(v),
            (Modifier::UVWMap { w_tile, .. } | Modifier::UVWXForm { w_tile, .. }, "W Tile") => w_tile.set(v),
            (Modifier::UVWMap { u_flip, .. } | Modifier::UVWXForm { u_flip, .. }, "U Flip") => u_flip.set(b),
            (Modifier::UVWMap { v_flip, .. } | Modifier::UVWXForm { v_flip, .. }, "V Flip") => v_flip.set(b),
            (Modifier::UVWMap { w_flip, .. } | Modifier::UVWXForm { w_flip, .. }, "W Flip") => w_flip.set(b),
            (Modifier::UVWMap { real_world, .. }, "Real-World Map Size") => real_world.set(b),
            (Modifier::UVWMap { channel, .. } | Modifier::UVWXForm { channel, .. }, "Channel") => channel.set(u.min(99)),
            (Modifier::UVWMap { alignment, .. }, "Alignment") => alignment.set(u.min(2)),
            (Modifier::UnwrapUVW(u), "Map Channel") => {
                if u.channel.get() != value.max(0.0) as u32 {
                    u.channel.set(value.clamp(0.0, 99.0) as u32);
                    u.edits = None;
                    u.tv_selection.clear();
                    u.tv_edge_selection.clear();
                }
            }
            (Modifier::SkinWrap(d), "Deformation Engine") => d.engine = u.min(1),
            (Modifier::SkinWrap(d), "Falloff") => d.falloff = v.max(0.0),
            (Modifier::SkinWrap(d), "Distance Influence") => d.distance_influence = v.max(0.001),
            (Modifier::SkinWrap(d), "Face Limit") => d.face_limit = u.max(1),
            (Modifier::SkinWrap(d), "Threshold") => d.threshold = v.max(0.0),
            (Modifier::SkinWrap(d), "Weight All Points") => d.weight_all_points = b,
            (Modifier::Morpher(d), "Use Vertex Selection") => d.use_vertex_selection = b,
            (Modifier::Morpher(d), "Spinner Min") => d.spinner_min = v,
            (Modifier::Morpher(d), "Spinner Max") => d.spinner_max = v,
            (Modifier::EditNormals(d), "Display Length") => d.display_length = v.max(0.0),
            (Modifier::VolSelect(d), "Stack Selection Level") => d.level = u.min(2),
            (Modifier::VolSelect(d), "Method") => d.method = u.min(2),
            (Modifier::VolSelect(d), "Invert") => d.invert = b,
            (Modifier::VolSelect(d), "Selection Type") => d.crossing = b,
            (Modifier::VolSelect(d), "Volume") => d.volume = u.min(3),
            (Modifier::VolSelect(d), "Select By") => d.select_by = u.min(2),
            (Modifier::VolSelect(d), "Material ID") => d.material_id = u.max(1),
            (Modifier::VolSelect(d), "Smoothing Group") => d.smoothing_group = u.clamp(1, 32),
            (Modifier::NormalizeSpline { seg_length }, "Seg Length") => *seg_length = v.max(0.001),
            (Modifier::FilletChamfer(d), "Radius") => d.radius = v.max(0.0),
            (Modifier::FilletChamfer(d), "Distance") => d.distance = v.max(0.0),
            (Modifier::CrossSection { kind }, "Spline Options") => *kind = u.min(3),
            (Modifier::Surface { threshold, .. }, "Threshold") => *threshold = v.max(0.0),
            (Modifier::Surface { flip, .. }, "Flip Normals") => *flip = b,
            (Modifier::Surface { remove_interior, .. }, "Remove Interior Patches") => *remove_interior = b,
            (Modifier::Surface { steps, .. }, "Steps") => *steps = u.min(100),
            (Modifier::EditPatch { patch, .. }, "View Steps") => patch.steps.set(u.min(100)),
            (Modifier::EditPatch { patch, .. }, "Show Interior Edges") => patch.show_interior = b,
            (Modifier::BevelProfile { cap_start, .. }, "Cap Start") => *cap_start = b,
            (Modifier::BevelProfile { cap_end, .. }, "Cap End") => *cap_end = b,
            (Modifier::AffectRegion(d), "Falloff") => d.falloff.set(v.max(0.0)),
            (Modifier::AffectRegion(d), "Pinch") => d.pinch.set(v),
            (Modifier::AffectRegion(d), "Bubble") => d.bubble.set(v),
            (Modifier::AffectRegion(d), "Ignore Back Facing") => d.ignore_back_facing.set(b),
            (Modifier::PathDeform(d), "Percent") => d.percent.set(v),
            (Modifier::PathDeform(d), "Stretch") => d.stretch.set(v),
            (Modifier::PathDeform(d), "Rotation") => d.rotation.set(v),
            (Modifier::PathDeform(d), "Twist") => d.twist.set(v),
            (Modifier::PathDeform(d), "Path Deform Axis") => d.axis.set(u.min(2)),
            (Modifier::PathDeform(d), "Flip") => d.flip.set(b),
            (Modifier::VertexWeld { threshold }, "Threshold") => threshold.set(v.max(0.0)),
            (Modifier::Flex(d), "Flex") => d.flex.set(v),
            (Modifier::Flex(d), "Strength") => d.strength.set(v.max(0.0)),
            (Modifier::Flex(d), "Sways") => d.sways.set(v.max(0.0)),
            (Modifier::Flex(d), "Chase Springs") => d.chase_springs = b,
            (Modifier::Flex(d), "Use Weights") => d.use_weights = b,
            (Modifier::Flex(d), "Samples") => d.samples = u.clamp(1, 100),
            (Modifier::Flex(d), "Reference Frame") => d.reference_frame = value as i32,
            (Modifier::SplineIkControl(d), "Link Types") => d.link_type = u.min(2),
            (Modifier::SplineIkControl(d), "Helper Display") => d.display = u.min(3),
            (Modifier::SplineIkControl(d), "Helper Size") => d.helper_size = v.max(0.001),
            (Modifier::CameraMap(d), "Map Channel") => d.channel = u.clamp(1, 99),
            (Modifier::CameraMap(d), "Vertex Color Channel") => d.vertex_color = b,
            (Modifier::MapScaler(d), "Scale") => d.scale.set(v.max(0.001)),
            (Modifier::MapScaler(d), "U Offset") => d.u_offset.set(v),
            (Modifier::MapScaler(d), "V Offset") => d.v_offset.set(v),
            (Modifier::MapScaler(d), "Wrap Texture") => d.wrap = b,
            (Modifier::MapScaler(d), "Map Channel") => d.channel = u.clamp(1, 99),
            (Modifier::UvwMappingClear { channel }, "Map Channel") => *channel = u.min(99),
            (Modifier::RenderableSpline(r), "Enable In Renderer") => r.enable_in_renderer = b,
            (Modifier::RenderableSpline(r), "Enable In Viewport") => r.enable_in_viewport = b,
            (Modifier::RenderableSpline(r), "Rectangular") => r.rectangular = b,
            (Modifier::RenderableSpline(r), "Thickness") => r.thickness = v.max(0.0),
            (Modifier::RenderableSpline(r), "Sides") => r.sides = u.clamp(3, 100),
            (Modifier::RenderableSpline(r), "Angle") => r.angle = v,
            (Modifier::RenderableSpline(r), "Length") => r.length = v.max(0.0),
            (Modifier::RenderableSpline(r), "Width") => r.width = v.max(0.0),
            (Modifier::RenderableSpline(r), "Auto Smooth") => r.auto_smooth = b,
            (Modifier::RenderableSpline(r), "Threshold") => r.auto_smooth_threshold = v,
            (Modifier::EditNormals(d), "Ignore Backfacing") => d.ignore_backfacing = b,
            (Modifier::EditNormals(d), "Use Threshold") => d.use_threshold = b,
            (Modifier::EditNormals(d), "Threshold") => d.threshold = v.max(0.0),
            (Modifier::Skin(d), "Bone Affect Limit") => d.bone_affect_limit = u.max(1),
            (Modifier::Skin(d), "Always Deform") => d.always_deform = b,
            (Modifier::Skin(d), "Weight All Vertices") => d.weight_all_vertices = b,
            (Modifier::Skin(d), "Show Colored Vertices") => d.display.colored_vertices = b,
            (Modifier::Skin(d), "Show Colored Faces") => d.display.colored_faces = b,
            (Modifier::Skin(d), "Color All Weights") => d.display.color_all_weights = b,
            (Modifier::Skin(d), "Show All Envelopes") => d.display.all_envelopes = b,
            (Modifier::Skin(d), "Show All Gizmos") => d.display.all_gizmos = b,
            (Modifier::Skin(d), "Show No Envelopes") => d.display.no_envelopes = b,
            (Modifier::Skin(d), "Draw CrossSections On Top") => d.display.cross_sections_on_top = b,
            (Modifier::Skin(d), "Draw Envelope On Top") => d.display.envelope_on_top = b,
            (Modifier::Skin(d), "Mirror Mode") => d.mirror.mode = b,
            (Modifier::Skin(d), "Mirror Plane") => d.mirror.plane = u.min(2),
            (Modifier::Skin(d), "Mirror Offset") => d.mirror.offset = v,
            (Modifier::Skin(d), "Mirror Threshold") => d.mirror.threshold = v.max(0.0),
            (Modifier::UnwrapUVW(u), "Ignore Backfacing") => u.ignore_backfacing.set(b),
            (Modifier::UnwrapUVW(u), "Select By Element") => u.select_by_element.set(b),
            (Modifier::UnwrapUVW(u), "Planar Angle") => u.planar_angle.set(v.clamp(0.0, 180.0)),
            (Modifier::UnwrapUVW(u), "Select MatID") => u.select_matid.set(value.clamp(1.0, 65535.0) as u32),
            (Modifier::UnwrapUVW(u), "Select SG") => u.select_sg.set(value.clamp(1.0, 32.0) as u32),
            (Modifier::UnwrapUVW(u), "Show Seams") => u.show_seams.set(b),
            (Modifier::UnwrapUVW(u), "Thin Seam Display") => u.thin_seams.set(b),
            (Modifier::UnwrapUVW(u), "Always Show Pelt Seam") => u.always_show_pelt_seam.set(b),
            (Modifier::UnwrapUVW(u), "Prevent Reflattening") => u.prevent_reflattening.set(b),
            (Modifier::UnwrapUVW(u), "Map Kind") => u.map_kind.set(value.clamp(0.0, 4.0) as u32),
            (Modifier::UnwrapUVW(u), "Normalize Map") => u.normalize_map.set(b),
            (Modifier::UVWXForm { u_offset, .. }, "U Offset") => u_offset.set(v),
            (Modifier::UVWXForm { v_offset, .. }, "V Offset") => v_offset.set(v),
            (Modifier::UVWXForm { w_offset, .. }, "W Offset") => w_offset.set(v),
            (Modifier::UVWXForm { rotation, .. }, "Rotation") => rotation.set(v),
            (Modifier::Mirror { offset, .. }, "Offset") => offset.set(v),
            (Modifier::Mirror { copy, .. }, "Copy") => copy.set(b),
            (Modifier::Symmetry { axis, .. }, "Mirror Axis") => axis.set(u.min(2)),
            (Modifier::Symmetry { flip, .. }, "Flip") => flip.set(b),
            (Modifier::Symmetry { slice, .. }, "Slice Along Mirror") => slice.set(b),
            (Modifier::Symmetry { weld, .. }, "Weld Seam") => weld.set(b),
            (Modifier::Symmetry { threshold, .. }, "Threshold") => threshold.set(v.max(0.0)),
            (Modifier::Slice { mode, .. }, "Slice Type") => mode.set(u.min(3)),
            (Modifier::Lattice { geometry, .. }, "Geometry") => geometry.set(u.min(2)),
            (Modifier::Lattice { strut_radius, .. }, "Strut Radius") => strut_radius.set(v.max(0.001)),
            (Modifier::Lattice { strut_sides, .. }, "Strut Sides") => strut_sides.set(u.clamp(3, 32)),
            (Modifier::Lattice { joint_radius, .. }, "Joint Radius") => joint_radius.set(v.max(0.001)),
            (Modifier::Lattice { joint_segments, .. }, "Joint Segments") => joint_segments.set(u.clamp(1, 4)),
            (Modifier::Preserve { iterations, .. }, "Iterations") => iterations.set(u.clamp(1, 100)),
            (Modifier::Preserve { weight, .. }, "Weight") => weight.set(v.clamp(0.0, 1.0)),
            (Modifier::Normal { flip, .. }, "Flip Normals") => flip.set(b),
            (Modifier::Smooth { auto_smooth, .. }, "Auto Smooth") => auto_smooth.set(b),
            (Modifier::Smooth { prevent_indirect, .. }, "Prevent Indirect Smoothing") => prevent_indirect.set(b),
            (Modifier::Smooth { threshold, .. }, "Threshold") => threshold.set(v.clamp(0.0, 180.0)),
            (Modifier::Smooth { groups, .. }, "Smoothing Groups") => groups.set(value.max(0.0) as u32),
            (Modifier::FFD { deform, .. }, "Deform") => deform.set(u.min(1)),
            (Modifier::FFD { falloff, .. }, "Falloff") => falloff.set(v.max(0.0)),
            (Modifier::TurboSmooth { iterations, .. }, "Iterations") => iterations.set(u.min(6)),
            (Modifier::TurboSmooth { render_iters, .. }, "Render Iters") => render_iters.set(u.min(6)),
            (Modifier::TurboSmooth { render_iters_on, .. }, "Render Iters On") => render_iters_on.set(b),
            (Modifier::TurboSmooth { isoline, .. }, "Isoline Display") => isoline.set(b),
            (Modifier::TurboSmooth { explicit_normals, .. }, "Explicit Normals") => explicit_normals.set(b),
            (Modifier::TurboSmooth { sep_smoothing, .. } | Modifier::MeshSmooth { sep_smoothing, .. }, "Smoothing Groups") => sep_smoothing.set(b),
            (Modifier::TurboSmooth { sep_materials, .. } | Modifier::MeshSmooth { sep_materials, .. }, "Materials") => sep_materials.set(b),
            (Modifier::MeshSmooth { method, .. }, "Subdivision Method") => method.set(u.min(2)),
            (Modifier::MeshSmooth { iterations, .. }, "Iterations") => iterations.set(u.min(6)),
            (Modifier::MeshSmooth { smoothness, .. }, "Smoothness") => smoothness.set(v.clamp(0.0, 1.0)),
            (Modifier::Shell { inner, .. }, "Inner Amount") => inner.set(v),
            (Modifier::Shell { outer, .. }, "Outer Amount") => outer.set(v),
            (Modifier::Shell { segments, .. }, "Segments") => segments.set(u.clamp(1, 100)),
            (Modifier::Shell { override_inner, .. }, "Override Inner Mat ID") => override_inner.set(b),
            (Modifier::Shell { inner_id, .. }, "Inner Mat ID") => inner_id.set(u.max(1)),
            (Modifier::Shell { override_outer, .. }, "Override Outer Mat ID") => override_outer.set(b),
            (Modifier::Shell { outer_id, .. }, "Outer Mat ID") => outer_id.set(u.max(1)),
            (Modifier::Shell { override_edge, .. }, "Override Edge Mat ID") => override_edge.set(b),
            (Modifier::Shell { edge_id, .. }, "Edge Mat ID") => edge_id.set(u.max(1)),
            (Modifier::Shell { auto_smooth, .. }, "Auto Smooth Edge") => auto_smooth.set(b),
            (Modifier::Shell { angle, .. }, "Angle") => angle.set(v.clamp(0.0, 180.0)),
            (Modifier::Relax { amount, .. }, "Relax Value") => amount.set(v.clamp(-1.0, 1.0)),
            (Modifier::Relax { iterations, .. }, "Iterations") => iterations.set(u.min(100)),
            (Modifier::Relax { keep_boundary, .. }, "Keep Boundary Pts Fixed") => keep_boundary.set(b),
            (Modifier::Relax { save_outer, .. }, "Save Outer Corners") => save_outer.set(b),
            (Modifier::Optimize { face_threshold, .. }, "Face Thresh") => face_threshold.set(v.clamp(0.0, 90.0)),
            (Modifier::Optimize { edge_threshold, .. }, "Edge Thresh") => edge_threshold.set(v.clamp(0.0, 90.0)),
            (Modifier::Optimize { max_edge_length, .. }, "Max Edge Len") => max_edge_length.set(v.max(0.0)),
            (Modifier::ProOptimizer { vertex_percent, .. }, "Vertex %") => vertex_percent.set(v.clamp(1.0, 100.0)),
            (Modifier::ProOptimizer { calculated, .. }, "Calculate") => calculated.set(b),
            (Modifier::MultiRes { vertex_percent, .. }, "Vert Percent") => vertex_percent.set(v.clamp(1.0, 100.0)),
            (Modifier::MultiRes { generated, .. }, "Generate") => generated.set(b),
            (Modifier::Tessellate { operation, .. }, "Operation") => operation.set(u.min(1)),
            (Modifier::Tessellate { tension, .. }, "Tension") => tension.set(v.clamp(-100.0, 100.0)),
            (Modifier::Tessellate { iterations, .. }, "Iterations") => iterations.set(u.clamp(1, 4)),
            (Modifier::Subdivide { size, .. }, "Size") => size.set(v.max(0.001)),
            (Modifier::Hsds(d), "Smooth Result") => d.smooth_result = b,
            (Modifier::PointCache(d), "Strength") => d.strength.set(v),
            (Modifier::PointCache(d), "Relative Offset") => d.relative = b,
            (Modifier::PointCache(d), "Playback Type") => d.playback = u.min(1),
            (Modifier::PointCache(d), "Start Frame") => d.start_frame = v,
            (Modifier::STLCheck { check, .. }, "Check") => check.set(b),
            (Modifier::STLCheck { test, .. }, "Test") => test.set(u.min(4)),
            (Modifier::STLCheck { select_faces, .. }, "Select Faces") => select_faces.set(b),
            (Modifier::FaceExtrude { amount, .. }, "Amount") => amount.set(v),
            (Modifier::FaceExtrude { scale, .. }, "Scale") => scale.set(v.max(0.0)),
            (Modifier::TurnToPoly { keep_convex, .. }, "Keep Polygons Convex") => keep_convex.set(b),
            (Modifier::TurnToPoly { limit_size, .. }, "Limit Polygon Size") => limit_size.set(b),
            (Modifier::TurnToPoly { max_size, .. }, "Max Size") => max_size.set(u.max(3)),
            (Modifier::TurnToPoly { threshold, .. }, "Planar Thresh") => threshold.set(v.clamp(0.0, 180.0)),
            (Modifier::Material { id }, "Material ID") => id.set(u.clamp(1, 65535)),
            (Modifier::MaterialByElement { random, .. }, "Random Distribution") => random.set(b),
            (Modifier::MaterialByElement { id_count, .. }, "ID Count") => id_count.set(u.clamp(1, 65535)),
            (Modifier::MaterialByElement { seed, .. }, "Seed") => seed.set(u),
            (Modifier::VertexPaint { channel, .. }, "Channel") => channel.set(u.min(2)),
            (Modifier::VertexPaint { brush_size, .. }, "Brush Size") => brush_size.set(v.max(0.01)),
            (Modifier::VertexPaint { opacity, .. }, "Opacity") => opacity.set(v.clamp(0.0, 1.0)),
            (Modifier::VertexPaint { color, .. }, "Color R") => color[0].set(v.clamp(0.0, 1.0)),
            (Modifier::VertexPaint { color, .. }, "Color G") => color[1].set(v.clamp(0.0, 1.0)),
            (Modifier::VertexPaint { color, .. }, "Color B") => color[2].set(v.clamp(0.0, 1.0)),
            (Modifier::VertexPaint { layer_opacity, .. }, "Layer Opacity") => layer_opacity.set(v.clamp(0.0, 1.0)),
            (Modifier::VertexPaint { mode, .. }, "Mode") => mode.set(u.min(VERTEX_PAINT_MODES.len() as u32 - 1)),
            (Modifier::Extrude { amount, .. }, "Amount") => amount.set(v),
            (Modifier::Extrude { segments, .. }, "Segments") => segments.set(u.clamp(1, 1000)),
            (Modifier::Extrude { cap_start, .. } | Modifier::Lathe { cap_start, .. } | Modifier::Bevel { cap_start, .. }, "Cap Start") => cap_start.set(b),
            (Modifier::Extrude { cap_end, .. } | Modifier::Lathe { cap_end, .. } | Modifier::Bevel { cap_end, .. }, "Cap End") => cap_end.set(b),
            (Modifier::Lathe { degrees, .. }, "Degrees") => degrees.set(v.clamp(0.001, 360.0)),
            (Modifier::Lathe { segments, .. }, "Segments") => segments.set(u.clamp(3, 1000)),
            (Modifier::Lathe { weld_core, .. }, "Weld Core") => weld_core.set(b),
            (Modifier::Lathe { flip_normals, .. }, "Flip Normals") => flip_normals.set(b),
            (Modifier::Lathe { direction, .. }, "Direction") => direction.set(u.min(2)),
            (Modifier::Lathe { align, .. }, "Align") => align.set(u.min(2)),
            (Modifier::Bevel { start_outline, .. }, "Start Outline") => start_outline.set(v),
            (Modifier::Bevel { level1, .. }, "Level 1 Height") => level1[0].set(v),
            (Modifier::Bevel { level1, .. }, "Level 1 Outline") => level1[1].set(v),
            (Modifier::Bevel { level2_on, .. }, "Level 2") => level2_on.set(b),
            (Modifier::Bevel { level2, .. }, "Level 2 Height") => level2[0].set(v),
            (Modifier::Bevel { level2, .. }, "Level 2 Outline") => level2[1].set(v),
            (Modifier::Bevel { level3_on, .. }, "Level 3") => level3_on.set(b),
            (Modifier::Bevel { level3, .. }, "Level 3 Height") => level3[0].set(v),
            (Modifier::Bevel { level3, .. }, "Level 3 Outline") => level3[1].set(v),
            (Modifier::Sweep { section, .. }, "Section") => section.set(u.min(10)),
            (Modifier::Sweep { length, .. }, "Length") => length.set(v.max(0.0)),
            (Modifier::Sweep { width, .. }, "Width") => width.set(v.max(0.0)),
            (Modifier::Sweep { thickness, .. }, "Thickness") => thickness.set(v.max(0.0)),
            (Modifier::Sweep { radius, .. }, "Radius") => radius.set(v.max(0.0)),
            (Modifier::Sweep { mirror, .. }, "Mirror") => mirror.set(b),
            (Modifier::Sweep { offset_x, .. }, "Offset X") => offset_x.set(v),
            (Modifier::Sweep { offset_y, .. }, "Offset Y") => offset_y.set(v),
            (Modifier::Sweep { angle, .. }, "Angle") => angle.set(v),
            (Modifier::Sweep { banking, .. }, "Banking") => banking.set(b),
            (Modifier::Sweep { smooth_section, .. }, "Smooth Section") => smooth_section.set(b),
            (Modifier::Sweep { smooth_path, .. }, "Smooth Path") => smooth_path.set(b),
            (m, _) => return Err(format!("{} has no parameter {name:?}", m.name())),
        }
        Ok(())
    }

    /// Whether the modifier changes vertex / face counts (topology dependence of entries above).
    pub fn changes_topology(&self) -> bool {
        matches!(
            self,
            Modifier::Mirror { .. } | Modifier::Symmetry { .. } | Modifier::Slice { .. } | Modifier::Lattice { .. } | Modifier::TurboSmooth { .. } | Modifier::MeshSmooth { .. } | Modifier::Shell { .. }
                | Modifier::Optimize { .. } | Modifier::ProOptimizer { .. } | Modifier::MultiRes { .. } | Modifier::Tessellate { .. } | Modifier::Subdivide { .. } | Modifier::CapHoles | Modifier::DeleteMesh
                | Modifier::Hsds(_) | Modifier::VertexWeld { .. } | Modifier::Substitute { .. }
                | Modifier::FaceExtrude { .. } | Modifier::TurnToPoly { .. } | Modifier::TurnToMesh | Modifier::EditPoly { .. } | Modifier::EditMesh { .. } | Modifier::EditPatch { .. }
                | Modifier::Extrude { .. } | Modifier::Lathe { .. } | Modifier::Bevel { .. } | Modifier::Sweep { .. } | Modifier::EditSpline { .. }
        )
    }

    pub fn deformer_mut(&mut self) -> Option<&mut Deformer> {
        match self {
            Modifier::Bend { d, .. } | Modifier::Taper { d, .. } | Modifier::Twist { d, .. } | Modifier::Noise { d, .. } | Modifier::Stretch { d, .. } | Modifier::Squeeze { d, .. }
            | Modifier::Wave { d, .. } | Modifier::Ripple { d, .. } | Modifier::Melt { d, .. } | Modifier::Skew { d, .. } => Some(d),
            _ => None,
        }
    }

    /// The stored sub-object selection, when the modifier keeps one.
    pub fn stored_selection(&self) -> Option<&StoredSelection> {
        match self {
            Modifier::PolySelect { selection } | Modifier::MeshSelect { selection } => Some(selection),
            _ => None,
        }
    }

    pub fn stored_selection_mut(&mut self) -> Option<&mut StoredSelection> {
        match self {
            Modifier::PolySelect { selection } | Modifier::MeshSelect { selection } => Some(selection),
            _ => None,
        }
    }

    pub fn apply(&self, mut v: StackValue, ctx: &EvalCtx<'_>) -> Result<StackValue, String> {
        if let Some(out) = self.apply_deformer(&v)? {
            return Ok(out);
        }
        match self {
            Modifier::UVWMap { projection, cap, length, width, height, u_tile, v_tile, w_tile, u_flip, v_flip, w_flip, real_world, channel, gizmo, .. } => {
                use loom_mesh::uvmap::{MapParams, Projection};
                // Real-World Map Size: one texture unit per scene unit (no tiling), a real-world scale.
                let rw = real_world.get();
                let params = MapParams {
                    projection: match projection.get() {
                        0 => Projection::Planar,
                        1 => Projection::Cylindrical { cap: cap.get() },
                        2 => Projection::Spherical,
                        3 => Projection::ShrinkWrap,
                        4 => Projection::Box,
                        5 => Projection::Face,
                        _ => Projection::XyzToUvw,
                    },
                    gizmo: gizmo.matrix().inverse(),
                    length: if rw { 1.0 } else { length.get().max(1e-6) },
                    width: if rw { 1.0 } else { width.get().max(1e-6) },
                    height: if rw { 1.0 } else { height.get().max(1e-6) },
                    u_tile: if rw { 1.0 } else { u_tile.get() },
                    v_tile: if rw { 1.0 } else { v_tile.get() },
                    w_tile: if rw { 1.0 } else { w_tile.get() },
                    u_flip: u_flip.get(),
                    v_flip: v_flip.get(),
                    w_flip: w_flip.get(),
                };
                let faces: Option<Vec<loom_mesh::FaceId>> = match &v.selection {
                    Some((SubLevel::Face | SubLevel::Element, ids)) if !ids.is_empty() => Some(ids.iter().map(|i| loom_mesh::FaceId(*i)).collect()),
                    _ => None,
                };
                let ch = channel.get();
                if ch == 0 {
                    // Vertex Color Channel: the projection lands on the vertex colours (u, v, w) = (r, g, b).
                    let tmp = 1000;
                    let mapped = match &faces {
                        Some(f) => loom_mesh::uvmap::project_faces(&v.mesh, &params, tmp, f),
                        None => loom_mesh::uvmap::project(&v.mesh, &params, tmp),
                    };
                    let mut colors: Vec<Option<[f32; 3]>> = vec![None; mapped.vertex_count()];
                    for (i, he) in mapped.half_edges().iter().enumerate() {
                        if he.face.is_none() {
                            continue;
                        }
                        if let Some(uv) = mapped.corner_uv(tmp, loom_mesh::HalfEdgeId(i as u32)) {
                            let c = &mut colors[he.origin.0 as usize];
                            if c.is_none() {
                                *c = Some([uv.x.clamp(0.0, 1.0), uv.y.clamp(0.0, 1.0), uv.z.clamp(0.0, 1.0)]);
                            }
                        }
                    }
                    for (vert, c) in v.mesh.vertices_mut().iter_mut().zip(colors) {
                        if let Some(c) = c {
                            vert.color = c;
                        }
                    }
                } else {
                    v.mesh = match &faces {
                        Some(f) => loom_mesh::uvmap::project_faces(&v.mesh, &params, ch, f),
                        None => loom_mesh::uvmap::project(&v.mesh, &params, ch),
                    };
                }
                Ok(v)
            }
            Modifier::Skin(d) => crate::skin::apply(d, v, ctx),
            Modifier::Morpher(d) => crate::morph::apply_morpher(d, v, ctx),
            Modifier::EditNormals(d) => {
                let mut v = v;
                let (mesh, warning) = d.apply(std::mem::take(&mut v.mesh));
                v.mesh = mesh;
                v.warnings.extend(warning);
                Ok(v)
            }
            Modifier::SkinMorph(d) => crate::morph::apply_skin_morph(d, v, ctx),
            Modifier::SkinWrap(d) => crate::morph::apply_skin_wrap(d, v, ctx),
            Modifier::UnwrapUVW(u) => {
                let ch = u.channel.get();
                if u.edits.is_none() {
                    if ch > 0 {
                        v.mesh.ensure_map(ch);
                    }
                    return Ok(v);
                }
                let (working, carried) = u.carry(&v.mesh);
                if let Some(n) = carried {
                    v.warnings.push(format!("Topology changed below Unwrap UVW: {n} of {} edited faces were carried", u.input_faces));
                }
                if ch == 0 {
                    if let Some(edits) = working.map(0) {
                        let mut out = v.mesh.clone();
                        apply_vertex_colour_channel(&mut out, edits);
                        v.mesh = out;
                    }
                } else {
                    v.mesh = working;
                }
                Ok(v)
            }
            Modifier::UVWXForm { u_tile, v_tile, w_tile, u_offset, v_offset, w_offset, rotation, u_flip, v_flip, w_flip, channel } => {
                let ch = channel.get();
                if ch == 0 {
                    // Vertex Color Channel: the colours as a channel, transformed, written back.
                    let colours: Vec<Vec3> = v.mesh.vertices().iter().map(|x| Vec3::from(x.color)).collect();
                    let corners: Vec<u32> = v.mesh.half_edges().iter().map(|h| h.origin.0).collect();
                    v.mesh.set_map(0, loom_mesh::half_edge::MapChannel { verts: colours, corners });
                }
                let (ut, vt, wt) = (u_tile.get(), v_tile.get(), w_tile.get());
                let (uo, vo, wo) = (u_offset.get(), v_offset.get(), w_offset.get());
                let (uf, vf, wf) = (u_flip.get(), v_flip.get(), w_flip.get());
                let rot = rotation.get().to_radians();
                let centre = glam::Vec2::new(0.5 * ut + uo, 0.5 * vt + vo);
                let m = v.mesh.ensure_map(ch);
                for t in &mut m.verts {
                    let mut u = if uf { 1.0 - t.x } else { t.x } * ut + uo;
                    let mut w = if vf { 1.0 - t.y } else { t.y } * vt + vo;
                    let z = if wf { 1.0 - t.z } else { t.z } * wt + wo;
                    if rot.abs() > 1e-9 {
                        let d = glam::Vec2::new(u, w) - centre;
                        let r = glam::Vec2::new(d.x * rot.cos() - d.y * rot.sin(), d.x * rot.sin() + d.y * rot.cos()) + centre;
                        u = r.x;
                        w = r.y;
                    }
                    *t = Vec3::new(u, w, z);
                }
                if ch == 0 {
                    if let Some(edits) = v.mesh.map(0).cloned() {
                        apply_vertex_colour_channel(&mut v.mesh, &edits);
                        v.mesh.remove_map(0);
                    }
                }
                Ok(v)
            }
            Modifier::Mirror { gizmo, axis, offset, copy } => {
                in_gizmo(&mut v.mesh, gizmo, |m| deform::mirror(m, axis.get() as u8, offset.get(), copy.get()))?;
                Ok(v)
            }
            Modifier::Symmetry { gizmo, axis, flip, slice, weld, threshold } => {
                in_gizmo(&mut v.mesh, gizmo, |m| deform::symmetry(m, axis.get() as u8, flip.get(), slice.get(), weld.get(), threshold.get()))?;
                Ok(v)
            }
            Modifier::Slice { gizmo, mode } => {
                let g = gizmo.matrix();
                let origin = g.transform_point3(Vec3::ZERO);
                let normal = g.transform_vector3(Vec3::Z).normalize_or_zero();
                deform::slice_modifier(&mut v.mesh, origin, normal, mode.get() as u8)?;
                Ok(v)
            }
            Modifier::Lattice { geometry, strut_radius, strut_sides, joint_radius, joint_segments } => {
                let g = geometry.get();
                v.mesh = deform::lattice(&v.mesh, g != 0, g != 1, strut_radius.get(), strut_sides.get(), joint_radius.get(), joint_segments.get())?;
                v.selection = None;
                Ok(v)
            }
            Modifier::LinkedXForm { control, control_inverse_at_pick } => {
                let (Some(c), Some(inv)) = (control, control_inverse_at_pick) else { return Ok(v) };
                let Some(world) = ctx.node_world(*c) else { return Ok(v) };
                let delta_world = world * Mat4::from_cols_array_2d(inv);
                let m = ctx.object_inverse * delta_world * ctx.object_world;
                let ids: Vec<VertexId> = (0..v.mesh.vertex_count() as u32).map(VertexId).collect();
                loom_mesh::edit_ops::transform_vertices(&mut v.mesh, &ids, m)?;
                Ok(v)
            }
            Modifier::Preserve { original, iterations, weight } => {
                let Some(o) = original else { return Ok(v) };
                let Some(orig) = ctx.node_mesh(*o) else { return Err("Preserve: pick an original object".into()) };
                deform::preserve(&mut v.mesh, &orig, iterations.get(), weight.get())?;
                Ok(v)
            }
            Modifier::Push { amount } => {
                let a = amount.get();
                let normals: Vec<Vec3> = (0..v.mesh.vertex_count() as u32).map(|i| vertex_normal(&v.mesh, VertexId(i))).collect();
                let sel = selected_vertices(&v);
                for i in sel {
                    let p = v.mesh.vertex_position(VertexId(i)).unwrap_or_default();
                    loom_mesh::edit_ops::translate_vertices(&mut v.mesh, &[VertexId(i)], normals[i as usize] * a)?;
                    let _ = p;
                }
                Ok(v)
            }
            Modifier::Displace { strength, decay } => {
                // Decay: the strength falls off exponentially with the distance from the object centre.
                let s = strength.get();
                let (lo, hi) = v.mesh.aabb();
                let centre = (lo + hi) * 0.5;
                let normals: Vec<Vec3> = (0..v.mesh.vertex_count() as u32).map(|i| vertex_normal(&v.mesh, VertexId(i))).collect();
                for i in selected_vertices(&v) {
                    let p = v.mesh.vertex_position(VertexId(i)).unwrap_or_default();
                    let k = (-decay.get().max(0.0) * p.distance(centre) / ((hi - lo).length().max(1e-6) * 0.5)).exp();
                    loom_mesh::edit_ops::translate_vertices(&mut v.mesh, &[VertexId(i)], normals[i as usize] * (s * k))?;
                }
                Ok(v)
            }
            Modifier::XForm { gizmo } => {
                let m = gizmo.matrix();
                let ids: Vec<loom_mesh::VertexId> = match &v.selection {
                    Some((level, ids)) => loom_mesh::select_ops::vertices_of(&v.mesh, *level, ids).into_iter().collect(),
                    None => (0..v.mesh.vertex_count() as u32).map(loom_mesh::VertexId).collect(),
                };
                loom_mesh::edit_ops::transform_vertices(&mut v.mesh, &ids, m)?;
                // A shape flowing up is transformed too (and passed on).
                if let Some(shape) = v.shape.take() {
                    v.shape = Some(shape.transformed(m));
                }
                Ok(v)
            }
            Modifier::Normal { flip } => {
                // A manifold half-edge mesh is always unified; Flip is the one switch.
                let faces: Vec<loom_mesh::FaceId> = match &v.selection {
                    Some((SubLevel::Face | SubLevel::Element, ids)) => ids.iter().map(|i| loom_mesh::FaceId(*i)).collect(),
                    _ => (0..v.mesh.face_count() as u32).map(loom_mesh::FaceId).collect(),
                };
                if flip.get() {
                    let all = faces.len() == v.mesh.face_count();
                    if all {
                        loom_mesh::poly_ops::flip_faces(&mut v.mesh, &faces)?;
                    } else {
                        // Flipping part of a manifold mesh needs whole elements.
                        loom_mesh::poly_ops::flip_faces(&mut v.mesh, &faces).map_err(|e| format!("Normal: {e}"))?;
                    }
                }
                Ok(v)
            }
            Modifier::Smooth { auto_smooth, threshold, groups, prevent_indirect } => {
                let faces: Vec<loom_mesh::FaceId> = match &v.selection {
                    Some((SubLevel::Face | SubLevel::Element, ids)) => ids.iter().map(|i| loom_mesh::FaceId(*i)).collect(),
                    _ => (0..v.mesh.face_count() as u32).map(loom_mesh::FaceId).collect(),
                };
                if auto_smooth.get() {
                    v.mesh.auto_smooth_faces_with(&faces, threshold.get(), prevent_indirect.get());
                } else {
                    let bits = groups.get();
                    for f in &faces {
                        if let Some(face) = v.mesh.faces_mut().get_mut(f.0 as usize) {
                            face.smoothing_groups = bits;
                        }
                    }
                }
                Ok(v)
            }
            Modifier::MeshSelect { selection } => {
                let level = match selection.level {
                    1 => Some(SubLevel::Vertex),
                    2 => Some(SubLevel::Edge),
                    3 | 4 => Some(SubLevel::Face),
                    5 => Some(SubLevel::Element),
                    _ => None,
                };
                v.selection = level.map(|l| (l, selection.ids.clone()));
                Ok(v)
            }
            Modifier::VolSelect(d) => crate::select_mods::vol_select(d, v, ctx),
            Modifier::SplineSelect { level, ids, knots } => {
                let sh = v.shape.as_ref().ok_or("Spline Select needs a shape below it")?;
                if *knots != 0 && sh.knot_count() != *knots {
                    v.warnings.push("Spline Select: the spline below changed; its selection was dropped".into());
                    v.shape_selection = None;
                    return Ok(v);
                }
                v.shape_selection = (*level > 0).then(|| (*level, ids.clone()));
                Ok(v)
            }
            Modifier::DeleteSpline => crate::select_mods::delete_spline(v),
            Modifier::FilletChamfer(d) => crate::select_mods::fillet_chamfer(d, v),
            Modifier::TrimExtend { picks } => {
                let mut sh = v.shape.take().ok_or("Trim/Extend needs a shape below it")?;
                for (p, extend) in picks {
                    let _ = if *extend { loom_shape::trim::extend(&mut sh, *p) } else { loom_shape::trim::trim(&mut sh, *p) };
                }
                v.mesh = crate::node::shape_cage_mesh(&sh, false);
                v.shape = Some(sh);
                v.shape_selection = None;
                Ok(v)
            }
            Modifier::CrossSection { kind } => {
                let mut sh = v.shape.take().ok_or("CrossSection needs a shape below it")?;
                let k = [loom_shape::KnotKind::Corner, loom_shape::KnotKind::Smooth, loom_shape::KnotKind::Bezier, loom_shape::KnotKind::BezierCorner][(*kind as usize).min(3)];
                let all: Vec<usize> = (0..sh.splines.len()).collect();
                let extra = loom_shape::trim::cross_section(&sh, &all, k);
                sh.splines.extend(extra);
                v.mesh = crate::node::shape_cage_mesh(&sh, false);
                v.shape = Some(sh);
                v.shape_selection = None;
                Ok(v)
            }
            Modifier::Surface { threshold, flip, remove_interior, steps } => {
                let sh = v.shape.take().ok_or("Surface needs a spline cage below it")?;
                v.mesh = loom_shape::trim::surface(&sh, *threshold, *steps, *flip, *remove_interior);
                v.patch = Some(loom_shape::trim::surface_patches(&sh, *threshold, *flip, *remove_interior));
                v.selection = None;
                Ok(v)
            }
            Modifier::BevelProfile { profile, cap_start, cap_end } => {
                let sh = v.shape.take().ok_or("Bevel Profile needs a shape below it")?;
                let pid = profile.ok_or("Bevel Profile: pick a profile shape")?;
                let prof = ctx.node_eval_shape(pid).ok_or("Bevel Profile: the profile is not a shape (or depends on this object)")?;
                let pts: Vec<Vec3> = prof.polylines().into_iter().next().unwrap_or_default();
                if pts.len() < 2 {
                    return Err("Bevel Profile: the profile needs two points".into());
                }
                let levels: Vec<(f32, f32)> = pts.windows(2).map(|w| (w[1].y - w[0].y, w[1].x - w[0].x)).collect();
                v.mesh = loom_shape::mesh_from_shape::bevel(&sh, pts[0].x, &levels, *cap_start, *cap_end)?;
                v.selection = None;
                Ok(v)
            }
            Modifier::NormalizeSpline { seg_length } => crate::select_mods::normalize_spline(v, *seg_length),
            Modifier::AffectRegion(d) => {
                let sel = selected_vertices(&v);
                crate::deform9::affect_region(d, v, &sel)
            }
            Modifier::PathDeform(d) => crate::deform9::path_deform(d, v, ctx),
            Modifier::VertexWeld { threshold } => {
                let all: Vec<VertexId> = (0..v.mesh.vertex_count() as u32).map(VertexId).collect();
                loom_mesh::topo_ops::weld_vertices(&mut v.mesh, &all, threshold.get())?;
                v.selection = None;
                Ok(v)
            }
            Modifier::Substitute { node } => {
                let Some(n) = node else { return Ok(v) };
                let mesh = ctx.node_evaluated(*n).map(|(_, m)| m).or_else(|| ctx.node_mesh(*n)).ok_or("Substitute: pick a scene object")?;
                v.mesh = mesh;
                v.selection = None;
                Ok(v)
            }
            Modifier::Flex(d) => crate::deform9::flex(d, v, ctx),
            Modifier::SplineIkControl(d) => crate::deform9::spline_ik_control(d, v, ctx),
            Modifier::CameraMap(d) => crate::mapping9::camera_map(d, v, ctx),
            Modifier::MapScaler(d) => crate::mapping9::map_scaler(d, v, ctx),
            Modifier::UvwMappingAdd => crate::mapping9::mapping_add(v),
            Modifier::UvwMappingClear { channel } => crate::mapping9::mapping_clear(*channel, v),
            Modifier::Hsds(d) => crate::caches9::hsds(d, v),
            Modifier::PointCache(d) => crate::caches9::point_cache(d, v, ctx),
            Modifier::RenderableSpline(r) => {
                let mut sh = v.shape.take().ok_or("Renderable Spline needs a shape below it")?;
                sh.render = (**r).clone();
                v.mesh = crate::node::shape_cage_mesh(&sh, false);
                v.shape = Some(sh);
                Ok(v)
            }
            Modifier::PolySelect { selection } => {
                let level = match selection.level {
                    1 => Some(SubLevel::Vertex),
                    2 => Some(SubLevel::Edge),
                    3 => Some(SubLevel::Border),
                    4 => Some(SubLevel::Face),
                    5 => Some(SubLevel::Element),
                    _ => None,
                };
                v.selection = level.map(|l| (l, selection.ids.clone()));
                Ok(v)
            }
            Modifier::FFD { kind, dims, points, rest, lo, hi, gizmo, deform, falloff } => {
                if points.is_empty() {
                    return Ok(v);
                }
                // The lattice volume and rest grid live in object space; the
                // gizmo carries the control points, so moving the whole lattice
                // (Lattice level) drags the object along.
                let g = gizmo.matrix();
                let size = (*hi - *lo).max(Vec3::splat(1e-6));
                let to_unit = |p: Vec3| (p - *lo) / size;
                let pu: Vec<Vec3> = points.iter().map(|p| to_unit(g.transform_point3(*p))).collect();
                let ru: Vec<Vec3> = rest.iter().map(|p| to_unit(*p)).collect();
                let cyl = *kind == 4;
                let (all, fall) = (deform.get() == 1, falloff.get());
                for i in selected_vertices(&v) {
                    let p = v.mesh.vertex_position(VertexId(i)).unwrap_or_default();
                    let q = loom_mesh::ffd::deform(to_unit(p), *dims, cyl, &pu, &ru, all, fall);
                    v.mesh.vertices_mut()[i as usize].position = *lo + q * size;
                }
                Ok(v)
            }
            Modifier::TurboSmooth { iterations, sep_smoothing, sep_materials, .. } => {
                for _ in 0..iterations.get() {
                    v.mesh = deform_msmooth(&v.mesh, 1.0, sep_smoothing.get(), sep_materials.get())?;
                }
                v.selection = None;
                Ok(v)
            }
            Modifier::MeshSmooth { method, iterations, smoothness, sep_smoothing, sep_materials } => {
                // Classic subdivides linearly; NURMS / Quad Output smooth.
                let s = if method.get() == 0 { 0.0 } else { smoothness.get() };
                for _ in 0..iterations.get() {
                    v.mesh = deform_msmooth(&v.mesh, s, sep_smoothing.get(), sep_materials.get())?;
                }
                v.selection = None;
                Ok(v)
            }
            Modifier::Shell { inner, outer, segments, override_inner, inner_id, override_outer, outer_id, override_edge, edge_id, auto_smooth, angle } => {
                let outer_faces = v.mesh.face_count();
                let (mesh, inner_faces, rim) = loom_mesh::mesh_mods::shell(&v.mesh, inner.get(), outer.get(), segments.get())?;
                v.mesh = mesh;
                if override_outer.get() {
                    for f in 0..outer_faces {
                        v.mesh.faces_mut()[f].material_id = outer_id.get() as u16;
                    }
                }
                if override_inner.get() {
                    for f in &inner_faces {
                        v.mesh.faces_mut()[f.0 as usize].material_id = inner_id.get() as u16;
                    }
                }
                if override_edge.get() {
                    for f in &rim {
                        v.mesh.faces_mut()[f.0 as usize].material_id = edge_id.get() as u16;
                    }
                }
                if auto_smooth.get() && !rim.is_empty() {
                    v.mesh.auto_smooth_faces(&rim, angle.get());
                }
                v.selection = None;
                Ok(v)
            }
            Modifier::Relax { amount, iterations, keep_boundary, save_outer } => {
                let verts: Vec<VertexId> = selected_vertices(&v).into_iter().map(VertexId).collect();
                loom_mesh::geom_ops::relax(&mut v.mesh, &verts, amount.get(), iterations.get(), keep_boundary.get(), save_outer.get())?;
                Ok(v)
            }
            Modifier::Optimize { face_threshold, edge_threshold, max_edge_length } => {
                v.mesh = loom_mesh::optimize::optimize(&v.mesh, face_threshold.get(), edge_threshold.get(), max_edge_length.get())?;
                v.selection = None;
                Ok(v)
            }
            Modifier::ProOptimizer { vertex_percent, calculated } | Modifier::MultiRes { vertex_percent, generated: calculated } => {
                if calculated.get() {
                    let count = loom_mesh::optimize::triangle_vertex_count(&v.mesh);
                    let target = ((count as f32 * vertex_percent.get() / 100.0).round() as usize).max(3);
                    v.mesh = loom_mesh::optimize::decimate(&v.mesh, target)?;
                    v.selection = None;
                }
                Ok(v)
            }
            Modifier::Tessellate { operation, tension, iterations } => {
                let kind = if operation.get() == 0 { loom_mesh::geom_ops::TessKind::Edge } else { loom_mesh::geom_ops::TessKind::Face };
                for _ in 0..iterations.get().max(1) {
                    let faces = selected_faces(&v);
                    loom_mesh::geom_ops::tessellate(&mut v.mesh, &faces, kind, tension.get())?;
                    v.selection = None;
                }
                Ok(v)
            }
            Modifier::Subdivide { size, world } => {
                v.mesh = if *world {
                    // Size in world units: subdivide the world-space mesh, then back.
                    let mut m = v.mesh.clone();
                    loom_mesh::edit_ops::transform_mesh(&mut m, ctx.object_world)?;
                    let mut m = loom_mesh::mesh_mods::subdivide(&m, size.get())?;
                    loom_mesh::edit_ops::transform_mesh(&mut m, ctx.object_inverse)?;
                    m
                } else {
                    loom_mesh::mesh_mods::subdivide(&v.mesh, size.get())?
                };
                v.selection = None;
                Ok(v)
            }
            Modifier::CapHoles => {
                loom_mesh::mesh_mods::cap_holes(&mut v.mesh)?;
                v.selection = None;
                Ok(v)
            }
            Modifier::STLCheck { check, test, select_faces } => {
                if check.get() && select_faces.get() {
                    let r = loom_mesh::mesh_mods::stl_check(&v.mesh);
                    let faces: Vec<u32> = match test.get() {
                        t @ 0..=3 => r.faces[t as usize].iter().map(|f| f.0).collect(),
                        _ => {
                            let mut all: Vec<u32> = r.faces.iter().flatten().map(|f| f.0).collect();
                            all.sort();
                            all.dedup();
                            all
                        }
                    };
                    v.selection = Some((SubLevel::Face, faces));
                }
                Ok(v)
            }
            Modifier::DeleteMesh => {
                let faces = v.selection.as_ref().map(|_| selected_faces(&v));
                loom_mesh::mesh_mods::delete_mesh(&mut v.mesh, faces.as_deref())?;
                v.selection = None;
                Ok(v)
            }
            Modifier::FaceExtrude { amount, scale } => {
                let faces = selected_faces(&v);
                if !faces.is_empty() {
                    loom_mesh::mesh_mods::face_extrude(&mut v.mesh, &faces, amount.get(), scale.get())?;
                }
                v.selection = None;
                Ok(v)
            }
            Modifier::TurnToPoly { keep_convex, limit_size, max_size, threshold } => {
                v.mesh = loom_mesh::optimize::turn_to_poly(&v.mesh, threshold.get(), limit_size.get().then_some(max_size.get() as usize), keep_convex.get())?;
                v.selection = None;
                Ok(v)
            }
            Modifier::TurnToMesh => {
                v.mesh = loom_mesh::mesh_mods::triangulate(&v.mesh)?;
                v.selection = None;
                Ok(v)
            }
            Modifier::Material { id } => {
                for f in selected_faces(&v) {
                    v.mesh.faces_mut()[f.0 as usize].material_id = id.get() as u16;
                }
                Ok(v)
            }
            Modifier::MaterialByElement { random, id_count, seed } => {
                let ids: Vec<u16> = (1..=id_count.get().max(1) as u16).collect();
                loom_mesh::mesh_mods::material_by_element(&mut v.mesh, &ids, random.get(), seed.get());
                Ok(v)
            }
            Modifier::VertexPaint { channel, paint, coverage, layer_opacity, mode, .. } => {
                let (o, m) = (layer_opacity.get(), mode.get());
                for (i, c) in paint {
                    let k = coverage.get(i).copied().unwrap_or(1.0) * o;
                    let mix = |b: f32, t: f32| b + (blend_channel(m, b, t) - b) * k;
                    if let Some(vx) = v.mesh.vertices_mut().get_mut(*i as usize) {
                        match channel.get() {
                            0 => vx.color = [mix(vx.color[0], c[0]), mix(vx.color[1], c[1]), mix(vx.color[2], c[2])],
                            1 => vx.illum = [mix(vx.illum[0], c[0]), mix(vx.illum[1], c[1]), mix(vx.illum[2], c[2])],
                            _ => vx.alpha = mix(vx.alpha, c[0]),
                        }
                    }
                }
                Ok(v)
            }
            Modifier::EditPoly { poly, input_vertices, input_faces, snapshot } => {
                if v.mesh.vertex_count() != *input_vertices || v.mesh.face_count() != *input_faces {
                    return Err("topology changed below: its edits were dropped".into());
                }
                let mut out = poly.display_mesh().into_owned();
                // Deltas from below reach the edited vertices by index while the
                // count is unchanged; after the entry's own deletes / welds they
                // reach the vertices that still sit on a snapshot position.
                let map = vertex_map(&out, snapshot);
                for (i, vertex) in out.vertices_mut().iter_mut().enumerate() {
                    if let Some(src) = map[i] {
                        let delta = v.mesh.vertex_position(VertexId(src as u32)).unwrap_or_default() - snapshot[src];
                        vertex.position += delta;
                    }
                }
                Ok(StackValue { mesh: out, selection: poly.passes_up.clone(), shape: None, shape_selection: None, warnings: Vec::new(), patch: None })
            }
            Modifier::EditMesh { poly, input_vertices, input_faces, snapshot } => {
                if v.mesh.vertex_count() != *input_vertices || v.mesh.face_count() != *input_faces {
                    return Err("topology changed below: its edits were dropped".into());
                }
                let mut out = poly.mesh.clone();
                let map = vertex_map(&out, snapshot);
                for (i, vertex) in out.vertices_mut().iter_mut().enumerate() {
                    if let Some(src) = map[i] {
                        let delta = v.mesh.vertex_position(VertexId(src as u32)).unwrap_or_default() - snapshot[src];
                        vertex.position += delta;
                    }
                }
                Ok(StackValue { mesh: out, selection: poly.passes_up.clone(), shape: None, shape_selection: None, warnings: Vec::new(), patch: None })
            }
            Modifier::EditPatch { patch, input_vertices, input_faces } => {
                if v.mesh.vertex_count() != *input_vertices || v.mesh.face_count() != *input_faces {
                    return Err("topology changed below: its edits were dropped".into());
                }
                Ok(StackValue { mesh: patch.surface(), selection: None, shape: None, shape_selection: None, warnings: Vec::new(), patch: Some(patch.patch.clone()) })
            }
            Modifier::Extrude { amount, segments, cap_start, cap_end } => {
                let shape = v.shape.take().ok_or("Extrude needs a shape (a spline) below it")?;
                v.mesh = loom_shape::mesh_from_shape::extrude(&shape, amount.get(), segments.get(), cap_start.get(), cap_end.get())?;
                v.selection = None;
                Ok(v)
            }
            Modifier::Lathe { degrees, segments, cap_start, cap_end, weld_core, flip_normals, direction, align } => {
                let shape = v.shape.take().ok_or("Lathe needs a shape (a spline) below it")?;
                v.mesh = loom_shape::mesh_from_shape::lathe(&shape, degrees.get(), segments.get(), cap_start.get(), cap_end.get(), weld_core.get(), flip_normals.get(), direction.get(), align.get())?;
                v.selection = None;
                Ok(v)
            }
            Modifier::Bevel { start_outline, level1, level2_on, level2, level3_on, level3, cap_start, cap_end } => {
                let shape = v.shape.take().ok_or("Bevel needs a shape (a spline) below it")?;
                let mut levels = vec![(level1[0].get(), level1[1].get())];
                if level2_on.get() {
                    levels.push((level2[0].get(), level2[1].get()));
                    if level3_on.get() {
                        levels.push((level3[0].get(), level3[1].get()));
                    }
                }
                v.mesh = loom_shape::mesh_from_shape::bevel(&shape, start_outline.get(), &levels, cap_start.get(), cap_end.get())?;
                v.selection = None;
                Ok(v)
            }
            Modifier::Sweep { section, length, width, thickness, radius, custom, mirror, offset_x, offset_y, angle, banking, smooth_section, smooth_path } => {
                use loom_shape::mesh_from_shape::{Section, SweepParams};
                let shape = v.shape.take().ok_or("Sweep needs a shape (a spline) below it")?;
                let (l, w, t, r) = (length.get(), width.get(), thickness.get(), radius.get());
                let sec = match section.get() {
                    0 => Section::Angle { length: l, width: w, thickness: t },
                    1 => Section::Bar { length: l, width: w },
                    2 => Section::Channel { length: l, width: w, thickness: t },
                    3 => Section::Cylinder { radius: r },
                    4 => Section::HalfRound { radius: r },
                    5 => Section::Pipe { radius: r, thickness: t },
                    6 => Section::QuarterRound { radius: r },
                    7 => Section::Tee { length: l, width: w, thickness: t },
                    8 => Section::Tube { length: l, width: w, thickness: t },
                    9 => Section::WideFlange { length: l, width: w, thickness: t },
                    _ => {
                        let id = custom.ok_or("Sweep > Custom Section: pick a shape")?;
                        let cs = ctx.node_shape(id).ok_or("the custom section is not a shape")?;
                        Section::Custom(cs.polylines())
                    }
                };
                let params = SweepParams { mirror: mirror.get(), offset: glam::Vec2::new(offset_x.get(), offset_y.get()), angle_deg: angle.get(), banking: banking.get(), smooth_section: smooth_section.get(), smooth_path: smooth_path.get() };
                v.mesh = loom_shape::mesh_from_shape::sweep(&shape, &sec, params)?;
                v.selection = None;
                Ok(v)
            }
            Modifier::EditSpline { spline, input_knots, snapshot } => {
                let input = v.shape.take().ok_or("Edit Spline needs a shape (a spline) below it")?;
                if input.knot_count() != *input_knots {
                    return Err("the spline below changed its knots: its edits were dropped".into());
                }
                // Knot moves below carry through as deltas; the edited shape keeps everything else.
                let mut out = spline.shape.clone();
                let below = input.knot_positions();
                let mut edited = out.knot_positions();
                let map = knot_map(&edited, snapshot);
                for (i, src) in map.iter().enumerate() {
                    if let Some(src) = *src {
                        if src < below.len() {
                            edited[i] += below[src] - snapshot[src];
                        }
                    }
                }
                out.set_knot_positions(&edited);
                v.mesh = crate::node::shape_cage_mesh(&out, true);
                v.shape = Some(out);
                v.selection = None;
                Ok(v)
            }
            // Deformers were handled by apply_deformer above.
            _ => Ok(v),
        }
    }
}

impl Modifier {
    /// The per-point deformers: points go into gizmo space, the axis is
    /// mapped to Z, the function runs, and everything comes back.
    fn apply_deformer(&self, v: &StackValue) -> Result<Option<StackValue>, String> {
        let Some(d) = (match self {
            Modifier::Bend { d, .. } | Modifier::Taper { d, .. } | Modifier::Twist { d, .. } | Modifier::Noise { d, .. } | Modifier::Stretch { d, .. } | Modifier::Squeeze { d, .. }
            | Modifier::Wave { d, .. } | Modifier::Ripple { d, .. } | Modifier::Melt { d, .. } | Modifier::Skew { d, .. } => Some(d),
            _ => None,
        }) else {
            if let Modifier::Spherify { gizmo, percent } = self {
                let mut out = v.clone();
                let g = gizmo.matrix();
                let inv = g.inverse();
                let (lo, hi) = out.mesh.aabb();
                let radius = ((hi - lo) * 0.5).max_element().max(1e-6);
                let sel = selected_vertices(v);
                for i in sel {
                    let p = inv.transform_point3(out.mesh.vertex_position(VertexId(i)).unwrap_or_default());
                    let q = g.transform_point3(deform::spherify(p, radius, percent.get()));
                    out.mesh.vertices_mut()[i as usize].position = q;
                }
                return Ok(Some(out));
            }
            return Ok(None);
        };
        let mut out = v.clone();
        // Gizmo space shifted to the Center: the deformation is measured from it.
        let g = d.gizmo.matrix() * Mat4::from_translation(d.center.get());
        let inv = g.inverse();
        let (to_z, from_z) = deform::axis_frame(d.axis.get() as u8);
        // Extents in the deformation frame.
        let pts: Vec<Vec3> = out.mesh.vertices().iter().map(|vx| to_z * inv.transform_point3(vx.position)).collect();
        let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
        for p in &pts {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
        if pts.is_empty() {
            return Ok(Some(out));
        }
        let len = (hi.z - lo.z).max(1e-6);
        let limits = d.limits();
        let radius = ((hi.x - lo.x).max(hi.y - lo.y) * 0.5).max(1e-6);
        let sel = selected_vertices(v);
        for i in sel {
            let p = pts[i as usize];
            let q = match self {
                Modifier::Bend { angle, direction, .. } => deform::bend(p, angle.get(), direction.get(), len, limits),
                Modifier::Taper { amount, curve, effect, symmetry, .. } => deform::taper(p, amount.get(), curve.get(), effect.get() as u8, symmetry.get(), len, limits),
                Modifier::Twist { angle, bias, .. } => deform::twist(p, angle.get(), bias.get(), len, limits),
                Modifier::Noise { seed, scale, fractal, roughness, iterations, strength, .. } => deform::noise(p, seed.get(), scale.get(), fractal.get(), roughness.get(), iterations.get(), Vec3::new(strength[0].get(), strength[1].get(), strength[2].get())),
                Modifier::Stretch { stretch, amplify, .. } => deform::stretch(p, stretch.get(), amplify.get(), len, limits),
                Modifier::Squeeze { axial_amount, axial_curve, radial_amount, radial_curve, .. } => deform::squeeze(p, axial_amount.get(), axial_curve.get(), radial_amount.get(), radial_curve.get(), len, radius, limits),
                Modifier::Wave { amplitude1, amplitude2, wavelength, phase, decay, .. } => deform::wave(p, amplitude1.get(), amplitude2.get(), wavelength.get(), phase.get(), decay.get(), (hi.y - lo.y).max(1e-6)),
                Modifier::Ripple { amplitude1, amplitude2, wavelength, phase, decay, .. } => deform::ripple(p, amplitude1.get(), amplitude2.get(), wavelength.get(), phase.get(), decay.get()),
                Modifier::Melt { amount, spread, solidity, flip, .. } => deform::melt(p, amount.get(), spread.get(), solidity.get(), flip.get(), lo.z, hi.z),
                Modifier::Skew { amount, direction, .. } => deform::skew(p, amount.get(), direction.get(), len, limits),
                _ => p,
            };
            out.mesh.vertices_mut()[i as usize].position = g.transform_point3(from_z * q);
        }
        Ok(Some(out))
    }
}

/// Which snapshot vertex each edited vertex descends from: by index while
/// the counts match, else by an identical position (moved or new vertices
/// get None and keep their edited place).
fn vertex_map(edited: &Mesh, snapshot: &[Vec3]) -> Vec<Option<usize>> {
    let positions: Vec<Vec3> = edited.vertices().iter().map(|v| v.position).collect();
    knot_map(&positions, snapshot)
}

fn knot_map(edited: &[Vec3], snapshot: &[Vec3]) -> Vec<Option<usize>> {
    if edited.len() == snapshot.len() {
        return (0..edited.len()).map(Some).collect();
    }
    let key = |p: Vec3| (p.x.to_bits(), p.y.to_bits(), p.z.to_bits());
    let by_pos: std::collections::HashMap<(u32, u32, u32), usize> = snapshot.iter().enumerate().map(|(i, p)| (key(*p), i)).collect();
    edited.iter().map(|p| by_pos.get(&key(*p)).copied()).collect()
}

/// MSmooth of the whole mesh at a smoothness (0 = linear subdivision).
fn deform_msmooth(mesh: &Mesh, smoothness: f32, sep_smoothing: bool, sep_materials: bool) -> Result<Mesh, String> {
    loom_mesh::geom_ops::msmooth_faces(mesh, None, smoothness, sep_smoothing, sep_materials)
}

/// Faces a modifier acts on: the incoming selection's, else all.
fn selected_faces(v: &StackValue) -> Vec<loom_mesh::FaceId> {
    match &v.selection {
        Some((level, ids)) if !ids.is_empty() => loom_mesh::mesh_mods::faces_of_selection(&v.mesh, *level, ids),
        _ => (0..v.mesh.face_count() as u32).map(loom_mesh::FaceId).collect(),
    }
}

/// Vertices a modifier acts on: the incoming selection's, else all.
fn selected_vertices(v: &StackValue) -> Vec<u32> {
    match &v.selection {
        Some((level, ids)) if !ids.is_empty() => loom_mesh::select_ops::vertices_of(&v.mesh, *level, ids).into_iter().map(|x| x.0).collect(),
        _ => (0..v.mesh.vertex_count() as u32).collect(),
    }
}

fn vertex_normal(mesh: &Mesh, v: VertexId) -> Vec3 {
    let mut n = Vec3::ZERO;
    for f in mesh.faces_around_vertex(v) {
        n += mesh.face_normal(f);
    }
    n.normalize_or_zero()
}

/// Runs a mesh op in gizmo space.
fn in_gizmo(mesh: &mut Mesh, gizmo: &Transform, f: impl FnOnce(&mut Mesh) -> Result<(), String>) -> Result<(), String> {
    let g = gizmo.matrix();
    loom_mesh::edit_ops::transform_mesh(mesh, g.inverse())?;
    f(mesh)?;
    loom_mesh::edit_ops::transform_mesh(mesh, g)
}

/// One entry of a node's modifier stack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModifierSlot {
    pub modifier: Modifier,
    pub enabled: bool,
    pub enabled_in_viewport: bool,
    /// Display name (an entry can be renamed).
    pub name: String,
    /// Active sub-object level of this entry (0 = none), per `sub_levels()`.
    #[serde(default)]
    pub sub_level: u32,
    /// Instanced modifier: the slot whose parameters this one shares.
    #[serde(default)]
    pub instance_of: Option<(NodeId, usize)>,
}

impl ModifierSlot {
    pub fn new(modifier: Modifier) -> Self {
        let name = modifier.name().to_owned();
        Self { modifier, enabled: true, enabled_in_viewport: true, name, sub_level: 0, instance_of: None }
    }
}

/// A node's evaluated stack output.
#[derive(Debug, Clone)]
pub struct Evaluated {
    pub value: StackValue,
    /// Topology-dependence warnings and modifier errors, one line each.
    pub warnings: Vec<String>,
}

/// The gizmo rotation for UVW Map > Alignment (0 X, 1 Y, 2 Z): its Z axis
/// along the chosen object axis.
pub fn uvw_alignment_rotation(alignment: u32) -> glam::Quat {
    match alignment {
        0 => glam::Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
        1 => glam::Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
        _ => glam::Quat::IDENTITY,
    }
}

#[cfg(test)]
mod modifier_list_tests {
    use super::Modifier;

    /// Every Modifier List entry is implemented (in `LIST` /
    /// `WSM_LIST`, constructible by name) or listed out with its reason.
    #[test]
    fn every_modifier_is_in_or_out() {
        let tsv = include_str!("../../../docs/reference/modifiers.tsv");
        let mut seen = std::collections::BTreeSet::new();
        for line in tsv.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
            let cols: Vec<&str> = line.split('\t').collect();
            assert!(cols.len() >= 3, "{line}");
            let (name, group, status) = (cols[0], cols[1], cols[2]);
            let listed = if group == "WSM" { Modifier::WSM_LIST.contains(&name) } else { Modifier::LIST.contains(&name) };
            match status {
                "in" => {
                    assert!(listed, "{name} is marked in but not in the {group} list");
                    let m = Modifier::by_name(name).unwrap_or_else(|| panic!("{name} cannot be created"));
                    assert_eq!(m.is_wsm(), group == "WSM", "{name}");
                }
                "out" => {
                    assert!(!listed, "{name} is marked out but listed");
                    assert!(cols.get(3).is_some_and(|r| !r.trim().is_empty()), "{name} needs a reason");
                }
                other => panic!("{name}: status {other}"),
            }
            seen.insert(name);
        }
        for n in Modifier::LIST.iter().chain(Modifier::WSM_LIST) {
            assert!(seen.contains(n), "{n} is in Loom's list but not in modifiers.tsv");
        }
    }
}
