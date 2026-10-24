//! Import File / Export File / Export Selected (OBJ).

use loom_commands::{execute, AppState, Command, Dialog, PrimitiveKind, SelectOp};
use loom_scene::{NodeId, ObjectKind};
use std::path::PathBuf;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join("loom-obj-tests").join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn boxed(s: &mut AppState, x: f32) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [x, 0.0, 0.0], size: [10.0; 3] }).unwrap();
    *s.selection.iter().next().unwrap()
}

#[test]
fn export_selected_writes_only_selection_in_world_space() {
    let d = dir("export_selected");
    let mut s = AppState::new();
    let a = boxed(&mut s, 0.0);
    let b = boxed(&mut s, 100.0);
    execute(&mut s, Command::SelectNodes { ids: vec![b.0], op: SelectOp::Replace }).unwrap();
    let path = d.join("sel.obj");
    execute(&mut s, Command::ExportSelected { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(text.matches("\no ").count(), 1, "{text}");
    assert!(text.contains("o Box002"));
    let objs = loom_mesh::obj::parse(&text, "x").unwrap();
    let (lo, hi) = objs[0].mesh.aabb();
    assert!((lo.x - 95.0).abs() < 1e-3 && (hi.x - 105.0).abs() < 1e-3, "world space: {lo:?} {hi:?}");
    // Export File writes every visible geometry object; a hidden one is skipped.
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::HideSelection).unwrap();
    let all = d.join("all.obj");
    execute(&mut s, Command::ExportFile { path: Some(all.to_string_lossy().into_owned()) }).unwrap();
    let text = std::fs::read_to_string(&all).unwrap();
    assert!(text.contains("o Box002") && !text.contains("o Box001"), "{text}");
    execute(&mut s, Command::ExportFile { path: None }).unwrap();
    assert_eq!(s.dialog, Some(Dialog::ExportFile));
}

#[test]
fn import_creates_editable_polys_as_one_undo_entry() {
    let d = dir("import");
    let path = d.join("two.obj");
    std::fs::write(&path, "o Left\nv 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\ns 1\nf 1 2 3 4\no Right\nv 5 0 0\nv 6 0 0\nv 6 1 0\nf 5 6 7\n").unwrap();
    let mut s = AppState::new();
    boxed(&mut s, 0.0);
    let r = execute(&mut s, Command::ImportFile { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    assert_eq!(r["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(s.scene.len(), 3);
    let names: Vec<String> = s.scene.nodes().map(|n| n.name.clone()).collect();
    assert!(names.contains(&"Left".to_owned()) && names.contains(&"Right".to_owned()), "{names:?}");
    assert_eq!(s.selection.len(), 2, "the imported objects are the selection");
    for id in s.selection.clone() {
        assert!(matches!(s.scene.node(id).unwrap().object, ObjectKind::EditablePoly(_)));
    }
    assert_eq!(s.undo.labels().last().unwrap(), "Import two.obj");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.len(), 1, "one undo removes both");
    // A bad file imports nothing and reports the line.
    let bad = d.join("bad.obj");
    std::fs::write(&bad, "v 0 0 0\nf 1 2 3\n").unwrap();
    let e = execute(&mut s, Command::ImportFile { path: Some(bad.to_string_lossy().into_owned()) }).unwrap_err();
    assert!(e.contains("bad.obj") && e.contains("line 2"), "{e}");
    assert_eq!(s.scene.len(), 1);
}

#[test]
fn export_then_import_matches_face_counts() {
    let d = dir("round");
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Teapot, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    let t = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::AddModifier { name: "Bend".into() }).unwrap();
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "Angle".into(), value: 45.0 }).unwrap();
    let faces = s.scene.eval_mesh(t).value.mesh.face_count();
    let bent_top = s.scene.eval_mesh(t).value.mesh.aabb().1;
    let path = d.join("teapot.obj");
    execute(&mut s, Command::ExportFile { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    execute(&mut s, Command::NewScene).unwrap();
    execute(&mut s, Command::ImportFile { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    let id = *s.selection.iter().next().unwrap();
    let m = s.scene.eval_mesh(id).value.mesh.clone();
    m.check_invariants().unwrap();
    assert_eq!(m.face_count(), faces);
    assert!((m.aabb().1 - bent_top).length() < 1e-3, "the stack result was exported");
}
