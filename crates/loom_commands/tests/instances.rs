//! Instances share their object: sub-object edits on one change them all.
use loom_commands::{execute, AppState, Axis, CloneKind, Command, PrimitiveKind, SelectOp, SelectionMode};
use loom_scene::NodeId;

fn positions(s: &AppState, id: NodeId) -> Vec<glam::Vec3> {
    s.scene.node(id).unwrap().object.mesh().vertices().iter().map(|v| v.position).collect()
}

fn top_vertices(s: &AppState, id: NodeId) -> Vec<u32> {
    positions(s, id).iter().enumerate().filter(|(_, p)| p.z > 9.9).map(|(i, _)| i as u32).collect()
}

#[test]
fn vertex_edits_on_an_instance_change_every_instance() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    let a = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::ConvertToEditablePoly).unwrap();
    execute(&mut s, Command::CloneOptions { kind: CloneKind::Instance, copies: 2, name: None, offset: Some([20.0, 0.0, 0.0]), rotate_deg: None, scale: None }).unwrap();
    let copies: Vec<NodeId> = s.selection.iter().copied().collect();
    assert_eq!(copies.len(), 2);
    let original = positions(&s, a);
    // Edit the first blade's tip.
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Vertex }).unwrap();
    let top = top_vertices(&s, a);
    execute(&mut s, Command::SelectSub { node: Some(a.0), ids: top.clone(), op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::RotateSelection { axis: Axis::Z, degrees: 35.0, system: None }).unwrap();
    let edited = positions(&s, a);
    assert_ne!(edited, original, "the tip turned");
    for c in &copies {
        assert_eq!(positions(&s, *c), edited, "instance {c:?} follows the edit");
    }
    // One undo step puts all of them back.
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    for id in [a, copies[0], copies[1]] {
        assert_eq!(positions(&s, id), original);
    }
    // Editing an instance changes the source too.
    execute(&mut s, Command::SelectNodes { ids: vec![copies[1].0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(copies[1].0), ids: top, op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 4.0], system: None }).unwrap();
    let edited = positions(&s, copies[1]);
    assert_ne!(edited, original);
    assert_eq!(positions(&s, a), edited);
    assert_eq!(positions(&s, copies[0]), edited);
    // A copy keeps its own object.
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Object }).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::CloneOptions { kind: CloneKind::Copy, copies: 1, name: None, offset: Some([0.0, 20.0, 0.0]), rotate_deg: None, scale: None }).unwrap();
    let copy = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Vertex }).unwrap();
    let top = top_vertices(&s, copy);
    execute(&mut s, Command::SelectSub { node: Some(copy.0), ids: top, op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 3.0], system: None }).unwrap();
    assert_ne!(positions(&s, copy), positions(&s, a));
    assert_eq!(positions(&s, a), edited, "the instance group is untouched by the copy's edit");
}
