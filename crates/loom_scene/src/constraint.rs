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
    /// Surface Constraint: on a Sphere, Cylinder, Torus, Plane or Quad Patch
    /// at U / V Position (percent around / along it), lifted by Offset along
    /// the normal; `align` 0 none, 1 X along U, 2 X along V (Z on the normal).
    Surface { target: NodeId, u: Param<f32>, v: Param<f32>, offset: f32, align: u32, flip: bool },
}

/// A point of a parametric surface at `u`, `v` (0-1) of the object `kind`
/// with its `params` (rollout name, value), in object space: (point, dP/du, dP/dv).
pub fn surface_point(kind: &str, params: &[(&str, f32)], u: f32, v: f32) -> Option<(Vec3, Vec3, Vec3)> {
    use std::f32::consts::{PI, TAU};
    let p = |n: &str| params.iter().find(|(k, _)| *k == n).map(|(_, x)| *x);
    match kind {
        "Sphere" => {
            let r = p("Radius")?;
            // U around Z, V from the bottom pole to the top.
            let (a, b) = (u * TAU, (v - 0.5) * PI);
            let pt = Vec3::new(b.cos() * a.cos(), b.cos() * a.sin(), b.sin()) * r;
            let du = Vec3::new(-a.sin(), a.cos(), 0.0) * (b.cos().max(1e-3) * r);
            let dv = Vec3::new(-b.sin() * a.cos(), -b.sin() * a.sin(), b.cos()) * r;
            Some((pt, du, dv))
        }
        "Cylinder" => {
            let (r, h) = (p("Radius")?, p("Height")?);
            let a = u * TAU;
            Some((Vec3::new(r * a.cos(), r * a.sin(), v * h), Vec3::new(-a.sin(), a.cos(), 0.0) * r, Vec3::Z * h.abs().max(1e-3) * h.signum()))
        }
        "Torus" => {
            let (r1, r2) = (p("Radius 1")?, p("Radius 2")?);
            let (a, b) = (u * TAU, v * TAU);
            let ring = Vec3::new(a.cos(), a.sin(), 0.0);
            let pt = ring * (r1 + r2 * b.cos()) + Vec3::Z * r2 * b.sin();
            let du = Vec3::new(-a.sin(), a.cos(), 0.0) * (r1 + r2 * b.cos());
            let dv = ring * (-r2 * b.sin()) + Vec3::Z * r2 * b.cos();
            Some((pt, du, dv))
        }
        "Plane" | "QuadPatch" => {
            let (l, w) = (p("Length")?, p("Width")?);
            Some((Vec3::new((u - 0.5) * w, (v - 0.5) * l, 0.0), Vec3::X * w, Vec3::Y * l))
        }
        _ => None,
    }
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
    /// `roll`: degrees about the aiming axis (the target camera's Roll track).
    LookAt {
        targets: Vec<Target>,
        axis: u32,
        flip: bool,
        upnode: Option<NodeId>,
        keep_offset: bool,
        offset: Quat,
        #[serde(default = "no_roll")]
        roll: Param<f32>,
    },
    Orientation { targets: Vec<Target>, keep_offset: bool, offset: Quat },
}

fn no_roll() -> Param<f32> {
    Param::new(0.0)
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
    /// Key Modes: what Add Link keys at the link frame.
    #[serde(default)]
    pub key_mode: LinkKeyMode,
    /// Key Nodes / Key Entire Hierarchy: Parents (the targets too) or Child only.
    #[serde(default)]
    pub key_parents: bool,
}

/// Link Constraint Key Modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LinkKeyMode {
    /// No keys on any of the objects.
    #[default]
    NoKey,
    /// The child (and with Parents its targets) get transform keys.
    KeyNodes,
    /// The same nodes and every ancestor up to the world.
    KeyHierarchy,
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
            Some(PositionConstraint::Attachment { target, .. }) | Some(PositionConstraint::Surface { target, .. }) => out.push(*target),
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
            Some(PositionConstraint::Attachment { target, .. }) | Some(PositionConstraint::Surface { target, .. }) => match f(*target) {
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
            PositionConstraint::Surface { .. } => "Surface",
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
