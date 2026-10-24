//! Open / Save / Save As / Save As Copy / Save Incremental / Reset File.

use loom_commands::{execute, parse_chord, AppState, Command, Dialog, PrimitiveKind, SelectOp, DEFERRED};
use loom_scene::NodeId;
use std::path::PathBuf;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join("loom-file-cmd-tests").join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn boxed(s: &mut AppState, x: f32) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [x, 0.0, 0.0], size: [10.0; 3] }).unwrap();
    *s.selection.iter().next().unwrap()
}

fn p(path: &PathBuf) -> Option<String> {
    Some(path.to_string_lossy().into_owned())
}

#[test]
fn save_then_open_restores_scene_and_selection() {
    let d = dir("save_open");
    let path = d.join("scene.loom");
    let mut s = AppState::new();
    let a = boxed(&mut s, 0.0);
    boxed(&mut s, 20.0);
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::NamedSelectionSet { name: "first".into() }).unwrap();
    execute(&mut s, Command::SaveFile { path: p(&path) }).unwrap();
    assert_eq!(s.file.as_deref(), Some(path.as_path()));
    assert!(!s.dirty());
    let mut t = AppState::new();
    let r = execute(&mut t, Command::OpenFile { path: p(&path) }).unwrap();
    assert_eq!(r["nodes"], 2);
    assert_eq!(t.scene.len(), 2);
    assert_eq!(t.selection.iter().copied().collect::<Vec<_>>(), vec![a]);
    assert_eq!(t.named_sets.get("first").unwrap(), &vec![a]);
    assert_eq!(t.file.as_deref(), Some(path.as_path()));
    assert!(!t.dirty());
    assert!(!t.undo.can_undo(), "Open clears the undo stack");
}

#[test]
fn save_file_without_path_opens_save_as_dialog() {
    let mut s = AppState::new();
    boxed(&mut s, 0.0);
    execute(&mut s, Command::SaveFile { path: None }).unwrap();
    assert_eq!(s.dialog, Some(Dialog::SaveFileAs));
    execute(&mut s, Command::OpenFile { path: None }).unwrap();
    assert_eq!(s.dialog, Some(Dialog::OpenFile));
    execute(&mut s, Command::SaveFileAsCopy { path: None }).unwrap();
    assert_eq!(s.dialog, Some(Dialog::SaveFileAsCopy));
    // With a current file, Save goes straight to it.
    let path = dir("save_direct").join("a.loom");
    execute(&mut s, Command::SaveFileAs { path: p(&path) }).unwrap();
    boxed(&mut s, 30.0);
    assert!(s.dirty());
    execute(&mut s, Command::SaveFile { path: None }).unwrap();
    assert!(!s.dirty());
    assert_eq!(loom_commands::file::read(&path).unwrap().scene.len(), 2);
}

#[test]
fn save_as_copy_keeps_current_file() {
    let d = dir("copy");
    let mut s = AppState::new();
    boxed(&mut s, 0.0);
    let a = d.join("a.loom");
    let b = d.join("b.loom");
    execute(&mut s, Command::SaveFile { path: p(&a) }).unwrap();
    boxed(&mut s, 20.0);
    execute(&mut s, Command::SaveFileAsCopy { path: p(&b) }).unwrap();
    assert_eq!(s.file.as_deref(), Some(a.as_path()));
    assert!(s.dirty(), "a copy does not mark the scene saved");
    assert_eq!(loom_commands::file::read(&b).unwrap().scene.len(), 2);
    assert_eq!(loom_commands::file::read(&a).unwrap().scene.len(), 1);
}

#[test]
fn save_incremental_numbers_files() {
    let d = dir("incremental");
    let mut s = AppState::new();
    boxed(&mut s, 0.0);
    execute(&mut s, Command::SaveIncremental).unwrap();
    assert_eq!(s.dialog, Some(Dialog::SaveFileAs), "no current file: acts as Save As");
    s.dialog = None;
    let a = d.join("a.loom");
    execute(&mut s, Command::SaveFile { path: p(&a) }).unwrap();
    execute(&mut s, Command::SaveIncremental).unwrap();
    assert_eq!(s.file.as_deref(), Some(d.join("a01.loom").as_path()));
    execute(&mut s, Command::SaveIncremental).unwrap();
    assert_eq!(s.file.as_deref(), Some(d.join("a02.loom").as_path()));
    assert!(d.join("a01.loom").exists() && d.join("a02.loom").exists());
    assert!(!s.dirty());
}

#[test]
fn open_clears_undo_and_sets_title() {
    let d = dir("title");
    let mut s = AppState::new();
    assert_eq!(s.title(), "Untitled - 3dLoom");
    boxed(&mut s, 0.0);
    assert_eq!(s.title(), "Untitled * - 3dLoom");
    let path = d.join("My Scene.loom");
    execute(&mut s, Command::SaveFile { path: p(&path) }).unwrap();
    assert_eq!(s.title(), "My Scene.loom - 3dLoom");
    boxed(&mut s, 20.0);
    assert_eq!(s.title(), "My Scene.loom * - 3dLoom");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(!s.dirty(), "undone back to the saved version is clean");
    let mut t = AppState::new();
    boxed(&mut t, 0.0);
    execute(&mut t, Command::OpenFile { path: p(&path) }).unwrap();
    assert!(!t.undo.can_undo());
    assert_eq!(t.title(), "My Scene.loom - 3dLoom");
}

#[test]
fn reset_file_restores_defaults() {
    let d = dir("reset");
    let mut s = AppState::new();
    boxed(&mut s, 0.0);
    execute(&mut s, Command::ViewportLayout { layout: loom_commands::ViewportLayout::Single }).unwrap();
    execute(&mut s, Command::SetTimeConfiguration { start: Some(5), end: Some(50), fps: Some(24.0) }).unwrap();
    execute(&mut s, Command::SaveFile { path: p(&d.join("x.loom")) }).unwrap();
    execute(&mut s, Command::ResetFile).unwrap();
    assert_eq!(s.scene.len(), 0);
    assert_eq!(s.file, None);
    assert_eq!(s.layout, loom_commands::ViewportLayout::FourGrid);
    assert_eq!(s.frame_range, (0, 100));
    assert_eq!(s.fps, 30.0);
    assert!(!s.undo.can_undo());
    assert!(!s.dirty());
    assert_eq!(s.title(), "Untitled - 3dLoom");
}

#[test]
fn corrupt_file_leaves_scene_untouched() {
    let d = dir("corrupt");
    let bad = d.join("bad.loom");
    std::fs::write(&bad, "{ \"format\": 1, \"scene\": 42 }").unwrap();
    let mut s = AppState::new();
    boxed(&mut s, 0.0);
    let err = execute(&mut s, Command::OpenFile { path: p(&bad) }).unwrap_err();
    assert!(err.contains("bad.loom"), "{err}");
    assert_eq!(s.scene.len(), 1);
    assert!(s.undo.can_undo());
    assert_eq!(s.file, None);
    assert!(execute(&mut s, Command::OpenFile { path: p(&d.join("missing.loom")) }).is_err());
    assert_eq!(s.scene.len(), 1);
}

#[test]
fn open_and_save_are_registered_and_bound() {
    let s = AppState::new();
    assert_eq!(s.hotkey(parse_chord("Ctrl+O").unwrap()), Some("Open File"));
    assert_eq!(s.hotkey(parse_chord("Ctrl+S").unwrap()), Some("Save File"));
    assert!(!DEFERRED.iter().any(|(n, _)| *n == "Open File" || *n == "Save File"));
    for name in ["Open File", "Save File", "Save File As", "Save File As Copy", "Save Incremental (+)", "Reset File"] {
        assert!(loom_commands::registry().iter().any(|i| i.name == name), "{name} missing");
    }
}
