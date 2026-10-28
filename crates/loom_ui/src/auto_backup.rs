//! One owned automatic-backup snapshot at a time. Manual backup joins it first.
use loom_commands::{autosave::{BackupCompletion, BackupJob}, AppState, Command};
use std::{path::{Component, Path, PathBuf}, thread::JoinHandle};

#[derive(Default)]
pub(crate) struct AutoBackup {
    pending: Option<(PathBuf, JoinHandle<Result<BackupCompletion, String>>)>,
    pub close_requested: bool,
    close_token: Option<(u64,u64)>,
}

impl AutoBackup {
    pub fn defer_close(&mut self, state: &AppState) {
        if !self.close_requested {
            self.close_token = Some((state.scene.instance_id(), state.undo.version()));
        }
        self.close_requested = true;
    }
    pub fn close_needs_prompt(&self, state: &AppState) -> bool {
        self.close_requested && state.dirty() && (!state.exit_requested
            || self.close_token != Some((state.scene.instance_id(), state.undo.version())))
    }
    pub fn pending(&self) -> bool { self.pending.is_some() }
    pub fn start(&mut self, job: BackupJob) {
        assert!(self.pending.is_none(), "one automatic backup at a time");
        self.pending = Some((job.path.clone(), std::thread::spawn(move || job.write())));
    }
    pub fn poll(&mut self, state: &mut AppState) -> Option<Result<PathBuf, String>> {
        if !self.pending.as_ref().is_some_and(|(_, worker)| worker.is_finished()) { return None; }
        Some(self.finish(state))
    }
    pub fn finish(&mut self, state: &mut AppState) -> Result<PathBuf, String> {
        let (_, worker) = self.pending.take().expect("pending automatic backup");
        worker.join().map_err(|_| "Automatic backup worker panicked".to_owned())?
            .map(|done| done.apply(state))
    }
    pub fn command_needs_drain(&self, state: &AppState, cmd: &Command) -> bool {
        let Some((active, _)) = &self.pending else { return false; };
        if matches!(cmd, Command::AutoBackupNow) { return true; }
        loom_commands::autosave::command_write_target(state, cmd)
            .is_some_and(|path| write_paths_overlap(active, &path))
    }
}

impl Drop for AutoBackup {
    fn drop(&mut self) {
        if let Some((_, worker)) = self.pending.take() {
            match worker.join() {
                Ok(Ok(_)) => {},
                Ok(Err(error)) => log::error!("Automatic backup during shutdown: {error}"),
                Err(_) => log::error!("Automatic backup worker panicked during shutdown"),
            }
        }
    }
}

fn normalized(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() { path.to_owned() } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    // Resolve the nearest existing directory, even on the first backup before
    // the worker has created autoback. Preserve the final name: rename replaces
    // a file symlink rather than writing through it.
    let mut parent = absolute.parent().unwrap_or(Path::new("/"));
    let mut tail = vec![absolute.file_name().unwrap_or_default().to_owned()];
    let base = loop {
        if let Ok(existing) = parent.canonicalize() { break existing; }
        let Some(name) = parent.file_name() else { break parent.to_owned(); };
        tail.push(name.to_owned());
        let Some(next) = parent.parent() else { break parent.to_owned(); };
        parent = next;
    };
    let mut clean = base;
    for part in tail.iter().rev().flat_map(|part| Path::new(part).components()) {
        match part { Component::CurDir => {}, Component::ParentDir => { clean.pop(); }, _ => clean.push(part.as_os_str()) }
    }
    clean
}

fn write_paths_overlap(a: &Path, b: &Path) -> bool {
    let a = [normalized(a), normalized(&a.with_extension("loom.tmp"))];
    let b = [normalized(b), normalized(&b.with_extension("loom.tmp"))];
    a.iter().any(|a| b.contains(a))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{mpsc, Mutex};
    static ENV: Mutex<()> = Mutex::new(());
    fn setup(name: &str) -> (AppState, std::sync::MutexGuard<'static, ()>, PathBuf) {
        let lock=ENV.lock().unwrap_or_else(|e|e.into_inner());
        let home=std::env::temp_dir().join(format!("loom-async-backup-{}-{name}",std::process::id()));
        let _=std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        std::env::set_var("LOOM_HOME",&home);
        (AppState::new(),lock,home)
    }
    fn delayed(state: &AppState) -> (AutoBackup,mpsc::Sender<()>,PathBuf) {
        let job=loom_commands::autosave::prepare_now(state);
        let path=job.path.clone();let (release,wait)=mpsc::channel();
        let worker=std::thread::spawn(move||{wait.recv().unwrap();job.write()});
        (AutoBackup{pending:Some((path.clone(),worker)),close_requested:false,close_token:None},release,path)
    }
    #[test]
    fn pending_snapshot_is_bounded_and_manual_backup_is_serialized() {
        let (mut state,_lock,home)=setup("ordering");
        let (mut backup,release,first)=delayed(&state);
        assert!(backup.poll(&mut state).is_none());
        assert!(backup.pending());assert!(!first.exists());
        assert!(backup.command_needs_drain(&state,&Command::AutoBackupNow));
        assert!(!backup.command_needs_drain(&state,&Command::Hold));
        assert!(!backup.command_needs_drain(&state,&Command::SaveFile{path:Some(home.join("other.loom").display().to_string())}));
        assert!(backup.command_needs_drain(&state,&Command::SaveFile{path:Some(first.with_extension("json").display().to_string())}));
        release.send(()).unwrap();backup.finish(&mut state).unwrap();
        let second=loom_commands::autosave::backup_now(&mut state).unwrap();
        assert_ne!(first,second);assert!(!backup.pending());
        assert_eq!(loom_commands::file::read(&first).unwrap().scene.len(),0);
    }
    #[test]
    fn replacement_scene_is_not_acknowledged_and_failure_preserves_rotation() {
        let (mut state,_lock,home)=setup("replacement");
        let (mut backup,release,_)=delayed(&state);
        let old=state.scene.instance_id();
        loom_commands::execute(&mut state,Command::ResetFile).unwrap();
        assert_ne!(old,state.scene.instance_id());
        release.send(()).unwrap();backup.finish(&mut state).unwrap();
        assert_eq!(state.autosave.last_version,None);assert_eq!(state.autosave.next_index,2);
        let blocked=home.join("blocked");std::fs::write(&blocked,b"file").unwrap();
        let mut job=loom_commands::autosave::prepare_now(&state);job.path=blocked.join("backup.loom");
        backup.start(job);assert!(backup.finish(&mut state).is_err());
        assert_eq!(state.autosave.last_version,None);assert_eq!(state.autosave.next_index,2);
    }
    #[test]
    fn drop_drains_the_started_real_write() {
        let (state,_lock,_)=setup("shutdown");
        let (backup,release,path)=delayed(&state);
        let dropper=std::thread::spawn(move||drop(backup));
        assert!(!dropper.is_finished());release.send(()).unwrap();dropper.join().unwrap();
        assert_eq!(loom_commands::file::read(&path).unwrap().scene.len(),0);
    }
    #[test]
    fn temporary_path_collision_is_detected() {
        assert!(write_paths_overlap(Path::new("/a/scene.loom"),Path::new("/a/scene.json")));
        assert!(write_paths_overlap(Path::new("/a/./scene.loom"),Path::new("/a/scene.loom")));
        assert!(!write_paths_overlap(Path::new("/a/scene01.loom"),Path::new("/a/scene02.loom")));
    }
    #[test]
    fn json_and_typed_manual_backups_join_before_writing_next_slot() {
        let (_state,_lock,home)=setup("dispatch");
        let mut app=crate::LoomApp::headless();
        let (backup,release,first)=delayed(&app.state);
        app.auto_backup=backup;
        loom_commands::execute(&mut app.state,Command::CreatePrimitive {
            kind:loom_commands::PrimitiveKind::Box,position:[0.;3],size:[1.;3]
        }).unwrap();
        release.send(()).unwrap();
        let result=app.dispatch_json(&serde_json::json!({"cmd":"Auto Backup Now","args":{}}));
        assert_eq!(result["ok"],true);
        assert_eq!(loom_commands::file::read(&first).unwrap().scene.len(),0);
        assert_eq!(loom_commands::file::read(&home.join("autoback/AutoBackup02.loom")).unwrap().scene.len(),1);
        assert!(!app.auto_backup.pending());
        let (backup,release,third)=delayed(&app.state);app.auto_backup=backup;
        release.send(()).unwrap();
        app.dispatch(Command::AutoBackupNow).unwrap();
        assert!(third.exists());assert!(!app.auto_backup.pending());
    }
    #[test]
    fn incremental_and_save_changes_paths_are_guarded() {
        let (mut state,_lock,home)=setup("nested-save");
        let active=home.join("scene02.loom");
        let job=loom_commands::autosave::prepare_now(&state);
        let worker=std::thread::spawn(move||job.write());
        let backup=AutoBackup{pending:Some((active.clone(),worker)),close_requested:false,close_token:None};
        state.file=Some(home.join("scene01.loom"));
        std::fs::write(state.file.as_ref().unwrap(),b"existing").unwrap();
        assert!(backup.command_needs_drain(&state,&Command::SaveIncremental));
        state.file=Some(active);
        state.dialog=Some(loom_commands::Dialog::SaveChanges(loom_commands::PendingFileAction::Exit));
        assert!(backup.command_needs_drain(&state,&Command::SaveChangesReply{reply:loom_commands::command::SaveChangesReply::Yes}));
        let cmd=loom_commands::execute::parse_json_command(&serde_json::json!({"cmd":"Save Incremental (+)","args":{}})).unwrap();
        assert!(matches!(cmd,Command::SaveIncremental));
    }

    #[test]
    fn edits_after_raw_close_or_approved_exit_require_a_new_prompt() {
        let (mut state,_lock,_)=setup("close-edits");
        for approved in [false,true] {
            state.undo.mark_saved();state.exit_requested=approved;
            let (mut backup,release,_)=delayed(&state);
            backup.defer_close(&state);
            assert!(!backup.close_needs_prompt(&state));
            loom_commands::execute(&mut state,Command::CreatePrimitive {
                kind:loom_commands::PrimitiveKind::Box,position:[0.;3],size:[1.;3]
            }).unwrap();
            release.send(()).unwrap();backup.finish(&mut state).unwrap();
            assert!(backup.close_needs_prompt(&state), "new edit after approved={approved}");
        }
    }
    #[test]
    fn save_selected_archive_and_export_conflicts_are_guarded() {
        let (state,_lock,_)=setup("other-writers");
        let (mut backup,release,path)=delayed(&state);
        let path=Some(path.display().to_string());
        for cmd in [Command::SaveSelected{path:path.clone()},Command::ArchiveFile{path:path.clone()},
            Command::ExportFile{path:path.clone()},Command::ExportSelected{path:path.clone()}] {
            assert!(backup.command_needs_drain(&state,&cmd));
        }
        release.send(()).unwrap();backup.finish(&mut {state}).unwrap();
    }

    #[test]
    #[ignore = "actual frozen Ray snapshot: explicit isolated verification"]
    fn ray_snapshot_worker_reopens_full_document() {
        let (mut state,_lock,_)=setup("ray-document");
        let input=std::path::Path::new("/tmp/loom-conditioning-capture/input.loom");
        let document=loom_commands::file::read(input).unwrap();
        state.scene=document.scene;document.settings.apply_to(&mut state);
        assert_eq!(state.scene.len(),461);
        for name in ["Ray_OfficeDesk","Ray_OfficeChair"] {
            assert!(state.scene.nodes().any(|n|n.name==name));
        }
        let before=serde_json::to_vec(&state.scene).unwrap();
        let mut backup=AutoBackup::default();
        backup.start(loom_commands::autosave::prepare_now(&state));
        let path=backup.finish(&mut state).unwrap();
        let reopened=loom_commands::file::read(&path).unwrap();
        assert_eq!(before,serde_json::to_vec(&reopened.scene).unwrap());
        assert_eq!(serde_json::to_value(loom_commands::file::SceneSettings::from_state(&state)).unwrap(),
            serde_json::to_value(reopened.settings).unwrap());
    }

    #[test]
    fn unrelated_save_and_hold_finish_while_automatic_writer_is_delayed() {
        let (_state,_lock,home)=setup("independent-save");
        let mut app=crate::LoomApp::headless();
        let (backup,release,_)=delayed(&app.state);app.auto_backup=backup;
        app.dispatch(Command::Hold).unwrap();
        app.dispatch(Command::SaveFile{path:Some(home.join("manual.loom").display().to_string())}).unwrap();
        assert!(home.join("manual.loom").exists());
        assert!(home.join("autoback/loomhold.loom").exists());
        assert!(app.auto_backup.pending());
        release.send(()).unwrap();app.finish_automatic_backup().unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn directory_symlink_alias_is_guarded_before_first_backup_creates_its_folder() {
        let (_state,_lock,home)=setup("symlink");
        let actual=home.join("actual");std::fs::create_dir(&actual).unwrap();
        let alias=home.join("alias");std::os::unix::fs::symlink(&actual,&alias).unwrap();
        assert!(write_paths_overlap(&actual.join("new/autoback01.loom"),&alias.join("new/autoback01.json")));
    }

}
