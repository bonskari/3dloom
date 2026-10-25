//! Selection undo, as in the reference: a selection change is an undo step
//! ("Select"), and undoing a scene operation gives back the selection it
//! started from.
//!
//! The selection lives in `AppState`, the undo stack in the scene crate, so
//! the selection history sits beside the stack: one entry per undo
//! position, holding the selection before and after that step. A
//! selection-only step is an empty marker entry on the stack, so Ctrl+Z
//! walks selection and scene steps in the order they happened. Steps that
//! land without a command (an interactive drag's transaction) have no
//! entry; undoing them keeps the sub-object ids that are still valid.

use crate::state::{AppState, SelectionMode, SubObjectId};
use loom_scene::NodeId;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelSnap {
    nodes: BTreeSet<NodeId>,
    sub: Vec<(NodeId, SubObjectId)>,
    mode: SelectionMode,
}

#[derive(Debug, Clone)]
struct Entry {
    /// `UndoStack::position()` with this step on top.
    pos: u64,
    before: SelSnap,
    after: SelSnap,
}

#[derive(Debug, Default)]
pub struct SelHistory {
    entries: Vec<Entry>,
    /// Nested `execute` calls: only the outermost records.
    depth: u32,
}

pub fn snap(state: &AppState) -> SelSnap {
    SelSnap { nodes: state.selection.clone(), sub: state.sub_selection.iter().collect(), mode: state.selection_mode }
}

/// Called around every command: true when this is the outermost call.
pub fn enter(state: &mut AppState) -> bool {
    state.sel_history.depth += 1;
    state.sel_history.depth == 1
}

pub fn leave(state: &mut AppState) {
    state.sel_history.depth -= 1;
}

/// After an outermost command (not Undo / Redo): records the selection
/// change, as its own step or attached to the scene step the command made.
pub fn record(state: &mut AppState, before: SelSnap, pos_before: u64) {
    if state.undo.is_empty() && !state.undo.can_redo() {
        // New, Open, Reset, Fetch: the history starts over.
        state.sel_history.entries.clear();
    }
    if state.undo.in_transaction() {
        return;
    }
    let after = snap(state);
    let mut pos = state.undo.position();
    if pos <= pos_before {
        // No scene step: a selection step when the selection changed (a
        // level switch alone is not one).
        if before == after || before.mode != after.mode || pos < pos_before {
            return;
        }
        state.undo.push_marker("Select");
        pos = state.undo.position();
    }
    let h = &mut state.sel_history.entries;
    h.retain(|e| e.pos < pos);
    let floor = pos.saturating_sub(state.undo.len() as u64);
    h.retain(|e| e.pos > floor);
    h.push(Entry { pos, before, after });
}

/// After an undo that took the step at `pos` off: its selection before.
/// Returns false when no entry was recorded for that step.
pub fn undone(state: &mut AppState, pos: u64) -> bool {
    let Some(e) = state.sel_history.entries.iter().find(|e| e.pos == pos).cloned() else { return false };
    restore(state, &e.before);
    true
}

/// After a redo that put the step at `pos` back: its selection after.
pub fn redone(state: &mut AppState, pos: u64) -> bool {
    let Some(e) = state.sel_history.entries.iter().find(|e| e.pos == pos).cloned() else { return false };
    restore(state, &e.after);
    true
}

fn restore(state: &mut AppState, s: &SelSnap) {
    state.selection = s.nodes.iter().copied().filter(|id| state.scene.node(*id).is_some()).collect();
    state.sub_selection.clear();
    if s.mode == state.selection_mode {
        for (n, id) in &s.sub {
            if state.selection.contains(n) && valid(state, *n, *id) {
                state.sub_selection.insert(*n, *id);
            }
        }
    }
}

/// A step with no recorded selection (an interactive drag): keep the
/// sub-objects whose ids still name an element of the node's mesh.
pub fn keep_valid(state: &mut AppState) {
    let keep: Vec<(NodeId, SubObjectId)> = state.sub_selection.iter().filter(|(n, id)| state.selection.contains(n) && valid(state, *n, *id)).collect();
    state.sub_selection.clear();
    for (n, id) in keep {
        state.sub_selection.insert(n, id);
    }
}

fn valid(state: &AppState, node: NodeId, id: SubObjectId) -> bool {
    let Some(n) = state.scene.node(node) else { return false };
    let mesh = n.cage_mesh();
    match id {
        SubObjectId::Vertex(v) => mesh.vertex_position(v).is_some(),
        SubObjectId::Edge(e) => mesh.edge_vertices(e).is_some(),
        SubObjectId::Face(f) => mesh.face_positions(f).is_some(),
    }
}
