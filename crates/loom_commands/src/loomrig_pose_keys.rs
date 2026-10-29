//! Atomic timeline edits of full-rig pose events and their actual animation keys.
use crate::AppState;
use loom_scene::{Change, tracks::{self, AnyParam, TrackOwner}};
use loom_scene::param::Controller;
use serde_json::{json, Value};

fn edit_controller<T>(controller:&mut Controller<T>,from:f32,to:Option<f32>)->Result<bool,String> {
    match controller {
        Controller::Keys(keys)=> {
            let mut changed=false;
            for track in &mut keys.tracks {
                let Some(index)=track.key_at(from) else {continue};
                if let Some(to)=to {
                    if track.key_at(to).is_some_and(|other|other != index) {return Err("rig pose target collides with an existing channel key".into());}
                    track.keys[index].time=to;track.sort();
                } else {track.keys.remove(index);}
                changed=true;
            }
            Ok(changed)
        }
        Controller::List(items)=> {
            let mut changed=false;
            for item in items {changed |= edit_controller(&mut item.controller,from,to)?;}
            Ok(changed)
        }
        _=>Ok(false),
    }
}
fn edit_param(param:&mut AnyParam,from:f32,to:Option<f32>)->Result<bool,String> {
    match param {
        AnyParam::F32(p)=>edit_controller(&mut p.controller,from,to),
        AnyParam::Vec3(p)=>edit_controller(&mut p.controller,from,to),
        AnyParam::Bool(p)=>edit_controller(&mut p.controller,from,to),
        AnyParam::U32(p)=>edit_controller(&mut p.controller,from,to),
    }
}

pub(crate) fn edit(state:&mut AppState,rig_id:u32,from:f32,to:Option<f32>)->Result<Value,String> {
    let valid=|time:f32|time.is_finite() && time as f64 >= i32::MIN as f64 && time as f64 <= i32::MAX as f64;
    if !valid(from) || to.is_some_and(|to|!valid(to)) {return Err("rig pose key times must be finite and within the timeline range".into());}
    let before=state.scene.loom_rigs.clone();
    let mut after=before.clone();
    let rig=after.iter_mut().find(|rig|rig.id == rig_id).ok_or("LoomRig not found")?;
    if let Some(to)=to {rig.pose_frames.move_key(from,to)?;} else {rig.pose_frames.remove(from)?;}
    if to == Some(from) {return Ok(json!({"changed":0}));}
    let owned:std::collections::BTreeSet<_>=rig.bindings.iter().map(|binding|binding.control)
        .chain([rig.master]).chain(rig.limbs.iter().flat_map(|limb|[limb.goal,limb.pole])).collect();
    let mut changes=Vec::new();
    for id in owned {
        let node=state.scene.node(id).ok_or("rig control is missing")?;
        if node.object.is_bone() {return Err("rig controls must not overwrite source bone tracks".into());}
        let mut edited=node.clone();
        let mut edits=Vec::new();
        for track in tracks::node_tracks(node).into_iter().filter(|track|track.owner == TrackOwner::Transform) {
            let mut param=tracks::read_node(node,&track).ok_or("rig control track is missing")?;
            if edit_param(&mut param,from,to)? {
                if state.scene.track_data.flags.iter().any(|(key,flags)|*key == track.key() && (flags.locked || flags.not_keyable)) {
                    return Err("rig pose contains a protected track; unlock it before editing".into());
                }
                edits.push((track,param));
            }
        }
        for (track,param) in edits {tracks::edit_node(&mut edited,&[track],|_,value|*value=param.clone())?;}
        if let Some(change)=crate::anim_cmds::node_change(node,&edited) {changes.push(change);}
    }
    for limb in &mut rig.limbs {
        edit_controller(&mut limb.blend.controller,from,to)?;
        for offset in limb.pose_offsets.iter_mut().take(3) {edit_controller(&mut offset.controller,from,to)?;}
        if limb.pose_matches.keys().iter().any(|key|key.time == from) {
            if let Some(to)=to {limb.pose_matches.move_key(from,to)?;} else {limb.pose_matches.remove_key(from);}
        }
        if limb.contacts.keys().iter().any(|key|key.time == from) {
            if let Some(to)=to {limb.contacts.move_key(from,to)?;} else {limb.contacts.remove_key(from)?;}
        }
    }
    changes.push(Change::SetLoomRigs {before,after});
    let change=Change::Group(changes);
    crate::xref::guard(&state.scene,&change)?;
    crate::execute::commit_exact(state,if to.is_some() {"Move Rig Pose Key"} else {"Delete Rig Pose Key"},change)?;
    Ok(json!({"rig":rig_id,"from":from,"to":to}))
}
