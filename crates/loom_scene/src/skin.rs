//! The Skin modifier: bones with envelopes, per-vertex weights (from the
//! envelopes or set by hand), linear blend skinning against the bind pose,
//! mirror settings, Joint Angle / Bulge Angle gizmos.

use crate::modifier::{EvalCtx, StackValue};
use crate::{NodeId, Scene, Transform};
use glam::{Mat4, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// A capsule in the bone's bind-pose local frame: radius `inner` → weight 1,
/// `outer` → 0, interpolated along the start→end segment.
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
    pub squash: f32,
}

impl Default for Envelope {
    fn default() -> Self {
        Self { start: Vec3::ZERO, end: Vec3::X * 10.0, inner: [2.5, 2.5], outer: [5.0, 5.0], falloff: 0, abs_effect: 1.0, squash: 1.0 }
    }
}

/// Envelope weight of a point in the bone's bind-pose local frame.
pub fn envelope_weight(e: &Envelope, p: Vec3) -> f32 {
    let seg = e.end - e.start;
    let len2 = seg.length_squared();
    let u = if len2 < 1e-12 { 0.0 } else { ((p - e.start).dot(seg) / len2).clamp(0.0, 1.0) };
    let closest = e.start + seg * u;
    let d = p.distance(closest);
    let inner = e.inner[0] + (e.inner[1] - e.inner[0]) * u;
    let outer = e.outer[0] + (e.outer[1] - e.outer[0]) * u;
    if d <= inner {
        return e.abs_effect.max(0.0);
    }
    if d >= outer || outer <= inner {
        return 0.0;
    }
    let t = (d - inner) / (outer - inner);
    let w = match e.falloff {
        1 => 0.5 + 0.5 * (std::f32::consts::PI * t).cos(),
        2 => (1.0 - t) * (1.0 - t),
        3 => 1.0 - t * t,
        _ => 1.0 - t,
    };
    (w * e.abs_effect).max(0.0)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkinBone {
    pub node: NodeId,
    /// Inverse of the bone's world matrix at bind time.
    pub bind_inverse: Mat4,
    pub envelope: Envelope,
    /// Extra cross sections: (u along the envelope, inner, outer).
    #[serde(default)]
    pub cross_sections: Vec<(f32, f32, f32)>,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Envelope Visibility: the envelope stays drawn while another bone is current.
    #[serde(default)]
    pub envelope_visible: bool,
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
}

impl Default for SkinDisplay {
    fn default() -> Self {
        Self { colored_vertices: true, colored_faces: false, color_all_weights: false, all_envelopes: false, all_gizmos: false, no_envelopes: false, cross_sections_on_top: true, envelope_on_top: true, show_hidden_vertices: false }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkinMirror {
    /// 0 X, 1 Y, 2 Z.
    pub plane: u32,
    pub offset: f32,
    pub threshold: f32,
    pub mode: bool,
}

impl Default for SkinMirror {
    fn default() -> Self {
        Self { plane: 0, offset: 0.0, threshold: 0.5, mode: false }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GizmoKind {
    JointAngle,
    BulgeAngle,
}

/// One angle key of a gizmo: Joint Angle keeps per-vertex deltas in the
/// child bone's frame, Bulge Angle a radial scale.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GizmoKey {
    pub angle_deg: f32,
    pub deltas: Vec<(u32, Vec3)>,
    pub bulge: f32,
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
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
}

fn twenty() -> u32 {
    20
}

/// The bone's envelope from its length: inner a quarter, outer half.
fn default_envelope(scene: &Scene, bone: NodeId) -> Envelope {
    let len = scene.node(bone).and_then(|n| n.object.bone_length()).unwrap_or_else(|| {
        scene.node(bone).map(|n| { let (lo, hi) = n.object.mesh().aabb(); (hi - lo).max_element().max(1.0) }).unwrap_or(10.0)
    });
    let dir = scene.node(bone).and_then(|n| n.object.bone_axis()).unwrap_or(Vec3::X);
    Envelope { start: Vec3::ZERO, end: dir * len, inner: [len * 0.25; 2], outer: [len * 0.5; 2], falloff: 0, abs_effect: 1.0, squash: 1.0 }
}

impl SkinData {
    /// Binds `node`'s current pose to `bones` (their world matrices now are the bind pose).
    pub fn bind(scene: &Scene, node: NodeId, bones: &[NodeId]) -> Self {
        let mesh = scene.node(node).map(|n| n.object.mesh().into_owned()).unwrap_or_default();
        Self::bind_mesh(scene, node, bones, &mesh)
    }

    /// Binds against `mesh`, the mesh flowing into the Skin.
    pub fn bind_mesh(scene: &Scene, node: NodeId, bones: &[NodeId], mesh: &loom_mesh::Mesh) -> Self {
        let mut data = Self { mesh_bind: scene.object_matrix(node), bone_affect_limit: 20, always_deform: true, weight_all_vertices: true, ..Default::default() };
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
        let mut data = Self { mesh_bind, bone_affect_limit: 20, always_deform: true, weight_all_vertices: true, ..Default::default() };
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

    /// Envelope weights of a bind-pose world point, limited and normalised.
    pub fn envelope_weights(&self, p_world: Vec3, vertex: Option<u32>) -> Vec<(u32, f32)> {
        let mut w: Vec<(u32, f32)> = self
            .bones
            .iter()
            .enumerate()
            .filter(|(i, b)| b.enabled && vertex.is_none_or(|v| !self.excluded.get(*i).is_some_and(|x| x.contains(&v))))
            .filter_map(|(i, b)| {
                let local = b.bind_inverse.transform_point3(p_world);
                let mut e = envelope_weight(&b.envelope, local);
                for (u, inner, outer) in &b.cross_sections {
                    let cs = Envelope { start: b.envelope.start.lerp(b.envelope.end, *u), end: b.envelope.start.lerp(b.envelope.end, *u), inner: [*inner; 2], outer: [*outer; 2], ..b.envelope.clone() };
                    e = e.max(envelope_weight(&cs, local));
                }
                (e > 1e-6).then_some((i as u32, e))
            })
            .collect();
        if w.is_empty() && self.weight_all_vertices {
            // Weight All Vertices: a vertex outside every envelope goes fully to the nearest bone.
            let nearest = self
                .bones
                .iter()
                .enumerate()
                .filter(|(i, b)| b.enabled && vertex.is_none_or(|v| !self.excluded.get(*i).is_some_and(|x| x.contains(&v))))
                .map(|(i, b)| {
                    let local = b.bind_inverse.transform_point3(p_world);
                    let seg = b.envelope.end - b.envelope.start;
                    let u = if seg.length_squared() < 1e-12 { 0.0 } else { ((local - b.envelope.start).dot(seg) / seg.length_squared()).clamp(0.0, 1.0) };
                    // Distance in world units (the bind matrix may scale).
                    let closest = b.envelope.start + seg * u;
                    let d = (b.bind_inverse.inverse().transform_point3(closest) - p_world).length();
                    (i, d)
                })
                .min_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((i, _)) = nearest {
                w.push((i as u32, 1.0));
            }
        }
        limit_and_normalize(&mut w, self.bone_affect_limit);
        w
    }

    /// The weights a vertex uses: its own when hand-set, else the envelopes'.
    pub fn vertex_weights(&self, i: usize, p_world: Vec3) -> Vec<(u32, f32)> {
        if self.explicit.get(i).copied().unwrap_or(false) {
            let mut w = self.weights.get(i).cloned().unwrap_or_default();
            w.retain(|(b, _)| (*b as usize) < self.bones.len());
            if self.rigid.get(i).copied().unwrap_or(false) {
                if let Some(best) = w.iter().copied().max_by(|a, b| a.1.total_cmp(&b.1)) {
                    return vec![(best.0, 1.0)];
                }
            }
            limit_and_normalize(&mut w, self.bone_affect_limit);
            return w;
        }
        self.envelope_weights(p_world, Some(i as u32))
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

fn limit_and_normalize(w: &mut Vec<(u32, f32)>, limit: u32) {
    let limit = limit.max(1) as usize;
    if w.len() > limit {
        w.sort_by(|a, b| b.1.total_cmp(&a.1));
        w.truncate(limit);
    }
    normalize(w);
}

/// Linear blend skinning of the stack value against the bind pose.
pub fn apply(data: &SkinData, mut v: StackValue, ctx: &EvalCtx<'_>) -> Result<StackValue, String> {
    let n = v.mesh.vertex_count();
    let mut data = std::borrow::Cow::Borrowed(data);
    if data.weights.len() != n {
        let d = data.to_mut();
        let kept = d.weights.len().min(n);
        let dropped = d.weights.len().saturating_sub(n);
        d.resize(n);
        v.warnings.push(format!("Topology changed below Skin: {} vertices keep their weights, {} re-weighted from envelopes", kept, n.saturating_sub(kept) + dropped.min(0)));
    }
    if data.bones.is_empty() {
        return Ok(v);
    }
    // Bone matrices: current world × bind inverse (world → world).
    let mats: Vec<Option<Mat4>> = data.bones.iter().map(|b| ctx.node_world_matrix(b.node).map(|w| w * b.bind_inverse)).collect();
    if !data.always_deform && mats.iter().any(|m| m.is_none()) {
        return Ok(v);
    }
    let mesh_bind = data.mesh_bind;
    let positions: Vec<Vec3> = v.mesh.vertices().iter().map(|vx| vx.position).collect();
    let mut out = positions.clone();
    for (i, p) in positions.iter().enumerate() {
        let p_world = mesh_bind.transform_point3(*p);
        let w = data.vertex_weights(i, p_world);
        if w.is_empty() {
            if !data.always_deform {
                return Ok(v);
            }
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
            out[i] = ctx.object_inverse.transform_point3(acc / total);
        }
    }
    // Gizmos: Joint Angle deltas and Bulge Angle scaling driven by the bones' current angles.
    gizmos::apply(&data, ctx, &positions, &mut out);
    for (vx, p) in v.mesh.vertices_mut().iter_mut().zip(out) {
        vx.position = p;
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

/// Mirror Mode: the bone across the plane for each bone (by its bind base
/// within `threshold`, else by name), as indices; a bone with no partner maps to itself.
pub fn mirror_bone_pairs(data: &SkinData, names: &[String], plane: u32, offset: f32, threshold: f32) -> Vec<usize> {
    let a = (plane.min(2)) as usize;
    let to_obj = data.mesh_bind.inverse();
    let bases: Vec<Vec3> = data.bones.iter().map(|b| to_obj.transform_point3(b.bind_inverse.inverse().transform_point3(Vec3::ZERO))).collect();
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

    pub fn apply(data: &SkinData, ctx: &EvalCtx<'_>, rest: &[Vec3], out: &mut [Vec3]) {
        for g in data.gizmos.iter().filter(|g| g.enabled && !g.keys.is_empty()) {
            let Some(angle) = bone_angle(data, ctx, g.bone, g.parent_bone) else { continue };
            let blend = key_blend(&g.keys, angle);
            let Some(bone) = data.bones.get(g.bone) else { continue };
            let Some(bw) = ctx.node_world_matrix(bone.node) else { continue };
            match g.kind {
                GizmoKind::JointAngle => {
                    for (w, k) in &blend {
                        for (vi, delta) in &g.keys[*k].deltas {
                            if let Some(p) = out.get_mut(*vi as usize) {
                                let world = bw.transform_vector3(*delta) * *w * g.influence;
                                *p += ctx.object_inverse.transform_vector3(world);
                            }
                        }
                    }
                }
                GizmoKind::BulgeAngle => {
                    let bulge: f32 = blend.iter().map(|(w, k)| w * g.keys[*k].bulge).sum::<f32>() * g.influence;
                    if bulge.abs() < 1e-6 {
                        continue;
                    }
                    // Vertices weighted to both bones bulge away from the child bone's axis.
                    let axis_o = bw.transform_point3(Vec3::ZERO);
                    let axis_d = bw.transform_vector3(Vec3::X).normalize_or_zero();
                    for (i, p) in out.iter_mut().enumerate() {
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
}
