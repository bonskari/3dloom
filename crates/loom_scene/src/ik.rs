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
    HD,
    Limb,
    Spline,
}

impl IkSolver {
    pub fn label(self) -> &'static str {
        match self {
            IkSolver::HI => "HI Solver",
            IkSolver::HD => "HD Solver",
            IkSolver::Limb => "IK Limb Solver",
            IkSolver::Spline => "Spline IK Solver",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "hi" | "hi solver" | "history-independent" | "history-independent ik solver" => Some(IkSolver::HI),
            "hd" | "hd solver" | "history-dependent" | "history-dependent ik solver" => Some(IkSolver::HD),
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

/// Per-axis joint parameters (Hierarchy > IK > Rotational Joints / Sliding Joints).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct JointAxis {
    pub active: bool,
    pub limited: bool,
    pub ease: bool,
    pub from: f32,
    pub to: f32,
    pub preferred: f32,
    pub spring_back: bool,
    pub spring_back_val: f32,
    pub spring_tension: f32,
    pub damping: f32,
}

impl JointAxis {
    pub const fn default_rotational() -> Self {
        Self {
            active: true,
            limited: false,
            ease: false,
            from: -180.0,
            to: 180.0,
            preferred: 0.0,
            spring_back: false,
            spring_back_val: 0.0,
            spring_tension: 0.02,
            damping: 0.0,
        }
    }

    pub const fn default_sliding() -> Self {
        Self {
            active: false,
            limited: false,
            ease: false,
            from: -100.0,
            to: 100.0,
            preferred: 0.0,
            spring_back: false,
            spring_back_val: 0.0,
            spring_tension: 0.02,
            damping: 0.0,
        }
    }
}

impl Default for JointAxis {
    fn default() -> Self {
        Self::default_rotational()
    }
}

/// Rotational and Sliding joint settings of a node (Hierarchy > IK).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NodeJoints {
    pub rotational: [JointAxis; 3],
    pub sliding: [JointAxis; 3],
    /// Object Parameters > Terminator: Interactive / Applied IK chains stop here.
    #[serde(default)]
    pub terminator: bool,
    /// Object Parameters > Bind to Follow Object: Applied IK pulls this
    /// node (the end effector) onto the follow object's pivot.
    #[serde(default)]
    pub follow: Option<NodeId>,
}

impl Default for NodeJoints {
    fn default() -> Self {
        Self {
            rotational: [JointAxis::default_rotational(); 3],
            sliding: [JointAxis::default_sliding(); 3],
            terminator: false,
            follow: None,
        }
    }
}

impl NodeJoints {
    /// Clamps and damps a solved local rotation `q` against `fk` according to
    /// the node's Rotational Joints (Active, Limited From..To, Spring Back, Damping).
    pub fn constrain_rotation(&self, fk: Quat, q: Quat) -> Quat {
        if self.rotational.iter().all(|a| a.active && !a.limited && !a.spring_back && a.damping == 0.0) {
            return q;
        }
        let (fx, fy, fz) = fk.to_euler(glam::EulerRot::XYZ);
        let fk_deg = [fx.to_degrees(), fy.to_degrees(), fz.to_degrees()];
        let (qx, qy, qz) = q.to_euler(glam::EulerRot::XYZ);
        let mut deg = [qx.to_degrees(), qy.to_degrees(), qz.to_degrees()];
        for k in 0..3 {
            let ax = &self.rotational[k];
            if !ax.active {
                deg[k] = fk_deg[k];
                continue;
            }
            if ax.damping > 0.0 {
                let d = ax.damping.clamp(0.0, 1.0);
                deg[k] = fk_deg[k] + (deg[k] - fk_deg[k]) * (1.0 - d);
            }
            if ax.spring_back {
                let t = ax.spring_tension.clamp(0.0, 1.0);
                deg[k] += (ax.spring_back_val - deg[k]) * t;
            }
            if ax.limited {
                let lo = ax.from.min(ax.to);
                let hi = ax.from.max(ax.to);
                deg[k] = deg[k].clamp(lo, hi);
            }
        }
        Quat::from_euler(
            glam::EulerRot::XYZ,
            deg[0].to_radians(),
            deg[1].to_radians(),
            deg[2].to_radians(),
        )
        .normalize()
    }
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
    pub joints: NodeJoints,
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
    let mut q: Vec<Quat> = bones
        .iter()
        .map(|b| {
            let base = b.rotation.normalize();
            let pref = b.joints.rotational;
            if pref.iter().any(|a| a.preferred.abs() > 1e-4) {
                let (mut x, mut y, mut z) = base.to_euler(glam::EulerRot::XYZ);
                if pref[0].active && pref[0].preferred.abs() > 1e-4 { x = pref[0].preferred.to_radians(); }
                if pref[1].active && pref[1].preferred.abs() > 1e-4 { y = pref[1].preferred.to_radians(); }
                if pref[2].active && pref[2].preferred.abs() > 1e-4 { z = pref[2].preferred.to_radians(); }
                Quat::from_euler(glam::EulerRot::XYZ, x, y, z).normalize()
            } else {
                base
            }
        })
        .collect();
    match chain.solver {
        IkSolver::HI | IkSolver::HD => {
            let hd = chain.solver == IkSolver::HD;
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
                    let r = if hd {
                        // History-Dependent solver: root-to-tip joint damping so end bones absorb motion first.
                        let weight = ((i + 1) as f32 / n as f32).clamp(0.15, 1.0);
                        Quat::IDENTITY.slerp(r, weight)
                    } else {
                        r
                    };
                    let parent_rot = rot_of(if i == 0 { bones[0].parent_frame } else { ws[i - 1] });
                    let raw = (parent_rot.inverse() * r * rot_of(ws[i])).normalize();
                    q[i] = bones[i].joints.constrain_rotation(bones[i].rotation, raw);
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
            // Two bones: the elbow from the law of cosines, in the plane the
            // swivel angle turns about the root→goal axis (0 = the first bone's +Y).
            let ws = worlds(bones, &q);
            let root = ws[0].transform_point3(Vec3::ZERO);
            let a = bones[0].tip.length().max(1e-4);
            let b = if n > 1 { bones[1].tip.length().max(1e-4) } else { 0.0 };
            let to_goal = goal_world - root;
            let d = to_goal.length().clamp((a - b).abs() + 1e-4, a + b - 1e-4);
            let u = to_goal.normalize_or_zero();
            if u == Vec3::ZERO {
                return bones.iter().zip(q).map(|(b, r)| (b.id, r)).collect();
            }
            let pole = ws[0].transform_vector3(perpendicular(bones[0].tip));
            let mut v = pole - u * pole.dot(u);
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
