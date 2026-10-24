//! File > Import of .max scenes. Reads a .max file from a local asset folder
//! (not in the repository) named by the `LOOM_TEST_ASSETS` environment
//! variable; skips when it is unset or the file is missing.

use loom_commands::{execute, AppState, Command};
use loom_scene::NodeId;

#[test]
fn import_max_keeps_world_placement_and_undoes() {
    let Some(assets) = std::env::var_os("LOOM_TEST_ASSETS") else {
        eprintln!("skipped: LOOM_TEST_ASSETS is not set");
        return;
    };
    let truck = std::path::Path::new(&assets).join("Models/Truck/Turck.max");
    if !truck.exists() {
        eprintln!("skipped: {} is not available", truck.display());
        return;
    }
    let mut s = AppState::new();
    let r = execute(&mut s, Command::ImportFile { path: Some(truck.to_string_lossy().into_owned()) }).unwrap();
    let ids: Vec<NodeId> = r["nodes"].as_array().unwrap().iter().map(|v| NodeId(v.as_u64().unwrap())).collect();
    let doc = loom_max::read(&std::fs::read(&truck).unwrap()).unwrap();
    assert_eq!(ids.len(), doc.nodes.len());
    // Every node's world and object matrices are the file's (parent chain × local, × pivot offset).
    let world = |i: usize| {
        let mut m = doc.nodes[i].local;
        let mut p = doc.nodes[i].parent;
        while let Some(pi) = p {
            m = doc.nodes[pi].local * m;
            p = doc.nodes[pi].parent;
        }
        m
    };
    for (i, id) in ids.iter().enumerate() {
        let want = world(i) * doc.nodes[i].offset;
        let got = s.scene.object_matrix(*id);
        for (a, b) in got.to_cols_array().iter().zip(want.to_cols_array()) {
            assert!((a - b).abs() < 1e-2 * (1.0 + b.abs()), "{}: {got} vs {want}", doc.nodes[i].name);
        }
        assert_eq!(s.scene.node(*id).unwrap().name.is_empty(), false);
    }
    // What was left out is reported for the Import Summary window.
    assert!(s.import_report.iter().any(|w| w.contains("modifier stack not imported")), "{:?}", s.import_report);
    // Parametric primitives stay parametric.
    assert!(ids.iter().any(|id| matches!(s.scene.node(*id).unwrap().object, loom_scene::ObjectKind::Cylinder { .. })));
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.nodes().count(), 0, "one undo step");
}
