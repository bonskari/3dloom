//! Morpher (channels with progressive targets), Skin Morph (bone-angle
//! driven vertex deltas), Skin Wrap (a mesh that follows deformed control
//! objects) and Skin Wrap Patch (a mesh that follows a patch object).

use crate::modifier::{EvalCtx, StackValue};
use crate::{NodeId, Param, Scene};
use glam::{Mat4, Quat, Vec3};
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
    /// Joint Type: false Ball Joint (all of the bone's rotation), true Planar
    /// Joint (only its rotation in the parent's XY plane).
    #[serde(default)]
    pub planar: bool,
}

/// Falloff 4: the morph's own graph (Custom Falloff).
pub const FALLOFF_CUSTOM: u32 = 4;

/// The graph a new morph's Custom Falloff and the soft selection start
/// with: the Sinual curve.
pub fn default_curve() -> Vec<[f32; 2]> {
    [0.0f32, 0.25, 0.5, 0.75, 1.0].iter().map(|x| [*x, 0.5 - 0.5 * (std::f32::consts::PI * x).cos()]).collect()
}

/// A graph's value at `x`: straight segments between its points (sorted by x),
/// level beyond the ends.
pub fn curve_value(points: &[[f32; 2]], x: f32) -> f32 {
    let Some(first) = points.first() else { return x.clamp(0.0, 1.0) };
    if x <= first[0] {
        return first[1];
    }
    for w in points.windows(2) {
        let (a, b) = (w[0], w[1]);
        if x <= b[0] {
            let t = if (b[0] - a[0]).abs() < 1e-6 { 1.0 } else { (x - a[0]) / (b[0] - a[0]) };
            return a[1] + (b[1] - a[1]) * t;
        }
    }
    points[points.len() - 1][1]
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkinMorphTarget {
    /// Index into `SkinMorphData::bones`.
    pub bone: usize,
    pub name: String,
    /// The bone's angle from its bind direction at the pose (degrees; shown, and
    /// the driver of morphs saved before poses were kept).
    pub angle_deg: f32,
    pub influence_deg: f32,
    /// 0 Linear, 1 Sinual, 2 Fast, 3 Slow, 4 Custom (the graph).
    pub falloff: u32,
    pub enabled: bool,
    /// Per vertex: the delta in the bone's local frame.
    pub deltas: Vec<(u32, Vec3)>,
    /// The bone's rotation in its parent's frame where the morph is full
    /// (Create Morph / Reset Orientation).
    #[serde(default)]
    pub pose: Option<Quat>,
    /// The Custom Falloff graph: x 0 at the edge of the influence, 1 at the pose.
    #[serde(default = "default_curve")]
    pub curve: Vec<[f32; 2]>,
    /// External Mesh: the target Reload Target reads.
    #[serde(default)]
    pub external: Option<NodeId>,
    /// Reload Only Selected Verts.
    #[serde(default)]
    pub reload_selected: bool,
}

impl Default for SkinMorphTarget {
    fn default() -> Self {
        Self { bone: 0, name: String::new(), angle_deg: 0.0, influence_deg: 90.0, falloff: 0, enabled: true, deltas: Vec::new(), pose: None, curve: default_curve(), external: None, reload_selected: false }
    }
}

/// Selection rollout: soft selection by a radius, optionally limited to a
/// number of edges, shaped by a graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SkinMorphSoft {
    pub on: bool,
    pub radius: f32,
    pub edge_limit_on: bool,
    pub edge_limit: u32,
    pub curve: Vec<[f32; 2]>,
}

impl Default for SkinMorphSoft {
    fn default() -> Self {
        Self { on: false, radius: 10.0, edge_limit_on: false, edge_limit: 2, curve: default_curve() }
    }
}

/// Copy and Paste rollout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MorphMirror {
    /// Mirror Plane: perpendicular to 0 X, 1 Y, 2 Z (world).
    pub axis: u32,
    pub offset: f32,
    pub threshold: f32,
    pub show_plane: bool,
    pub preview_bone: bool,
    pub preview_vertices: bool,
}

impl Default for MorphMirror {
    fn default() -> Self {
        Self { axis: 0, offset: 0.0, threshold: 1.0, show_plane: false, preview_bone: false, preview_vertices: false }
    }
}

impl MorphMirror {
    pub fn reflect_point(&self, p: Vec3) -> Vec3 {
        let mut q = p;
        let a = self.axis.min(2) as usize;
        q[a] = 2.0 * self.offset - p[a];
        q
    }
    pub fn reflect_vector(&self, v: Vec3) -> Vec3 {
        let mut q = v;
        q[self.axis.min(2) as usize] = -v[self.axis.min(2) as usize];
        q
    }
}

/// Options rollout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SkinMorphOptions {
    pub beginner_mode: bool,
    pub show_driver_matrix: bool,
    pub show_morph_matrix: bool,
    pub show_current_angle: bool,
    pub show_edges: bool,
    pub matrix_size: f32,
    pub bone_size: f32,
}

impl Default for SkinMorphOptions {
    fn default() -> Self {
        Self { beginner_mode: true, show_driver_matrix: false, show_morph_matrix: false, show_current_angle: false, show_edges: true, matrix_size: 5.0, bone_size: 2.0 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SkinMorphData {
    pub bones: Vec<SkinMorphBone>,
    pub morphs: Vec<SkinMorphTarget>,
    /// Points sub-object selection (vertex ids of the mesh below).
    #[serde(default)]
    pub selection: Vec<u32>,
    /// Edit: the morph being shaped (held at 100%).
    #[serde(default)]
    pub editing: Option<usize>,
    /// The bone and its transform from before Edit posed it.
    #[serde(default)]
    pub edit_restore: Option<(NodeId, crate::Transform)>,
    #[serde(default)]
    pub soft: SkinMorphSoft,
    #[serde(default)]
    pub mirror: MorphMirror,
    #[serde(default)]
    pub options: SkinMorphOptions,
}

fn rotation_of(m: Mat4) -> Quat {
    m.to_scale_rotation_translation().1.normalize()
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
        self.bones.push(SkinMorphBone { node: bone, bind_world: scene.world_matrix(bone), parent, parent_bind_world: parent.map(|p| scene.world_matrix(p)).unwrap_or(Mat4::IDENTITY), planar: false });
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

    /// The bone's rotation in its parent's frame now.
    pub fn local_rotation(&self, bone: usize, world: &dyn Fn(NodeId) -> Option<Mat4>) -> Option<Quat> {
        let b = self.bones.get(bone)?;
        let bw = world(b.node)?;
        let pw = b.parent.and_then(world).unwrap_or(Mat4::IDENTITY);
        Some((rotation_of(pw).inverse() * rotation_of(bw)).normalize())
    }

    /// The bone's rotation in its parent's frame at bind.
    pub fn bind_rotation(&self, bone: usize) -> Quat {
        self.bones.get(bone).map(|b| (rotation_of(b.parent_bind_world).inverse() * rotation_of(b.bind_world)).normalize()).unwrap_or(Quat::IDENTITY)
    }

    /// Create Morph: a morph on bone `bone` (list index) with the bone's
    /// current orientation as its pose; returns its index.
    pub fn create_morph(&mut self, scene: &Scene, bone: usize) -> usize {
        let world = |id: NodeId| Some(scene.world_matrix(id));
        let angle = self.bone_angle(bone, &world).unwrap_or(0.0);
        let pose = self.local_rotation(bone, &world);
        let name = format!("Morph {}", self.morphs.len() + 1);
        self.morphs.push(SkinMorphTarget { bone, name, angle_deg: angle, pose, ..Default::default() });
        self.morphs.len() - 1
    }

    /// The angle (degrees) between the bone now and the morph's pose, as its
    /// Joint Type tracks it: all rotation (Ball) or the rotation in the
    /// parent's XY plane (Planar).
    pub fn pose_angle(&self, m: &SkinMorphTarget, world: &dyn Fn(NodeId) -> Option<Mat4>) -> Option<f32> {
        let Some(pose) = m.pose else {
            return self.bone_angle(m.bone, world).map(|a| (a - m.angle_deg).abs());
        };
        let cur = self.local_rotation(m.bone, world)?;
        if self.bones.get(m.bone)?.planar {
            let (a, b) = (cur * Vec3::X, pose * Vec3::X);
            let (a, b) = (glam::Vec2::new(a.x, a.y), glam::Vec2::new(b.x, b.y));
            if a.length_squared() < 1e-8 || b.length_squared() < 1e-8 {
                return Some(0.0);
            }
            return Some(a.angle_to(b).abs().to_degrees());
        }
        Some(cur.angle_between(pose).to_degrees())
    }

    /// The morph's influence now (0..1): full at its pose, gone half the
    /// Influence Angle away, shaped by its Falloff; 1 while it is edited.
    pub fn morph_weight(&self, i: usize, world: &dyn Fn(NodeId) -> Option<Mat4>) -> f32 {
        let Some(m) = self.morphs.get(i) else { return 0.0 };
        if self.editing == Some(i) {
            return 1.0;
        }
        if !m.enabled {
            return 0.0;
        }
        let Some(angle) = self.pose_angle(m, world) else { return 0.0 };
        let half = (m.influence_deg * 0.5).max(1e-3);
        let t = 1.0 - angle / half;
        if t <= 0.0 {
            return 0.0;
        }
        falloff_weight(t, m.falloff, &m.curve)
    }

    /// Soft selection weight per vertex of `mesh` (placed by `world`): 1 on
    /// the selection; with Use Soft Selection, the graph over the distance
    /// within Radius (and within Edge Limit edges when that is on).
    pub fn soft_weights(&self, mesh: &Mesh, world: Mat4) -> Vec<f32> {
        let n = mesh.vertex_count();
        let mut w = vec![0.0; n];
        let selected: Vec<usize> = self.selection.iter().map(|v| *v as usize).filter(|v| *v < n).collect();
        for v in &selected {
            w[*v] = 1.0;
        }
        if !self.soft.on || selected.is_empty() || self.soft.radius <= 0.0 {
            return w;
        }
        let reach: Option<Vec<bool>> = self.soft.edge_limit_on.then(|| {
            let mut seen = vec![false; n];
            let mut front: Vec<usize> = selected.clone();
            for v in &front {
                seen[*v] = true;
            }
            for _ in 0..self.soft.edge_limit {
                let mut next = Vec::new();
                for v in front {
                    for nb in loom_mesh::select_ops::vertex_neighbours(mesh, loom_mesh::VertexId(v as u32)) {
                        let i = nb.0 as usize;
                        if i < n && !seen[i] {
                            seen[i] = true;
                            next.push(i);
                        }
                    }
                }
                front = next;
            }
            seen
        });
        let pos: Vec<Vec3> = mesh.vertices().iter().map(|v| world.transform_point3(v.position)).collect();
        for i in 0..n {
            if w[i] == 1.0 || reach.as_ref().is_some_and(|r| !r[i]) {
                continue;
            }
            let d = selected.iter().map(|s| pos[*s].distance(pos[i])).fold(f32::INFINITY, f32::min);
            if d < self.soft.radius {
                w[i] = curve_value(&self.soft.curve, 1.0 - d / self.soft.radius).clamp(0.0, 1.0);
            }
        }
        w
    }
}

fn falloff_weight(t: f32, falloff: u32, curve: &[[f32; 2]]) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match falloff {
        1 => 0.5 - 0.5 * (std::f32::consts::PI * t).cos(),
        2 => 1.0 - (1.0 - t) * (1.0 - t),
        3 => t * t,
        FALLOFF_CUSTOM => curve_value(curve, t).max(0.0),
        _ => t,
    }
}

pub fn apply_skin_morph(data: &SkinMorphData, mut v: StackValue, ctx: &EvalCtx<'_>) -> Result<StackValue, String> {
    let n = v.mesh.vertex_count();
    let world = |id: NodeId| ctx.node_world_matrix(id);
    let mut delta = vec![Vec3::ZERO; n];
    for (i, m) in data.morphs.iter().enumerate() {
        let w = data.morph_weight(i, &world);
        if w <= 0.0 {
            continue;
        }
        let Some(bw) = data.bones.get(m.bone).and_then(|b| world(b.node)) else { continue };
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

/// One binding of a wrapped point to a control face (Face Deformation) or
/// control vertex (Vertex Deformation): its position in that frame at bind time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WrapBind {
    pub base: u32,
    /// Face id (Face Deformation) or vertex id (Vertex Deformation).
    pub target: u32,
    pub local: Vec3,
    pub weight: f32,
}

/// Local parameters of one control vertex.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CvLocal {
    /// Local Strength: how far (and which way) the vertex pulls its points.
    pub strength: f32,
    /// Local Scale Mult: the volume of influence, uniformly.
    pub scale: f32,
    /// Local X / Y / Z: the volume along the vertex's axes.
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Default for CvLocal {
    fn default() -> Self {
        Self { strength: 1.0, scale: 1.0, x: 1.0, y: 1.0, z: 1.0 }
    }
}

/// Advanced Parameters: the mirror tools.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WrapMirror {
    pub show: bool,
    /// Mirror Plane: perpendicular to 0 X, 1 Y, 2 Z (world).
    pub axis: u32,
    pub offset: f32,
    pub threshold: f32,
}

impl Default for WrapMirror {
    fn default() -> Self {
        Self { show: false, axis: 0, offset: 0.0, threshold: 1.0 }
    }
}

impl WrapMirror {
    pub fn reflect_point(&self, p: Vec3) -> Vec3 {
        let mut q = p;
        let a = self.axis.min(2) as usize;
        q[a] = 2.0 * self.offset - p[a];
        q
    }
}

/// Display Parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WrapDisplay {
    pub loops: bool,
    pub axis: bool,
    pub face_limit: bool,
    pub unassigned: bool,
    pub control_verts: bool,
}

impl Default for WrapDisplay {
    fn default() -> Self {
        Self { loops: true, axis: true, face_limit: true, unassigned: false, control_verts: true }
    }
}

/// Control-vertex settings Bake Control Verts stores on a control object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkinWrapBake {
    pub locals: Vec<CvLocal>,
    pub falloff: f32,
    pub distance_influence: f32,
    pub face_limit: u32,
}

fn five() -> f32 {
    5.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkinWrapData {
    /// The control objects.
    pub bases: Vec<NodeId>,
    /// 0 Face Deformation, 1 Vertex Deformation.
    pub engine: u32,
    pub falloff: f32,
    pub distance_influence: f32,
    pub face_limit: u32,
    pub threshold: f32,
    pub weight_all_points: bool,
    #[serde(default)]
    pub blend_to_base: bool,
    #[serde(default = "five")]
    pub blend_distance: f32,
    /// Local parameters per control object, per control vertex.
    #[serde(default)]
    pub locals: Vec<Vec<CvLocal>>,
    /// Control Vertices sub-object selection: (control object, vertex).
    #[serde(default)]
    pub selection: Vec<(u32, u32)>,
    #[serde(default)]
    pub mirror: WrapMirror,
    #[serde(default)]
    pub display: WrapDisplay,
    /// Per wrapped vertex.
    pub binds: Vec<Vec<WrapBind>>,
    /// Face / vertex counts of each base at bind time (a changed base warns).
    pub base_counts: Vec<(usize, usize)>,
    /// Each point's world position at bind.
    #[serde(default)]
    pub rest: Vec<Vec3>,
    /// Each point's closest control face at bind (control object, face), if
    /// one was found within Threshold.
    #[serde(default)]
    pub closest: Vec<Option<(u32, u32)>>,
    /// Blend To Base Mesh: each point's share of the deformation.
    #[serde(default)]
    pub blend: Vec<f32>,
    /// Each control vertex's frame at bind (origin, x, y, z axes) and radius.
    #[serde(default)]
    pub cv_frames: Vec<Vec<Option<(Frame, f32)>>>,
}

impl Default for SkinWrapData {
    fn default() -> Self {
        Self {
            bases: Vec::new(),
            engine: 1,
            falloff: 1.0,
            distance_influence: 1.2,
            face_limit: 3,
            threshold: 5.0,
            weight_all_points: false,
            blend_to_base: false,
            blend_distance: 5.0,
            locals: Vec::new(),
            selection: Vec::new(),
            mirror: WrapMirror::default(),
            display: WrapDisplay::default(),
            binds: Vec::new(),
            base_counts: Vec::new(),
            rest: Vec::new(),
            closest: Vec::new(),
            blend: Vec::new(),
            cv_frames: Vec::new(),
        }
    }
}

/// An orthonormal frame: origin, then the x, y, z axes.
pub type Frame = (Vec3, Vec3, Vec3, Vec3);

/// An orthonormal frame on a base face: origin at the centroid, u along the
/// first edge, n the face normal, w = n × u.
fn face_frame(mesh: &Mesh, face: loom_mesh::FaceId, world: Mat4) -> Option<Frame> {
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
fn vertex_frame(mesh: &Mesh, v: loom_mesh::VertexId, normals: &[Vec3], world: Mat4) -> Option<Frame> {
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

fn to_local(frame: &Frame, p: Vec3) -> Vec3 {
    let d = p - frame.0;
    Vec3::new(d.dot(frame.1), d.dot(frame.2), d.dot(frame.3))
}

fn from_local(frame: &Frame, l: Vec3) -> Vec3 {
    frame.0 + frame.1 * l.x + frame.2 * l.y + frame.3 * l.z
}

/// The point of triangle abc closest to p.
fn closest_on_triangle(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

/// One control object in world space with the adjacency the bind needs.
pub struct ControlGeo {
    pub mesh: Mesh,
    pub world: Mat4,
    pub pos: Vec<Vec3>,
    pub frames: Vec<Option<Frame>>,
    /// The mean length of each vertex's edges.
    pub vertex_edge: Vec<f32>,
    /// The mean edge length of the whole object.
    pub mean_edge: f32,
    pub face_verts: Vec<Vec<u32>>,
    pub vert_faces: Vec<Vec<u32>>,
}

impl ControlGeo {
    pub fn new(mesh: Mesh, world: Mat4) -> Self {
        let normals = loom_mesh::mesh_mods::vertex_normals(&mesh);
        let nv = mesh.vertex_count();
        let pos: Vec<Vec3> = mesh.vertices().iter().map(|v| world.transform_point3(v.position)).collect();
        let frames = (0..nv).map(|v| vertex_frame(&mesh, loom_mesh::VertexId(v as u32), &normals, world)).collect();
        let mut vertex_edge = vec![0.0; nv];
        let (mut total, mut count) = (0.0, 0usize);
        for (v, slot) in vertex_edge.iter_mut().enumerate() {
            let nb = loom_mesh::select_ops::vertex_neighbours(&mesh, loom_mesh::VertexId(v as u32));
            let lens: Vec<f32> = nb.iter().map(|o| pos[o.0 as usize].distance(pos[v])).collect();
            if !lens.is_empty() {
                *slot = lens.iter().sum::<f32>() / lens.len() as f32;
                total += lens.iter().sum::<f32>();
                count += lens.len();
            }
        }
        let face_verts: Vec<Vec<u32>> = (0..mesh.face_count()).map(|f| mesh.face_vertex_ids(loom_mesh::FaceId(f as u32)).unwrap_or_default().into_iter().map(|v| v.0).collect()).collect();
        let mut vert_faces = vec![Vec::new(); nv];
        for (f, vs) in face_verts.iter().enumerate() {
            for v in vs {
                if let Some(slot) = vert_faces.get_mut(*v as usize) {
                    slot.push(f as u32);
                }
            }
        }
        Self { mesh, world, pos, frames, vertex_edge, mean_edge: if count > 0 { total / count as f32 } else { 1.0 }, face_verts, vert_faces }
    }

    /// Distance from p to face f.
    pub fn face_distance(&self, f: usize, p: Vec3) -> f32 {
        let vs = &self.face_verts[f];
        if vs.len() < 3 {
            return f32::INFINITY;
        }
        let a = self.pos[vs[0] as usize];
        (1..vs.len() - 1).map(|i| closest_on_triangle(p, a, self.pos[vs[i] as usize], self.pos[vs[i + 1] as usize]).distance(p)).fold(f32::INFINITY, f32::min)
    }

    /// The faces within `hops` vertex-sharing steps of `seed`.
    pub fn face_ring(&self, seed: &[u32], hops: u32) -> std::collections::BTreeSet<u32> {
        let mut set: std::collections::BTreeSet<u32> = seed.iter().copied().collect();
        let mut front: Vec<u32> = seed.to_vec();
        for _ in 0..hops {
            let mut next = Vec::new();
            for f in front {
                for v in &self.face_verts[f as usize] {
                    for g in &self.vert_faces[*v as usize] {
                        if set.insert(*g) {
                            next.push(*g);
                        }
                    }
                }
            }
            front = next;
        }
        set
    }
}

impl SkinWrapData {
    /// Binds `wrapped`'s vertices to the evaluated bases as they are now.
    pub fn bind(scene: &Scene, wrapped: NodeId, bases: &[NodeId]) -> Self {
        let mut d = Self { bases: bases.to_vec(), ..Default::default() };
        d.rebind(scene, wrapped, 0);
        d
    }

    /// The control objects in world space as they evaluate now.
    pub fn control_geometry(&self, scene: &Scene) -> Vec<ControlGeo> {
        self.bases.iter().map(|b| ControlGeo::new(scene.eval_mesh(*b).value.mesh.clone(), scene.object_matrix(*b))).collect()
    }

    /// The local parameters of a control vertex (defaults when unset).
    pub fn local(&self, base: u32, v: u32) -> CvLocal {
        self.locals.get(base as usize).and_then(|l| l.get(v as usize)).copied().unwrap_or_default()
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
        let geos = self.control_geometry(scene);
        self.base_counts = geos.iter().map(|g| (g.mesh.face_count(), g.mesh.vertex_count())).collect();
        self.locals.resize(geos.len(), Vec::new());
        for (l, g) in self.locals.iter_mut().zip(&geos) {
            if l.len() != g.mesh.vertex_count() {
                *l = vec![CvLocal::default(); g.mesh.vertex_count()];
            }
        }
        self.selection.retain(|(b, v)| geos.get(*b as usize).is_some_and(|g| (*v as usize) < g.mesh.vertex_count()));
        self.cv_frames = geos
            .iter()
            .enumerate()
            .map(|(b, g)| (0..g.mesh.vertex_count()).map(|v| g.frames[v].map(|f| (f, g.vertex_edge[v] * self.distance_influence * self.local(b as u32, v as u32).scale))).collect())
            .collect();
        let mut binds = Vec::with_capacity(points.len());
        let mut closest = Vec::with_capacity(points.len());
        let mut blend = Vec::with_capacity(points.len());
        for p in &points {
            let (b, c, f) = self.bind_point(*p, &geos);
            binds.push(b);
            closest.push(c);
            blend.push(f);
        }
        self.binds = binds;
        self.closest = closest;
        self.blend = blend;
        self.rest = points;
    }

    /// The normalised distance of `p` inside control vertex (b, v)'s volume
    /// of influence (1 at its edge), from its frame and radius at bind.
    pub fn volume_distance(&self, b: u32, v: u32, p: Vec3, frame: &Frame) -> Option<f32> {
        let (_, r) = self.cv_frames.get(b as usize)?.get(v as usize)?.as_ref()?;
        let loc = self.local(b, v);
        let l = to_local(frame, p);
        let s = |x: f32| (r * x).max(1e-6);
        Some(((l.x / s(loc.x)).powi(2) + (l.y / s(loc.y)).powi(2) + (l.z / s(loc.z)).powi(2)).sqrt())
    }

    fn bind_point(&self, p: Vec3, geos: &[ControlGeo]) -> (Vec<WrapBind>, Option<(u32, u32)>, f32) {
        let mut best: Option<(u32, u32, f32)> = None;
        for (bi, g) in geos.iter().enumerate() {
            for f in 0..g.face_verts.len() {
                let d = g.face_distance(f, p);
                if best.is_none_or(|b| d < b.2) {
                    best = Some((bi as u32, f as u32, d));
                }
            }
        }
        let Some((cb, cf, cd)) = best.filter(|b| b.2 <= self.threshold || self.weight_all_points) else { return (Vec::new(), None, 0.0) };
        let g = &geos[cb as usize];
        let blend = if self.blend_to_base { (1.0 - cd / (self.blend_distance.max(1e-3) * g.mean_edge.max(1e-6))).clamp(0.0, 1.0) } else { 1.0 };
        let mut out: Vec<WrapBind> = Vec::new();
        if self.engine == 0 {
            let candidates: Vec<(u32, f32)> = if self.falloff <= 0.001 + 1e-6 {
                vec![(cf, cd)]
            } else {
                g.face_ring(&[cf], 1).into_iter().map(|f| (f, g.face_distance(f as usize, p))).collect()
            };
            for (f, d) in candidates {
                let Some(frame) = face_frame(&g.mesh, loom_mesh::FaceId(f), g.world) else { continue };
                let w = if f == cf { 1.0 } else { (-(d - cd).max(0.0) / (self.falloff * g.mean_edge).max(1e-6)).exp() };
                out.push(WrapBind { base: cb, target: f, local: to_local(&frame, p), weight: w });
            }
        } else {
            let allowed: std::collections::BTreeSet<u32> = g.face_ring(&[cf], self.face_limit).into_iter().flat_map(|f| g.face_verts[f as usize].clone()).collect();
            let mut nearest: Option<(u32, f32, Frame)> = None;
            for v in allowed {
                let Some(frame) = g.frames.get(v as usize).copied().flatten() else { continue };
                let Some(d) = self.volume_distance(cb, v, p, &frame) else { continue };
                if d < 1.0 {
                    out.push(WrapBind { base: cb, target: v, local: to_local(&frame, p), weight: (1.0 - d).powf(self.falloff.max(0.001)) });
                }
                if nearest.as_ref().is_none_or(|n| d < n.1) {
                    nearest = Some((v, d, frame));
                }
            }
            if out.is_empty() && self.weight_all_points {
                if let Some((v, _, frame)) = nearest {
                    out.push(WrapBind { base: cb, target: v, local: to_local(&frame, p), weight: 1.0 });
                }
            }
        }
        let sum: f32 = out.iter().map(|b| b.weight).sum();
        if sum > 1e-12 {
            for b in &mut out {
                b.weight /= sum;
            }
        } else {
            out.clear();
        }
        (out, Some((cb, cf)), blend)
    }

    /// The points control vertex (b, v) weights, with their weights.
    pub fn influence_of(&self, b: u32, v: u32) -> Vec<(usize, f32)> {
        if self.engine == 0 {
            return Vec::new();
        }
        self.binds.iter().enumerate().filter_map(|(i, bs)| bs.iter().find(|x| x.base == b && x.target == v).map(|x| (i, x.weight))).collect()
    }

    /// The points inside control vertex (b, v)'s volume at bind that it does
    /// not weight (beyond the Face Limit): drawn red.
    pub fn in_range_unweighted(&self, b: u32, v: u32) -> Vec<usize> {
        let Some(Some((frame, _))) = self.cv_frames.get(b as usize).and_then(|f| f.get(v as usize)) else { return Vec::new() };
        self.rest
            .iter()
            .enumerate()
            .filter(|(i, p)| self.volume_distance(b, v, **p, frame).is_some_and(|d| d < 1.0) && !self.binds.get(*i).is_some_and(|bs| bs.iter().any(|x| x.base == b && x.target == v)))
            .map(|(i, _)| i)
            .collect()
    }

    /// Display Face Limit: the points whose closest face lies within Face
    /// Limit faces of control vertex (b, v), so the vertex can affect them.
    pub fn face_limit_points(&self, geo: &ControlGeo, b: u32, v: u32) -> Vec<usize> {
        let seed = geo.vert_faces.get(v as usize).cloned().unwrap_or_default();
        let ring = geo.face_ring(&seed, self.face_limit);
        self.closest.iter().enumerate().filter(|(_, c)| c.is_some_and(|(cb, cf)| cb == b && ring.contains(&cf))).map(|(i, _)| i).collect()
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
    let relative = data.rest.len() == n;
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
            let predicted = from_local(&frame, b.local);
            if relative {
                let strength = if data.engine == 0 {
                    let vs = mesh.face_vertex_ids(loom_mesh::FaceId(b.target)).unwrap_or_default();
                    if vs.is_empty() { 1.0 } else { vs.iter().map(|x| data.local(b.base, x.0).strength).sum::<f32>() / vs.len() as f32 }
                } else {
                    data.local(b.base, b.target).strength
                };
                acc += (predicted - data.rest[i]) * strength * b.weight;
            } else {
                acc += predicted * b.weight;
            }
            total += b.weight;
        }
        if total > 1e-6 {
            let p = if relative {
                let blend = data.blend.get(i).copied().unwrap_or(1.0);
                ctx.object_world.transform_point3(positions[i]) + acc / total * blend
            } else {
                acc / total
            };
            v.mesh.vertices_mut()[i].position = ctx.object_inverse.transform_point3(p);
        }
    }
    Ok(v)
}

// -------------------------------------------------------- Skin Wrap Patch

/// A wrapped point's binding to one sample of the patch surface (patch,
/// u, v), in the sample's frame at bind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatchBind {
    pub patch: u32,
    pub u: f32,
    pub v: f32,
    pub local: Vec3,
    pub weight: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SkinWrapPatchData {
    /// Pick Patch: the deforming patch object.
    pub patch: Option<NodeId>,
    /// Samples per patch edge.
    pub sample_rate: u32,
    pub binds: Vec<Vec<PatchBind>>,
    /// Each point's world position at bind.
    pub rest: Vec<Vec3>,
    /// The patch count at bind (a change asks for Resample).
    pub patch_count: usize,
}

impl Default for SkinWrapPatchData {
    fn default() -> Self {
        Self { patch: None, sample_rate: 5, binds: Vec::new(), rest: Vec::new(), patch_count: 0 }
    }
}

/// Samples bound per point.
const PATCH_SAMPLES_PER_POINT: usize = 4;

/// The sample parameters of every patch at `rate` steps per edge.
fn patch_samples(pm: &loom_mesh::patch::PatchMesh, rate: u32) -> Vec<(u32, f32, f32)> {
    let n = rate.max(1);
    let mut out = Vec::new();
    for (p, pa) in pm.patches.iter().enumerate() {
        for i in 0..=n {
            for j in 0..=n {
                if pa.v.len() == 3 && i + j > n {
                    continue;
                }
                out.push((p as u32, i as f32 / n as f32, j as f32 / n as f32));
            }
        }
    }
    out
}

/// The frame of the patch surface at (u, v): the point, the surface
/// directions along u and v (one-sided differences inside the domain), the normal.
fn patch_frame(pm: &loom_mesh::patch::PatchMesh, world: Mat4, p: u32, u: f32, v: f32) -> Option<Frame> {
    let tri = pm.patches.get(p as usize)?.v.len() == 3;
    let at = |a: f32, b: f32| pm.surface_point(p as usize, a, b).map(|q| world.transform_point3(q));
    let o = at(u, v)?;
    let h = 1e-3;
    let inside = |a: f32, b: f32| a <= 1.0 && b <= 1.0 && (!tri || a + b <= 1.0);
    let du = if inside(u + h, v) { at(u + h, v)? - o } else { o - at(u - h, v)? };
    let dv = if inside(u, v + h) { at(u, v + h)? - o } else { o - at(u, v - h)? };
    let x = du.normalize_or_zero();
    let n = du.cross(dv).normalize_or_zero();
    if x == Vec3::ZERO || n == Vec3::ZERO {
        return None;
    }
    Some((o, x, n.cross(x), n))
}

impl SkinWrapPatchData {
    /// Resample: binds the mesh flowing into stack entry `index` to the
    /// picked patch's surface as it is now.
    pub fn resample(&mut self, scene: &Scene, wrapped: NodeId, index: usize) {
        self.binds.clear();
        self.rest.clear();
        self.patch_count = 0;
        let Some(node) = scene.node(wrapped) else { return };
        let Some(patch) = self.patch else { return };
        let Some(pm) = scene.evaluated(patch, None).value.patch.clone() else { return };
        let pworld = scene.object_matrix(patch);
        let world = scene.object_matrix(wrapped);
        let mesh = match index.checked_sub(1) {
            Some(prev) => scene.evaluated(wrapped, Some(prev)).value.mesh.clone(),
            None => node.object.display_output(),
        };
        let samples: Vec<((u32, f32, f32), Frame)> = patch_samples(&pm, self.sample_rate).into_iter().filter_map(|s| patch_frame(&pm, pworld, s.0, s.1, s.2).map(|f| (s, f))).collect();
        self.patch_count = pm.patches.len();
        for v in mesh.vertices() {
            let p = world.transform_point3(v.position);
            let mut near: Vec<(f32, usize)> = samples.iter().enumerate().map(|(i, (_, f))| (f.0.distance_squared(p), i)).collect();
            near.sort_by(|a, b| a.0.total_cmp(&b.0));
            near.truncate(PATCH_SAMPLES_PER_POINT);
            let mut binds: Vec<PatchBind> = near.iter().map(|(d2, i)| {
                let ((pi, u, w), frame) = &samples[*i];
                PatchBind { patch: *pi, u: *u, v: *w, local: to_local(frame, p), weight: 1.0 / (d2 + 1e-6) }
            }).collect();
            let sum: f32 = binds.iter().map(|b| b.weight).sum();
            for b in &mut binds {
                b.weight /= sum.max(1e-12);
            }
            self.binds.push(binds);
            self.rest.push(p);
        }
    }
}

pub fn apply_skin_wrap_patch(data: &SkinWrapPatchData, mut v: StackValue, ctx: &EvalCtx<'_>) -> Result<StackValue, String> {
    let n = v.mesh.vertex_count();
    let Some(patch) = data.patch else { return Ok(v) };
    if data.binds.is_empty() {
        return Ok(v);
    }
    if data.binds.len() != n || data.rest.len() != n {
        v.warnings.push(format!("Skin Wrap Patch: {} sampled vertices for a mesh of {n}; Resample", data.binds.len()));
        return Ok(v);
    }
    let Some((pworld, pm)) = ctx.node_patch(patch) else { return Ok(v) };
    if pm.patches.len() != data.patch_count {
        v.warnings.push("Skin Wrap Patch: the patch changed its topology; Resample".to_string());
        return Ok(v);
    }
    for i in 0..n {
        let (mut acc, mut total) = (Vec3::ZERO, 0.0);
        for b in &data.binds[i] {
            let Some(frame) = patch_frame(&pm, pworld, b.patch, b.u, b.v) else { continue };
            acc += (from_local(&frame, b.local) - data.rest[i]) * b.weight;
            total += b.weight;
        }
        if total > 1e-6 {
            let p = ctx.object_world.transform_point3(v.mesh.vertices()[i].position) + acc / total;
            v.mesh.vertices_mut()[i].position = ctx.object_inverse.transform_point3(p);
        }
    }
    Ok(v)
}
