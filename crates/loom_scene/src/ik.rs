//! IK chains (Hierarchy > IK): the HI solver (CCD with a swivel angle), the
//! IK Limb solver (two bones, analytic) and Spline IK (bones laid along a
//! shape). `solve` returns the local rotation of every bone of a chain;
//! positions and scales stay FK.

use crate::NodeId;
use glam::{Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum IkSolver {
    #[default]
    HI,
    Limb,
    Spline,
}

impl IkSolver {
    pub fn label(self) -> &'static str {
        match self {
            IkSolver::HI => "HI Solver",
            IkSolver::Limb => "IK Limb Solver",
            IkSolver::Spline => "Spline IK Solver",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "hi" | "hi solver" => Some(IkSolver::HI),
            "limb" | "ik limb" | "ik limb solver" => Some(IkSolver::Limb),
            "spline" | "spline ik" | "spline ik solver" => Some(IkSolver::Spline),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IkChain {
    pub id: u32,
    pub name: String,
    pub solver: IkSolver,
    /// The root bone of the chain (the IK terminator).
    pub start: NodeId,
    /// The last bone; the goal sits at its base.
    pub end: NodeId,
    pub goal: NodeId,
    pub enabled: bool,
    pub swivel_deg: f32,
    pub threshold: f32,
    pub iterations: u32,
    /// Spline IK: the shape node the bones follow.
    pub spline: Option<NodeId>,
    /// IK for FK Pose: the chain also solves while a bone is rotated by hand.
    pub ik_for_fk: bool,
}

/// One bone of a chain, FK: `position` / `rotation` are the local transform
/// (the bone's base sits at its parent's tip), `parent_frame` matters for the
/// first bone only (the rest chain from the bone before). `tip` is the next
/// joint in this bone's frame (the child's local position), whatever axis the
/// bone points along (+X for native bones, +Y for FBX skeletons).
#[derive(Debug, Clone)]
pub struct IkBone {
    pub id: NodeId,
    pub parent_frame: Mat4,
    pub position: Vec3,
    pub rotation: Quat,
    pub tip: Vec3,
}

/// A unit vector perpendicular to `v` (the bone's "up" in its own frame).
fn perpendicular(v: Vec3) -> Vec3 {
    let v = v.normalize_or_zero();
    let mut p = Vec3::Y - v * Vec3::Y.dot(v);
    if p.length_squared() < 1e-6 {
        p = Vec3::Z - v * Vec3::Z.dot(v);
    }
    p.normalize_or_zero()
}

/// The rotation taking the local pair (dir, up) onto the world pair (dir, up).
fn frame_rotation(dir_local: Vec3, up_local: Vec3, dir_world: Vec3, up_world: Vec3) -> Quat {
    let basis = |d: Vec3, u: Vec3| {
        let x = d.normalize_or_zero();
        let mut y = u - x * u.dot(x);
        if y.length_squared() < 1e-8 {
            y = perpendicular(x);
        }
        let y = y.normalize();
        glam::Mat3::from_cols(x, y, x.cross(y))
    };
    let l = basis(dir_local, up_local);
    let w = basis(dir_world, up_world);
    Quat::from_mat3(&(w * l.transpose())).normalize()
}

fn worlds(bones: &[IkBone], q: &[Quat]) -> Vec<Mat4> {
    // (the bone after `i` sits at `bones[i].tip`, which is its own FK position)
    let mut out = Vec::with_capacity(bones.len());
    let mut m = Mat4::IDENTITY;
    for (i, b) in bones.iter().enumerate() {
        let parent = if i == 0 { b.parent_frame } else { m };
        m = parent * Mat4::from_translation(b.position) * Mat4::from_quat(q[i]);
        out.push(m);
    }
    out
}

fn rot_of(m: Mat4) -> Quat {
    m.to_scale_rotation_translation().1.normalize()
}

/// A world rotation taking local +X to `dir` with local +Y as close to `up` as possible.
#[allow(dead_code)]
fn look_rotation(dir: Vec3, up: Vec3) -> Quat {
    let x = dir.normalize_or_zero();
    if x == Vec3::ZERO {
        return Quat::IDENTITY;
    }
    let mut y = up - x * up.dot(x);
    if y.length_squared() < 1e-8 {
        y = Vec3::Z - x * Vec3::Z.dot(x);
        if y.length_squared() < 1e-8 {
            y = Vec3::Y - x * Vec3::Y.dot(x);
        }
    }
    let y = y.normalize();
    let z = x.cross(y);
    Quat::from_mat3(&glam::Mat3::from_cols(x, y, z)).normalize()
}

/// Local rotations of the chain's bones for `goal_world` (the goal's world
/// position); `spline` is the shape in world space for Spline IK.
pub fn solve(chain: &IkChain, bones: &[IkBone], goal_world: Vec3, spline: Option<&loom_shape::Shape>) -> Vec<(NodeId, Quat)> {
    let n = bones.len();
    if n == 0 {
        return Vec::new();
    }
    let mut q: Vec<Quat> = bones.iter().map(|b| b.rotation.normalize()).collect();
    match chain.solver {
        IkSolver::HI => {
            let iterations = chain.iterations.max(1);
            'outer: for _ in 0..iterations {
                for i in (0..n).rev() {
                    let ws = worlds(bones, &q);
                    let tip = ws[n - 1].transform_point3(bones[n - 1].tip);
                    if tip.distance(goal_world) < chain.threshold.max(1e-4) {
                        break 'outer;
                    }
                    let joint = ws[i].transform_point3(Vec3::ZERO);
                    let a = (tip - joint).normalize_or_zero();
                    let b = (goal_world - joint).normalize_or_zero();
                    if a == Vec3::ZERO || b == Vec3::ZERO {
                        continue;
                    }
                    let r = Quat::from_rotation_arc(a, b);
                    let parent_rot = rot_of(if i == 0 { bones[0].parent_frame } else { ws[i - 1] });
                    q[i] = (parent_rot.inverse() * r * rot_of(ws[i])).normalize();
                }
            }
            if chain.swivel_deg.abs() > 1e-6 {
                let ws = worlds(bones, &q);
                let root = ws[0].transform_point3(Vec3::ZERO);
                let axis = (goal_world - root).normalize_or_zero();
                if axis != Vec3::ZERO {
                    let parent_rot = rot_of(bones[0].parent_frame);
                    q[0] = (parent_rot.inverse() * Quat::from_axis_angle(axis, chain.swivel_deg.to_radians()) * rot_of(ws[0])).normalize();
                }
            }
        }
        IkSolver::Limb => {
            // Legacy scenes can contain a Limb chain with the wrong number
            // of segments. It cannot be solved analytically; keep its FK pose.
            if n != 2 {
                return bones.iter().zip(q).map(|(b, r)| (b.id, r)).collect();
            }
            // Two bones: preserve the existing elbow/knee bend side, with
            // the swivel rotating that plane about the root→goal axis.
            let ws = worlds(bones, &q);
            let fk_tip = ws[1].transform_point3(bones[1].tip);
            if chain.swivel_deg.abs() < 1e-6 && fk_tip.distance_squared(goal_world) < 1e-10 {
                return bones.iter().zip(q).map(|(b, r)| (b.id, r)).collect();
            }
            let root = ws[0].transform_point3(Vec3::ZERO);
            let a = bones[0].tip.length().max(1e-4);
            let b = if n > 1 { bones[1].tip.length().max(1e-4) } else { 0.0 };
            let to_goal = goal_world - root;
            let d = to_goal.length().clamp((a - b).abs() + 1e-4, a + b - 1e-4);
            let u = to_goal.normalize_or_zero();
            if u == Vec3::ZERO {
                return bones.iter().zip(q).map(|(b, r)| (b.id, r)).collect();
            }
            let elbow_direction = ws[1].transform_point3(Vec3::ZERO) - root;
            let mut v = elbow_direction - u * elbow_direction.dot(u);
            if v.length_squared() < 1e-8 {
                // A straight limb has no bend plane: retain the old local-up
                // convention until an elbow/knee direction is available.
                let pole = ws[0].transform_vector3(perpendicular(bones[0].tip));
                v = pole - u * pole.dot(u);
            }
            if v.length_squared() < 1e-8 {
                v = Vec3::Z - u * Vec3::Z.dot(u);
                if v.length_squared() < 1e-8 {
                    v = Vec3::Y - u * Vec3::Y.dot(u);
                }
            }
            let v = Quat::from_axis_angle(u, chain.swivel_deg.to_radians()).mul_vec3(v.normalize());
            let cos_a = ((a * a + d * d - b * b) / (2.0 * a * d)).clamp(-1.0, 1.0);
            let sin_a = (1.0 - cos_a * cos_a).max(0.0).sqrt();
            let elbow = root + u * (a * cos_a) + v * (a * sin_a);
            let rw0 = frame_rotation(bones[0].tip, perpendicular(bones[0].tip), elbow - root, v);
            let parent_rot = rot_of(bones[0].parent_frame);
            q[0] = (parent_rot.inverse() * rw0).normalize();
            if n > 1 {
                let rw1 = frame_rotation(bones[1].tip, perpendicular(bones[1].tip), goal_world - elbow, v);
                q[1] = (rw0.inverse() * rw1).normalize();
            }
        }
        IkSolver::Spline => {
            let Some(shape) = spline else {
                return bones.iter().zip(q).map(|(b, r)| (b.id, r)).collect();
            };
            let pts: Vec<Vec3> = shape.splines.first().map(|s| s.polyline(shape.steps.max(1), false, false)).unwrap_or_default();
            if pts.len() < 2 {
                return bones.iter().zip(q).map(|(b, r)| (b.id, r)).collect();
            }
            let mut cum = vec![0.0f32];
            for w in pts.windows(2) {
                cum.push(cum.last().unwrap() + w[0].distance(w[1]));
            }
            let total = *cum.last().unwrap();
            let point_at = |s: f32| -> Vec3 {
                let s = s.clamp(0.0, total);
                for i in 1..cum.len() {
                    if s <= cum[i] {
                        let seg = (cum[i] - cum[i - 1]).max(1e-6);
                        return pts[i - 1].lerp(pts[i], (s - cum[i - 1]) / seg);
                    }
                }
                *pts.last().unwrap()
            };
            let mut s = 0.0;
            let mut parent_rot = rot_of(bones[0].parent_frame);
            let fk = worlds(bones, &q);
            for (i, b) in bones.iter().enumerate() {
                let len = b.tip.length();
                let base = point_at(s);
                let mut tip = point_at(s + len);
                if tip.distance(base) < 1e-5 {
                    // Past the end of the shape: continue along its last direction.
                    let dir = (pts[pts.len() - 1] - pts[pts.len() - 2]).normalize_or_zero();
                    tip = base + dir * len;
                }
                let up_local = perpendicular(b.tip);
                let up = fk[i].transform_vector3(up_local);
                let rw = frame_rotation(b.tip, up_local, tip - base, up);
                q[i] = (parent_rot.inverse() * rw).normalize();
                parent_rot = rw;
                s += len;
            }
        }
    }
    bones.iter().zip(q).map(|(b, r)| (b.id, r)).collect()
}

/// The first point of the shape's first spline (Spline IK moves the chain's root there).
pub fn spline_start(shape: &loom_shape::Shape) -> Option<Vec3> {
    shape.splines.first().and_then(|s| s.knots.first()).map(|k| k.pos)
}
