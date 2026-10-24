use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp, ShapeKind};
use loom_scene::{NodeId, ObjectKind};

fn boxed(s: &mut AppState, position: [f32; 3], size: [f32; 3]) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position, size }).unwrap();
    *s.selection.iter().next().unwrap()
}

#[test]
fn boolean_subtracts_the_picked_operand_and_keeps_or_moves_it() {
    let mut s = AppState::new();
    let a = boxed(&mut s, [0.0; 3], [10.0, 10.0, 10.0]);
    let b = boxed(&mut s, [0.0, 0.0, -2.0], [4.0, 4.0, 14.0]);
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::CreateBoolean { operand_b: b.0, operation: 2, move_operand: true }).unwrap();
    let node = s.scene.node(a).unwrap();
    assert!(matches!(node.object, ObjectKind::Boolean { .. }));
    assert_eq!(node.object.base_name(), "Boolean");
    assert!(s.scene.node(b).is_none(), "Move removes operand B");
    let m = node.object.mesh().into_owned();
    m.check_invariants().unwrap();
    assert!(m.face_count() > 12, "{}", m.face_count());
    assert_eq!(m.aabb().1.z, 10.0);
    // Union / Intersection / Display switch through Set Parameter; errors are reported, not panicked.
    execute(&mut s, Command::SetParameter { id: a.0, name: "Operation".into(), value: 0.0 }).unwrap();
    assert!(s.scene.node(a).unwrap().object.mesh().aabb().1.z > 11.9, "union reaches the top of B");
    execute(&mut s, Command::SetParameter { id: a.0, name: "Operation".into(), value: 1.0 }).unwrap();
    assert!((s.scene.node(a).unwrap().object.mesh().aabb().1.x - 2.0).abs() < 1e-4, "intersection is B's column inside A");
    execute(&mut s, Command::SetParameter { id: a.0, name: "Display".into(), value: 1.0 }).unwrap();
    assert!(s.scene.node(a).unwrap().object.mesh().face_count() == 12, "operands display shows both boxes");
    assert!(s.scene.node(a).unwrap().object.compound_error().is_none());
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(matches!(s.scene.node(a).unwrap().object, ObjectKind::Box { .. }));
    assert!(s.scene.node(b).is_some(), "undo brings B back");
    // Copy keeps B.
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::CreateBoolean { operand_b: b.0, operation: 2, move_operand: false }).unwrap();
    assert!(s.scene.node(b).is_some());
    // Disjoint operands: an intersection reports instead of panicking.
    let c = boxed(&mut s, [100.0, 0.0, 0.0], [5.0; 3]);
    let d = boxed(&mut s, [200.0, 0.0, 0.0], [5.0; 3]);
    execute(&mut s, Command::SelectNodes { ids: vec![c.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::CreateBoolean { operand_b: d.0, operation: 1, move_operand: true }).unwrap();
    assert!(s.scene.node(c).unwrap().object.compound_error().is_some());
}

#[test]
fn proboolean_chains_operands() {
    let mut s = AppState::new();
    let a = boxed(&mut s, [0.0; 3], [10.0, 10.0, 10.0]);
    let b = boxed(&mut s, [3.0, 0.0, 0.0], [10.0, 6.0, 6.0]);
    let c = boxed(&mut s, [0.0, 0.0, 5.0], [3.0, 3.0, 20.0]);
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::CreateProBoolean).unwrap();
    assert!(matches!(s.scene.node(a).unwrap().object, ObjectKind::ProBoolean { .. }));
    execute(&mut s, Command::ProBooleanAddOperand { node: b.0, operation: 0, move_operand: true }).unwrap();
    assert!((s.scene.node(a).unwrap().object.mesh().aabb().1.x - 8.0).abs() < 1e-4, "union with B");
    execute(&mut s, Command::ProBooleanAddOperand { node: c.0, operation: 2, move_operand: true }).unwrap();
    let m = s.scene.node(a).unwrap().object.mesh().into_owned();
    m.check_invariants().unwrap();
    assert!(s.scene.node(b).is_none() && s.scene.node(c).is_none());
    assert!(m.face_count() > 12);
    let ObjectKind::ProBoolean { operands, .. } = &s.scene.node(a).unwrap().object else { panic!() };
    assert_eq!(operands.len(), 2);
}

#[test]
fn loft_skins_shapes_along_a_path() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreateShape { kind: ShapeKind::Circle, position: [0.0; 3], size: [10.0, 10.0, 0.0] }).unwrap();
    let circle = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::LineStart { point: [0.0, 0.0, 0.0] }).unwrap();
    let path = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::LineAddKnot { node: path.0, point: [0.0, 0.0, 30.0], smooth: false }).unwrap();
    execute(&mut s, Command::CreateShape { kind: ShapeKind::Rectangle, position: [0.0; 3], size: [8.0, 8.0, 0.0] }).unwrap();
    let rect = *s.selection.iter().next().unwrap();
    // Get Path: the selected circle becomes the loft along the line.
    execute(&mut s, Command::SelectNodes { ids: vec![circle.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::CreateLoft { path: path.0 }).unwrap();
    let node = s.scene.node(circle).unwrap();
    assert!(matches!(node.object, ObjectKind::Loft { .. }));
    let m = node.object.mesh().into_owned();
    m.check_invariants().unwrap();
    assert!(m.face_count() > 20);
    assert!((m.aabb().1.z - 30.0).abs() < 1e-3, "{:?}", m.aabb());
    execute(&mut s, Command::LoftAddShape { shape: rect.0, percent: 100.0 }).unwrap();
    let ObjectKind::Loft { shapes, .. } = &s.scene.node(circle).unwrap().object else { panic!() };
    assert_eq!(shapes.len(), 2);
    let m2 = s.scene.node(circle).unwrap().object.mesh().into_owned();
    m2.check_invariants().unwrap();
    execute(&mut s, Command::SetParameter { id: circle.0, name: "Path Steps".into(), value: 2.0 }).unwrap();
    execute(&mut s, Command::SetParameter { id: circle.0, name: "Cap End".into(), value: 0.0 }).unwrap();
    assert!(!loom_mesh::select_ops::border_edges(&s.scene.node(circle).unwrap().object.mesh()).is_empty(), "no end cap");
}

/// The reference starts the Boolean rollout on Subtraction (A-B): picking B cuts it out of A.
#[test]
fn boolean_pick_defaults_to_subtraction_a_minus_b() {
    let mut s = AppState::new();
    assert_eq!(s.compound_settings.boolean_op, 2, "Subtraction (A-B)");
    let a = boxed(&mut s, [0.0; 3], [10.0, 10.0, 10.0]);
    let b = boxed(&mut s, [0.0, 0.0, -2.0], [4.0, 4.0, 14.0]);
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::PickOperandB).unwrap();
    execute(&mut s, Command::PickHit { id: b.0 }).unwrap();
    let ObjectKind::Boolean { op, .. } = &s.scene.node(a).unwrap().object else { panic!("a Boolean") };
    assert_eq!(op.get(), 2);
    assert_eq!(s.scene.node(a).unwrap().object.mesh().aabb().1.z, 10.0, "B's column is cut out, not added");
}
