use glam::Vec3;
use loom_commands::{execute, AppState, Command, CoordSystem, PrimitiveKind, SelectOp};
use loom_scene::NodeId;

fn state() -> AppState {
    AppState::new()
}

fn plane(s: &mut AppState) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0; 3], size: [40.0, 40.0, 0.0] }).unwrap();
    let id = *s.selection.iter().next().unwrap();
    execute(s, Command::SetParameter { id: id.0, name: "Length Segs".into(), value: 4.0 }).unwrap();
    execute(s, Command::SetParameter { id: id.0, name: "Width Segs".into(), value: 4.0 }).unwrap();
    execute(s, Command::ConvertToEditablePoly).unwrap();
    id
}

fn z(s: &AppState, id: NodeId, v: u32) -> f32 {
    s.scene.node(id).unwrap().object.mesh().vertex_position(loom_mesh::VertexId(v)).unwrap().z
}

#[test]
fn soft_move_scales_neighbours_by_weight() {
    let mut s = state();
    let id = plane(&mut s);
    execute(&mut s, Command::VertexLevel).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![12], op: SelectOp::Replace }).unwrap();
    // Soft Selection off: only the selected vertex moves.
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 10.0], system: Some(CoordSystem::World) }).unwrap();
    assert_eq!(z(&s, id, 12), 10.0);
    assert_eq!(z(&s, id, 13), 0.0);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![12], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::UseSoftSelection).unwrap();
    assert!(s.scene.node(id).unwrap().object.poly().unwrap().soft.on);
    execute(&mut s, Command::SoftSelectionSettings { falloff: Some(15.0), pinch: None, bubble: None, edge_distance: None, affect_backfacing: None }).unwrap();
    let w = s.soft_weights(id);
    assert_eq!(w[12], 1.0);
    let expected = loom_mesh::soft_select::curve(10.0, 15.0, 0.0, 0.0);
    assert!((expected - 0.259_259).abs() < 1e-5);
    assert!((w[13] - expected).abs() < 1e-5, "a 10-unit neighbour at falloff 15: {}", w[13]);
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 10.0], system: Some(CoordSystem::World) }).unwrap();
    assert_eq!(z(&s, id, 12), 10.0);
    assert!((z(&s, id, 13) - 10.0 * expected).abs() < 1e-4);
    assert_eq!(z(&s, id, 0), 0.0, "the corner is past the falloff");
}

#[test]
fn lock_keeps_weights_after_reselect() {
    let mut s = state();
    let id = plane(&mut s);
    execute(&mut s, Command::VertexLevel).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![12], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::UseSoftSelection).unwrap();
    execute(&mut s, Command::SoftSelectionSettings { falloff: Some(15.0), pinch: None, bubble: None, edge_distance: None, affect_backfacing: None }).unwrap();
    execute(&mut s, Command::LockSoftSelection).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![0], op: SelectOp::Replace }).unwrap();
    let w = s.soft_weights(id);
    assert_eq!(w[12], 1.0, "locked weights ignore the new selection");
    assert!((w[13] - loom_mesh::soft_select::curve(10.0, 15.0, 0.0, 0.0)).abs() < 1e-5);
}

#[test]
fn paint_soft_selection_sets_weights() {
    let mut s = state();
    let id = plane(&mut s);
    execute(&mut s, Command::VertexLevel).unwrap();
    execute(&mut s, Command::UseSoftSelection).unwrap();
    execute(&mut s, Command::PaintSoftSelection).unwrap();
    assert_eq!(format!("{:?}", s.soft_paint), "Some(Paint)");
    execute(&mut s, Command::PaintSoftSelectionStroke { node: id.0, points: vec![[0.0, 0.0, 0.0]], value: 1.0, size: 12.0, strength: 1.0 }).unwrap();
    let w = s.soft_weights(id);
    assert_eq!(w[12], 1.0);
    assert!(w[13] > 0.0 && w[13] < 1.0, "falls off across the brush: {}", w[13]);
    assert_eq!(w[0], 0.0);
    execute(&mut s, Command::SoftSelectionRevert).unwrap();
    assert!(s.soft_weights(id).iter().all(|w| *w == 0.0));
    execute(&mut s, Command::ShadedFaceToggle).unwrap();
    assert!(s.soft_shaded_faces);
    let _ = Vec3::ZERO;
}
