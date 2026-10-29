//! Key commands: Auto / Set Key, Key Filters, Key Position…, default
//! tangents, track bar edits, playback, Re-scale Time and animation clips.

use crate::command::Command;
use crate::execute::{commit_exact as commit, set_animation_time, set_frame};
use crate::state::AppState;
use loom_scene::anim::{self, TangentType};
use loom_scene::tracks::{self, AnyParam, TrackOwner, TrackPath};
use loom_scene::{Change, Node, NodeId};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

/// Key Filters: which tracks Set Keys keys. Defaults: Position,
/// Rotation, Scale and IK Parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyFilters {
    pub position: bool,
    pub rotation: bool,
    pub scale: bool,
    pub ik_parameters: bool,
    pub object_params: bool,
    pub custom_attributes: bool,
    pub modifiers: bool,
    pub materials: bool,
    pub other: bool,
}

impl Default for KeyFilters {
    fn default() -> Self {
        Self { position: true, rotation: true, scale: true, ik_parameters: true, object_params: false, custom_attributes: false, modifiers: false, materials: false, other: false }
    }
}

impl KeyFilters {
    fn all(&self) -> bool {
        self.position && self.rotation && self.scale && self.ik_parameters && self.object_params && self.custom_attributes && self.modifiers && self.materials && self.other
    }

    pub fn allows(&self, t: &TrackPath) -> bool {
        match t.owner {
            TrackOwner::Transform => match t.pointer.as_str() {
                "/position" => self.position,
                "/rotation" => self.rotation,
                _ => self.scale,
            },
            TrackOwner::Object => self.object_params,
            TrackOwner::Modifier(_) => self.modifiers,
            TrackOwner::Constraints | TrackOwner::Visibility => self.other,
        }
    }
}

/// Track bar right-click > Filter: which keys the track bar shows and edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TrackBarFilter {
    #[default]
    AllKeys,
    AllTransformKeys,
    CurrentTransform,
    Object,
    Material,
}

/// Whether the track bar filter lets a track through.
pub fn track_bar_allows(state: &AppState, t: &TrackPath) -> bool {
    use crate::state::Tool;
    match state.track_bar_filter {
        TrackBarFilter::AllKeys => true,
        TrackBarFilter::AllTransformKeys => t.owner == TrackOwner::Transform,
        TrackBarFilter::CurrentTransform => {
            t.owner == TrackOwner::Transform
                && match state.active_tool {
                    Tool::Move => t.pointer == "/position",
                    Tool::Rotate => t.pointer == "/rotation",
                    Tool::Scale => t.pointer == "/scale",
                    Tool::Select => true,
                }
        }
        TrackBarFilter::Object => matches!(t.owner, TrackOwner::Object | TrackOwner::Modifier(_)),
        // Loom has no material parameters to animate.
        TrackBarFilter::Material => false,
    }
}

/// Set Key mode's pose: the history position and node versions it started from.
#[derive(Debug, Clone)]
pub struct SetKeyBuffer {
    pub position: u64,
    pub versions: HashMap<NodeId, (u64, u64)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PlayDirection {
    #[default]
    Forward,
    Reverse,
    PingPong,
}

/// Time Configuration > Playback.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Playback {
    pub real_time: bool,
    pub loop_playback: bool,
    pub speed: f32,
    pub direction: PlayDirection,
    /// Subframe preview offset while Real Time plays; the timeline stays integer.
    pub accum: f32,
    /// Ping Pong: +1 forward, -1 back.
    pub sign: i32,
}

impl Default for Playback {
    fn default() -> Self {
        Self { real_time: true, loop_playback: true, speed: 1.0, direction: PlayDirection::Forward, accum: 0.0, sign: 1 }
    }
}

pub fn handles(c: &Command) -> bool {
    use Command as C;
    matches!(
        c,
        C::AutoKeyModeToggle
            | C::ForwardTimeOneUnit
            | C::BackupTimeOneUnit
            | C::SetKeyMode
            | C::SetKeys
            | C::SetKeySelectionList { .. }
            | C::SetKeySelected
            | C::ClearSetKeyModeBuffer
            | C::KeyAll
            | C::KeyPosition
            | C::KeyPositionX
            | C::KeyPositionY
            | C::KeyPositionZ
            | C::KeyRotation
            | C::KeyRotationX
            | C::KeyRotationY
            | C::KeyRotationZ
            | C::KeyScale
            | C::KeyScaleX
            | C::KeyScaleY
            | C::KeyScaleZ
            | C::KeyPositionRotationScale
            | C::CreateKey { .. }
            | C::CreatePositionLockKey
            | C::CreateRotationLockKey
            | C::SetKeyAllFilter
            | C::SetKeyPositionFilter
            | C::SetKeyRotationFilter
            | C::SetKeyScaleFilter
            | C::SetKeyIkParametersFilter
            | C::SetKeyObjectParamsFilter
            | C::SetKeyCustomAttributesFilter
            | C::SetKeyModifiersFilter
            | C::SetKeyMaterialsFilter
            | C::SetKeyOtherFilter
            | C::SetDefaultTangentAsAuto
            | C::SetDefaultTangentAsFast
            | C::SetDefaultTangentAsLinear
            | C::SetDefaultTangentAsSlow
            | C::SetDefaultTangentAsSmooth
            | C::SetDefaultTangentAsSpline
            | C::SetDefaultTangentAsStepped
            | C::SetDefaultTangents { .. }
            | C::PlayAnimation
            | C::NextPreviousKeyModeToggle
            | C::GoToNextKey
            | C::GoToPreviousKey
            | C::DeleteKeys { .. }
            | C::MoveKeysInTime { .. }
            | C::CopyKeys { .. }
            | C::SetPlayback { .. }
            | C::RescaleTime { .. }
            | C::TickPlayback { .. }
            | C::SaveAnimation { .. }
            | C::LoadAnimation { .. }
            | C::SetTrackBarFilter { .. }
            | C::ShowTrackBarToggle
            | C::ShowTrajectoriesToggle
            | C::DeleteSelectedAnimation
    )
}

pub fn execute(state: &mut AppState, cmd: Command) -> Result<Value, String> {
    use Command as C;
    let pos = |t: &TrackPath, p: &str| t.owner == TrackOwner::Transform && t.pointer == p;
    match cmd {
        C::AutoKeyModeToggle => {
            state.auto_key = !state.auto_key;
            if state.auto_key && state.set_key_mode {
                discard_pose(state);
                state.set_key_mode = false;
                state.set_key_buffer = None;
            }
            Ok(json!({ "auto_key": state.auto_key }))
        }
        C::SetKeyMode => {
            if state.set_key_mode {
                discard_pose(state);
                state.set_key_mode = false;
                state.set_key_buffer = None;
            } else {
                state.set_key_mode = true;
                state.auto_key = false;
                start_pose(state);
            }
            Ok(json!({ "set_key_mode": state.set_key_mode }))
        }
        C::ClearSetKeyModeBuffer => {
            discard_pose(state);
            if state.set_key_mode {
                start_pose(state);
            }
            Ok(json!({}))
        }
        C::SetKeys => {
            // The Selection List's named set stands in for the selection while keying.
            let Some(list) = state.key_selection_list.clone() else { return set_keys(state) };
            let ids = state.named_sets.get(&list).cloned().ok_or_else(|| format!("no named selection set \"{list}\""))?;
            let saved = std::mem::replace(&mut state.selection, ids.into_iter().collect());
            let r = set_keys(state);
            state.selection = saved;
            r
        }
        C::SetKeySelectionList { name } => {
            if let Some(n) = &name {
                if !state.named_sets.contains_key(n) {
                    return Err(format!("no named selection set \"{n}\""));
                }
            }
            state.key_selection_list = name;
            Ok(json!({ "selection_list": state.key_selection_list }))
        }
        C::SetKeySelected => {
            let f = state.key_filters;
            key_selection(state, "Set Key Selected", |t| f.allows(t).then_some(None))
        }
        C::KeyAll => key_selection(state, "Key All", |_| Some(None)),
        C::KeyPosition => key_selection(state, "Key Position", |t| pos(t, "/position").then_some(None)),
        C::KeyPositionX => key_selection(state, "Key Position X", |t| pos(t, "/position").then_some(Some(0))),
        C::KeyPositionY => key_selection(state, "Key Position Y", |t| pos(t, "/position").then_some(Some(1))),
        C::KeyPositionZ => key_selection(state, "Key Position Z", |t| pos(t, "/position").then_some(Some(2))),
        C::KeyRotation => key_selection(state, "Key Rotation", |t| pos(t, "/rotation").then_some(None)),
        C::KeyRotationX => key_selection(state, "Key Rotation X", |t| pos(t, "/rotation").then_some(Some(0))),
        C::KeyRotationY => key_selection(state, "Key Rotation Y", |t| pos(t, "/rotation").then_some(Some(1))),
        C::KeyRotationZ => key_selection(state, "Key Rotation Z", |t| pos(t, "/rotation").then_some(Some(2))),
        C::KeyScale => key_selection(state, "Key Scale", |t| pos(t, "/scale").then_some(None)),
        C::KeyScaleX => key_selection(state, "Key Scale X", |t| pos(t, "/scale").then_some(Some(0))),
        C::KeyScaleY => key_selection(state, "Key Scale Y", |t| pos(t, "/scale").then_some(Some(1))),
        C::KeyScaleZ => key_selection(state, "Key Scale Z", |t| pos(t, "/scale").then_some(Some(2))),
        C::KeyPositionRotationScale => key_selection(state, "Key Position, Rotation, Scale", |t| (t.owner == TrackOwner::Transform).then_some(None)),
        C::CreateKey { source, destination, position, rotation, scale } => {
            let src = source.unwrap_or(state.current_frame) as f32;
            let dst = destination.unwrap_or(state.current_frame) as f32;
            let want = |t: &TrackPath| match t.pointer.as_str() {
                "/position" => position.unwrap_or(true),
                "/rotation" => rotation.unwrap_or(true),
                _ => scale.unwrap_or(true),
            };
            let tangents = state.default_tangent;
            edit_selection(state, "Create Key", |t| t.owner == TrackOwner::Transform && want(t), |_, a| {
                let c = a.comps_at(src);
                if let Some(k) = a.ensure_keys() {
                    for (tr, v) in k.tracks.iter_mut().zip(c) {
                        tr.set_key(dst, v, tangents);
                    }
                }
            })
        }
        C::CreatePositionLockKey => lock_key(state, "/position", "Create Position Lock Key"),
        C::CreateRotationLockKey => lock_key(state, "/rotation", "Create Rotation Lock Key"),
        C::SetKeyAllFilter => {
            let on = !state.key_filters.all();
            state.key_filters = KeyFilters { position: on, rotation: on, scale: on, ik_parameters: on, object_params: on, custom_attributes: on, modifiers: on, materials: on, other: on };
            Ok(filters_json(state))
        }
        C::SetKeyPositionFilter => flip(state, |f| &mut f.position),
        C::SetKeyRotationFilter => flip(state, |f| &mut f.rotation),
        C::SetKeyScaleFilter => flip(state, |f| &mut f.scale),
        C::SetKeyIkParametersFilter => flip(state, |f| &mut f.ik_parameters),
        C::SetKeyObjectParamsFilter => flip(state, |f| &mut f.object_params),
        C::SetKeyCustomAttributesFilter => flip(state, |f| &mut f.custom_attributes),
        C::SetKeyModifiersFilter => flip(state, |f| &mut f.modifiers),
        C::SetKeyMaterialsFilter => flip(state, |f| &mut f.materials),
        C::SetKeyOtherFilter => flip(state, |f| &mut f.other),
        C::SetDefaultTangentAsAuto => default_tangent(state, TangentType::Auto),
        C::SetDefaultTangentAsFast => default_tangent(state, TangentType::Fast),
        C::SetDefaultTangentAsLinear => default_tangent(state, TangentType::Linear),
        C::SetDefaultTangentAsSlow => default_tangent(state, TangentType::Slow),
        C::SetDefaultTangentAsSmooth => default_tangent(state, TangentType::Smooth),
        C::SetDefaultTangentAsSpline => default_tangent(state, TangentType::Custom),
        C::SetDefaultTangentAsStepped => default_tangent(state, TangentType::Step),
        C::SetDefaultTangents { tangent_in, tangent_out } => {
            state.default_tangent = (tangent_in.unwrap_or(state.default_tangent.0), tangent_out.unwrap_or(state.default_tangent.1));
            anim::set_default_tangents(state.default_tangent);
            Ok(state_json(state))
        }
        C::PlayAnimation => {
            state.playing = !state.playing;
            state.playback.accum = 0.0;
            state.playback.sign = 1;
            Ok(json!({ "playing": state.playing }))
        }
        C::NextPreviousKeyModeToggle => {
            state.key_step_mode = !state.key_step_mode;
            Ok(json!({ "key_mode": state.key_step_mode }))
        }
        C::ForwardTimeOneUnit if state.key_step_mode => step_key(state, 1),
        C::BackupTimeOneUnit if state.key_step_mode => step_key(state, -1),
        C::ForwardTimeOneUnit => set_frame(state, state.current_frame + 1),
        C::BackupTimeOneUnit => set_frame(state, state.current_frame - 1),
        C::GoToNextKey => step_key(state, 1),
        C::GoToPreviousKey => step_key(state, -1),
        C::DeleteKeys { from, to } => delete_keys(state, from, to),
        C::MoveKeysInTime { from, to, delta } => shift_keys(state, from, to, delta, false),
        C::CopyKeys { from, to, delta } => shift_keys(state, from, to, delta, true),
        C::SetPlayback { real_time, loop_playback, speed, direction } => {
            let snap = real_time.is_some_and(|v| v != state.playback.real_time);
            let p = &mut state.playback;
            if let Some(v) = real_time {
                p.real_time = v;
            }
            if let Some(v) = loop_playback {
                p.loop_playback = v;
            }
            if let Some(v) = speed {
                if !(v.is_finite() && v > 0.0) {
                    return Err("speed must be positive".into());
                }
                p.speed = v;
            }
            if let Some(v) = direction {
                p.direction = v;
            }
            if snap {
                state.playback.accum = 0.0;
            }
            Ok(state_json(state))
        }
        C::TickPlayback { seconds } => tick(state, seconds),
        C::RescaleTime { start, end } => rescale(state, start, end),
        C::SaveAnimation { path } => {
            let Some(path) = path else {
                state.dialog = Some(crate::viewport::Dialog::SaveAnimation);
                return Ok(json!({ "dialog": "SaveAnimation" }));
            };
            save_clip(state, &path)
        }
        C::LoadAnimation { path, offset } => {
            let Some(path) = path else {
                state.dialog = Some(crate::viewport::Dialog::LoadAnimation);
                return Ok(json!({ "dialog": "LoadAnimation" }));
            };
            load_clip(state, &path, offset.unwrap_or(0))
        }
        C::SetTrackBarFilter { filter } => {
            state.track_bar_filter = filter;
            Ok(state_json(state))
        }
        C::ShowTrackBarToggle => {
            state.show_track_bar = !state.show_track_bar;
            Ok(json!({ "show_track_bar": state.show_track_bar }))
        }
        C::ShowTrajectoriesToggle => {
            state.show_trajectories = !state.show_trajectories;
            Ok(json!({ "show_trajectories": state.show_trajectories }))
        }
        C::DeleteSelectedAnimation => delete_selected_animation(state),
        _ => Err("not an animation command".into()),
    }
}

/// The keyed tracks of a parameter that has keys (None for constants, Noise, List).
fn keyed(p: &mut AnyParam) -> Option<&mut loom_scene::param::KeyedTracks> {
    if p.keys().is_some_and(|k| k.has_keys()) { p.ensure_keys() } else { None }
}

fn range(a: i32, b: i32) -> (f32, f32) {
    (a.min(b) as f32 - 1e-4, a.max(b) as f32 + 1e-4)
}

pub fn state_json(state: &AppState) -> Value {
    json!({
        "auto_key": state.auto_key,
        "set_key_mode": state.set_key_mode,
        "default_tangent": format!("{:?}", state.default_tangent.0),
        "default_tangent_out": format!("{:?}", state.default_tangent.1),
        "key_filters": serde_json::to_value(state.key_filters).unwrap_or(Value::Null),
        "key_mode": state.key_step_mode,
        "playing": state.playing,
        "real_time": state.playback.real_time,
        "loop": state.playback.loop_playback,
        "speed": state.playback.speed,
        "direction": format!("{:?}", state.playback.direction),
        "track_bar_filter": format!("{:?}", state.track_bar_filter),
        "show_trajectories": state.show_trajectories,
        "frame_range": [state.frame_range.0, state.frame_range.1],
        "fps": state.fps,
        "track_view": {
            "open": state.track_view.open,
            "mode": format!("{:?}", state.track_view.mode),
            "highlighted": state.track_view.highlighted,
            "selected_keys": state.track_view.selected_keys.len(),
        },
    })
}

fn filters_json(state: &AppState) -> Value {
    json!({ "key_filters": serde_json::to_value(state.key_filters).unwrap_or(Value::Null) })
}

fn flip(state: &mut AppState, f: impl FnOnce(&mut KeyFilters) -> &mut bool) -> Result<Value, String> {
    let b = f(&mut state.key_filters);
    *b = !*b;
    Ok(filters_json(state))
}

fn default_tangent(state: &mut AppState, t: TangentType) -> Result<Value, String> {
    state.default_tangent = (t, t);
    anim::set_default_tangents(state.default_tangent);
    Ok(state_json(state))
}

/// The change from `before` to `after` of one node: transform, object, stack.
pub fn node_change(before: &Node, after: &Node) -> Option<Change> {
    let mut steps = Vec::new();
    if before.transform != after.transform {
        steps.push(Change::SetTransform { id: before.id, before: before.transform.clone(), after: after.transform.clone() });
    }
    if before.object != after.object {
        steps.push(Change::ReplaceObject { id: before.id, before: Box::new(before.object.clone()), after: Box::new(after.object.clone()) });
    }
    if before.modifiers != after.modifiers {
        steps.push(Change::SetStack { id: before.id, before: before.modifiers.clone(), after: after.modifiers.clone() });
    }
    if before.constraints != after.constraints {
        steps.push(Change::SetConstraints { id: before.id, before: Box::new(before.constraints.clone()), after: Box::new(after.constraints.clone()) });
    }
    (!steps.is_empty()).then_some(Change::Group(steps))
}

/// Builds selected node-track edits; the caller owns the Undo transaction.
fn node_edits(state: &AppState, ids: &[NodeId], choose: impl Fn(&TrackPath) -> bool, mut f: impl FnMut(&TrackPath, &mut AnyParam)) -> Result<Vec<Change>, String> {
    let mut changes = Vec::new();
    // Locked tracks (Track View > Locked) take no edits.
    let locked: Vec<tracks::TrackKey> = state.scene.track_data.flags.iter().filter(|(_, f)| f.locked).map(|(k, _)| k.clone()).collect();
    for id in ids {
        let Some(n) = state.scene.node(*id) else { continue };
        let chosen: Vec<TrackPath> = tracks::node_tracks(n).into_iter().filter(|t| choose(t) && !locked.contains(&t.key())).collect();
        let mut after = n.clone();
        tracks::edit_node(&mut after, &chosen, &mut f)?;
        if let Some(c) = node_change(n, &after) {
            changes.push(c);
        }
    }
    Ok(changes)
}

/// Edits the chosen tracks of nodes `ids` as one undo entry.
pub fn edit_nodes(state: &mut AppState, label: &str, ids: &[NodeId], choose: impl Fn(&TrackPath) -> bool, f: impl FnMut(&TrackPath, &mut AnyParam)) -> Result<Value, String> {
    let changes = node_edits(state,ids,choose,f)?;
    let n = changes.len();
    if n > 0 { commit(state,label,Change::Group(changes))?; }
    Ok(json!({ "changed":n }))
}

fn controller_has_key<T>(controller:&loom_scene::param::Controller<T>,time:f32)->bool {
    match controller {
        loom_scene::param::Controller::Keys(keys)=>keys.tracks.iter().any(|track|track.key_at(time).is_some()),
        loom_scene::param::Controller::List(items)=>items.iter().any(|item|controller_has_key(&item.controller,time)),
        _=>false,
    }
}

fn delete_selected_animation(state: &mut AppState) -> Result<Value, String> {
    let t = state.current_frame as f32;
    let ids: Vec<_> = state.selection.iter().copied().collect();
    let mut changes = node_edits(state,&ids,|_|true,|_,p| { if p.is_animated() { p.freeze(t); } })?;
    let before = state.scene.loom_rigs.clone();
    let mut after = before.clone();
    for rig in &mut after {
        let owned:Vec<_>=rig.bindings.iter().map(|binding|binding.control).chain([rig.master])
            .chain(rig.limbs.iter().flat_map(|limb|[limb.goal,limb.pole])).collect();
        if owned.iter().all(|id|ids.contains(id)) {
            // Locked node tracks survive freezing; retain their actual grouped keys.
            for time in rig.pose_frames.times().to_vec() {
                let remaining=owned.iter().filter_map(|id|state.scene.node(*id)).any(|node| {
                    tracks::node_tracks(node).iter().any(|track| {
                        track.owner == TrackOwner::Transform
                            && state.scene.track_data.flags.iter().any(|(key,flags)|*key == track.key() && flags.locked)
                            && matches!(tracks::read_node(node,track),Some(AnyParam::Vec3(param)) if controller_has_key(&param.controller,time))
                    })
                });
                if !remaining {rig.pose_frames.remove(time)?;}
            }
        }
        let frame = state.scene.world_matrix(rig.mapping.root);
        for limb in &mut rig.limbs {
            let bones = [limb.mapping.start,limb.mapping.middle,limb.mapping.end];
            let selected = ids.contains(&limb.goal) || ids.contains(&limb.pole)
                || rig.bindings.iter().any(|binding|bones.contains(&binding.bone) && ids.contains(&binding.control));
            if selected {
                limb.contacts = limb.contacts.frozen(t,frame).map_err(|error|format!("LoomRig {} {}: {error}",rig.id,limb.mapping.name))?;
                limb.pose_matches = limb.pose_matches.frozen(t)?;
                limb.blend = loom_scene::Param::new(limb.blend.get());
                for offset in &mut limb.pose_offsets { *offset = loom_scene::Param::new(offset.get()); }
            }
        }
    }
    if before != after { changes.push(Change::SetLoomRigs { before,after }); }
    let n = changes.len();
    if n > 0 { commit(state,"Delete Selected Animation",Change::Group(changes))?; }
    Ok(json!({ "changed":n }))
}

fn edit_selection(state: &mut AppState, label: &str, choose: impl Fn(&TrackPath) -> bool, f: impl FnMut(&TrackPath, &mut AnyParam)) -> Result<Value, String> {
    let ids: Vec<NodeId> = state.selection.iter().copied().collect();
    edit_nodes(state, label, &ids, choose, f)
}

/// Keys the chosen tracks (Some(None) = every component, Some(Some(i)) = one).
fn key_selection(state: &mut AppState, label: &str, choose: impl Fn(&TrackPath) -> Option<Option<usize>>) -> Result<Value, String> {
    use std::collections::BTreeSet;
    let unkeyable = unkeyable(state);
    let locked: Vec<tracks::TrackKey> = state.scene.track_data.flags.iter().filter(|(_, f)| f.locked).map(|(k, _)| k.clone()).collect();
    let mut contact_goals = BTreeSet::new();
    let mut contact_poles = BTreeSet::new();
    for rig in &state.scene.loom_rigs {
        for limb in &rig.limbs {
            if state.scene.contact_control(limb.goal)?.is_some() {
                contact_goals.insert(limb.goal);
                contact_poles.insert(limb.pole);
            }
        }
    }
    let ids: Vec<NodeId> = state.selection.iter().copied().collect();
    let mut changes = node_edits(
        state,
        &ids,
        |t| {
            !unkeyable.contains(&t.key())
                && !(contact_goals.contains(&t.node) && t.owner == TrackOwner::Transform && matches!(t.pointer.as_str(), "/position" | "/rotation"))
                && !(contact_poles.contains(&t.node) && t.owner == TrackOwner::Transform && t.pointer == "/position")
                && choose(t).is_some()
        },
        |t, p| match choose(t).flatten() {
            Some(i) => p.key_comp(i),
            None => p.key_now(),
        },
    )?;
    let time = state.current_frame as f32;
    let before = state.scene.loom_rigs.clone();
    let mut after = before.clone();
    for (ri, current) in before.iter().enumerate() {
        let body_frame = state.scene.world_matrix(current.mapping.root);
        for (li, limb) in current.limbs.iter().enumerate() {
            let Some(control) = state.scene.contact_control(limb.goal)? else { continue };
            let can_key_track = |node_id: NodeId, pointer: &str| -> bool {
                let Some(node) = state.scene.node(node_id) else { return false };
                tracks::node_tracks(node).into_iter().any(|t| {
                    t.owner == TrackOwner::Transform
                        && t.pointer == pointer
                        && !unkeyable.contains(&t.key())
                        && !locked.contains(&t.key())
                        && choose(&t).is_some()
                })
            };
            let goal_key = state.selection.contains(&limb.goal) && (can_key_track(limb.goal, "/position") || can_key_track(limb.goal, "/rotation"));
            let pole_key = state.selection.contains(&limb.pole) && can_key_track(limb.pole, "/position");
            if goal_key || pole_key {
                crate::loomrig_contacts::key_contact_target(&mut after[ri].limbs[li].contacts, time, &control.sample, body_frame)?;
            }
        }
    }
    if before != after {
        changes.push(Change::SetLoomRigs { before, after });
    }
    let n = changes.len();
    if n > 0 {
        commit(state, label, Change::Group(changes))?;
    }
    Ok(json!({ "changed": n }))
}

fn lock_key(state: &mut AppState, pointer: &str, label: &str) -> Result<Value, String> {
    edit_selection(state, label, |t| t.owner == TrackOwner::Transform && t.pointer == pointer, |_, p| {
        p.key_now();
        let t = anim::time();
        if let Some(k) = p.ensure_keys() {
            for tr in &mut k.tracks {
                if let Some(i) = tr.key_at(t) {
                    let key = &mut tr.keys[i];
                    key.in_type = TangentType::Custom;
                    key.out_type = TangentType::Custom;
                    key.in_tan = 0.0;
                    key.out_tan = 0.0;
                }
            }
        }
    })
}

/// Tracks turned non-keyable in Track View (Set Keys skips them).
fn unkeyable(state: &AppState) -> Vec<tracks::TrackKey> {
    state.scene.track_data.flags.iter().filter(|(_, f)| f.not_keyable).map(|(k, _)| k.clone()).collect()
}

/// Edits that make up a Set Key pose (transforms and parameters), and the
/// commands that manage the pose themselves.
pub fn is_pose_edit(c: &Command) -> bool {
    use Command as C;
    matches!(
        c,
        C::MoveSelection { .. }
            | C::RotateSelection { .. }
            | C::ScaleSelection { .. }
            | C::SetTransform { .. }
            | C::SetParameter { .. }
            | C::SetModifierParameter { .. }
            | C::SetKeys
            | C::SetKeySelectionList { .. }
            | C::SetKeyMode
            | C::ClearSetKeyModeBuffer
            | C::SetFrame { .. }
            | C::ForwardTimeOneUnit
            | C::BackupTimeOneUnit
            | C::GoToStartFrame
            | C::GoToEndFrame
            | C::GoToNextKey
            | C::GoToPreviousKey
            | C::TickPlayback { .. }
            | C::UndoSceneOperation
            | C::RedoSceneOperation
    )
}

pub fn is_pose_change(c:&Command) -> bool {
    matches!(c,Command::MoveSelection {..} | Command::RotateSelection {..} | Command::ScaleSelection {..}
        | Command::SetTransform {..} | Command::SetParameter {..} | Command::SetModifierParameter {..})
}

/// Starts a Set Key pose at the current history position.
pub fn start_pose(state: &mut AppState) {
    state.undo.finish_transaction();
    let versions = state.scene.nodes().map(|n| (n.id, (n.version, n.geom_version))).collect();
    state.undo.retain_from(Some(state.undo.position()));
    state.set_key_buffer = Some(SetKeyBuffer { position: state.undo.position(), versions });
}

/// Drops a Set Key pose: undoes back to where it started.
pub fn discard_pose(state: &mut AppState) {
    let Some(buf) = state.set_key_buffer.take() else { return };
    state.undo.finish_transaction();
    while state.undo.position() > buf.position && state.undo.undo_is_pose_preview() {
        if state.undo.undo(&mut state.scene).is_none() {
            break;
        }
    }
    state.undo.retain_from(None);
    state.mesh_dirty = true;
}

fn set_keys(state: &mut AppState) -> Result<Value, String> {
    use std::collections::BTreeSet;
    let filters = state.key_filters;
    let t = state.current_frame as f32;
    let staged = if state.set_key_mode {
        Some(state.undo.pose_baseline(&state.scene, state.set_key_buffer.as_ref().ok_or("Set Key pose baseline is missing")?.position)?)
    } else {
        None
    };
    let baseline = staged.as_ref().unwrap_or(&state.scene);
    let mut ids: Vec<NodeId> = state.selection.iter().copied().collect();
    if state.set_key_mode {
        let buf = state.set_key_buffer.as_ref().ok_or("Set Key pose baseline is missing")?;
        for n in state.scene.nodes() {
            if buf.versions.get(&n.id) != Some(&(n.version, n.geom_version)) && !ids.contains(&n.id) {
                ids.push(n.id);
            }
        }
    }
    let mut contact_goals = BTreeSet::new();
    let mut contact_poles = BTreeSet::new();
    for rig in &state.scene.loom_rigs {
        let owned: BTreeSet<_> = rig.bindings.iter().map(|b| b.control).chain([rig.master]).chain(rig.limbs.iter().flat_map(|l| [l.goal, l.pole])).collect();
        if owned.iter().any(|id| ids.contains(id) && state.scene.node(*id).is_some_and(|n| n.object.is_bone())) {
            return Err("rig controls must not overwrite source bone tracks".into());
        }
        for limb in &rig.limbs {
            if state.scene.contact_control(limb.goal)?.is_some() {
                contact_goals.insert(limb.goal);
                contact_poles.insert(limb.pole);
            }
        }
    }
    let locked: Vec<tracks::TrackKey> = baseline.track_data.flags.iter().filter(|(_, f)| f.locked || f.not_keyable).map(|(k, _)| k.clone()).collect();
    let mut changes = Vec::new();
    for id in &ids {
        let Some(before) = baseline.node(*id) else { continue };
        let Some(posed) = state.scene.node(*id) else { continue };
        let chosen: Vec<TrackPath> = tracks::node_tracks(before)
            .into_iter()
            .filter(|tr| {
                filters.allows(tr)
                    && !locked.contains(&tr.key())
                    && !(contact_goals.contains(id) && tr.owner == TrackOwner::Transform && matches!(tr.pointer.as_str(), "/position" | "/rotation"))
                    && !(contact_poles.contains(id) && tr.owner == TrackOwner::Transform && tr.pointer == "/position")
            })
            .collect();
        let mut after = before.clone();
        tracks::edit_node(&mut after, &chosen, |tr, p| {
            let c = tracks::read_node(posed, tr).unwrap_or_else(|| p.clone()).comps_at(t);
            anim::with(t, true, || {
                p.key_now();
                p.set_comps(&c);
            });
        })?;
        if let Some(c) = node_change(before, &after) {
            changes.push(c);
        }
    }
    if filters.ik_parameters {
        let before_rigs = baseline.loom_rigs.clone();
        let mut after_rigs = before_rigs.clone();
        for current in &state.scene.loom_rigs {
            let index = after_rigs.iter().position(|r| r.id == current.id).ok_or("rig is missing from pose baseline")?;
            let base_rig = &before_rigs[index];
            let owned: BTreeSet<_> = current.bindings.iter().map(|b| b.control).chain([current.master]).chain(current.limbs.iter().flat_map(|l| [l.goal, l.pole])).collect();
            let all_owned_selected = !owned.is_empty() && owned.iter().all(|id| ids.contains(id));
            let mut keyed = current.clone();
            if all_owned_selected && (filters.position || filters.rotation || filters.scale) {
                keyed.pose_frames.insert(t)?;
            }
            for (li, limb) in current.limbs.iter().enumerate() {
                let base_limb = base_rig.limbs.get(li).ok_or("rig limb is missing from pose baseline")?;
                let goal_selected = ids.contains(&limb.goal);
                let pole_selected = ids.contains(&limb.pole);
                let fk_selected = current.bindings.iter().any(|b| [limb.mapping.start, limb.mapping.middle, limb.mapping.end].contains(&b.bone) && ids.contains(&b.control));
                let limb_selected = goal_selected || pole_selected || fk_selected;
                let limb_posed = state.set_key_mode && limb != base_limb;
                if !(limb_selected || limb_posed) {
                    continue;
                }
                let keyed_limb = &mut keyed.limbs[li];
                if let Some(control) = state.scene.contact_control(limb.goal)? {
                    let mut contacts = base_limb.contacts.clone();
                    let target_posed = state.set_key_mode && limb.contacts.references() != base_limb.contacts.references();
                    let target_requested = target_posed || (goal_selected && (filters.position || filters.rotation)) || (pole_selected && filters.position);
                    if target_requested {
                        crate::loomrig_contacts::key_contact_target(&mut contacts, t, &control.sample, state.scene.world_matrix(current.mapping.root))?;
                    } else if limb_selected || limb.contacts != base_limb.contacts {
                        crate::loomrig_contacts::key_contact_weight(&mut contacts, t, control.sample.blend, &control.sample, state.scene.world_matrix(current.mapping.root))?;
                    }
                    keyed_limb.contacts = contacts;
                    if all_owned_selected || fk_selected || limb.blend != base_limb.blend {
                        if !keyable(&keyed_limb.blend.controller) || !limb.blend.get_at(t).is_finite() {
                            return Err("rig blend controller cannot be keyed".into());
                        }
                        let blend = limb.blend.get_at(t).clamp(0., 1.);
                        anim::with(t, true, || {
                            keyed_limb.blend.key_now();
                            keyed_limb.blend.set(blend);
                        });
                    }
                    if all_owned_selected || fk_selected || limb.pose_offsets != base_limb.pose_offsets {
                        while keyed_limb.pose_offsets.len() < 3 {
                            keyed_limb.pose_offsets.push(loom_scene::Param::new(glam::Vec3::ZERO));
                        }
                        for (oi, offset) in keyed_limb.pose_offsets.iter_mut().take(3).enumerate() {
                            let val = limb.pose_offsets.get(oi).map(|p| p.get_at(t)).unwrap_or(glam::Vec3::ZERO);
                            if !keyable(&offset.controller) || !val.is_finite() {
                                return Err("rig offset controller cannot be keyed".into());
                            }
                            anim::with(t, true, || {
                                offset.key_now();
                                offset.set(val);
                            });
                        }
                    }
                    if !limb.pose_matches.is_empty() && (all_owned_selected || fk_selected || limb.pose_matches != base_limb.pose_matches) {
                        keyed_limb.pose_matches.set_key(loom_scene::loomrig_pose::RigPoseKey { time: t, offsets: limb.pose_matches.sample(t)? })?;
                    }
                } else {
                    if !keyable(&keyed_limb.blend.controller) || !limb.blend.get_at(t).is_finite() {
                        return Err("rig blend controller cannot be keyed".into());
                    }
                    let blend = limb.blend.get_at(t).clamp(0., 1.);
                    anim::with(t, true, || {
                        keyed_limb.blend.key_now();
                        keyed_limb.blend.set(blend);
                    });
                    while keyed_limb.pose_offsets.len() < 3 {
                        keyed_limb.pose_offsets.push(loom_scene::Param::new(glam::Vec3::ZERO));
                    }
                    for (oi, offset) in keyed_limb.pose_offsets.iter_mut().take(3).enumerate() {
                        let val = limb.pose_offsets.get(oi).map(|p| p.get_at(t)).unwrap_or(glam::Vec3::ZERO);
                        if !keyable(&offset.controller) || !val.is_finite() {
                            return Err("rig offset controller cannot be keyed".into());
                        }
                        anim::with(t, true, || {
                            offset.key_now();
                            offset.set(val);
                        });
                    }
                    if !limb.pose_matches.is_empty() {
                        keyed_limb.pose_matches.set_key(loom_scene::loomrig_pose::RigPoseKey { time: t, offsets: limb.pose_matches.sample(t)? })?;
                    }
                }
            }
            after_rigs[index] = keyed;
        }
        if before_rigs != after_rigs {
            changes.push(Change::SetLoomRigs { before: before_rigs, after: after_rigs });
        }
    }
    let n = changes.len();
    let change = Change::Group(changes);
    crate::xref::guard(baseline, &change)?;
    if state.set_key_mode {
        discard_pose(state);
    }
    if n > 0 {
        commit(state, "Set Keys", change)?;
    }
    if state.set_key_mode {
        start_pose(state);
    }
    Ok(json!({ "changed": n }))
}

fn keyable<T>(controller:&loom_scene::param::Controller<T>) -> bool {
    use loom_scene::param::Controller;
    match controller {
        Controller::Constant(_) | Controller::Keys(_) => true,
        Controller::Noise(_) => false,
        Controller::List(items) => items.first().is_some_and(|item|item.weight != 0. && keyable(&item.controller)),
    }
}

fn keyable_param(param:&AnyParam) -> bool {
    match param {
        AnyParam::F32(p)=>keyable(&p.controller),AnyParam::U32(p)=>keyable(&p.controller),
        AnyParam::Bool(p)=>keyable(&p.controller),AnyParam::Vec3(p)=>keyable(&p.controller),
    }
}

/// Explicit full-rig capture; prepare every fallible edit before live rollback.
pub(crate) fn key_rig_pose(state:&mut AppState,rig_id:u32) -> Result<Value,String> {
    use std::collections::BTreeSet;
    let time=state.current_frame as f32;
    let rig=state.scene.loom_rigs.iter().find(|rig|rig.id == rig_id).ok_or("LoomRig not found")?;
    let mut owned:BTreeSet<_>=rig.bindings.iter().map(|binding|binding.control).collect();
    owned.insert(rig.master);
    let mut contact_goals=BTreeSet::new();
    let mut contact_poles=BTreeSet::new();
    for limb in &rig.limbs {
        owned.extend([limb.goal,limb.pole]);
        if state.scene.contact_control(limb.goal)?.is_some() {
            contact_goals.insert(limb.goal);contact_poles.insert(limb.pole);
        }
    }
    if owned.iter().any(|id|state.scene.node(*id).is_some_and(|node|node.object.is_bone())) {
        return Err("rig controls must not overwrite source bone tracks".into());
    }
    let staged=if state.set_key_mode {
        Some(state.undo.pose_baseline(&state.scene,state.set_key_buffer.as_ref().ok_or("Set Key pose baseline is missing")?.position)?)
    } else {None};
    let baseline=staged.as_ref().unwrap_or(&state.scene);
    let mut ids=owned.clone();
    if state.set_key_mode {
        let buffer=state.set_key_buffer.as_ref().unwrap();
        ids.extend(state.scene.nodes().filter(|node|buffer.versions.get(&node.id) != Some(&(node.version,node.geom_version))).map(|node|node.id));
    }
    let mut changes=Vec::new();
    for id in ids {
        let before=baseline.node(id).ok_or("posed node is missing from its baseline")?;
        let posed=state.scene.node(id).ok_or("posed node is missing")?;
        let mut after=before.clone();
        let mut chosen=Vec::new();
        for track in tracks::node_tracks(before) {
            let wanted=if owned.contains(&id) {
                track.owner == TrackOwner::Transform
                    && !(contact_goals.contains(&id) && matches!(track.pointer.as_str(),"/position"|"/rotation"))
                    && !(contact_poles.contains(&id) && track.pointer == "/position")
            } else {state.key_filters.allows(&track)};
            if !wanted {continue;}
            let pose=tracks::read_node(posed,&track).ok_or("posed track is missing")?;
            let value=pose.comps_at(time);
            if value.iter().any(|value|!value.is_finite()) {return Err("cannot key a nonfinite rig pose".into());}
            let protected=baseline.track_data.flags.iter().any(|(key,flags)|*key == track.key() && (flags.locked || flags.not_keyable));
            if protected {
                if tracks::read_node(before,&track).ok_or("baseline track is missing")?.comps_at(time) != value {
                    return Err("pose changes a protected track; unlock it before keying".into());
                }
                continue;
            }
            if !keyable_param(&tracks::read_node(before,&track).ok_or("baseline track is missing")?) {
                return Err("rig pose controller cannot be keyed".into());
            }
            chosen.push(track);
        }
        tracks::edit_node(&mut after,&chosen,|track,param| {
            let value=tracks::read_node(posed,track).unwrap().comps_at(time);
            anim::with(time,true,|| {param.key_now();param.set_comps(&value);});
        })?;
        if let Some(change)=node_change(before,&after) {changes.push(change);}
    }
    let before=baseline.loom_rigs.clone();
    let mut after=before.clone();
    for current in &state.scene.loom_rigs {
        let index=after.iter().position(|rig|rig.id == current.id).ok_or("rig is missing from pose baseline")?;
        let capture=current.id == rig_id || (state.set_key_mode && state.key_filters.ik_parameters && current != &before[index]);
        if !capture {continue;}
        let mut keyed=current.clone();
        if current.id == rig_id {keyed.pose_frames.insert(time)?;}
        for (li,limb) in keyed.limbs.iter_mut().enumerate() {
            if !keyable(&limb.blend.controller) || !limb.blend.get_at(time).is_finite() {return Err("rig blend controller cannot be keyed".into());}
            let blend=limb.blend.get_at(time).clamp(0.,1.);
            anim::with(time,true,|| {limb.blend.key_now();limb.blend.set(blend);});
            while limb.pose_offsets.len()<3 {limb.pose_offsets.push(loom_scene::Param::new(glam::Vec3::ZERO));}
            for offset in limb.pose_offsets.iter_mut().take(3) {
                if !keyable(&offset.controller) || !offset.get_at(time).is_finite() {return Err("rig offset controller cannot be keyed".into());}
                anim::with(time,false,||offset.key_now());
            }
            if !limb.pose_matches.is_empty() {
                limb.pose_matches.set_key(loom_scene::loomrig_pose::RigPoseKey {time,offsets:limb.pose_matches.sample(time)?})?;
            }
            if let Some(control)=state.scene.contact_control(limb.goal)? {
                let base_contacts = &before[index].limbs[li].contacts;
                let mut contacts = base_contacts.clone();
                if state.set_key_mode && limb.contacts.references() != base_contacts.references() {
                    crate::loomrig_contacts::key_contact_target(&mut contacts,time,&control.sample,state.scene.world_matrix(current.mapping.root))?;
                } else {
                    crate::loomrig_contacts::key_contact_weight(&mut contacts,time,control.sample.blend,&control.sample,state.scene.world_matrix(current.mapping.root))?;
                }
                limb.contacts = contacts;
            }
        }
        after[index]=keyed;
    }
    if before != after {changes.push(Change::SetLoomRigs {before,after});}
    let count=changes.len();
    let change=Change::Group(changes);
    crate::xref::guard(baseline,&change)?;
    // Baseline/desired controllers are now validated; rollback cannot drop a failed draft.
    if state.set_key_mode {discard_pose(state);}
    if count>0 {commit(state,"Key Rig Pose",change)?;}
    if state.set_key_mode {start_pose(state);}
    Ok(json!({"rig":rig_id,"changed":count,"frame":state.current_frame}))
}

/// The selection's tracks the track bar filter lets through.
fn track_bar_tracks(state: &AppState) -> Vec<tracks::TrackKey> {
    state.selection.iter().flat_map(|id| tracks::list_tracks(&state.scene, *id)).filter(|t| track_bar_allows(state, t)).map(|t| t.key()).collect()
}

fn track_bar_allows_contact(state: &AppState, is_goal: bool) -> bool {
    use crate::state::Tool;
    match state.track_bar_filter {
        TrackBarFilter::AllKeys | TrackBarFilter::AllTransformKeys => true,
        TrackBarFilter::CurrentTransform => match state.active_tool {
            Tool::Select | Tool::Move => true,
            Tool::Rotate => is_goal,
            Tool::Scale => false,
        },
        TrackBarFilter::Object | TrackBarFilter::Material => false,
    }
}

/// Per node: the versions the entry was built at, and every track with its key times.
pub type KeyTimes = ((u64, u64), Vec<(TrackPath, Vec<f32>)>);

/// The track bar's per-node cache (repaints read it; an edit bumps the node's versions).
#[derive(Debug, Default)]
pub struct KeyTimeCache(std::cell::RefCell<HashMap<NodeId, KeyTimes>>);

impl Clone for KeyTimeCache {
    fn clone(&self) -> Self {
        Self::default()
    }
}

fn node_key_times(state: &AppState, id: NodeId) -> Vec<(TrackPath, Vec<f32>)> {
    let Some(n) = state.scene.node(id) else { return Vec::new() };
    let versions = (n.version, n.geom_version);
    if let Some((v, e)) = state.key_time_cache.0.borrow().get(&id) {
        if *v == versions {
            return e.clone();
        }
    }
    let entry: Vec<(TrackPath, Vec<f32>)> = tracks::node_tracks(n)
        .into_iter()
        .map(|t| {
            let times = tracks::read_node(n, &t).and_then(|p| p.keys().map(|k| k.tracks.iter().flat_map(|tr| tr.keys.iter().map(|k| k.time)).collect())).unwrap_or_default();
            (t, times)
        })
        .collect();
    state.key_time_cache.0.borrow_mut().insert(id, (versions, entry.clone()));
    entry
}

/// Every key time of the selection's tracks (as the track bar filter shows them).
pub fn selection_key_times(state: &AppState) -> Vec<f32> {
    let mut times = Vec::new();
    for id in &state.selection {
        for (t, ts) in node_key_times(state, *id) {
            if track_bar_allows(state, &t) {
                times.extend(ts);
            }
        }
    }
    for rig in &state.scene.loom_rigs {
        if state.selection.contains(&rig.master) && matches!(state.track_bar_filter, TrackBarFilter::AllKeys | TrackBarFilter::AllTransformKeys) {
            times.extend(rig.pose_frames.times().iter().copied());
        }
        for limb in &rig.limbs {
            let goal_sel = state.selection.contains(&limb.goal);
            let pole_sel = state.selection.contains(&limb.pole);
            let fk_sel = rig.bindings.iter().any(|b| [limb.mapping.start, limb.mapping.middle, limb.mapping.end].contains(&b.bone) && state.selection.contains(&b.control));
            if (goal_sel || pole_sel) && track_bar_allows_contact(state, goal_sel) {
                times.extend(limb.contacts.keys().iter().map(|k| k.time));
            }
            if (goal_sel || pole_sel || fk_sel) && state.track_bar_filter == TrackBarFilter::AllKeys {
                times.extend(limb.contacts.keys().iter().map(|k| k.time));
                times.extend(limb.pose_matches.keys().iter().map(|k| k.time));
                if let Some(k) = limb.blend.keys() {
                    times.extend(k.tracks.iter().flat_map(|tr| tr.keys.iter().map(|key| key.time)));
                }
                for offset in limb.pose_offsets.iter().take(3) {
                    if let Some(k) = offset.keys() {
                        times.extend(k.tracks.iter().flat_map(|tr| tr.keys.iter().map(|key| key.time)));
                    }
                }
            }
        }
    }
    times.sort_by(f32::total_cmp);
    times.dedup_by(|a, b| (*a - *b).abs() < 1e-4);
    times
}

fn step_key(state: &mut AppState, dir: i32) -> Result<Value, String> {
    let cur = state.current_frame as f32;
    let times = selection_key_times(state);
    let next = if dir > 0 { times.into_iter().find(|t| *t > cur + 1e-3) } else { times.into_iter().rev().find(|t| *t < cur - 1e-3) };
    match next {
        Some(t) => set_frame(state, t.round() as i32),
        None => Ok(json!({ "frame": state.current_frame })),
    }
}

fn delete_keys(state: &mut AppState, from: i32, to: i32) -> Result<Value, String> {
    let (a, b) = range(from, to);
    let allowed = track_bar_tracks(state);
    let ids: Vec<NodeId> = state.selection.iter().copied().collect();
    let mut changes = node_edits(state, &ids, |t| allowed.contains(&t.key()), |_, p| {
        if let Some(k) = keyed(p) {
            let t = anim::time();
            let rest = k.comps_at(t);
            for (tr, r) in k.tracks.iter_mut().zip(rest.iter().zip(k.rest.iter_mut())) {
                tr.keys.retain(|key| key.time < a || key.time > b);
                if tr.keys.is_empty() {
                    *r.1 = *r.0;
                }
            }
        }
    })?;
    let before = state.scene.loom_rigs.clone();
    let mut after = before.clone();
    let t = state.current_frame as f32;
    for rig in &mut after {
        if state.selection.contains(&rig.master) && matches!(state.track_bar_filter, TrackBarFilter::AllKeys | TrackBarFilter::AllTransformKeys) {
            let times: Vec<f32> = rig.pose_frames.times().iter().copied().filter(|&time| time >= a && time <= b).collect();
            for time in times {
                rig.pose_frames.remove(time)?;
            }
        }
        for limb in &mut rig.limbs {
            let goal_sel = state.selection.contains(&limb.goal);
            let pole_sel = state.selection.contains(&limb.pole);
            let fk_sel = rig.bindings.iter().any(|bn| [limb.mapping.start, limb.mapping.middle, limb.mapping.end].contains(&bn.bone) && state.selection.contains(&bn.control));
            if (goal_sel || pole_sel) && track_bar_allows_contact(state, goal_sel) || (fk_sel && state.track_bar_filter == TrackBarFilter::AllKeys) {
                let times: Vec<f32> = limb.contacts.keys().iter().map(|k| k.time).filter(|&time| time >= a && time <= b).collect();
                for time in times {
                    limb.contacts.remove_key(time)?;
                    limb.pose_matches.remove_key(time);
                }
            }
            if (goal_sel || pole_sel || fk_sel) && state.track_bar_filter == TrackBarFilter::AllKeys {
                let match_times: Vec<f32> = limb.pose_matches.keys().iter().map(|k| k.time).filter(|&time| time >= a && time <= b).collect();
                for time in match_times {
                    limb.pose_matches.remove_key(time);
                }
                if let Some(k) = limb.blend.keys().is_some_and(|k| k.has_keys()).then(|| limb.blend.ensure_keys()).flatten() {
                    let rest = k.comps_at(t);
                    for (tr, r) in k.tracks.iter_mut().zip(rest.iter().zip(k.rest.iter_mut())) {
                        tr.keys.retain(|key| key.time < a || key.time > b);
                        if tr.keys.is_empty() {
                            *r.1 = *r.0;
                        }
                    }
                }
                for offset in limb.pose_offsets.iter_mut().take(3) {
                    if let Some(k) = offset.keys().is_some_and(|k| k.has_keys()).then(|| offset.ensure_keys()).flatten() {
                        let rest = k.comps_at(t);
                        for (tr, r) in k.tracks.iter_mut().zip(rest.iter().zip(k.rest.iter_mut())) {
                            tr.keys.retain(|key| key.time < a || key.time > b);
                            if tr.keys.is_empty() {
                                *r.1 = *r.0;
                            }
                        }
                    }
                }
            }
        }
    }
    if before != after {
        changes.push(Change::SetLoomRigs { before, after });
    }
    let n = changes.len();
    if n > 0 {
        commit(state, "Delete Keys", Change::Group(changes))?;
    }
    Ok(json!({ "changed": n }))
}

fn shift_keys(state: &mut AppState, from: i32, to: i32, delta: i32, copy: bool) -> Result<Value, String> {
    let (a, b) = range(from, to);
    let d = delta as f32;
    let allowed = track_bar_tracks(state);
    let ids: Vec<NodeId> = state.selection.iter().copied().collect();
    let shift_param = |k: &mut loom_scene::param::KeyedTracks| {
        for tr in &mut k.tracks {
            let picked: Vec<loom_scene::anim::Key> = tr.keys.iter().filter(|k| k.time >= a && k.time <= b).cloned().collect();
            if picked.is_empty() {
                continue;
            }
            if !copy {
                tr.keys.retain(|k| k.time < a || k.time > b);
            }
            for mut key in picked {
                key.time += d;
                tr.keys.retain(|k| (k.time - key.time).abs() >= 1e-4);
                tr.keys.push(key);
            }
            tr.sort();
        }
    };
    let mut changes = node_edits(state, &ids, |t| allowed.contains(&t.key()), |_, p| {
        if let Some(k) = keyed(p) {
            shift_param(k);
        }
    })?;
    if !copy && delta != 0 {
        let before = state.scene.loom_rigs.clone();
        let mut after = before.clone();
        for rig in &mut after {
            if state.selection.contains(&rig.master) && matches!(state.track_bar_filter, TrackBarFilter::AllKeys | TrackBarFilter::AllTransformKeys) {
                let mut times: Vec<f32> = rig.pose_frames.times().iter().copied().filter(|&time| time >= a && time <= b).collect();
                if d > 0. {
                    times.reverse();
                }
                for time in times {
                    rig.pose_frames.move_key(time, time + d)?;
                }
            }
            for limb in &mut rig.limbs {
                let goal_sel = state.selection.contains(&limb.goal);
                let pole_sel = state.selection.contains(&limb.pole);
                let fk_sel = rig.bindings.iter().any(|bn| [limb.mapping.start, limb.mapping.middle, limb.mapping.end].contains(&bn.bone) && state.selection.contains(&bn.control));
                if (goal_sel || pole_sel) && track_bar_allows_contact(state, goal_sel) || (fk_sel && state.track_bar_filter == TrackBarFilter::AllKeys) {
                    let mut times: Vec<f32> = limb.contacts.keys().iter().map(|k| k.time).filter(|&time| time >= a && time <= b).collect();
                    if d > 0. {
                        times.reverse();
                    }
                    for time in times {
                        limb.contacts.move_key(time, time + d)?;
                        limb.pose_matches.move_key(time, time + d)?;
                    }
                }
                if (goal_sel || pole_sel || fk_sel) && state.track_bar_filter == TrackBarFilter::AllKeys {
                    if let Some(k) = limb.blend.keys().is_some_and(|k| k.has_keys()).then(|| limb.blend.ensure_keys()).flatten() {
                        shift_param(k);
                    }
                    for offset in limb.pose_offsets.iter_mut().take(3) {
                        if let Some(k) = offset.keys().is_some_and(|k| k.has_keys()).then(|| offset.ensure_keys()).flatten() {
                            shift_param(k);
                        }
                    }
                }
            }
        }
        if before != after {
            changes.push(Change::SetLoomRigs { before, after });
        }
    }
    let n = changes.len();
    if n > 0 {
        commit(state, if copy { "Copy Keys" } else { "Move Keys" }, Change::Group(changes))?;
    }
    Ok(json!({ "changed": n }))
}

fn tick(state: &mut AppState, seconds: f32) -> Result<Value, String> {
    if !seconds.is_finite() {
        return Err("playback elapsed time must be finite".into());
    }
    if !state.playing {
        return Ok(json!({ "playing": false, "frame": state.current_frame }));
    }
    let (s0, s1) = state.frame_range;
    let len = i64::from(s1) - i64::from(s0) + 1;
    let p = state.playback;
    let (steps, dir, remainder) = if p.real_time {
        if !(state.fps.is_finite() && state.fps > 0.0 && p.speed.is_finite() && p.speed > 0.0 && p.accum.is_finite()) {
            return Err("playback FPS, speed and accumulator must be finite and valid".into());
        }
        let acc = p.accum + seconds.max(0.0) * state.fps * p.speed;
        if !acc.is_finite() || acc < 0.0 {
            return Err("playback clock must remain finite and nonnegative".into());
        }
        let steps = acc.floor();
        if steps as f64 > i32::MAX as f64 {
            return Err("playback advance exceeds the supported integer frame step".into());
        }
        (steps as i32, 1, acc - steps)
    } else {
        let dir = match p.direction {
            PlayDirection::Forward => 1,
            PlayDirection::Reverse => -1,
            PlayDirection::PingPong => p.sign,
        };
        (1, dir, 0.0)
    };
    let mut sign = p.sign;
    let mut f = i64::from(state.current_frame) + i64::from(steps) * i64::from(dir);
    if !p.real_time && p.direction == PlayDirection::PingPong && (f > i64::from(s1) || f < i64::from(s0)) {
        sign = p.sign.checked_neg().ok_or("invalid playback direction")?;
        f = i64::from(state.current_frame) - i64::from(steps) * i64::from(dir);
    }
    let mut playing = state.playing;
    if f > i64::from(s1) || f < i64::from(s0) {
        if p.loop_playback {
            f = i64::from(s0) + (f - i64::from(s0)).rem_euclid(len);
        } else {
            f = f.clamp(i64::from(s0), i64::from(s1));
            playing = false;
        }
    }
    let fraction = if p.real_time && playing { remainder } else { 0.0 };
    state.playing = playing;
    state.playback.sign = sign;
    state.playback.accum = fraction;
    set_animation_time(state, f as i32, f as f32 + fraction)?;
    Ok(json!({ "playing": state.playing, "frame": state.current_frame }))
}

fn rescale_controller<T>(controller: &mut loom_scene::param::Controller<T>, k: f32, map: &impl Fn(f32) -> Result<f32, String>) -> Result<bool, String> {
    use loom_scene::param::Controller;
    match controller {
        Controller::Constant(_) | Controller::Noise(_) => Ok(false),
        Controller::Keys(keys) => {
            let mut changed = false;
            for tr in &mut keys.tracks {
                if tr.keys.is_empty() {
                    continue;
                }
                for key in &mut tr.keys {
                    key.time = map(key.time)?;
                    key.in_tan /= k;
                    key.out_tan /= k;
                    if !key.in_tan.is_finite() || !key.out_tan.is_finite() {
                        return Err("rescaled tangent is not finite".into());
                    }
                    changed = true;
                }
                if tr.keys.windows(2).any(|pair| pair[0].time >= pair[1].time || pair[1].time - pair[0].time < loom_scene::anim::SAME_TIME) {
                    return Err("rescaled keys collide in time".into());
                }
            }
            Ok(changed)
        }
        Controller::List(items) => {
            let mut changed = false;
            for item in items {
                changed |= rescale_controller(&mut item.controller, k, map)?;
            }
            Ok(changed)
        }
    }
}

fn rescale_param(param: &mut AnyParam, k: f32, map: &impl Fn(f32) -> Result<f32, String>) -> Result<bool, String> {
    match param {
        AnyParam::F32(p) => rescale_controller(&mut p.controller, k, map),
        AnyParam::U32(p) => rescale_controller(&mut p.controller, k, map),
        AnyParam::Bool(p) => rescale_controller(&mut p.controller, k, map),
        AnyParam::Vec3(p) => rescale_controller(&mut p.controller, k, map),
    }
}

fn rescale(state: &mut AppState, start: i32, end: i32) -> Result<Value, String> {
    if end <= start {
        return Err("end time must be after start time".into());
    }
    let (s0, s1) = state.frame_range;
    let span = i64::from(s1) - i64::from(s0);
    if span <= 0 {
        return Err("active frame range must have positive length".into());
    }
    let new_span = i64::from(end) - i64::from(start);
    let k = (new_span as f64 / span as f64) as f32;
    if !(k.is_finite() && k > 0.0) {
        return Err("time scale factor must be finite and positive".into());
    }
    let map = |t: f32| -> Result<f32, String> {
        let mapped = (start as f64 + (t as f64 - s0 as f64) * (new_span as f64 / span as f64)) as f32;
        if !mapped.is_finite() || (mapped as f64) < i32::MIN as f64 || (mapped as f64) > i32::MAX as f64 {
            return Err("rescaled key time exceeds the timeline range".into());
        }
        Ok(mapped)
    };
    let locked: Vec<tracks::TrackKey> = state.scene.track_data.flags.iter().filter(|(_, f)| f.locked).map(|(k, _)| k.clone()).collect();
    for rig in &state.scene.loom_rigs {
        let rig_animated = !rig.pose_frames.is_empty()
            || rig.limbs.iter().any(|l| !l.contacts.keys().is_empty() || !l.pose_matches.keys().is_empty() || l.blend.is_animated() || l.pose_offsets.iter().any(|o| o.is_animated()));
        let owned: std::collections::BTreeSet<_> = rig.bindings.iter().map(|b| b.control).chain([rig.master]).chain(rig.limbs.iter().flat_map(|l| [l.goal, l.pole])).collect();
        for id in owned {
            let Some(node) = state.scene.node(id) else { continue };
            for track in tracks::node_tracks(node) {
                if locked.contains(&track.key()) && (rig_animated || tracks::read_node(node, &track).is_some_and(|p| p.is_animated())) {
                    return Err("rig pose contains a locked track; unlock it before rescaling time".into());
                }
            }
        }
    }
    let ids: Vec<NodeId> = state.scene.nodes().map(|n| n.id).collect();
    let mut changes = Vec::new();
    for id in &ids {
        let Some(n) = state.scene.node(*id) else { continue };
        let chosen: Vec<TrackPath> = tracks::node_tracks(n).into_iter().filter(|t| !locked.contains(&t.key())).collect();
        let mut after = n.clone();
        let mut error = None;
        tracks::edit_node(&mut after, &chosen, |_, p| {
            if let Err(e) = rescale_param(p, k, &map) {
                error = Some(e);
            }
        })?;
        if let Some(e) = error {
            return Err(e);
        }
        for p in [&mut after.object_offset.position, &mut after.object_offset.rotation, &mut after.object_offset.scale] {
            rescale_controller(&mut p.controller, k, &map)?;
        }
        if let Some(c) = node_change(n, &after) {
            changes.push(c);
        }
        if n.visibility != after.visibility {
            changes.push(Change::SetVisibility { id: *id, before: n.visibility.clone(), after: after.visibility.clone() });
        }
        if n.object_offset != after.object_offset {
            changes.push(Change::SetNodeProps { id: *id, before: Box::new(n.props()), after: Box::new(after.props()) });
        }
    }
    let before_rigs = state.scene.loom_rigs.clone();
    let mut after_rigs = before_rigs.clone();
    for rig in &mut after_rigs {
        if !rig.pose_frames.is_empty() {
            let mapped: Vec<f32> = rig.pose_frames.times().iter().map(|&t| map(t)).collect::<Result<_, _>>()?;
            rig.pose_frames = loom_scene::loomrig_pose::RigPoseFrames::try_from(mapped)?;
        }
        for limb in &mut rig.limbs {
            rescale_controller(&mut limb.blend.controller, k, &map)?;
            for offset in &mut limb.pose_offsets {
                rescale_controller(&mut offset.controller, k, &map)?;
            }
            if !limb.pose_matches.keys().is_empty() {
                limb.pose_matches = limb.pose_matches.rescaled(&map)?;
            }
            if !limb.contacts.keys().is_empty() {
                limb.contacts = limb.contacts.rescaled(&map)?;
            }
        }
    }
    if before_rigs != after_rigs {
        changes.push(Change::SetLoomRigs { before: before_rigs, after: after_rigs });
    }
    let before_clips = state.scene.animation_clips.clone();
    let mut after_clips = before_clips.clone();
    for clip in &mut after_clips {
        clip.start = map(clip.start)?;
        clip.end = map(clip.end)?;
    }
    if before_clips != after_clips {
        changes.push(Change::SetAnimationClips { before: before_clips, after: after_clips });
    }
    let f = map(state.current_frame as f32)?.round();
    if !(f.is_finite() && (f as f64) >= i32::MIN as f64 && (f as f64) <= i32::MAX as f64) {
        return Err("rescaled current frame exceeds the timeline range".into());
    }
    let f = f as i32;
    let change = Change::Group(changes);
    crate::xref::guard(&state.scene, &change)?;
    while state.undo.in_transaction() {
        state.undo.end();
    }
    let timing = crate::timing_undo::snap(state);
    if (start, end) != state.frame_range || f != state.current_frame || !matches!(&change, Change::Group(steps) if steps.is_empty()) {
        crate::execute::commit_exact(state, "Re-scale Time", change)?;
        state.frame_range = (start, end);
        set_frame(state, f)?;
        crate::timing_undo::record(state, timing);
    }
    Ok(json!({ "start": start, "end": end, "frame": state.current_frame }))
}

/// An animation clip file (.lxaf): animated tracks by node name.
#[derive(Debug, Serialize, Deserialize)]
struct Clip {
    format: String,
    version: u32,
    fps: f32,
    range: (i32, i32),
    nodes: Vec<ClipNode>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ClipNode {
    name: String,
    tracks: Vec<ClipTrack>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ClipTrack {
    owner: TrackOwner,
    pointer: String,
    name: String,
    ty: loom_scene::param::ParamType,
    controller: Value,
}

const CLIP_FORMAT: &str = "loom-animation";

fn save_clip(state: &AppState, path: &str) -> Result<Value, String> {
    let mut nodes = Vec::new();
    for id in &state.selection {
        let Some(n) = state.scene.node(*id) else { continue };
        let tracks: Vec<ClipTrack> = tracks::node_tracks(n)
            .into_iter()
            .filter_map(|t| {
                let p = tracks::read_node(n, &t)?;
                p.is_animated().then(|| ClipTrack { owner: t.owner, pointer: t.pointer, name: t.name, ty: t.ty, controller: p.controller_json() })
            })
            .collect();
        if !tracks.is_empty() {
            nodes.push(ClipNode { name: n.name.clone(), tracks });
        }
    }
    if nodes.is_empty() {
        return Err("the selection has no animated tracks".into());
    }
    let clip = Clip { format: CLIP_FORMAT.into(), version: 1, fps: state.fps, range: state.frame_range, nodes };
    let text = serde_json::to_string_pretty(&clip).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| format!("{path}: {e}"))?;
    Ok(json!({ "nodes": clip.nodes.len(), "path": path }))
}

fn load_clip(state: &mut AppState, path: &str, offset: i32) -> Result<Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let clip: Clip = serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))?;
    if clip.format != CLIP_FORMAT {
        return Err(format!("{path}: not a loom animation clip"));
    }
    let pool: Vec<NodeId> = if state.selection.is_empty() { state.scene.nodes().map(|n| n.id).collect() } else { state.selection.iter().copied().collect() };
    let mut changes = Vec::new();
    for cn in &clip.nodes {
        let Some(n) = pool.iter().filter_map(|id| state.scene.node(*id)).find(|n| n.name == cn.name) else { continue };
        let all = tracks::node_tracks(n);
        let mut after = n.clone();
        let wanted: Vec<TrackPath> = all.into_iter().filter(|t| cn.tracks.iter().any(|c| c.owner == t.owner && c.pointer == t.pointer && c.ty == t.ty)).collect();
        let mut err = None;
        tracks::edit_node(&mut after, &wanted, |t, p| {
            let Some(c) = cn.tracks.iter().find(|c| c.owner == t.owner && c.pointer == t.pointer) else { return };
            if let Err(e) = p.set_controller_json(&c.controller) {
                err = Some(e);
                return;
            }
            if offset != 0 {
                if let Some(k) = keyed(p) {
                    for tr in &mut k.tracks {
                        for key in &mut tr.keys {
                            key.time += offset as f32;
                        }
                    }
                }
            }
        })?;
        if let Some(e) = err {
            return Err(e);
        }
        if let Some(c) = node_change(n, &after) {
            changes.push(c);
        }
    }
    let count = changes.len();
    if count == 0 {
        return Err("no object in the clip matches by name".into());
    }
    commit(state, "Load Animation", Change::Group(changes))?;
    Ok(json!({ "nodes": count }))
}
