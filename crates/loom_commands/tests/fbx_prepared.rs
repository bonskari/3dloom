use glam::Vec3;
use loom_commands::{execute, fbx_io, AppState, Command};
use loom_fbx::{FbxDocument, FbxKind, FbxNode, WriteOpts};
use loom_scene::ObjectKind;

fn fixture(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("loom-fbx-prepared-{name}.fbx"));
    let doc = FbxDocument { nodes: vec![FbxNode::new("joint", FbxKind::Bone { length: 1.0, size: 0.1 }), FbxNode::new("box", FbxKind::Mesh { mesh: loom_mesh::box_primitive(Vec3::ONE), materials: vec![] })], unit_metres: 0.0254, ..Default::default() };
    std::fs::write(&path, loom_fbx::write(&doc, &WriteOpts::default())).unwrap();
    path
}
#[test]
fn prepared_import_uses_snapshot_once_without_reopening_source_and_is_one_undo() {
    fn assert_send<T: Send>() {}
    assert_send::<fbx_io::PreparedFbxImport>();
    let path = fixture("once");
    let mut s = AppState::new();
    let prepared = fbx_io::prepare(&path, &s.exchange.fbx_import, 0.0254, false).unwrap();
    assert_eq!(s.scene.nodes().count(), 0);
    std::fs::remove_file(&path).unwrap();
    s.exchange.fbx_import.bones_as_dummies = true;
    s.prepared_fbx_import = Some(prepared);
    let steps = s.undo.len();
    let result = execute(&mut s, Command::ImportFile { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    assert_eq!(result["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(s.undo.len(), steps + 1);
    assert_eq!(s.scene.nodes().filter(|n| matches!(n.object, ObjectKind::Bone { .. })).count(), 1);
    assert!(s.exchange.fbx_import.bones_as_dummies);
    assert!(s.prepared_fbx_import.is_none());
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.nodes().count(), 0);
    execute(&mut s, Command::RedoSceneOperation).unwrap();
    assert_eq!(s.scene.nodes().count(), 2);
}
#[test]
fn mismatched_prepared_path_kind_or_units_do_not_modify_scene() {
    let path = fixture("reject");
    for mismatch in ["path", "kind", "units"] {
        let mut s = AppState::new();
        s.prepared_fbx_import = Some(fbx_io::prepare(&path, &s.exchange.fbx_import, 0.0254, false).unwrap());
        if mismatch == "units" { s.units.system_scale = 2.0; }
        let source = if mismatch == "path" { path.with_file_name("another.fbx") } else { path.clone() };
        let command = if mismatch == "kind" { Command::ImportAnimationClip { path: Some(source.to_string_lossy().into_owned()) } } else { Command::ImportFile { path: Some(source.to_string_lossy().into_owned()) } };
        let before = serde_json::to_value(&s.scene).unwrap();
        let undo = s.undo.len();
        assert!(execute(&mut s, command).is_err(), "{mismatch}");
        assert_eq!(serde_json::to_value(&s.scene).unwrap(), before);
        assert_eq!(s.undo.len(), undo);
    }
    std::fs::remove_file(path).unwrap();
}
#[test]
fn preparation_errors_do_not_touch_existing_scene() {
    let path = fixture("no-animation");
    let mut s = AppState::new();
    execute(&mut s, Command::ImportFile { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    let before = serde_json::to_value(&s.scene).unwrap();
    let undo = s.undo.len();
    assert!(fbx_io::prepare(&path, &s.exchange.fbx_import, 0.0254, true).unwrap_err().contains("no animated take"));
    assert_eq!(serde_json::to_value(&s.scene).unwrap(), before);
    assert_eq!(s.undo.len(), undo);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn external_prepared_metahuman_preserves_source_and_commits_native_body_atomically() {
    let Some(path) = std::env::var_os("LOOM_METAHUMAN_FBX") else { return };
    let path = std::path::Path::new(&path);
    let bytes = std::fs::read(path).unwrap();
    let mut s = AppState::new();
    s.exchange.fbx_import.metahuman_body_only = true;
    let start = std::time::Instant::now();
    s.prepared_fbx_import = Some(fbx_io::prepare(path, &s.exchange.fbx_import, 0.0254, false).unwrap());
    eprintln!("PREPARED source_bytes={} prepare_ms={:.3}", bytes.len(), start.elapsed().as_secs_f64() * 1000.0);
    assert_eq!(s.scene.nodes().count(), 0);
    let start = std::time::Instant::now();
    execute(&mut s, Command::ImportFile { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    eprintln!("PREPARED native_commit_ms={:.3}", start.elapsed().as_secs_f64() * 1000.0);
    assert_eq!(s.scene.nodes().filter(|n| matches!(n.object, ObjectKind::Bone { .. })).count(), 342);
    assert_eq!(std::fs::read(path).unwrap(), bytes);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.nodes().count(), 0);
}
