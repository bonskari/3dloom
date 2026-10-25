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

/// Structure > Body Type: the overall look of the parts.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum BodyType {
    /// Thin, tapered bones that fit inside a character mesh.
    #[default]
    Skeleton,
    /// A silhouette with male proportions (broad shoulders, narrow hips).
    Male,
    /// A silhouette with female proportions (narrow shoulders, wide hips).
    Female,
    /// The original boxes.
    Classic,
}

impl BodyType {
    /// In the order of the reference's drop-down list.
    pub const ALL: [BodyType; 4] = [BodyType::Skeleton, BodyType::Male, BodyType::Female, BodyType::Classic];

    pub fn label(self) -> &'static str {
        match self {
            BodyType::Skeleton => "Skeleton",
            BodyType::Male => "Male",
            BodyType::Female => "Female",
            BodyType::Classic => "Classic",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|b| b.label().eq_ignore_ascii_case(name.trim()))
    }

    pub fn index(self) -> u32 {
        Self::ALL.iter().position(|b| *b == self).unwrap_or(0) as u32
    }
}

/// Structure > Xtras: an extra tail, animated with forward kinematics like a
/// ponytail but attached to any part.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Xtra {
    /// The tail's name after the root name; its links are `name`, `name1`, ... and `nameNub`.
    pub name: String,
    pub links: u32,
    /// The parent part's name after the root name; None = the centre of mass.
    pub parent: Option<String>,
    /// Where the tail starts, in biped space measured from the feet, in heights.
    pub base: Vec3,
    /// The direction the tail points in, in biped space.
    pub dir: Vec3,
    /// The tail made from this one by Create Opposite Xtra (or the one this was made from).
    #[serde(default)]
    pub opposite: Option<String>,
}

/// Length of an extra tail, in heights.
pub const XTRA_LENGTH: f32 = 0.25;

/// The Structure rollout.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct BipedStructure {
    pub body_type: BodyType,
    pub arms: Param<bool>,
    pub neck_links: Param<u32>,
    pub spine_links: Param<u32>,
    pub leg_links: Param<u32>,
    pub tail_links: Param<u32>,
    pub ponytail1_links: Param<u32>,
    pub ponytail2_links: Param<u32>,
    pub fingers: Param<u32>,
    pub finger_links: Param<u32>,
    pub toes: Param<u32>,
    pub toe_links: Param<u32>,
    pub prop1: Param<bool>,
    pub prop2: Param<bool>,
    pub prop3: Param<bool>,
    /// Where the ankle meets the foot block: 0 the heel, 1 the toes.
    pub ankle_attach: Param<f32>,
    pub height: Param<f32>,
    /// The thighs hang from the lowest spine link instead of the pelvis.
    pub triangle_pelvis: Param<bool>,
    /// The clavicles hang from the top spine link instead of the neck.
    pub triangle_neck: Param<bool>,
    /// The hands and fingers behave as feet and toes (a quadruped).
    pub forefeet: Param<bool>,
    /// An anatomical hand: a small base and a bone per digit.
    pub knuckles: Param<bool>,
    /// With Knuckles: the thumb has one link less than the fingers.
    pub short_thumb: Param<bool>,
    /// Twist Links: frozen links along the limbs for skin deformation.
    pub twists: Param<bool>,
    pub upper_arm_twists: Param<u32>,
    pub forearm_twists: Param<u32>,
    pub thigh_twists: Param<u32>,
    pub calf_twists: Param<u32>,
    pub horse_link_twists: Param<u32>,
    pub xtras: Vec<Xtra>,
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

impl Default for BipedStructure {
    fn default() -> Self {
        Self::new(DEFAULT_HEIGHT_METRES)
    }
}

impl BipedStructure {
    /// The reference defaults: Skeleton, Neck Links 1, Spine Links 4, Leg
    /// Links 3, Tail Links 0, Ponytails 0, Fingers 1, Finger Links 1, Toes 1,
    /// Toe Links 3, arms on, no props, Ankle Attach 0.2, no twist links.
    pub fn new(height: f32) -> Self {
        Self {
            body_type: BodyType::Skeleton,
            arms: Param::new(true),
            neck_links: Param::new(1),
            spine_links: Param::new(4),
            leg_links: Param::new(3),
            tail_links: Param::new(0),
            ponytail1_links: Param::new(0),
            ponytail2_links: Param::new(0),
            fingers: Param::new(1),
            finger_links: Param::new(1),
            toes: Param::new(1),
            toe_links: Param::new(3),
            prop1: Param::new(false),
            prop2: Param::new(false),
            prop3: Param::new(false),
            ankle_attach: Param::new(0.2),
            height: Param::new(height.max(MIN_HEIGHT)),
            triangle_pelvis: Param::new(false),
            triangle_neck: Param::new(false),
            forefeet: Param::new(false),
            knuckles: Param::new(false),
            short_thumb: Param::new(true),
            twists: Param::new(false),
            upper_arm_twists: Param::new(0),
            forearm_twists: Param::new(0),
            thigh_twists: Param::new(0),
            calf_twists: Param::new(0),
            horse_link_twists: Param::new(0),
            xtras: Vec::new(),
            figure_mode: true,
            figure_pose: Vec::new(),
            anim_pose: Vec::new(),
        }
    }

    /// Structure parameter names with their ranges (name, min, max).
    pub const LINKS: [(&'static str, u32, u32); 10] = [
        ("Neck Links", 1, 25),
        ("Spine Links", 1, 10),
        ("Leg Links", 3, 4),
        ("Tail Links", 0, 25),
        ("Ponytail1 Links", 0, 25),
        ("Ponytail2 Links", 0, 25),
        ("Fingers", 0, 5),
        ("Finger Links", 1, 4),
        ("Toes", 1, 5),
        ("Toe Links", 1, 3),
    ];

    /// Twist Links group: twist links per limb (name, min, max).
    pub const TWISTS: [(&'static str, u32, u32); 5] = [("Upper Arm", 0, 10), ("Forearm", 0, 10), ("Thigh", 0, 10), ("Calf", 0, 10), ("Horse Link", 0, 10)];

    /// On/off parameters by their rollout names.
    pub const FLAGS: [&'static str; 10] = ["Arms", "Prop1", "Prop2", "Prop3", "Triangle Pelvis", "Triangle Neck", "ForeFeet", "Knuckles", "Short Thumb", "Twists"];

    pub fn count_mut(&mut self, name: &str) -> Option<&mut Param<u32>> {
        Some(match name {
            "Neck Links" => &mut self.neck_links,
            "Spine Links" => &mut self.spine_links,
            "Leg Links" => &mut self.leg_links,
            "Tail Links" => &mut self.tail_links,
            "Ponytail1 Links" => &mut self.ponytail1_links,
            "Ponytail2 Links" => &mut self.ponytail2_links,
            "Fingers" => &mut self.fingers,
            "Finger Links" => &mut self.finger_links,
            "Toes" => &mut self.toes,
            "Toe Links" => &mut self.toe_links,
            "Upper Arm" => &mut self.upper_arm_twists,
            "Forearm" => &mut self.forearm_twists,
            "Thigh" => &mut self.thigh_twists,
            "Calf" => &mut self.calf_twists,
            "Horse Link" => &mut self.horse_link_twists,
            _ => return None,
        })
    }

    pub fn count(&self, name: &str) -> Option<u32> {
        self.clone().count_mut(name).map(|p| p.get())
    }

    pub fn flag_mut(&mut self, name: &str) -> Option<&mut Param<bool>> {
        Some(match name {
            "Arms" => &mut self.arms,
            "Prop1" => &mut self.prop1,
            "Prop2" => &mut self.prop2,
            "Prop3" => &mut self.prop3,
            "Triangle Pelvis" => &mut self.triangle_pelvis,
            "Triangle Neck" => &mut self.triangle_neck,
            "ForeFeet" => &mut self.forefeet,
            "Knuckles" => &mut self.knuckles,
            "Short Thumb" => &mut self.short_thumb,
            "Twists" => &mut self.twists,
            _ => return None,
        })
    }

    pub fn flag(&self, name: &str) -> Option<bool> {
        self.clone().flag_mut(name).map(|p| p.get())
    }

    /// Sets a structure parameter by its rollout name, clamped to its range.
    /// Turning Knuckles on sets Fingers 5, Finger Links 4 and Short Thumb on;
    /// Knuckles and ForeFeet exclude each other.
    pub fn set(&mut self, name: &str, value: f64) -> Result<(), String> {
        match name {
            "Height" => self.height.set((value as f32).max(MIN_HEIGHT)),
            "Ankle Attach" => self.ankle_attach.set((value as f32).clamp(0.0, 1.0)),
            "Body Type" => self.body_type = *BodyType::ALL.get(value.round().max(0.0) as usize).ok_or_else(|| format!("Body Type is 0-3, not {value}"))?,
            "Knuckles" => {
                let on = value != 0.0;
                if on && self.forefeet.get() {
                    return Err("Knuckles is disabled while ForeFeet is on".into());
                }
                if on && !self.knuckles.get() {
                    self.fingers.set(5);
                    self.finger_links.set(4);
                    self.short_thumb.set(true);
                }
                self.knuckles.set(on);
            }
            "ForeFeet" => {
                let on = value != 0.0;
                if on && self.knuckles.get() {
                    return Err("ForeFeet is disabled while Knuckles is on".into());
                }
                self.forefeet.set(on);
            }
            _ if Self::FLAGS.contains(&name) => self.flag_mut(name).expect("listed").set(value != 0.0),
            _ => {
                let (_, lo, hi) = Self::LINKS.iter().chain(Self::TWISTS.iter()).find(|(n, _, _)| *n == name).copied().ok_or_else(|| format!("Biped has no parameter {name:?}"))?;
                let v = (value.round().max(0.0) as u32).clamp(lo, hi);
                self.count_mut(name).expect("listed").set(v);
            }
        }
        Ok(())
    }

    /// The twist links of a limb that the layout makes (Twists off: none;
    /// Horse Link only with four leg links).
    pub fn twist_links(&self, limb: &str) -> u32 {
        if !self.twists.get() || (limb == "Horse Link" && self.leg_links.get() < 4) {
            return 0;
        }
        self.count(limb).unwrap_or(0)
    }

    /// What decides each group's parts (a group is laid out again when its key changes).
    fn group_key(&self, group: PartGroup) -> Vec<u32> {
        let arms = self.arms.get() as u32;
        let body = self.body_type.index();
        let ankle = self.ankle_attach.get().to_bits();
        match group {
            PartGroup::Centre => vec![body],
            PartGroup::Spine => vec![body, self.spine_links.get()],
            PartGroup::Neck => vec![body, self.neck_links.get()],
            PartGroup::Head => vec![body],
            PartGroup::Arm => vec![body, arms, self.knuckles.get() as u32],
            PartGroup::Fingers => vec![body, arms, self.fingers.get(), self.finger_links.get(), self.knuckles.get() as u32, self.short_thumb.get() as u32],
            PartGroup::Leg => vec![body, self.leg_links.get(), ankle],
            PartGroup::Toes => vec![body, self.leg_links.get(), self.toes.get(), self.toe_links.get(), ankle],
            PartGroup::Tail => vec![body, self.tail_links.get()],
            PartGroup::Ponytail => vec![body, self.ponytail1_links.get(), self.ponytail2_links.get()],
            PartGroup::Prop => vec![body, arms, self.prop1.get() as u32, self.prop2.get() as u32, self.prop3.get() as u32, self.knuckles.get() as u32],
            PartGroup::Twist => {
                let mut v = vec![body, arms, self.leg_links.get(), self.twists.get() as u32];
                v.extend(Self::TWISTS.iter().map(|(n, _, _)| self.twist_links(n)));
                v
            }
            PartGroup::Xtra => vec![],
        }
    }

    /// True when `self` and `other` lay out `group` differently.
    pub fn group_changed(&self, other: &Self, group: PartGroup) -> bool {
        if group == PartGroup::Xtra && self.xtras != other.xtras {
            return true;
        }
        self.group_key(group) != other.group_key(group)
    }

    /// The next free extra tail name: Xtra01, Xtra02, ...
    pub fn next_xtra_name(&self) -> String {
        (1..).map(|i| format!("Xtra{i:02}")).find(|n| !self.xtras.iter().any(|x| &x.name == n)).expect("a free name")
    }

    /// The extra tail a part belongs to (its links and nub).
    pub fn xtra_of_part(&self, suffix: &str) -> Option<&Xtra> {
        self.xtras.iter().filter(|x| suffix.starts_with(&x.name)).max_by_key(|x| x.name.len()).filter(|x| {
            let rest = &suffix[x.name.len()..];
            rest.is_empty() || rest == "Nub" || rest.parse::<u32>().is_ok()
        })
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
    Ponytail,
    Prop,
    Twist,
    Xtra,
}

/// Default colours: left blue, right green, the centre yellow, the head
/// light blue, the centre of mass blue.
pub const LEFT_COLOR: [u8; 3] = [28, 28, 177];
pub const RIGHT_COLOR: [u8; 3] = [6, 134, 6];
pub const CENTRE_COLOR: [u8; 3] = [225, 198, 87];
pub const HEAD_COLOR: [u8; 3] = [134, 171, 226];
pub const ROOT_COLOR: [u8; 3] = [8, 8, 200];
/// Props are drawn in a neutral grey.
pub const PROP_COLOR: [u8; 3] = [140, 140, 140];

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
    /// How much the far end shrinks (the bone's Taper, 0-1).
    pub taper: f32,
    /// The part's frame in biped space (local +X along the bone).
    pub frame: Mat4,
    /// Where the box sits relative to the pivot, in the part's frame.
    pub offset: Vec3,
    /// Twist links are created frozen.
    pub frozen: bool,
}

impl PartSpec {
    /// The part's transform relative to its parent's frame (both in biped space).
    pub fn local(&self, parent_frame: Mat4) -> Mat4 {
        parent_frame.inverse() * self.frame
    }

    /// The bone's far end in biped space.
    pub fn tip(&self) -> Vec3 {
        self.frame.transform_point3(Vec3::X * self.length)
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

/// Sizes that differ between body types, as fractions of the height.
struct Proportions {
    /// Limb thickness relative to the classic boxes.
    limb: f32,
    /// Limb taper.
    taper: f32,
    /// Half the shoulder width.
    shoulder: f32,
    /// Half the hip width.
    hip: f32,
    /// Spine width at the bottom and the top link.
    spine: (f32, f32),
    spine_depth: f32,
    pelvis: f32,
    head: (f32, f32),
    /// Foot block thickness relative to the ankle height.
    foot: f32,
}

fn proportions(b: BodyType) -> Proportions {
    match b {
        BodyType::Classic => Proportions { limb: 1.0, taper: 0.0, shoulder: 0.10, hip: 0.055, spine: (0.16, 0.16), spine_depth: 0.10, pelvis: 0.18, head: (0.10, 0.12), foot: 1.0 },
        BodyType::Skeleton => Proportions { limb: 0.45, taper: 0.4, shoulder: 0.10, hip: 0.055, spine: (0.06, 0.13), spine_depth: 0.07, pelvis: 0.16, head: (0.09, 0.11), foot: 0.5 },
        BodyType::Male => Proportions { limb: 1.1, taper: 0.3, shoulder: 0.12, hip: 0.052, spine: (0.14, 0.22), spine_depth: 0.11, pelvis: 0.16, head: (0.095, 0.115), foot: 1.0 },
        BodyType::Female => Proportions { limb: 0.85, taper: 0.3, shoulder: 0.092, hip: 0.064, spine: (0.17, 0.14), spine_depth: 0.09, pelvis: 0.21, head: (0.09, 0.11), foot: 0.9 },
    }
}

struct Builder {
    parts: Vec<PartSpec>,
    h: f32,
}

impl Builder {
    /// Adds a part from `base` to `tip` (heights measured from the feet).
    #[allow(clippy::too_many_arguments)]
    fn part(&mut self, suffix: String, parent: Option<String>, side: Side, group: PartGroup, base: Vec3, tip: Vec3, up: Vec3, width: f32, depth: f32, offset: Vec3) -> &mut PartSpec {
        let lift = Vec3::Z * (COM_HEIGHT * self.h);
        let (base, tip) = (base - lift, tip - lift);
        let color = match (group, side) {
            (PartGroup::Head | PartGroup::Ponytail, _) => HEAD_COLOR,
            (PartGroup::Prop, _) => PROP_COLOR,
            (_, Side::Left) => LEFT_COLOR,
            (_, Side::Right) => RIGHT_COLOR,
            (_, Side::Centre) => CENTRE_COLOR,
        };
        self.parts.push(PartSpec { suffix, parent, side, group, color, base, length: base.distance(tip).max(1e-4), width, depth, taper: 0.0, frame: frame(base, tip - base, up), offset, frozen: false });
        self.parts.last_mut().expect("pushed")
    }

    /// A chain of `links` equal parts from `a` to `b`, then a nub when `nub`.
    #[allow(clippy::too_many_arguments)]
    fn chain(&mut self, name: &str, parent: Option<String>, side: Side, group: PartGroup, a: Vec3, b: Vec3, up: Vec3, width: f32, depth: f32, links: u32, nub: bool) -> String {
        self.chain_widths(name, parent, side, group, a, b, up, (width, width), depth, links, nub)
    }

    /// `chain` with the width going from `widths.0` at the first link to `widths.1` at the last.
    #[allow(clippy::too_many_arguments)]
    fn chain_widths(&mut self, name: &str, parent: Option<String>, side: Side, group: PartGroup, a: Vec3, b: Vec3, up: Vec3, widths: (f32, f32), depth: f32, links: u32, nub: bool) -> String {
        let mut parent = parent;
        let n = links.max(1);
        for i in 0..n {
            let (p0, p1) = (a.lerp(b, i as f32 / n as f32), a.lerp(b, (i + 1) as f32 / n as f32));
            let s = link(name, i);
            let w = if n == 1 { widths.0 } else { widths.0 + (widths.1 - widths.0) * i as f32 / (n - 1) as f32 };
            self.part(s.clone(), parent.clone(), side, group, p0, p1, up, w, depth, Vec3::ZERO);
            parent = Some(s);
        }
        let last = parent.clone().expect("a chain has a link");
        if nub {
            let dir = (b - a).normalize_or(Vec3::Z);
            let len = (a.distance(b) / n as f32 * 0.1).max(self.h * 0.005);
            self.part(format!("{name}Nub"), parent, side, group, b, b + dir * len, up, widths.1 * 0.5, depth * 0.5, Vec3::ZERO);
        }
        last
    }

    fn has(&self, suffix: &str) -> bool {
        self.parts.iter().any(|p| p.suffix == suffix)
    }

    /// A part's base and tip measured from the feet.
    fn ends(&self, suffix: &str) -> Option<(Vec3, Vec3)> {
        let lift = Vec3::Z * (COM_HEIGHT * self.h);
        self.parts.iter().find(|p| p.suffix == suffix).map(|p| (p.base + lift, p.tip() + lift))
    }
}

/// The parts of a biped with `s`'s structure, parents before children.
pub fn layout(s: &BipedStructure) -> Vec<PartSpec> {
    let h = s.height.get().max(MIN_HEIGHT);
    let pr = proportions(s.body_type);
    let mut b = Builder { parts: Vec::new(), h };
    let front = Vec3::NEG_Y;
    let up = Vec3::Z;
    let v = |x: f32, y: f32, z: f32| Vec3::new(x, y, z) * h;
    let limb = |w: f32| w * pr.limb * h;
    // Pelvis: a wide box centred on the centre of mass, pointing up.
    let pelvis_len = 0.06 * h;
    b.part("Pelvis".into(), None, Side::Centre, PartGroup::Centre, v(0.0, 0.0, COM_HEIGHT), v(0.0, 0.0, COM_HEIGHT + 0.06), front, pr.pelvis * h, 0.10 * h, Vec3::new(-pelvis_len * 0.5, 0.0, 0.0));
    let spine_top = 0.80;
    let spine = b.chain_widths("Spine", Some("Pelvis".into()), Side::Centre, PartGroup::Spine, v(0.0, 0.0, COM_HEIGHT + 0.03), v(0.0, 0.0, spine_top), front, (pr.spine.0 * h, pr.spine.1 * h), pr.spine_depth * h, s.spine_links.get(), false);
    let neck = b.chain("Neck", Some(spine.clone()), Side::Centre, PartGroup::Neck, v(0.0, 0.0, spine_top), v(0.0, 0.0, 0.87), front, 0.05 * h * pr.limb.max(0.7), 0.05 * h * pr.limb.max(0.7), s.neck_links.get(), false);
    if pr.taper > 0.0 {
        for p in b.parts.iter_mut().filter(|p| p.group == PartGroup::Neck) {
            p.taper = pr.taper * 0.5;
        }
    }
    b.chain("Head", Some(neck.clone()), Side::Centre, PartGroup::Head, v(0.0, 0.0, 0.87), v(0.0, 0.0, 1.0), front, pr.head.0 * h, pr.head.1 * h, 1, true);
    // Ponytails hang from the back of the head.
    for (i, (links, x)) in [(s.ponytail1_links.get(), 0.025f32), (s.ponytail2_links.get(), -0.025)].into_iter().enumerate() {
        if links > 0 {
            let a = v(x, 0.05, 0.95);
            let tip = a + Vec3::new(0.0, 0.6, -0.8).normalize() * (0.2 * h);
            b.chain(&format!("Ponytail{}", i + 1), Some("Head".into()), Side::Centre, PartGroup::Ponytail, a, tip, front, 0.025 * h, 0.025 * h, links, true);
        }
    }
    let clavicle_parent = if s.triangle_neck.get() { spine.clone() } else { "Neck".to_owned() };
    let thigh_parent = if s.triangle_pelvis.get() { "Spine".to_owned() } else { "Pelvis".to_owned() };
    for (side, sx, tag) in [(Side::Left, 1.0f32, "L"), (Side::Right, -1.0, "R")] {
        if s.arms.get() {
            let shoulder = v(sx * pr.shoulder, 0.0, 0.80);
            b.part(format!("{tag} Clavicle"), Some(clavicle_parent.clone()), side, PartGroup::Arm, v(sx * 0.01, 0.0, 0.79), shoulder, up, limb(0.03), limb(0.03), Vec3::ZERO).taper = pr.taper * 0.5;
            let elbow = v(sx * (pr.shoulder + 0.17), 0.0, 0.80);
            let wrist = v(sx * (pr.shoulder + 0.32), 0.0, 0.80);
            let knuckles = wrist + Vec3::X * (sx * 0.06 * h);
            b.part(format!("{tag} UpperArm"), Some(format!("{tag} Clavicle")), side, PartGroup::Arm, shoulder, elbow, up, limb(0.05), limb(0.05), Vec3::ZERO).taper = pr.taper;
            b.part(format!("{tag} Forearm"), Some(format!("{tag} UpperArm")), side, PartGroup::Arm, elbow, wrist, up, limb(0.04), limb(0.04), Vec3::ZERO).taper = pr.taper;
            let hand_w = 0.06 * h;
            let out = Vec3::X * sx;
            if s.knuckles.get() {
                // An anatomical hand: a small base, each digit from the wrist.
                let base_end = wrist + out * (0.02 * h);
                b.part(format!("{tag} Hand"), Some(format!("{tag} Forearm")), side, PartGroup::Arm, wrist, base_end, up, hand_w * 0.6, 0.02 * h, Vec3::ZERO);
                let n = s.fingers.get();
                for i in 0..n {
                    let (a, dir, len, w) = if i == 0 {
                        (wrist + out * (0.01 * h) + front * (hand_w * 0.35), (out + front).normalize(), 0.07 * h, 0.014 * h)
                    } else {
                        let k = (n - 1).max(1);
                        let w = hand_w / k as f32;
                        let y = -hand_w * 0.5 + w * (i as f32 - 0.5);
                        (base_end + Vec3::Y * y, out, 0.12 * h, w * 0.7)
                    };
                    let links = if i == 0 && s.short_thumb.get() && n > 1 { s.finger_links.get().saturating_sub(1).max(1) } else { s.finger_links.get() };
                    b.chain(&format!("{tag} Finger{i}"), Some(format!("{tag} Hand")), side, PartGroup::Fingers, a, a + dir * len, up, w, 0.014 * h, links, true);
                }
            } else {
                b.part(format!("{tag} Hand"), Some(format!("{tag} Forearm")), side, PartGroup::Arm, wrist, knuckles, up, hand_w, 0.02 * h, Vec3::ZERO);
                let n = s.fingers.get();
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
        }
        // Legs: thigh, calf (and a horse link with four links), foot, toes.
        let hip = v(sx * pr.hip, 0.0, COM_HEIGHT - 0.03);
        let ankle_h = 0.045;
        let ankle = v(sx * pr.hip, 0.0, ankle_h);
        let thigh = format!("{tag} Thigh");
        let calf = format!("{tag} Calf");
        if s.leg_links.get() >= 4 {
            let knee = v(sx * pr.hip, 0.0, 0.30);
            let hock = v(sx * pr.hip, 0.0, 0.15);
            b.part(thigh.clone(), Some(thigh_parent.clone()), side, PartGroup::Leg, hip, knee, front, limb(0.07), limb(0.07), Vec3::ZERO).taper = pr.taper;
            b.part(calf.clone(), Some(thigh), side, PartGroup::Leg, knee, hock, front, limb(0.055), limb(0.055), Vec3::ZERO).taper = pr.taper;
            b.part(format!("{tag} HorseLink"), Some(calf), side, PartGroup::Leg, hock, ankle, front, limb(0.05), limb(0.05), Vec3::ZERO).taper = pr.taper;
        } else {
            let knee = v(sx * pr.hip, 0.0, 0.27);
            b.part(thigh.clone(), Some(thigh_parent.clone()), side, PartGroup::Leg, hip, knee, front, limb(0.07), limb(0.07), Vec3::ZERO).taper = pr.taper;
            b.part(calf, Some(thigh), side, PartGroup::Leg, knee, ankle, front, limb(0.055), limb(0.055), Vec3::ZERO).taper = pr.taper;
        }
        let leg_end = if s.leg_links.get() >= 4 { format!("{tag} HorseLink") } else { format!("{tag} Calf") };
        // Foot: pivot at the ankle, pointing forward; the block reaches the
        // ground and the ankle sits Ankle Attach of the way from the heel.
        let foot_len = 0.11 * h;
        let foot_w = 0.06 * h * pr.limb.clamp(0.7, 1.0);
        let attach = s.ankle_attach.get().clamp(0.0, 1.0);
        let foot_d = pr.foot * ankle_h * h;
        let foot = format!("{tag} Foot");
        b.part(foot.clone(), Some(leg_end), side, PartGroup::Leg, ankle, ankle + front * foot_len, up, foot_w, foot_d, Vec3::new(-attach * foot_len, 0.0, foot_d * 0.5 - ankle_h * h));
        let ball = v(sx * pr.hip, 0.0, 0.0125) + front * ((1.0 - attach) * foot_len);
        let n = s.toes.get();
        for i in 0..n {
            let w = foot_w / n as f32;
            let x = -foot_w * 0.5 + w * (i as f32 + 0.5);
            let a = ball + Vec3::X * x;
            b.chain(&format!("{tag} Toe{i}"), Some(foot.clone()), side, PartGroup::Toes, a, a + front * (0.045 * h), up, w * 0.85, 0.025 * h * pr.foot.max(0.6), s.toe_links.get(), true);
        }
    }
    if s.tail_links.get() > 0 {
        let a = v(0.0, 0.05, COM_HEIGHT - 0.01);
        let b2 = a + Vec3::new(0.0, 0.7, -0.7).normalize() * (0.3 * h);
        b.chain("Tail", Some("Pelvis".into()), Side::Centre, PartGroup::Tail, a, b2, up, 0.04 * h, 0.04 * h, s.tail_links.get(), true);
    }
    // Twist links: frozen links along each limb, inside it.
    for (side, tag) in [(Side::Left, "L"), (Side::Right, "R")] {
        for (limb_name, part, name) in [("Upper Arm", "UpperArm", "UpperArmTwist"), ("Forearm", "Forearm", "ForeTwist"), ("Thigh", "Thigh", "ThighTwist"), ("Calf", "Calf", "CalfTwist"), ("Horse Link", "HorseLink", "HorseTwist")] {
            let n = s.twist_links(limb_name);
            let part = format!("{tag} {part}");
            let Some((a, tip)) = b.ends(&part).filter(|_| n > 0) else { continue };
            let spec = b.parts.iter().find(|p| p.suffix == part).expect("found").clone();
            let up = spec.frame.z_axis.truncate();
            let first = b.parts.len();
            b.chain(&format!("{tag} {name}"), Some(part), side, PartGroup::Twist, a, tip, up, spec.width * 0.7, spec.depth * 0.7, n, false);
            for p in &mut b.parts[first..] {
                p.frozen = true;
            }
        }
    }
    // Props: 1 by the right hand, 2 by the left hand, 3 in front of the torso.
    for (i, on) in [s.prop1.get(), s.prop2.get(), s.prop3.get()].into_iter().enumerate() {
        if !on {
            continue;
        }
        let hand = match i {
            0 => Some("R Hand"),
            1 => Some("L Hand"),
            _ => None,
        };
        let (parent, a) = match hand.and_then(|n| b.ends(n).map(|e| (n, e))) {
            Some((n, (_, tip))) => (Some(n.to_owned()), tip + front * (0.04 * h)),
            None if i < 2 => (None, v(if i == 0 { -0.45 } else { 0.45 }, -0.04, 0.80)),
            None => (None, v(0.0, -0.15, 0.62)),
        };
        let side = [Side::Right, Side::Left, Side::Centre][i];
        b.part(format!("Prop{}", i + 1), parent, side, PartGroup::Prop, a, a + up * (0.2 * h), front, 0.025 * h, 0.025 * h, Vec3::ZERO);
    }
    // Extra tails, each after its parent.
    let mut pending: Vec<&Xtra> = s.xtras.iter().collect();
    while !pending.is_empty() {
        let ready = pending.iter().position(|x| x.parent.as_ref().is_none_or(|p| b.has(p) || !s.xtras.iter().any(|o| !b.has(&o.name) && p.starts_with(&o.name))));
        let x = pending.remove(ready.unwrap_or(0));
        let parent = x.parent.clone().filter(|p| b.has(p));
        let a = x.base * h;
        let tip = a + x.dir.normalize_or(Vec3::Z) * (XTRA_LENGTH * h);
        let side = if x.base.x > 1e-4 { Side::Left } else if x.base.x < -1e-4 { Side::Right } else { Side::Centre };
        b.chain(&x.name, parent, side, PartGroup::Xtra, a, tip, front, 0.03 * h, 0.03 * h, x.links.max(1), true);
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
