//! Move a source clip's native keys as one reversible edit.
use crate::AppState;
use loom_scene::{param::Animatable, tracks::{self, AnyParam}, Change, Controller};
use serde_json::{json, Value};
use std::collections::BTreeSet;

fn shift<T: Animatable>(controller: &mut Controller<T>, delta: f32, range:Option<(f32,f32)>) -> Result<(), String> {
    match controller {
        Controller::Keys(k) => for track in &mut k.tracks {
            if range.is_some() && !track.keys.is_empty() && track.interp!=loom_scene::anim::Interp::Linear {
                return Err("moving shared clip nodes requires linear native keys".into());
            }
            for key in &mut track.keys {
                if !key.time.is_finite() {return Err("animation key time overflow".into());}
                if let Some((start,end))=range {
                    if key.time<start || key.time>end {
                        if key.time>=start+delta && key.time<=end+delta {return Err("clip destination contains authored keys outside its ownership".into());}
                        continue;
                    }
                }
                let time = key.time + delta;
                if !time.is_finite() { return Err("animation key time overflow".into()); }
                key.time = time;
            }
            if range.is_some() {
                track.keys.sort_by(|a,b|a.time.total_cmp(&b.time));
                if track.keys.windows(2).any(|keys|keys[0].time>=keys[1].time) {return Err("clip move collides with existing keys".into());}
            }
        },
        Controller::List(items) => for item in items { shift(&mut item.controller, delta,range)?; },
        _ => {},
    }
    Ok(())
}

pub(crate) fn move_clip(state: &mut AppState, id: u64, start: f32) -> Result<Value, String> {
    if !start.is_finite() { return Err("clip start must be finite".into()); }
    let clips = &state.scene.animation_clips;
    let index = clips.iter().position(|c| c.id == id).ok_or_else(|| format!("no animation clip {id}"))?;
    if clips.iter().filter(|c| c.id == id).count() != 1 { return Err("duplicate animation clip ID".into()); }
    let clip = &clips[index];
    if !clip.start.is_finite() || !clip.end.is_finite() || clip.end < clip.start { return Err("invalid animation clip range".into()); }
    let delta = start - clip.start;
    let end = clip.end + delta;
    if !delta.is_finite() || !end.is_finite() || (start.floor() as f64) < i32::MIN as f64 || (end.ceil() as f64) > i32::MAX as f64 {
        return Err("animation clip exceeds timeline frame limits".into());
    }
    let members: BTreeSet<_> = clip.nodes.iter().copied().collect();
    if members.is_empty() || members.len() != clip.nodes.len() { return Err("animation clip must have unique node members".into()); }
    for (i,other) in clips.iter().enumerate() {
        if i==index || !other.nodes.iter().any(|n|members.contains(n)) {continue;}
        if !other.start.is_finite() || !other.end.is_finite() || other.end<other.start {return Err("invalid shared clip range".into());}
        if clip.start<=other.end && clip.end>=other.start || start<=other.end && end>=other.start {
            return Err("shared clip ranges overlap or touch".into());
        }
    }
    let mut changes = Vec::new();
    for id in &clip.nodes {
        let node = state.scene.node(*id).ok_or_else(|| format!("animation clip member {} is missing", id.0))?;
        let range=clips.iter().enumerate().any(|(i,c)|i!=index && c.nodes.contains(id)).then_some((clip.start,clip.end));
        let mut after = node.clone();
        let all = tracks::node_tracks(node);
        let mut error = None;
        tracks::edit_node(&mut after, &all, |_, p| {
            let result = match p {
                AnyParam::F32(p) => shift(&mut p.controller, delta,range),
                AnyParam::U32(p) => shift(&mut p.controller, delta,range),
                AnyParam::Bool(p) => shift(&mut p.controller, delta,range),
                AnyParam::Vec3(p) => shift(&mut p.controller, delta,range),
            };
            if let Err(e) = result { error = Some(e); }
        })?;
        if let Some(e) = error { return Err(e); }
        for p in [&mut after.object_offset.position, &mut after.object_offset.rotation, &mut after.object_offset.scale] {
            shift(&mut p.controller, delta,range)?;
        }
        if let Some(c) = crate::anim_cmds::node_change(node, &after) { changes.push(c); }
        if node.visibility != after.visibility {
            changes.push(Change::SetVisibility { id: *id, before: node.visibility.clone(), after: after.visibility.clone() });
        }
        if node.object_offset != after.object_offset {
            changes.push(Change::SetNodeProps { id: *id, before: Box::new(node.props()), after: Box::new(after.props()) });
        }
    }
    if delta == 0.0 { return Ok(json!({ "clip": id, "start": start, "end": end })); }
    let before = clips.clone();
    let mut after = before.clone();
    after[index].start = start;
    after[index].end = end;
    changes.push(Change::SetAnimationClips { before, after });
    while state.undo.in_transaction() { state.undo.end(); }
    let timing = crate::timing_undo::snap(state);
    crate::execute::commit_exact(state, "Move Animation Clip", Change::Group(changes))?;
    state.frame_range.0 = state.frame_range.0.min(start.floor() as i32);
    state.frame_range.1 = state.frame_range.1.max(end.ceil() as i32);
    crate::timing_undo::record(state, timing);
    Ok(json!({ "clip": id, "start": start, "end": end }))
}

