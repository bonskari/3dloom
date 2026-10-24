//! Import File / Export File / Export Selected with .fbx.

use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp};
use loom_scene::{NodeId, ObjectKind};
use std::path::PathBuf;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join("loom-fbx-tests").join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn boxed(s: &mut AppState, x: f32) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [x, 0.0, 0.0], size: [10.0; 3] }).unwrap();
    *s.selection.iter().next().unwrap()
}

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../loom_fbx/tests/fixtures/character_for_mixamo.fbx");

#[test]
fn import_fbx_creates_nodes_with_hierarchy_materials_and_one_undo_entry() {
    let mut s = AppState::new();
    let before = s.undo.len();
    let r = execute(&mut s, Command::ImportFile { path: Some(FIXTURE.into()) }).unwrap();
    let ids: Vec<u64> = serde_json::from_value(r["nodes"].clone()).unwrap();
    assert!(!ids.is_empty());
    assert_eq!(s.undo.len(), before + 1, "one undo entry");
    assert_eq!(s.selection.len(), ids.len(), "imported nodes are selected");
    let meshes: Vec<_> = ids.iter().map(|i| s.scene.node(NodeId(*i)).unwrap()).filter(|n| matches!(n.object, ObjectKind::EditablePoly(_))).collect();
    assert!(!meshes.is_empty());
    assert!(meshes.iter().all(|n| !n.materials.is_empty()), "material names kept");
    assert!(s.scene.eval_mesh(meshes[0].id).value.mesh.map(1).is_some());
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(s.scene.node(NodeId(ids[0])).is_none());
}

#[test]
fn export_selected_child_keeps_world_transform() {
    let d = dir("child");
    let mut s = AppState::new();
    let a = boxed(&mut s, 0.0);
    let b = boxed(&mut s, 0.0);
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [100.0, 0.0, 0.0], system: None }).unwrap();
    execute(&mut s, Command::SetParent { id: b.0, parent: Some(a.0) }).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![b.0], op: SelectOp::Replace }).unwrap();
    let path = d.join("child.fbx");
    execute(&mut s, Command::ExportSelected { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    let doc = loom_fbx::read(&std::fs::read(&path).unwrap(), 0.0254).unwrap();
    assert_eq!(doc.nodes.len(), 1);
    assert_eq!(doc.nodes[0].parent, None);
    assert!(doc.nodes[0].translation.x.abs() < 1e-3, "the child stays at the world origin: {:?}", doc.nodes[0].translation);
    // And the parent alone carries its own move.
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    let path = d.join("parent.fbx");
    execute(&mut s, Command::ExportSelected { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    let doc = loom_fbx::read(&std::fs::read(&path).unwrap(), 0.0254).unwrap();
    assert_eq!(doc.nodes.len(), 1);
    assert!((doc.nodes[0].translation.x - 100.0).abs() < 1e-3, "{:?}", doc.nodes[0].translation);
}

#[test]
fn export_and_import_round_trip_keeps_names_materials_uvs_and_hierarchy() {
    let d = dir("round");
    let mut s = AppState::new();
    let a = boxed(&mut s, 0.0);
    let b = boxed(&mut s, 30.0);
    execute(&mut s, Command::SetParent { id: b.0, parent: Some(a.0) }).unwrap();
    s.scene.node_mut(a).unwrap().materials = vec!["Paint".into()];
    execute(&mut s, Command::CreateCamera { target: false }).unwrap();
    let path = d.join("scene.fbx");
    execute(&mut s, Command::ExportFile { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    execute(&mut s, Command::NewScene).unwrap();
    execute(&mut s, Command::ImportFile { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    let names: Vec<String> = s.scene.nodes().map(|n| n.name.clone()).collect();
    assert!(names.contains(&"Box001".to_string()) && names.contains(&"Box002".to_string()), "{names:?}");
    let a2 = s.scene.nodes().find(|n| n.name == "Box001").unwrap();
    let b2 = s.scene.nodes().find(|n| n.name == "Box002").unwrap();
    assert_eq!(b2.parent, Some(a2.id));
    assert_eq!(a2.materials, vec!["Paint".to_string()]);
    assert!(s.scene.nodes().any(|n| matches!(n.object, ObjectKind::Camera { .. })));
    assert!((b2.transform.position.get().x - 30.0).abs() < 1e-3, "{:?}", b2.transform.position.get());
    let m = s.scene.eval_mesh(a2.id).value.mesh.clone();
    assert_eq!(m.face_count(), 6);
    assert!(m.map(1).is_some());
    assert!(m.faces().iter().all(|f| f.smoothing_groups != 0));
}

#[test]
fn obj_still_goes_through_the_obj_path() {
    let d = dir("obj");
    let mut s = AppState::new();
    boxed(&mut s, 0.0);
    let path = d.join("x.obj");
    execute(&mut s, Command::ExportFile { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    assert!(std::fs::read_to_string(&path).unwrap().starts_with("# 3dLoom OBJ"));
}
