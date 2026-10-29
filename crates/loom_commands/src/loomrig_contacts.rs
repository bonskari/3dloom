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

fn capture_fk_match(scene:&Scene, ri:usize, li:usize, time:f32) -> Result<[glam::Mat4;3],String> {
    let limb = &scene.loom_rigs[ri].limbs[li];
    let ids = [limb.mapping.start,limb.mapping.middle,limb.mapping.end];
    let visible = scene.at_time(time,||ids.map(|id|scene.local_matrix(id)));
    let mut probe = scene.clone();
    let fk = &mut probe.loom_rigs[ri].limbs[li];
    fk.contacts = Default::default();
    fk.pose_matches = Default::default();
    fk.blend = loom_scene::Param::new(0.);
    probe.bump_version();
    probe.at_time(time,|| {
        let mut offsets = [glam::Mat4::IDENTITY;3];
        for i in 0..3 {
            offsets[i] = probe.local_matrix(ids[i]).inverse() * visible[i];
            loom_scene::loomrig_contact::affine_rotation(offsets[i])?;
        }
        Ok(offsets)
    })
}

pub(crate) fn set_active_limb(
    state:&mut AppState, ri:usize, li:usize, solver:loom_scene::ik::IkSolver, blend:f32,
) -> Result<Option<Value>,String> {
    let old = &state.scene.loom_rigs[ri].limbs[li];
    let Some(control) = state.scene.contact_control(old.goal)? else {return Ok(None)};
    let time = state.current_frame as f32;
    let before = state.scene.loom_rigs.clone();
    let mut after = before.clone();
    let limb = &mut after[ri].limbs[li];
    limb.solver = solver;
    if state.auto_key {
        key_contact_weight(&mut limb.contacts,time,blend,&control.sample,state.scene.world_matrix(before[ri].mapping.root))?;
    } else {
        limb.contacts.set_blend(time,blend)?;
    }
    if blend == 0. && control.sample.blend > 0. {
        let offsets = capture_fk_match(&state.scene,ri,li,time)?;
        if limb.contacts.keys().is_empty() && limb.pose_matches.keys().is_empty() {
            limb.pose_matches = loom_scene::loomrig_pose::RigPoseTrack::constant(offsets)?;
        } else {
            let baseline = limb.contacts.keys().iter().rev().find(|key|key.time < time).map(|key|key.time)
                .or_else(||(time > 0.).then_some(0.));
            if let Some(baseline) = baseline {
                if !limb.pose_matches.keys().iter().any(|key|key.time == baseline) {
                    limb.pose_matches.set_key(loom_scene::loomrig_pose::RigPoseKey {
                        time:baseline,offsets:limb.pose_matches.sample(baseline)?,
                    })?;
                }
            }
            limb.pose_matches.set_key(loom_scene::loomrig_pose::RigPoseKey {time,offsets})?;
        }
    }
    if before != after {
        crate::execute::commit_exact(state,"Set LoomRig Limb",Change::SetLoomRigs {before,after})?;
    }
    Ok(Some(json!({})))
}

fn key_contact_weight(
    track:&mut loom_scene::loomrig_contact::ContactTrack,time:f32,blend:f32,
    sample:&loom_scene::loomrig_contact::ContactSample,body_frame:glam::Mat4,
) -> Result<(),String> {
    let references = track.active_references(time)?;
    let reference = if references.len() == 1 {references[0]} else {
        let id = track.references().last_key_value().and_then(|(id,_)|id.checked_add(1)).ok_or("contact reference IDs exhausted")?;
        let first_space = &track.references()[&references[0]].space;
        let space = if references.iter().all(|id|track.references()[id].space == *first_space) {first_space.clone()} else {ContactSpace::World};
        let pose = ContactReference::capture(space,body_frame,
            glam::Mat4::from_rotation_translation(sample.rotation,sample.position),sample.pole_position)?;
        track.insert_reference(id,pose)?;
        id
    };
    if track.active_key(time)?.is_none() && time > 0. {
        if let Some(state) = track.constant_state().cloned() {
            track.insert_key(ContactKey {time:0.,reference:state.reference,blend:state.blend})?;
        }
    }
    let key = ContactKey {time,reference,blend};
    if track.keys().iter().any(|key|key.time == time) {track.update_key(key)} else {track.insert_key(key)}
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
    let mut free_match = None;
    let mut moved_match = None;
    let mut deleted_match = None;
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
            if preset == "free" {
                let offsets = capture_fk_match(&state.scene,ri,li,time)?;
                let baseline = track.keys().iter().rev().find(|key|key.time < time).map(|key|key.time)
                    .or_else(||(time > 0.).then_some(0.));
                free_match = Some((time,offsets,baseline));
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
            moved_match = Some((from,to));
        }
        Command::DeleteLoomRigContactKey { time, .. } => {
            valid_time(time)?;
            track.remove_key(time)?;
            deleted_match = Some(time);
        }
        _ => unreachable!(),
    }
    if let Some((from,to)) = moved_match {after[ri].limbs[li].pose_matches.move_key(from,to)?;}
    if let Some(time) = deleted_match {after[ri].limbs[li].pose_matches.remove_key(time);}
    if let Some((time,offsets,baseline)) = free_match {
        let matches = &mut after[ri].limbs[li].pose_matches;
        if let Some(baseline) = baseline {
            if !matches.keys().iter().any(|key|key.time == baseline) {
                matches.set_key(loom_scene::loomrig_pose::RigPoseKey {time:baseline,offsets:matches.sample(baseline)?})?;
            }
        }
        matches.set_key(loom_scene::loomrig_pose::RigPoseKey {time,offsets})?;
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
