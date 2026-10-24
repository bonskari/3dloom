//! Morpher (channels with progressive targets), Skin Morph (bone-angle
//! driven vertex deltas) and Skin Wrap (a mesh that follows a deformed base).

use crate::modifier::{EvalCtx, StackValue};
use crate::{NodeId, Param, Scene};
use glam::{Mat4, Vec3};
use loom_mesh::Mesh;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------- Morpher

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MorphTarget {
    /// The scene object the target came from (Reload re-captures it); None for a captured state.
    pub node: Option<NodeId>,
    /// Object-space offsets per vertex of the mesh below the Morpher.
    pub offsets: Vec<Vec3>,
    /// Progressive morph percent at which this target is reached fully (the main target: 100).
    pub percent: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MorphChannel {
    pub name: String,
    pub targets: Vec<MorphTarget>,
    pub value: Param<f32>,
    pub active: bool,
    pub use_limits: bool,
    pub min: f32,
    pub max: f32,
    pub tension: f32,
    /// Every target has one offset per vertex (a wrong vertex count greys the channel out).
    pub valid: bool,
}

impl Default for MorphChannel {
    fn default() -> Self {
        Self { name: "- empty -".into(), targets: Vec::new(), value: Param::new(0.0), active: true, use_limits: true, min: 0.0, max: 100.0, tension: 0.5, valid: true }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MorpherData {
    pub channels: Vec<MorphChannel>,
    pub use_vertex_selection: bool,
    pub spinner_min: f32,
    pub spinner_max: f32,
}

impl Default for MorpherData {
    fn default() -> Self {
        Self { channels: Vec::new(), use_vertex_selection: false, spinner_min: 0.0, spinner_max: 100.0 }
    }
}

/// The Morpher has 100 channels; they are created as they are used.
pub const MORPHER_CHANNELS: usize = 100;

impl MorpherData {
    pub fn channel(&mut self, i: usize) -> &mut MorphChannel {
        let i = i.min(MORPHER_CHANNELS - 1);
        if self.channels.len() <= i {
            self.channels.resize_with(i + 1, MorphChannel::default);
        }
        &mut self.channels[i]
    }

    pub fn channel_ref(&self, i: usize) -> Option<&MorphChannel> {
        self.channels.get(i)
    }

    /// The object-space offsets of `target_mesh` against `base_mesh` (vertex counts must match).
    pub fn offsets_between(base_mesh: &Mesh, target_mesh: &Mesh) -> Option<Vec<Vec3>> {
        if base_mesh.vertex_count() != target_mesh.vertex_count() {
            return None;
        }
        Some(base_mesh.vertices().iter().zip(target_mesh.vertices()).map(|(a, b)| b.position - a.position).collect())
    }

    /// Pick Object From Scene: channel `i` takes `target`'s evaluated mesh (its own
    /// object space) against the mesh below the Morpher of `base` (`below`: the
    /// stack entry index of the Morpher, None = above the base object only).
    pub fn pick_target(&mut self, scene: &Scene, base: NodeId, below: Option<usize>, i: usize, target: NodeId) {
        let base_mesh = match below {
            Some(index) if index > 0 => scene.evaluated(base, Some(index - 1)).value.mesh.clone(),
            _ => scene.node(base).map(|n| n.object.display_output()).unwrap_or_default(),
        };
        let target_mesh = scene.eval_mesh(target).value.mesh.clone();
        let name = scene.node(target).map(|n| n.name.clone()).unwrap_or_else(|| "Target".into());
        let offsets = Self::offsets_between(&base_mesh, &target_mesh);
        let ch = self.channel(i);
        ch.name = name;
        ch.valid = offsets.is_some();
        ch.targets = vec![MorphTarget { node: Some(target), offsets: offsets.unwrap_or_default(), percent: 100.0 }];
    }

    /// Reload: every target with a scene object re-captures it.
    pub fn reload(&mut self, scene: &Scene, base: NodeId, below: Option<usize>) {
        let base_mesh = match below {
            Some(index) if index > 0 => scene.evaluated(base, Some(index - 1)).value.mesh.clone(),
            _ => scene.node(base).map(|n| n.object.display_output()).unwrap_or_default(),
        };
        for ch in &mut self.channels {
            let mut valid = true;
            for t in &mut ch.targets {
                if let Some(node) = t.node {
                    if scene.node(node).is_some() {
                        match Self::offsets_between(&base_mesh, &scene.eval_mesh(node).value.mesh) {
                            Some(o) => t.offsets = o,
                            None => valid = false,
                        }
                    }
                }
                if t.offsets.len() != base_mesh.vertex_count() {
                    valid = false;
                }
            }
            ch.valid = valid && !ch.targets.is_empty() || ch.targets.is_empty();
        }
    }
}

/// Progressive morph: the offsets at `value` (0..100) through the targets sorted by percent.
fn channel_offsets(ch: &MorphChannel, value: f32, n: usize) -> Option<Vec<Vec3>> {
    let mut targets: Vec<&MorphTarget> = ch.targets.iter().filter(|t| t.offsets.len() == n).collect();
    if targets.is_empty() {
        return None;
    }
    targets.sort_by(|a, b| a.percent.total_cmp(&b.percent));
    let first = targets[0];
    if value <= first.percent {
        let k = if first.percent.abs() < 1e-6 { 1.0 } else { value / first.percent };
        return Some(first.offsets.iter().map(|o| *o * k).collect());
    }
    for w in targets.windows(2) {
        let (a, b) = (w[0], w[1]);
        if value >= a.percent && value <= b.percent {
            let t = if (b.percent - a.percent).abs() < 1e-6 { 1.0 } else { (value - a.percent) / (b.percent - a.percent) };
            return Some(a.offsets.iter().zip(&b.offsets).map(|(x, y)| x.lerp(*y, t)).collect());
        }
    }
    let last = targets[targets.len() - 1];
    let k = if last.percent.abs() < 1e-6 { 1.0 } else { value / last.percent };
    Some(last.offsets.iter().map(|o| *o * k).collect())
}

pub fn apply_morpher(data: &MorpherData, mut v: StackValue, _ctx: &EvalCtx<'_>) -> Result<StackValue, String> {
    let n = v.mesh.vertex_count();
    let selection: Option<std::collections::HashSet<u32>> = if data.use_vertex_selection {
        match &v.selection {
            Some((loom_mesh::select_ops::SubLevel::Vertex, ids)) => Some(ids.iter().copied().collect()),
            _ => None,
        }
    } else {
        None
    };
    let mut delta = vec![Vec3::ZERO; n];
    for (i, ch) in data.channels.iter().enumerate() {
        if !ch.active || ch.targets.is_empty() {
            continue;
        }
        let mut value = ch.value.get();
        if ch.use_limits {
            value = value.clamp(ch.min.min(ch.max), ch.max.max(ch.min));
        }
        if value.abs() < 1e-6 {
            continue;
        }
        if !ch.valid || ch.targets.iter().any(|t| t.offsets.len() != n) {
            v.warnings.push(format!("Morpher: channel {} {}: wrong vertex count", i + 1, ch.name));
            continue;
        }
        let Some(offsets) = channel_offsets(ch, value, n) else { continue };
        for (k, o) in offsets.into_iter().enumerate() {
            if selection.as_ref().is_none_or(|s| s.contains(&(k as u32))) {
                delta[k] += o;
            }
        }
    }
    for (vx, d) in v.mesh.vertices_mut().iter_mut().zip(delta) {
        vx.position += d;
    }
    Ok(v)
}

// ------------------------------------------------------------- Skin Morph

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkinMorphBone {
    pub node: NodeId,
    /// The bone's world matrix at bind.
    pub bind_world: Mat4,
    pub parent: Option<NodeId>,
    pub parent_bind_world: Mat4,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkinMorphTarget {
    /// Index into `SkinMorphData::bones`.
    pub bone: usize,
    pub name: String,
    /// The bone angle (degrees from its bind direction in the parent's frame) at which the morph is full.
    pub angle_deg: f32,
    pub influence_deg: f32,
    /// 0 Linear, 1 Sinual, 2 Fast, 3 Slow.
    pub falloff: u32,
    pub enabled: bool,
    /// Per vertex: the delta in the bone's local frame.
    pub deltas: Vec<(u32, Vec3)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SkinMorphData {
    pub bones: Vec<SkinMorphBone>,
    pub morphs: Vec<SkinMorphTarget>,
}

impl SkinMorphData {
    pub fn bind(scene: &Scene, bones: &[NodeId]) -> Self {
        let mut d = Self::default();
        for b in bones {
            d.add_bone(scene, *b);
        }
        d
    }

    pub fn add_bone(&mut self, scene: &Scene, bone: NodeId) {
        if self.bones.iter().any(|b| b.node == bone) {
            return;
        }
        let parent = scene.node(bone).and_then(|n| n.parent);
        self.bones.push(SkinMorphBone { node: bone, bind_world: scene.world_matrix(bone), parent, parent_bind_world: parent.map(|p| scene.world_matrix(p)).unwrap_or(Mat4::IDENTITY) });
    }

    /// The bone's current angle from its bind direction, in the parent's frame (degrees).
    pub fn bone_angle(&self, bone: usize, world: &dyn Fn(NodeId) -> Option<Mat4>) -> Option<f32> {
        let b = self.bones.get(bone)?;
        let bw = world(b.node)?;
        let pw = b.parent.and_then(|p| world(p)).unwrap_or(Mat4::IDENTITY);
        let bind_dir_parent = b.parent_bind_world.inverse().transform_vector3(b.bind_world.transform_vector3(Vec3::X)).normalize_or_zero();
        let cur_dir_parent = pw.inverse().transform_vector3(bw.transform_vector3(Vec3::X)).normalize_or_zero();
        Some(bind_dir_parent.dot(cur_dir_parent).clamp(-1.0, 1.0).acos().to_degrees())
    }
}

fn falloff_weight(t: f32, falloff: u32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match falloff {
        1 => 0.5 - 0.5 * (std::f32::consts::PI * t).cos(),
        2 => 1.0 - (1.0 - t) * (1.0 - t),
        3 => t * t,
        _ => t,
    }
}

pub fn apply_skin_morph(data: &SkinMorphData, mut v: StackValue, ctx: &EvalCtx<'_>) -> Result<StackValue, String> {
    let n = v.mesh.vertex_count();
    let world = |id: NodeId| ctx.node_world_matrix(id);
    let mut delta = vec![Vec3::ZERO; n];
    for m in data.morphs.iter().filter(|m| m.enabled) {
        let Some(angle) = data.bone_angle(m.bone, &world) else { continue };
        let influence = m.influence_deg.max(1e-3);
        let w = falloff_weight(1.0 - (angle - m.angle_deg).abs() / influence, m.falloff);
        if w <= 0.0 {
            continue;
        }
        let Some(bw) = world(data.bones[m.bone].node) else { continue };
        for (vi, d) in &m.deltas {
            if let Some(slot) = delta.get_mut(*vi as usize) {
                *slot += ctx.object_inverse.transform_vector3(bw.transform_vector3(*d)) * w;
            }
        }
    }
    for (vx, d) in v.mesh.vertices_mut().iter_mut().zip(delta) {
        vx.position += d;
    }
    Ok(v)
}

// -------------------------------------------------------------- Skin Wrap

/// One binding of a wrapped vertex to a base face (Face Deformation) or
/// base vertex (Vertex Deformation): its position in that frame at bind time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WrapBind {
    pub base: u32,
    /// Face id (Face Deformation) or vertex id (Vertex Deformation).
    pub target: u32,
    pub local: Vec3,
    pub weight: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkinWrapData {
    pub bases: Vec<NodeId>,
    /// 0 Face Deformation, 1 Vertex Deformation.
    pub engine: u32,
    pub falloff: f32,
    pub distance_influence: f32,
    pub face_limit: u32,
    pub threshold: f32,
    pub weight_all_points: bool,
    /// Per wrapped vertex.
    pub binds: Vec<Vec<WrapBind>>,
    /// Face / vertex counts of each base at bind time (a changed base warns).
    pub base_counts: Vec<(usize, usize)>,
}

impl Default for SkinWrapData {
    fn default() -> Self {
        Self { bases: Vec::new(), engine: 0, falloff: 1.0, distance_influence: 1.2, face_limit: 3, threshold: 5.0, weight_all_points: false, binds: Vec::new(), base_counts: Vec::new() }
    }
}

/// An orthonormal frame on a base face: origin at the centroid, u along the
/// first edge, n the face normal, w = n × u.
fn face_frame(mesh: &Mesh, face: loom_mesh::FaceId, world: Mat4) -> Option<(Vec3, Vec3, Vec3, Vec3)> {
    let pts: Vec<Vec3> = mesh.face_positions(face)?.into_iter().map(|p| world.transform_point3(p)).collect();
    if pts.len() < 3 {
        return None;
    }
    let origin = pts.iter().sum::<Vec3>() / pts.len() as f32;
    let u = (pts[1] - pts[0]).normalize_or_zero();
    let mut n = Vec3::ZERO;
    for i in 0..pts.len() {
        n += pts[i].cross(pts[(i + 1) % pts.len()]);
    }
    let n = n.normalize_or_zero();
    if u == Vec3::ZERO || n == Vec3::ZERO {
        return None;
    }
    let w = n.cross(u).normalize_or_zero();
    Some((origin, u, w, n))
}

/// A frame on a base vertex: origin at the vertex, n its normal, u along its first edge.
fn vertex_frame(mesh: &Mesh, v: loom_mesh::VertexId, normals: &[Vec3], world: Mat4) -> Option<(Vec3, Vec3, Vec3, Vec3)> {
    let origin = world.transform_point3(mesh.vertex_position(v)?);
    let n = world.transform_vector3(*normals.get(v.0 as usize)?).normalize_or_zero();
    let nb = loom_mesh::select_ops::vertex_neighbours(mesh, v);
    let first = nb.first().and_then(|o| mesh.vertex_position(*o)).map(|p| world.transform_point3(p))?;
    let mut u = first - origin;
    u -= n * u.dot(n);
    let u = u.normalize_or_zero();
    if n == Vec3::ZERO || u == Vec3::ZERO {
        return None;
    }
    Some((origin, u, n.cross(u).normalize_or_zero(), n))
}

fn to_local(frame: &(Vec3, Vec3, Vec3, Vec3), p: Vec3) -> Vec3 {
    let d = p - frame.0;
    Vec3::new(d.dot(frame.1), d.dot(frame.2), d.dot(frame.3))
}

fn from_local(frame: &(Vec3, Vec3, Vec3, Vec3), l: Vec3) -> Vec3 {
    frame.0 + frame.1 * l.x + frame.2 * l.y + frame.3 * l.z
}

impl SkinWrapData {
    /// Binds `wrapped`'s vertices to the evaluated bases as they are now.
    pub fn bind(scene: &Scene, wrapped: NodeId, bases: &[NodeId]) -> Self {
        let mut d = Self { bases: bases.to_vec(), ..Default::default() };
        d.rebind(scene, wrapped, 0);
        d
    }

    /// Reset: rebinds the mesh flowing into stack entry `index` (this Skin Wrap)
    /// with the current settings and base poses.
    pub fn rebind(&mut self, scene: &Scene, wrapped: NodeId, index: usize) {
        let Some(node) = scene.node(wrapped) else { return };
        let world = scene.object_matrix(wrapped);
        let mesh = match index.checked_sub(1) {
            Some(prev) => scene.evaluated(wrapped, Some(prev)).value.mesh.clone(),
            None => node.object.display_output(),
        };
        let points: Vec<Vec3> = mesh.vertices().iter().map(|v| world.transform_point3(v.position)).collect();
        let bases: Vec<(Mesh, Mat4)> = self.bases.iter().map(|b| (scene.eval_mesh(*b).value.mesh.clone(), scene.object_matrix(*b))).collect();
        self.base_counts = bases.iter().map(|(m, _)| (m.face_count(), m.vertex_count())).collect();
        self.binds = points.iter().map(|p| self.bind_point(*p, &bases)).collect();
    }

    fn bind_point(&self, p: Vec3, bases: &[(Mesh, Mat4)]) -> Vec<WrapBind> {
        let mut candidates: Vec<(u32, u32, f32)> = Vec::new();
        for (bi, (mesh, world)) in bases.iter().enumerate() {
            if self.engine == 0 {
                for f in 0..mesh.face_count() {
                    if let Some(pts) = mesh.face_positions(loom_mesh::FaceId(f as u32)) {
                        let c = pts.iter().map(|q| world.transform_point3(*q)).sum::<Vec3>() / pts.len() as f32;
                        candidates.push((bi as u32, f as u32, c.distance(p)));
                    }
                }
            } else {
                for (v, vx) in mesh.vertices().iter().enumerate() {
                    candidates.push((bi as u32, v as u32, world.transform_point3(vx.position).distance(p)));
                }
            }
        }
        candidates.sort_by(|a, b| a.2.total_cmp(&b.2));
        let mut chosen: Vec<&(u32, u32, f32)> = candidates.iter().filter(|c| c.2 <= self.threshold).take(self.face_limit.max(1) as usize).collect();
        if chosen.is_empty() && self.weight_all_points {
            chosen = candidates.iter().take(1).collect();
        }
        let normals: Vec<Vec<Vec3>> = if self.engine == 1 { bases.iter().map(|(m, _)| loom_mesh::mesh_mods::vertex_normals(m)).collect() } else { Vec::new() };
        let mut out: Vec<WrapBind> = chosen
            .into_iter()
            .filter_map(|(bi, t, dist)| {
                let (mesh, world) = &bases[*bi as usize];
                let frame = if self.engine == 0 { face_frame(mesh, loom_mesh::FaceId(*t), *world)? } else { vertex_frame(mesh, loom_mesh::VertexId(*t), &normals[*bi as usize], *world)? };
                let w = 1.0 / (dist.max(1e-3)).powf(self.falloff.max(0.0)) * self.distance_influence.max(1e-3);
                Some(WrapBind { base: *bi, target: *t, local: to_local(&frame, p), weight: w })
            })
            .collect();
        let sum: f32 = out.iter().map(|b| b.weight).sum();
        if sum > 1e-9 {
            for b in &mut out {
                b.weight /= sum;
            }
        }
        out
    }
}

pub fn apply_skin_wrap(data: &SkinWrapData, mut v: StackValue, ctx: &EvalCtx<'_>) -> Result<StackValue, String> {
    let n = v.mesh.vertex_count();
    if data.bases.is_empty() || data.binds.is_empty() {
        return Ok(v);
    }
    if data.binds.len() != n {
        v.warnings.push(format!("Skin Wrap: {} bound vertices for a mesh of {n}; Reset to rebind", data.binds.len()));
        return Ok(v);
    }
    // The bases' evaluated meshes now, in world space.
    let mut bases: Vec<Option<(Mesh, Mat4, Vec<Vec3>)>> = Vec::with_capacity(data.bases.len());
    for (i, b) in data.bases.iter().enumerate() {
        match ctx.node_evaluated(*b) {
            Some((world, mesh)) => {
                let ok = data.base_counts.get(i).is_none_or(|(f, vc)| *f == mesh.face_count() && *vc == mesh.vertex_count());
                if !ok {
                    v.warnings.push(format!("Skin Wrap: base {} topology changed; Reset to rebind", i + 1));
                    bases.push(None);
                } else {
                    let normals = if data.engine == 1 { loom_mesh::mesh_mods::vertex_normals(&mesh) } else { Vec::new() };
                    bases.push(Some((mesh, world, normals)));
                }
            }
            None => bases.push(None),
        }
    }
    let positions: Vec<Vec3> = v.mesh.vertices().iter().map(|vx| vx.position).collect();
    for (i, binds) in data.binds.iter().enumerate() {
        if binds.is_empty() {
            continue;
        }
        let mut acc = Vec3::ZERO;
        let mut total = 0.0;
        for b in binds {
            let Some(Some((mesh, world, normals))) = bases.get(b.base as usize) else { continue };
            let frame = if data.engine == 0 { face_frame(mesh, loom_mesh::FaceId(b.target), *world) } else { vertex_frame(mesh, loom_mesh::VertexId(b.target), normals, *world) };
            let Some(frame) = frame else { continue };
            acc += from_local(&frame, b.local) * b.weight;
            total += b.weight;
        }
        if total > 1e-6 {
            v.mesh.vertices_mut()[i].position = ctx.object_inverse.transform_point3(acc / total);
        } else {
            v.mesh.vertices_mut()[i].position = positions[i];
        }
    }
    Ok(v)
}
