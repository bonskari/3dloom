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

/// Open finds a referenced file gone: Missing XRef Paths shows (not Missing
/// External Files); Browse with the folder it moved to references it from
/// there and reads it again.
#[test]
fn missing_xref_paths_on_open_and_browse_finds_the_moved_file() {
    let d = dir("missing");
    let old_dir = d.join("old");
    let new_dir = d.join("new");
    std::fs::create_dir_all(&old_dir).unwrap();
    std::fs::create_dir_all(&new_dir).unwrap();
    let path = old_dir.join("props.loom");
    let mut src = AppState::new();
    named_box(&mut src, "Crate", 0.0);
    save(&mut src, &path, 1);
    let mut s = AppState::new();
    execute(&mut s, Command::XrefObjects { path: Some(path.to_string_lossy().into_owned()), names: None, automatic: Some(false) }).unwrap();
    let scene = d.join("scene.loom");
    execute(&mut s, Command::SaveFileAsCopy { path: Some(scene.to_string_lossy().into_owned()) }).unwrap();
    std::fs::rename(&path, new_dir.join("props.loom")).unwrap();
    let mut s = AppState::new();
    let r = execute(&mut s, Command::OpenFile { path: Some(scene.to_string_lossy().into_owned()) }).unwrap();
    assert_eq!(r["missing_xrefs"].as_array().unwrap().len(), 1, "{r}");
    assert!(r["missing"].as_array().unwrap().is_empty(), "an XRef is not a missing external file: {r}");
    assert_eq!(s.dialog, Some(loom_commands::Dialog::MissingXrefPaths));
    let r = execute(&mut s, Command::FindMissingXrefs { folder: new_dir.to_string_lossy().into_owned() }).unwrap();
    assert_eq!(r["found"].as_array().unwrap().len(), 1, "{r}");
    assert!(r["missing"].as_array().unwrap().is_empty());
    assert_eq!(s.scene.xrefs[0].path, new_dir.join("props.loom").to_string_lossy());
    by_name(&s, "Crate");
}

/// The XRef dialogs' Automatic switches a reference's automatic update, one
/// undo entry; XRef Record List opens the dialog.
#[test]
fn set_xref_automatic_toggles_and_undoes() {
    let d = std::env::temp_dir().join("loom-xref-automatic");
    std::fs::create_dir_all(&d).unwrap();
    let path = d.join("props.loom");
    let mut src = AppState::new();
    named_box(&mut src, "Crate", 0.0);
    save(&mut src, &path, 1);
    let mut s = AppState::new();
    let p = path.to_string_lossy().into_owned();
    execute(&mut s, Command::XrefScene { path: Some(p.clone()), automatic: None }).unwrap();
    assert!(s.scene.xrefs[0].automatic, "on by default");
    execute(&mut s, Command::SetXrefAutomatic { path: p.clone(), automatic: false }).unwrap();
    assert!(!s.scene.xrefs[0].automatic);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(s.scene.xrefs[0].automatic, "undone");
    assert!(execute(&mut s, Command::SetXrefAutomatic { path: "/nowhere.loom".into(), automatic: false }).is_err());
    execute(&mut s, Command::XrefRecords { scene: true }).unwrap();
    assert_eq!(s.dialog, Some(loom_commands::Dialog::XrefRecords { scene: true }));
}

/// The XRef dialogs' Enabled, path, Overlay, Merge and Convert Selected.
#[test]
fn xref_enabled_path_overlay_merge_and_convert_selected() {
    let d = dir("dialog");
    let (a, b) = (d.join("a.loom"), d.join("b.loom"));
    let mut src = AppState::new();
    named_box(&mut src, "Wall", 0.0);
    save(&mut src, &a, 1);
    let mut src2 = AppState::new();
    named_box(&mut src2, "Wall", 0.0);
    named_box(&mut src2, "Door", 20.0);
    save(&mut src2, &b, 2);
    let (ap, bp) = (a.to_string_lossy().into_owned(), b.to_string_lossy().into_owned());
    let mut s = AppState::new();
    execute(&mut s, Command::XrefScene { path: Some(ap.clone()), automatic: Some(false) }).unwrap();
    assert!(s.scene.xrefs[0].enabled && !s.scene.xrefs[0].overlay, "Enabled on, Overlay off by default");
    // Enabled off: the objects go, the record stays; on brings them back.
    execute(&mut s, Command::SetXrefEnabled { path: ap.clone(), scene: true, enabled: false }).unwrap();
    assert_eq!((s.scene.nodes().count(), s.scene.xrefs.len()), (0, 1));
    execute(&mut s, Command::UpdateXrefs { path: None }).unwrap();
    assert_eq!(s.scene.nodes().count(), 0, "a disabled reference is not read");
    execute(&mut s, Command::SetXrefEnabled { path: ap.clone(), scene: true, enabled: true }).unwrap();
    assert!(s.scene.node(by_name(&s, "Wall")).unwrap().frozen);
    // The path field: b.loom has Wall and Door.
    execute(&mut s, Command::SetXrefPath { path: Some(ap.clone()), scene: true, new_path: bp.clone() }).unwrap();
    assert_eq!(s.scene.xrefs[0].path, bp);
    let mut names: Vec<String> = s.scene.nodes().map(|n| n.name.clone()).collect();
    names.sort();
    assert_eq!(names, ["Door", "Wall"]);
    // Overlay: a scene that references this one leaves the overlay's objects out.
    execute(&mut s, Command::SetXrefOverlay { path: bp.clone(), scene: true, overlay: true }).unwrap();
    named_box(&mut s, "Lamp", 50.0);
    let master = d.join("master.loom");
    save(&mut s, &master, 3);
    let mut outer = AppState::new();
    execute(&mut outer, Command::XrefScene { path: Some(master.to_string_lossy().into_owned()), automatic: Some(false) }).unwrap();
    let names: Vec<String> = outer.scene.nodes().map(|n| n.name.clone()).collect();
    assert_eq!(names, ["Lamp"], "the overlay's Wall and Door stay out");
    // Merge: the objects become this scene's own, unfrozen, and the record goes.
    execute(&mut s, Command::MergeXref { path: bp.clone(), scene: true }).unwrap();
    assert!(s.scene.xrefs.is_empty());
    let door = by_name(&s, "Door");
    assert!(!s.scene.node(door).unwrap().frozen);
    execute(&mut s, Command::SetTransform { id: door.0, position: Some([1.0, 0.0, 0.0]), rotation_deg: None, scale: None }).unwrap();
    // Convert Selected: Lamp goes to its own file and comes back as an XRef Scene.
    let lamp = by_name(&s, "Lamp");
    execute(&mut s, Command::SelectNodes { ids: vec![lamp.0], op: loom_commands::SelectOp::Replace }).unwrap();
    let lp = d.join("lamp.loom");
    execute(&mut s, Command::ConvertSelectedToXref { path: Some(lp.to_string_lossy().into_owned()) }).unwrap();
    assert!(lp.is_file());
    assert_eq!(s.scene.xrefs.len(), 1);
    assert!(s.scene.xrefs[0].scene);
    assert!(s.scene.node(by_name(&s, "Lamp")).unwrap().frozen, "Lamp is a scene reference now");
}
