use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp, SelectionMode, ShapeKind};
use loom_scene::{NodeId, ObjectKind};
use loom_shape::KnotKind;

fn square(s: &mut AppState) -> NodeId {
    execute(s, Command::CreateShape { kind: ShapeKind::Rectangle, position: [0.0; 3], size: [10.0, 10.0, 0.0] }).unwrap();
    execute(s, Command::ConvertToEditableSpline).unwrap();
    *s.selection.iter().next().unwrap()
}

fn shape(s: &AppState, id: NodeId) -> loom_shape::Shape {
    s.scene.node(id).unwrap().object.shape().unwrap()
}

fn level(s: &mut AppState, mode: SelectionMode) {
    execute(s, Command::SelectionMode { mode }).unwrap();
}

#[test]
fn spline_levels_pick_and_move_knots_segments_and_splines() {
    let mut s = AppState::new();
    let id = square(&mut s);
    assert_eq!(s.level_label(SelectionMode::Edge), "Segment");
    assert_eq!(s.level_label(SelectionMode::Face), "Spline");
    execute(&mut s, Command::SubObjectLevelCycle).unwrap();
    execute(&mut s, Command::SubObjectLevelCycle).unwrap();
    execute(&mut s, Command::SubObjectLevelCycle).unwrap();
    assert_eq!(s.selection_mode, SelectionMode::Face, "Vertex, Segment, Spline");
    execute(&mut s, Command::SubObjectLevelCycle).unwrap();
    assert_eq!(s.selection_mode, SelectionMode::Object);
    // The knots are the cage vertices: moving vertex 0 moves the knot.
    level(&mut s, SelectionMode::Vertex);
    execute(&mut s, Command::SelectSub { node: None, ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 3.0], system: None }).unwrap();
    let sh = shape(&s, id);
    assert!((sh.splines[0].knots[0].pos.z - 3.0).abs() < 1e-5, "{:?}", sh.splines[0].knots[0].pos);
    assert!(matches!(s.scene.node(id).unwrap().object, ObjectKind::EditableSpline(_)), "still a spline");
    // Segment 1 moves both its knots.
    level(&mut s, SelectionMode::Edge);
    execute(&mut s, Command::SelectSub { node: None, ids: vec![1], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [5.0, 0.0, 0.0], system: None }).unwrap();
    let sh = shape(&s, id);
    assert!((sh.splines[0].knots[1].pos.x - 10.0).abs() < 1e-5 && (sh.splines[0].knots[2].pos.x - 10.0).abs() < 1e-5);
    // Spline level moves everything; Delete at Segment level opens the loop.
    level(&mut s, SelectionMode::Face);
    execute(&mut s, Command::SelectSub { node: None, ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 1.0, 0.0], system: None }).unwrap();
    assert!(shape(&s, id).splines[0].knots.iter().all(|k| k.pos.y > -5.0 + 0.5));
    level(&mut s, SelectionMode::Edge);
    execute(&mut s, Command::SelectSub { node: None, ids: vec![3], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::DeleteObjects).unwrap();
    let sh = shape(&s, id);
    assert!(!sh.splines[0].closed && sh.splines[0].knots.len() == 4);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(shape(&s, id).splines[0].closed);
}

#[test]
fn geometry_rollout_operations() {
    let mut s = AppState::new();
    let id = square(&mut s);
    level(&mut s, SelectionMode::Edge);
    execute(&mut s, Command::SplineRefine { node: id.0, segment: 0, t: 0.5 }).unwrap();
    let sh = shape(&s, id);
    assert_eq!(sh.splines[0].knots.len(), 5);
    assert!((sh.splines[0].knots[1].pos - glam::Vec3::new(0.0, -5.0, 0.0)).length() < 1e-4, "{:?}", sh.splines[0].knots[1].pos);
    level(&mut s, SelectionMode::Vertex);
    execute(&mut s, Command::SelectSub { node: None, ids: vec![2], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SplineFillet { amount: 2.0 }).unwrap();
    assert_eq!(shape(&s, id).splines[0].knots.len(), 6);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::SelectSub { node: None, ids: vec![2], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SplineChamfer { amount: 2.0 }).unwrap();
    let sh = shape(&s, id);
    assert_eq!(sh.splines[0].knots.len(), 6);
    assert!(sh.splines[0].is_straight(2));
    execute(&mut s, Command::SetVertexType { kind: "Smooth".into() }).unwrap();
    assert!(shape(&s, id).splines[0].knots[2].kind == KnotKind::Smooth || shape(&s, id).splines[0].knots[3].kind == KnotKind::Smooth);
    // Outline adds a spline; Boolean union with a second spline merges them.
    level(&mut s, SelectionMode::Face);
    execute(&mut s, Command::SelectSub { node: None, ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SplineOutline { amount: 1.0, center: false }).unwrap();
    assert_eq!(shape(&s, id).splines.len(), 2);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::SelectSub { node: None, ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SplineMirror { axis: 0, copy: true, about_pivot: false }).unwrap();
    assert_eq!(shape(&s, id).splines.len(), 2, "Mirror with Copy adds the mirrored spline");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Object }).unwrap();
    execute(&mut s, Command::CreateShape { kind: ShapeKind::Rectangle, position: [5.0, 5.0, 0.0], size: [10.0, 10.0, 0.0] }).unwrap();
    let other = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![id.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SplineAttach { node: other.0, reorient: false }).unwrap();
    assert!(s.scene.node(other).is_none(), "attached shape is gone");
    assert_eq!(shape(&s, id).splines.len(), 2);
    level(&mut s, SelectionMode::Face);
    execute(&mut s, Command::SelectSub { node: None, ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SplineBoolean { op: "Union".into(), other: 1 }).unwrap();
    let sh = shape(&s, id);
    assert_eq!(sh.splines.len(), 1);
    assert!(sh.splines[0].knots.len() >= 8, "{}", sh.splines[0].knots.len());
    // Explode / Detach / Close / Reverse / Make First / Divide / Weld / Fuse.
    execute(&mut s, Command::SelectSub { node: None, ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SplineExplode { to_objects: false }).unwrap();
    let n = shape(&s, id).splines.len();
    assert!(n >= 8);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::SelectSub { node: None, ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SplineDetach { same_shape: false, reorient: false, copy: true }).unwrap();
    assert_eq!(s.scene.nodes().count(), 2, "Detach Copy made a new shape");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![id.0], op: SelectOp::Replace }).unwrap();
    level(&mut s, SelectionMode::Vertex);
    execute(&mut s, Command::SelectSub { node: None, ids: vec![0, 1], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SplineFuse).unwrap();
    let sh = shape(&s, id);
    assert_eq!(sh.splines[0].knots[0].pos, sh.splines[0].knots[1].pos);
    execute(&mut s, Command::SplineWeld { threshold: 0.1 }).unwrap();
    assert_eq!(shape(&s, id).splines[0].knots.len(), sh.splines[0].knots.len() - 1, "the fused pair welds into one");
}

#[test]
fn create_shape_from_edges_and_extrude_along_spline() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    execute(&mut s, Command::ConvertToEditablePoly).unwrap();
    let b = *s.selection.iter().next().unwrap();
    level(&mut s, SelectionMode::Edge);
    // The four top edges form a loop.
    let mesh = s.scene.node(b).unwrap().object.mesh().into_owned();
    let top: Vec<u32> = (0..mesh.half_edges().len() as u32)
        .map(loom_mesh::HalfEdgeId)
        .filter(|e| mesh.canonical_edge_id(*e) == Some(*e))
        .filter(|e| mesh.edge_vertices(*e).is_some_and(|(a, c)| mesh.vertex_position(a).unwrap().z > 9.9 && mesh.vertex_position(c).unwrap().z > 9.9))
        .map(|e| e.0)
        .collect();
    assert_eq!(top.len(), 4);
    execute(&mut s, Command::SelectSub { node: None, ids: top, op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::CreateShapeFromEdges { name: Some("Rim".into()), smooth: false }).unwrap();
    let rim = s.scene.nodes().find(|n| n.name == "Rim").expect("new shape");
    let sh = rim.object.shape().unwrap();
    assert_eq!(sh.splines.len(), 1);
    assert!(sh.splines[0].closed && sh.splines[0].knots.len() == 4);
    // Extrude the box top along a straight line of three knots.
    execute(&mut s, Command::LineStart { point: [0.0, 0.0, 20.0] }).unwrap();
    let line = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::LineAddKnot { node: line.0, point: [0.0, 0.0, 30.0], smooth: false }).unwrap();
    execute(&mut s, Command::LineAddKnot { node: line.0, point: [0.0, 10.0, 40.0], smooth: false }).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![b.0], op: SelectOp::Replace }).unwrap();
    level(&mut s, SelectionMode::Face);
    let topf = (0..mesh.face_count() as u32).find(|f| mesh.face_normal(loom_mesh::FaceId(*f)).z > 0.9).unwrap();
    execute(&mut s, Command::SelectSub { node: None, ids: vec![topf], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::ExtrudeAlongSpline { spline: line.0, segments: 4, taper: 0.0, taper_curve: 0.0, twist: 0.0, rotation: 0.0, align: true }).unwrap();
    let m = s.scene.node(b).unwrap().object.mesh().into_owned();
    m.check_invariants().unwrap();
    assert_eq!(m.face_count(), 6 + 4 * 4, "four segments of four sides: {}", m.face_count());
    assert!(m.aabb().1.z > 25.0, "{:?}", m.aabb());
}
