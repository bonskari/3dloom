//! Phase 9 deformers: Affect Region, PathDeform (and its WSM), Vertex Weld,
//! Substitute, Flex and Spline IK Control.

use crate::modifier::{EvalCtx, StackValue};
use crate::node::{NodeId, Transform};
use crate::param::Param;
use glam::{Mat4, Quat, Vec3};
use loom_mesh::VertexId;
use serde::{Deserialize, Serialize};

/// Affect Region: vertices near the start point are pulled toward the end
/// point (the gizmo position, moved at the Points level), by a soft-selection
/// curve of their distance from the start.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AffectRegionData {
    /// Points: the gizmo's position is the end point; the start is the object's pivot.
    pub gizmo: Transform,
    pub falloff: Param<f32>,
    pub pinch: Param<f32>,
    pub bubble: Param<f32>,
    pub ignore_back_facing: Param<bool>,
}

impl Default for AffectRegionData {
    fn default() -> Self {
        let mut gizmo = Transform::default();
        gizmo.position = Param::new(Vec3::new(0.0, 0.0, 20.0));
        Self { gizmo, falloff: Param::new(20.0), pinch: Param::new(0.0), bubble: Param::new(0.0), ignore_back_facing: Param::new(false) }
    }
}

pub fn affect_region(d: &AffectRegionData, mut v: StackValue, selected: &[u32]) -> Result<StackValue, String> {
    let end = d.gizmo.position.get();
    let start = Vec3::ZERO;
    let pull = end - start;
    let (falloff, pinch, bubble) = (d.falloff.get(), d.pinch.get(), d.bubble.get());
    let ignore = d.ignore_back_facing.get();
    let normals: Vec<Vec3> = if ignore { (0..v.mesh.vertex_count() as u32).map(|i| vertex_normal(&v.mesh, VertexId(i))).collect() } else { Vec::new() };
    let mut moves = Vec::new();
    for &i in selected {
        let Some(p) = v.mesh.vertex_position(VertexId(i)) else { continue };
        if ignore && normals[i as usize].dot(pull) < 0.0 {
            continue;
        }
        let w = if p.distance(start) < 1e-6 { 1.0 } else { loom_mesh::soft_select::curve(p.distance(start), falloff, pinch, bubble) };
        if w > 0.0 {
            moves.push((VertexId(i), pull * w));
        }
    }
    // One pass over the positions (topology is untouched).
    let verts = v.mesh.vertices_mut();
    for (vid, d) in moves {
        verts[vid.0 as usize].position += d;
    }
    Ok(v)
}

fn vertex_normal(mesh: &loom_mesh::Mesh, v: VertexId) -> Vec3 {
    let mut n = Vec3::ZERO;
    for f in mesh.faces_around_vertex(v) {
        n += mesh.face_normal(f);
    }
    n.normalize_or_zero()
}

/// PathDeform / PathDeform (WSM): the object is bent along a picked path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PathDeformData {
    pub path: Option<NodeId>,
    pub percent: Param<f32>,
    pub stretch: Param<f32>,
    pub rotation: Param<f32>,
    pub twist: Param<f32>,
    /// Path Deform Axis: 0 X, 1 Y, 2 Z.
    pub axis: Param<u32>,
    pub flip: Param<bool>,
    /// The WSM: the path is used where it is in the world.
    pub world: bool,
}

impl Default for PathDeformData {
    fn default() -> Self {
        Self { path: None, percent: Param::new(0.0), stretch: Param::new(1.0), rotation: Param::new(0.0), twist: Param::new(0.0), axis: Param::new(0), flip: Param::new(false), world: false }
    }
}

/// Arc-length table of a polyline.
struct PathTable {
    pts: Vec<Vec3>,
    /// Unit tangent at each point (the mean of its segments), so the frame
    /// turns smoothly instead of jumping at polyline corners.
    tans: Vec<Vec3>,
    acc: Vec<f32>,
    closed: bool,
}

impl PathTable {
    fn new(shape: &loom_shape::Shape) -> Option<Self> {
        let sp = shape.splines.first()?;
        // The curve itself, finely sampled (not the viewport's interpolation steps).
        let mut pts = Vec::new();
        for i in 0..sp.segment_count() {
            let b = sp.segment(i)?;
            let n = if loom_shape::interp::is_straight(&b) { 1 } else { 64 };
            for k in 0..n {
                pts.push(loom_shape::interp::point_at(&b, k as f32 / n as f32));
            }
        }
        if !sp.closed {
            pts.push(sp.knots.last()?.pos);
        }
        if sp.closed {
            if let Some(f) = pts.first().copied() {
                pts.push(f);
            }
        }
        if pts.len() < 2 {
            return None;
        }
        let mut acc = vec![0.0];
        for w in pts.windows(2) {
            acc.push(acc.last().unwrap() + w[0].distance(w[1]));
        }
        let n = pts.len();
        let seg = |i: usize| (pts[i + 1] - pts[i]).normalize_or_zero();
        let tans: Vec<Vec3> = (0..n)
            .map(|i| {
                let before = if i > 0 { Some(seg(i - 1)) } else if sp.closed { Some(seg(n - 2)) } else { None };
                let after = if i + 1 < n { Some(seg(i)) } else if sp.closed { Some(seg(0)) } else { None };
                (before.unwrap_or(Vec3::ZERO) + after.unwrap_or(Vec3::ZERO)).normalize_or_zero()
            })
            .collect();
        (*acc.last().unwrap() > 1e-6).then_some(Self { pts, tans, acc, closed: sp.closed })
    }

    fn length(&self) -> f32 {
        *self.acc.last().unwrap()
    }

    /// Point and unit tangent at arc length `u`: a closed path wraps, an
    /// open one runs straight on past its ends.
    fn at(&self, u: f32) -> (Vec3, Vec3) {
        let l = self.length();
        let u = if self.closed { u.rem_euclid(l) } else { u };
        let n = self.pts.len();
        if !self.closed && u < 0.0 {
            let t = (self.pts[1] - self.pts[0]).normalize_or_zero();
            return (self.pts[0] + t * u, t);
        }
        if !self.closed && u > l {
            let t = (self.pts[n - 1] - self.pts[n - 2]).normalize_or_zero();
            return (self.pts[n - 1] + t * (u - l), t);
        }
        let i = self.acc.partition_point(|a| *a <= u).clamp(1, n - 1);
        let (a, b) = (self.pts[i - 1], self.pts[i]);
        let seg = (self.acc[i] - self.acc[i - 1]).max(1e-9);
        let t = ((u - self.acc[i - 1]) / seg).clamp(0.0, 1.0);
        (a.lerp(b, t), self.tans[i - 1].lerp(self.tans[i], t).normalize_or_zero())
    }
}

pub fn path_deform(d: &PathDeformData, mut v: StackValue, ctx: &EvalCtx<'_>) -> Result<StackValue, String> {
    let Some(path) = d.path else { return Ok(v) };
    let shape = ctx.node_eval_shape(path).ok_or("PathDeform: pick a shape as the path (one that does not depend on this object)")?;
    let table = PathTable::new(&shape).ok_or("PathDeform: the path has no length")?;
    // Path space → this object's space.
    let to_object = if d.world { ctx.object_inverse * ctx.node_world_matrix(path).unwrap_or(Mat4::IDENTITY) } else { Mat4::IDENTITY };
    let l = table.length();
    let axis = d.axis.get().min(2) as usize;
    let (ia, ib) = ((axis + 1) % 3, (axis + 2) % 3);
    let start = d.percent.get() / 100.0 * l;
    let stretch = d.stretch.get();
    let rotation = d.rotation.get().to_radians();
    let twist = d.twist.get().to_radians();
    let sign = if d.flip.get() { -1.0 } else { 1.0 };
    let n = v.mesh.vertex_count();
    let mut targets = Vec::with_capacity(n);
    for i in 0..n {
        let p = v.mesh.vertex_position(VertexId(i as u32)).unwrap_or_default();
        let along = p[axis] * stretch * sign;
        let u = start + along;
        let (pos, tan) = table.at(u);
        // Frame: the path's up (its local Z) made square to the tangent.
        let mut up = Vec3::Z - tan * Vec3::Z.dot(tan);
        if up.length_squared() < 1e-8 {
            up = Vec3::X - tan * Vec3::X.dot(tan);
        }
        let up = up.normalize();
        let side = up.cross(tan);
        let angle = rotation + twist * (along / l);
        let r = Quat::from_axis_angle(tan, angle);
        let off = r * (side * p[ia] * sign + up * p[ib]);
        targets.push(to_object.transform_point3(pos + off));
    }
    for (vert, t) in v.mesh.vertices_mut().iter_mut().zip(targets) {
        vert.position = t;
    }
    Ok(v)
}

/// Flex: vertices lag behind the object's motion on damped springs,
/// simulated from the Reference Frame to the current frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FlexData {
    pub flex: Param<f32>,
    pub strength: Param<f32>,
    pub sways: Param<f32>,
    pub chase_springs: bool,
    pub use_weights: bool,
    pub samples: u32,
    pub reference_frame: i32,
}

impl Default for FlexData {
    fn default() -> Self {
        Self { flex: Param::new(1.0), strength: Param::new(3.0), sways: Param::new(7.0), chase_springs: true, use_weights: true, samples: 5, reference_frame: 0 }
    }
}

pub fn flex(d: &FlexData, mut v: StackValue, ctx: &EvalCtx<'_>) -> Result<StackValue, String> {
    let Some(world_at) = ctx.self_world_at else { return Ok(v) };
    let now = crate::anim::time();
    // The result changes with the frame: the stack cache keys it by time.
    crate::anim::note_animated_read();
    let amount = d.flex.get();
    let t0 = d.reference_frame as f32;
    if amount == 0.0 || now <= t0 {
        return Ok(v);
    }
    let n = v.mesh.vertex_count();
    let rest: Vec<Vec3> = (0..n).map(|i| v.mesh.vertex_position(VertexId(i as u32)).unwrap_or_default()).collect();
    // Weights: 0 at the pivot, 1 at the farthest vertex (Use Weights off: all 1).
    let far = rest.iter().map(|p| p.length()).fold(0.0f32, f32::max).max(1e-6);
    let w: Vec<f32> = rest.iter().map(|p| if d.use_weights { p.length() / far } else { 1.0 }).collect();
    let k = if d.chase_springs { d.strength.get().max(0.0) } else { 0.0 };
    // Sways: more sways, less damping (default 7 → about a third of critical).
    let zeta = 0.7 / (1.0 + d.sways.get().max(0.0) / 7.0);
    let c = 2.0 * k.sqrt().max(0.5) * zeta;
    let steps = ((now - t0) * d.samples.max(1) as f32).ceil().max(1.0) as usize;
    let dt = (now - t0) / steps as f32;
    let m0 = world_at(t0);
    let mut x: Vec<Vec3> = rest.iter().map(|p| m0.transform_point3(*p)).collect();
    let mut vel = vec![Vec3::ZERO; n];
    let mut m = m0;
    for s in 1..=steps {
        m = world_at(t0 + dt * s as f32);
        for i in 0..n {
            let target = m.transform_point3(rest[i]);
            let a = (target - x[i]) * k - vel[i] * c;
            vel[i] += a * dt;
            x[i] += vel[i] * dt;
        }
    }
    let inv = m.inverse();
    for (i, vert) in v.mesh.vertices_mut().iter_mut().enumerate() {
        let target = m.transform_point3(rest[i]);
        let out = target + (x[i] - target) * (amount * w[i]);
        vert.position = inv.transform_point3(out);
    }
    Ok(v)
}

/// Spline IK Control: a helper per knot; the knots follow their helpers.
///
/// Mapping: `helpers` is in flat knot order (spline by spline) and
/// `knot_counts` holds each spline's knot count when the helpers were made,
/// so helper `j` of spline `s` sits at `knot_counts[..s].sum() + j`. A spline
/// is driven only while its knot count below is still the recorded one: a
/// knot added or removed below shifts the indices, and guessing which helper
/// belongs to which knot would silently move the wrong knots. Such a spline
/// (or a spline that is gone) passes through unchanged and a warning names
/// it and its unused helpers; the other splines keep following theirs.
/// Files without `knot_counts` treat the whole shape as one flat list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SplineIkControlData {
    pub helpers: Vec<Option<NodeId>>,
    /// Knots of each spline when Create Helpers ran.
    pub knot_counts: Vec<usize>,
    /// Link Types: 0 Link All in Hierarchy, 1 Link All to Root, 2 No Linking.
    pub link_type: u32,
    /// Helper Display: 0 Center Marker, 1 Axis Tripod, 2 Cross, 3 Box.
    pub display: u32,
    pub helper_size: f32,
}

impl Default for SplineIkControlData {
    fn default() -> Self {
        Self { helpers: Vec::new(), knot_counts: Vec::new(), link_type: 0, display: 2, helper_size: 20.0 }
    }
}

pub fn spline_ik_control(d: &SplineIkControlData, mut v: StackValue, ctx: &EvalCtx<'_>) -> Result<StackValue, String> {
    let mut sh = v.shape.take().ok_or("Spline IK Control needs a shape below it")?;
    // Without recorded counts (an older file): the whole shape as one flat list.
    let legacy;
    let counts: &[usize] = if d.knot_counts.is_empty() {
        legacy = [d.helpers.len()];
        &legacy
    } else {
        &d.knot_counts
    };
    let flat_now = d.knot_counts.is_empty();
    let spline_count = sh.splines.len();
    let mut start = 0usize;
    for (s, &count) in counts.iter().enumerate() {
        let helpers = d.helpers.get(start..(start + count).min(d.helpers.len())).unwrap_or(&[]);
        start += count;
        let used = helpers.iter().flatten().count();
        let knots: Vec<&mut loom_shape::Knot> = if flat_now { sh.splines.iter_mut().flat_map(|sp| sp.knots.iter_mut()).collect() } else { sh.splines.get_mut(s).map(|sp| sp.knots.iter_mut().collect()).unwrap_or_default() };
        if knots.len() != count {
            if used > 0 {
                let which = if flat_now { "the shape".to_string() } else { format!("spline {}", s + 1) };
                let now = if flat_now || s < spline_count { format!("has {} knots", knots.len()) } else { "is gone".to_string() };
                v.warnings.push(format!("Spline IK Control: {which} {now} (its helpers were made for {count}); its {used} helper(s) are not used"));
            }
            continue;
        }
        for (k, h) in knots.into_iter().zip(helpers) {
            if let Some(m) = h.and_then(|h| ctx.node_world_matrix(h)) {
                // The knot moves with its helper; its handles keep their vectors.
                k.pos = ctx.object_inverse.transform_point3(m.w_axis.truncate());
            }
        }
    }
    v.mesh = crate::node::shape_cage_mesh(&sh, false);
    v.shape = Some(sh);
    v.shape_selection = None;
    Ok(v)
}
