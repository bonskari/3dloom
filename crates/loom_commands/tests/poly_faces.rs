use loom_commands::{execute, AppState, Axis, Command, ExtrudeKind, PickMode, PrimitiveKind, SelectOp, SelectionMode};
use loom_scene::NodeId;

fn poly_box(s: &mut AppState, segs: f64) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    let id = *s.selection.iter().next().unwrap();
    for n in ["Length Segs", "Width Segs"] {
        execute(s, Command::SetParameter { id: id.0, name: n.into(), value: segs }).unwrap();
    }
    execute(s, Command::ConvertToEditablePoly).unwrap();
    id
}

fn faces(s: &AppState, id: NodeId) -> usize {
    s.scene.node(id).unwrap().object.mesh().face_count()
}

fn top_faces(s: &AppState, id: NodeId) -> Vec<u32> {
    let m = s.scene.node(id).unwrap().object.mesh().into_owned();
    (0..m.face_count() as u32).filter(|f| m.face_normal(loom_mesh::FaceId(*f)).z > 0.9).collect()
}

#[test]
fn extrude_kinds_through_commands() {
    let mut s = AppState::new();
    let id = poly_box(&mut s, 2.0);
    execute(&mut s, Command::FaceLevel).unwrap();
    let top = top_faces(&s, id);
    assert_eq!(top.len(), 4);
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: top.clone(), op: SelectOp::Replace }).unwrap();
    let before = faces(&s, id);
    execute(&mut s, Command::Extrude { height: 2.0, width: 0.0, kind: ExtrudeKind::Group }).unwrap();
    assert_eq!(faces(&s, id), before + 8);
    assert_eq!(s.sub_selection.len(), 4, "the caps stay selected");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: top, op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::Extrude { height: 2.0, width: 0.0, kind: ExtrudeKind::ByPolygon }).unwrap();
    assert_eq!(faces(&s, id), before + 16);
    assert_eq!(s.poly_settings.extrude_kind, ExtrudeKind::ByPolygon);
}

#[test]
fn inset_bevel_outline_and_flip() {
    let mut s = AppState::new();
    let id = poly_box(&mut s, 1.0);
    execute(&mut s, Command::FaceLevel).unwrap();
    let top = top_faces(&s, id);
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: top.clone(), op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::Inset { amount: 1.0, by_polygon: false }).unwrap();
    assert_eq!(faces(&s, id), 10);
    assert_eq!(s.sub_selection.len(), 1);
    execute(&mut s, Command::Bevel { height: 1.0, outline: -0.5, kind: ExtrudeKind::Group }).unwrap();
    assert_eq!(faces(&s, id), 14);
    execute(&mut s, Command::Outline { amount: 0.5 }).unwrap();
    assert_eq!(faces(&s, id), 14);
    assert_eq!(s.poly_settings.outline_amount, 0.5);
    assert!(execute(&mut s, Command::FlipNormals).is_err(), "one polygon of an element cannot flip");
    execute(&mut s, Command::SelectAll).unwrap();
    execute(&mut s, Command::FlipNormals).unwrap();
    assert_eq!(faces(&s, id), 14);
    // Extrude Along Spline arms its pick mode (the spline is clicked next).
    execute(&mut s, Command::ExtrudeAlongSplineMode).unwrap();
    assert_eq!(s.pick_mode, Some(loom_commands::PickMode::ExtrudeAlongSpline));
}

#[test]
fn hinge_from_picked_edge() {
    let mut s = AppState::new();
    let id = poly_box(&mut s, 1.0);
    execute(&mut s, Command::FaceLevel).unwrap();
    let top = top_faces(&s, id);
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: top.clone(), op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::PickHinge).unwrap();
    assert_eq!(s.pick_mode, Some(PickMode::HingeEdge));
    let m = s.scene.node(id).unwrap().object.mesh().into_owned();
    let edge = m.faces()[top[0] as usize].he.0;
    execute(&mut s, Command::PickSub { node: id.0, id: edge }).unwrap();
    assert_eq!(s.poly_settings.hinge_edge, Some((id, edge)));
    assert_eq!(s.pick_mode, None);
    execute(&mut s, Command::HingeFromEdge { angle: 45.0, segments: 2, edge: None }).unwrap();
    assert_eq!(faces(&s, id), 6 + 3 * 2);
    assert_eq!(s.selection_mode, SelectionMode::Face);
}

#[test]
fn make_planar_and_grid_align() {
    let mut s = AppState::new();
    let id = poly_box(&mut s, 1.0);
    execute(&mut s, Command::VertexLevel).unwrap();
    execute(&mut s, Command::SelectAll).unwrap();
    execute(&mut s, Command::MakePlanarAxis { axis: Axis::Z }).unwrap();
    let m = s.scene.node(id).unwrap().object.mesh().into_owned();
    assert!(m.vertices().iter().all(|v| (v.position.z - 5.0).abs() < 1e-4));
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::SetTransform { id: id.0, position: Some([0.0, 0.0, 3.0]), rotation_deg: None, scale: None }).unwrap();
    execute(&mut s, Command::SelectAll).unwrap();
    execute(&mut s, Command::AlignToGrid).unwrap();
    let m = s.scene.node(id).unwrap().object.mesh().into_owned();
    // The home grid is world z = 0: local z = -3 for every vertex.
    assert!(m.vertices().iter().all(|v| (v.position.z + 3.0).abs() < 1e-4), "{:?}", m.vertices().iter().map(|v| v.position.z).collect::<Vec<_>>());
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::FaceLevel).unwrap();
    let top = top_faces(&s, id);
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: top, op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MakePlanar).unwrap();
    assert!(execute(&mut s, Command::ViewAlign).is_ok());
}
