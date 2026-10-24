//! Transform constraints: Path, Position, Attachment (position), LookAt,
//! Orientation (rotation) and Link (the frame a node hangs in, by time).

use crate::node::NodeId;
use crate::param::Param;
use glam::{Mat3, Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};

/// A constraint target with its (animatable) weight, 0–100.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Target {
    pub node: NodeId,
    pub weight: Param<f32>,
}

impl Target {
    /// The default target weight.
    pub fn new(node: NodeId) -> Self {
        Self { node, weight: Param::new(50.0) }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PositionConstraint {
    Path(PathConstraint),
    Position { targets: Vec<Target>, keep_offset: bool, offset: Vec3 },
    Attachment { target: NodeId, face: u32, a: f32, b: f32, align: bool },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PathConstraint {
    pub targets: Vec<Target>,
    pub percent: Param<f32>,
    pub follow: bool,
    pub bank: bool,
    pub bank_amount: f32,
    pub smoothness: f32,
    pub allow_upside_down: bool,
    pub constant_velocity: bool,
    pub loop_path: bool,
    pub relative: bool,
    /// Follow axis: 0 X, 1 Y, 2 Z.
    pub axis: u32,
    pub flip: bool,
    /// Relative: the object's offset from the path start when assigned.
    pub offset: Vec3,
}

impl Default for PathConstraint {
    fn default() -> Self {
        Self { targets: Vec::new(), percent: Param::new(0.0), follow: false, bank: false, bank_amount: 0.5, smoothness: 0.5, allow_upside_down: false, constant_velocity: false, loop_path: false, relative: false, axis: 0, flip: false, offset: Vec3::ZERO }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RotationConstraint {
    LookAt { targets: Vec<Target>, axis: u32, flip: bool, upnode: Option<NodeId>, keep_offset: bool, offset: Quat },
    Orientation { targets: Vec<Target>, keep_offset: bool, offset: Quat },
}

/// Link Constraint: from each frame on, the node hangs from that target
/// (None = the world), keeping its world pose at the switch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct LinkConstraint {
    pub links: Vec<(f32, Option<NodeId>)>,
    /// Per link, the node's frame in its target's space when the link was
    /// added (so a later plain move of the target carries the node). Empty
    /// in older files: the offsets then follow from the link frames.
    #[serde(default)]
    pub offsets: Vec<Mat4>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Constraints {
    pub position: Option<PositionConstraint>,
    pub rotation: Option<RotationConstraint>,
    pub link: Option<LinkConstraint>,
}

impl Constraints {
    pub fn is_empty(&self) -> bool {
        self.position.is_none() && self.rotation.is_none() && self.link.is_none()
    }

    pub fn nodes(&self) -> Vec<NodeId> {
        let mut out = Vec::new();
        match &self.position {
            Some(PositionConstraint::Path(p)) => out.extend(p.targets.iter().map(|t| t.node)),
            Some(PositionConstraint::Position { targets, .. }) => out.extend(targets.iter().map(|t| t.node)),
            Some(PositionConstraint::Attachment { target, .. }) => out.push(*target),
            None => {}
        }
        match &self.rotation {
            Some(RotationConstraint::LookAt { targets, upnode, .. }) => {
                out.extend(targets.iter().map(|t| t.node));
                out.extend(*upnode);
            }
            Some(RotationConstraint::Orientation { targets, .. }) => out.extend(targets.iter().map(|t| t.node)),
            None => {}
        }
        if let Some(l) = &self.link {
            out.extend(l.links.iter().filter_map(|(_, n)| *n));
        }
        out
    }

    pub fn remap_ids(&mut self, f: &dyn Fn(NodeId) -> Option<NodeId>) {
        let fix = |ts: &mut Vec<Target>| ts.retain_mut(|t| f(t.node).map(|n| t.node = n).is_some());
        match &mut self.position {
            Some(PositionConstraint::Path(p)) => fix(&mut p.targets),
            Some(PositionConstraint::Position { targets, .. }) => fix(targets),
            Some(PositionConstraint::Attachment { target, .. }) => match f(*target) {
                Some(n) => *target = n,
                None => self.position = None,
            },
            None => {}
        }
        match &mut self.rotation {
            Some(RotationConstraint::LookAt { targets, upnode, .. }) => {
                fix(targets);
                *upnode = upnode.and_then(f);
            }
            Some(RotationConstraint::Orientation { targets, .. }) => fix(targets),
            None => {}
        }
        if let Some(l) = &mut self.link {
            for (_, n) in &mut l.links {
                *n = n.and_then(f);
            }
        }
    }

    /// Controller names, for the Motion panel and Track View.
    pub fn position_name(&self) -> Option<&'static str> {
        self.position.as_ref().map(|p| match p {
            PositionConstraint::Path(_) => "Path Constraint",
            PositionConstraint::Position { .. } => "Position Constraint",
            PositionConstraint::Attachment { .. } => "Attachment",
        })
    }

    pub fn rotation_name(&self) -> Option<&'static str> {
        self.rotation.as_ref().map(|r| match r {
            RotationConstraint::LookAt { .. } => "LookAt Constraint",
            RotationConstraint::Orientation { .. } => "Orientation Constraint",
        })
    }
}

/// Normalised weights (all zero = equal weights).
pub fn weights(targets: &[Target]) -> Vec<f32> {
    let w: Vec<f32> = targets.iter().map(|t| t.weight.get().max(0.0)).collect();
    let sum: f32 = w.iter().sum();
    if sum <= 0.0 {
        return vec![1.0 / targets.len().max(1) as f32; targets.len()];
    }
    w.into_iter().map(|x| x / sum).collect()
}

/// A point and unit tangent at `u` (0..1) along a spline (the first of the shape).
pub fn path_point(shape: &loom_shape::Shape, u: f32, constant_velocity: bool) -> Option<(Vec3, Vec3)> {
    let sp = shape.splines.first()?;
    let n = sp.segment_count();
    if n == 0 {
        return sp.knots.first().map(|k| (k.pos, Vec3::X));
    }
    let u = u.clamp(0.0, 1.0);
    let (seg, t) = if constant_velocity {
        let lens: Vec<f32> = (0..n).map(|i| loom_shape::interp::length(&sp.segment(i).unwrap(), 32)).collect();
        let total: f32 = lens.iter().sum();
        let mut d = u * total;
        let mut seg = n - 1;
        for (i, l) in lens.iter().enumerate() {
            if d <= *l || i == n - 1 {
                seg = i;
                break;
            }
            d -= l;
        }
        (seg, loom_shape::interp::t_at_length(&sp.segment(seg).unwrap(), d.min(lens[seg])))
    } else {
        let x = u * n as f32;
        let seg = (x.floor() as usize).min(n - 1);
        (seg, x - seg as f32)
    };
    let b = sp.segment(seg)?;
    Some((loom_shape::interp::point_at(&b, t), loom_shape::interp::tangent_at(&b, t).normalize_or_zero()))
}

/// The rotation that points `axis` (flipped when asked) along `dir`,
/// keeping the next axis as close to `up` as possible.
pub fn aim(dir: Vec3, axis: u32, flip: bool, up: Vec3) -> Quat {
    let f = if flip { -dir } else { dir }.normalize_or_zero();
    if f == Vec3::ZERO {
        return Quat::IDENTITY;
    }
    let up = if f.cross(up).length_squared() < 1e-8 { if f.cross(Vec3::Y).length_squared() < 1e-8 { Vec3::X } else { Vec3::Y } } else { up };
    // Basis: a = f, the "up-like" axis is the one after `axis` cyclically... world Z stays up.
    let side = up.cross(f).normalize();
    let upv = f.cross(side).normalize();
    let m = match axis {
        // X aims: Y = side, Z = up.
        0 => Mat3::from_cols(f, side, upv),
        // Y aims: Z = up, X = side... (right-handed: X = Y × Z).
        1 => Mat3::from_cols(f.cross(upv), f, upv),
        // Z aims: Y = up-ish, X = Y × Z.
        _ => {
            let y = upv;
            Mat3::from_cols(y.cross(f), y, f)
        }
    };
    Quat::from_mat3(&m).normalize()
}

/// Weighted average of rotations (sign-aligned normalised sum).
pub fn blend_rotations(rots: &[(Quat, f32)]) -> Quat {
    let Some(first) = rots.first() else { return Quat::IDENTITY };
    let mut acc = glam::Vec4::ZERO;
    for (q, w) in rots {
        let v = glam::Vec4::from(*q);
        let v = if v.dot(glam::Vec4::from(first.0)) < 0.0 { -v } else { v };
        acc += v * *w;
    }
    Quat::from_vec4(acc).normalize()
}

pub fn decompose(m: Mat4) -> (Vec3, Quat, Vec3) {
    m.to_scale_rotation_translation()
}
