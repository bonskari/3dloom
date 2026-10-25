//! File > XRef Objects / XRef Scene: read-only references to the objects of
//! another file, updated from it.

use loom_commands::{execute, AppState, Command, PrimitiveKind};
use loom_scene::{NodeId, ObjectKind};
use std::path::PathBuf;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join("loom-xref-tests").join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn named_box(s: &mut AppState, name: &str, x: f32) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [x, 0.0, 0.0], size: [10.0; 3] }).unwrap();
    let id = *s.selection.iter().next().unwrap();
    execute(s, Command::Rename { id: id.0, name: name.into() }).unwrap();
    id
}

fn save(s: &mut AppState, path: &PathBuf, stamp: u64) {
    execute(s, Command::SaveFileAsCopy { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    // A distinct modification time, whatever the file system's clock resolution.
    let t = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000 + stamp);
    std::fs::File::options().write(true).open(path).unwrap().set_modified(t).unwrap();
}

fn length(s: &AppState, id: NodeId) -> f32 {
    let ObjectKind::Box { length, .. } = &s.scene.node(id).unwrap().object else { panic!() };
    length.get()
}

fn by_name(s: &AppState, name: &str) -> NodeId {
    s.scene.nodes().find(|n| n.name == name).unwrap_or_else(|| panic!("no {name}")).id
}

#[test]
fn xref_objects_are_read_only_and_follow_their_file() {
    let d = dir("objects");
    let path = d.join("props.loom");
    let mut src = AppState::new();
    let crate_id = named_box(&mut src, "Crate", 0.0);
    named_box(&mut src, "Barrel", 30.0);
    save(&mut src, &path, 1);
    let mut s = AppState::new();
    let r = execute(&mut s, Command::XrefObjects { path: Some(path.to_string_lossy().into_owned()), names: Some(vec!["Crate".into()]), automatic: Some(false) }).unwrap();
    assert_eq!(r["nodes"].as_array().unwrap().len(), 1);
    let id = by_name(&s, "Crate");
    assert_eq!(s.scene.nodes().count(), 1, "only the chosen object");
    assert_eq!(s.scene.xrefs.len(), 1);
    // Its object cannot be edited here; it can be moved.
    let e = execute(&mut s, Command::SetParameter { id: id.0, name: "Length".into(), value: 50.0 }).unwrap_err();
    assert!(e.contains("XRef"), "{e}");
    assert_eq!(length(&s, id), 10.0);
    execute(&mut s, Command::SetTransform { id: id.0, position: Some([5.0, 6.0, 7.0]), rotation_deg: None, scale: None }).unwrap();
    // The file changes: Update takes its object and keeps the placement made here.
    execute(&mut src, Command::SetParameter { id: crate_id.0, name: "Length".into(), value: 42.0 }).unwrap();
    save(&mut src, &path, 2);
    execute(&mut s, Command::UpdateXrefs { path: None }).unwrap();
    assert_eq!(length(&s, id), 42.0);
    assert_eq!(s.scene.node(id).unwrap().transform.position.get(), glam::Vec3::new(5.0, 6.0, 7.0));
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(length(&s, id), 10.0, "Update is one undo step");
    execute(&mut s, Command::RedoSceneOperation).unwrap();
    // Remove drops the reference and its objects.
    execute(&mut s, Command::RemoveXref { path: path.to_string_lossy().into_owned() }).unwrap();
    assert_eq!((s.scene.nodes().count(), s.scene.xrefs.len()), (0, 0));
}

#[test]
fn xref_scene_is_frozen_and_takes_in_what_the_file_gains() {
    let d = dir("scene");
    let path = d.join("set.loom");
    let mut src = AppState::new();
    named_box(&mut src, "Wall", 0.0);
    let floor = named_box(&mut src, "Floor", 30.0);
    save(&mut src, &path, 1);
    let mut s = AppState::new();
    named_box(&mut s, "Hero", 100.0);
    execute(&mut s, Command::XrefScene { path: Some(path.to_string_lossy().into_owned()), automatic: Some(true) }).unwrap();
    let wall = by_name(&s, "Wall");
    assert!(s.scene.node(wall).unwrap().frozen, "XRef Scene objects cannot be selected");
    let e = execute(&mut s, Command::SetTransform { id: wall.0, position: Some([1.0, 0.0, 0.0]), rotation_deg: None, scale: None }).unwrap_err();
    assert!(e.contains("XRef Scene"), "{e}");
    // The file loses Floor and gains Door; Update Automatically sees the change on disk.
    execute(&mut src, Command::SelectNodes { ids: vec![floor.0], op: loom_commands::SelectOp::Replace }).unwrap();
    execute(&mut src, Command::DeleteObjects).unwrap();
    named_box(&mut src, "Door", 60.0);
    save(&mut src, &path, 2);
    let updated = loom_commands::xref::tick(&mut s).unwrap();
    assert_eq!(updated.len(), 1);
    let mut names: Vec<String> = s.scene.nodes().map(|n| n.name.clone()).collect();
    names.sort();
    assert_eq!(names, ["Door", "Hero", "Wall"]);
    assert!(s.scene.node(by_name(&s, "Door")).unwrap().frozen);
    // Nothing changed since: the next check reads nothing.
    assert!(loom_commands::xref::tick(&mut s).unwrap().is_empty());
    // A saved scene keeps its references.
    let saved = d.join("shot.loom");
    execute(&mut s, Command::SaveFileAsCopy { path: Some(saved.to_string_lossy().into_owned()) }).unwrap();
    let doc = loom_commands::file::read(&saved).unwrap();
    assert_eq!(doc.scene.xrefs.len(), 1);
    assert!(doc.scene.xrefs[0].scene);
}
