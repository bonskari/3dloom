//! Reference-pose corrected humanoid motion transfer onto native bones.
use crate::AppState;
use glam::{Mat3, Mat4, Quat, Vec3};
use loom_scene::{Change, Controller, NodeId, Param};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BoneMapping {
    pub source: NodeId,
    pub target: NodeId,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn world_reference_correction_preserves_target_bind_axes() {
        let bind = Quat::from_rotation_y(0.8);
        let motion = Quat::from_rotation_z(0.5);
        assert!(correct_rotation(motion, Quat::IDENTITY, bind).abs_diff_eq(motion * bind, 1e-6));
        let source_bind = Quat::from_rotation_x(-1.2);
        assert!(correct_rotation(source_bind, source_bind, bind).abs_diff_eq(bind, 1e-6));
        let aligned = Quat::from_rotation_z(1.4);
        assert!(correct_rotation(aligned * motion, aligned, bind)
            .abs_diff_eq(aligned * motion * aligned.inverse() * bind, 1e-6));
    }
    #[test]
    fn palm_reference_preserves_source_axial_twist() {
        let correction = reference_axes(Vec3::X, Vec3::Z, Some((Vec3::Y, Vec3::Y)));
        let twist = Quat::from_rotation_x(0.7);
        let posed = correct_rotation(twist, Quat::IDENTITY, correction);
        assert!((posed * Vec3::Z).abs_diff_eq(Vec3::X, 1e-6));
        assert!((posed * Vec3::Y).abs_diff_eq(twist * Vec3::Y, 1e-6));
        assert!(
            (posed * Vec3::Y).distance(correction * Vec3::Y) > 0.5,
            "palm roll was collapsed"
        );
    }
    #[test]
    fn continuous_euler_keys_reconstruct_quaternion_across_wrap() {
        let mut previous = None;
        for angle in [170.0_f32, 179.0, 181.0, 190.0] {
            let q = Quat::from_rotation_z(angle.to_radians());
            let r = euler_near(q, previous);
            if let Some(p) = previous {
                assert!((r - p).length() < 20.0);
            }
            let reconstructed = Quat::from_euler(
                glam::EulerRot::XYZ,
                r.x.to_radians(),
                r.y.to_radians(),
                r.z.to_radians(),
            );
            assert!(q.dot(reconstructed).abs() > 0.99999);
            previous = Some(r);
        }
    }
}

fn correct_rotation(sample: Quat, source_bind: Quat, target_bind: Quat) -> Quat {
    (sample * source_bind.inverse() * target_bind).normalize()
}

/// Align anatomical reference axes while retaining all animated quaternion
/// degrees of freedom. A palm's second axis determines roll as well as swing.
fn reference_axes(source: Vec3, target: Vec3, secondary: Option<(Vec3, Vec3)>) -> Quat {
    let source = source.normalize_or_zero();
    let target = target.normalize_or_zero();
    if source.length_squared() < 0.5 || target.length_squared() < 0.5 {
        return Quat::IDENTITY;
    }
    if let Some((source_side, target_side)) = secondary {
        let a = (source_side - source * source_side.dot(source)).normalize_or_zero();
        let b = (target_side - target * target_side.dot(target)).normalize_or_zero();
        if a.length_squared() > 0.5 && b.length_squared() > 0.5 {
            let from = Quat::from_mat3(&Mat3::from_cols(target, b, target.cross(b)));
            let to = Quat::from_mat3(&Mat3::from_cols(source, a, source.cross(a)));
            return (to * from.inverse()).normalize();
        }
    }
    Quat::from_rotation_arc(target, source)
}

fn rest(p: &Param<Vec3>) -> Result<Vec3, String> {
    match &p.controller {
        Controller::Constant(v) => Ok(*v),
        Controller::Keys(k) if k.rest.len() == 3 => Ok(Vec3::from_slice(&k.rest)),
        _ => Err("retarget reference requires constant or keyed PRS controllers".into()),
    }
}
fn reference(scene: &loom_scene::Scene, id: NodeId, depth: usize) -> Result<Mat4, String> {
    if depth > 512 {
        return Err("skeleton hierarchy cycle or excessive depth".into());
    }
    let n = scene.node(id).ok_or("missing reference bone")?;
    if n.inherit != loom_scene::node::Axes3::ALL || !n.constraints.is_empty() {
        return Err(format!(
            "{} has unsupported transform inheritance or constraints",
            n.name
        ));
    }
    let r = rest(&n.transform.rotation)?;
    let m = Mat4::from_scale_rotation_translation(
        rest(&n.transform.scale)?,
        Quat::from_euler(
            glam::EulerRot::XYZ,
            r.x.to_radians(),
            r.y.to_radians(),
            r.z.to_radians(),
        ),
        rest(&n.transform.position)?,
    );
    Ok(if let Some(p) = n.parent {
        reference(scene, p, depth + 1)? * m
    } else {
        m
    })
}
fn under(scene: &loom_scene::Scene, mut id: NodeId, root: NodeId) -> bool {
    for _ in 0..512 {
        if id == root {
            return true;
        }
        match scene.node(id).and_then(|n| n.parent) {
            Some(p) => id = p,
            None => return false,
        }
    }
    false
}
fn short(name: &str) -> &str {
    name.rsplit([':', '|']).next().unwrap_or(name)
}

/// Resolve the known Kimodo / MetaHuman body vocabulary. Face and twist bones
/// are deliberately not mapped; they retain their original local transforms.
pub fn auto_mapping(
    state: &AppState,
    source_clip: u64,
    target_root: NodeId,
) -> Result<Vec<BoneMapping>, String> {
    let clip = state
        .scene
        .animation_clips
        .iter()
        .find(|c| c.id == source_clip)
        .ok_or("source clip not found")?;
    let mut names = vec![
        ("Hips".to_owned(), "pelvis".to_owned()),
        ("Spine1".into(), "spine_01".into()),
        ("Spine2".into(), "spine_03".into()),
        ("Chest".into(), "spine_05".into()),
        ("Neck1".into(), "neck_01".into()),
        ("Neck2".into(), "neck_02".into()),
        ("Head".into(), "head".into()),
    ];
    for (side, suffix) in [("Left", "l"), ("Right", "r")] {
        for (s, t) in [
            ("Shoulder", "clavicle"),
            ("Arm", "upperarm"),
            ("ForeArm", "lowerarm"),
            ("Hand", "hand"),
            ("Leg", "thigh"),
            ("Shin", "calf"),
            ("Foot", "foot"),
            ("ToeBase", "ball"),
        ] {
            names.push((format!("{side}{s}"), format!("{t}_{suffix}")));
        }
        for (s, t) in [
            ("Thumb", "thumb"),
            ("Index", "index"),
            ("Middle", "middle"),
            ("Ring", "ring"),
            ("Pinky", "pinky"),
        ] {
            // Kimodo's non-thumb joint 1 is the metacarpal, followed by
            // three phalanges. MetaHuman names those four bones explicitly.
            let offset = if s == "Thumb" {
                0
            } else {
                names.push((
                    format!("{side}Hand{s}1"),
                    format!("{t}_metacarpal_{suffix}"),
                ));
                1
            };
            for i in 1..=3 {
                names.push((
                    format!("{side}Hand{s}{}", i + offset),
                    format!("{t}_{i:02}_{suffix}"),
                ));
            }
        }
    }
    let mut result = Vec::new();
    for (s, t) in names {
        let sources: Vec<_> = clip
            .nodes
            .iter()
            .filter(|id| state.scene.node(**id).is_some_and(|n| short(&n.name) == s))
            .collect();
        let targets: Vec<_> = state
            .scene
            .nodes()
            .filter(|n| short(&n.name) == t && under(&state.scene, n.id, target_root))
            .collect();
        if sources.len() > 1 || targets.len() > 1 {
            return Err(format!(
                "ambiguous retarget bone {s} / {t}; provide explicit mapping"
            ));
        }
        if let (Some(source), Some(target)) = (sources.first(), targets.first()) {
            result.push(BoneMapping {
                source: **source,
                target: target.id,
            });
        }
    }
    if !result.iter().any(|m| {
        state
            .scene
            .node(m.target)
            .is_some_and(|n| short(&n.name) == "pelvis")
    }) {
        return Err("auto mapping requires Kimodo Hips and MetaHuman pelvis".into());
    }
    Ok(result)
}

fn keyed(rest: Vec3, samples: &[(f32, Vec3)]) -> Param<Vec3> {
    use loom_scene::{
        anim::{Interp, TangentType},
        param::{KeyedTracks, ParamType},
    };
    let mut k = KeyedTracks::from_rest(ParamType::Vec3, rest.to_array().to_vec());
    for (axis, track) in k.tracks.iter_mut().enumerate() {
        track.interp = Interp::Linear;
        for (time, v) in samples {
            track.set_key(*time, v[axis], (TangentType::Linear, TangentType::Linear));
        }
    }
    Param {
        controller: Controller::Keys(k),
    }
}
fn append_keys(previous: &Param<Vec3>, mut added: Param<Vec3>) -> Param<Vec3> {
    if let (Controller::Keys(old), Controller::Keys(new)) =
        (&previous.controller, &mut added.controller)
    {
        let mut merged = old.clone();
        for (target, extra) in merged.tracks.iter_mut().zip(&new.tracks) {
            target.keys.extend(extra.keys.iter().cloned());
            target.keys.sort_by(|a, b| a.time.total_cmp(&b.time));
        }
        *new = merged;
    }
    added
}
fn euler_near(q: Quat, previous: Option<Vec3>) -> Vec3 {
    let (x, y, z) = q.to_euler(glam::EulerRot::XYZ);
    let mut r = Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees());
    if let Some(p) = previous {
        let near = |mut v: Vec3| {
            for i in 0..3 {
                v[i] += ((p[i] - v[i]) / 360.0).round() * 360.0;
            }
            v
        };
        r = near(r);
        let alternative = near(Vec3::new(r.x + 180.0, 180.0 - r.y, r.z + 180.0));
        if alternative.distance_squared(p) < r.distance_squared(p) {
            r = alternative;
        }
    }
    r
}

/// Bake corrected source world rotations into original target local controllers.
/// Disjoint clips share native controllers; unowned keys and overlapping ranges
/// are rejected rather than overwritten.
pub fn retarget(
    state: &mut AppState,
    source_clip: u64,
    target_root: NodeId,
    mapping: &[BoneMapping],
) -> Result<Value, String> {
    let owned;
    let mapping = if mapping.is_empty() {
        owned = auto_mapping(state, source_clip, target_root)?;
        &owned[..]
    } else {
        mapping
    };
    let clip = state
        .scene
        .animation_clips
        .iter()
        .find(|c| c.id == source_clip)
        .ok_or("source clip not found")?
        .clone();
    let scene = &state.scene;
    scene.node(target_root).ok_or("target root not found")?;
    let sources: BTreeSet<_> = mapping.iter().map(|m| m.source).collect();
    let targets: BTreeSet<_> = mapping.iter().map(|m| m.target).collect();
    if sources.len() != mapping.len() || targets.len() != mapping.len() {
        return Err("retarget mapping must be one-to-one".into());
    }
    if targets.iter().any(|t| clip.nodes.contains(t)) {
        return Err("source and target skeletons must be separate".into());
    }
    // Clip intervals own their native keys. Shared target nodes are safe only
    // when every existing key is covered and no two owners touch or overlap.
    for m in mapping {
        if !clip.nodes.contains(&m.source) || !under(scene, m.target, target_root) {
            return Err("mapping bone is outside source clip or target hierarchy".into());
        }
        let n = scene.node(m.target).ok_or("target bone missing")?;
        let owners: Vec<_> = scene
            .animation_clips
            .iter()
            .filter(|c| c.nodes.contains(&m.target))
            .collect();
        for (i, owner) in owners.iter().enumerate() {
            if !owner.start.is_finite() || !owner.end.is_finite() || owner.end < owner.start {
                return Err("invalid existing target clip range".into());
            }
            if clip.start <= owner.end && clip.end >= owner.start
                || owners[..i]
                    .iter()
                    .any(|other| other.start <= owner.end && other.end >= owner.start)
            {
                return Err("target clip ranges overlap or touch; leave a strict gap".into());
            }
        }
        if !matches!(n.transform.scale.controller, Controller::Constant(_)) {
            return Err(format!(
                "{} has unsupported target scale controller",
                n.name
            ));
        }
        for p in [&n.transform.position, &n.transform.rotation] {
            match &p.controller {
                Controller::Constant(_) => {}
                Controller::Keys(k) => {
                    for track in &k.tracks {
                        if !track.keys.is_empty()
                            && track.interp != loom_scene::anim::Interp::Linear
                        {
                            return Err(
                                "retarget sequencing requires linear native target keys".into()
                            );
                        }
                        if track.keys.iter().any(|key| {
                            !key.time.is_finite()
                                || !key.value.is_finite()
                                || !owners
                                    .iter()
                                    .any(|c| key.time >= c.start && key.time <= c.end)
                        }) {
                            return Err(format!("{} has unowned target keys; retarget refuses destructive overwrite",n.name));
                        }
                    }
                }
                _ => {
                    return Err(format!(
                        "{} has unsupported target animation controller",
                        n.name
                    ))
                }
            }
        }
    }
    for id in &clip.nodes {
        let n = scene.node(*id).ok_or("source clip member missing")?;
        match &n.transform.scale.controller {
            Controller::Constant(_) => {}
            Controller::Keys(k)
                if k.rest.len() == 3
                    && k.tracks.len() == 3
                    && k.tracks.iter().enumerate().all(|(axis, tr)| {
                        tr.keys.iter().all(|key| {
                            key.value.is_finite() && (key.value - k.rest[axis]).abs() < 1e-6
                        })
                    }) => {}
            _ => {
                return Err(format!(
                "{} has nonconstant source scale; retarget does not support animated bone scaling",
                n.name
            ))
            }
        }
        if !n.constraints.is_empty() {
            return Err(format!("{} has unsupported source constraints", n.name));
        }
    }
    let mut times = vec![clip.start, clip.end];
    for id in &clip.nodes {
        if let Some(n) = scene.node(*id) {
            for p in [&n.transform.position, &n.transform.rotation] {
                if let Controller::Keys(k) = &p.controller {
                    for tr in &k.tracks {
                        times.extend(
                            tr.keys
                                .iter()
                                .map(|k| k.time)
                                .filter(|t| *t >= clip.start && *t <= clip.end),
                        );
                    }
                }
            }
        }
    }
    if times.iter().any(|t| !t.is_finite()) || clip.end < clip.start {
        return Err("invalid source clip time range".into());
    }
    times.sort_by(f32::total_cmp);
    times.dedup();
    let mut binds = BTreeMap::new();
    for m in mapping {
        binds.insert(m.source, reference(scene, m.source, 0)?);
        binds.insert(m.target, reference(scene, m.target, 0)?);
    }
    // Sum corresponding bind distances, using nearest mapped target ancestor:
    // MetaHuman's extra spine/twist joints must not change the scale estimate.
    let by_target: BTreeMap<_, _> = mapping.iter().map(|m| (m.target, m.source)).collect();
    let mut source_length = 0.0;
    let mut target_length = 0.0;
    let mut roots = Vec::new();
    let mut children: BTreeMap<NodeId, Vec<(NodeId, NodeId)>> = BTreeMap::new();
    for m in mapping {
        let mut p = scene.node(m.target).and_then(|n| n.parent);
        let mut ancestor = None;
        while let Some(id) = p {
            if let Some(source) = by_target.get(&id) {
                ancestor = Some((id, *source));
                break;
            }
            p = scene.node(id).and_then(|n| n.parent);
        }
        if let Some((t, s)) = ancestor {
            children.entry(t).or_default().push((m.target, m.source));
            source_length +=
                (binds[&m.source].w_axis.truncate() - binds[&s].w_axis.truncate()).length();
            target_length +=
                (binds[&m.target].w_axis.truncate() - binds[&t].w_axis.truncate()).length();
        } else {
            roots.push(m.target);
        }
    }
    if roots.len() != 1 || source_length <= 1e-6 || target_length <= 1e-6 {
        return Err(
            "mapping needs one connected humanoid root and nonzero reference lengths".into(),
        );
    }
    let ratio = target_length / source_length;
    fn top(scene: &loom_scene::Scene, mut id: NodeId) -> NodeId {
        for _ in 0..512 {
            match scene.node(id).and_then(|n| n.parent) {
                Some(p) => id = p,
                None => break,
            }
        }
        id
    }
    let source_top = top(scene, mapping[0].source);
    let target_top = top(scene, target_root);
    let alignment = reference(scene, target_top, 0)?
        .to_scale_rotation_translation()
        .1
        * reference(scene, source_top, 0)?
            .to_scale_rotation_translation()
            .1
            .inverse();
    let mut corrections = BTreeMap::new();
    for m in mapping {
        let source_position = binds[&m.source].w_axis.truncate();
        let target_position = binds[&m.target].w_axis.truncate();
        let incoming = scene
            .node(m.source)
            .and_then(|n| n.parent)
            .map(|p| reference(scene, p, 0).map(|b| source_position - b.w_axis.truncate()))
            .transpose()?
            .unwrap_or(Vec3::ZERO);
        let candidates: Vec<_> = children
            .get(&m.target)
            .into_iter()
            .flatten()
            .copied()
            .filter(|(_, s)| under(scene, *s, m.source))
            .collect();
        let middle = if candidates.len() >= 4 {
            candidates.iter().find(|(_, s)| {
                scene
                    .node(*s)
                    .is_some_and(|n| n.name.to_ascii_lowercase().contains("middle"))
            })
        } else {
            None
        };
        let primary = middle.or_else(|| {
            candidates.iter().max_by(|(_, a), (_, b)| {
                let score = |s: &NodeId| {
                    (binds[s].w_axis.truncate() - source_position)
                        .normalize_or_zero()
                        .dot(incoming.normalize_or_zero())
                };
                score(a).total_cmp(&score(b))
            })
        });
        let (source_axis, target_axis) = if let Some((target_child, source_child)) = primary {
            (
                binds[source_child].w_axis.truncate() - source_position,
                binds[target_child].w_axis.truncate() - target_position,
            )
        } else {
            let target_incoming = scene
                .node(m.target)
                .and_then(|n| n.parent)
                .map(|p| reference(scene, p, 0).map(|b| target_position - b.w_axis.truncate()))
                .transpose()?
                .unwrap_or(Vec3::ZERO);
            (incoming, target_incoming)
        };
        // A hand supplies a complete palm frame: middle metacarpal axis
        // and index-to-pinky span. Unknown names fall back to the widest pair.
        // Torso branches retain reference roll instead of deriving it from
        // differing clavicle or hip attachment positions.
        let mut secondary = None;
        if candidates.len() >= 4 {
            let find = |part: &str| {
                candidates.iter().find(|(_, s)| {
                    scene
                        .node(*s)
                        .is_some_and(|n| n.name.to_ascii_lowercase().contains(part))
                })
            };
            if let (Some((ta, sa)), Some((tb, sb))) =
                (find("index"), find("pinky").or_else(|| find("little")))
            {
                secondary = Some((
                    alignment * (binds[sa].w_axis.truncate() - binds[sb].w_axis.truncate()),
                    binds[ta].w_axis.truncate() - binds[tb].w_axis.truncate(),
                ));
            } else {
                let axis = source_axis.normalize_or_zero();
                let mut widest = 0.0;
                for (i, (ta, sa)) in candidates.iter().enumerate() {
                    for (tb, sb) in &candidates[..i] {
                        let span = binds[sa].w_axis.truncate() - binds[sb].w_axis.truncate();
                        let width = (span - axis * span.dot(axis)).length_squared();
                        if width > widest {
                            widest = width;
                            secondary = Some((
                                alignment * span,
                                binds[ta].w_axis.truncate() - binds[tb].w_axis.truncate(),
                            ));
                        }
                    }
                }
            }
        }
        corrections.insert(
            m.target,
            reference_axes(alignment * source_axis, target_axis, secondary),
        );
    }
    let mut rotations: BTreeMap<NodeId, Vec<(f32, Vec3)>> = BTreeMap::new();
    let mut positions = Vec::new();
    for time in times {
        let desired: BTreeMap<_, _> = scene.at_time(time, || {
            mapping
                .iter()
                .map(|m| {
                    (
                        m.target,
                        correct_rotation(
                            alignment
                                * scene
                                    .world_matrix(m.source)
                                    .to_scale_rotation_translation()
                                    .1,
                            alignment * binds[&m.source].to_scale_rotation_translation().1,
                            corrections[&m.target]
                                * binds[&m.target].to_scale_rotation_translation().1,
                        ),
                    )
                })
                .collect()
        });
        // Recursively evaluate unmapped intervening joints under the new parent
        // rotations, keeping all their local bind transforms unchanged.
        fn world(
            scene: &loom_scene::Scene,
            id: NodeId,
            desired: &BTreeMap<NodeId, Quat>,
            depth: usize,
        ) -> Result<Quat, String> {
            if depth > 512 {
                return Err("hierarchy depth exceeded".into());
            }
            if let Some(q) = desired.get(&id) {
                return Ok(*q);
            }
            let n = scene.node(id).ok_or("missing target ancestor")?;
            Ok(n.parent
                .map(|p| world(scene, p, desired, depth + 1))
                .transpose()?
                .unwrap_or(Quat::IDENTITY)
                * n.transform.rotation_quat())
        }
        for m in mapping {
            let n = scene.node(m.target).unwrap();
            let parent = scene
                .at_time(time, || {
                    n.parent.map(|p| world(scene, p, &desired, 0)).transpose()
                })?
                .unwrap_or(Quat::IDENTITY);
            let values = rotations.entry(m.target).or_default();
            values.push((
                time,
                euler_near(
                    parent.inverse() * desired[&m.target],
                    values
                        .last()
                        .map(|(_, v)| *v)
                        .or(Some(n.transform.rotation.get_at(time))),
                ),
            ));
            if m.target == roots[0] {
                let source_pos =
                    scene.at_time(time, || scene.world_matrix(m.source).w_axis.truncate());
                let delta = alignment * (source_pos - binds[&m.source].w_axis.truncate()) * ratio;
                // Bake in the target reference frame. Live animator offsets
                // (Master rotation/scale, for example) must remain an overlay:
                // using their current world matrix would cancel them here.
                let parent_matrix = n
                    .parent
                    .map(|p| reference(scene, p, 0))
                    .transpose()?
                    .unwrap_or(Mat4::IDENTITY);
                let position =
                    rest(&n.transform.position)? + parent_matrix.inverse().transform_vector3(delta);
                if !position.is_finite() {
                    return Err("non-finite retarget root translation".into());
                }
                positions.push((time, position));
            }
        }
    }
    let mut changes = Vec::new();
    for m in mapping {
        let n = scene.node(m.target).unwrap();
        let mut after = n.transform.clone();
        after.rotation = append_keys(
            &n.transform.rotation,
            keyed(rest(&n.transform.rotation)?, &rotations[&m.target]),
        );
        if m.target == roots[0] {
            after.position = append_keys(
                &n.transform.position,
                keyed(rest(&n.transform.position)?, &positions),
            );
        }
        changes.push(Change::SetTransform {
            id: m.target,
            before: n.transform.clone(),
            after,
        });
    }
    let before = scene.animation_clips.clone();
    let mut after = before.clone();
    let id = before
        .iter()
        .map(|c| c.id)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or("animation clip IDs exhausted")?;
    after.push(loom_scene::AnimationClip {
        id,
        name: format!("{} → {}", clip.name, scene.node(target_root).unwrap().name),
        nodes: targets.into_iter().collect(),
        start: clip.start,
        end: clip.end,
    });
    changes.push(Change::SetAnimationClips { before, after });
    while state.undo.in_transaction() {
        state.undo.end();
    }
    crate::execute::commit_exact(state, "Retarget Animation", Change::Group(changes))?;
    Ok(
        json!({"clip":id,"source_clip":source_clip,"mapped_bones":mapping.len(),"translation_scale":ratio}),
    )
}

#[cfg(test)]
mod real_assets {
    use super::*;
    #[test]
    #[ignore = "requires LOOM_RETARGET_BVH and LOOM_RETARGET_FBX real production assets"]
    fn kimodo_on_metahuman_preserves_skin_and_skeleton_and_deforms_mesh() {
        let bvh = std::env::var("LOOM_RETARGET_BVH").expect("LOOM_RETARGET_BVH");
        let fbx = std::env::var("LOOM_RETARGET_FBX").expect("LOOM_RETARGET_FBX");
        let mut s = AppState::new();
        s.exchange.fbx_import.metahuman_body_only = true;
        let ids = crate::fbx_io::import(&mut s, std::path::Path::new(&fbx)).unwrap();
        let target = ids
            .iter()
            .copied()
            .find(|id| s.scene.node(*id).unwrap().parent.is_none())
            .unwrap();
        let original: Vec<_> = ids
            .iter()
            .map(|id| {
                (
                    *id,
                    serde_json::to_value(s.scene.node(*id).unwrap()).unwrap(),
                )
            })
            .collect();
        let placement = s.scene.world_matrix(target);
        s.fps = 24.0;
        crate::bvh_io::import_animation(&mut s, std::path::Path::new(&bvh)).unwrap();
        let source = s.scene.animation_clips.last().unwrap().id;
        let map = auto_mapping(&s, source, target).unwrap();
        assert!(
            map.len() >= 45,
            "incomplete body/fingers mapping: {}",
            map.len()
        );
        assert!(map
            .iter()
            .all(|m| !s.scene.node(m.target).unwrap().name.contains("FACIAL")));
        for (source_name, target_name) in [
            ("LeftHandIndex1", "index_metacarpal_l"),
            ("LeftHandIndex2", "index_01_l"),
            ("LeftHandIndex4", "index_03_l"),
            ("RightHandMiddle4", "middle_03_r"),
        ] {
            assert!(
                map.iter().any(
                    |m| short(&s.scene.node(m.source).unwrap().name) == source_name
                        && short(&s.scene.node(m.target).unwrap().name) == target_name
                ),
                "wrong finger correspondence {source_name}→{target_name}"
            );
        }
        let result = retarget(&mut s, source, target, &map).unwrap();
        let clip = s
            .scene
            .animation_clips
            .iter()
            .find(|c| Some(c.id) == result["clip"].as_u64())
            .unwrap()
            .clone();
        assert!(
            placement.abs_diff_eq(s.scene.world_matrix(target), 1e-5),
            "scene placement changed"
        );
        for (id, before) in &original {
            let now = serde_json::to_value(s.scene.node(*id).unwrap()).unwrap();
            for key in ["name", "parent", "object", "modifiers"] {
                assert_eq!(before[key], now[key], "changed {key} on {}", id.0);
            }
            assert_eq!(
                before["transform"]["scale"], now["transform"]["scale"],
                "changed native bone scale"
            );
            let pelvis = map
                .iter()
                .find(|m| short(&s.scene.node(m.source).unwrap().name) == "Hips")
                .unwrap()
                .target;
            if *id != pelvis {
                assert_eq!(
                    before["transform"]["position"], now["transform"]["position"],
                    "changed native bone offset/length on {}",
                    id.0
                );
            }
            if !map.iter().any(|m| m.target == *id) {
                assert_eq!(
                    before["transform"], now["transform"],
                    "changed unmapped/twist bone transform on {}",
                    id.0
                );
            }
        }
        let mesh = ids
            .iter()
            .copied()
            .find(|id| {
                !s.scene.node(*id).unwrap().modifiers.is_empty()
                    && !s
                        .scene
                        .node(*id)
                        .unwrap()
                        .object
                        .mesh()
                        .vertices()
                        .is_empty()
            })
            .expect("real skinned mesh");
        let mesh_start = s.scene.at_time(clip.start, || {
            s.scene
                .eval_mesh(mesh)
                .value
                .mesh
                .vertices()
                .iter()
                .map(|v| v.position)
                .collect::<Vec<_>>()
        });
        let mesh_end = s.scene.at_time(clip.end, || {
            s.scene
                .eval_mesh(mesh)
                .value
                .mesh
                .vertices()
                .iter()
                .map(|v| v.position)
                .collect::<Vec<_>>()
        });
        assert_eq!(mesh_start.len(), mesh_end.len());
        assert!(
            mesh_start
                .iter()
                .zip(&mesh_end)
                .any(|(a, b)| a.distance(*b) > 1e-4),
            "real skinned geometry did not move"
        );
        assert!(mesh_start.iter().chain(&mesh_end).all(|v| v.is_finite()));
        let bind_height = ids
            .iter()
            .map(|id| reference(&s.scene, *id, 0).unwrap().w_axis.z)
            .fold(0.0_f32, f32::max);
        let movement = mesh_start
            .iter()
            .zip(&mesh_end)
            .map(|(a, b)| a.distance(*b))
            .fold(0.0_f32, f32::max);
        assert!(
            movement < bind_height * 2.0,
            "implausible mesh displacement {movement}, body height {bind_height}"
        );
        eprintln!(
            "Retargeted {} body/finger bones, scale {}, {} vertices, max displacement {}",
            map.len(),
            result["translation_scale"],
            mesh_start.len(),
            movement
        );
        let first = s.scene.at_time(clip.start, || {
            map.iter()
                .map(|m| s.scene.world_matrix(m.target))
                .collect::<Vec<_>>()
        });
        let last = s.scene.at_time(clip.end, || {
            map.iter()
                .map(|m| s.scene.world_matrix(m.target))
                .collect::<Vec<_>>()
        });
        assert!(
            first
                .iter()
                .zip(&last)
                .any(|(a, b)| !a.abs_diff_eq(*b, 1e-4)),
            "no retargeted motion"
        );
        for matrix in first.iter().chain(&last) {
            assert!(matrix.is_finite());
        }
        // Compare evaluated thigh/knee/ankle angles against the real seated
        // source, rather than merely checking that some controller changed.
        for side in ["Left", "Right"] {
            let source_id = |part: &str| {
                *s.scene
                    .animation_clips
                    .iter()
                    .find(|c| c.id == source)
                    .unwrap()
                    .nodes
                    .iter()
                    .find(|id| short(&s.scene.node(**id).unwrap().name) == format!("{side}{part}"))
                    .unwrap()
            };
            let joint_ids = [source_id("Leg"), source_id("Shin"), source_id("Foot")];
            let target_ids =
                joint_ids.map(|id| map.iter().find(|m| m.source == id).unwrap().target);
            let knee_angle = |ids: [NodeId; 3]| {
                s.scene.at_time(clip.start, || {
                    let [a, b, c] = ids.map(|id| s.scene.world_matrix(id).w_axis.truncate());
                    (b - a).normalize().dot((c - b).normalize())
                })
            };
            let source_angle = knee_angle(joint_ids);
            let target_angle = knee_angle(target_ids);
            assert!(
                (source_angle - target_angle).abs() < 0.25,
                "{side} knee angle diverges: source {source_angle}, target {target_angle}"
            );
        }
        let alignment = reference(&s.scene, target, 0)
            .unwrap()
            .to_scale_rotation_translation()
            .1
            * reference(
                &s.scene,
                s.scene
                    .animation_clips
                    .iter()
                    .find(|c| c.id == source)
                    .unwrap()
                    .nodes[0],
                0,
            )
            .unwrap()
            .to_scale_rotation_translation()
            .1
            .inverse();
        for time in [clip.start, (clip.start + clip.end) * 0.5, clip.end] {
            for side in ["Left", "Right"] {
                for (a, b) in [
                    ("Arm", "ForeArm"),
                    ("ForeArm", "Hand"),
                    ("Leg", "Shin"),
                    ("Shin", "Foot"),
                    ("Foot", "ToeBase"),
                    ("Hand", "HandMiddle1"),
                    ("HandIndex1", "HandIndex2"),
                    ("HandMiddle1", "HandMiddle2"),
                    ("HandRing1", "HandRing2"),
                    ("HandPinky1", "HandPinky2"),
                ] {
                    let find = |part: &str| {
                        map.iter()
                            .find(|m| {
                                short(&s.scene.node(m.source).unwrap().name)
                                    == format!("{side}{part}")
                            })
                            .unwrap()
                    };
                    let a = find(a);
                    let b = find(b);
                    let (expected, actual) = s.scene.at_time(time, || {
                        let source = (s.scene.world_matrix(b.source).w_axis.truncate()
                            - s.scene.world_matrix(a.source).w_axis.truncate())
                        .normalize();
                        let actual = (s.scene.world_matrix(b.target).w_axis.truncate()
                            - s.scene.world_matrix(a.target).w_axis.truncate())
                        .normalize();
                        (alignment * source, actual)
                    });
                    assert!(
                        expected.dot(actual) > 0.999,
                        "frame{time} {}→{} expected aligned source {expected:?}, target {actual:?}",
                        s.scene.node(a.source).unwrap().name,
                        s.scene.node(b.source).unwrap().name
                    );
                }
            }
        }
        for side in ["Left", "Right"] {
            let find = |part: &str| {
                map.iter()
                    .find(|m| {
                        short(&s.scene.node(m.source).unwrap().name) == format!("{side}{part}")
                    })
                    .unwrap()
            };
            let [hand, middle, index, pinky] =
                ["Hand", "HandMiddle1", "HandIndex1", "HandPinky1"].map(find);
            for time in [clip.start, (clip.start + clip.end) * 0.5, clip.end] {
                let (expected, actual) = s.scene.at_time(time, || {
                    let position = |id| s.scene.world_matrix(id).w_axis.truncate();
                    let source_normal = (position(middle.source) - position(hand.source))
                        .cross(position(index.source) - position(pinky.source))
                        .normalize();
                    let target_normal = (position(middle.target) - position(hand.target))
                        .cross(position(index.target) - position(pinky.target))
                        .normalize();
                    (alignment * source_normal, target_normal)
                });
                assert!(expected.dot(actual)>0.999,"{side} palm roll differs from aligned source at frame{time}: expected{expected:?}, actual{actual:?}");
            }
        }
        // Rotation deltas include axial twist, even when the primary bone
        // direction itself does not change. Check every mapped joint.
        for pair in &map {
            let rotation = |id, time| {
                s.scene.at_time(time, || {
                    s.scene.world_matrix(id).to_scale_rotation_translation().1
                })
            };
            let source_start = rotation(pair.source, clip.start);
            let target_start = rotation(pair.target, clip.start);
            for time in [(clip.start + clip.end) * 0.5, clip.end] {
                let expected = alignment
                    * rotation(pair.source, time)
                    * source_start.inverse()
                    * alignment.inverse();
                let actual = rotation(pair.target, time) * target_start.inverse();
                assert!(
                    expected.dot(actual).abs() > 0.9999,
                    "source quaternion/twist not preserved on {} at frame{time}",
                    s.scene.node(pair.target).unwrap().name
                );
            }
        }
        // Sequence the actual motion a second time on the same native skin.
        let first_keys: Vec<_> = map
            .iter()
            .map(|m| (m.target, s.scene.node(m.target).unwrap().transform.clone()))
            .collect();
        let next_start = clip.end + 10.25;
        crate::execute(
            &mut s,
            crate::Command::MoveAnimationClip {
                id: source,
                start: next_start,
            },
        )
        .unwrap();
        let second = retarget(&mut s, source, target, &map).unwrap()["clip"]
            .as_u64()
            .unwrap();
        crate::execute(
            &mut s,
            crate::Command::MoveAnimationClip {
                id: second,
                start: next_start + clip.end - clip.start + 2.0,
            },
        )
        .unwrap();
        for (ids, expected) in [(&first, clip.start), (&last, clip.end)] {
            let actual = s.scene.at_time(expected, || {
                map.iter()
                    .map(|m| s.scene.world_matrix(m.target))
                    .collect::<Vec<_>>()
            });
            for (before, after) in ids.iter().zip(actual) {
                assert!(
                    before.abs_diff_eq(after, 1e-5),
                    "sequencing changed earlier evaluated pose"
                );
            }
        }
        for _ in 0..3 {
            crate::execute(&mut s, crate::Command::UndoSceneOperation).unwrap();
        }
        for (id, keys) in first_keys {
            assert_eq!(
                s.scene.node(id).unwrap().transform,
                keys,
                "sequence undo did not restore native keys exactly"
            );
        }
        assert!(retarget(&mut s, source, target, &map)
            .unwrap_err()
            .contains("overlap"));
        crate::execute(&mut s, crate::Command::UndoSceneOperation).unwrap();
        for (id, before) in original {
            assert_eq!(
                before,
                serde_json::to_value(s.scene.node(id).unwrap()).unwrap(),
                "undo changed target"
            );
        }
    }
}
