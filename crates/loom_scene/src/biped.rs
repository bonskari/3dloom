//! Systems > Biped: a humanoid skeleton of named parts under a centre of
//! mass (the root, "Bip001" by default). The root holds the Structure
//! parameters; `layout` builds the parts those parameters ask for, and the
//! command layer turns the layout into nodes (bones with box shapes).
//!
//! Biped space: the root's own frame, origin at the centre of mass, +Z up,
//! the figure facing -Y with its left side towards +X.

use crate::node::Transform;
use crate::param::Param;
use glam::{Mat3, Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};

/// Height of a new biped in metres (scaled to the scene's units by the caller).
pub const DEFAULT_HEIGHT_METRES: f32 = 1.8;
/// The root's base name: the first biped is Bip001, the next Bip002, ...
pub const ROOT_BASE: &str = "Bip";

/// The Structure rollout.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BipedStructure {
    pub arms: Param<bool>,
    pub neck_links: Param<u32>,
    pub spine_links: Param<u32>,
    pub leg_links: Param<u32>,
    pub tail_links: Param<u32>,
    pub fingers: Param<u32>,
    pub finger_links: Param<u32>,
    pub toes: Param<u32>,
    pub toe_links: Param<u32>,
    pub height: Param<f32>,
    /// Figure Mode (Biped rollout): the structure can be edited and the parts
    /// show the figure pose. A mode, not an animatable parameter.
    #[serde(default)]
    pub figure_mode: bool,
    /// The figure pose by part name (without the root name), stored while
    /// Figure Mode is off.
    #[serde(default)]
    pub figure_pose: Vec<(String, Transform)>,
    /// The animated pose by part name, stored while Figure Mode is on.
    #[serde(default)]
    pub anim_pose: Vec<(String, Transform)>,
}

impl BipedStructure {
    /// The reference defaults: Neck Links 1, Spine Links 4, Leg Links 3,
    /// Tail Links 0, Fingers 1, Finger Links 1, Toes 1, Toe Links 3, arms on.
    pub fn new(height: f32) -> Self {
        Self {
            arms: Param::new(true),
            neck_links: Param::new(1),
            spine_links: Param::new(4),
            leg_links: Param::new(3),
            tail_links: Param::new(0),
            fingers: Param::new(1),
            finger_links: Param::new(1),
            toes: Param::new(1),
            toe_links: Param::new(3),
            height: Param::new(height.max(MIN_HEIGHT)),
            figure_mode: true,
            figure_pose: Vec::new(),
            anim_pose: Vec::new(),
        }
    }

    /// Structure parameter names with their ranges (name, min, max).
    pub const LINKS: [(&'static str, u32, u32); 8] = [
        ("Neck Links", 1, 25),
        ("Spine Links", 1, 10),
        ("Leg Links", 3, 4),
        ("Tail Links", 0, 25),
        ("Fingers", 0, 5),
        ("Finger Links", 1, 4),
        ("Toes", 1, 5),
        ("Toe Links", 1, 3),
    ];

    pub fn count_mut(&mut self, name: &str) -> Option<&mut Param<u32>> {
        Some(match name {
            "Neck Links" => &mut self.neck_links,
            "Spine Links" => &mut self.spine_links,
            "Leg Links" => &mut self.leg_links,
            "Tail Links" => &mut self.tail_links,
            "Fingers" => &mut self.fingers,
            "Finger Links" => &mut self.finger_links,
            "Toes" => &mut self.toes,
            "Toe Links" => &mut self.toe_links,
            _ => return None,
        })
    }

    pub fn count(&self, name: &str) -> Option<u32> {
        self.clone().count_mut(name).map(|p| p.get())
    }

    /// Sets a structure parameter by its rollout name, clamped to its range.
    pub fn set(&mut self, name: &str, value: f64) -> Result<(), String> {
        match name {
            "Arms" => self.arms.set(value != 0.0),
            "Height" => self.height.set((value as f32).max(MIN_HEIGHT)),
            _ => {
                let (_, lo, hi) = Self::LINKS.iter().find(|(n, _, _)| *n == name).copied().ok_or_else(|| format!("Biped has no parameter {name:?}"))?;
                let v = (value.round().max(0.0) as u32).clamp(lo, hi);
                self.count_mut(name).expect("listed").set(v);
            }
        }
        Ok(())
    }

    /// What decides each group's parts (a group is laid out again when its key changes).
    fn group_key(&self, group: PartGroup) -> Vec<u32> {
        let arms = self.arms.get() as u32;
        match group {
            PartGroup::Centre => vec![],
            PartGroup::Spine => vec![self.spine_links.get()],
            PartGroup::Neck => vec![self.neck_links.get()],
            PartGroup::Head => vec![],
            PartGroup::Arm => vec![arms],
            PartGroup::Fingers => vec![arms, self.fingers.get(), self.finger_links.get()],
            PartGroup::Leg => vec![self.leg_links.get()],
            PartGroup::Toes => vec![self.leg_links.get(), self.toes.get(), self.toe_links.get()],
            PartGroup::Tail => vec![self.tail_links.get()],
        }
    }

    /// True when `self` and `other` lay out `group` differently.
    pub fn group_changed(&self, other: &Self, group: PartGroup) -> bool {
        self.group_key(group) != other.group_key(group)
    }
}

pub const MIN_HEIGHT: f32 = 0.01;

/// Which side a part is on; it picks the part's colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Centre,
    Left,
    Right,
}

/// Parts that are laid out together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartGroup {
    Centre,
    Spine,
    Neck,
    Head,
    Arm,
    Fingers,
    Leg,
    Toes,
    Tail,
}

/// Default colours: left blue, right green, the centre yellow, the head
/// light blue, the centre of mass blue.
pub const LEFT_COLOR: [u8; 3] = [28, 28, 177];
pub const RIGHT_COLOR: [u8; 3] = [6, 134, 6];
pub const CENTRE_COLOR: [u8; 3] = [225, 198, 87];
pub const HEAD_COLOR: [u8; 3] = [134, 171, 226];
pub const ROOT_COLOR: [u8; 3] = [8, 8, 200];

/// One part of the layout.
#[derive(Debug, Clone, PartialEq)]
pub struct PartSpec {
    /// The name after the root name ("Pelvis", "L UpperArm", ...).
    pub suffix: String,
    /// The parent's suffix; None = the root (centre of mass).
    pub parent: Option<String>,
    pub side: Side,
    pub group: PartGroup,
    pub color: [u8; 3],
    /// The joint (the bone's pivot) in biped space.
    pub base: Vec3,
    /// The bone's length along its local +X.
    pub length: f32,
    /// Box size across (local Y) and through (local Z).
    pub width: f32,
    pub depth: f32,
    /// The part's frame in biped space (local +X along the bone).
    pub frame: Mat4,
    /// Where the box sits relative to the pivot, in the part's frame.
    pub offset: Vec3,
}

impl PartSpec {
    /// The part's transform relative to its parent's frame (both in biped space).
    pub fn local(&self, parent_frame: Mat4) -> Mat4 {
        parent_frame.inverse() * self.frame
    }
}

/// A frame with +X along `dir` and local Z as close to `up` as allowed.
fn frame(base: Vec3, dir: Vec3, up: Vec3) -> Mat4 {
    let x = dir.normalize_or(Vec3::Z);
    let mut z = up - x * up.dot(x);
    if z.length_squared() < 1e-8 {
        z = Vec3::NEG_Y - x * Vec3::NEG_Y.dot(x);
    }
    let z = z.normalize();
    let y = z.cross(x).normalize();
    Mat4::from_rotation_translation(Quat::from_mat3(&Mat3::from_cols(x, y, z)), base)
}

/// The suffix of a link `i` of a chain: "Spine", "Spine1", "Spine2", ...
fn link(name: &str, i: u32) -> String {
    if i == 0 {
        name.to_owned()
    } else {
        format!("{name}{i}")
    }
}

/// Height of the centre of mass above the feet, as a fraction of the height.
pub const COM_HEIGHT: f32 = 0.53;

struct Builder {
    parts: Vec<PartSpec>,
    h: f32,
}

impl Builder {
    /// Adds a part from `base` to `tip` (heights measured from the feet).
    #[allow(clippy::too_many_arguments)]
    fn part(&mut self, suffix: String, parent: Option<String>, side: Side, group: PartGroup, base: Vec3, tip: Vec3, up: Vec3, width: f32, depth: f32, offset: Vec3) {
        let lift = Vec3::Z * (COM_HEIGHT * self.h);
        let (base, tip) = (base - lift, tip - lift);
        let color = match (group, side) {
            (PartGroup::Head, _) => HEAD_COLOR,
            (_, Side::Left) => LEFT_COLOR,
            (_, Side::Right) => RIGHT_COLOR,
            (_, Side::Centre) => CENTRE_COLOR,
        };
        self.parts.push(PartSpec { suffix, parent, side, group, color, base, length: base.distance(tip).max(1e-4), width, depth, frame: frame(base, tip - base, up), offset });
    }

    /// A chain of `links` equal parts from `a` to `b`, then a nub when `nub`.
    #[allow(clippy::too_many_arguments)]
    fn chain(&mut self, name: &str, parent: Option<String>, side: Side, group: PartGroup, a: Vec3, b: Vec3, up: Vec3, width: f32, depth: f32, links: u32, nub: bool) -> String {
        let mut parent = parent;
        let n = links.max(1);
        for i in 0..n {
            let (p0, p1) = (a.lerp(b, i as f32 / n as f32), a.lerp(b, (i + 1) as f32 / n as f32));
            let s = link(name, i);
            self.part(s.clone(), parent.clone(), side, group, p0, p1, up, width, depth, Vec3::ZERO);
            parent = Some(s);
        }
        let last = parent.clone().expect("a chain has a link");
        if nub {
            let dir = (b - a).normalize_or(Vec3::Z);
            let len = (a.distance(b) / n as f32 * 0.1).max(self.h * 0.005);
            self.part(format!("{name}Nub"), parent, side, group, b, b + dir * len, up, width * 0.5, depth * 0.5, Vec3::ZERO);
        }
        last
    }
}

/// The parts of a biped with `s`'s structure, parents before children.
pub fn layout(s: &BipedStructure) -> Vec<PartSpec> {
    let h = s.height.get().max(MIN_HEIGHT);
    let mut b = Builder { parts: Vec::new(), h };
    let front = Vec3::NEG_Y;
    let up = Vec3::Z;
    let v = |x: f32, y: f32, z: f32| Vec3::new(x, y, z) * h;
    // Pelvis: a wide box centred on the centre of mass, pointing up.
    let pelvis_len = 0.06 * h;
    b.part("Pelvis".into(), None, Side::Centre, PartGroup::Centre, v(0.0, 0.0, COM_HEIGHT), v(0.0, 0.0, COM_HEIGHT + 0.06), front, 0.18 * h, 0.10 * h, Vec3::new(-pelvis_len * 0.5, 0.0, 0.0));
    let spine_top = 0.80;
    let spine = b.chain("Spine", Some("Pelvis".into()), Side::Centre, PartGroup::Spine, v(0.0, 0.0, COM_HEIGHT + 0.03), v(0.0, 0.0, spine_top), front, 0.16 * h, 0.10 * h, s.spine_links.get(), false);
    let neck = b.chain("Neck", Some(spine), Side::Centre, PartGroup::Neck, v(0.0, 0.0, spine_top), v(0.0, 0.0, 0.87), front, 0.05 * h, 0.05 * h, s.neck_links.get(), false);
    b.chain("Head", Some(neck.clone()), Side::Centre, PartGroup::Head, v(0.0, 0.0, 0.87), v(0.0, 0.0, 1.0), front, 0.10 * h, 0.12 * h, 1, true);
    for (side, sx, tag) in [(Side::Left, 1.0f32, "L"), (Side::Right, -1.0, "R")] {
        if s.arms.get() {
            let shoulder = v(sx * 0.10, 0.0, 0.80);
            b.part(format!("{tag} Clavicle"), Some("Neck".into()), side, PartGroup::Arm, v(sx * 0.01, 0.0, 0.79), shoulder, up, 0.03 * h, 0.03 * h, Vec3::ZERO);
            let elbow = v(sx * 0.27, 0.0, 0.80);
            let wrist = v(sx * 0.42, 0.0, 0.80);
            let knuckles = v(sx * 0.48, 0.0, 0.80);
            b.part(format!("{tag} UpperArm"), Some(format!("{tag} Clavicle")), side, PartGroup::Arm, shoulder, elbow, up, 0.05 * h, 0.05 * h, Vec3::ZERO);
            b.part(format!("{tag} Forearm"), Some(format!("{tag} UpperArm")), side, PartGroup::Arm, elbow, wrist, up, 0.04 * h, 0.04 * h, Vec3::ZERO);
            let hand_w = 0.06 * h;
            b.part(format!("{tag} Hand"), Some(format!("{tag} Forearm")), side, PartGroup::Arm, wrist, knuckles, up, hand_w, 0.02 * h, Vec3::ZERO);
            let n = s.fingers.get();
            let out = Vec3::X * sx;
            for i in 0..n {
                // One finger: a mitten across the hand. More: Finger0 is the thumb, the rest share the hand's width.
                let (a, dir, w) = if n == 1 {
                    (knuckles, out, hand_w * 0.9)
                } else if i == 0 {
                    (wrist + out * (0.02 * h) + front * (hand_w * 0.5), (out + front).normalize(), 0.015 * h)
                } else {
                    let k = n - 1;
                    let w = hand_w / k as f32;
                    let y = -hand_w * 0.5 + w * (i as f32 - 0.5);
                    (knuckles + Vec3::Y * y, out, w * 0.8)
                };
                let len = if n > 1 && i == 0 { 0.05 * h } else { 0.08 * h };
                b.chain(&format!("{tag} Finger{i}"), Some(format!("{tag} Hand")), side, PartGroup::Fingers, a, a + dir * len, up, w, 0.015 * h, s.finger_links.get(), true);
            }
        }
        // Legs: thigh, calf (and a horse link with four links), foot, toes.
        let hip = v(sx * 0.055, 0.0, COM_HEIGHT - 0.03);
        let ankle_h = 0.045;
        let ankle = v(sx * 0.055, 0.0, ankle_h);
        let thigh = format!("{tag} Thigh");
        let calf = format!("{tag} Calf");
        if s.leg_links.get() >= 4 {
            let knee = v(sx * 0.055, 0.0, 0.30);
            let hock = v(sx * 0.055, 0.0, 0.15);
            b.part(thigh.clone(), Some("Pelvis".into()), side, PartGroup::Leg, hip, knee, front, 0.07 * h, 0.07 * h, Vec3::ZERO);
            b.part(calf.clone(), Some(thigh), side, PartGroup::Leg, knee, hock, front, 0.055 * h, 0.055 * h, Vec3::ZERO);
            b.part(format!("{tag} HorseLink"), Some(calf), side, PartGroup::Leg, hock, ankle, front, 0.05 * h, 0.05 * h, Vec3::ZERO);
        } else {
            let knee = v(sx * 0.055, 0.0, 0.27);
            b.part(thigh.clone(), Some("Pelvis".into()), side, PartGroup::Leg, hip, knee, front, 0.07 * h, 0.07 * h, Vec3::ZERO);
            b.part(calf, Some(thigh), side, PartGroup::Leg, knee, ankle, front, 0.055 * h, 0.055 * h, Vec3::ZERO);
        }
        let leg_end = if s.leg_links.get() >= 4 { format!("{tag} HorseLink") } else { format!("{tag} Calf") };
        // Foot: pivot at the ankle, pointing forward; the box reaches the ground and
        // starts behind the ankle (the ankle sits a fifth of the way from the heel).
        let foot_len = 0.11 * h;
        let foot_w = 0.06 * h;
        let foot = format!("{tag} Foot");
        b.part(foot.clone(), Some(leg_end), side, PartGroup::Leg, ankle, ankle + front * foot_len, up, foot_w, ankle_h * h, Vec3::new(-0.2 * foot_len, 0.0, -0.5 * ankle_h * h));
        let ball = v(sx * 0.055, 0.0, 0.0125) + front * (0.8 * foot_len);
        let n = s.toes.get();
        for i in 0..n {
            let w = foot_w / n as f32;
            let x = -foot_w * 0.5 + w * (i as f32 + 0.5);
            let a = ball + Vec3::X * x;
            b.chain(&format!("{tag} Toe{i}"), Some(foot.clone()), side, PartGroup::Toes, a, a + front * (0.045 * h), up, w * 0.85, 0.025 * h, s.toe_links.get(), true);
        }
    }
    if s.tail_links.get() > 0 {
        let a = v(0.0, 0.05, COM_HEIGHT - 0.01);
        let b2 = a + Vec3::new(0.0, 0.7, -0.7).normalize() * (0.3 * h);
        b.chain("Tail", Some("Pelvis".into()), Side::Centre, PartGroup::Tail, a, b2, up, 0.04 * h, 0.04 * h, s.tail_links.get(), true);
    }
    b.parts
}

/// The frame of each part in biped space, by suffix.
pub fn frames(parts: &[PartSpec]) -> std::collections::HashMap<String, Mat4> {
    parts.iter().map(|p| (p.suffix.clone(), p.frame)).collect()
}

/// The display of the centre of mass: an octahedron this big for a biped of `height`.
pub fn root_size(height: f32) -> f32 {
    height * 0.03
}
