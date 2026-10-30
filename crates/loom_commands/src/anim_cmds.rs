//! Key commands: Auto / Set Key, Key Filters, Key Position…, default
//! tangents, track bar edits, playback, Re-scale Time and animation clips.

use crate::command::Command;
use crate::execute::{commit_exact as commit, set_frame};
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
    /// Fractional frames not played yet (Real Time).
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
        C::DeleteKeys { from, to } => {
            let (a, b) = range(from, to);
            let allowed = track_bar_tracks(state);
            edit_selection(state, "Delete Keys", |t| allowed.contains(&t.key()), |_, p| {
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
            })
        }
        C::MoveKeysInTime { from, to, delta } => shift_keys(state, from, to, delta, false),
        C::CopyKeys { from, to, delta } => shift_keys(state, from, to, delta, true),
        C::SetPlayback { real_time, loop_playback, speed, direction } => {
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
        C::DeleteSelectedAnimation => {
            let t = state.current_frame as f32;
            edit_selection(state, "Delete Selected Animation", |_| true, |_, p| {
                if p.is_animated() {
                    p.freeze(t);
                }
            })
        }
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

/// Edits the chosen tracks of nodes `ids` as one undo entry.
pub fn edit_nodes(state: &mut AppState, label: &str, ids: &[NodeId], choose: impl Fn(&TrackPath) -> bool, mut f: impl FnMut(&TrackPath, &mut AnyParam)) -> Result<Value, String> {
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
    let n = changes.len();
    if n > 0 {
        commit(state, label, Change::Group(changes))?;
    }
    Ok(json!({ "changed": n }))
}

fn edit_selection(state: &mut AppState, label: &str, choose: impl Fn(&TrackPath) -> bool, f: impl FnMut(&TrackPath, &mut AnyParam)) -> Result<Value, String> {
    let ids: Vec<NodeId> = state.selection.iter().copied().collect();
    edit_nodes(state, label, &ids, choose, f)
}

/// Keys the chosen tracks (Some(None) = every component, Some(Some(i)) = one).
fn key_selection(state: &mut AppState, label: &str, choose: impl Fn(&TrackPath) -> Option<Option<usize>>) -> Result<Value, String> {
    let unkeyable = unkeyable(state);
    edit_selection(state, label, |t| !unkeyable.contains(&t.key()) && choose(t).is_some(), |t, p| match choose(t).flatten() {
        Some(i) => p.key_comp(i),
        None => p.key_now(),
    })
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

/// Starts a Set Key pose at the current history position.
pub fn start_pose(state: &mut AppState) {
    let versions = state.scene.nodes().map(|n| (n.id, (n.version, n.geom_version))).collect();
    state.set_key_buffer = Some(SetKeyBuffer { position: state.undo.position(), versions });
}

/// Drops a Set Key pose: undoes back to where it started.
pub fn discard_pose(state: &mut AppState) {
    let Some(buf) = state.set_key_buffer.take() else { return };
    while state.undo.position() > buf.position {
        if state.undo.undo(&mut state.scene).is_none() {
            break;
        }
    }
    state.mesh_dirty = true;
}

fn set_keys(state: &mut AppState) -> Result<Value, String> {
    let filters = state.key_filters;
    if !state.set_key_mode {
        return key_selection(state, "Set Keys", |t| filters.allows(t).then_some(None));
    }
    // Set Key mode: the posed nodes (and the selection) get keys of their pose.
    let buf = state.set_key_buffer.clone();
    let mut ids: Vec<NodeId> = state.selection.iter().copied().collect();
    for n in state.scene.nodes() {
        let changed = buf.as_ref().is_none_or(|b| b.versions.get(&n.id) != Some(&(n.version, n.geom_version)));
        if changed && !ids.contains(&n.id) {
            ids.push(n.id);
        }
    }
    let posed: HashMap<NodeId, Node> = ids.iter().filter_map(|id| state.scene.node(*id).map(|n| (*id, n.clone()))).collect();
    discard_pose(state);
    let t = state.current_frame as f32;
    let unkeyable = unkeyable(state);
    let r = edit_nodes(state, "Set Keys", &ids, |tr| filters.allows(tr) && !unkeyable.contains(&tr.key()), |tr, p| {
        p.key_now();
        if let Some(pose) = posed.get(&tr.node).and_then(|n| tracks::read_node(n, tr)) {
            let c = pose.comps_at(t);
            anim::with(t, true, || p.set_comps(&c));
        }
    });
    start_pose(state);
    r
}

/// The selection's tracks the track bar filter lets through.
fn track_bar_tracks(state: &AppState) -> Vec<tracks::TrackKey> {
    state.selection.iter().flat_map(|id| tracks::list_tracks(&state.scene, *id)).filter(|t| track_bar_allows(state, t)).map(|t| t.key()).collect()
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

/// The kinds of track a track bar key belongs to (the track bar colours
/// position, rotation and scale keys apart from the others).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyKinds {
    pub position: bool,
    pub rotation: bool,
    pub scale: bool,
    pub other: bool,
}

/// Each key time of the selection's tracks (as the track bar filter shows
/// them) with the kinds of track keyed there.
pub fn selection_key_kinds(state: &AppState) -> Vec<(f32, KeyKinds)> {
    let mut out: Vec<(f32, KeyKinds)> = Vec::new();
    for id in &state.selection {
        for (t, ts) in node_key_times(state, *id) {
            if !track_bar_allows(state, &t) {
                continue;
            }
            for time in ts {
                let i = match out.iter().position(|(x, _)| (*x - time).abs() < 1e-4) {
                    Some(i) => i,
                    None => {
                        out.push((time, KeyKinds::default()));
                        out.len() - 1
                    }
                };
                let k = &mut out[i].1;
                match (t.owner == TrackOwner::Transform, t.pointer.as_str()) {
                    (true, "/position") => k.position = true,
                    (true, "/rotation") => k.rotation = true,
                    (true, "/scale") => k.scale = true,
                    _ => k.other = true,
                }
            }
        }
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
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
    times.sort_by(f32::total_cmp);
    times.dedup_by(|a, b| (*a - *b).abs() < 1e-4);
    times
}

/// The key times Key Mode steps between (Time Configuration > Key Steps).
pub fn key_step_times(state: &AppState) -> Vec<f32> {
    let k = state.time_config.key_steps;
    if k.use_track_bar {
        return selection_key_times(state);
    }
    let (p, r, s) = if k.use_current_transform {
        match state.active_tool {
            crate::state::Tool::Move => (true, false, false),
            crate::state::Tool::Rotate => (false, true, false),
            crate::state::Tool::Scale => (false, false, true),
            crate::state::Tool::Select => (true, true, true),
        }
    } else {
        (k.position, k.rotation, k.scale)
    };
    let ids: Vec<NodeId> = if k.selected_only { state.selection.iter().copied().collect() } else { state.scene.nodes().map(|n| n.id).collect() };
    let mut times = Vec::new();
    for id in ids {
        for (t, ts) in node_key_times(state, id) {
            let want = t.owner == TrackOwner::Transform && ((p && t.pointer == "/position") || (r && t.pointer == "/rotation") || (s && t.pointer == "/scale"));
            if want {
                times.extend(ts);
            }
        }
    }
    times.sort_by(f32::total_cmp);
    times.dedup_by(|a, b| (*a - *b).abs() < 1e-4);
    times
}

fn step_key(state: &mut AppState, dir: i32) -> Result<Value, String> {
    let cur = state.current_frame as f32;
    let times = key_step_times(state);
    let next = if dir > 0 { times.into_iter().find(|t| *t > cur + 1e-3) } else { times.into_iter().rev().find(|t| *t < cur - 1e-3) };
    match next {
        Some(t) => set_frame(state, t.round() as i32),
        None => Ok(json!({ "frame": state.current_frame })),
    }
}

fn shift_keys(state: &mut AppState, from: i32, to: i32, delta: i32, copy: bool) -> Result<Value, String> {
    let (a, b) = range(from, to);
    let d = delta as f32;
    let allowed = track_bar_tracks(state);
    edit_selection(state, if copy { "Copy Keys" } else { "Move Keys" }, |t| allowed.contains(&t.key()), |_, p| {
        if let Some(k) = keyed(p) {
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
        }
    })
}

fn tick(state: &mut AppState, seconds: f32) -> Result<Value, String> {
    if !state.playing {
        return Ok(json!({ "playing": false, "frame": state.current_frame }));
    }
    let (s0, s1) = state.frame_range;
    let len = s1 - s0 + 1;
    let p = state.playback;
    let (steps, dir) = if p.real_time {
        let acc = state.playback.accum + seconds.max(0.0) * state.fps * p.speed;
        let steps = acc.floor();
        state.playback.accum = acc - steps;
        (steps as i32, 1)
    } else {
        match p.direction {
            PlayDirection::Forward => (1, 1),
            PlayDirection::Reverse => (1, -1),
            PlayDirection::PingPong => (1, p.sign),
        }
    };
    let mut f = state.current_frame + steps * dir;
    if !p.real_time && p.direction == PlayDirection::PingPong && (f > s1 || f < s0) {
        state.playback.sign = -p.sign;
        f = state.current_frame - steps * dir;
    }
    if f > s1 || f < s0 {
        if p.loop_playback {
            f = s0 + (f - s0).rem_euclid(len);
        } else {
            f = f.clamp(s0, s1);
            state.playing = false;
        }
    }
    set_frame(state, f)?;
    Ok(json!({ "playing": state.playing, "frame": state.current_frame }))
}

fn rescale(state: &mut AppState, start: i32, end: i32) -> Result<Value, String> {
    if end <= start {
        return Err("end time must be after start time".into());
    }
    let (s0, s1) = state.frame_range;
    let k = (end - start) as f32 / (s1 - s0) as f32;
    let map = |t: f32| start as f32 + (t - s0 as f32) * k;
    let ids: Vec<NodeId> = state.scene.nodes().map(|n| n.id).collect();
    edit_nodes(state, "Re-scale Time", &ids, |_| true, |_, p| {
        if let Some(keys) = keyed(p) {
            for tr in &mut keys.tracks {
                for key in &mut tr.keys {
                    key.time = map(key.time);
                    key.in_tan /= k;
                    key.out_tan /= k;
                }
            }
        }
    })?;
    state.frame_range = (start, end);
    let f = map(state.current_frame as f32).round() as i32;
    set_frame(state, f)?;
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
