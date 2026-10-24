//! Auto Backup (Preferences > Files) and the Hold file, under the data directory.

use crate::state::AppState;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// `$LOOM_HOME`, else `$XDG_DATA_HOME/3dloom`, else `~/.local/share/3dloom`.
pub fn loom_home() -> PathBuf {
    if let Some(h) = std::env::var_os("LOOM_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(h);
    }
    if let Some(x) = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(x).join("3dloom");
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    home.join(".local").join("share").join("3dloom")
}

/// The `autoback` folder: Auto Backup files and the Hold file.
pub fn autoback_dir() -> PathBuf {
    loom_home().join("autoback")
}

/// The Hold file.
pub fn hold_path() -> PathBuf {
    autoback_dir().join("loomhold.loom")
}

/// The Auto Backup clock.
#[derive(Debug, Clone, Default)]
pub struct AutosaveState {
    /// When the interval started (the first tick, or the last backup).
    pub last: Option<Instant>,
    /// 1-based index of the next file to write (cycles through the count).
    pub next_index: u32,
    /// `undo.version()` at the last backup, so an unchanged scene is not rewritten.
    pub last_version: Option<u64>,
}

/// Called every second or so with the current time. Writes
/// `autoback/<name>NN.loom` when Auto Backup is enabled, the interval has
/// elapsed, the scene changed since the last backup and no drag is in
/// progress. Returns the path written.
pub fn tick(state: &mut AppState, now: Instant) -> Result<Option<PathBuf>, String> {
    let Some(last) = state.autosave.last else {
        state.autosave.last = Some(now);
        return Ok(None);
    };
    if !state.prefs.auto_backup_enable {
        return Ok(None);
    }
    let interval = Duration::from_secs_f32(state.prefs.auto_backup_interval_min.max(0.1) * 60.0);
    if now.duration_since(last) < interval {
        return Ok(None);
    }
    if state.undo.in_transaction() {
        return Ok(None);
    }
    if state.autosave.last_version == Some(state.undo.version()) {
        state.autosave.last = Some(now);
        return Ok(None);
    }
    // Retry only after another interval, even when the write failed.
    state.autosave.last = Some(now);
    let path = backup_now(state)?;
    Ok(Some(path))
}

/// Writes the next backup file now (Auto Backup Now).
pub fn backup_now(state: &mut AppState) -> Result<PathBuf, String> {
    let count = state.prefs.auto_backup_count.max(1);
    let index = state.autosave.next_index.clamp(1, count);
    let dir = autoback_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(format!("{}{index:02}.loom", state.prefs.auto_backup_name));
    let doc = crate::file::Document {
        format: crate::file::FORMAT,
        app: format!("3dloom {}", env!("CARGO_PKG_VERSION")),
        scene: state.scene.clone(),
        settings: crate::file::SceneSettings::from_state(state),
    };
    crate::file::write(&path, &doc)?;
    state.autosave.next_index = if index >= count { 1 } else { index + 1 };
    state.autosave.last_version = Some(state.undo.version());
    Ok(path)
}
