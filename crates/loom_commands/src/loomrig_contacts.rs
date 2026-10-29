//! Atomic contact metadata edits, independent of imported bone controllers.
use crate::{AppState, Command};
use glam::Vec3;
use loom_scene::loomrig_contact::{ContactKey, ContactReference, ContactSpace};
use loom_scene::{Change, Scene};
use serde_json::{json, Value};

fn valid_time(time: f32) -> Result<(), String> {
    if !time.is_finite() || (time as f64) < i32::MIN as f64 || (time as f64) > i32::MAX as f64 {
        return Err("contact key time must be finite and within the timeline range".into());
    }
    Ok(())
}

fn capture(
    scene: &Scene,
    rig: &loom_scene::loomrig::LoomRig,
    limb: &loom_scene::loomrig::RigLimb,
    time: f32,
    space: ContactSpace,
) -> Result<ContactReference, String> {
    scene.at_time(time, || {
        let start = scene.world_matrix(limb.mapping.start).w_axis.truncate();
        let middle = scene.world_matrix(limb.mapping.middle).w_axis.truncate();
        let end = scene.world_matrix(limb.mapping.end);
        let axis = (end.w_axis.truncate() - start).normalize_or_zero();
        let mut bend = middle - start - axis * (middle - start).dot(axis);
        if bend.length_squared() < 1e-8 {
            bend = scene.world_matrix(limb.pole).w_axis.truncate() - start;
            bend -= axis * bend.dot(axis);
            if bend.length_squared() < 1e-8 {
                bend = Vec3::Y - axis * Vec3::Y.dot(axis);
                if bend.length_squared() < 1e-8 {
                    bend = Vec3::Z - axis * Vec3::Z.dot(axis);
                }
            }
        }
        let pole = middle
            + bend.normalize_or_zero()
                * (start.distance(middle) + middle.distance(end.w_axis.truncate())).max(1.);
        ContactReference::capture(space, scene.world_matrix(rig.mapping.root), end, pole)
    })
}

pub fn execute(state: &mut AppState, command: Command) -> Result<Value, String> {
    let (rig_id, name) = match &command {
        Command::CreateLoomRigContactKey { rig, limb, .. }
        | Command::EditLoomRigContactKey { rig, limb, .. }
        | Command::EditLoomRigContactReference { rig, limb, .. }
        | Command::MoveLoomRigContactKey { rig, limb, .. }
        | Command::DeleteLoomRigContactKey { rig, limb, .. } => (*rig, limb.clone()),
        _ => return Err("not a LoomRig contact command".into()),
    };
    let before = state.scene.loom_rigs.clone();
    let ri = before
        .iter()
        .position(|rig| rig.id == rig_id)
        .ok_or("LoomRig not found")?;
    let li = before[ri]
        .limbs
        .iter()
        .position(|limb| limb.mapping.name == name)
        .ok_or("LoomRig limb not found")?;
    let mut after = before.clone();
    let track = &mut after[ri].limbs[li].contacts;
    let mut reference_id = None;
    match command {
        Command::CreateLoomRigContactKey { time, preset, .. } => {
            valid_time(time)?;
            let (space, blend) = match preset.as_str() {
                "planted" | "sliding" => (ContactSpace::World, 1.),
                "free" => (ContactSpace::Body, 0.),
                _ => return Err("contact preset must be planted, sliding or free".into()),
            };
            if track.keys().iter().any(|key| key.time == time) {
                return Err("contact key time already exists".into());
            }
            let pose = capture(
                &state.scene,
                &before[ri],
                &before[ri].limbs[li],
                time,
                space,
            )?;
            let joined = if preset == "planted" {
                track
                    .keys()
                    .iter()
                    .rev()
                    .find(|key| key.time < time)
                    .filter(|key| key.blend > 0.)
                    .and_then(|key| {
                        let previous = &track.references()[&key.reference];
                        let same_rotation = previous.rotation.dot(pose.rotation).abs() > 1. - 1e-6;
                        (previous.space == ContactSpace::World
                            && previous.position.abs_diff_eq(pose.position, 1e-3)
                            && previous.pole_position.abs_diff_eq(pose.pole_position, 1e-3)
                            && same_rotation)
                            .then_some(key.reference)
                    })
            } else {
                None
            };
            let reference = if let Some(id) = joined {
                id
            } else {
                let id = track
                    .references()
                    .last_key_value()
                    .map_or(Some(1), |(id, _)| id.checked_add(1))
                    .ok_or("contact reference IDs exhausted")?;
                track.insert_reference(id, pose)?;
                id
            };
            track.insert_key(ContactKey {
                time,
                reference,
                blend,
            })?;
            reference_id = Some(reference);
        }
        Command::EditLoomRigContactKey { key, .. } => {
            valid_time(key.time)?;
            track.update_key(key)?;
        }
        Command::EditLoomRigContactReference {
            reference, pose, ..
        } => {
            track.update_reference(reference, pose)?;
            reference_id = Some(reference);
        }
        Command::MoveLoomRigContactKey { from, to, .. } => {
            valid_time(from)?;
            valid_time(to)?;
            track.move_key(from, to)?;
        }
        Command::DeleteLoomRigContactKey { time, .. } => {
            valid_time(time)?;
            track.remove_key(time)?;
        }
        _ => unreachable!(),
    }
    if before != after {
        crate::execute::commit_exact(
            state,
            "Edit LoomRig Contact",
            Change::SetLoomRigs { before, after },
        )?;
    }
    Ok(json!({"rig":rig_id,"limb":name,"reference":reference_id}))
}
