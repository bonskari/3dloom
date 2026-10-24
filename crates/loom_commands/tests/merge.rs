//! Merge File: nodes from another .loom join the scene with fresh ids.

use loom_commands::{execute, AppState, Command, Dialog, PrimitiveKind, SelectOp};
use loom_scene::{Modifier, NodeId, ObjectKind};
use std::path::PathBuf;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join("loom-merge-tests").join(name);
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

/// A file with: Box001 (parent of Box002), Box003 = instance of Box001 with
/// an instanced Bend, Circle001 picked as the Linked XForm control of Box002, and
/// a target camera.
fn source(path: &PathBuf) -> AppState {
    let mut s = AppState::new();
    let a = boxed(&mut s, 0.0);
    let b = boxed(&mut s, 20.0);
    execute(&mut s, Command::SelectNodes { ids: vec![b.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SetParent { id: b.0, parent: Some(a.0) }).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::AddModifier { name: "Bend".into() }).unwrap();
    execute(&mut s, Command::CloneOptions { kind: loom_commands::CloneKind::Instance, copies: 1, name: None, offset: Some([40.0, 0.0, 0.0]), rotate_deg: None, scale: None }).unwrap();
    execute(&mut s, Command::CreateShape { kind: loom_commands::ShapeKind::Circle, position: [0.0, 50.0, 0.0], size: [4.0; 3] }).unwrap();
    let circle = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![b.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::AddModifier { name: "Linked XForm".into() }).unwrap();
    execute(&mut s, Command::PickModifierNode { index: 0, node: circle.0 }).unwrap();
    execute(&mut s, Command::CreateCamera { target: true }).unwrap();
    execute(&mut s, Command::SaveFile { path: p(path) }).unwrap();
    s
}

#[test]
fn merge_adds_all_nodes_with_fresh_ids_and_selects_them() {
    let d = dir("all");
    let path = d.join("src.loom");
    let src = source(&path);
    let mut s = AppState::new();
    let mine = boxed(&mut s, 0.0);
    let r = execute(&mut s, Command::MergeFile { path: p(&path), names: None }).unwrap();
    let added = r["nodes"].as_array().unwrap().len();
    assert_eq!(added, src.scene.len());
    assert_eq!(s.scene.len(), src.scene.len() + 1);
    assert_eq!(s.selection.len(), added, "merged nodes become the selection");
    assert!(!s.selection.contains(&mine));
    assert!(s.selection.iter().all(|id| id.0 > mine.0), "fresh ids");
    assert!(s.dirty());
    execute(&mut s, Command::MergeFile { path: None, names: None }).unwrap();
    assert_eq!(s.dialog, Some(Dialog::MergeFile));
}

#[test]
fn merge_remaps_all_node_references() {
    let d = dir("remap");
    let path = d.join("src.loom");
    source(&path);
    let mut s = AppState::new();
    let mine = boxed(&mut s, 0.0);
    boxed(&mut s, 1.0);
    boxed(&mut s, 2.0);
    execute(&mut s, Command::MergeFile { path: p(&path), names: None }).unwrap();
    let merged: Vec<NodeId> = s.selection.iter().copied().collect();
    let is_merged = |id: NodeId| merged.contains(&id);
    let mut parents = 0;
    let mut instances = 0;
    let mut mod_instances = 0;
    let mut picks = 0;
    let mut targets = 0;
    for id in &merged {
        let n = s.scene.node(*id).unwrap();
        if let Some(parent) = n.parent {
            assert!(is_merged(parent), "{}: parent {} is not a merged node", n.name, parent.0);
            parents += 1;
        }
        if let Some(src) = n.instance_of {
            assert!(is_merged(src), "{}: instance source outside the merge", n.name);
            instances += 1;
        }
        for slot in &n.modifiers {
            if let Some((src, _)) = slot.instance_of {
                assert!(is_merged(src));
                mod_instances += 1;
            }
            if let Some(picked) = slot.modifier.picked_node() {
                assert!(is_merged(picked), "{}: picked node outside the merge", n.name);
                picks += 1;
            }
        }
        if let ObjectKind::Camera { target: Some(t), .. } = n.object {
            assert!(is_merged(t));
            targets += 1;
        }
    }
    assert!(parents >= 1 && instances == 1 && picks == 1 && targets == 1, "{parents} {instances} {mod_instances} {picks} {targets}");
    let _ = mod_instances; // whole-node instances carry their stack; slot-level instances are checked by the remap code path too
    assert!(s.scene.node(mine).unwrap().parent.is_none(), "my nodes are untouched");
    // The instanced Bend still evaluates through its merged source.
    let inst = merged.iter().find(|id| s.scene.node(**id).unwrap().instance_of.is_some()).unwrap();
    assert!(matches!(s.scene.resolved_modifier(*inst, 0), Some(Modifier::Bend { .. })));
}

#[test]
fn merge_renames_colliding_names() {
    let d = dir("names");
    let path = d.join("src.loom");
    source(&path);
    let mut s = AppState::new();
    boxed(&mut s, 0.0);
    boxed(&mut s, 1.0);
    execute(&mut s, Command::MergeFile { path: p(&path), names: Some(vec!["Box001".into(), "Box002".into()]) }).unwrap();
    let mut names: Vec<String> = s.scene.nodes().map(|n| n.name.clone()).collect();
    let before = names.len();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), before, "every name unique: {names:?}");
    assert!(names.iter().any(|n| n.starts_with("Box") && n != "Box001" && n != "Box002"), "{names:?}");
}

#[test]
fn merge_subset_by_name_drops_outside_parent_keeping_world_transform() {
    let d = dir("subset");
    let path = d.join("src.loom");
    let src = source(&path);
    let child = src.scene.nodes().find(|n| n.name == "Box002").unwrap();
    let world = src.scene.world_matrix(child.id);
    let mut s = AppState::new();
    let r = execute(&mut s, Command::MergeFile { path: p(&path), names: Some(vec!["Box002".into()]) }).unwrap();
    assert_eq!(r["nodes"].as_array().unwrap().len(), 1);
    let id = *s.selection.iter().next().unwrap();
    let n = s.scene.node(id).unwrap();
    assert_eq!(n.name, "Box002");
    assert!(n.parent.is_none(), "the parent was not merged");
    assert!((s.scene.world_matrix(id) - world).abs_diff_eq(glam::Mat4::ZERO, 1e-3), "world transform kept");
    assert!(n.modifiers[0].modifier.picked_node().is_none(), "a picked node outside the merge is dropped");
    assert!(execute(&mut s, Command::MergeFile { path: p(&path), names: Some(vec!["Nope".into()]) }).is_err());
}

#[test]
fn merge_is_one_undo_entry() {
    let d = dir("undo");
    let path = d.join("src.loom");
    source(&path);
    let mut s = AppState::new();
    boxed(&mut s, 0.0);
    let before = s.undo.len();
    execute(&mut s, Command::MergeFile { path: p(&path), names: None }).unwrap();
    assert_eq!(s.undo.len(), before + 1);
    assert_eq!(s.undo.labels().last().unwrap(), "Merge src.loom");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.len(), 1);
}
