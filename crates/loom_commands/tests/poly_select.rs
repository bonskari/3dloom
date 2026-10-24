use loom_commands::{execute, AppState, Command, Keymap, PrimitiveKind, SelectOp, SelectionMode};
use loom_scene::NodeId;

fn state() -> AppState {
    AppState::new()
}

/// A 3×3-segment plane converted to an Editable Poly, selected.
fn plane(s: &mut AppState) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0; 3], size: [30.0, 30.0, 0.0] }).unwrap();
    let id = *s.selection.iter().next().unwrap();
    execute(s, Command::SetParameter { id: id.0, name: "Length Segs".into(), value: 3.0 }).unwrap();
    execute(s, Command::SetParameter { id: id.0, name: "Width Segs".into(), value: 3.0 }).unwrap();
    execute(s, Command::ConvertToEditablePoly).unwrap();
    id
}

fn sub(s: &AppState) -> Vec<u32> {
    let mut v: Vec<u32> = s.sub_selection.iter().map(|(_, id)| match id {
        loom_commands::SubObjectId::Vertex(v) => v.0,
        loom_commands::SubObjectId::Edge(e) => e.0,
        loom_commands::SubObjectId::Face(f) => f.0,
    }).collect();
    v.sort();
    v
}

#[test]
fn grow_shrink_round_trip_on_plane() {
    let mut s = state();
    let id = plane(&mut s);
    execute(&mut s, Command::FaceLevel).unwrap();
    assert_eq!(s.selection_mode, SelectionMode::Face);
    // The centre face of a 3×3 plane is face 4.
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![4], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::GrowSelection).unwrap();
    assert_eq!(s.sub_selection.len(), 9, "grow by vertex adjacency reaches every face");
    execute(&mut s, Command::ShrinkSelection).unwrap();
    assert_eq!(sub(&s), vec![4]);
    execute(&mut s, Command::ShrinkSelection).unwrap();
    assert!(s.sub_selection.is_empty());
    // Vertices: the centre vertex grows to its four edge neighbours.
    execute(&mut s, Command::VertexLevel).unwrap();
    let centre = (0..16u32).find(|v| s.scene.node(id).unwrap().object.mesh().vertex_position(loom_mesh::VertexId(*v)).unwrap().abs_diff_eq(glam::Vec3::new(5.0, 5.0, 0.0), 1e-3)).expect("interior vertex");
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![centre], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::GrowSelection).unwrap();
    assert_eq!(s.sub_selection.len(), 5);
}

#[test]
fn ring_and_loop_step_move_along_the_plane() {
    let mut s = state();
    let id = plane(&mut s);
    execute(&mut s, Command::EdgeLevel).unwrap();
    let mesh = s.scene.node(id).unwrap().object.mesh().into_owned();
    // An interior edge along X near the centre.
    let e = (0..mesh.half_edges().len() as u32)
        .map(loom_mesh::HalfEdgeId)
        .filter(|h| mesh.canonical_edge_id(*h) == Some(*h))
        .find(|h| {
            let (a, b) = mesh.edge_vertices(*h).unwrap();
            let (pa, pb) = (mesh.vertex_position(a).unwrap(), mesh.vertex_position(b).unwrap());
            (pa.y - pb.y).abs() < 1e-4 && (pa.y - 5.0).abs() < 1e-4 && pa.x.abs() < 6.0 && pb.x.abs() < 6.0
        })
        .expect("an interior X edge at y=5");
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![e.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SelectEdgeLoop).unwrap();
    assert_eq!(s.sub_selection.len(), 3, "the y=5 loop has three edges");
    execute(&mut s, Command::SelectEdgeLoopUp).unwrap();
    let ys: Vec<f32> = s.sub_selection.iter().map(|(_, sid)| match sid { loom_commands::SubObjectId::Edge(h) => mesh.vertex_position(mesh.edge_vertices(h).unwrap().0).unwrap().y, _ => unreachable!() }).collect();
    assert_eq!(s.sub_selection.len(), 3);
    assert!(ys.iter().all(|y| (*y - 15.0).abs() < 1e-3 || (*y + 5.0).abs() < 1e-3), "the loop moved to a neighbouring loop: {ys:?}");
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![e.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SelectEdgeRing).unwrap();
    assert_eq!(s.sub_selection.len(), 4, "the ring across the plane has four edges");
    execute(&mut s, Command::SelectEdgeRingUpAdd).unwrap();
    assert!(s.sub_selection.len() > 4, "Ring Up Add keeps the old ring");
}

#[test]
fn by_vertex_and_convert_selection() {
    let mut s = state();
    let id = plane(&mut s);
    execute(&mut s, Command::VertexLevel).unwrap();
    let centre = (0..16u32).find(|v| s.scene.node(id).unwrap().object.mesh().vertex_position(loom_mesh::VertexId(*v)).unwrap().abs_diff_eq(glam::Vec3::new(5.0, 5.0, 0.0), 1e-3)).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![centre], op: SelectOp::Replace }).unwrap();
    // Ctrl+click Polygon: every face touching the vertex.
    execute(&mut s, Command::ConvertSelection { to: SelectionMode::Face, contained: false }).unwrap();
    assert_eq!(s.selection_mode, SelectionMode::Face);
    assert_eq!(s.sub_selection.len(), 4);
    // Shift+click Vertex then Shift+click Polygon: only faces whose vertices are all selected.
    execute(&mut s, Command::ConvertSelection { to: SelectionMode::Vertex, contained: false }).unwrap();
    assert_eq!(s.sub_selection.len(), 9);
    execute(&mut s, Command::ConvertSelection { to: SelectionMode::Face, contained: true }).unwrap();
    assert_eq!(s.sub_selection.len(), 4);
    execute(&mut s, Command::SelectByVertex).unwrap();
    assert!(s.select_by_vertex);
    execute(&mut s, Command::IgnoreBackfacingInSelections).unwrap();
    assert!(s.ignore_backfacing);
    execute(&mut s, Command::PreviewSelectMode).unwrap();
    assert_eq!(format!("{:?}", s.preview_selection), "SubObj");
}

#[test]
fn hide_unhide_sub_objects_affect_picks() {
    let mut s = state();
    let id = plane(&mut s);
    execute(&mut s, Command::FaceLevel).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![4], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::Hide).unwrap();
    assert!(s.scene.node(id).unwrap().object.mesh().faces()[4].hidden);
    assert!(s.sub_selection.is_empty(), "hidden sub-objects are deselected");
    execute(&mut s, Command::SelectAll).unwrap();
    assert_eq!(s.sub_selection.len(), 8, "Select All skips hidden faces");
    execute(&mut s, Command::HideUnselected).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![4], op: SelectOp::Replace }).unwrap();
    assert!(s.sub_selection.is_empty(), "a hidden face cannot be selected");
    execute(&mut s, Command::UnhideAll).unwrap();
    assert!(s.scene.node(id).unwrap().object.mesh().faces().iter().all(|f| !f.hidden));
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(s.scene.node(id).unwrap().object.mesh().faces()[4].hidden, "hide is undoable");
}

#[test]
fn editable_poly_chords_only_with_a_poly_selected() {
    let mut s = state();
    let km = Keymap::editable_poly();
    assert_eq!(km.lookup(loom_commands::parse_chord("Shift+Ctrl+E").unwrap()), Some("Connect"));
    assert_eq!(km.lookup(loom_commands::parse_chord("1").unwrap()), Some("Vertex Level"));
    // Nothing selected: Main UI only.
    assert_eq!(s.hotkey(loom_commands::parse_chord("1").unwrap()), None);
    assert_eq!(s.hotkey(loom_commands::parse_chord("Ctrl+Z").unwrap()), Some("Undo Scene Operation"));
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [1.0; 3] }).unwrap();
    assert_eq!(s.hotkey(loom_commands::parse_chord("1").unwrap()), None, "a primitive has no sub-object levels");
    execute(&mut s, Command::ConvertToEditablePoly).unwrap();
    assert_eq!(s.hotkey(loom_commands::parse_chord("1").unwrap()), Some("Vertex Level"));
    assert_eq!(s.hotkey(loom_commands::parse_chord("Ctrl+Z").unwrap()), Some("Undo Scene Operation"));
    execute(&mut s, Command::Hotkey { chord: "1".into() }).unwrap();
    assert_eq!(s.selection_mode, SelectionMode::Vertex);
    execute(&mut s, Command::Hotkey { chord: "1".into() }).unwrap();
    assert_eq!(s.selection_mode, SelectionMode::Object, "the same level key again returns to the top level");
    // Keyboard Shortcut Override off: only Main UI.
    execute(&mut s, Command::KeyboardShortcutOverrideToggle).unwrap();
    assert_eq!(s.hotkey(loom_commands::parse_chord("1").unwrap()), None);
}

#[test]
fn named_sub_selection_copy_paste() {
    let mut s = state();
    let id = plane(&mut s);
    execute(&mut s, Command::FaceLevel).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![0, 1], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::NamedSubSelectionCopy { name: "corner".into() }).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![8], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::NamedSubSelectionPaste { name: "corner".into() }).unwrap();
    assert_eq!(sub(&s), vec![0, 1]);
    assert!(execute(&mut s, Command::NamedSubSelectionPaste { name: "nope".into() }).is_err());
}

/// A sub-object level edits only the selected objects; another object's
/// vertices can't be picked until you return to the object level.
#[test]
fn sub_object_level_cannot_select_an_unselected_object() {
    let mut s = state();
    let a = plane(&mut s);
    let b = plane(&mut s);
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::VertexLevel).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(a.0), ids: vec![0], op: SelectOp::Replace }).unwrap();
    assert!(execute(&mut s, Command::SelectSub { node: Some(b.0), ids: vec![1], op: SelectOp::Add }).is_err());
    assert_eq!(s.sub_selection.iter().map(|(n, _)| n).collect::<Vec<_>>(), vec![a]);
    assert_eq!(s.selection.iter().copied().collect::<Vec<_>>(), vec![a]);
}

/// The modifier stack lists the base object's sub-object levels under it (the `+`).
#[test]
fn base_object_stack_levels() {
    let mut s = state();
    let id = plane(&mut s);
    let poly = &s.scene.node(id).unwrap().object;
    let levels = loom_commands::base_sub_levels(poly);
    assert_eq!(levels.iter().map(|(l, _)| *l).collect::<Vec<_>>(), ["Vertex", "Edge", "Border", "Polygon", "Element"]);
    assert_eq!(levels[2].1, SelectionMode::Border);

    execute(&mut s, Command::CreateShape { kind: loom_commands::ShapeKind::Circle, position: [0.0; 3], size: [5.0, 5.0, 0.0] }).unwrap();
    execute(&mut s, Command::ConvertToEditableSpline).unwrap();
    let spline = *s.selection.iter().next().unwrap();
    let levels = loom_commands::base_sub_levels(&s.scene.node(spline).unwrap().object);
    assert_eq!(levels, vec![("Vertex", SelectionMode::Vertex), ("Segment", SelectionMode::Edge), ("Spline", SelectionMode::Face)]);

    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [1.0; 3] }).unwrap();
    let boxed = *s.selection.iter().next().unwrap();
    assert!(loom_commands::base_sub_levels(&s.scene.node(boxed).unwrap().object).is_empty());
}
