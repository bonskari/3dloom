//! Track View: the controller hierarchy, key selection and editing (Curve
//! Editor and Dope Sheet), tangents, out-of-range types, Reduce Keys, the
//! Dope Sheet time tools, note tracks, track sets, filters, the controller
//! clipboard and Track View Utilities.

use crate::anim_cmds::{edit_nodes, node_change};
use crate::command::{Command, SelectOp};
use crate::execute::commit_exact as commit;
use crate::state::AppState;
use crate::viewport::Dialog;
use loom_scene::anim::{FloatTrack, Key, Ort, TangentType};
use loom_scene::param::{KeyedTracks, ParamType};
use loom_scene::tracks::{self, AnyParam, NoteKey, NoteTrack, Role, TrackKey, TrackOwner, TrackPath};
use loom_scene::{Change, NodeId};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TvMode {
    #[default]
    CurveEditor,
    DopeSheet,
}

/// Dope Sheet: Edit Keys, Edit Ranges, or Edit Time (a time block is selected).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DopeMode {
    #[default]
    EditKeys,
    EditRanges,
    EditTime,
}

/// The key window tool the mouse drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TvTool {
    #[default]
    Move,
    MoveHorizontal,
    MoveVertical,
    Slide,
    ScaleKeys,
    ScaleValues,
    AddKeys,
    DrawCurves,
    Region,
    Pan,
    Zoom,
    ZoomTime,
    ZoomValues,
    ZoomRegion,
    SelectTime,
    InsertTime,
    ScaleTime,
    Retime,
    RetimeAll,
}

/// One selected key: its track, the component (X/Y/Z) and its frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyRef {
    pub track: TrackKey,
    pub comp: u32,
    pub time: f32,
}

/// A key as MCP names it: node id, track name, component, frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyArg {
    pub id: u64,
    pub track: String,
    #[serde(default)]
    pub comp: u32,
    pub time: f32,
}

/// Filters dialog (Show, Show Only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TvFilters {
    pub transforms: bool,
    pub base_objects: bool,
    pub modified_objects: bool,
    pub note_tracks: bool,
    pub visibility_tracks: bool,
    pub controller_types: bool,
    pub animated_only: bool,
    pub selected_objects: bool,
    pub selected_tracks: bool,
    pub visible_objects: bool,
    pub keyable_only: bool,
    pub unlocked_only: bool,
    pub active_layer_only: bool,
    /// The tracks highlighted when Selected Tracks was turned on.
    pub selected_snapshot: Vec<TrackKey>,
}

impl Default for TvFilters {
    fn default() -> Self {
        Self { transforms: true, base_objects: true, modified_objects: true, note_tracks: true, visibility_tracks: true, controller_types: false, animated_only: false, selected_objects: false, selected_tracks: false, visible_objects: true, keyable_only: false, unlocked_only: false, active_layer_only: false, selected_snapshot: Vec::new() }
    }
}

/// Copied time: keys per (track, component) relative to the block start.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeClip {
    pub length: f32,
    pub tracks: Vec<(TrackKey, u32, Vec<Key>, f32)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrackViewState {
    pub open: bool,
    /// Keyboard focus is in the Track View window (its shortcuts apply).
    pub focused: bool,
    pub name: String,
    pub mode: TvMode,
    pub dope: DopeMode,
    pub tool: TvTool,
    /// Highlighted hierarchy items (tree ids).
    pub highlighted: Vec<String>,
    pub selected_keys: Vec<KeyRef>,
    pub collapsed: BTreeSet<String>,
    pub filters: TvFilters,
    pub snap_frames: bool,
    pub lock_tangents: bool,
    pub lock_selection: bool,
    pub show_tangents: bool,
    pub all_tangents: bool,
    pub key_stats: bool,
    pub freeze_nonselected: bool,
    pub keys_on_frozen: bool,
    pub hide_nonselected: bool,
    pub modify_subtree: bool,
    pub exclude_left: bool,
    pub exclude_right: bool,
    pub isolate: bool,
    pub interactive_update: bool,
    pub sync_cursor_time: bool,
    pub manual_navigation: bool,
    pub auto_expand_animated: bool,
    pub auto_expand_keyable: bool,
    pub auto_expand_unlocked: bool,
    pub auto_expand_limits: bool,
    pub position_ranges: bool,
    /// 0 = Parameter curve, 1 = Ease curve, 2 = Multiplier curve.
    pub ort_target: u8,
    pub limit_clip: Option<(bool, Option<f32>, Option<f32>)>,
    pub soft: bool,
    pub soft_range: f32,
    pub soft_falloff: f32,
    /// Access … Field: the field the next keystrokes go to.
    pub focus_field: Option<String>,
    pub time_selection: Option<(f32, f32)>,
    pub time_clip: Option<TimeClip>,
    pub controller_clip: Option<(ParamType, Role, Value)>,
    /// Key window view: time start, time end, value low, value high.
    pub view: [f32; 4],
    /// Move Highlight Up / Down: the hierarchy row the highlight is on.
    pub cursor: usize,
    /// Hierarchy scroll in rows.
    pub scroll: usize,
    pub object_order: Vec<NodeId>,
}

impl Default for TrackViewState {
    fn default() -> Self {
        Self {
            open: false,
            focused: false,
            name: "Untitled 1".into(),
            mode: TvMode::CurveEditor,
            dope: DopeMode::EditKeys,
            tool: TvTool::Move,
            highlighted: Vec::new(),
            selected_keys: Vec::new(),
            collapsed: BTreeSet::new(),
            filters: TvFilters::default(),
            snap_frames: true,
            lock_tangents: false,
            lock_selection: false,
            show_tangents: true,
            all_tangents: false,
            key_stats: false,
            freeze_nonselected: true,
            keys_on_frozen: true,
            hide_nonselected: true,
            modify_subtree: true,
            exclude_left: false,
            exclude_right: false,
            isolate: false,
            interactive_update: false,
            sync_cursor_time: false,
            manual_navigation: false,
            auto_expand_animated: false,
            auto_expand_keyable: false,
            auto_expand_unlocked: false,
            auto_expand_limits: false,
            position_ranges: false,
            ort_target: 0,
            limit_clip: None,
            soft: false,
            soft_range: 20.0,
            soft_falloff: 1.0,
            focus_field: None,
            time_selection: None,
            time_clip: None,
            controller_clip: None,
            view: [0.0, 100.0, -10.0, 110.0],
            cursor: 0,
            scroll: 0,
            object_order: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TvItemKind {
    Object(NodeId),
    Group(NodeId, &'static str),
    Track(TrackPath),
    Note(usize),
}

/// One row of the Track View hierarchy.
#[derive(Debug, Clone, PartialEq)]
pub struct TvItem {
    pub id: String,
    pub depth: u32,
    pub label: String,
    pub kind: TvItemKind,
    /// The controller name shown with Controller Types.
    pub controller: String,
    pub animated: bool,
    pub locked: bool,
    pub keyable: bool,
}

pub fn track_id(t: &TrackPath) -> String {
    format!("{}:{}", t.node.0, t.name)
}

fn group_of(t: &TrackPath) -> &'static str {
    match t.owner {
        TrackOwner::Transform | TrackOwner::Constraints => "Transform",
        TrackOwner::Object => "Object",
        TrackOwner::Modifier(_) => "Modified Object",
        TrackOwner::Visibility => "",
    }
}

/// The objects in Track View order (Move Object Up / Down reorders).
fn object_order(state: &AppState) -> Vec<NodeId> {
    let mut ids: Vec<NodeId> = state.track_view.object_order.iter().copied().filter(|i| state.scene.node(*i).is_some()).collect();
    for n in state.scene.nodes() {
        if !ids.contains(&n.id) {
            ids.push(n.id);
        }
    }
    ids
}

/// The visible hierarchy: filters and collapsed branches applied.
pub fn tree(state: &AppState) -> Vec<TvItem> {
    let tv = &state.track_view;
    let f = &tv.filters;
    let data = &state.scene.track_data;
    let mut out = Vec::new();
    for id in object_order(state) {
        let Some(n) = state.scene.node(id) else { continue };
        if f.visible_objects && n.hidden {
            continue;
        }
        if f.selected_objects && !state.selection.contains(&id) {
            continue;
        }
        if f.active_layer_only && n.layer != state.scene.current_layer {
            continue;
        }
        let all = tracks::node_tracks(n);
        let shown: Vec<(TrackPath, AnyParam)> = all
            .into_iter()
            .filter_map(|t| {
                let p = tracks::read_node(n, &t)?;
                let fl = data.flags_of(&t.key());
                let cat = match t.owner {
                    TrackOwner::Transform | TrackOwner::Constraints => f.transforms,
                    TrackOwner::Object => f.base_objects,
                    TrackOwner::Modifier(_) => f.modified_objects,
                    TrackOwner::Visibility => f.visibility_tracks,
                };
                let keep = cat && (!f.animated_only || p.is_animated()) && (!f.keyable_only || !fl.not_keyable) && (!f.unlocked_only || !fl.locked) && (!f.selected_tracks || f.selected_snapshot.contains(&t.key()));
                keep.then_some((t, p))
            })
            .collect();
        let notes: Vec<usize> = data.note_tracks.iter().enumerate().filter(|(_, nt)| nt.node == id && f.note_tracks).map(|(i, _)| i).collect();
        if (f.animated_only || f.selected_tracks || f.keyable_only || f.unlocked_only) && shown.is_empty() && notes.is_empty() {
            continue;
        }
        let oid = format!("{}", id.0);
        out.push(TvItem { id: oid.clone(), depth: 0, label: n.name.clone(), kind: TvItemKind::Object(id), controller: String::new(), animated: shown.iter().any(|(_, p)| p.is_animated()), locked: false, keyable: true });
        if tv.collapsed.contains(&oid) {
            continue;
        }
        let mut last_group = "";
        for (t, p) in &shown {
            let g = group_of(t);
            let gid = format!("{}:{}", id.0, g);
            if !g.is_empty() && g != last_group {
                last_group = g;
                let label = if g == "Object" { format!("Object ({})", n.object.base_name()) } else { g.to_owned() };
                out.push(TvItem { id: gid.clone(), depth: 1, label, kind: TvItemKind::Group(id, g), controller: String::new(), animated: false, locked: false, keyable: true });
            }
            if !g.is_empty() && tv.collapsed.contains(&gid) {
                continue;
            }
            let fl = data.flags_of(&t.key());
            out.push(TvItem { id: track_id(t), depth: if g.is_empty() { 1 } else { 2 }, label: t.name.clone(), kind: TvItemKind::Track(t.clone()), controller: controller_name(n, t, p), animated: p.is_animated(), locked: fl.locked, keyable: !fl.not_keyable });
        }
        for i in notes {
            out.push(TvItem { id: format!("{}:note:{i}", id.0), depth: 1, label: "Note Track".into(), kind: TvItemKind::Note(i), controller: String::new(), animated: false, locked: false, keyable: true });
        }
    }
    out
}

fn controller_name(n: &loom_scene::Node, t: &TrackPath, p: &AnyParam) -> String {
    match (t.owner, t.pointer.as_str()) {
        (TrackOwner::Transform, "/position") => n.constraints.position_name().map(str::to_owned).unwrap_or_else(|| p.controller_name(Role::Position)),
        (TrackOwner::Transform, "/rotation") => n.constraints.rotation_name().map(str::to_owned).unwrap_or_else(|| p.controller_name(Role::Rotation)),
        _ => p.controller_name(Role::of(t)),
    }
}

/// Every track of a node with its id (unfiltered: highlights may name hidden ones).
fn node_track_ids(state: &AppState, id: NodeId) -> Vec<(String, TrackPath)> {
    tracks::list_tracks(&state.scene, id).into_iter().map(|t| (track_id(&t), t)).collect()
}

/// The tracks an item stands for: itself, or (Modify Subtree) the tracks under it.
fn item_tracks(state: &AppState, item: &str) -> Vec<TrackPath> {
    let mut parts = item.splitn(2, ':');
    let Some(Ok(id)) = parts.next().map(|p| p.parse::<u64>()) else { return Vec::new() };
    let rest = parts.next();
    let all = node_track_ids(state, NodeId(id));
    match rest {
        None => all.into_iter().map(|(_, t)| t).collect(),
        Some(g) if ["Transform", "Object", "Modified Object"].contains(&g) => all.into_iter().filter(|(_, t)| group_of(t) == g).map(|(_, t)| t).collect(),
        Some(_) => all.into_iter().filter(|(i, _)| i == item).map(|(_, t)| t).collect(),
    }
}

pub fn highlighted_tracks(state: &AppState) -> Vec<TrackPath> {
    let mut out: Vec<TrackPath> = Vec::new();
    for item in &state.track_view.highlighted {
        for t in item_tracks(state, item) {
            if !out.contains(&t) {
                out.push(t);
            }
        }
    }
    out
}

fn note_of(item: &str) -> Option<usize> {
    item.split(":note:").nth(1)?.parse().ok()
}

fn object_of(item: &str) -> Option<NodeId> {
    (!item.contains(':')).then(|| item.parse::<u64>().ok().map(NodeId)).flatten()
}

fn resolve(state: &AppState, k: &KeyArg) -> Result<KeyRef, String> {
    let t = crate::controllers::find_track(state, NodeId(k.id), &k.track)?;
    Ok(KeyRef { track: t.key(), comp: k.comp, time: k.time })
}

fn path_of(state: &AppState, key: &TrackKey) -> Option<TrackPath> {
    tracks::list_tracks(&state.scene, key.node).into_iter().find(|t| t.key() == *key)
}

const SAME: f32 = 1e-3;

/// Edits the selected keys: `f` gets each track's float track for one component
/// and the key indices selected on it; it returns the new times of those keys.
fn edit_selected(state: &mut AppState, label: &str, mut f: impl FnMut(&mut FloatTrack, &[usize]) -> Vec<f32>) -> Result<Value, String> {
    let mut by_track: HashMap<TrackKey, HashMap<u32, Vec<f32>>> = HashMap::new();
    for k in &state.track_view.selected_keys {
        by_track.entry(k.track.clone()).or_default().entry(k.comp).or_default().push(k.time);
    }
    let mut new_sel: Vec<KeyRef> = Vec::new();
    let mut nodes: Vec<NodeId> = by_track.keys().map(|k| k.node).collect();
    nodes.sort();
    nodes.dedup();
    let paths: Vec<TrackPath> = by_track.keys().filter_map(|k| path_of(state, k)).collect();
    let r = edit_nodes(state, label, &nodes, |t| paths.contains(t), |t, p| {
        let Some(comps) = by_track.get(&t.key()) else { return };
        let Some(k) = p.ensure_keys() else { return };
        for (comp, times) in comps {
            let Some(tr) = k.tracks.get_mut(*comp as usize) else { continue };
            let idx: Vec<usize> = times.iter().filter_map(|tm| tr.keys.iter().position(|key| (key.time - tm).abs() < SAME)).collect();
            let moved = f(tr, &idx);
            for tm in moved {
                new_sel.push(KeyRef { track: t.key(), comp: *comp, time: tm });
            }
        }
    })?;
    state.track_view.selected_keys = new_sel;
    Ok(r)
}

/// Moves keys `idx` of a track by `dt` / `dv`; keys they land on are replaced.
/// Returns the moved keys' new times.
fn move_keys(tr: &mut FloatTrack, idx: &[usize], dt: f32, dv: f32, snap: bool, soft: Option<(f32, f32)>) -> Vec<f32> {
    let sel: Vec<f32> = idx.iter().map(|i| tr.keys[*i].time).collect();
    let weight = |t: f32| -> f32 {
        if sel.iter().any(|s| (s - t).abs() < SAME) {
            return 1.0;
        }
        match soft {
            Some((range, falloff)) => {
                let d = sel.iter().map(|s| (s - t).abs()).fold(f32::MAX, f32::min);
                if d >= range { 0.0 } else { (1.0 - d / range).powf(falloff.max(0.01)) }
            }
            None => 0.0,
        }
    };
    let mut out = Vec::new();
    let mut moved: Vec<Key> = Vec::new();
    let mut kept: Vec<Key> = Vec::new();
    for key in tr.keys.drain(..) {
        let w = weight(key.time);
        if w > 0.0 {
            let mut k = key;
            let mut t = k.time + dt * w;
            if snap {
                t = t.round();
            }
            let is_sel = w >= 1.0;
            k.time = t;
            k.value += dv * w;
            if is_sel {
                out.push(t);
            }
            moved.push(k);
        } else {
            kept.push(key);
        }
    }
    kept.retain(|k| !moved.iter().any(|m| (m.time - k.time).abs() < SAME));
    tr.keys = kept;
    tr.keys.extend(moved);
    tr.sort();
    out
}

fn tangent_set(tr: &mut FloatTrack, idx: &[usize], t: TangentType, side: &str) -> Vec<f32> {
    for i in idx {
        let k = &mut tr.keys[*i];
        if side != "out" {
            k.in_type = t;
        }
        if side != "in" {
            k.out_type = t;
        }
    }
    idx.iter().map(|i| tr.keys[*i].time).collect()
}

/// Edits every keyed component of the highlighted tracks.
fn edit_highlighted(state: &mut AppState, label: &str, mut f: impl FnMut(&mut KeyedTracks)) -> Result<Value, String> {
    let paths = highlighted_tracks(state);
    if paths.is_empty() {
        return Err("highlight tracks in the Track View hierarchy first".into());
    }
    let mut nodes: Vec<NodeId> = paths.iter().map(|t| t.node).collect();
    nodes.sort();
    nodes.dedup();
    edit_nodes(state, label, &nodes, |t| paths.contains(t), |_, p| {
        if let Some(k) = p.keys().is_some_and(|k| k.has_keys()).then_some(()).and_then(|_| p.ensure_keys()) {
            f(k);
        }
    })
}

/// Edits every highlighted track, converting constant tracks to keyed tracks if needed.
fn edit_highlighted_all(state: &mut AppState, label: &str, mut f: impl FnMut(&mut KeyedTracks, f32)) -> Result<Value, String> {
    let paths = highlighted_tracks(state);
    if paths.is_empty() {
        return Err("highlight tracks in the Track View hierarchy first".into());
    }
    let now = state.current_frame as f32;
    let mut nodes: Vec<NodeId> = paths.iter().map(|t| t.node).collect();
    nodes.sort();
    nodes.dedup();
    edit_nodes(state, label, &nodes, |t| paths.contains(t), |_, p| {
        if let Some(k) = p.ensure_keys() {
            f(k, now);
        }
    })
}

fn snap(state: &AppState, t: f32) -> f32 {
    if state.track_view.snap_frames { t.round() } else { t }
}

/// Reduce Keys: the fewest keys (frames of the old curve, Custom
/// slopes from it) that stay within `threshold` of the old curve.
fn reduce(tr: &mut FloatTrack, threshold: f32) {
    if tr.keys.len() < 3 {
        return;
    }
    let (a, b) = (tr.keys[0].time.floor() as i32, tr.keys.last().unwrap().time.ceil() as i32);
    let old = tr.clone();
    let v = |t: f32| old.eval(t).unwrap_or(0.0);
    let slope = |t: f32| {
        let (lo, hi) = ((t - 0.5).max(a as f32), (t + 0.5).min(b as f32));
        if hi > lo { (v(hi) - v(lo)) / (hi - lo) } else { 0.0 }
    };
    let make = |t: f32| Key { time: t, value: v(t), in_type: TangentType::Custom, out_type: TangentType::Custom, in_tan: slope(t), out_tan: slope(t), ..Default::default() };
    let mut picked: Vec<f32> = vec![a as f32, b as f32];
    loop {
        let mut cand = FloatTrack { keys: picked.iter().map(|t| make(*t)).collect(), ..old.clone() };
        cand.sort();
        let (mut worst, mut at) = (0.0f32, None);
        for f in a..=b {
            let e = (cand.eval(f as f32).unwrap_or(0.0) - v(f as f32)).abs();
            if e > worst {
                worst = e;
                at = Some(f as f32);
            }
        }
        match at {
            Some(t) if worst > threshold && !picked.contains(&t) => picked.push(t),
            _ => {
                tr.keys = cand.keys;
                return;
            }
        }
        picked.sort_by(f32::total_cmp);
    }
}

fn time_block(state: &AppState) -> Result<(f32, f32), String> {
    state.track_view.time_selection.ok_or_else(|| "select a block of time first (Select Time)".into())
}

/// Whether key time `t` lies in the block, with the end point exclusions.
fn in_block(t: f32, (s, e): (f32, f32), ex_l: bool, ex_r: bool) -> bool {
    let after_s = if ex_l { t > s + SAME } else { t >= s - SAME };
    let before_e = if ex_r { t < e - SAME } else { t <= e + SAME };
    after_s && before_e
}

fn copy_block(state: &AppState, blk: (f32, f32)) -> TimeClip {
    let (ex_l, ex_r) = (state.track_view.exclude_left, state.track_view.exclude_right);
    let mut out = Vec::new();
    for t in highlighted_tracks(state) {
        let Some(p) = tracks::read(&state.scene, &t) else { continue };
        let Some(k) = p.keys() else { continue };
        for (c, tr) in k.tracks.iter().enumerate() {
            let keys: Vec<Key> = tr.keys.iter().filter(|k| in_block(k.time, blk, ex_l, ex_r)).map(|k| Key { time: k.time - blk.0, ..k.clone() }).collect();
            if !keys.is_empty() {
                out.push((t.key(), c as u32, keys, tr.eval(blk.0).unwrap_or(0.0)));
            }
        }
    }
    TimeClip { length: blk.1 - blk.0, tracks: out }
}

/// Removes the block (keys inside it), pulling later keys back by its length.
fn delete_block(state: &mut AppState, label: &str, blk: (f32, f32)) -> Result<Value, String> {
    let (ex_l, ex_r) = (state.track_view.exclude_left, state.track_view.exclude_right);
    let len = blk.1 - blk.0;
    edit_highlighted(state, label, |k| {
        for tr in &mut k.tracks {
            tr.keys.retain(|key| !in_block(key.time, blk, ex_l, ex_r));
            for key in &mut tr.keys {
                if key.time >= blk.1 - SAME {
                    key.time -= len;
                }
            }
            tr.keys.dedup_by(|a, b| (a.time - b.time).abs() < SAME);
        }
    })
}

fn toggle(flag: &mut bool, name: &str) -> Result<Value, String> {
    *flag = !*flag;
    Ok(json!({ name: *flag }))
}

fn set_tool(state: &mut AppState, t: TvTool) -> Result<Value, String> {
    state.track_view.tool = t;
    Ok(json!({ "tool": format!("{t:?}") }))
}

fn open_dialog(state: &mut AppState, d: Dialog) -> Result<Value, String> {
    state.dialog = Some(d);
    Ok(json!({ "dialog": format!("{d:?}") }))
}

fn set_data(state: &mut AppState, label: &str, f: impl FnOnce(&mut tracks::TrackData) -> Result<(), String>) -> Result<Value, String> {
    let before = state.scene.track_data.clone();
    let mut after = before.clone();
    f(&mut after)?;
    if after != before {
        commit(state, label, Change::SetTrackData { before: Box::new(before), after: Box::new(after) })?;
    }
    Ok(json!({}))
}

fn highlight(state: &mut AppState, items: Vec<String>, op: SelectOp) {
    let h = &mut state.track_view.highlighted;
    match op {
        SelectOp::Replace => *h = items,
        SelectOp::Add => {
            for i in items {
                if !h.contains(&i) {
                    h.push(i);
                }
            }
        }
        SelectOp::Subtract => h.retain(|i| !items.contains(i)),
    }
}

/// The objects the highlight names (an object, or the object of a track).
fn highlighted_objects(state: &AppState) -> Vec<NodeId> {
    let mut out = Vec::new();
    for i in &state.track_view.highlighted {
        if let Some(Ok(id)) = i.split(':').next().map(|p| p.parse::<u64>()) {
            let id = NodeId(id);
            if !out.contains(&id) && state.scene.node(id).is_some() {
                out.push(id);
            }
        }
    }
    if out.is_empty() {
        out = state.selection.iter().copied().collect();
    }
    out
}

fn key_extents(state: &AppState) -> Option<(f32, f32, f32, f32)> {
    let mut e: Option<(f32, f32, f32, f32)> = None;
    for item in tree(state) {
        let TvItemKind::Track(t) = item.kind else { continue };
        let Some(p) = tracks::read(&state.scene, &t) else { continue };
        let Some(k) = p.keys() else { continue };
        for key in k.tracks.iter().flat_map(|tr| tr.keys.iter()) {
            e = Some(match e {
                None => (key.time, key.time, key.value, key.value),
                Some((a, b, c, d)) => (a.min(key.time), b.max(key.time), c.min(key.value), d.max(key.value)),
            });
        }
    }
    e
}

pub fn execute(state: &mut AppState, cmd: Command) -> Result<Value, String> {
    use Command as C;
    let tv_json = |state: &AppState| json!({ "highlighted": state.track_view.highlighted, "selected_keys": state.track_view.selected_keys.len() });
    match cmd {
        C::OpenCurveEditor | C::TrackViewFloater => {
            state.track_view.open = true;
            state.track_view.mode = TvMode::CurveEditor;
            Ok(json!({ "open": true, "mode": "Curve Editor" }))
        }
        C::OpenDopeSheet => {
            state.track_view.open = true;
            state.track_view.mode = TvMode::DopeSheet;
            Ok(json!({ "open": true, "mode": "Dope Sheet" }))
        }
        C::CloseTrackView => {
            state.track_view.open = false;
            state.track_view.focused = false;
            Ok(json!({ "open": false }))
        }
        C::FunctionCurvesMode => {
            state.track_view.mode = TvMode::CurveEditor;
            Ok(json!({ "mode": "Curve Editor" }))
        }
        C::EditKeysMode | C::EditRangesMode | C::EditTimeMode => {
            state.track_view.mode = TvMode::DopeSheet;
            state.track_view.dope = match cmd {
                C::EditRangesMode => DopeMode::EditRanges,
                C::EditTimeMode => DopeMode::EditTime,
                _ => DopeMode::EditKeys,
            };
            if state.track_view.dope == DopeMode::EditTime {
                state.track_view.tool = TvTool::SelectTime;
            }
            Ok(json!({ "dope": format!("{:?}", state.track_view.dope) }))
        }
        C::TrackViewHighlight { items, op } => {
            highlight(state, items, op);
            Ok(tv_json(state))
        }
        C::TrackViewSelectKeys { keys, op } => {
            if state.track_view.lock_selection {
                return Ok(tv_json(state));
            }
            let refs: Vec<KeyRef> = keys.iter().map(|k| resolve(state, k)).collect::<Result<_, _>>()?;
            let sel = &mut state.track_view.selected_keys;
            match op {
                SelectOp::Replace => *sel = refs,
                SelectOp::Add => {
                    for r in refs {
                        if !sel.contains(&r) {
                            sel.push(r);
                        }
                    }
                }
                SelectOp::Subtract => sel.retain(|s| !refs.iter().any(|r| r.track == s.track && r.comp == s.comp && (r.time - s.time).abs() < SAME)),
            }
            Ok(tv_json(state))
        }
        C::TrackViewSelectKeysInRange { start, end, low, high, op } => {
            if state.track_view.lock_selection {
                return Ok(tv_json(state));
            }
            let mut refs = Vec::new();
            for t in highlighted_tracks(state) {
                let Some(p) = tracks::read(&state.scene, &t) else { continue };
                let Some(k) = p.keys() else { continue };
                for (c, tr) in k.tracks.iter().enumerate() {
                    for key in &tr.keys {
                        let in_v = low.is_none_or(|l| key.value >= l) && high.is_none_or(|h| key.value <= h);
                        if key.time >= start - SAME && key.time <= end + SAME && in_v {
                            refs.push(KeyRef { track: t.key(), comp: c as u32, time: key.time });
                        }
                    }
                }
            }
            let sel = &mut state.track_view.selected_keys;
            match op {
                SelectOp::Replace => *sel = refs,
                SelectOp::Add => sel.extend(refs),
                SelectOp::Subtract => sel.retain(|s| !refs.contains(s)),
            }
            Ok(tv_json(state))
        }
        C::TrackViewInsertKey { id, track, time, value, comp } => {
            let t = crate::controllers::find_track(state, NodeId(id), &track)?;
            let time = snap(state, time);
            let tangents = state.default_tangent;
            let r = edit_nodes(state, "Insert Keys", &[NodeId(id)], |x| *x == t, |_, p| {
                let cur = p.comps_at(time);
                if let Some(k) = p.ensure_keys() {
                    for (c, tr) in k.tracks.iter_mut().enumerate() {
                        if comp.is_none_or(|cc| cc as usize == c) || k.linked {
                            tr.set_key(time, if comp.is_none_or(|cc| cc as usize == c) { value.unwrap_or(cur[c]) } else { cur[c] }, tangents);
                        }
                    }
                }
            })?;
            Ok(r)
        }
        C::TrackViewMoveKeys { dt, dv } => {
            let snap_f = state.track_view.snap_frames;
            let soft = state.track_view.soft.then_some((state.track_view.soft_range, state.track_view.soft_falloff));
            edit_selected(state, "Move Keys", |tr, idx| move_keys(tr, idx, dt, dv, snap_f, soft))
        }
        C::NudgeKeysLeft | C::NudgeKeysRight => {
            let dt = if matches!(cmd, C::NudgeKeysLeft) { -1.0 } else { 1.0 };
            let soft = state.track_view.soft.then_some((state.track_view.soft_range, state.track_view.soft_falloff));
            edit_selected(state, "Nudge Keys", |tr, idx| move_keys(tr, idx, dt, 0.0, false, soft))
        }
        C::TrackViewSlideKeys { dt } => {
            let snap_f = state.track_view.snap_frames;
            edit_selected(state, "Slide Keys", |tr, idx| {
                let Some(first) = idx.iter().map(|i| tr.keys[*i].time).reduce(f32::min) else { return Vec::new() };
                let last = idx.iter().map(|i| tr.keys[*i].time).fold(f32::MIN, f32::max);
                // Everything toward the dragged-to end moves with the selection.
                let moving: Vec<usize> = (0..tr.keys.len()).filter(|i| if dt >= 0.0 { tr.keys[*i].time >= first - SAME } else { tr.keys[*i].time <= last + SAME }).collect();
                let sel_t: Vec<f32> = idx.iter().map(|i| tr.keys[*i].time).collect();
                move_keys(tr, &moving, dt, 0.0, snap_f, None).into_iter().filter(|t| sel_t.iter().any(|s| (s + if snap_f { dt.round() } else { dt } - t).abs() < SAME * 10.0)).collect()
            })
        }
        C::TrackViewScaleKeys { factor, origin } => {
            let o = origin.unwrap_or(state.current_frame as f32);
            let snap_f = state.track_view.snap_frames;
            edit_selected(state, "Scale Keys", |tr, idx| {
                let mut out = Vec::new();
                for i in idx {
                    let mut t = o + (tr.keys[*i].time - o) * factor;
                    if snap_f {
                        t = t.round();
                    }
                    tr.keys[*i].time = t;
                    tr.keys[*i].in_tan /= factor;
                    tr.keys[*i].out_tan /= factor;
                    out.push(t);
                }
                tr.sort();
                out
            })
        }
        C::TrackViewScaleValues { factor, origin } => edit_selected(state, "Scale Values", |tr, idx| {
            for i in idx {
                let k = &mut tr.keys[*i];
                k.value = origin + (k.value - origin) * factor;
                k.in_tan *= factor;
                k.out_tan *= factor;
            }
            idx.iter().map(|i| tr.keys[*i].time).collect()
        }),
        C::AlignKeys => {
            let now = state.current_frame as f32;
            edit_selected(state, "Align Keys", |tr, idx| {
                let Some(first) = idx.iter().map(|i| tr.keys[*i].time).reduce(f32::min) else { return Vec::new() };
                move_keys(tr, idx, now - first, 0.0, false, None)
            })
        }
        C::TrackViewDeleteKeys => {
            let r = edit_selected(state, "Delete Keys", |tr, idx| {
                let times: Vec<f32> = idx.iter().map(|i| tr.keys[*i].time).collect();
                tr.keys.retain(|k| !times.iter().any(|t| (t - k.time).abs() < SAME));
                Vec::new()
            });
            state.track_view.selected_keys.clear();
            r
        }
        C::TrackViewSetTangents { tangent, side } => {
            let side = side.unwrap_or_else(|| "both".into());
            edit_selected(state, "Set Tangents", |tr, idx| tangent_set(tr, idx, tangent, &side))
        }
        C::SetTangentsToAuto => edit_selected(state, "Set Tangents to Auto", |tr, idx| tangent_set(tr, idx, TangentType::Auto, "both")),
        C::SetTangentsToFast => edit_selected(state, "Set Tangents to Fast", |tr, idx| tangent_set(tr, idx, TangentType::Fast, "both")),
        C::SetTangentsToLinear | C::Linear => edit_selected(state, "Set Tangents to Linear", |tr, idx| tangent_set(tr, idx, TangentType::Linear, "both")),
        C::SetTangentsToSlow => edit_selected(state, "Set Tangents to Slow", |tr, idx| tangent_set(tr, idx, TangentType::Slow, "both")),
        C::EaseIn => edit_selected(state, "Ease In", |tr, idx| tangent_set(tr, idx, TangentType::Slow, "in")),
        C::EaseOut => edit_selected(state, "Ease Out", |tr, idx| tangent_set(tr, idx, TangentType::Slow, "out")),
        C::SetTangentsToSmooth => edit_selected(state, "Set Tangents to Smooth", |tr, idx| tangent_set(tr, idx, TangentType::Smooth, "both")),
        C::SetTangentsToStepped => edit_selected(state, "Set Tangents to Stepped", |tr, idx| tangent_set(tr, idx, TangentType::Step, "both")),
        C::SetTangentsToSpline => edit_selected(state, "Set Tangents to Spline", |tr, idx| {
            // Spline (Custom) starts from the slope the key has now.
            for i in idx {
                let t = tr.keys[*i].time;
                let (a, b) = (tr.eval(t - 0.5).unwrap_or(0.0), tr.eval(t + 0.5).unwrap_or(0.0));
                let k = &mut tr.keys[*i];
                if k.in_type != TangentType::Custom {
                    k.in_tan = b - a;
                }
                if k.out_type != TangentType::Custom {
                    k.out_tan = b - a;
                }
            }
            tangent_set(tr, idx, TangentType::Custom, "both")
        }),
        C::BreakTangents | C::UnifyTangents => {
            let broken = matches!(cmd, C::BreakTangents);
            edit_selected(state, if broken { "Break Tangents" } else { "Unify Tangents" }, |tr, idx| {
                for i in idx {
                    let k = &mut tr.keys[*i];
                    k.broken = broken;
                    if !broken && k.in_type == TangentType::Custom && k.out_type == TangentType::Custom {
                        let m = (k.in_tan + k.out_tan) / 2.0;
                        k.in_tan = m;
                        k.out_tan = m;
                    }
                }
                idx.iter().map(|i| tr.keys[*i].time).collect()
            })
        }
        C::TrackViewSetKeyValues { time, value, relative } => {
            let rel = relative.unwrap_or(false);
            let snap_f = state.track_view.snap_frames;
            edit_selected(state, "Key Value", |tr, idx| {
                let mut out = Vec::new();
                for i in idx {
                    let k = &mut tr.keys[*i];
                    if let Some(v) = value {
                        k.value = if rel { k.value + v } else { v };
                    }
                    if let Some(t) = time {
                        let t = if rel { k.time + t } else { t };
                        k.time = if snap_f { t.round() } else { t };
                    }
                    out.push(k.time);
                }
                tr.sort();
                out
            })
        }
        C::TrackViewSetKeyInfo { in_type, out_type, in_tan, out_tan, tension, continuity, bias, ease_to, ease_from } => edit_selected(state, "Key Info", |tr, idx| {
            for i in idx {
                let k = &mut tr.keys[*i];
                if let Some(v) = in_type { k.in_type = v; }
                if let Some(v) = out_type { k.out_type = v; }
                if let Some(v) = in_tan { k.in_tan = v; }
                if let Some(v) = out_tan { k.out_tan = v; }
                if let Some(v) = tension { k.tension = v.clamp(0.0, 50.0); }
                if let Some(v) = continuity { k.continuity = v.clamp(0.0, 50.0); }
                if let Some(v) = bias { k.bias = v.clamp(0.0, 50.0); }
                if let Some(v) = ease_to { k.ease_to = v.clamp(0.0, 50.0); }
                if let Some(v) = ease_from { k.ease_from = v.clamp(0.0, 50.0); }
            }
            idx.iter().map(|i| tr.keys[*i].time).collect()
        }),
        C::TrackViewNormalizeTime => {
            let snap_f = state.track_view.snap_frames;
            edit_selected(state, "Normalize Time", |tr, idx| {
                let mut idx = idx.to_vec();
                idx.sort_by(|a, b| tr.keys[*a].time.total_cmp(&tr.keys[*b].time));
                if idx.len() < 3 {
                    return idx.iter().map(|i| tr.keys[*i].time).collect();
                }
                let (a, b) = (tr.keys[idx[0]].time, tr.keys[*idx.last().unwrap()].time);
                let n = (idx.len() - 1) as f32;
                let mut out = Vec::new();
                for (j, i) in idx.iter().enumerate() {
                    let mut t = a + (b - a) * j as f32 / n;
                    if snap_f {
                        t = t.round();
                    }
                    tr.keys[*i].time = t;
                    out.push(t);
                }
                tr.sort();
                out
            })
        }
        C::TrackViewSetConstantVelocity { on } => edit_selected(state, "Constant Velocity", |tr, idx| {
            for i in idx {
                tr.keys[*i].constant_velocity = on;
            }
            idx.iter().map(|i| tr.keys[*i].time).collect()
        }),
        C::TrackViewSetOrt { before, after } => {
            let target = state.track_view.ort_target;
            edit_highlighted(state, "Parameter Curve Out-of-Range Types", |k| {
                for tr in &mut k.tracks {
                    match target {
                        1 => {
                            if let Some(ease) = &mut tr.ease_curve {
                                ease.before = before;
                                ease.after = after;
                            } else {
                                tr.before = before;
                                tr.after = after;
                            }
                        }
                        2 => {
                            if let Some(mult) = &mut tr.multiplier_curve {
                                mult.before = before;
                                mult.after = after;
                            } else {
                                tr.before = before;
                                tr.after = after;
                            }
                        }
                        _ => {
                            tr.before = before;
                            tr.after = after;
                        }
                    }
                }
            })
        }
        C::ParameterCurveOutOfRangeTypes => {
            state.track_view.ort_target = 0;
            open_dialog(state, Dialog::OutOfRangeTypes)
        }
        C::EaseCurveOutOfRangeTypes => {
            state.track_view.ort_target = 1;
            open_dialog(state, Dialog::OutOfRangeTypes)
        }
        C::MultiplierCurveOutOfRangeTypes => {
            state.track_view.ort_target = 2;
            open_dialog(state, Dialog::OutOfRangeTypes)
        }
        C::ApplyEaseCurve => {
            let (s, e) = (state.frame_range.0 as f32, state.frame_range.1 as f32);
            edit_highlighted_all(state, "Apply Ease Curve", |k, _| {
                for tr in &mut k.tracks {
                    let mut ease = FloatTrack::new(loom_scene::anim::Interp::Bezier);
                    ease.set_key(s, s, (TangentType::Linear, TangentType::Linear));
                    ease.set_key(e, e, (TangentType::Linear, TangentType::Linear));
                    tr.ease_curve = Some(Box::new(ease));
                    tr.ease_multiplier_disabled = false;
                }
            })
        }
        C::ApplyMultiplierCurve => {
            let (s, e) = (state.frame_range.0 as f32, state.frame_range.1 as f32);
            edit_highlighted_all(state, "Apply Multiplier Curve", |k, _| {
                for tr in &mut k.tracks {
                    let mut mult = FloatTrack::new(loom_scene::anim::Interp::Bezier);
                    mult.set_key(s, 1.0, (TangentType::Auto, TangentType::Auto));
                    mult.set_key(e, 1.0, (TangentType::Auto, TangentType::Auto));
                    tr.multiplier_curve = Some(Box::new(mult));
                    tr.ease_multiplier_disabled = false;
                }
            })
        }
        C::DeleteEaseMultiplierCurve => edit_highlighted_all(state, "Delete Ease/Multiplier Curve", |k, _| {
            for tr in &mut k.tracks {
                tr.ease_curve = None;
                tr.multiplier_curve = None;
                tr.ease_multiplier_disabled = false;
            }
        }),
        C::EaseMultiplierCurveEnableToggle => edit_highlighted_all(state, "Ease/Multiplier Curve Enable Toggle", |k, _| {
            for tr in &mut k.tracks {
                tr.ease_multiplier_disabled = !tr.ease_multiplier_disabled;
            }
        }),
        C::CopyLimitOnly => {
            let paths = highlighted_tracks(state);
            let Some(first) = paths.first() else { return Err("highlight a track first".into()) };
            let p = tracks::read(&state.scene, first).ok_or("no such track")?;
            let lim = p.keys().and_then(|k| k.tracks.first()).map(|tr| (tr.limit_enabled, tr.lower_limit, tr.upper_limit)).unwrap_or((false, None, None));
            state.track_view.limit_clip = Some(lim);
            Ok(json!({ "limit_enabled": lim.0, "lower_limit": lim.1, "upper_limit": lim.2 }))
        }
        C::PasteLimitOnly => {
            let (en, lo, hi) = state.track_view.limit_clip.ok_or("copy a limit first (Copy Limit Only)")?;
            edit_highlighted_all(state, "Paste Limit Only", |k, _| {
                for tr in &mut k.tracks {
                    tr.limit_enabled = en;
                    tr.lower_limit = lo;
                    tr.upper_limit = hi;
                }
            })
        }
        C::RemoveLimit => edit_highlighted_all(state, "Remove Limit", |k, _| {
            for tr in &mut k.tracks {
                tr.limit_enabled = false;
                tr.lower_limit = None;
                tr.upper_limit = None;
            }
        }),
        C::SetLowerLimit => edit_highlighted_all(state, "Set Lower Limit", |k, now| {
            let cur = k.comps_at(now);
            for (tr, v) in k.tracks.iter_mut().zip(cur) {
                tr.lower_limit = Some(v);
                tr.limit_enabled = true;
            }
        }),
        C::SetUpperLimit => edit_highlighted_all(state, "Set Upper Limit", |k, now| {
            let cur = k.comps_at(now);
            for (tr, v) in k.tracks.iter_mut().zip(cur) {
                tr.upper_limit = Some(v);
                tr.limit_enabled = true;
            }
        }),
        C::ToggleLimit => edit_highlighted_all(state, "Toggle Limit", |k, _| {
            let to = k.tracks.first().map(|tr| !tr.limit_enabled).unwrap_or(true);
            for tr in &mut k.tracks {
                tr.limit_enabled = to;
            }
        }),
        C::RespectAnimationRange | C::IgnoreAnimationRange => {
            let respect = matches!(cmd, C::RespectAnimationRange);
            let range = (state.frame_range.0 as f32, state.frame_range.1 as f32);
            edit_highlighted_all(state, if respect { "Respect Animation Range" } else { "Ignore Animation Range" }, |k, _| {
                for tr in &mut k.tracks {
                    tr.respect_range = respect;
                    if respect && tr.range_override.is_none() && tr.keys.len() < 2 {
                        tr.range_override = Some(range);
                    }
                }
            })
        }
        C::PositionRangesMode => {
            state.track_view.mode = TvMode::DopeSheet;
            state.track_view.dope = DopeMode::EditRanges;
            toggle(&mut state.track_view.position_ranges, "position_ranges")
        }
        C::RetimeTool => set_tool(state, TvTool::Retime),
        C::RetimeAllTool => set_tool(state, TvTool::RetimeAll),
        C::RetimeMoveLeft | C::RetimeMoveRight => {
            let dt = if matches!(cmd, C::RetimeMoveLeft) { -1.0 } else { 1.0 };
            if !state.track_view.selected_keys.is_empty() {
                let soft = state.track_view.soft.then_some((state.track_view.soft_range, state.track_view.soft_falloff));
                edit_selected(state, cmd.name(), |tr, idx| move_keys(tr, idx, dt, 0.0, false, soft))
            } else {
                let now = state.current_frame as f32;
                edit_highlighted(state, cmd.name(), |k| {
                    for tr in &mut k.tracks {
                        for key in &mut tr.keys {
                            if key.time >= now - SAME {
                                key.time += dt;
                            }
                        }
                        tr.sort();
                    }
                })
            }
        }
        C::TrackViewReduceKeys { threshold } => edit_highlighted(state, "Reduce Keys", |k| {
            for tr in &mut k.tracks {
                reduce(tr, threshold.max(0.0));
            }
        }),
        C::SimplifyCurve => open_dialog(state, Dialog::ReduceKeys),
        C::TrackViewDrawCurve { id, track, comp, points } => {
            let t = crate::controllers::find_track(state, NodeId(id), &track)?;
            let tangents = state.default_tangent;
            let mut pts: Vec<[f32; 2]> = points.iter().map(|p| [p[0].round(), p[1]]).collect();
            pts.sort_by(|a, b| a[0].total_cmp(&b[0]));
            pts.dedup_by(|a, b| a[0] == b[0]);
            let Some((t0, t1)) = pts.first().zip(pts.last()).map(|(a, b)| (a[0], b[0])) else { return Err("draw at least one point".into()) };
            edit_nodes(state, "Draw Curves", &[NodeId(id)], |x| *x == t, |_, p| {
                if let Some(k) = p.ensure_keys() {
                    if let Some(tr) = k.tracks.get_mut(comp as usize) {
                        tr.keys.retain(|key| key.time < t0 - SAME || key.time > t1 + SAME);
                        for q in &pts {
                            tr.set_key(q[0], q[1], tangents);
                        }
                    }
                }
            })
        }
        C::TrackViewSelectTime { start, end } => {
            state.track_view.time_selection = Some((start.min(end), start.max(end)));
            state.track_view.dope = DopeMode::EditTime;
            Ok(json!({ "time_selection": [start.min(end), start.max(end)] }))
        }
        C::TrackViewInsertTime { at, amount } => edit_highlighted(state, "Insert Time", |k| {
            for tr in &mut k.tracks {
                for key in &mut tr.keys {
                    if key.time >= at - SAME {
                        key.time += amount;
                    }
                }
            }
        }),
        C::ReverseTime => {
            let (s, e) = time_block(state)?;
            edit_highlighted(state, "Reverse Time", |k| {
                for tr in &mut k.tracks {
                    for key in &mut tr.keys {
                        if key.time >= s - SAME && key.time <= e + SAME {
                            key.time = s + e - key.time;
                            std::mem::swap(&mut key.in_type, &mut key.out_type);
                            let (a, b) = (key.in_tan, key.out_tan);
                            key.in_tan = -b;
                            key.out_tan = -a;
                        }
                    }
                    tr.sort();
                }
            })
        }
        C::TrackViewScaleTime { factor } => {
            if factor <= 0.0 {
                return Err("scale must be positive".into());
            }
            let (s, e) = time_block(state)?;
            let shift = (e - s) * (factor - 1.0);
            let r = edit_highlighted(state, "Scale Time", |k| {
                for tr in &mut k.tracks {
                    for key in &mut tr.keys {
                        if key.time > e + SAME {
                            key.time += shift;
                        } else if key.time >= s - SAME {
                            key.time = s + (key.time - s) * factor;
                            key.in_tan /= factor;
                            key.out_tan /= factor;
                        }
                    }
                    tr.sort();
                }
            })?;
            state.track_view.time_selection = Some((s, s + (e - s) * factor));
            Ok(r)
        }
        C::CopyTime => {
            let blk = time_block(state)?;
            let clip = copy_block(state, blk);
            let n = clip.tracks.len();
            state.track_view.time_clip = Some(clip);
            Ok(json!({ "tracks": n }))
        }
        C::CutTime => {
            let blk = time_block(state)?;
            state.track_view.time_clip = Some(copy_block(state, blk));
            delete_block(state, "Cut Time", blk)
        }
        C::TrackViewDeleteTime => {
            let blk = time_block(state)?;
            delete_block(state, "Delete Time", blk)
        }
        C::PasteTime => {
            let at = state.track_view.time_selection.map(|b| b.0).unwrap_or(state.current_frame as f32);
            paste_time(state, at, false)
        }
        C::TrackViewPasteTime { at, relative } => {
            let at = at.or(state.track_view.time_selection.map(|b| b.0)).unwrap_or(state.current_frame as f32);
            paste_time(state, at, relative.unwrap_or(false))
        }
        C::ExcludeLeftEndPointToggle => toggle(&mut state.track_view.exclude_left, "exclude_left"),
        C::ExcludeRightEndPointToggle => toggle(&mut state.track_view.exclude_right, "exclude_right"),
        C::TrackViewMoveRange { dt } => {
            let decoupled = state.track_view.position_ranges;
            let default_range = (state.frame_range.0 as f32, state.frame_range.1 as f32);
            edit_highlighted(state, "Move Range", |k| {
                for tr in &mut k.tracks {
                    if decoupled {
                        let (r0, r1) = tr.range_override.unwrap_or_else(|| {
                            if tr.keys.len() >= 2 {
                                (tr.keys[0].time, tr.keys[tr.keys.len() - 1].time)
                            } else {
                                default_range
                            }
                        });
                        tr.range_override = Some((r0 + dt, r1 + dt));
                    } else {
                        for key in &mut tr.keys {
                            key.time += dt;
                        }
                    }
                }
            })
        }
        C::TrackViewScaleRange { factor, origin } => {
            let decoupled = state.track_view.position_ranges;
            let default_range = (state.frame_range.0 as f32, state.frame_range.1 as f32);
            edit_highlighted(state, "Scale Range", |k| {
                for tr in &mut k.tracks {
                    if decoupled {
                        let (r0, r1) = tr.range_override.unwrap_or_else(|| {
                            if tr.keys.len() >= 2 {
                                (tr.keys[0].time, tr.keys[tr.keys.len() - 1].time)
                            } else {
                                default_range
                            }
                        });
                        tr.range_override = Some((origin + (r0 - origin) * factor, origin + (r1 - origin) * factor));
                    } else {
                        for key in &mut tr.keys {
                            key.time = origin + (key.time - origin) * factor;
                        }
                        tr.sort();
                    }
                }
            })
        }
        C::AddNoteTrack => {
            let items = state.track_view.highlighted.clone();
            if items.is_empty() {
                return Err("highlight an item to add a note track under".into());
            }
            let mut adds = Vec::new();
            for i in &items {
                if note_of(i).is_some() {
                    continue;
                }
                let Some(Ok(id)) = i.split(':').next().map(|p| p.parse::<u64>()) else { continue };
                let under = item_tracks(state, i).into_iter().next().filter(|_| i.contains(':')).map(|t| t.key());
                adds.push(NoteTrack { node: NodeId(id), under, keys: Vec::new() });
            }
            set_data(state, "Add Note Track", |d| {
                d.note_tracks.extend(adds);
                Ok(())
            })
        }
        C::DeleteNoteTrack => {
            let mut idx: Vec<usize> = state.track_view.highlighted.iter().filter_map(|i| note_of(i)).collect();
            idx.sort();
            idx.dedup();
            if idx.is_empty() {
                return Err("highlight the note tracks to remove".into());
            }
            state.track_view.highlighted.retain(|i| note_of(i).is_none());
            set_data(state, "Remove Note Track", |d| {
                for i in idx.into_iter().rev() {
                    if i < d.note_tracks.len() {
                        d.note_tracks.remove(i);
                    }
                }
                Ok(())
            })
        }
        C::AddNoteKey { note, time, text } => {
            let t = snap(state, time);
            set_data(state, "Add Note Key", |d| {
                let nt = d.note_tracks.get_mut(note as usize).ok_or("no such note track")?;
                nt.keys.retain(|k| (k.time - t).abs() > SAME);
                nt.keys.push(NoteKey { time: t, text, locked: false });
                nt.keys.sort_by(|a, b| a.time.total_cmp(&b.time));
                Ok(())
            })
        }
        C::SetNoteKey { note, index, text, time, locked } => set_data(state, "Notes", |d| {
            let nt = d.note_tracks.get_mut(note as usize).ok_or("no such note track")?;
            let k = nt.keys.get_mut(index as usize).ok_or("no such note key")?;
            if let Some(t) = text { k.text = t; }
            if let Some(t) = time { k.time = t; }
            if let Some(l) = locked { k.locked = l; }
            nt.keys.sort_by(|a, b| a.time.total_cmp(&b.time));
            Ok(())
        }),
        C::DeleteNoteKey { note, index } => set_data(state, "Delete Note Key", |d| {
            let nt = d.note_tracks.get_mut(note as usize).ok_or("no such note track")?;
            if (index as usize) < nt.keys.len() {
                nt.keys.remove(index as usize);
                Ok(())
            } else {
                Err("no such note key".into())
            }
        }),
        C::AddVisibilityTrack | C::DeleteVisibilityTrack => {
            let add = matches!(cmd, C::AddVisibilityTrack);
            let ids: Vec<NodeId> = highlighted_objects(state);
            let mut changes = Vec::new();
            for id in ids {
                let Some(n) = state.scene.node(id) else { continue };
                let after = if add { n.visibility.clone().or(Some(loom_scene::Param::new(1.0))) } else { None };
                if after != n.visibility {
                    changes.push(Change::SetVisibility { id, before: n.visibility.clone(), after });
                }
            }
            if changes.is_empty() {
                return Err(if add { "the objects already have visibility tracks".into() } else { "no visibility track to remove".to_string() });
            }
            commit(state, if add { "Add Visibility Track" } else { "Remove Visibility Track" }, Change::Group(changes))?;
            Ok(json!({}))
        }
        C::TrackSetCreate { name } => {
            let keys: Vec<TrackKey> = highlighted_tracks(state).iter().map(|t| t.key()).collect();
            set_data(state, "Track Set", |d| {
                d.track_sets.retain(|(n, _)| *n != name);
                d.track_sets.push((name, keys));
                Ok(())
            })
        }
        C::TrackSetDelete { name } => set_data(state, "Delete Track Set", |d| {
            let n0 = d.track_sets.len();
            d.track_sets.retain(|(n, _)| *n != name);
            if d.track_sets.len() == n0 { Err(format!("no track set \"{name}\"")) } else { Ok(()) }
        }),
        C::TrackSetAddHighlighted { ref name } | C::TrackSetRemoveHighlighted { ref name } => {
            let add = matches!(cmd, C::TrackSetAddHighlighted { .. });
            let name = name.clone();
            let keys: Vec<TrackKey> = highlighted_tracks(state).iter().map(|t| t.key()).collect();
            set_data(state, "Track Set", |d| {
                let set = d.track_sets.iter_mut().find(|(n, _)| *n == name).ok_or(format!("no track set \"{name}\""))?;
                if add {
                    for k in keys {
                        if !set.1.contains(&k) {
                            set.1.push(k);
                        }
                    }
                } else {
                    set.1.retain(|k| !keys.contains(k));
                }
                Ok(())
            })
        }
        C::TrackSetSelect { name, op } => {
            let set = state.scene.track_data.track_sets.iter().find(|(n, _)| *n == name).cloned().ok_or(format!("no track set \"{name}\""))?;
            let items: Vec<String> = set.1.iter().filter_map(|k| path_of(state, k)).map(|t| track_id(&t)).collect();
            highlight(state, items, op);
            Ok(tv_json(state))
        }
        C::EditTrackSet => open_dialog(state, Dialog::TrackSets),
        C::KeyablePropertyToggle | C::Locked => {
            let lock = matches!(cmd, C::Locked);
            let paths = highlighted_tracks(state);
            let Some(first) = paths.first() else { return Err("highlight tracks first".into()) };
            let f0 = state.scene.track_data.flags_of(&first.key());
            let to = if lock { !f0.locked } else { !f0.not_keyable };
            set_data(state, if lock { "Lock Tracks" } else { "Keyable" }, |d| {
                for t in &paths {
                    let mut f = d.flags_of(&t.key());
                    if lock {
                        f.locked = to;
                    } else {
                        f.not_keyable = to;
                    }
                    d.set_flags(t.key(), f);
                }
                Ok(())
            })
        }
        C::CopyController => {
            let paths = highlighted_tracks(state);
            let [t] = paths.as_slice() else { return Err("highlight exactly one track to copy".into()) };
            let p = tracks::read(&state.scene, t).ok_or("no such track")?;
            state.track_view.controller_clip = Some((p.ty(), Role::of(t), p.controller_json()));
            Ok(json!({ "copied": t.name }))
        }
        C::PasteController => {
            let (ty, role, c) = state.track_view.controller_clip.clone().ok_or("the controller clipboard is empty")?;
            let paths = highlighted_tracks(state);
            let fits: Vec<TrackPath> = paths.into_iter().filter(|t| t.ty == ty && (Role::of(t) == role || !matches!(role, Role::Position | Role::Rotation | Role::Scale))).collect();
            if fits.is_empty() {
                return Err("no highlighted track takes this controller type".into());
            }
            let mut nodes: Vec<NodeId> = fits.iter().map(|t| t.node).collect();
            nodes.sort();
            nodes.dedup();
            edit_nodes(state, "Paste Controller", &nodes, |t| fits.contains(t), |_, p| {
                let _ = p.set_controller_json(&c);
            })
        }
        C::MakeControllerUnique => {
            let paths = highlighted_tracks(state);
            let mut changes = Vec::new();
            for t in &paths {
                let TrackOwner::Modifier(i) = t.owner else { continue };
                let Some(n) = state.scene.node(t.node) else { continue };
                if n.modifiers.get(i).is_some_and(|s| s.instance_of.is_some()) {
                    let mut after = n.clone();
                    if let Some(m) = state.scene.resolved_modifier(t.node, i) {
                        after.modifiers[i].modifier = m;
                    }
                    after.modifiers[i].instance_of = None;
                    if let Some(c) = node_change(n, &after) {
                        changes.push(c);
                    }
                }
            }
            if changes.is_empty() {
                return Err("the highlighted controllers are not instanced".into());
            }
            commit(state, "Make Controller Unique", Change::Group(changes))?;
            Ok(json!({}))
        }
        C::DeleteController => {
            let paths = highlighted_tracks(state);
            let [t] = paths.as_slice() else { return Err("highlight one track".into()) };
            if t.owner == TrackOwner::Visibility {
                let n = state.scene.node(t.node).ok_or("no such node")?;
                commit(state, "Delete Controller", Change::SetVisibility { id: t.node, before: n.visibility.clone(), after: None })?;
                return Ok(json!({}));
            }
            let p = tracks::read(&state.scene, t).ok_or("no such track")?;
            if p.controller_json().get("List").and_then(|l| l.as_array()).is_some_and(|l| l.len() > 1) {
                return crate::controllers::execute(state, C::ListDeleteController { id: t.node.0, track: t.name.clone(), index: 0 });
            }
            Err(format!("{} can only be replaced (Assign Controller)", t.name))
        }
        C::AssignControllerTrackView => {
            if highlighted_tracks(state).is_empty() {
                return Err("highlight a track to assign a controller to".into());
            }
            open_dialog(state, Dialog::AssignController)
        }
        C::PropertiesTrackView => {
            if state.track_view.selected_keys.is_empty() && highlighted_tracks(state).is_empty() {
                return Err("select keys or highlight a track".into());
            }
            open_dialog(state, Dialog::KeyInfo)
        }
        C::Filters => open_dialog(state, Dialog::TrackViewFilters),
        C::TrackViewUtilities => open_dialog(state, Dialog::TrackViewUtilities),
        C::TrackViewFilter { name, on } => {
            let f = &mut state.track_view.filters;
            let flag = match name.to_ascii_lowercase().as_str() {
                "transforms" => &mut f.transforms,
                "base objects" => &mut f.base_objects,
                "modified objects" => &mut f.modified_objects,
                "note tracks" => &mut f.note_tracks,
                "visibility tracks" => &mut f.visibility_tracks,
                "controller types" => &mut f.controller_types,
                "animated tracks" => &mut f.animated_only,
                "selected objects" => &mut f.selected_objects,
                "visible objects" => &mut f.visible_objects,
                "keyable tracks" => &mut f.keyable_only,
                "unlocked attributes" => &mut f.unlocked_only,
                "selected tracks" => {
                    f.selected_tracks = on;
                    let snap: Vec<TrackKey> = highlighted_tracks(state).iter().map(|t| t.key()).collect();
                    state.track_view.filters.selected_snapshot = snap;
                    return Ok(json!({ "selected tracks": on }));
                }
                other => return Err(format!("no filter \"{other}\"")),
            };
            *flag = on;
            Ok(json!({ name: on }))
        }
        C::FilterAnimatedTracksToggle => toggle(&mut state.track_view.filters.animated_only, "animated_only"),
        C::FilterKeyableTracksToggle => toggle(&mut state.track_view.filters.keyable_only, "keyable_only"),
        C::FilterSelectedObjectsToggle => toggle(&mut state.track_view.filters.selected_objects, "selected_objects"),
        C::FilterUnlockedAttributesToggle | C::UnlockedAttributesToggle => toggle(&mut state.track_view.filters.unlocked_only, "unlocked_only"),
        C::FilterVisibleObjectsToggle => toggle(&mut state.track_view.filters.visible_objects, "visible_objects"),
        C::FilterActiveLayerToggle => toggle(&mut state.track_view.filters.active_layer_only, "active_layer_only"),
        C::FilterSelectedTracksToggle => {
            let snap: Vec<TrackKey> = highlighted_tracks(state).iter().map(|t| t.key()).collect();
            let f = &mut state.track_view.filters;
            f.selected_tracks = !f.selected_tracks;
            if f.selected_tracks {
                f.selected_snapshot = snap;
            }
            Ok(json!({ "selected_tracks": f.selected_tracks }))
        }
        C::TrackViewOption { name, on } => {
            let tv = &mut state.track_view;
            let flag = match name.to_ascii_lowercase().as_str() {
                "interactive update" => &mut tv.interactive_update,
                "sync cursor time" => {
                    tv.sync_cursor_time = on;
                    if on {
                        tv.interactive_update = true;
                    }
                    return Ok(json!({ name: on }));
                }
                "manual navigation" => &mut tv.manual_navigation,
                "hide non-selected curves" => &mut tv.hide_nonselected,
                "show non-selected curves" => {
                    tv.hide_nonselected = !on;
                    return Ok(json!({ name: on }));
                }
                "use soft select" => &mut tv.soft,
                "auto expand animated" => &mut tv.auto_expand_animated,
                "auto expand keyable" => &mut tv.auto_expand_keyable,
                "modify subtree" => &mut tv.modify_subtree,
                other => return Err(format!("no Track View option \"{other}\"")),
            };
            *flag = on;
            Ok(json!({ name: on }))
        }
        C::TrackViewSoftSelection { soft, range, falloff } => {
            let tv = &mut state.track_view;
            if let Some(v) = soft { tv.soft = v; }
            if let Some(v) = range { tv.soft_range = v.max(0.0); }
            if let Some(v) = falloff { tv.soft_falloff = v.max(0.0); }
            Ok(json!({ "soft": tv.soft, "range": tv.soft_range, "falloff": tv.soft_falloff }))
        }
        C::AutoExpandAnimated => toggle(&mut state.track_view.auto_expand_animated, "auto_expand_animated"),
        C::AutoExpandKeyable => toggle(&mut state.track_view.auto_expand_keyable, "auto_expand_keyable"),
        C::AutoExpandUnlocked => toggle(&mut state.track_view.auto_expand_unlocked, "auto_expand_unlocked"),
        C::AutoExpandLimits => toggle(&mut state.track_view.auto_expand_limits, "auto_expand_limits"),
        C::FreezeNonselectedCurves => toggle(&mut state.track_view.freeze_nonselected, "freeze_nonselected"),
        C::ShowKeysOnFrozenCurves => toggle(&mut state.track_view.keys_on_frozen, "keys_on_frozen"),
        C::ShowSelectedKeyStats => toggle(&mut state.track_view.key_stats, "key_stats"),
        C::ShowTangentsToggle => toggle(&mut state.track_view.show_tangents, "show_tangents"),
        C::ViewAllTangents => toggle(&mut state.track_view.all_tangents, "all_tangents"),
        C::LockTangentsToggle => toggle(&mut state.track_view.lock_tangents, "lock_tangents"),
        C::LockSelection => toggle(&mut state.track_view.lock_selection, "lock_selection"),
        C::SnapFrames => toggle(&mut state.track_view.snap_frames, "snap_frames"),
        C::ModifySubtreeToggle => toggle(&mut state.track_view.modify_subtree, "modify_subtree"),
        C::IsolateCurve => toggle(&mut state.track_view.isolate, "isolate"),
        C::AddKeys => set_tool(state, TvTool::AddKeys),
        C::MoveKeys => set_tool(state, TvTool::Move),
        C::MoveKeysHorizontal => set_tool(state, TvTool::MoveHorizontal),
        C::MoveKeysVertical => set_tool(state, TvTool::MoveVertical),
        C::SlideKeys => set_tool(state, TvTool::Slide),
        C::ScaleKeys => set_tool(state, TvTool::ScaleKeys),
        C::ScaleValues => set_tool(state, TvTool::ScaleValues),
        C::RegionKeysTool => set_tool(state, TvTool::Region),
        C::DrawCurves => set_tool(state, TvTool::DrawCurves),
        C::PanTrackView => set_tool(state, TvTool::Pan),
        C::Zoom => set_tool(state, TvTool::Zoom),
        C::ZoomTime => set_tool(state, TvTool::ZoomTime),
        C::ZoomValues => set_tool(state, TvTool::ZoomValues),
        C::ZoomRegion => set_tool(state, TvTool::ZoomRegion),
        C::SelectTime => {
            state.track_view.mode = TvMode::DopeSheet;
            state.track_view.dope = DopeMode::EditTime;
            set_tool(state, TvTool::SelectTime)
        }
        C::InsertTime => set_tool(state, TvTool::InsertTime),
        C::ScaleTime => set_tool(state, TvTool::ScaleTime),
        C::AccessHierarchySelectNameField => focus(state, "hierarchy name"),
        C::AccessTimeField => focus(state, "time"),
        C::AccessTrackNameField => focus(state, "track view name"),
        C::AccessValueField => focus(state, "value"),
        C::TrackViewSetView { start, end, low, high } => {
            if end <= start || high <= low {
                return Err("the view needs a positive size".into());
            }
            state.track_view.view = [start, end, low, high];
            Ok(json!({ "view": state.track_view.view }))
        }
        C::FrameHorizontalExtents => {
            let (a, b) = state.frame_range;
            state.track_view.view[0] = a as f32;
            state.track_view.view[1] = b as f32;
            Ok(json!({ "view": state.track_view.view }))
        }
        C::FrameHorizontalExtentsKeys => {
            let (a, b, _, _) = key_extents(state).ok_or("no keys to frame")?;
            let pad = ((b - a) * 0.05).max(1.0);
            state.track_view.view[0] = a - pad;
            state.track_view.view[1] = b + pad;
            Ok(json!({ "view": state.track_view.view }))
        }
        C::FrameValueExtents => {
            let (_, _, lo, hi) = key_extents(state).ok_or("no keys to frame")?;
            let pad = ((hi - lo) * 0.05).max(1.0);
            state.track_view.view[2] = lo - pad;
            state.track_view.view[3] = hi + pad;
            Ok(json!({ "view": state.track_view.view }))
        }
        C::ZoomSelectedObject => {
            let Some(id) = state.selection.iter().next().copied() else { return Err("select an object".into()) };
            state.track_view.collapsed.remove(&format!("{}", id.0));
            let rows = tree(state);
            state.track_view.scroll = rows.iter().position(|i| i.kind == TvItemKind::Object(id)).unwrap_or(0);
            Ok(json!({ "scroll": state.track_view.scroll }))
        }
        C::ScrollUp | C::ScrollDown => {
            let n = tree(state).len();
            let s = &mut state.track_view.scroll;
            *s = if matches!(cmd, C::ScrollUp) { s.saturating_sub(1) } else { (*s + 1).min(n.saturating_sub(1)) };
            Ok(json!({ "scroll": *s }))
        }
        C::MoveHighlightUp | C::MoveHighlightDown => {
            let rows = tree(state);
            if rows.is_empty() {
                return Ok(json!({}));
            }
            let cur = state.track_view.highlighted.last().and_then(|h| rows.iter().position(|r| &r.id == h)).unwrap_or(state.track_view.cursor.min(rows.len() - 1));
            let next = if matches!(cmd, C::MoveHighlightUp) { cur.saturating_sub(1) } else { (cur + 1).min(rows.len() - 1) };
            state.track_view.cursor = next;
            state.track_view.highlighted = vec![rows[next].id.clone()];
            Ok(tv_json(state))
        }
        C::MoveObjectUp | C::MoveObjectDown => {
            let Some(id) = highlighted_objects(state).first().copied() else { return Err("highlight an object".into()) };
            let mut order = object_order(state);
            let i = order.iter().position(|x| *x == id).unwrap();
            let j = if matches!(cmd, C::MoveObjectUp) { i.saturating_sub(1) } else { (i + 1).min(order.len() - 1) };
            order.swap(i, j);
            state.track_view.object_order = order;
            Ok(json!({}))
        }
        C::ExpandAll | C::ExpandObjects | C::ExpandTracks | C::CollapseAllTrackView | C::CollapseObjects | C::CollapseTracks | C::ExpandObjectToggle | C::ExpandTrackToggle => expand(state, &cmd),
        C::SelectAllTrackView => {
            state.track_view.highlighted = tree(state).into_iter().map(|i| i.id).collect();
            Ok(tv_json(state))
        }
        C::SelectNoneTrackView => {
            state.track_view.highlighted.clear();
            Ok(tv_json(state))
        }
        C::SelectInvertTrackView => {
            let all: Vec<String> = tree(state).into_iter().map(|i| i.id).collect();
            let h = state.track_view.highlighted.clone();
            state.track_view.highlighted = if h.is_empty() { all } else { all.into_iter().filter(|i| !h.contains(i)).collect() };
            Ok(tv_json(state))
        }
        C::SelectChildrenTrackView => {
            let h = state.track_view.highlighted.clone();
            let mut out = h.clone();
            for item in &h {
                if let Some(id) = object_of(item) {
                    for (tid, _) in node_track_ids(state, id) {
                        if !out.contains(&tid) {
                            out.push(tid);
                        }
                    }
                    for c in state.scene.descendants(id) {
                        let cid = format!("{}", c.0);
                        if !out.contains(&cid) {
                            out.push(cid);
                        }
                    }
                }
            }
            state.track_view.highlighted = out;
            Ok(tv_json(state))
        }
        C::RandomizeKeys { time, value, seed } => {
            let mut s = seed.max(1);
            let mut rnd = move || {
                s ^= s << 13;
                s ^= s >> 17;
                s ^= s << 5;
                (s as f32 / u32::MAX as f32) * 2.0 - 1.0
            };
            let snap_f = state.track_view.snap_frames;
            edit_selected(state, "Randomize Keys", |tr, idx| {
                let mut out = Vec::new();
                for i in idx {
                    let k = &mut tr.keys[*i];
                    let mut t = k.time + rnd() * time;
                    if snap_f {
                        t = t.round();
                    }
                    k.time = t;
                    k.value += rnd() * value;
                    out.push(t);
                }
                tr.sort();
                out
            })
        }
        C::CreateOutOfRangeKeys { before, after, samples } => {
            let samples = samples.max(1) as f32;
            edit_highlighted(state, "Create Out of Range Keys", |k| {
                for tr in &mut k.tracks {
                    if tr.keys.len() < 2 {
                        continue;
                    }
                    let (a, b) = (tr.keys[0].time, tr.keys.last().unwrap().time);
                    let old = tr.clone();
                    let step = (b - a) / samples;
                    let mut add = Vec::new();
                    let mut t = a - before as f32;
                    while t < a - SAME {
                        add.push(t);
                        t += step.max(1.0);
                    }
                    let mut t = b + step.max(1.0);
                    while t <= b + after as f32 + SAME {
                        add.push(t);
                        t += step.max(1.0);
                    }
                    for t in add {
                        let v = old.eval(t).unwrap_or(0.0);
                        tr.set_key(t, v, (TangentType::Auto, TangentType::Auto));
                    }
                    tr.before = Ort::Constant;
                    tr.after = Ort::Constant;
                }
            })
        }
        C::SelectKeysByTime { start, end, clear } => {
            if clear {
                state.track_view.selected_keys.clear();
            }
            execute(state, C::TrackViewSelectKeysInRange { start, end, low: None, high: None, op: SelectOp::Add })
        }
        C::EulerFilter { start, end } => {
            let (s, e) = (start.map(|v| v as f32).unwrap_or(f32::MIN), end.map(|v| v as f32).unwrap_or(f32::MAX));
            let paths: Vec<TrackPath> = highlighted_tracks(state).into_iter().filter(|t| t.owner == TrackOwner::Transform && t.pointer == "/rotation").collect();
            if paths.is_empty() {
                return Err("highlight Euler rotation tracks".into());
            }
            let mut nodes: Vec<NodeId> = paths.iter().map(|t| t.node).collect();
            nodes.sort();
            nodes.dedup();
            edit_nodes(state, "Euler Filter", &nodes, |t| paths.contains(t), |_, p| {
                let Some(k) = p.ensure_keys() else { return };
                for tr in &mut k.tracks {
                    // Unwrap ±360° flips between neighbouring keys.
                    for i in 1..tr.keys.len() {
                        if tr.keys[i].time < s || tr.keys[i].time > e {
                            continue;
                        }
                        let prev = tr.keys[i - 1].value;
                        let mut v = tr.keys[i].value;
                        while v - prev > 180.0 {
                            v -= 360.0;
                        }
                        while v - prev < -180.0 {
                            v += 360.0;
                        }
                        tr.keys[i].value = v;
                    }
                }
            })
        }
        C::CollapseController { start, end, samples } => {
            let (s, e) = (start.unwrap_or(state.frame_range.0) as f32, end.unwrap_or(state.frame_range.1) as f32);
            let step = samples.unwrap_or(1).max(1) as f32;
            let paths = highlighted_tracks(state);
            if paths.is_empty() {
                return Err("highlight the tracks to collapse".into());
            }
            let mut nodes: Vec<NodeId> = paths.iter().map(|t| t.node).collect();
            nodes.sort();
            nodes.dedup();
            let tangents = state.default_tangent;
            edit_nodes(state, "Collapse Controller", &nodes, |t| paths.contains(t), |_, p| {
                let mut frames = Vec::new();
                let mut t = s;
                while t <= e + SAME {
                    frames.push((t, p.comps_at(t)));
                    t += step;
                }
                let ty = p.ty();
                let mut k = KeyedTracks::from_rest(ty, frames.first().map(|f| f.1.clone()).unwrap_or_default());
                for (t, c) in frames {
                    for (tr, v) in k.tracks.iter_mut().zip(c) {
                        tr.set_key(t, v, tangents);
                    }
                }
                let _ = p.set_controller_json(&json!({ "Keys": k }));
            })
        }
        _ => Err("not a Track View command".into()),
    }
}

fn focus(state: &mut AppState, f: &str) -> Result<Value, String> {
    state.track_view.focus_field = Some(f.to_owned());
    Ok(json!({ "focus": f }))
}

fn expand(state: &mut AppState, cmd: &Command) -> Result<Value, String> {
    use Command as C;
    let h = state.track_view.highlighted.clone();
    let targets: Vec<String> = if h.is_empty() { state.scene.nodes().map(|n| format!("{}", n.id.0)).collect() } else { h };
    let object = |i: &str| i.split(':').next().unwrap_or(i).to_owned();
    let c = &mut state.track_view.collapsed;
    match cmd {
        C::ExpandObjectToggle | C::ExpandTrackToggle => {
            for i in &targets {
                if !c.remove(i) {
                    c.insert(i.clone());
                }
            }
        }
        // Everything below the item opens.
        C::ExpandAll | C::ExpandTracks => {
            for i in &targets {
                let o = object(i);
                c.retain(|x| !(x == i || x.starts_with(&format!("{i}:")) || (*i == o && object(x) == o)));
            }
        }
        C::ExpandObjects => {
            for i in &targets {
                c.remove(&object(i));
            }
        }
        C::CollapseAllTrackView | C::CollapseTracks => {
            for i in &targets {
                let o = object(i);
                for g in ["Transform", "Object", "Modified Object"] {
                    c.insert(format!("{o}:{g}"));
                }
                if matches!(cmd, C::CollapseAllTrackView) {
                    c.insert(o);
                }
            }
        }
        C::CollapseObjects => {
            for i in &targets {
                c.insert(object(i));
            }
        }
        _ => {}
    }
    Ok(json!({ "collapsed": state.track_view.collapsed.len() }))
}

fn paste_time(state: &mut AppState, at: f32, relative: bool) -> Result<Value, String> {
    let clip = state.track_view.time_clip.clone().ok_or("the time clipboard is empty")?;
    let paths = highlighted_tracks(state);
    let mut nodes: Vec<NodeId> = paths.iter().map(|t| t.node).collect();
    nodes.sort();
    nodes.dedup();
    // A single copied track pastes into any highlighted track of its type.
    let single = clip.tracks.len() == 1;
    let len = clip.length;
    edit_nodes(state, "Paste Time", &nodes, |t| paths.contains(t), |t, p| {
        let base = p.comps_at(at);
        let Some(k) = p.ensure_keys() else { return };
        let dims = k.tracks.len();
        for (c, tr) in k.tracks.iter_mut().enumerate() {
            // A single copied track pastes into another track of its type: component to the same component,
            // or into a one-component track.
            let src = clip.tracks.iter().find(|(tk, cc, _, _)| *tk == t.key() && *cc as usize == c).or_else(|| {
                let s0 = &clip.tracks[0];
                (single && s0.0 != t.key() && (dims == 1 || s0.1 as usize == c)).then_some(s0)
            });
            let Some((_, _, keys, start_value)) = src else { continue };
            for key in &mut tr.keys {
                if key.time >= at - SAME {
                    key.time += len;
                }
            }
            let offset = if relative { base[c] - start_value } else { 0.0 };
            for key in keys {
                let mut nk = key.clone();
                nk.time += at;
                nk.value += offset;
                tr.keys.retain(|x| (x.time - nk.time).abs() > SAME);
                tr.keys.push(nk);
            }
            tr.sort();
        }
    })
}

/// The Loom-group commands this module runs (the TrackView group runs here too).
pub fn handles(c: &Command) -> bool {
    use Command as C;
    matches!(
        c,
        C::TrackViewFloater
            | C::OpenCurveEditor
            | C::OpenDopeSheet
            | C::CloseTrackView
            | C::TrackViewHighlight { .. }
            | C::TrackViewSelectKeys { .. }
            | C::TrackViewSelectKeysInRange { .. }
            | C::TrackViewInsertKey { .. }
            | C::TrackViewMoveKeys { .. }
            | C::TrackViewSlideKeys { .. }
            | C::TrackViewScaleKeys { .. }
            | C::TrackViewScaleValues { .. }
            | C::TrackViewDeleteKeys
            | C::TrackViewSetTangents { .. }
            | C::TrackViewSetKeyValues { .. }
            | C::TrackViewSetKeyInfo { .. }
            | C::TrackViewSetOrt { .. }
            | C::TrackViewNormalizeTime
            | C::TrackViewSetConstantVelocity { .. }
            | C::TrackViewReduceKeys { .. }
            | C::TrackViewDrawCurve { .. }
            | C::TrackViewSelectTime { .. }
            | C::TrackViewInsertTime { .. }
            | C::TrackViewScaleTime { .. }
            | C::TrackViewDeleteTime
            | C::TrackViewPasteTime { .. }
            | C::TrackViewMoveRange { .. }
            | C::TrackViewScaleRange { .. }
            | C::AddNoteKey { .. }
            | C::SetNoteKey { .. }
            | C::DeleteNoteKey { .. }
            | C::TrackSetCreate { .. }
            | C::TrackSetDelete { .. }
            | C::TrackSetSelect { .. }
            | C::TrackSetAddHighlighted { .. }
            | C::TrackSetRemoveHighlighted { .. }
            | C::TrackViewFilter { .. }
            | C::TrackViewOption { .. }
            | C::TrackViewSoftSelection { .. }
            | C::TrackViewSetView { .. }
            | C::RandomizeKeys { .. }
            | C::CreateOutOfRangeKeys { .. }
            | C::SelectKeysByTime { .. }
            | C::EulerFilter { .. }
            | C::CollapseController { .. }
    )
}

/// The first selected key (Key Info / Key Stats): its track, component,
/// index on that component, the key and the track's interpolation.
pub fn first_selected_key(state: &AppState) -> Option<(TrackPath, u32, usize, Key, loom_scene::anim::Interp)> {
    let k = state.track_view.selected_keys.first()?;
    let t = path_of(state, &k.track)?;
    let p = tracks::read(&state.scene, &t)?;
    let tr = p.keys()?.tracks.get(k.comp as usize)?;
    let i = tr.keys.iter().position(|x| (x.time - k.time).abs() < SAME)?;
    Some((t, k.comp, i, tr.keys[i].clone(), tr.interp))
}
