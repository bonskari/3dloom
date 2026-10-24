//! Phase 4 review fix pass: each test reproduces a finding.

use loom_commands::autosave::tick;
use loom_commands::{execute, AppState, Command, Dialog, PendingFileAction, PrimitiveKind, SaveChangesReply};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

static ENV: Mutex<()> = Mutex::new(());

fn home(name: &str) -> (PathBuf, MutexGuard<'static, ()>) {
    let guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let d = std::env::temp_dir().join("loom-review4-tests").join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::env::set_var("LOOM_HOME", &d);
    (d, guard)
}

fn boxed(s: &mut AppState, x: f32) {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [x, 0.0, 0.0], size: [10.0; 3] }).unwrap();
}

fn p(path: &PathBuf) -> Option<String> {
    Some(path.to_string_lossy().into_owned())
}

#[test]
fn autosave_resumes_after_open() {
    let (d, _g) = home("resume");
    let mut s = AppState::new();
    boxed(&mut s, 0.0);
    let file = d.join("a.loom");
    execute(&mut s, Command::SaveFile { path: p(&file) }).unwrap();
    let t0 = Instant::now();
    tick(&mut s, t0).unwrap();
    assert!(tick(&mut s, t0 + Duration::from_secs(300)).unwrap().is_some());
    execute(&mut s, Command::OpenFile { path: p(&file) }).unwrap();
    boxed(&mut s, 20.0);
    assert!(tick(&mut s, t0 + Duration::from_secs(600)).unwrap().is_some(), "an edit after Open is backed up");
    execute(&mut s, Command::ResetFile).unwrap();
    boxed(&mut s, 20.0);
    assert!(tick(&mut s, t0 + Duration::from_secs(900)).unwrap().is_some(), "an edit after Reset is backed up");
}

#[test]
fn save_incremental_handles_multibyte_names() {
    let (d, _g) = home("multibyte");
    let mut s = AppState::new();
    boxed(&mut s, 0.0);
    execute(&mut s, Command::SaveFile { path: p(&d.join("日本.loom")) }).unwrap();
    execute(&mut s, Command::SaveIncremental).unwrap();
    assert_eq!(s.file.as_deref(), Some(d.join("日本01.loom").as_path()));
    execute(&mut s, Command::SaveFile { path: p(&d.join("é7.loom")) }).unwrap();
    execute(&mut s, Command::SaveIncremental).unwrap();
    assert_eq!(s.file.as_deref(), Some(d.join("é01.loom").as_path()), "trailing digits are replaced");
}

#[test]
fn auto_backup_interval_is_clamped() {
    let (_d, _g) = home("clamp");
    let mut s = AppState::new();
    boxed(&mut s, 0.0);
    execute(&mut s, Command::SetPreference { name: "Auto Backup Interval".into(), value: 1e30 }).unwrap();
    assert!(s.prefs.auto_backup_interval_min <= 1440.0);
    let t0 = Instant::now();
    tick(&mut s, t0).unwrap();
    tick(&mut s, t0 + Duration::from_secs(60)).unwrap();
}

#[test]
fn import_names_are_unique() {
    let (d, _g) = home("names");
    let path = d.join("three.obj");
    std::fs::write(&path, "o Box\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\no Box\nv 2 0 0\nv 3 0 0\nv 2 1 0\nf 4 5 6\no Box\nv 4 0 0\nv 5 0 0\nv 4 1 0\nf 7 8 9\n").unwrap();
    let mut s = AppState::new();
    execute(&mut s, Command::ImportFile { path: p(&path) }).unwrap();
    let mut names: Vec<String> = s.scene.nodes().map(|n| n.name.clone()).collect();
    let n = names.len();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), n, "{names:?}");
}

#[test]
fn settings_missing_keys_default_and_viewports_match_layout() {
    let (d, _g) = home("defaults");
    let path = d.join("sparse.loom");
    let mut s = AppState::new();
    boxed(&mut s, 0.0);
    let scene = serde_json::to_value(&s.scene).unwrap();
    std::fs::write(&path, serde_json::json!({ "format": 1, "app": "x", "scene": scene, "settings": { "layout": "TwoV" } }).to_string()).unwrap();
    let mut t = AppState::new();
    execute(&mut t, Command::OpenFile { path: p(&path) }).unwrap();
    assert_eq!(t.layout, loom_commands::ViewportLayout::TwoV);
    assert_eq!(t.viewports.len(), 2, "viewports follow the layout when the file has none");
    assert_eq!(t.fps, 30.0);
    // A viewport list that does not fit the layout is rebuilt from the layout.
    std::fs::write(&path, serde_json::json!({ "format": 1, "app": "x", "scene": scene, "settings": { "layout": "FourGrid", "viewports": [] } }).to_string()).unwrap();
    execute(&mut t, Command::OpenFile { path: p(&path) }).unwrap();
    assert_eq!(t.viewports.len(), 4);
}

#[test]
fn save_changes_reply_runs_the_pending_action() {
    let (d, _g) = home("reply");
    let mut s = AppState::new();
    boxed(&mut s, 0.0);
    // Cancel keeps everything.
    s.dialog = Some(Dialog::SaveChanges(PendingFileAction::New));
    execute(&mut s, Command::SaveChangesReply { reply: SaveChangesReply::Cancel }).unwrap();
    assert_eq!(s.dialog, None);
    assert_eq!(s.scene.len(), 1);
    // No runs the action without saving.
    s.dialog = Some(Dialog::SaveChanges(PendingFileAction::Open));
    execute(&mut s, Command::SaveChangesReply { reply: SaveChangesReply::No }).unwrap();
    assert_eq!(s.dialog, Some(Dialog::OpenFile), "the Open dialog is up, not closed again");
    s.dialog = None;
    // Yes without a file: Save As first, then the action once saved.
    s.dialog = Some(Dialog::SaveChanges(PendingFileAction::New));
    execute(&mut s, Command::SaveChangesReply { reply: SaveChangesReply::Yes }).unwrap();
    assert_eq!(s.dialog, Some(Dialog::SaveFileAs));
    assert_eq!(s.pending_file_action, Some(PendingFileAction::New));
    assert_eq!(s.scene.len(), 1);
    let file = d.join("a.loom");
    execute(&mut s, Command::SaveFileAs { path: p(&file) }).unwrap();
    assert_eq!(s.scene.len(), 0, "New ran after the save");
    assert_eq!(s.pending_file_action, None);
    assert_eq!(loom_commands::file::read(&file).unwrap().scene.len(), 1);
    // Yes with a file: saves and runs at once; Exit sets the flag.
    execute(&mut s, Command::SaveFile { path: p(&d.join("c.loom")) }).unwrap();
    boxed(&mut s, 0.0);
    s.dialog = Some(Dialog::SaveChanges(PendingFileAction::Exit));
    execute(&mut s, Command::SaveChangesReply { reply: SaveChangesReply::Yes }).unwrap();
    assert!(s.exit_requested);
    assert!(!s.dirty());
    assert!(execute(&mut s, Command::SaveChangesReply { reply: SaveChangesReply::No }).is_err(), "no prompt open");
}

#[test]
fn new_scene_clears_the_file_name() {
    let (d, _g) = home("new");
    let mut s = AppState::new();
    boxed(&mut s, 0.0);
    let file = d.join("b.loom");
    execute(&mut s, Command::SaveFile { path: p(&file) }).unwrap();
    execute(&mut s, Command::NewScene).unwrap();
    assert_eq!(s.file, None);
    assert_eq!(s.title(), "Untitled - 3dLoom");
    execute(&mut s, Command::SaveFile { path: None }).unwrap();
    assert_eq!(s.dialog, Some(Dialog::SaveFileAs), "Save after New cannot overwrite the old file");
    assert_eq!(loom_commands::file::read(&file).unwrap().scene.len(), 1);
}
