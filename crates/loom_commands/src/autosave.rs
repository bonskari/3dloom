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

/// Captures a due `autoback/<name>NN.loom` snapshot when the interval has
/// elapsed, the scene changed and no drag is in progress. The caller writes
/// the owned job synchronously or on its single automatic-backup worker.
pub fn prepare_due(state: &mut AppState, now: Instant) -> Option<BackupJob> {
    let Some(last) = state.autosave.last else {
        state.autosave.last = Some(now);
        return None;
    };
    if !state.prefs.auto_backup_enable {
        return None;
    }
    let interval = Duration::from_secs_f32(state.prefs.auto_backup_interval_min.max(0.1) * 60.0);
    if now.duration_since(last) < interval {
        return None;
    }
    if state.undo.in_transaction() {
        return None;
    }
    if state.autosave.last_version == Some(state.undo.version()) {
        state.autosave.last = Some(now);
        return None;
    }
    // Retry only after another interval, even when the write failed.
    state.autosave.last = Some(now);
    Some(prepare_now(state))
}

/// Synchronous backup tick for non-UI callers.
pub fn tick(state: &mut AppState, now: Instant) -> Result<Option<PathBuf>, String> {
    prepare_due(state, now).map(|job| job.write().map(|done| done.apply(state))).transpose()
}

/// An owned scene snapshot and its captured backup acknowledgement.
pub struct BackupJob {
    pub path: PathBuf,
    document: crate::file::Document,
    scene_instance: u64,
    version: u64,
    next_index: u32,
}

pub struct BackupCompletion {
    pub path: PathBuf,
    scene_instance: u64,
    version: u64,
    next_index: u32,
}

pub fn prepare_now(state: &AppState) -> BackupJob {
    let count = state.prefs.auto_backup_count.max(1);
    let index = state.autosave.next_index.clamp(1, count);
    BackupJob {
        path: autoback_dir().join(format!("{}{index:02}.loom", state.prefs.auto_backup_name)),
        document: crate::file::Document {
            format: crate::file::FORMAT,
            app: format!("3dloom {}", env!("CARGO_PKG_VERSION")),
            scene: state.scene.clone(),
            settings: crate::file::SceneSettings::from_state(state),
        },
        scene_instance: state.scene.instance_id(),
        version: state.undo.version(),
        next_index: if index >= count { 1 } else { index + 1 },
    }
}

impl BackupJob {
    pub fn write(self) -> Result<BackupCompletion, String> {
        let dir = self.path.parent().expect("backup directory");
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        crate::file::write(&self.path, &self.document)?;
        Ok(BackupCompletion { path: self.path, scene_instance: self.scene_instance,
            version: self.version, next_index: self.next_index })
    }
}

impl BackupCompletion {
    pub fn apply(self, state: &mut AppState) -> PathBuf {
        state.autosave.next_index = self.next_index;
        if state.scene.instance_id() == self.scene_instance {
            state.autosave.last_version = Some(self.version);
        }
        self.path
    }
}

/// Writes and acknowledges the next backup synchronously (Auto Backup Now).
pub fn backup_now(state: &mut AppState) -> Result<PathBuf, String> {
    Ok(prepare_now(state).write()?.apply(state))
}

/// Target of commands which can write a scene, including a Save Changes reply.
/// Used to serialize only actual path conflicts with the automatic writer.
pub fn command_write_target(state: &AppState, command: &crate::Command) -> Option<PathBuf> {
    match command {
        crate::Command::SaveFile { path } => path.as_ref().map(PathBuf::from).or_else(|| state.file.clone()),
        crate::Command::SaveFileAs { path } | crate::Command::SaveFileAsCopy { path } | crate::Command::SaveSelected { path } | crate::Command::ArchiveFile { path } | crate::Command::ExportFile { path } | crate::Command::ExportSelected { path } => path.as_ref().map(PathBuf::from),
        crate::Command::SaveIncremental => state.file.as_deref().map(crate::execute::incremental_path),
        crate::Command::SaveChangesReply { reply: crate::command::SaveChangesReply::Yes }
            if matches!(state.dialog, Some(crate::Dialog::SaveChanges(_))) => state.file.clone(),
        crate::Command::Hold => Some(hold_path()),
        _ => None,
    }
}
