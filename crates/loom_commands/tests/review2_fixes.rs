//! Regressions from the phase 2 review fix pass.
use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp, SelectionMode};
use loom_scene::NodeId;

fn boxed(s: &mut AppState, pos: [f32; 3]) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: pos, size: [10.0; 3] }).unwrap();
    execute(s, Command::ConvertToEditablePoly).unwrap();
    *s.selection.iter().next().unwrap()
}

fn faces(s: &AppState, id: NodeId) -> usize {
    s.scene.node(id).unwrap().object.mesh().face_count()
}

#[test]
fn named_paste_drops_stale_ids_instead_of_panicking() {
    let mut s = AppState::new();
    let a = boxed(&mut s, [0.0; 3]);
    execute(&mut s, Command::FaceLevel).unwrap();
    execute(&mut s, Command::SelectAll).unwrap();
    execute(&mut s, Command::NamedSubSelectionCopy { name: "all".into() }).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(a.0), ids: vec![1, 2, 3, 4, 5], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::DeleteObjects).unwrap();
    // Four vertices would be left alone: the Delete prompt asks; Yes.
    execute(&mut s, Command::DeletePolygons { delete_isolated: true }).unwrap();
    assert_eq!(faces(&s, a), 1);
    execute(&mut s, Command::NamedSubSelectionPaste { name: "all".into() }).unwrap();
    assert_eq!(s.sub_selection.len(), 1, "only the surviving polygon");
    execute(&mut s, Command::Extrude { height: 1.0, width: 0.0, kind: loom_commands::ExtrudeKind::ByPolygon }).unwrap();
    assert_eq!(faces(&s, a), 5);
}

#[test]
fn closed_group_delete_clone_attach() {
    let mut s = AppState::new();
    let a = boxed(&mut s, [0.0; 3]);
    let b = boxed(&mut s, [30.0, 0.0, 0.0]);
    execute(&mut s, Command::SelectNodes { ids: vec![a.0, b.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::Group { name: None }).unwrap();
    let head = *s.selection.iter().next().unwrap();
    let count = s.scene.nodes().count();
    execute(&mut s, Command::CloneOptions { kind: loom_commands::CloneKind::Copy, copies: 1, name: None, offset: None, rotate_deg: None, scale: None }).unwrap();
    assert_eq!(s.scene.nodes().count(), count + 3, "the group clones with both members");
    let new_head = *s.selection.iter().next().unwrap();
    assert_eq!(s.scene.children(new_head).len(), 2);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.nodes().count(), count);
    // Attach refuses a group head.
    let c = boxed(&mut s, [60.0, 0.0, 0.0]);
    execute(&mut s, Command::SelectNodes { ids: vec![c.0], op: SelectOp::Replace }).unwrap();
    assert!(execute(&mut s, Command::AttachNode { id: head.0 }).is_err());
    assert!(s.scene.node(head).is_some() && s.scene.node(a).is_some());
    // Delete removes the members too.
    execute(&mut s, Command::SelectNodes { ids: vec![head.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::DeleteObjects).unwrap();
    assert!(s.scene.node(a).is_none() && s.scene.node(b).is_none() && s.scene.node(head).is_none());
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(s.scene.node(a).is_some() && s.scene.node(b).is_some() && s.scene.node(head).is_some());
    assert_eq!(s.scene.node(a).unwrap().parent, Some(head));
}

#[test]
fn detach_keeps_the_object_offset() {
    let mut s = AppState::new();
    let a = boxed(&mut s, [0.0; 3]);
    // Move the pivot only: the mesh stays where it is in the world.
    execute(&mut s, Command::AffectPivotOnly).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [30.0, 0.0, 0.0], system: None }).unwrap();
    execute(&mut s, Command::AffectPivotOnly).unwrap();
    let before = s.object_bounds(a).unwrap();
    execute(&mut s, Command::FaceLevel).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(a.0), ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::DetachSelection { name: Some("Part".into()), to_element: false, as_clone: true }).unwrap();
    let part = s.scene.nodes().find(|n| n.name == "Part").unwrap().id;
    let pb = s.object_bounds(part).unwrap();
    assert!(pb.0.x >= before.0.x - 1e-3 && pb.1.x <= before.1.x + 1e-3, "the part stays inside the source's bounds: {pb:?} vs {before:?}");
}

#[test]
fn remove_two_edge_vertex_restores_quads_and_vertex_chamfer_selects_vertices() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0; 3], size: [20.0, 10.0, 0.0] }).unwrap();
    let id = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::SetParameter { id: id.0, name: "Width Segs".into(), value: 2.0 }).unwrap();
    execute(&mut s, Command::SetParameter { id: id.0, name: "Length Segs".into(), value: 1.0 }).unwrap();
    execute(&mut s, Command::ConvertToEditablePoly).unwrap();
    assert_eq!(faces(&s, id), 2);
    let m = s.scene.node(id).unwrap().object.mesh().into_owned();
    let shared = (0..m.half_edges().len() as u32).map(loom_mesh::HalfEdgeId).find(|e| m.canonical_edge_id(*e) == Some(*e) && !loom_mesh::select_ops::is_border_edge(&m, *e)).unwrap();
    execute(&mut s, Command::InsertVertexOnEdge { node: id.0, edge: shared.0, t: 0.5 }).unwrap();
    let v = s.scene.node(id).unwrap().object.mesh().vertex_count() as u32 - 1;
    execute(&mut s, Command::VertexLevel).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![v], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::Remove).unwrap();
    assert_eq!(faces(&s, id), 2, "the two quads come back");
    assert_eq!(s.scene.node(id).unwrap().object.mesh().vertex_count(), 6);
    // Vertex chamfer at a border vertex and at a corner leaves vertices selected.
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::Chamfer { amount: 1.0, segments: 1, open: false }).unwrap();
    assert_eq!(s.selection_mode, SelectionMode::Vertex);
    assert!(s.sub_selection.iter().all(|(_, so)| matches!(so, loom_commands::SubObjectId::Vertex(_))));
    let m2 = s.scene.node(id).unwrap().object.mesh().into_owned();
    m2.check_invariants().unwrap();
    assert_eq!(m2.vertex_count(), 7, "a corner vertex becomes two");
}

#[test]
fn connect_with_many_edges_and_msmooth_on_a_selection() {
    let mut s = AppState::new();
    let a = boxed(&mut s, [0.0; 3]);
    execute(&mut s, Command::EdgeLevel).unwrap();
    let m = s.scene.node(a).unwrap().object.mesh().into_owned();
    let top: Vec<u32> = (0..m.half_edges().len() as u32).map(loom_mesh::HalfEdgeId).filter(|e| m.canonical_edge_id(*e) == Some(*e)).filter(|e| {
        let (x, y) = m.edge_vertices(*e).unwrap();
        m.vertex_position(x).unwrap().z > 9.0 && m.vertex_position(y).unwrap().z > 9.0
    }).map(|e| e.0).collect();
    execute(&mut s, Command::SelectSub { node: Some(a.0), ids: top, op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::ConnectEdges { segments: 1, pinch: 0.0, slide: 0.0 }).unwrap();
    assert!(faces(&s, a) >= 6 + 4, "a centre polygon and corner pieces: {}", faces(&s, a));
    s.scene.node(a).unwrap().object.mesh().check_invariants().unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(a.0), ids: vec![0], op: SelectOp::Replace }).unwrap();
    assert!(execute(&mut s, Command::ConnectEdges { segments: 1, pinch: 0.0, slide: 0.0 }).is_err(), "one edge per polygon connects nothing and says so");
    assert_eq!(faces(&s, a), 6);
    // MSmooth at polygon level touches only the selected polygon.
    execute(&mut s, Command::FaceLevel).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(a.0), ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MSmooth { smoothness: 1.0, separate_by_smoothing: false, separate_by_materials: false }).unwrap();
    assert_eq!(faces(&s, a), 5 + 4, "one quad became four, the rest stayed");
    s.scene.node(a).unwrap().object.mesh().check_invariants().unwrap();
}

#[test]
fn preview_version_survives_a_full_undo_stack() {
    let mut s = AppState::new();
    s.undo.set_limit(3);
    let _ = boxed(&mut s, [0.0; 3]);
    let v0 = s.undo.version();
    for _ in 0..5 {
        execute(&mut s, Command::MoveSelection { delta: [1.0, 0.0, 0.0], system: None }).unwrap();
    }
    assert_eq!(s.undo.len(), 3, "the stack is full");
    assert!(s.undo.version() > v0 + 4, "every commit still bumps the version");
    let v = s.undo.version();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(s.undo.version() > v);
}

#[test]
fn soft_selection_backfacing_uses_the_object_frame() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0; 3], size: [40.0, 40.0, 0.0] }).unwrap();
    let id = *s.selection.iter().next().unwrap();
    for n in ["Length Segs", "Width Segs"] {
        execute(&mut s, Command::SetParameter { id: id.0, name: n.into(), value: 4.0 }).unwrap();
    }
    execute(&mut s, Command::ConvertToEditablePoly).unwrap();
    execute(&mut s, Command::TopView).unwrap();
    execute(&mut s, Command::VertexLevel).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![12], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::UseSoftSelection).unwrap();
    execute(&mut s, Command::SoftSelectionSettings { falloff: Some(30.0), pinch: None, bubble: None, edge_distance: None, affect_backfacing: Some(false) }).unwrap();
    let facing: usize = s.soft_weights(id).iter().filter(|w| **w > 0.0).count();
    assert!(facing > 1, "a plane facing the Top view keeps its soft weights");
    // Flip the plane over: now every vertex faces away.
    execute(&mut s, Command::SetTransform { id: id.0, position: None, rotation_deg: Some([180.0, 0.0, 0.0]), scale: None }).unwrap();
    let away: usize = s.soft_weights(id).iter().filter(|w| **w > 0.0).count();
    assert_eq!(away, 1, "only the selected vertex itself keeps weight when the surface faces away");
}
