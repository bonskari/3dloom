//! IK chains through commands: goal follows, FK snap, toggle, persistence.

use loom_commands::{execute, AppState, Command, SelectOp};
use loom_scene::NodeId;

#[test]
fn hi_chain_moves_bones_when_the_goal_moves_and_fk_snap_bakes_it() {
    let mut s = AppState::new();
    let r = execute(&mut s, Command::CreateBoneChain { points: vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [20.0, 0.0, 0.0], [21.0, 0.0, 0.0]], name: None }).unwrap();
    let ids: Vec<u64> = serde_json::from_value(r["nodes"].clone()).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![ids[0], ids[2]], op: SelectOp::Replace }).unwrap();
    let r = execute(&mut s, Command::HistoryIndependentIkSolver).unwrap();
    let goal = NodeId(r["goal"].as_u64().unwrap());
    assert_eq!(s.scene.node(goal).unwrap().name, "IK Chain001");
    assert_eq!(s.selection.iter().copied().collect::<Vec<_>>(), vec![goal]);
    assert_eq!(s.scene.ik_chains.len(), 1);
    // The goal starts at the end bone's base (at the chain's end joint).
    let gp = s.scene.world_matrix(goal).transform_point3(glam::Vec3::ZERO);
    assert!((gp - glam::Vec3::new(20.0, 0.0, 0.0)).length() < 1e-3, "{gp:?}");
    execute(&mut s, Command::MoveSelection { delta: [-5.0, 10.0, 0.0], system: None }).unwrap();
    let tip = s.scene.world_matrix(NodeId(ids[2])).transform_point3(glam::Vec3::ZERO);
    assert!((tip - glam::Vec3::new(15.0, 10.0, 0.0)).length() < 0.2, "the end bone follows the goal: {tip:?}");
    // FK snap writes the solved rotations into the bones, so disabling IK keeps the pose.
    execute(&mut s, Command::IkChainFkSnap).unwrap();
    execute(&mut s, Command::IkSolverToggle).unwrap();
    assert!(!s.scene.ik_chains[0].enabled);
    let tip2 = s.scene.world_matrix(NodeId(ids[2])).transform_point3(glam::Vec3::ZERO);
    assert!((tip2 - tip).length() < 0.2, "{tip2:?}");
    let saved = serde_json::to_string(&s.scene).unwrap();
    let back: loom_scene::Scene = serde_json::from_str(&saved).unwrap();
    assert_eq!(back.ik_chains.len(), 1);
    // Undo removes the toggle, the snap and finally the chain.
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(s.scene.ik_chains[0].enabled);
    let r = execute(&mut s, Command::State).unwrap();
    assert_eq!(r["ik_chains"].as_array().unwrap().len(), 1);
    for name in ["History-Independent IK Solver", "IK Limb Solver", "Spline IK Solver", "IK Solver Toggle", "IK Chain FK Snap", "IK Chain IK Snap", "IK Chain Snap Action", "IK Terminator Toggle", "Bones IK Chain"] {
        assert!(loom_commands::registry().iter().any(|i| i.name == name), "{name} missing");
    }
}

#[test]
fn limb_and_spline_chains_through_commands() {
    let mut s = AppState::new();
    let r = execute(&mut s, Command::CreateBoneChain { points: vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [20.0, 0.0, 0.0], [21.0, 0.0, 0.0]], name: None }).unwrap();
    let ids: Vec<u64> = serde_json::from_value(r["nodes"].clone()).unwrap();
    // The end bone marks the end joint: a two-bone limb is start, middle, end.
    execute(&mut s, Command::SelectNodes { ids: vec![ids[0], ids[2]], op: SelectOp::Replace }).unwrap();
    let r = execute(&mut s, Command::IkLimbSolver).unwrap();
    let goal = NodeId(r["goal"].as_u64().unwrap());
    execute(&mut s, Command::SetIkChainParameter { chain: 1, name: "Swivel Angle".into(), value: 45.0 }).unwrap();
    assert!((s.scene.ik_chains[0].swivel_deg - 45.0).abs() < 1e-5);
    execute(&mut s, Command::SelectNodes { ids: vec![goal.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [-10.0, 10.0, 0.0], system: None }).unwrap();
    let tip = s.scene.world_matrix(NodeId(ids[2])).transform_point3(glam::Vec3::ZERO);
    assert!((tip - glam::Vec3::new(10.0, 10.0, 0.0)).length() < 1e-2, "limb end joint at the goal: {tip:?}");
    let elbow = s.scene.world_matrix(NodeId(ids[1])).transform_point3(glam::Vec3::ZERO);
    assert!(elbow.z.abs() > 1.0, "swivel 45 lifts the elbow out of the XY plane: {elbow:?}");
    // Spline IK on a fresh chain following a line shape.
    execute(&mut s, Command::CreateShape { kind: loom_commands::ShapeKind::Rectangle, position: [0.0, 0.0, 0.0], size: [30.0, 30.0, 0.0] }).unwrap();
    let shape = *s.selection.iter().next().unwrap();
    let r = execute(&mut s, Command::CreateBoneChain { points: vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [20.0, 0.0, 0.0], [21.0, 0.0, 0.0]], name: Some("Tail".into()) }).unwrap();
    let tail: Vec<u64> = serde_json::from_value(r["nodes"].clone()).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![tail[0], tail[2]], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SplineIkSolver).unwrap();
    execute(&mut s, Command::PickSplineIkShape { chain: 2, node: shape.0 }).unwrap();
    assert_eq!(s.scene.ik_chains[1].spline, Some(shape));
    // The bones now sit on the rectangle (its first corner is at (-15, -15)).
    let base = s.scene.world_matrix(NodeId(tail[0])).transform_point3(glam::Vec3::ZERO);
    assert!(base.x.abs() > 5.0 || base.y.abs() > 5.0, "bones moved onto the shape: {base:?}");
}
