//! The Skin modifier: bones with envelopes, per-vertex weights (from the
//! envelopes or set by hand), linear blend skinning against the bind pose,
//! mirror settings, Joint Angle / Bulge Angle / Morph Angle gizmos.
//!
//! On a patch object the weighted points are the lattice's vertices
//! followed by its handles (the order of `PatchMesh::cage`); the deformed
//! lattice flows up and is tessellated again.

use crate::modifier::{EvalCtx, StackValue};
use crate::{NodeId, Param, Scene, Transform};
use glam::{Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Animatable Envelopes: the radii as keyable parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvelopeKeys {
    pub inner: [Param<f32>; 2],
    pub outer: [Param<f32>; 2],
}

/// A capsule in the bone's bind-pose local frame: radius `inner` → weight 1,
/// `outer` → 0, interpolated along the start→end segment (or along `path`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub start: Vec3,
    pub end: Vec3,
    /// Inner radius at the start and at the end.
    pub inner: [f32; 2],
    pub outer: [f32; 2],
    /// 0 Linear, 1 Sinual, 2 Fast Out, 3 Slow Out.
    pub falloff: u32,
    /// Abs. Effect: the weight the envelope gives inside the inner radius.
    pub abs_effect: f32,
    /// Squash multiplier for a bone that stretches with Stretch = Squash.
    pub squash: f32,
    /// Relative (on) or Absolute (off): a relative envelope gives full
    /// weight only inside its inner radius; between the radii its weights
    /// are not scaled up to 1, so the rest of the vertex stays put.
    #[serde(default)]
    pub relative: bool,
    /// A spline bone's envelope follows the curve: points in the bone's
    /// frame (empty = the straight start → end segment).
    #[serde(default)]
    pub path: Vec<Vec3>,
    /// Animatable Envelopes: keyed radii (None = the plain values).
    #[serde(default)]
    pub keys: Option<Box<EnvelopeKeys>>,
}

impl Default for Envelope {
    fn default() -> Self {
        Self { start: Vec3::ZERO, end: Vec3::X * 10.0, inner: [2.5, 2.5], outer: [5.0, 5.0], falloff: 0, abs_effect: 1.0, squash: 1.0, relative: false, path: Vec::new(), keys: None }
    }
}

impl Envelope {
    /// Inner and outer radii at the ends, at the ambient time.
    pub fn radii(&self) -> ([f32; 2], [f32; 2]) {
        match &self.keys {
            Some(k) => ([k.inner[0].get(), k.inner[1].get()], [k.outer[0].get(), k.outer[1].get()]),
            None => (self.inner, self.outer),
        }
    }

    /// The axis as a polyline: the path, or start → end.
    fn axis(&self) -> Vec<Vec3> {
        if self.path.len() >= 2 {
            self.path.clone()
        } else {
            vec![self.start, self.end]
        }
    }

    /// The closest point of the axis to `p`: (u 0..1 by length along the axis, the point).
    pub fn closest(&self, p: Vec3) -> (f32, Vec3) {
        if self.path.len() < 2 {
            let seg = self.end - self.start;
            let len2 = seg.length_squared();
            let u = if len2 < 1e-12 { 0.0 } else { ((p - self.start).dot(seg) / len2).clamp(0.0, 1.0) };
            return (u, self.start + seg * u);
        }
        let pts = &self.path;
        let total: f32 = pts.windows(2).map(|w| w[0].distance(w[1])).sum::<f32>().max(1e-12);
        let mut best = (f32::INFINITY, 0.0, pts[0]);
        let mut run = 0.0;
        for w in pts.windows(2) {
            let seg = w[1] - w[0];
            let len = seg.length();
            let t = if len < 1e-9 { 0.0 } else { ((p - w[0]).dot(seg) / (len * len)).clamp(0.0, 1.0) };
            let q = w[0] + seg * t;
            let d = q.distance_squared(p);
            if d < best.0 {
                best = (d, (run + len * t) / total, q);
            }
            run += len;
        }
        (best.1, best.2)
    }

    /// The axis point `u` (0..1 by length) along the envelope.
    pub fn axis_point(&self, u: f32) -> Vec3 {
        let pts = self.axis();
        let total: f32 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
        let mut want = u.clamp(0.0, 1.0) * total;
        for w in pts.windows(2) {
            let len = w[0].distance(w[1]);
            if want <= len || len <= 0.0 {
                return if len <= 0.0 { w[0] } else { w[0].lerp(w[1], want / len) };
            }
            want -= len;
        }
        *pts.last().unwrap_or(&self.end)
    }

    /// The axis direction at `u` (start → end for a straight envelope).
    pub fn axis_direction(&self, u: f32) -> Vec3 {
        let a = self.axis_point((u - 0.01).max(0.0));
        let b = self.axis_point((u + 0.01).min(1.0));
        (b - a).normalize_or(Vec3::X)
    }

    /// Moves the envelope with its axis: Animatable Envelopes keep their keys.
    pub fn set_radius(&mut self, end: usize, outer: bool, r: f32) {
        let r = r.max(0.0);
        if let Some(k) = &mut self.keys {
            if outer {
                k.outer[end].set(r);
            } else {
                k.inner[end].set(r);
            }
        }
        if outer {
            self.outer[end] = r;
        } else {
            self.inner[end] = r;
        }
    }
}

/// Inner and outer radius at `u`: the end radii and the cross sections
/// (u, inner, outer) between them, linearly.
pub fn radius_profile(e: &Envelope, sections: &[(f32, f32, f32)], u: f32) -> (f32, f32) {
    let (i, o) = e.radii();
    let mut pts: Vec<(f32, f32, f32)> = Vec::with_capacity(sections.len() + 2);
    pts.push((0.0, i[0], o[0]));
    let mut mid: Vec<(f32, f32, f32)> = sections.iter().copied().filter(|s| s.0 > 0.0 && s.0 < 1.0).collect();
    mid.sort_by(|a, b| a.0.total_cmp(&b.0));
    pts.extend(mid);
    pts.push((1.0, i[1], o[1]));
    for w in pts.windows(2) {
        if u <= w[1].0 {
            let t = if w[1].0 - w[0].0 < 1e-9 { 0.0 } else { (u - w[0].0) / (w[1].0 - w[0].0) };
            return (w[0].1 + (w[1].1 - w[0].1) * t, w[0].2 + (w[1].2 - w[0].2) * t);
        }
    }
    (i[1], o[1])
}

/// What an envelope gives a point: its weight and whether the point lies
/// within the inner radius.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnvelopeSample {
    pub weight: f32,
    pub inside_inner: bool,
}

/// Envelope weight of a point in the bone's bind-pose local frame, with
/// the bone's cross sections shaping the radii.
pub fn envelope_sample(e: &Envelope, sections: &[(f32, f32, f32)], p: Vec3) -> EnvelopeSample {
    let (u, closest) = e.closest(p);
    let d = p.distance(closest);
    let (inner, outer) = radius_profile(e, sections, u);
    if d <= inner {
        return EnvelopeSample { weight: e.abs_effect.max(0.0), inside_inner: true };
    }
    if d >= outer || outer <= inner {
        return EnvelopeSample { weight: 0.0, inside_inner: false };
    }
    let t = (d - inner) / (outer - inner);
    let w = match e.falloff {
        1 => 0.5 + 0.5 * (std::f32::consts::PI * t).cos(),
        2 => (1.0 - t) * (1.0 - t),
        3 => 1.0 - t * t,
        _ => 1.0 - t,
    };
    EnvelopeSample { weight: (w * e.abs_effect).max(0.0), inside_inner: false }
}

/// Envelope weight of a point in the bone's bind-pose local frame.
pub fn envelope_weight(e: &Envelope, p: Vec3) -> f32 {
    envelope_sample(e, &[], p).weight
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkinBone {
    pub node: NodeId,
    /// Inverse of the bone's world matrix at bind time.
    pub bind_inverse: Mat4,
    pub envelope: Envelope,
    /// Extra cross sections: (u along the envelope, inner, outer), sorted by u.
    #[serde(default)]
    pub cross_sections: Vec<(f32, f32, f32)>,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Envelope Visibility: the envelope stays drawn while another bone is current.
    #[serde(default)]
    pub envelope_visible: bool,
}

impl SkinBone {
    /// The cross sections of the envelope, ends included: (u, inner, outer).
    pub fn sections(&self) -> Vec<(f32, f32, f32)> {
        let (i, o) = self.envelope.radii();
        let mut out = vec![(0.0, i[0], o[0])];
        out.extend(self.cross_sections.iter().copied());
        out.push((1.0, i[1], o[1]));
        out
    }
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkinDisplay {
    pub colored_vertices: bool,
    pub colored_faces: bool,
    pub color_all_weights: bool,
    pub all_envelopes: bool,
    pub all_gizmos: bool,
    pub no_envelopes: bool,
    pub cross_sections_on_top: bool,
    pub envelope_on_top: bool,
    pub show_hidden_vertices: bool,
    /// Show All Vertices: a tick at every vertex (and every patch handle).
    #[serde(default)]
    pub all_vertices: bool,
}

impl Default for SkinDisplay {
    fn default() -> Self {
        Self { colored_vertices: true, colored_faces: false, color_all_weights: false, all_envelopes: false, all_gizmos: false, no_envelopes: false, cross_sections_on_top: true, envelope_on_top: true, show_hidden_vertices: false, all_vertices: false }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkinMirror {
    /// 0 X, 1 Y, 2 Z.
    pub plane: u32,
    pub offset: f32,
    pub threshold: f32,
    pub mode: bool,
    /// Display Projection: 0 Default Display, 1 Positive, 2 Negative, 3 None.
    #[serde(default)]
    pub projection: u32,
    /// Manual Update: the side colours change only on Update.
    #[serde(default)]
    pub manual_update: bool,
}

impl Default for SkinMirror {
    fn default() -> Self {
        Self { plane: 0, offset: 0.0, threshold: 0.5, mode: false, projection: 0, manual_update: false }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GizmoKind {
    #[default]
    JointAngle,
    BulgeAngle,
    MorphAngle,
}

impl GizmoKind {
    pub fn label(self) -> &'static str {
        match self {
            GizmoKind::JointAngle => "Joint Angle Deformer",
            GizmoKind::BulgeAngle => "Bulge Angle Deformer",
            GizmoKind::MorphAngle => "Morph Angle Deformer",
        }
    }
}

/// One angle key of a gizmo: per-vertex deltas in the child bone's frame
/// (Joint Angle, Morph Angle targets), a radial bulge scale (Bulge Angle)
/// and the lattice's control point offsets at this angle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct GizmoKey {
    pub angle_deg: f32,
    pub deltas: Vec<(u32, Vec3)>,
    pub bulge: f32,
    /// Morph Angle target name.
    #[serde(default)]
    pub name: String,
    /// Edit Lattice: per control point offset in the lattice bone's frame (empty = none).
    #[serde(default)]
    pub lattice: Vec<Vec3>,
}

/// The lattice of a Joint Angle / Bulge Angle gizmo: a box in the bind
/// frame of its bone (the child bone for Joint, the parent for Bulge),
/// `dims` points along X (the bone axis), Y and Z, turned by Twist about
/// the box's own X axis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GizmoLattice {
    pub dims: [u32; 3],
    pub lo: Vec3,
    pub hi: Vec3,
}

impl GizmoLattice {
    pub const DIMS: [u32; 3] = [3, 3, 3];

    /// The box around `pts` (bone-frame points), padded a little.
    pub fn around(pts: &[Vec3]) -> Option<Self> {
        let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
        for p in pts {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
        if !lo.is_finite() {
            return None;
        }
        let pad = ((hi - lo).max_element() * 0.05).max(1e-3);
        Some(Self { dims: Self::DIMS, lo: lo - Vec3::splat(pad), hi: hi + Vec3::splat(pad) })
    }

    pub fn count(&self) -> usize {
        self.dims.iter().map(|d| d.max(&2)).product::<u32>() as usize
    }

    /// Box frame → bone frame (Twist turns the box about its centre line along X).
    pub fn frame(&self, twist_deg: f32) -> Mat4 {
        let c = (self.lo + self.hi) * 0.5;
        Mat4::from_translation(Vec3::new(0.0, c.y, c.z)) * Mat4::from_quat(Quat::from_rotation_x(twist_deg.to_radians())) * Mat4::from_translation(Vec3::new(0.0, -c.y, -c.z))
    }

    /// Rest control points in the bone frame.
    pub fn rest_points(&self, twist_deg: f32) -> Vec<Vec3> {
        let f = self.frame(twist_deg);
        loom_mesh::ffd::rest_points(self.dims, false).into_iter().map(|u| f.transform_point3(self.lo + (self.hi - self.lo) * u)).collect()
    }

    /// Unit coordinates of a bone-frame point in the (twisted) box.
    pub fn unit(&self, p: Vec3, twist_deg: f32) -> Vec3 {
        let q = self.frame(twist_deg).inverse().transform_point3(p);
        (q - self.lo) / (self.hi - self.lo).max(Vec3::splat(1e-6))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkinGizmo {
    pub kind: GizmoKind,
    /// Index into `SkinData::bones` of the child bone whose angle drives the gizmo…
    pub bone: usize,
    /// …against this bone.
    pub parent_bone: usize,
    pub keys: Vec<GizmoKey>,
    pub influence: f32,
    pub enabled: bool,
    /// Name field.
    #[serde(default)]
    pub name: String,
    /// The vertices selected when the gizmo was added (empty = every vertex of the bones).
    #[serde(default)]
    pub vertices: Vec<u32>,
    /// Twist: the lattice's turn about the bone axis (degrees).
    #[serde(default)]
    pub twist: f32,
    /// Use Bounding Volume: every vertex inside the lattice deforms, not only `vertices`.
    #[serde(default)]
    pub bounding_volume: bool,
    #[serde(default)]
    pub lattice: Option<GizmoLattice>,
}

impl Default for SkinGizmo {
    fn default() -> Self {
        Self { kind: GizmoKind::JointAngle, bone: 0, parent_bone: 0, keys: Vec::new(), influence: 1.0, enabled: true, name: String::new(), vertices: Vec::new(), twist: 0.0, bounding_volume: false, lattice: None }
    }
}

impl SkinGizmo {
    /// The bone whose bind frame holds the lattice: the child for Joint, the parent for Bulge.
    pub fn lattice_bone(&self) -> usize {
        if self.kind == GizmoKind::BulgeAngle {
            self.parent_bone
        } else {
            self.bone
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkinData {
    pub bones: Vec<SkinBone>,
    /// The object's world matrix at bind time.
    pub mesh_bind: Mat4,
    /// Per vertex: (bone index, weight); empty = from the envelopes.
    pub weights: Vec<Vec<(u32, f32)>>,
    /// Vertices whose weights were set by hand (envelopes no longer apply).
    pub explicit: Vec<bool>,
    pub rigid: Vec<bool>,
    /// Vertices excluded from each bone.
    pub excluded: Vec<BTreeSet<u32>>,
    #[serde(default = "twenty")]
    pub bone_affect_limit: u32,
    #[serde(default = "yes")]
    pub always_deform: bool,
    #[serde(default = "yes")]
    pub weight_all_vertices: bool,
    pub skin_pose: Option<Vec<(NodeId, Transform)>>,
    pub display: SkinDisplay,
    pub mirror: SkinMirror,
    pub gizmos: Vec<SkinGizmo>,
    /// Rigid Handles: the handles of these patch vertices follow the vertex's strongest bone.
    #[serde(default)]
    pub rigid_handles: Vec<bool>,
    /// Rigid Vertices (All): every vertex follows its strongest bone only.
    #[serde(default)]
    pub rigid_all: bool,
    /// Rigid Patch Handles (All): handles take their vertex's weights.
    #[serde(default)]
    pub rigid_patch_handles_all: bool,
    /// Back Transform Vertices: the result comes back through the object's
    /// current matrix, so a mesh linked to its bones does not move twice.
    #[serde(default = "yes")]
    pub back_transform: bool,
    /// Ref. Frame: the frame whose pose becomes the bind pose when Always Deform comes back on.
    #[serde(default)]
    pub ref_frame: f32,
    /// Ignore Bone Scale: a scaled bone moves the skin as if unscaled.
    #[serde(default)]
    pub ignore_bone_scale: bool,
    /// Fast Updates: the viewports show rigid deformation without gizmos (renders stay full).
    #[serde(default)]
    pub fast_updates: bool,
    /// Update on mouse up: envelope drags apply when the button comes up.
    #[serde(default)]
    pub update_on_mouse_up: bool,
    /// Animatable Envelopes: envelope radii key under Auto Key.
    #[serde(default)]
    pub animatable_envelopes: bool,
    /// Remove Zero Limit: Remove Zero Weights strips weights below this.
    #[serde(default)]
    pub remove_zero_limit: f32,
    /// Weight Table > Vertex Sets: named vertex sets.
    #[serde(default)]
    pub vertex_sets: Vec<(String, Vec<u32>)>,
}

impl Default for SkinData {
    fn default() -> Self {
        Self {
            bones: Vec::new(),
            mesh_bind: Mat4::IDENTITY,
            weights: Vec::new(),
            explicit: Vec::new(),
            rigid: Vec::new(),
            excluded: Vec::new(),
            bone_affect_limit: 20,
            always_deform: true,
            weight_all_vertices: true,
            skin_pose: None,
            display: SkinDisplay::default(),
            mirror: SkinMirror::default(),
            gizmos: Vec::new(),
            rigid_handles: Vec::new(),
            rigid_all: false,
            rigid_patch_handles_all: false,
            back_transform: true,
            ref_frame: 0.0,
            ignore_bone_scale: false,
            fast_updates: false,
            update_on_mouse_up: false,
            animatable_envelopes: false,
            remove_zero_limit: 0.0,
            vertex_sets: Vec::new(),
        }
    }
}

fn twenty() -> u32 {
    20
}

/// The bone's envelope from its shape: a bone along its axis, a spline
/// along its curve, any other object along the longest side of its box;
/// inner radius a quarter of the length, outer half.
fn default_envelope(scene: &Scene, bone: NodeId) -> Envelope {
    let Some(n) = scene.node(bone) else { return Envelope::default() };
    if let Some(len) = n.object.bone_length() {
        let dir = n.object.bone_axis().unwrap_or(Vec3::X);
        return Envelope { start: Vec3::ZERO, end: dir * len, inner: [len * 0.25; 2], outer: [len * 0.5; 2], ..Envelope::default() };
    }
    // Object space → the node's own frame (the envelope lives in the bone's frame).
    let to_node = scene.world_matrix(bone).inverse() * scene.object_matrix(bone);
    if let Some(shape) = n.object.shape() {
        if let Some(line) = shape.polylines().into_iter().find(|l| l.len() >= 2) {
            let path: Vec<Vec3> = line.iter().map(|p| to_node.transform_point3(*p)).collect();
            let len: f32 = path.windows(2).map(|w| w[0].distance(w[1])).sum::<f32>().max(1e-3);
            return Envelope { start: path[0], end: *path.last().unwrap(), inner: [len * 0.25; 2], outer: [len * 0.5; 2], path, ..Envelope::default() };
        }
    }
    let (lo, hi) = n.object.mesh().aabb();
    let size = hi - lo;
    let axis = if size.x >= size.y && size.x >= size.z { Vec3::X } else if size.y >= size.z { Vec3::Y } else { Vec3::Z };
    let len = (size * axis).length().max(1.0);
    let c = (lo + hi) * 0.5;
    let (a, b) = (c - axis * len * 0.5, c + axis * len * 0.5);
    Envelope { start: to_node.transform_point3(a), end: to_node.transform_point3(b), inner: [len * 0.25; 2], outer: [len * 0.5; 2], ..Envelope::default() }
}

impl SkinData {
    /// Binds `node`'s current pose to `bones` (their world matrices now are the bind pose).
    pub fn bind(scene: &Scene, node: NodeId, bones: &[NodeId]) -> Self {
        let mesh = scene.node(node).map(|n| n.object.mesh().into_owned()).unwrap_or_default();
        Self::bind_mesh(scene, node, bones, &mesh)
    }

    /// Binds against `mesh`, the mesh flowing into the Skin.
    pub fn bind_mesh(scene: &Scene, node: NodeId, bones: &[NodeId], mesh: &loom_mesh::Mesh) -> Self {
        let mut data = Self { mesh_bind: scene.object_matrix(node), ..Default::default() };
        for b in bones {
            data.add_bone(scene, *b);
        }
        data.resize(mesh.vertex_count());
        // The envelope weights of the bind mesh, so hand edits start from them.
        for (i, v) in mesh.vertices().iter().enumerate() {
            data.weights[i] = data.envelope_weights(data.mesh_bind.transform_point3(v.position), Some(i as u32));
        }
        data
    }

    /// A Skin from a file: bones with their mesh→bone bind matrices (in the
    /// object's space, `mesh_bind` identity) and hand-set weights per vertex.
    /// `mesh_bind` is the mesh node's world matrix at bind, so the skin lives in
    /// world space like a Skin made in the app (bind_inverse = geometry_to_bone · mesh_bind⁻¹).
    pub fn from_file(bones: &[(NodeId, Mat4, Envelope)], mesh_bind: Mat4, weights: Vec<Vec<(u32, f32)>>) -> Self {
        let mut data = Self { mesh_bind, ..Default::default() };
        let to_mesh = mesh_bind.inverse();
        for (node, geometry_to_bone, envelope) in bones {
            data.bones.push(SkinBone { node: *node, bind_inverse: *geometry_to_bone * to_mesh, envelope: envelope.clone(), cross_sections: Vec::new(), enabled: true, envelope_visible: false });
            data.excluded.push(BTreeSet::new());
        }
        let n = weights.len();
        data.weights = weights;
        data.explicit = vec![true; n];
        data.rigid = vec![false; n];
        data
    }

    /// Re-derives the envelope weights of every vertex that is not hand-set (envelope edits).
    pub fn refresh_envelope_weights(&mut self, mesh: &loom_mesh::Mesh) {
        self.resize(mesh.vertex_count());
        for (i, v) in mesh.vertices().iter().enumerate() {
            if !self.explicit[i] {
                self.weights[i] = self.envelope_weights(self.mesh_bind.transform_point3(v.position), Some(i as u32));
            }
        }
    }

    /// Adds a bone at its current world matrix (bind).
    pub fn add_bone(&mut self, scene: &Scene, bone: NodeId) {
        if self.bones.iter().any(|b| b.node == bone) {
            return;
        }
        self.bones.push(SkinBone { node: bone, bind_inverse: scene.world_matrix(bone).inverse(), envelope: default_envelope(scene, bone), cross_sections: Vec::new(), enabled: true, envelope_visible: false });
        self.excluded.push(BTreeSet::new());
    }

    /// Removes a bone; its explicit weights go to the other bones (renormalised).
    pub fn remove_bone(&mut self, index: usize) {
        if index >= self.bones.len() {
            return;
        }
        self.bones.remove(index);
        self.excluded.remove(index);
        for w in &mut self.weights {
            w.retain(|(b, _)| *b as usize != index);
            for (b, _) in w.iter_mut() {
                if *b as usize > index {
                    *b -= 1;
                }
            }
            normalize(w);
        }
    }

    /// Weight tables sized for `n` vertices (new vertices start from the envelopes).
    pub fn resize(&mut self, n: usize) {
        self.weights.resize(n, Vec::new());
        self.explicit.resize(n, false);
        self.rigid.resize(n, false);
        self.rigid_handles.resize(n, false);
    }

    /// Every vertex becomes hand-weighted with what the envelopes give now.
    pub fn explicit_all(&mut self) {
        let n = self.weights.len();
        for i in 0..n {
            if !self.explicit[i] {
                self.explicit[i] = true;
            }
        }
    }

    fn reaches(&self, i: usize, b: &SkinBone, vertex: Option<u32>) -> bool {
        b.enabled && vertex.is_none_or(|v| !self.excluded.get(i).is_some_and(|x| x.contains(&v)))
    }

    /// Envelope weights of a bind-pose world point, limited; normalised
    /// unless only Relative envelopes reach it with less than full weight
    /// (the rest of the vertex then stays at its rest position).
    pub fn envelope_weights(&self, p_world: Vec3, vertex: Option<u32>) -> Vec<(u32, f32)> {
        let samples: Vec<(u32, EnvelopeSample, bool)> = self
            .bones
            .iter()
            .enumerate()
            .filter(|(i, b)| self.reaches(*i, b, vertex))
            .filter_map(|(i, b)| {
                let local = b.bind_inverse.transform_point3(p_world);
                let s = envelope_sample(&b.envelope, &b.cross_sections, local);
                (s.weight > 1e-6).then_some((i as u32, s, b.envelope.relative))
            })
            .collect();
        // A point inside a Relative envelope's inner radius is locked to the bones whose inner radius it is in.
        let locked: Vec<(u32, f32)> = samples.iter().filter(|(_, s, rel)| *rel && s.inside_inner).map(|(b, s, _)| (*b, s.weight)).collect();
        let mut w: Vec<(u32, f32)> = if locked.is_empty() { samples.iter().map(|(b, s, _)| (*b, s.weight)).collect() } else { locked.clone() };
        let all_relative = !samples.is_empty() && locked.is_empty() && samples.iter().all(|(_, _, rel)| *rel);
        if w.is_empty() && self.weight_all_vertices {
            // Weight All Vertices: a vertex outside every envelope goes fully to the nearest bone.
            let nearest = self
                .bones
                .iter()
                .enumerate()
                .filter(|(i, b)| self.reaches(*i, b, vertex))
                .map(|(i, b)| {
                    let local = b.bind_inverse.transform_point3(p_world);
                    let (_, closest) = b.envelope.closest(local);
                    // Distance in world units (the bind matrix may scale).
                    let d = (b.bind_inverse.inverse().transform_point3(closest) - p_world).length();
                    (i, d)
                })
                .min_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((i, _)) = nearest {
                w.push((i as u32, 1.0));
            }
        }
        limit(&mut w, self.bone_affect_limit);
        if all_relative && w.iter().map(|(_, x)| *x).sum::<f32>() < 1.0 {
            w.retain(|(_, x)| *x > 1e-7);
            w.sort_by_key(|(b, _)| *b);
        } else {
            normalize(&mut w);
        }
        w
    }

    /// The weights a vertex uses: its own when hand-set, else the envelopes';
    /// Rigid (per vertex or Rigid Vertices (All)) keeps the strongest bone only.
    pub fn vertex_weights(&self, i: usize, p_world: Vec3) -> Vec<(u32, f32)> {
        let w = if self.explicit.get(i).copied().unwrap_or(false) {
            let mut w = self.weights.get(i).cloned().unwrap_or_default();
            w.retain(|(b, _)| (*b as usize) < self.bones.len());
            limit(&mut w, self.bone_affect_limit);
            normalize(&mut w);
            w
        } else {
            self.envelope_weights(p_world, Some(i as u32))
        };
        if self.rigid_all || self.rigid.get(i).copied().unwrap_or(false) {
            return strongest(&w);
        }
        w
    }

    /// Sets the bone's weight on a vertex (Abs. Effect / Weight Tool): the
    /// other bones share what is left, in proportion.
    pub fn set_weight(&mut self, vertex: usize, bone: u32, weight: f32, current: &[(u32, f32)]) {
        let w = weight.clamp(0.0, 1.0);
        let mut out: Vec<(u32, f32)> = current.iter().copied().filter(|(b, _)| *b != bone).collect();
        let rest: f32 = out.iter().map(|(_, x)| *x).sum();
        if rest > 1e-6 {
            for (_, x) in out.iter_mut() {
                *x *= (1.0 - w) / rest;
            }
        } else if w < 1.0 && out.is_empty() {
            // Nothing else to give the remainder to: the vertex keeps this bone fully.
        }
        if w > 0.0 || out.is_empty() {
            out.push((bone, w));
        }
        normalize(&mut out);
        self.weights[vertex] = out;
        self.explicit[vertex] = true;
    }

    /// The weights a point uses on a patch: a handle (index ≥ the vertex
    /// count) takes its vertex's weights with Rigid Patch Handles (All), or
    /// that vertex's strongest bone with Rigid Handles on the vertex.
    pub fn point_weights(&self, i: usize, p_world: Vec3, handle_owner: Option<(usize, Vec3)>) -> Vec<(u32, f32)> {
        if let Some((k, k_world)) = handle_owner {
            if self.rigid_patch_handles_all {
                return self.vertex_weights(k, k_world);
            }
            if self.rigid_handles.get(k).copied().unwrap_or(false) {
                return strongest(&self.vertex_weights(k, k_world));
            }
        }
        self.vertex_weights(i, p_world)
    }
}

/// The strongest bone alone, at full weight.
pub fn strongest(w: &[(u32, f32)]) -> Vec<(u32, f32)> {
    w.iter().copied().max_by(|a, b| a.1.total_cmp(&b.1)).map(|b| vec![(b.0, 1.0)]).unwrap_or_default()
}

pub fn normalize(w: &mut Vec<(u32, f32)>) {
    w.retain(|(_, x)| *x > 1e-7);
    let sum: f32 = w.iter().map(|(_, x)| *x).sum();
    if sum > 1e-7 {
        for (_, x) in w.iter_mut() {
            *x /= sum;
        }
    }
    w.sort_by_key(|(b, _)| *b);
}

fn limit(w: &mut Vec<(u32, f32)>, limit: u32) {
    let limit = limit.max(1) as usize;
    if w.len() > limit {
        w.sort_by(|a, b| b.1.total_cmp(&a.1));
        w.truncate(limit);
    }
}

/// The patch handle → its vertex (the edge end it belongs to), for every handle.
pub fn handle_owners(patch: &loom_mesh::patch::PatchMesh) -> Vec<Option<usize>> {
    let mut out = vec![None; patch.vecs.len()];
    for e in &patch.edges {
        for s in 0..2 {
            if let Some(o) = out.get_mut(e.vec[s] as usize) {
                *o = Some(e.v[s] as usize);
            }
        }
    }
    out
}

/// The points a Skin weights in a stack value: a patch's vertices then
/// handles, else the mesh's vertices.
pub fn weighted_points(v: &StackValue) -> Vec<Vec3> {
    match &v.patch {
        Some(p) => p.verts.iter().chain(p.vecs.iter()).copied().collect(),
        None => v.mesh.vertices().iter().map(|x| x.position).collect(),
    }
}

/// A matrix without its scale (Ignore Bone Scale).
fn unscaled(m: Mat4) -> Mat4 {
    let (_, r, t) = m.to_scale_rotation_translation();
    Mat4::from_rotation_translation(r, t)
}

/// Each bone's world → world matrix now: current world × bind inverse,
/// unscaled with Ignore Bone Scale, with the envelope's Squash applied
/// across a bone that stretches with Stretch = Squash.
pub fn bone_matrices(data: &SkinData, ctx: &EvalCtx<'_>) -> Vec<Option<Mat4>> {
    data.bones
        .iter()
        .map(|b| {
            let w = ctx.node_world_matrix(b.node)?;
            let bind = b.bind_inverse.inverse();
            let (w, bind) = if data.ignore_bone_scale { (unscaled(w), unscaled(bind)) } else { (w, bind) };
            let mut m = w * bind.inverse();
            if ctx.bone_squashes.is_some_and(|f| f(b.node)) {
                // Stretch along the bone axis relative to the bind pose; Squash scales across it by s^(-squash/2).
                let axis = (b.envelope.end - b.envelope.start).normalize_or(Vec3::X);
                let s = w.transform_vector3(axis).length() / bind.transform_vector3(axis).length().max(1e-9);
                if (s - 1.0).abs() > 1e-5 {
                    let k = s.powf(-0.5 * b.envelope.squash);
                    let across = Mat4::from_cols_array_2d(&{
                        // k·I + (1 − k)·a·aᵀ: scale k across the axis, 1 along it (bone frame).
                        let a = axis;
                        let c = |i: usize, j: usize| (if i == j { k } else { 0.0 }) + (1.0 - k) * a[i] * a[j];
                        [[c(0, 0), c(0, 1), c(0, 2), 0.0], [c(1, 0), c(1, 1), c(1, 2), 0.0], [c(2, 0), c(2, 1), c(2, 2), 0.0], [0.0, 0.0, 0.0, 1.0]]
                    });
                    m = w * across * bind.inverse();
                }
            }
            Some(m)
        })
        .collect()
}

/// Linear blend skinning of the stack value against the bind pose.
pub fn apply(data: &SkinData, mut v: StackValue, ctx: &EvalCtx<'_>) -> Result<StackValue, String> {
    let positions = weighted_points(&v);
    let n = positions.len();
    let mut data = std::borrow::Cow::Borrowed(data);
    if data.weights.len() != n {
        let d = data.to_mut();
        let kept = d.weights.len().min(n);
        d.resize(n);
        v.warnings.push(format!("Topology changed below Skin: {} vertices keep their weights, {} re-weighted from envelopes", kept, n.saturating_sub(kept)));
    }
    // Always Deform off: no deformation until it is turned on again (which binds anew).
    if data.bones.is_empty() || !data.always_deform {
        return Ok(v);
    }
    let fast = data.fast_updates && !crate::anim::rendering();
    let mats = bone_matrices(&data, ctx);
    let mesh_bind = data.mesh_bind;
    let back = if data.back_transform { ctx.object_inverse } else { mesh_bind.inverse() };
    let nv = v.patch.as_ref().map_or(n, |p| p.verts.len());
    let owners = v.patch.as_ref().map(handle_owners).unwrap_or_default();
    let mut out = positions.clone();
    for (i, p) in positions.iter().enumerate() {
        let p_world = mesh_bind.transform_point3(*p);
        let owner = if i >= nv { owners.get(i - nv).copied().flatten().map(|k| (k, mesh_bind.transform_point3(positions[k]))) } else { None };
        let mut w = data.point_weights(i, p_world, owner);
        if fast {
            w = strongest(&w);
        }
        let sum: f32 = w.iter().map(|(_, x)| *x).sum();
        if sum <= 1e-6 {
            continue;
        }
        let mut acc = Vec3::ZERO;
        let mut total = 0.0;
        for (b, x) in &w {
            if let Some(m) = mats.get(*b as usize).copied().flatten() {
                acc += m.transform_point3(p_world) * *x;
                total += *x;
            }
        }
        if total > 1e-6 {
            // A Relative vertex with less than full weight keeps the rest at its own (undeformed) place.
            let k = sum.min(1.0);
            let world = acc / total * k + ctx.object_world.transform_point3(*p) * (1.0 - k);
            out[i] = back.transform_point3(world);
        }
    }
    if !fast {
        // Gizmos: Joint Angle / Bulge Angle lattices and deltas, Morph Angle targets, driven by the bones' current angles.
        gizmos::apply(&data, ctx, &positions, &mut out);
    }
    match &mut v.patch {
        Some(patch) => {
            let (verts, vecs) = out.split_at(nv);
            patch.verts = verts.to_vec();
            patch.vecs = vecs.to_vec();
            let (steps, interior) = v.patch_steps.unwrap_or((5, false));
            v.mesh = patch.tessellate(steps, interior);
        }
        None => {
            for (vx, p) in v.mesh.vertices_mut().iter_mut().zip(out) {
                vx.position = p;
            }
        }
    }
    Ok(v)
}

/// `Bone_L` ↔ `Bone_R`, `LeftArm` ↔ `RightArm` (for bones Mirror Mode cannot pair by position).
pub fn mirrored_bone_name(name: &str) -> Option<String> {
    for (a, b) in [("_L", "_R"), ("_R", "_L"), ("Left", "Right"), ("Right", "Left"), (" L", " R"), (" R", " L"), ("L_", "R_"), ("R_", "L_")] {
        if let Some(stem) = name.strip_suffix(a) {
            return Some(format!("{stem}{b}"));
        }
        if let Some(rest) = name.strip_prefix(a) {
            return Some(format!("{b}{rest}"));
        }
        if name.contains(a) && !name.contains(b) {
            return Some(name.replacen(a, b, 1));
        }
    }
    None
}

/// The bones' bind bases in the object's space.
pub fn bone_bases(data: &SkinData) -> Vec<Vec3> {
    let to_obj = data.mesh_bind.inverse();
    data.bones.iter().map(|b| to_obj.transform_point3(b.bind_inverse.inverse().transform_point3(Vec3::ZERO))).collect()
}

/// Mirror Mode: the bone across the plane for each bone (by its bind base
/// within `threshold`, else by name), as indices; a bone with no partner maps to itself.
pub fn mirror_bone_pairs(data: &SkinData, names: &[String], plane: u32, offset: f32, threshold: f32) -> Vec<usize> {
    let a = (plane.min(2)) as usize;
    let bases = bone_bases(data);
    let mirror = |mut p: Vec3| {
        p[a] = 2.0 * offset - p[a];
        p
    };
    (0..data.bones.len())
        .map(|i| {
            let m = mirror(bases[i]);
            let by_pos = (0..data.bones.len()).filter(|j| *j != i).map(|j| (j, bases[j].distance(m))).filter(|(_, d)| *d <= threshold).min_by(|x, y| x.1.total_cmp(&y.1)).map(|(j, _)| j);
            by_pos
                .or_else(|| {
                    let want = mirrored_bone_name(names.get(i)?)?;
                    names.iter().position(|n| *n == want)
                })
                .unwrap_or(i)
        })
        .collect()
}

/// Mirror Mode side of a point: -1 green (negative side), 1 blue (positive),
/// 0 red (on the plane, or without a mirrored partner within the threshold).
pub fn mirror_sides(points: &[Vec3], plane: u32, offset: f32, threshold: f32) -> Vec<i8> {
    let a = plane.min(2) as usize;
    points
        .iter()
        .map(|p| {
            let d = p[a] - offset;
            if d.abs() <= 1e-6 {
                return 0;
            }
            let mut m = *p;
            m[a] = 2.0 * offset - m[a];
            let paired = points.iter().any(|q| (q[a] - offset) * d < 0.0 && q.distance(m) <= threshold);
            if !paired {
                0
            } else if d < 0.0 {
                -1
            } else {
                1
            }
        })
        .collect()
}

/// Mirror Mode side of each bone (by its bind base): -1 green, 1 blue, 0 red.
pub fn bone_sides(data: &SkinData, names: &[String]) -> Vec<i8> {
    let m = &data.mirror;
    let a = m.plane.min(2) as usize;
    let pairs = mirror_bone_pairs(data, names, m.plane, m.offset, m.threshold);
    bone_bases(data)
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if pairs[i] == i {
                0
            } else if p[a] < m.offset {
                -1
            } else {
                1
            }
        })
        .collect()
}

/// Mirror Mode > Paste Green to Blue Bones / Blue to Green Bones: each
/// source-side bone's envelope (mirrored across the plane) lands on its
/// partner. Returns the count pasted.
pub fn mirror_envelopes(data: &mut SkinData, names: &[String], green_to_blue: bool) -> usize {
    let sides = bone_sides(data, names);
    let m = data.mirror.clone();
    let pairs = mirror_bone_pairs(data, names, m.plane, m.offset, m.threshold);
    let a = m.plane.min(2) as usize;
    let mesh_bind = data.mesh_bind;
    let flip = |p: Vec3| {
        let mut q = mesh_bind.inverse().transform_point3(p);
        q[a] = 2.0 * m.offset - q[a];
        mesh_bind.transform_point3(q)
    };
    let src = if green_to_blue { -1 } else { 1 };
    let mut pasted = 0;
    let before = data.bones.clone();
    for (i, b) in before.iter().enumerate() {
        let j = pairs[i];
        if sides[i] != src || j == i {
            continue;
        }
        // Envelope end points: source bone frame → world → mirrored → target bone frame.
        let to_world = b.bind_inverse.inverse();
        let target_inv = before[j].bind_inverse;
        let map = |p: Vec3| target_inv.transform_point3(flip(to_world.transform_point3(p)));
        let e = &mut data.bones[j];
        e.envelope = Envelope { start: map(b.envelope.start), end: map(b.envelope.end), path: b.envelope.path.iter().map(|p| map(*p)).collect(), ..b.envelope.clone() };
        e.cross_sections = b.cross_sections.clone();
        pasted += 1;
    }
    pasted
}

/// Mirror Mode > Paste Green to Blue / Blue to Green: weights of the vertices on
/// the source side of the plane (green = negative, blue = positive) land on the
/// vertex mirrored across it, bones swapped by `mirror_bone_pairs`. Returns the count pasted.
pub fn mirror_weights(data: &mut SkinData, mesh: &loom_mesh::Mesh, plane: u32, offset: f32, threshold: f32, green_to_blue: bool) -> usize {
    mirror_weights_named(data, mesh, &vec![String::new(); data.bones.len()], plane, offset, threshold, green_to_blue)
}

pub fn mirror_weights_named(data: &mut SkinData, mesh: &loom_mesh::Mesh, names: &[String], plane: u32, offset: f32, threshold: f32, green_to_blue: bool) -> usize {
    let a = (plane.min(2)) as usize;
    let pairs = mirror_bone_pairs(data, names, plane, offset, threshold);
    let n = mesh.vertex_count();
    data.resize(n);
    let positions: Vec<Vec3> = mesh.vertices().iter().map(|v| v.position).collect();
    let source_side = |p: Vec3| if green_to_blue { p[a] < offset } else { p[a] > offset };
    let targets: Vec<usize> = (0..n).filter(|i| !source_side(positions[*i]) && (positions[*i][a] - offset).abs() > 1e-6).collect();
    let mut pasted = 0;
    let current: Vec<Vec<(u32, f32)>> = (0..n).map(|i| data.vertex_weights(i, data.mesh_bind.transform_point3(positions[i]))).collect();
    for i in 0..n {
        if !source_side(positions[i]) {
            continue;
        }
        let mut m = positions[i];
        m[a] = 2.0 * offset - m[a];
        let Some((t, _)) = targets.iter().map(|t| (*t, positions[*t].distance(m))).filter(|(_, d)| *d <= threshold).min_by(|x, y| x.1.total_cmp(&y.1)) else { continue };
        let mut w: Vec<(u32, f32)> = current[i].iter().map(|(b, x)| (pairs[*b as usize] as u32, *x)).collect();
        normalize(&mut w);
        data.weights[t] = w;
        data.explicit[t] = true;
        pasted += 1;
    }
    pasted
}

/// The angle (degrees) a child bone has turned from its bind direction, in the parent bone's frame.
pub fn bone_angle(data: &SkinData, world: &dyn Fn(NodeId) -> Option<Mat4>, bone: usize, parent: usize) -> Option<f32> {
    let (b, p) = (data.bones.get(bone)?, data.bones.get(parent)?);
    let bw = world(b.node)?;
    let pw = world(p.node)?;
    let bind_dir_world = b.bind_inverse.inverse().transform_vector3(Vec3::X).normalize_or_zero();
    let bind_dir_parent = p.bind_inverse.transform_vector3(bind_dir_world).normalize_or_zero();
    let cur_dir_parent = pw.inverse().transform_vector3(bw.transform_vector3(Vec3::X)).normalize_or_zero();
    Some(bind_dir_parent.dot(cur_dir_parent).clamp(-1.0, 1.0).acos().to_degrees())
}

pub mod gizmos {
    use super::*;

    /// The angle (degrees) between a bone's bind direction and its current direction, in the parent bone's frame.
    pub fn bone_angle(data: &SkinData, ctx: &EvalCtx<'_>, bone: usize, parent: usize) -> Option<f32> {
        super::bone_angle(data, &|id| ctx.node_world_matrix(id), bone, parent)
    }

    /// Interpolated key value at an angle: linear between the neighbouring keys, clamped outside.
    pub fn key_blend(keys: &[GizmoKey], angle: f32) -> Vec<(f32, usize)> {
        if keys.is_empty() {
            return Vec::new();
        }
        let mut sorted: Vec<usize> = (0..keys.len()).collect();
        sorted.sort_by(|a, b| keys[*a].angle_deg.total_cmp(&keys[*b].angle_deg));
        if angle <= keys[sorted[0]].angle_deg {
            return vec![(1.0, sorted[0])];
        }
        if angle >= keys[*sorted.last().unwrap()].angle_deg {
            return vec![(1.0, *sorted.last().unwrap())];
        }
        for w in sorted.windows(2) {
            let (a, b) = (keys[w[0]].angle_deg, keys[w[1]].angle_deg);
            if angle >= a && angle <= b {
                let t = if (b - a).abs() < 1e-6 { 0.0 } else { (angle - a) / (b - a) };
                return vec![(1.0 - t, w[0]), (t, w[1])];
            }
        }
        Vec::new()
    }

    /// The lattice offsets (bone frame) of a gizmo at an angle, blended between keys.
    pub fn lattice_offsets(g: &SkinGizmo, blend: &[(f32, usize)]) -> Option<Vec<Vec3>> {
        let lat = g.lattice.as_ref()?;
        let n = lat.count();
        let mut out = vec![Vec3::ZERO; n];
        let mut any = false;
        for (w, k) in blend {
            let key = &g.keys[*k];
            if key.lattice.len() == n {
                any = true;
                for (o, d) in out.iter_mut().zip(&key.lattice) {
                    *o += *d * *w;
                }
            }
        }
        any.then_some(out)
    }

    /// Whether vertex `i` belongs to the gizmo: its stored vertices, or (with
    /// none stored) every vertex its bones weight; Bulge Angle only the parent bone's.
    fn member(data: &SkinData, g: &SkinGizmo, i: usize, w: &[(u32, f32)]) -> bool {
        if !g.vertices.is_empty() {
            return g.vertices.binary_search(&(i as u32)).is_ok() || g.vertices.contains(&(i as u32));
        }
        let on = |b: usize| w.iter().any(|(x, y)| *x as usize == b && *y > 0.0);
        let _ = data;
        match g.kind {
            GizmoKind::BulgeAngle => on(g.parent_bone),
            _ => on(g.bone) || on(g.parent_bone),
        }
    }

    pub fn apply(data: &SkinData, ctx: &EvalCtx<'_>, rest: &[Vec3], out: &mut [Vec3]) {
        for g in data.gizmos.iter().filter(|g| g.enabled && !g.keys.is_empty()) {
            let Some(angle) = bone_angle(data, ctx, g.bone, g.parent_bone) else { continue };
            let blend = key_blend(&g.keys, angle);
            let Some(bone) = data.bones.get(g.bone) else { continue };
            let Some(bw) = ctx.node_world_matrix(bone.node) else { continue };
            // Joint Angle deltas and Morph Angle targets: per-vertex deltas in the child bone's frame.
            if matches!(g.kind, GizmoKind::JointAngle | GizmoKind::MorphAngle) {
                for (w, k) in &blend {
                    for (vi, delta) in &g.keys[*k].deltas {
                        if let Some(p) = out.get_mut(*vi as usize) {
                            let world = bw.transform_vector3(*delta) * *w * g.influence;
                            *p += ctx.object_inverse.transform_vector3(world);
                        }
                    }
                }
            }
            // Edit Lattice: the lattice's offsets at this angle move the vertices inside it (FFD weights).
            if let (Some(lat), Some(offsets)) = (g.lattice.as_ref(), lattice_offsets(g, &blend)) {
                let lb = g.lattice_bone();
                if let (Some(lbone), Some(lw)) = (data.bones.get(lb), data.bones.get(lb).and_then(|b| ctx.node_world_matrix(b.node))) {
                    for (i, p) in out.iter_mut().enumerate() {
                        let pw = data.mesh_bind.transform_point3(rest[i]);
                        let local = lbone.bind_inverse.transform_point3(pw);
                        let u = lat.unit(local, g.twist);
                        let inside = (0.0..=1.0).contains(&u.x) && (0.0..=1.0).contains(&u.y) && (0.0..=1.0).contains(&u.z);
                        let take = if g.bounding_volume { inside } else { inside && member(data, g, i, &data.vertex_weights(i, pw)) };
                        if !take {
                            continue;
                        }
                        let fw = loom_mesh::ffd::weights(lat.dims, false, u);
                        let d: Vec3 = fw.iter().zip(&offsets).map(|(w, o)| *o * *w).sum();
                        if d.length_squared() > 0.0 {
                            let twisted = lat.frame(g.twist).transform_vector3(d);
                            *p += ctx.object_inverse.transform_vector3(lw.transform_vector3(twisted) * g.influence);
                        }
                    }
                }
            }
            if g.kind == GizmoKind::BulgeAngle {
                let bulge: f32 = blend.iter().map(|(w, k)| w * g.keys[*k].bulge).sum::<f32>() * g.influence;
                if bulge.abs() < 1e-6 {
                    continue;
                }
                // Vertices weighted to both bones bulge away from the child bone's axis.
                let axis_o = bw.transform_point3(Vec3::ZERO);
                let axis_d = bw.transform_vector3(Vec3::X).normalize_or_zero();
                for (i, p) in out.iter_mut().enumerate() {
                    if !g.vertices.is_empty() && !g.vertices.contains(&(i as u32)) {
                        continue;
                    }
                    let pw = data.mesh_bind.transform_point3(rest[i]);
                    let w = data.vertex_weights(i, pw);
                    let wb = w.iter().find(|(b, _)| *b as usize == g.bone).map(|x| x.1).unwrap_or(0.0);
                    let wp = w.iter().find(|(b, _)| *b as usize == g.parent_bone).map(|x| x.1).unwrap_or(0.0);
                    let share = (wb.min(wp) * 2.0).min(1.0);
                    if share < 0.05 {
                        continue;
                    }
                    let world = ctx.object_world.transform_point3(*p);
                    let along = (world - axis_o).dot(axis_d);
                    let foot = axis_o + axis_d * along;
                    let radial = world - foot;
                    let moved = foot + radial * (1.0 + bulge * share);
                    *p = ctx.object_inverse.transform_point3(moved);
                }
            }
        }
    }
}
