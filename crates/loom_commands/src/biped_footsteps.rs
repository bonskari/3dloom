//! Biped Footstep Mode: footsteps (Create Multiple Footsteps for a walk, a
//! run or a jump, Create Footsteps (append)) and their activation (Create
//! Keys for Inactive Footsteps), which keys the centre of mass and the legs
//! so the biped walks over the footsteps when the time slider plays.
//!
//! The reference computes the motion with its biped dynamics. This is a
//! simpler procedural gait: a planted foot stays on its footstep, a swinging
//! foot arcs to its next footstep, the centre of mass follows the feet
//! (lowered so the legs reach the planted feet, on a ballistic arc while
//! airborne), and a two-bone IK bends each leg. It keys every frame.

use crate::biped::{parts, root_of, structure};
use crate::execute::commit_static;
use crate::state::AppState;
use crate::Command;
use glam::{Mat4, Quat, Vec3};
use loom_scene::anim::{self, TangentType};
use loom_scene::biped::{self, BipedStructure, Footstep, Gait, GaitTiming};
use loom_scene::{Change, NodeId, ObjectKind, Transform};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// The heading (degrees about Z, 0 = facing -Y) of a horizontal direction.
pub fn heading_of(dir: Vec3) -> f32 {
    dir.x.atan2(-dir.y).to_degrees()
}

/// The horizontal direction a heading faces.
pub fn heading_dir(deg: f32) -> Vec3 {
    let r = deg.to_radians();
    Vec3::new(r.sin(), -r.cos(), 0.0)
}

fn replace_structure(state: &mut AppState, root: NodeId, s: BipedStructure, label: &str) -> Result<(), String> {
    let node = state.scene.node(root).ok_or("no biped")?;
    let change = Change::ReplaceObject { id: root, before: Box::new(node.object.clone()), after: Box::new(ObjectKind::Biped(Box::new(s))) };
    commit_static(state, label, change)
}

fn footstep_structure(state: &AppState, root: NodeId) -> Result<BipedStructure, String> {
    let s = structure(state, root).ok_or("no biped")?.clone();
    if !s.footstep_mode {
        return Err("turn on Footstep Mode first".into());
    }
    Ok(s)
}

/// Biped rollout > Footstep Mode; turning it on leaves Figure Mode.
pub fn footstep_mode(state: &mut AppState, root: NodeId, on: Option<bool>) -> Result<Value, String> {
    let s = structure(state, root).ok_or("no biped")?.clone();
    let on = on.unwrap_or(!s.footstep_mode);
    if on == s.footstep_mode {
        return Ok(json!({ "footstep_mode": on }));
    }
    if on && s.figure_mode {
        crate::biped::figure_mode(state, root, Some(false))?;
    }
    let mut s = structure(state, root).ok_or("no biped")?.clone();
    s.footstep_mode = on;
    replace_structure(state, root, s, if on { "Footstep Mode On" } else { "Footstep Mode Off" })?;
    Ok(json!({ "footstep_mode": on }))
}

/// Where the biped stands: the ground point under the centre of mass, the
/// direction it faces and its left, flat on the ground.
fn stance(state: &AppState, root: NodeId, s: &BipedStructure) -> (Vec3, Vec3, Vec3) {
    let w = state.scene.world_matrix(root);
    let mut fwd = w.transform_vector3(Vec3::NEG_Y);
    fwd.z = 0.0;
    let fwd = fwd.normalize_or(Vec3::NEG_Y);
    let ground = w.w_axis.truncate() - Vec3::Z * (biped::COM_HEIGHT * s.height.get());
    (ground, fwd, Vec3::Z.cross(fwd))
}

/// The frames of the next footstep after `prev` (the latest footstep, on
/// either side) for `gait`; `pair` = the second foot of a jump.
fn next_timing(t: &GaitTiming, gait: Gait, prev: Option<&Footstep>, first_start: f32, pair: bool) -> (f32, f32) {
    let len = match gait {
        Gait::Walk => t.walk_footstep.max(1),
        Gait::Run => t.run_footstep.max(1),
        Gait::Jump => t.jump_two_feet_down.max(1),
    } as f32;
    let start = match prev {
        None => first_start,
        Some(p) if pair => p.start,
        Some(p) => match gait {
            Gait::Walk => p.start + (t.walk_footstep.max(1) as f32 - t.double_support.min(t.walk_footstep.saturating_sub(1)) as f32),
            Gait::Run => p.end + t.run_airborne as f32,
            Gait::Jump => p.end + t.jump_airborne as f32,
        },
    };
    (start, start + len)
}

/// Create Multiple Footsteps dialog values.
#[derive(Debug, Clone)]
pub struct Multiple {
    pub gait: Gait,
    pub count: u32,
    pub start_left: bool,
    pub alternate: bool,
    /// Parametric Stride Width: a fraction of the pelvis width.
    pub stride_width: f32,
    /// Parametric Stride Length: a fraction of the leg length.
    pub stride_length: f32,
    /// Actual Stride Height: the rise per footstep, in units.
    pub stride_height: f32,
    /// Start at Current Frame (instead of after the last footstep).
    pub at_current_frame: bool,
}

/// The leg length (hip height) and pelvis width of a biped, in units.
pub fn leg_and_pelvis(s: &BipedStructure) -> (f32, f32) {
    let h = s.height.get();
    ((biped::COM_HEIGHT - 0.03) * h, 2.0 * 0.055 * h)
}

/// Create Multiple Footsteps: `m.count` new inactive footsteps of a walk,
/// run or jump, from where the biped stands or after its last footstep.
pub fn create_multiple(state: &mut AppState, root: NodeId, m: &Multiple) -> Result<Value, String> {
    let mut s = footstep_structure(state, root)?;
    let (leg, pelvis) = leg_and_pelvis(&s);
    let stride = m.stride_length * leg;
    let width = m.stride_width * pelvis;
    let (ground, fwd, left) = stance(state, root, &s);
    let jump = m.gait == Gait::Jump;
    let alternate = m.alternate || m.gait == Gait::Walk;
    // The centre line, its direction, the first footstep's distance along it,
    // the first side and the timing before the new footsteps.
    let last = s.footsteps.last().cloned();
    let (origin, fwd, left, along0, mut side_left) = match &last {
        Some(l) => {
            let f = heading_dir(l.heading);
            let lf = Vec3::Z.cross(f);
            let centre = l.position - lf * (if l.left { width } else { -width } * 0.5);
            (centre, f, lf, stride, if jump || !alternate { m.start_left } else { !l.left })
        }
        None => (ground, fwd, left, 0.0, m.start_left),
    };
    // The first new footstep's start, unless it follows the last one.
    let first_start = match &last {
        Some(l) if m.at_current_frame && state.current_frame as f32 > l.end => Some(state.current_frame as f32),
        Some(_) => None,
        None if m.at_current_frame => Some(state.current_frame as f32),
        None => Some(0.0),
    };
    let mut prev = last.clone();
    let count = if jump { m.count.max(1).div_ceil(2) * 2 } else { m.count.max(1) };
    let first = s.footsteps.len();
    for i in 0..count {
        let k = if jump { i / 2 } else { i };
        let pair = jump && i % 2 == 1;
        let (start, end) = match first_start.filter(|_| i == 0) {
            Some(t) => next_timing(&s.gait, m.gait, None, t, false),
            None => next_timing(&s.gait, m.gait, prev.as_ref(), 0.0, pair),
        };
        let lateral = if side_left { width } else { -width } * 0.5;
        let position = origin + fwd * (along0 + stride * k as f32) + left * lateral + Vec3::Z * (m.stride_height * (k as f32 + if last.is_some() { 1.0 } else { 0.0 }));
        let f = Footstep { left: side_left, position, heading: heading_of(fwd), start, end, active: false };
        s.footsteps.push(f.clone());
        prev = Some(f);
        if alternate || pair || jump {
            side_left = !side_left;
        }
    }
    let n = s.footsteps.len() - first;
    replace_structure(state, root, s, "Create Multiple Footsteps")?;
    Ok(json!({ "created": n, "first": first }))
}

/// Create Footsteps (append): one inactive footstep at `position` after the
/// last; sides alternate and the first is a right footstep.
pub fn append(state: &mut AppState, root: NodeId, position: Vec3, left: Option<bool>) -> Result<Value, String> {
    let mut s = footstep_structure(state, root)?;
    let last = s.footsteps.last().cloned();
    let left = left.unwrap_or(last.as_ref().is_some_and(|l| !l.left));
    let heading = match &last {
        Some(l) if (position - l.position).truncate().length() > 1e-3 => {
            let d = position - l.position;
            heading_of(Vec3::new(d.x, d.y, 0.0))
        }
        Some(l) => l.heading,
        None => heading_of(stance(state, root, &s).1),
    };
    let gait = s.gait.gait;
    let (start, end) = next_timing(&s.gait, gait, last.as_ref(), 0.0, false);
    let index = s.footsteps.len();
    s.footsteps.push(Footstep { left, position, heading, start, end, active: false });
    replace_structure(state, root, s, "Create Footsteps")?;
    Ok(json!({ "index": index, "left": left, "start": start, "end": end }))
}

/// The legs' nodes on one side.
struct Leg {
    thigh: NodeId,
    calf: NodeId,
    foot: NodeId,
    /// The thigh's parent in root space.
    thigh_parent_in_root: Mat4,
    /// The thigh in root space.
    thigh_in_root: Mat4,
    calf_local: Mat4,
    /// The foot's parent (calf or horse link) relative to the calf.
    foot_parent_in_calf: Mat4,
    foot_local: Mat4,
    /// The foot's rotation relative to the root's.
    foot_rel: Quat,
    /// The ankle's ground point and height at rest.
    rest_ground: Vec3,
    ankle_height: f32,
    rest_heading: f32,
}

/// One frame's target for a foot: its ground point under the ankle, heading
/// and whether it is planted.
#[derive(Debug, Clone, Copy)]
struct FootTarget {
    point: Vec3,
    heading: f32,
    planted: bool,
}

fn lerp_angle(a: f32, b: f32, t: f32) -> f32 {
    let d = ((b - a + 540.0).rem_euclid(360.0)) - 180.0;
    a + d * t
}

/// Where one foot is at frame `t`: on its footstep, swinging between two,
/// or (before its first) swinging from where it stood at `t0`.
fn foot_at(steps: &[&Footstep], rest: (Vec3, f32), t0: f32, t: f32, lift: f32) -> FootTarget {
    if let Some(f) = steps.iter().find(|f| t >= f.start - 1e-4 && t <= f.end + 1e-4) {
        return FootTarget { point: f.position, heading: f.heading, planted: true };
    }
    let prev = steps.iter().filter(|f| f.end < t).max_by(|a, b| a.end.total_cmp(&b.end));
    let next = steps.iter().filter(|f| f.start > t).min_by(|a, b| a.start.total_cmp(&b.start));
    let (from, from_heading, from_t) = match prev {
        Some(p) => (p.position, p.heading, p.end),
        None => (rest.0, rest.1, t0),
    };
    match next {
        None => FootTarget { point: from, heading: from_heading, planted: true },
        Some(n) if t <= from_t => FootTarget { point: from, heading: from_heading, planted: prev.is_none() && n.start > t0 },
        Some(n) => {
            let u = ((t - from_t) / (n.start - from_t)).clamp(0.0, 1.0);
            let e = u * u * (3.0 - 2.0 * u);
            FootTarget { point: from.lerp(n.position, e) + Vec3::Z * (lift * (std::f32::consts::PI * u).sin()), heading: lerp_angle(from_heading, n.heading, e), planted: false }
        }
    }
}

/// Puts a key on every component of `p` at `t` with value `v`.
fn key_vec(p: &mut loom_scene::Param<Vec3>, t: f32, v: Vec3) {
    // Noise and List controllers have no keys to set.
    let Some(k) = p.ensure_keys() else { return };
    for (i, tr) in k.tracks.iter_mut().enumerate() {
        tr.set_key(t, v[i], (TangentType::Auto, TangentType::Auto));
    }
}

/// Removes the keys of `p` between `a` and `b`.
fn clear_keys(p: &mut loom_scene::Param<Vec3>, a: f32, b: f32) {
    if let Some(k) = p.keys_mut() {
        for tr in &mut k.tracks {
            tr.keys.retain(|key| key.time < a - 0.5 || key.time > b + 0.5);
        }
    }
}

fn leg(state: &AppState, root: NodeId, all: &BTreeMap<String, NodeId>, tag: &str, root_w: Mat4, h: f32) -> Result<Leg, String> {
    let get = |n: &str| all.get(&format!("{tag} {n}")).copied().ok_or_else(|| format!("the biped has no {tag} {n}"));
    let (thigh, calf, foot) = (get("Thigh")?, get("Calf")?, get("Foot")?);
    let w = |id: NodeId| state.scene.world_matrix(id);
    let inv = root_w.inverse();
    let thigh_parent = state.scene.node(thigh).and_then(|n| n.parent).unwrap_or(root);
    let foot_parent = state.scene.node(foot).and_then(|n| n.parent).unwrap_or(calf);
    let foot_w = w(foot);
    let ankle = foot_w.w_axis.truncate();
    let ankle_height = biped::ANKLE_HEIGHT * h;
    let mut fwd = foot_w.transform_vector3(Vec3::X);
    fwd.z = 0.0;
    Ok(Leg {
        thigh,
        calf,
        foot,
        thigh_parent_in_root: inv * w(thigh_parent),
        thigh_in_root: inv * w(thigh),
        calf_local: w(thigh).inverse() * w(calf),
        foot_parent_in_calf: w(calf).inverse() * w(foot_parent),
        foot_local: w(foot_parent).inverse() * foot_w,
        foot_rel: root_w.to_scale_rotation_translation().1.inverse() * foot_w.to_scale_rotation_translation().1,
        rest_ground: ankle - Vec3::Z * ankle_height,
        ankle_height,
        rest_heading: heading_of(fwd.normalize_or(Vec3::NEG_Y)),
    })
}

fn rot(m: Mat4) -> Quat {
    m.to_scale_rotation_translation().1
}

/// A local transform keeping `prev`'s Euler wrap.
fn local(m: Mat4, prev: &mut Transform) -> Transform {
    let t = Transform::from_matrix_near(m, prev, Vec3::ZERO);
    *prev = t.clone();
    t
}

/// Create Keys for Inactive Footsteps: keys the centre of mass and the legs
/// over the whole footstep sequence and makes every footstep active.
pub fn activate(state: &mut AppState, root: NodeId) -> Result<Value, String> {
    let mut s = structure(state, root).ok_or("no biped")?.clone();
    if s.figure_mode {
        return Err("turn off Figure Mode to activate footsteps".into());
    }
    if s.footsteps.is_empty() {
        return Err("the biped has no footsteps".into());
    }
    if s.footsteps.iter().all(|f| f.active) {
        return Ok(json!({ "keys": 0 }));
    }
    let t0 = s.footsteps.iter().map(|f| f.start).fold(f32::MAX, f32::min).floor();
    let t1 = s.footsteps.iter().map(|f| f.end).fold(f32::MIN, f32::max).ceil();
    let h = s.height.get();
    let all = parts(state, root);
    // The pose the gait starts from: the biped at the first footstep's frame.
    let (rest_root, legs) = anim::with(t0, false, || -> Result<_, String> {
        let root_w = state.scene.world_matrix(root);
        let legs = [leg(state, root, &all, "L", root_w, h)?, leg(state, root, &all, "R", root_w, h)?];
        Ok((root_w, legs))
    })?;
    let root_parent = state.scene.node(root).and_then(|n| n.parent).map(|p| state.scene.world_matrix(p)).unwrap_or(Mat4::IDENTITY);
    let (_, q0, com0) = rest_root.to_scale_rotation_translation();
    let mut fwd0 = q0 * Vec3::NEG_Y;
    fwd0.z = 0.0;
    let yaw0 = heading_of(fwd0.normalize_or(Vec3::NEG_Y));
    let standing = com0.z - (legs[0].rest_ground.z + legs[1].rest_ground.z) * 0.5;
    let lift = match s.gait.gait {
        Gait::Walk => 0.05,
        Gait::Run => 0.08,
        Gait::Jump => 0.06,
    } * h;
    let frames: Vec<f32> = (t0 as i32..=t1 as i32).map(|f| f as f32).collect();
    // Feet, then the centre of mass, frame by frame.
    let mut feet: Vec<[FootTarget; 2]> = Vec::new();
    for &t in &frames {
        let f = |left: bool, l: &Leg| {
            let steps: Vec<&Footstep> = s.footsteps.iter().filter(|f| f.left == left).collect();
            foot_at(&steps, (l.rest_ground, l.rest_heading), t0, t, lift)
        };
        feet.push([f(true, &legs[0]), f(false, &legs[1])]);
    }
    let reach = |l: &Leg| (l.calf_local.w_axis.truncate().length() + (l.foot_parent_in_calf * l.foot_local).w_axis.truncate().length()) * 0.98;
    let mut com_z: Vec<Option<f32>> = Vec::new();
    let mut com_xy: Vec<Vec3> = Vec::new();
    let mut yaws: Vec<f32> = Vec::new();
    for ft in &feet {
        let mid = (ft[0].point + ft[1].point) * 0.5;
        let yaw = lerp_angle(ft[0].heading, ft[1].heading, 0.5);
        let body = Quat::from_rotation_z((yaw - yaw0).to_radians()) * q0;
        com_xy.push(mid);
        yaws.push(yaw);
        let mut z: Option<f32> = None;
        for (i, l) in legs.iter().enumerate() {
            if !ft[i].planted {
                continue;
            }
            let ankle = ft[i].point + Vec3::Z * l.ankle_height;
            let hip_off = body * l.thigh_in_root.w_axis.truncate();
            let d = (Vec3::new(mid.x, mid.y, 0.0) + Vec3::new(hip_off.x, hip_off.y, 0.0) - Vec3::new(ankle.x, ankle.y, 0.0)).length();
            let r = reach(l);
            let dz = (r * r - d * d).max(0.0).sqrt();
            let limit = (ankle.z + dz - hip_off.z).min(ft[i].point.z + standing - 0.005 * h);
            z = Some(z.map_or(limit, |z: f32| z.min(limit)));
        }
        com_z.push(z);
    }
    // Airborne frames fly a ballistic arc between the supported ones.
    let g = 9.81 * (h / biped::DEFAULT_HEIGHT_METRES) / (state.fps.max(1.0) * state.fps.max(1.0));
    let mut zs: Vec<f32> = vec![0.0; frames.len()];
    let mut i = 0;
    while i < frames.len() {
        if let Some(z) = com_z[i] {
            zs[i] = z;
            i += 1;
            continue;
        }
        let a = i;
        while i < frames.len() && com_z[i].is_none() {
            i += 1;
        }
        let za = if a > 0 { zs[a - 1] } else { com_z.iter().flatten().next().copied().unwrap_or(com0.z) };
        let zb = com_z.get(i).copied().flatten().unwrap_or(za);
        let (ta, tb) = (a as f32 - 1.0, i as f32);
        for (j, z) in zs.iter_mut().enumerate().take(i).skip(a) {
            let t = j as f32;
            *z = za + (zb - za) * (t - ta) / (tb - ta) + 0.5 * g * (t - ta) * (tb - t);
        }
    }
    // Keys.
    let mut nodes: BTreeMap<NodeId, Transform> = BTreeMap::new();
    for id in std::iter::once(root).chain(legs.iter().flat_map(|l| [l.thigh, l.calf, l.foot])) {
        nodes.insert(id, state.scene.node(id).ok_or("a missing part")?.transform.clone());
    }
    let mut prev: BTreeMap<NodeId, Transform> = nodes.clone();
    for (fi, &t) in frames.iter().enumerate() {
        let body = Quat::from_rotation_z((yaws[fi] - yaw0).to_radians()) * q0;
        let com = Vec3::new(com_xy[fi].x, com_xy[fi].y, zs[fi]);
        let root_w = Mat4::from_rotation_translation(body, com);
        let fwd = heading_dir(yaws[fi]);
        let rl = local(root_parent.inverse() * root_w, prev.get_mut(&root).expect("listed"));
        let tr = nodes.get_mut(&root).expect("listed");
        key_vec(&mut tr.position, t, rl.position.get());
        key_vec(&mut tr.rotation, t, rl.rotation.get());
        for (i, l) in legs.iter().enumerate() {
            let target = feet[fi][i];
            let parent_w = root_w * l.thigh_parent_in_root;
            let thigh0 = root_w * l.thigh_in_root;
            let hip = thigh0.w_axis.truncate();
            let v1 = l.calf_local.w_axis.truncate();
            let calf_to_foot = (l.foot_parent_in_calf * l.foot_local).w_axis.truncate();
            let (l1, l2) = (v1.length(), calf_to_foot.length());
            let ankle = target.point + Vec3::Z * l.ankle_height;
            let to = ankle - hip;
            let d = to.length().clamp((l1 - l2).abs() + 1e-4, (l1 + l2) * 0.9999);
            let u = to.normalize_or(Vec3::NEG_Z);
            let w = (fwd - u * fwd.dot(u)).normalize_or(fwd);
            let cos_a = ((l1 * l1 + d * d - l2 * l2) / (2.0 * l1 * d)).clamp(-1.0, 1.0);
            let knee = hip + (u * cos_a + w * (1.0 - cos_a * cos_a).sqrt()) * l1;
            let r0 = rot(thigh0);
            let q1 = Quat::from_rotation_arc((r0 * v1).normalize_or(Vec3::X), (knee - hip).normalize_or(Vec3::X));
            let thigh_w = Mat4::from_rotation_translation(q1 * r0, hip);
            let calf0 = thigh_w * l.calf_local;
            let knee_p = calf0.w_axis.truncate();
            let rc0 = rot(calf0);
            let reach_to = hip + u * d;
            let q2 = Quat::from_rotation_arc((rc0 * calf_to_foot).normalize_or(Vec3::X), (reach_to - knee_p).normalize_or(Vec3::X));
            let calf_w = Mat4::from_rotation_translation(q2 * rc0, knee_p);
            let foot_parent_w = calf_w * l.foot_parent_in_calf;
            let foot_rot = Quat::from_rotation_z((target.heading - yaw0).to_radians()) * q0 * l.foot_rel;
            let foot_w = Mat4::from_rotation_translation(foot_rot, (foot_parent_w * l.foot_local).w_axis.truncate());
            for (id, m) in [(l.thigh, parent_w.inverse() * thigh_w), (l.calf, thigh_w.inverse() * calf_w), (l.foot, foot_parent_w.inverse() * foot_w)] {
                let lt = local(m, prev.get_mut(&id).expect("listed"));
                key_vec(&mut nodes.get_mut(&id).expect("listed").rotation, t, lt.rotation.get());
            }
        }
    }
    let mut changes: Vec<Change> = nodes
        .into_iter()
        .map(|(id, after)| Change::SetTransform { id, before: state.scene.node(id).expect("listed").transform.clone(), after })
        .collect();
    for f in &mut s.footsteps {
        f.active = true;
    }
    let node = state.scene.node(root).expect("checked");
    changes.push(Change::ReplaceObject { id: root, before: Box::new(node.object.clone()), after: Box::new(ObjectKind::Biped(Box::new(s))) });
    state.undo.commit_exact("Create Keys for Inactive Footsteps", Change::Group(changes), &mut state.scene)?;
    state.mesh_dirty = true;
    // The active time segment grows to hold the footsteps.
    if (t1 as i32) > state.frame_range.1 {
        state.frame_range.1 = t1 as i32;
    }
    Ok(json!({ "keys": frames.len(), "start": t0, "end": t1 }))
}

/// Deactivate Footsteps: removes the keys the footsteps made from the centre
/// of mass and the legs; the footsteps stay, inactive.
pub fn deactivate(state: &AppState, root: NodeId) -> Result<Vec<Change>, String> {
    let mut s = structure(state, root).ok_or("no biped")?.clone();
    let active: Vec<&Footstep> = s.footsteps.iter().filter(|f| f.active).collect();
    if active.is_empty() {
        return Ok(Vec::new());
    }
    let t0 = active.iter().map(|f| f.start).fold(f32::MAX, f32::min).floor();
    let t1 = active.iter().map(|f| f.end).fold(f32::MIN, f32::max).ceil();
    let all = parts(state, root);
    let mut changes = Vec::new();
    let ids = std::iter::once(Some(root)).chain(["L Thigh", "L Calf", "L Foot", "R Thigh", "R Calf", "R Foot"].iter().map(|n| all.get(*n).copied()));
    for id in ids.flatten() {
        let n = state.scene.node(id).ok_or("a missing part")?;
        let mut t = n.transform.clone();
        clear_keys(&mut t.position, t0, t1);
        clear_keys(&mut t.rotation, t0, t1);
        if t != n.transform {
            changes.push(Change::SetTransform { id, before: n.transform.clone(), after: t });
        }
    }
    for f in &mut s.footsteps {
        f.active = false;
    }
    let node = state.scene.node(root).expect("checked");
    changes.push(Change::ReplaceObject { id: root, before: Box::new(node.object.clone()), after: Box::new(ObjectKind::Biped(Box::new(s))) });
    Ok(changes)
}

/// Footstep colours: inactive bright blue (left) and green (right), active pastel.
pub const LEFT_INACTIVE: [u8; 3] = [30, 90, 255];
pub const RIGHT_INACTIVE: [u8; 3] = [0, 220, 0];
pub const LEFT_ACTIVE: [u8; 3] = [150, 170, 255];
pub const RIGHT_ACTIVE: [u8; 3] = [150, 230, 150];

pub fn footstep_color(f: &Footstep) -> [u8; 3] {
    match (f.left, f.active) {
        (true, false) => LEFT_INACTIVE,
        (false, false) => RIGHT_INACTIVE,
        (true, true) => LEFT_ACTIVE,
        (false, true) => RIGHT_ACTIVE,
    }
}

/// The footprint's centre (on the ground) and its forward and left axes, half length and half width.
fn footprint(s: &BipedStructure, f: &Footstep) -> (Vec3, Vec3, Vec3, f32, f32) {
    let h = s.height.get();
    let (len, wid) = (biped::FOOT_LENGTH * h, biped::FOOT_WIDTH * h);
    let fwd = heading_dir(f.heading);
    let left = Vec3::Z.cross(fwd);
    let attach = s.ankle_attach.get().clamp(0.0, 1.0);
    (f.position + fwd * ((0.5 - attach) * len), fwd, left, len * 0.5, wid * 0.5)
}

/// The footprints as viewport lines: an oval per footstep with a line to its toe.
pub fn footprint_lines(s: &BipedStructure) -> Vec<(Vec3, Vec3, crate::helpers::LineKind)> {
    let mut out = Vec::new();
    for f in &s.footsteps {
        let k = crate::helpers::LineKind::Rgb(footstep_color(f));
        let (c, fwd, left, hl, hw) = footprint(s, f);
        let lift = Vec3::Z * (hl * 0.02);
        let n = 20;
        let p = |i: u32| {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            // A foot shape: wider at the ball than at the heel.
            let w = hw * (0.8 + 0.2 * a.cos());
            c + lift + fwd * (hl * a.cos()) + left * (w * a.sin())
        };
        for i in 0..n {
            out.push((p(i), p(i + 1), k));
        }
        out.push((c + lift, c + lift + fwd * hl, k));
    }
    out
}

/// Each footstep's number at its centre.
pub fn footstep_labels(s: &BipedStructure) -> Vec<(Vec3, String)> {
    s.footsteps.iter().enumerate().map(|(i, f)| (footprint(s, f).0, i.to_string())).collect()
}

fn selected_root(state: &AppState) -> Result<NodeId, String> {
    state.selection.iter().find_map(|id| root_of(state, *id)).ok_or_else(|| "select a biped".into())
}

pub fn execute(state: &mut AppState, cmd: Command) -> Result<Value, String> {
    use Command as C;
    let root = selected_root(state)?;
    match cmd {
        C::BipedFootstepMode { on } => footstep_mode(state, root, on),
        C::BipedFootstepCreation { gait, walk_footstep, double_support, run_footstep, run_airborne, jump_two_feet_down, jump_airborne } => {
            let mut s = footstep_structure(state, root)?;
            let g = &mut s.gait;
            if let Some(name) = gait {
                g.gait = Gait::from_name(&name).ok_or_else(|| format!("unknown gait {name:?} (Walk, Run, Jump)"))?;
            }
            let set = |v: &mut u32, x: Option<u32>, min: u32| {
                if let Some(x) = x {
                    *v = x.clamp(min, 1000);
                }
            };
            set(&mut g.walk_footstep, walk_footstep, 2);
            set(&mut g.double_support, double_support, 0);
            set(&mut g.run_footstep, run_footstep, 1);
            set(&mut g.run_airborne, run_airborne, 0);
            set(&mut g.jump_two_feet_down, jump_two_feet_down, 1);
            set(&mut g.jump_airborne, jump_airborne, 0);
            g.double_support = g.double_support.min(g.walk_footstep - 1);
            let out = serde_json::to_value(&s.gait).map_err(|e| e.to_string())?;
            replace_structure(state, root, s, "Footstep Creation")?;
            Ok(out)
        }
        C::BipedCreateMultipleFootsteps { gait, count, start_left, alternate, stride_width, stride_length, stride_height, at_current_frame } => {
            let s = footstep_structure(state, root)?;
            let gait = match gait {
                Some(n) => Gait::from_name(&n).ok_or_else(|| format!("unknown gait {n:?} (Walk, Run, Jump)"))?,
                None => s.gait.gait,
            };
            let m = Multiple {
                gait,
                count: count.unwrap_or(4).clamp(1, 1000),
                start_left: start_left.unwrap_or(true),
                alternate: alternate.unwrap_or(true),
                stride_width: stride_width.unwrap_or(1.0),
                stride_length: stride_length.unwrap_or(gait.default_stride()),
                stride_height: stride_height.unwrap_or(0.0),
                at_current_frame: at_current_frame.unwrap_or(false),
            };
            create_multiple(state, root, &m)
        }
        C::BipedAppendFootstep { position, left } => match position {
            Some(p) => append(state, root, Vec3::from(p), left),
            None => {
                footstep_structure(state, root)?;
                state.pick_mode = Some(crate::state::PickMode::BipedFootstep);
                state.status_prompt = "Click to place footsteps; right-click ends".into();
                Ok(json!({ "pick": "BipedFootstep" }))
            }
        },
        C::BipedActivateFootsteps => activate(state, root),
        C::BipedDeactivateFootsteps => {
            let changes = deactivate(state, root)?;
            if changes.is_empty() {
                return Ok(json!({ "deactivated": 0 }));
            }
            state.undo.commit_exact("Deactivate Footsteps", Change::Group(changes), &mut state.scene)?;
            state.mesh_dirty = true;
            Ok(json!({}))
        }
        C::BipedDeleteFootsteps => {
            let mut s = structure(state, root).ok_or("no biped")?.clone();
            if s.footsteps.is_empty() {
                return Ok(json!({ "deleted": 0 }));
            }
            // The keys of active footsteps go with them.
            let mut changes: Vec<Change> = deactivate(state, root)?.into_iter().filter(|c| !matches!(c, Change::ReplaceObject { .. })).collect();
            let n = s.footsteps.len();
            s.footsteps.clear();
            let node = state.scene.node(root).expect("checked");
            changes.push(Change::ReplaceObject { id: root, before: Box::new(node.object.clone()), after: Box::new(ObjectKind::Biped(Box::new(s))) });
            state.undo.commit_exact("Delete Footsteps", Change::Group(changes), &mut state.scene)?;
            state.mesh_dirty = true;
            Ok(json!({ "deleted": n }))
        }
        other => Err(format!("{} is not a footstep command", other.name())),
    }
}
