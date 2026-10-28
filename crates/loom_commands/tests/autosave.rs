//! Auto Backup (Preferences > Files) and on-disk Hold / Fetch.

use loom_commands::autosave::{autoback_dir, hold_path, loom_home, tick};
use loom_commands::{execute, AppState, Command, PrimitiveKind};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Tests set `LOOM_HOME`; the process environment is shared, so one at a time.
static ENV: Mutex<()> = Mutex::new(());

fn home(name: &str) -> (PathBuf, MutexGuard<'static, ()>) {
    let guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let d = std::env::temp_dir().join("loom-autosave-tests").join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::env::set_var("LOOM_HOME", &d);
    (d, guard)
}

fn boxed(s: &mut AppState, x: f32) {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [x, 0.0, 0.0], size: [10.0; 3] }).unwrap();
}

fn names(dir: &PathBuf) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir).map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
    v.sort();
    v
}

#[test]
fn home_follows_loom_home_then_xdg() {
    let (d, _g) = home("home");
    assert_eq!(loom_home(), d);
    assert_eq!(autoback_dir(), d.join("autoback"));
    assert_eq!(hold_path(), d.join("autoback").join("loomhold.loom"));
    std::env::remove_var("LOOM_HOME");
    std::env::set_var("XDG_DATA_HOME", "/tmp/xdg-test");
    assert_eq!(loom_home(), PathBuf::from("/tmp/xdg-test/3dloom"));
    std::env::remove_var("XDG_DATA_HOME");
    assert!(loom_home().ends_with(".local/share/3dloom"));
}

#[test]
fn tick_writes_after_interval_and_cycles_names() {
    let (d, _g) = home("cycle");
    let mut s = AppState::new();
    boxed(&mut s, 0.0);
    let t0 = Instant::now();
    assert_eq!(tick(&mut s, t0).unwrap(), None, "the first tick only starts the clock");
    let min = |m: u64| t0 + Duration::from_secs(60 * m);
    assert_eq!(tick(&mut s, min(4)).unwrap(), None, "interval (5 min) not yet elapsed");
    let p1 = tick(&mut s, min(5)).unwrap().expect("first backup");
    assert_eq!(p1, d.join("autoback").join("AutoBackup01.loom"));
    assert_eq!(loom_commands::file::read(&p1).unwrap().scene.len(), 1);
    boxed(&mut s, 20.0);
    assert_eq!(tick(&mut s, min(10)).unwrap().unwrap().file_name().unwrap(), "AutoBackup02.loom");
    boxed(&mut s, 40.0);
    assert_eq!(tick(&mut s, min(15)).unwrap().unwrap().file_name().unwrap(), "AutoBackup03.loom");
    boxed(&mut s, 60.0);
    let p4 = tick(&mut s, min(20)).unwrap().unwrap();
    assert_eq!(p4.file_name().unwrap(), "AutoBackup01.loom", "three files cycle");
    assert_eq!(loom_commands::file::read(&p4).unwrap().scene.len(), 4);
    assert_eq!(names(&d.join("autoback")), vec!["AutoBackup01.loom", "AutoBackup02.loom", "AutoBackup03.loom"]);
}

#[test]
fn tick_skips_when_unchanged_or_disabled() {
    let (d, _g) = home("skip");
    let mut s = AppState::new();
    boxed(&mut s, 0.0);
    let t0 = Instant::now();
    tick(&mut s, t0).unwrap();
    let min = |m: u64| t0 + Duration::from_secs(60 * m);
    assert!(tick(&mut s, min(5)).unwrap().is_some());
    assert_eq!(tick(&mut s, min(10)).unwrap(), None, "nothing changed since the last backup");
    boxed(&mut s, 20.0);
    execute(&mut s, Command::SetPreference { name: "Auto Backup Enable".into(), value: 0.0 }).unwrap();
    assert_eq!(tick(&mut s, min(15)).unwrap(), None, "disabled");
    execute(&mut s, Command::SetPreference { name: "Auto Backup Enable".into(), value: 1.0 }).unwrap();
    execute(&mut s, Command::SetPreference { name: "Auto Backup Interval".into(), value: 1.0 }).unwrap();
    execute(&mut s, Command::SetPreference { name: "Auto Backup File Name".into(), value: 0.0 }).unwrap_err();
    assert!(tick(&mut s, min(16)).unwrap().is_some(), "one-minute interval");
    assert_eq!(names(&d.join("autoback")).len(), 2);
}

#[test]
fn autosave_skips_open_transaction() {
    let (_d, _g) = home("transaction");
    let mut s = AppState::new();
    boxed(&mut s, 0.0);
    let t0 = Instant::now();
    tick(&mut s, t0).unwrap();
    s.undo.begin("drag");
    boxed(&mut s, 20.0);
    assert_eq!(tick(&mut s, t0 + Duration::from_secs(600)).unwrap(), None, "a drag is in progress");
    s.undo.end();
    assert!(tick(&mut s, t0 + Duration::from_secs(601)).unwrap().is_some());
}

#[test]
fn auto_backup_name_and_count_preferences() {
    let (d, _g) = home("name");
    let mut s = AppState::new();
    boxed(&mut s, 0.0);
    execute(&mut s, Command::SetAutoBackupName { name: "Bak".into() }).unwrap();
    execute(&mut s, Command::SetPreference { name: "Auto Backup Count".into(), value: 1.0 }).unwrap();
    assert_eq!(s.prefs.auto_backup_name, "Bak");
    let t0 = Instant::now();
    tick(&mut s, t0).unwrap();
    assert_eq!(tick(&mut s, t0 + Duration::from_secs(300)).unwrap().unwrap().file_name().unwrap(), "Bak01.loom");
    boxed(&mut s, 20.0);
    assert_eq!(tick(&mut s, t0 + Duration::from_secs(600)).unwrap().unwrap().file_name().unwrap(), "Bak01.loom");
    assert_eq!(names(&d.join("autoback")), vec!["Bak01.loom"]);
    // Auto Backup Now writes regardless of the clock.
    boxed(&mut s, 40.0);
    let r = execute(&mut s, Command::AutoBackupNow).unwrap();
    assert!(r["path"].as_str().unwrap().ends_with("Bak01.loom"));
}

#[test]
fn hold_writes_file_and_fetch_reads_it_after_a_fresh_state() {
    let (d, _g) = home("hold");
    let mut s = AppState::new();
    boxed(&mut s, 0.0);
    boxed(&mut s, 20.0);
    execute(&mut s, Command::Hold).unwrap();
    assert!(d.join("autoback").join("loomhold.loom").exists());
    let mut t = AppState::new();
    let r = execute(&mut t, Command::Fetch).unwrap();
    assert_eq!(r["nodes"], 2);
    assert_eq!(t.scene.len(), 2);
    assert!(!t.undo.can_undo(), "Fetch is not undoable");
    assert!(t.dirty(), "a fetched scene is unsaved");
}

#[test]
fn fetch_without_hold_file_errors() {
    let (_d, _g) = home("nohold");
    let mut s = AppState::new();
    boxed(&mut s, 0.0);
    let err = execute(&mut s, Command::Fetch).unwrap_err();
    assert!(err.contains("nothing held"), "{err}");
    assert_eq!(s.scene.len(), 1);
}

#[test]
fn owned_job_captures_edit_version_and_does_not_acknowledge_opened_scene() {
    let (_home, _g) = home("snapshot-job");
    let mut state=AppState::new();
    boxed(&mut state,0.0);
    let captured=state.undo.version();
    let job=loom_commands::autosave::prepare_now(&state);
    boxed(&mut state,20.0);
    let path=job.write().unwrap().apply(&mut state);
    assert_eq!(state.autosave.last_version,Some(captured));
    assert_ne!(captured,state.undo.version());
    assert_eq!(loom_commands::file::read(&path).unwrap().scene.len(),1);
    let job=loom_commands::autosave::prepare_now(&state);
    let original=state.scene.instance_id();
    execute(&mut state,Command::OpenFile{path:Some(path.display().to_string())}).unwrap();
    assert_ne!(original,state.scene.instance_id());
    job.write().unwrap().apply(&mut state);
    assert_eq!(state.autosave.last_version,None);
    assert_eq!(state.autosave.next_index,3);
}
