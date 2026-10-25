//! Native timeline edits for the control rig's IK/FK blend curves.
use crate::{AppState, Command};
use loom_scene::{
    anim::{Interp, TangentType},
    Change,
};
use serde_json::{json, Value};

pub fn execute(state: &mut AppState, command: Command) -> Result<Value, String> {
    let (rig, limb, time, value, move_from) = match command {
        Command::SetLoomRigBlendKey {
            rig,
            limb,
            time,
            value,
        } => (rig, limb, time, value, None),
        Command::MoveLoomRigBlendKey {
            rig,
            limb,
            from,
            to,
        } => (rig, limb, to, None, Some(from)),
        _ => return Err("not a LoomRig key command".into()),
    };
    if !time.is_finite()
        || (time as f64) < i32::MIN as f64
        || (time as f64) > i32::MAX as f64
        || value.is_some_and(|v| !v.is_finite() || !(0.0..=1.0).contains(&v))
        || move_from.is_some_and(|v| !v.is_finite())
    {
        return Err("invalid IK/FK key time or value".into());
    }
    let before = state.scene.loom_rigs.clone();
    let mut after = before.clone();
    let limb = after
        .iter_mut()
        .find(|r| r.id == rig)
        .ok_or("LoomRig not found")?
        .limbs
        .iter_mut()
        .find(|l| l.mapping.name == limb)
        .ok_or("LoomRig limb not found")?;
    let keys = limb
        .blend
        .ensure_keys()
        .ok_or("IK/FK controller cannot be keyed")?;
    let track = &mut keys.tracks[0];
    if let Some(from) = move_from {
        let index = track.key_at(from).ok_or("IK/FK key not found")?;
        if from == time {
            return Ok(json!({}));
        }
        if track.key_at(time).is_some() {
            return Err("another IK/FK key already occupies the destination".into());
        }
        track.keys[index].time = time;
        track.sort();
    } else if let Some(value) = value {
        if track.keys.is_empty() {
            track.interp = Interp::Linear;
        }
        track.set_key(time, value, (TangentType::Linear, TangentType::Linear));
    } else {
        let index = track.key_at(time).ok_or("IK/FK key not found")?;
        track.keys.remove(index);
    }
    if after == before {
        return Ok(json!({}));
    }
    crate::execute::commit_exact(
        state,
        "Edit LoomRig IK/FK Key",
        Change::SetLoomRigs { before, after },
    )?;
    Ok(json!({"rig":rig,"time":time}))
}
