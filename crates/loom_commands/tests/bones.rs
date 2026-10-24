//! Bone chains, Bone Edit Mode, Refine, Mirror, Reassign Root, Bone Tools.

use loom_commands::{execute, AppState, Command, SelectOp};
use loom_scene::{NodeId, ObjectKind};

fn chain(s: &mut AppState) -> Vec<NodeId> {
    let r = execute(s, Command::CreateBoneChain { points: vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [10.0, 10.0, 0.0]], name: None }).unwrap();
    serde_json::from_value::<Vec<u64>>(r["nodes"].clone()).unwrap().into_iter().map(NodeId).collect()
}

#[test]
fn bone_chain_has_lengths_parents_and_a_nub() {
    let mut s = AppState::new();
    let ids = chain(&mut s);
    assert_eq!(ids.len(), 3, "two bones and a nub");
    let b0 = s.scene.node(ids[0]).unwrap();
    let b1 = s.scene.node(ids[1]).unwrap();
    let nub = s.scene.node(ids[2]).unwrap();
    assert_eq!(b0.name, "Bone001");
    assert_eq!(b1.parent, Some(ids[0]));
    assert_eq!(nub.parent, Some(ids[1]));
    let ObjectKind::Bone { length, .. } = &b0.object else { panic!() };
    assert!((length.get() - 10.0).abs() < 1e-4);
    // The second bone's base sits at the first bone's tip in world space.
    let w = s.scene.world_matrix(ids[1]);
    assert!((w.transform_point3(glam::Vec3::ZERO) - glam::Vec3::new(10.0, 0.0, 0.0)).length() < 1e-3);
    // Its local +X points at (10, 10, 0).
    let tip = w.transform_point3(glam::Vec3::X * 10.0);
    assert!((tip - glam::Vec3::new(10.0, 10.0, 0.0)).length() < 1e-3, "{tip:?}");
    assert_eq!(s.selection.len(), 3);
    assert!(nub.object.is_bone());
}

#[test]
fn bone_edit_mode_stretches_the_parent_and_refine_splits() {
    let mut s = AppState::new();
    let ids = chain(&mut s);
    execute(&mut s, Command::BoneEditModeToggle).unwrap();
    assert!(s.bone_edit_mode);
    execute(&mut s, Command::SelectNodes { ids: vec![ids[1].0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [5.0, 0.0, 0.0], system: None }).unwrap();
    let ObjectKind::Bone { length, .. } = &s.scene.node(ids[0]).unwrap().object else { panic!() };
    assert!((length.get() - 15.0).abs() < 1e-3, "parent stretched: {}", length.get());
    execute(&mut s, Command::BoneEditModeToggle).unwrap();
    let r = execute(&mut s, Command::RefineBone { id: ids[0].0, t: 0.5 }).unwrap();
    let new_id = NodeId(r["node"].as_u64().unwrap());
    let ObjectKind::Bone { length, .. } = &s.scene.node(ids[0]).unwrap().object else { panic!() };
    assert!((length.get() - 7.5).abs() < 1e-3, "{}", length.get());
    assert_eq!(s.scene.node(new_id).unwrap().parent, Some(ids[0]));
    assert_eq!(s.scene.node(ids[1]).unwrap().parent, Some(new_id));
    let ObjectKind::Bone { length, .. } = &s.scene.node(new_id).unwrap().object else { panic!() };
    assert!((length.get() - 7.5).abs() < 1e-3);
    // The child's world position is unchanged by the split.
    let base = s.scene.world_matrix(ids[1]).transform_point3(glam::Vec3::ZERO);
    assert!((base - glam::Vec3::new(15.0, 0.0, 0.0)).length() < 1e-3, "{base:?}");
}

#[test]
fn mirror_and_reassign_root() {
    let mut s = AppState::new();
    let ids = chain(&mut s);
    execute(&mut s, Command::SelectNodes { ids: vec![ids[0].0], op: SelectOp::Replace }).unwrap();
    let r = execute(&mut s, Command::MirrorBones { axis: 1, offset: 0.0, flip: 0 }).unwrap();
    let copies: Vec<u64> = serde_json::from_value(r["nodes"].clone()).unwrap();
    assert_eq!(copies.len(), 3);
    let tip = s.scene.world_matrix(NodeId(copies[1])).transform_point3(glam::Vec3::X * 10.0);
    assert!((tip - glam::Vec3::new(10.0, -10.0, 0.0)).length() < 1e-3, "{tip:?}");
    execute(&mut s, Command::ReassignRoot { id: ids[1].0 }).unwrap();
    assert_eq!(s.scene.node(ids[1]).unwrap().parent, None);
    assert_eq!(s.scene.node(ids[0]).unwrap().parent, Some(ids[1]));
    let base = s.scene.world_matrix(ids[0]).transform_point3(glam::Vec3::ZERO);
    assert!((base - glam::Vec3::new(10.0, 0.0, 0.0)).length() < 1e-3, "the old root now starts at the old joint: {base:?}");
    let tip0 = s.scene.world_matrix(ids[0]).transform_point3(glam::Vec3::X * 10.0);
    assert!((tip0 - glam::Vec3::ZERO).length() < 1e-3, "and points back to the old base: {tip0:?}");
    execute(&mut s, Command::BoneTools).unwrap();
    assert_eq!(s.dialog, Some(loom_commands::Dialog::BoneTools));
    // Helpers through the API.
    let r = execute(&mut s, Command::CreateHelper { kind: "dummy".into(), position: [1.0, 2.0, 3.0], size: 10.0 }).unwrap();
    let d = NodeId(r["node"].as_u64().unwrap());
    assert_eq!(s.scene.node(d).unwrap().name, "Dummy001");
    assert!(s.scene.node(d).unwrap().object.is_helper());
}
