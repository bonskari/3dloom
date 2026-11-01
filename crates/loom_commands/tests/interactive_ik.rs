//! Interactive IK and Applied IK (Hierarchy > IK > Inverse Kinematics) and the
//! Inverse Kinematics preferences.

use loom_commands::{execute, AppState, Command, SelectOp};
use loom_scene::NodeId;

fn arm(s: &mut AppState) -> Vec<u64> {
    let r = execute(s, Command::CreateBoneChain { points: vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [20.0, 0.0, 0.0], [21.0, 0.0, 0.0]], name: None }).unwrap();
    serde_json::from_value(r["nodes"].clone()).unwrap()
}

fn world(s: &AppState, id: u64) -> glam::Vec3 {
    s.scene.world_matrix(NodeId(id)).transform_point3(glam::Vec3::ZERO)
}

#[test]
fn interactive_ik_move_turns_the_ancestors_and_the_root_stays() {
    let mut s = AppState::new();
    let ids = arm(&mut s);
    execute(&mut s, Command::SelectNodes { ids: vec![ids[2]], op: SelectOp::Replace }).unwrap();
    let r = execute(&mut s, Command::InteractiveIkToggle).unwrap();
    assert_eq!(r["interactive_ik"], true);
    let local = s.scene.node(NodeId(ids[2])).unwrap().transform.position.get();
    execute(&mut s, Command::MoveSelection { delta: [-5.0, 10.0, 0.0], system: None }).unwrap();
    let tip = world(&s, ids[2]);
    assert!((tip - glam::Vec3::new(15.0, 10.0, 0.0)).length() < 1.0, "the moved bone reaches the cursor: {tip:?}");
    assert!(world(&s, ids[0]).length() < 1e-4, "the root does not move");
    assert_eq!(s.scene.node(NodeId(ids[2])).unwrap().transform.position.get(), local, "the moved object keeps its link offset");
    // A Terminator stops the chain: only the bone below it turns.
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::SetIkObjectParameters { id: ids[1], terminator: Some(true), follow: None, unbind: None }).unwrap();
    let r0 = s.scene.node(NodeId(ids[0])).unwrap().transform.rotation.get();
    execute(&mut s, Command::SelectNodes { ids: vec![ids[2]], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [-2.0, 5.0, 0.0], system: None }).unwrap();
    assert_eq!(s.scene.node(NodeId(ids[0])).unwrap().transform.rotation.get(), r0, "the bone above the terminator keeps its rotation");
    assert!(world(&s, ids[2]).y > 3.0, "{:?}", world(&s, ids[2]));
}

#[test]
fn interactive_ik_moves_an_unlinked_object_only_with_always_transform_children_of_the_world() {
    let mut s = AppState::new();
    let ids = arm(&mut s);
    execute(&mut s, Command::InteractiveIkToggle).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![ids[0]], op: SelectOp::Replace }).unwrap();
    assert_eq!(s.prefs.get("Always Transform Children of the World"), Some(1.0));
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 5.0], system: None }).unwrap();
    assert!((world(&s, ids[0]).z - 5.0).abs() < 1e-4);
    execute(&mut s, Command::SetPreference { name: "Always Transform Children of the World".into(), value: 0.0 }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 5.0], system: None }).unwrap();
    assert!((world(&s, ids[0]).z - 5.0).abs() < 1e-4, "the root stays put");
    for (name, v) in [("Interactive IK Threshold", 0.5), ("Interactive IK Iterations", 20.0), ("Applied IK Threshold", 0.25), ("Applied IK Iterations", 100.0)] {
        execute(&mut s, Command::SetPreference { name: name.into(), value: v }).unwrap();
        assert_eq!(s.prefs.get(name), Some(v));
    }
}

#[test]
fn apply_ik_keys_the_chain_onto_a_bound_follow_object() {
    let mut s = AppState::new();
    let ids = arm(&mut s);
    execute(&mut s, Command::CreateHelper { kind: "dummy".into(), position: [20.0, 0.0, 0.0], size: 1.0 }).unwrap();
    let dummy = s.selection.iter().next().unwrap().0;
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::SetFrame { frame: 10 }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [-6.0, 8.0, 0.0], system: None }).unwrap();
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::SetFrame { frame: 0 }).unwrap();
    execute(&mut s, Command::SetIkObjectParameters { id: ids[2], terminator: None, follow: Some(dummy), unbind: None }).unwrap();
    assert_eq!(s.scene.node(NodeId(ids[2])).unwrap().joints.follow, Some(NodeId(dummy)));
    execute(&mut s, Command::SelectNodes { ids: vec![ids[0]], op: SelectOp::Replace }).unwrap();
    let r = execute(&mut s, Command::ApplyIk { start: Some(0), end: Some(10), only_keys: Some(false), clear_keys: Some(true) }).unwrap();
    assert_eq!(r["frames"], 11);
    for f in [0, 5, 10] {
        execute(&mut s, Command::SetFrame { frame: f }).unwrap();
        let d = world(&s, ids[2]).distance(world(&s, dummy));
        assert!(d < 0.2, "frame {f}: the end effector is on the follow object ({d})");
    }
    // Apply Only to Keys: keys only where the follow object has keys (0 and 10).
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::ApplyIk { start: None, end: None, only_keys: Some(true), clear_keys: Some(true) }).unwrap();
    for track in &s.scene.node(NodeId(ids[1])).unwrap().transform.rotation.keys().unwrap().tracks {
        assert_eq!(track.keys.iter().map(|k| k.time).collect::<Vec<_>>(), vec![0.0, 10.0]);
    }
    // Unbind: nothing left to apply.
    execute(&mut s, Command::SetIkObjectParameters { id: ids[2], terminator: None, follow: None, unbind: Some(true) }).unwrap();
    assert!(execute(&mut s, Command::ApplyIk { start: None, end: None, only_keys: None, clear_keys: None }).is_err());
}
