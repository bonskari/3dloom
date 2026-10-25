//! AppState timeline changes attached to the actual scene undo entry.
//! Entry identities survive redo and cannot be reused by branches/eviction.
use crate::AppState;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Timing {
    fps: f32,
    range: (i32, i32),
    frame: i32,
}
#[derive(Debug, Default)]
pub(crate) struct TimingHistory {
    entries: Vec<(u64, Timing, Timing)>,
}
pub(crate) fn snap(state: &AppState) -> Timing {
    Timing { fps: state.fps, range: state.frame_range, frame: state.current_frame }
}
pub(crate) fn record(state: &mut AppState, before: Timing) {
    let after = snap(state);
    let Some(id) = state.undo.undo_entry_id() else { return };
    state.timing_history.entries.retain(|(entry, _, _)| state.undo.contains_entry(*entry));
    state.timing_history.entries.push((id, before, after));
}
pub(crate) fn restore(state: &mut AppState, id: Option<u64>, redo: bool) {
    let Some((_, before, after)) = state.timing_history.entries.iter().find(|(entry, _, _)| Some(*entry) == id) else { return };
    let t = if redo { *after } else { *before };
    state.fps = t.fps;
    state.frame_range = t.range;
    state.current_frame = t.frame;
    state.scene.set_time(t.frame as f32);
}
