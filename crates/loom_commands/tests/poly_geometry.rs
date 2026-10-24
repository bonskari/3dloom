use loom_commands::{execute, AppState, Command, PickMode, PolyConstraint, PrimitiveKind, SelectOp, SelectionMode, TessKind};
use loom_scene::NodeId;

fn poly_box(s: &mut AppState, pos: [f32; 3]) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: pos, size: [10.0; 3] }).unwrap();
    execute(s, Command::ConvertToEditablePoly).unwrap();
    *s.selection.iter().next().unwrap()
}

fn faces(s: &AppState, id: NodeId) -> usize {
    s.scene.node(id).unwrap().object.mesh().face_count()
}

#[test]
fn attach_and_detach_round_trip() {
    let mut s = AppState::new();
    let a = poly_box(&mut s, [0.0; 3]);
    let b = poly_box(&mut s, [30.0, 0.0, 0.0]);
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::Attach).unwrap();
    assert_eq!(s.pick_mode, Some(PickMode::AttachObject));
    execute(&mut s, Command::PickHit { id: b.0 }).unwrap();
    assert!(s.scene.node(b).is_none(), "the attached node is gone");
    assert_eq!(faces(&s, a), 12);
    let m = s.scene.node(a).unwrap().object.mesh().into_owned();
    let far = m.vertices().iter().filter(|v| v.position.x > 20.0).count();
    assert_eq!(far, 8, "attached vertices keep their world position");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(s.scene.node(b).is_some());
    assert_eq!(faces(&s, a), 6);
    // Detach as clone keeps the source.
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::FaceLevel).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(a.0), ids: vec![0], op: SelectOp::Replace }).unwrap();
    let count = s.scene.nodes().count();
    execute(&mut s, Command::DetachSelection { name: Some("Lid".into()), to_element: false, as_clone: true }).unwrap();
    assert_eq!(faces(&s, a), 6);
    assert_eq!(s.scene.nodes().count(), count + 1);
    assert!(s.scene.nodes().any(|n| n.name == "Lid"));
    execute(&mut s, Command::SelectSub { node: Some(a.0), ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::DetachSelection { name: None, to_element: true, as_clone: false }).unwrap();
    assert_eq!(faces(&s, a), 6);
    assert_eq!(s.scene.node(a).unwrap().object.mesh().vertex_count(), 12);
}

#[test]
fn slice_plane_moves_with_transforms_and_slices() {
    let mut s = AppState::new();
    let a = poly_box(&mut s, [0.0; 3]);
    execute(&mut s, Command::EdgeLevel).unwrap();
    execute(&mut s, Command::SlicePlaneMode).unwrap();
    assert!(s.slice_plane_mode);
    assert!((s.slice_plane.origin.z - 5.0).abs() < 1e-4, "{:?}", s.slice_plane.origin);
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 2.0], system: None }).unwrap();
    assert!((s.slice_plane.origin.z - 7.0).abs() < 1e-4);
    assert_eq!(faces(&s, a), 6, "moving the plane does not move the mesh");
    execute(&mut s, Command::Slice).unwrap();
    assert_eq!(faces(&s, a), 10);
    assert_eq!(s.sub_selection.len(), 4, "the new loop is selected at edge level");
    execute(&mut s, Command::SlicePlaneMode).unwrap();
    assert!(!s.slice_plane_mode);
    // QuickSlice through two points in the Top view (looking down -Z).
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::TopView).unwrap();
    execute(&mut s, Command::QuickSlice { a: [-20.0, 2.0, 0.0], b: [20.0, 2.0, 0.0] }).unwrap();
    assert_eq!(faces(&s, a), 10);
    assert!(execute(&mut s, Command::Cut).is_ok());
    assert_eq!(s.pick_mode, Some(PickMode::CutA));
}

#[test]
fn msmooth_tessellate_relax() {
    let mut s = AppState::new();
    let a = poly_box(&mut s, [0.0; 3]);
    execute(&mut s, Command::MSmooth { smoothness: 1.0, separate_by_smoothing: false, separate_by_materials: false }).unwrap();
    assert_eq!(faces(&s, a), 24);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::FaceLevel).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(a.0), ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::TessellateFaces { kind: TessKind::Face, tension: 0.0 }).unwrap();
    assert_eq!(faces(&s, a), 9);
    assert_eq!(s.poly_settings.tess_kind, TessKind::Face);
    execute(&mut s, Command::VertexLevel).unwrap();
    execute(&mut s, Command::SelectAll).unwrap();
    assert!(execute(&mut s, Command::Relax { amount: 0.5, iterations: 2, hold_boundary: true, hold_outer: false }).is_ok());
    assert_eq!(s.poly_settings.relax_iterations, 2);
}

#[test]
fn constrain_to_edges_keeps_vertex_on_edge() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0; 3], size: [20.0, 20.0, 0.0] }).unwrap();
    let id = *s.selection.iter().next().unwrap();
    for n in ["Length Segs", "Width Segs"] {
        execute(&mut s, Command::SetParameter { id: id.0, name: n.into(), value: 2.0 }).unwrap();
    }
    execute(&mut s, Command::ConvertToEditablePoly).unwrap();
    execute(&mut s, Command::VertexLevel).unwrap();
    let m = s.scene.node(id).unwrap().object.mesh().into_owned();
    let centre = (0..9u32).find(|v| m.vertex_position(loom_mesh::VertexId(*v)).unwrap().length() < 1e-3).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![centre], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::ConstrainToEdges).unwrap();
    assert_eq!(s.poly_constraint, PolyConstraint::Edges);
    execute(&mut s, Command::MoveSelection { delta: [3.0, 1.0, 2.0], system: None }).unwrap();
    let p = s.scene.node(id).unwrap().object.mesh().vertex_position(loom_mesh::VertexId(centre)).unwrap();
    assert!((p - glam::Vec3::new(3.0, 0.0, 0.0)).length() < 1e-3, "{p:?}");
    execute(&mut s, Command::ConstrainToEdges).unwrap();
    assert_eq!(s.poly_constraint, PolyConstraint::None, "pressing Edges again turns it off");
    assert_eq!(s.selection_mode, SelectionMode::Vertex);
}

#[test]
fn collapse_at_polygon_level() {
    let mut s = AppState::new();
    let a = poly_box(&mut s, [0.0; 3]);
    execute(&mut s, Command::FaceLevel).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(a.0), ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::Collapse).unwrap();
    assert_eq!(faces(&s, a), 5, "the polygon collapses to a point: four sides become triangles");
    assert_eq!(s.scene.node(a).unwrap().object.mesh().vertex_count(), 5);
}
