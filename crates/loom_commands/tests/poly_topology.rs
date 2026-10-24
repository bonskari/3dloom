//! Editable Poly topology tools as the reference's tutorials use them: Bridge
//! of edges on one open border, Use Specific Edges, Cut that chains click to
//! click, Clean Remove, Swift Loop, Loop Mode, Select By Half, the delete
//! prompt for isolated vertices and unit strings in settings spinners.

use glam::Vec3;
use loom_commands::{execute, AppState, Axis, Command, PickMode, PrimitiveKind, SelectOp, SubObjectId, UnitsSetup};
use loom_mesh::{FaceId, HalfEdgeId, Mesh, VertexId};
use loom_scene::NodeId;

fn mesh(s: &AppState, id: NodeId) -> Mesh {
    s.scene.node(id).unwrap().object.mesh().into_owned()
}

/// A `size` x `size` plane with `segs` x `segs` cells at the origin, as an Editable Poly.
fn poly_plane(s: &mut AppState, size: f32, segs: u32) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0; 3], size: [size, size, 0.0] }).unwrap();
    let id = *s.selection.iter().next().unwrap();
    for n in ["Length Segs", "Width Segs"] {
        execute(s, Command::SetParameter { id: id.0, name: n.into(), value: segs as f64 }).unwrap();
    }
    execute(s, Command::ConvertToEditablePoly).unwrap();
    id
}

fn poly_box(s: &mut AppState) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    execute(s, Command::ConvertToEditablePoly).unwrap();
    *s.selection.iter().next().unwrap()
}

fn centroid(m: &Mesh, f: FaceId) -> Vec3 {
    let p = m.face_positions(f).unwrap();
    p.iter().sum::<Vec3>() / p.len() as f32
}

fn face_at(m: &Mesh, x: f32, y: f32) -> u32 {
    (0..m.face_count() as u32).find(|f| centroid(m, FaceId(*f)).truncate().distance(glam::Vec2::new(x, y)) < 0.1).expect("face there")
}

/// The canonical edge between the vertices at two positions.
fn edge(m: &Mesh, a: Vec3, b: Vec3) -> u32 {
    let at = |p: Vec3| (0..m.vertex_count() as u32).map(VertexId).find(|v| m.vertex_position(*v).unwrap().distance(p) < 1e-3).expect("vertex there");
    let (va, vb) = (at(a), at(b));
    (0..m.half_edges().len() as u32)
        .map(HalfEdgeId)
        .find(|e| m.canonical_edge_id(*e) == Some(*e) && m.edge_vertices(*e).is_some_and(|(x, y)| (x == va && y == vb) || (x == vb && y == va)))
        .expect("edge there")
        .0
}

fn border_count(m: &Mesh) -> usize {
    loom_mesh::select_ops::border_edges(m).len()
}

fn v(x: f32, y: f32) -> Vec3 {
    Vec3::new(x, y, 0.0)
}

/// A 40-unit plane of 4 x 4 cells with a two-cell hole x -10..10, y 0..10.
fn plane_with_hole(s: &mut AppState) -> NodeId {
    let id = poly_plane(s, 40.0, 4);
    execute(s, Command::FaceLevel).unwrap();
    let m = mesh(s, id);
    let ids = vec![face_at(&m, -5.0, 5.0), face_at(&m, 5.0, 5.0)];
    execute(s, Command::SelectSub { node: Some(id.0), ids, op: SelectOp::Replace }).unwrap();
    execute(s, Command::DeleteObjects).unwrap();
    assert_eq!(mesh(s, id).face_count(), 14);
    execute(s, Command::EdgeLevel).unwrap();
    id
}

fn bridge(s: &mut AppState, segments: u32) -> Result<serde_json::Value, String> {
    execute(s, Command::Bridge { segments, smooth: 0.0, bridge_adjacent: 0.0, twist1: 0, twist2: 0 })
}

#[test]
fn bridge_two_edge_runs_on_one_border() {
    let mut s = AppState::new();
    let id = plane_with_hole(&mut s);
    let m = mesh(&s, id);
    // The hole's top and bottom: two runs of two edges on the same border.
    let ids = vec![edge(&m, v(-10.0, 10.0), v(0.0, 10.0)), edge(&m, v(0.0, 10.0), v(10.0, 10.0)), edge(&m, v(-10.0, 0.0), v(0.0, 0.0)), edge(&m, v(0.0, 0.0), v(10.0, 0.0))];
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids, op: SelectOp::Replace }).unwrap();
    bridge(&mut s, 1).unwrap();
    let m = mesh(&s, id);
    m.check_invariants().unwrap();
    assert_eq!(m.face_count(), 16, "two quads fill the hole");
    assert_eq!(border_count(&m), 16, "only the outer border is left");
    assert_eq!(s.sub_selection.len(), 4, "the bridged edges stay selected");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    // Two segments: two rows of quads.
    let m = mesh(&s, id);
    let ids = vec![edge(&m, v(-10.0, 10.0), v(0.0, 10.0)), edge(&m, v(0.0, 10.0), v(10.0, 10.0)), edge(&m, v(-10.0, 0.0), v(0.0, 0.0)), edge(&m, v(0.0, 0.0), v(10.0, 0.0))];
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids, op: SelectOp::Replace }).unwrap();
    bridge(&mut s, 2).unwrap();
    let m = mesh(&s, id);
    m.check_invariants().unwrap();
    assert_eq!(m.face_count(), 18);
}

#[test]
fn bridge_edge_runs_of_different_lengths_zip_with_triangles() {
    let mut s = AppState::new();
    let id = plane_with_hole(&mut s);
    let m = mesh(&s, id);
    let ids = vec![edge(&m, v(-10.0, 10.0), v(0.0, 10.0)), edge(&m, v(0.0, 10.0), v(10.0, 10.0)), edge(&m, v(-10.0, 0.0), v(0.0, 0.0))];
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids, op: SelectOp::Replace }).unwrap();
    bridge(&mut s, 1).unwrap();
    let m = mesh(&s, id);
    m.check_invariants().unwrap();
    assert_eq!(m.face_count(), 17, "three triangles between a run of two edges and a run of one");
}

#[test]
fn one_border_edge_is_not_enough_to_bridge() {
    let mut s = AppState::new();
    let id = plane_with_hole(&mut s);
    let m = mesh(&s, id);
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![edge(&m, v(-10.0, 10.0), v(0.0, 10.0))], op: SelectOp::Replace }).unwrap();
    assert!(bridge(&mut s, 1).is_err());
    assert_eq!(mesh(&s, id).face_count(), 14);
}

#[test]
fn bridge_specific_edges_uses_the_picked_edges() {
    let mut s = AppState::new();
    let id = plane_with_hole(&mut s);
    let m = mesh(&s, id);
    let (top, bottom) = (edge(&m, v(-10.0, 10.0), v(0.0, 10.0)), edge(&m, v(-10.0, 0.0), v(0.0, 0.0)));
    execute(&mut s, Command::PickBridgeEdge { which: 1 }).unwrap();
    assert_eq!(s.pick_mode, Some(PickMode::BridgeEdge1));
    assert!(s.poly_settings.bridge_specific, "picking turns on Use Specific Edges");
    // An interior edge is refused: Bridge connects border edges only.
    let inner = edge(&m, v(-10.0, -10.0), v(0.0, -10.0));
    assert!(execute(&mut s, Command::PickSub { node: id.0, id: inner }).is_err());
    execute(&mut s, Command::PickSub { node: id.0, id: top }).unwrap();
    assert_eq!(s.pick_mode, None);
    execute(&mut s, Command::PickBridgeEdge { which: 2 }).unwrap();
    execute(&mut s, Command::PickSub { node: id.0, id: bottom }).unwrap();
    assert_eq!(s.poly_settings.bridge_edges, [Some((id, top)), Some((id, bottom))]);
    // The edge selection does not matter with Use Specific Edges.
    bridge(&mut s, 1).unwrap();
    let m = mesh(&s, id);
    m.check_invariants().unwrap();
    assert_eq!(m.face_count(), 15);
    execute(&mut s, Command::BridgeSpecificEdges { on: false }).unwrap();
    assert!(!s.poly_settings.bridge_specific);
}

#[test]
fn ctrl_remove_also_removes_the_leftover_vertices() {
    let mut s = AppState::new();
    let id = poly_box(&mut s);
    execute(&mut s, Command::EdgeLevel).unwrap();
    let connect_loop = |s: &mut AppState| {
        execute(s, Command::SelectSub { node: Some(id.0), ids: vec![0], op: SelectOp::Replace }).unwrap();
        execute(s, Command::SelectEdgeRing).unwrap();
        execute(s, Command::ConnectEdges { segments: 1, pinch: 0.0, slide: 0.0 }).unwrap();
        assert_eq!(s.sub_selection.len(), 4, "the new loop is selected");
    };
    connect_loop(&mut s);
    execute(&mut s, Command::Remove).unwrap();
    assert_eq!(mesh(&s, id).face_count(), 6);
    assert_eq!(mesh(&s, id).vertex_count(), 12, "plain Remove leaves the loop's vertices");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    connect_loop(&mut s);
    execute(&mut s, Command::CleanRemove).unwrap();
    let m = mesh(&s, id);
    m.check_invariants().unwrap();
    assert_eq!(m.face_count(), 6);
    assert_eq!(m.vertex_count(), 8, "Ctrl+Remove removes them too");
}

#[test]
fn swift_loop_inserts_a_loop_across_the_ring() {
    let mut s = AppState::new();
    let id = poly_box(&mut s);
    execute(&mut s, Command::EdgeLevel).unwrap();
    execute(&mut s, Command::SwiftLoopMode).unwrap();
    assert_eq!(s.pick_mode, Some(PickMode::SwiftLoop));
    let before = mesh(&s, id);
    let (a, b) = before.edge_vertices(HalfEdgeId(0)).unwrap();
    let want = before.vertex_position(a).unwrap().lerp(before.vertex_position(b).unwrap(), 0.25);
    execute(&mut s, Command::SwiftLoop { node: id.0, edge: 0, t: 0.25 }).unwrap();
    let m = mesh(&s, id);
    m.check_invariants().unwrap();
    assert_eq!(m.face_count(), 10, "the closed ring of four quads is split");
    assert_eq!(m.vertex_count(), 12);
    assert!((0..m.vertex_count() as u32).any(|i| m.vertex_position(VertexId(i)).unwrap().distance(want) < 1e-4), "the loop passes the click point");
    assert_eq!(s.pick_mode, Some(PickMode::SwiftLoop), "Swift Loop stays on for more clicks");
    execute(&mut s, Command::SwiftLoopMode).unwrap();
    assert_eq!(s.pick_mode, None);
    // An open ring stops at the border: three quads of a 3 x 3 plane.
    let mut s = AppState::new();
    let p = poly_plane(&mut s, 30.0, 3);
    let m = mesh(&s, p);
    let e = edge(&m, v(-15.0, -5.0), v(-5.0, -5.0));
    execute(&mut s, Command::SwiftLoop { node: p.0, edge: e, t: 0.5 }).unwrap();
    let m = mesh(&s, p);
    m.check_invariants().unwrap();
    assert_eq!(m.face_count(), 12);
}

#[test]
fn loop_mode_selects_loops() {
    let mut s = AppState::new();
    let id = poly_plane(&mut s, 40.0, 4);
    execute(&mut s, Command::LoopMode).unwrap();
    assert!(s.loop_mode);
    let m = mesh(&s, id);
    execute(&mut s, Command::EdgeLevel).unwrap();
    let e = edge(&m, v(0.0, 0.0), v(0.0, 10.0));
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![e], op: SelectOp::Replace }).unwrap();
    assert_eq!(s.sub_selection.len(), 4, "the whole line of edges across the plane");
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![e], op: SelectOp::Subtract }).unwrap();
    assert_eq!(s.sub_selection.len(), 0, "Alt takes the loop away");
    // Vertex: two adjacent vertices give their loop.
    execute(&mut s, Command::VertexLevel).unwrap();
    let at = |p: Vec3| (0..m.vertex_count() as u32).find(|i| m.vertex_position(VertexId(*i)).unwrap().distance(p) < 1e-3).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![at(v(0.0, 0.0))], op: SelectOp::Replace }).unwrap();
    assert_eq!(s.sub_selection.len(), 1);
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![at(v(10.0, 0.0))], op: SelectOp::Add }).unwrap();
    assert_eq!(s.sub_selection.len(), 5);
    // Polygon: two adjacent polygons give their row.
    execute(&mut s, Command::FaceLevel).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![face_at(&m, 5.0, 5.0)], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![face_at(&m, 15.0, 5.0)], op: SelectOp::Add }).unwrap();
    assert_eq!(s.sub_selection.len(), 4);
    assert!(s.sub_selection.iter().all(|(_, f)| match f { SubObjectId::Face(f) => (centroid(&m, f).y - 5.0).abs() < 1e-3, _ => false }));
    execute(&mut s, Command::LoopMode).unwrap();
    assert!(!s.loop_mode);
}

#[test]
fn select_by_half_and_invert_axis() {
    let mut s = AppState::new();
    let id = poly_box(&mut s);
    execute(&mut s, Command::FaceLevel).unwrap();
    execute(&mut s, Command::SelectByHalf { axis: Axis::X, invert: false }).unwrap();
    let m = mesh(&s, id);
    assert_eq!(s.sub_selection.len(), 1);
    assert!(s.sub_selection.iter().all(|(_, f)| matches!(f, SubObjectId::Face(f) if centroid(&m, f).x > 4.0)));
    execute(&mut s, Command::SelectByHalf { axis: Axis::X, invert: true }).unwrap();
    assert!(s.sub_selection.iter().all(|(_, f)| matches!(f, SubObjectId::Face(f) if centroid(&m, f).x < -4.0)));
    execute(&mut s, Command::VertexLevel).unwrap();
    execute(&mut s, Command::SelectByHalf { axis: Axis::Y, invert: false }).unwrap();
    assert_eq!(s.sub_selection.len(), 4);
    execute(&mut s, Command::ObjectLevel).unwrap();
    assert!(execute(&mut s, Command::SelectByHalf { axis: Axis::Y, invert: false }).is_err());
}

#[test]
fn cut_chains_from_the_last_point_until_right_click() {
    let mut s = AppState::new();
    let id = poly_box(&mut s);
    execute(&mut s, Command::Cut).unwrap();
    assert_eq!(s.pick_mode, Some(PickMode::CutA));
    let top = |m: &Mesh| (0..m.face_count() as u32).find(|f| m.face_positions(FaceId(*f)).unwrap().iter().all(|p| (p.z - 10.0).abs() < 1e-3)).unwrap();
    let m = mesh(&s, id);
    let f = top(&m);
    execute(&mut s, Command::PickFacePoint { node: id.0, face: f, point: [-5.0, 0.0, 10.0] }).unwrap();
    execute(&mut s, Command::PickFacePoint { node: id.0, face: f, point: [5.0, 0.0, 10.0] }).unwrap();
    assert_eq!(mesh(&s, id).face_count(), 7);
    assert_eq!(s.pick_mode, Some(PickMode::CutB), "the next click cuts on from the last point");
    // Down the +X side from where the first cut ended.
    let m = mesh(&s, id);
    let side = (0..m.face_count() as u32).find(|f| m.face_positions(FaceId(*f)).unwrap().iter().all(|p| (p.x - 5.0).abs() < 1e-3)).unwrap();
    execute(&mut s, Command::PickFacePoint { node: id.0, face: side, point: [5.0, 0.0, 0.0] }).unwrap();
    let m = mesh(&s, id);
    m.check_invariants().unwrap();
    assert_eq!(m.face_count(), 8);
    // One right-click ends the cut; a second leaves Cut.
    execute(&mut s, Command::ExitPickMode).unwrap();
    assert_eq!(s.pick_mode, Some(PickMode::CutA));
    assert!(s.pick_point_a.is_none());
    execute(&mut s, Command::ExitPickMode).unwrap();
    assert_eq!(s.pick_mode, None);
}

#[test]
fn deleting_polygons_asks_about_isolated_vertices() {
    let mut s = AppState::new();
    let id = poly_plane(&mut s, 20.0, 2);
    execute(&mut s, Command::FaceLevel).unwrap();
    let corner = face_at(&mesh(&s, id), 5.0, 5.0);
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![corner], op: SelectOp::Replace }).unwrap();
    let r = execute(&mut s, Command::DeleteObjects).unwrap();
    assert_eq!(r["ask"], "Delete isolated vertices?");
    assert_eq!(s.dialog, Some(loom_commands::Dialog::DeleteIsolatedVertices));
    assert_eq!(mesh(&s, id).face_count(), 4, "nothing deleted before the answer");
    execute(&mut s, Command::DeletePolygons { delete_isolated: false }).unwrap();
    assert_eq!(s.dialog, None);
    assert_eq!(mesh(&s, id).face_count(), 3);
    assert_eq!(mesh(&s, id).vertex_count(), 9, "No keeps the corner vertex");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![corner], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::DeletePolygons { delete_isolated: true }).unwrap();
    assert_eq!(mesh(&s, id).vertex_count(), 8, "Yes deletes it");
    // Deleting a polygon that leaves no vertex alone does not ask.
    let mut s = AppState::new();
    let b = poly_box(&mut s);
    execute(&mut s, Command::FaceLevel).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(b.0), ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::DeleteObjects).unwrap();
    assert_eq!(mesh(&s, b).face_count(), 5);
    assert_eq!(s.dialog, None);
}

#[test]
fn settings_spinners_take_unit_strings() {
    // Default units: system inches, generic display.
    let u = UnitsSetup::default();
    let near = |t: &str, want: f64| {
        let got = u.parse(t).unwrap_or(f64::NAN);
        assert!((got - want).abs() < 1e-9, "{t}: {got} != {want}");
    };
    near("7\"", 7.0);
    near("\u{2013}3.5\u{201d}", -3.5);
    near("1.5in", 1.5);
    near("2'", 24.0);
    near("5'6\"", 66.0);
    near("2.54cm", 1.0);
    near("3", 3.0);
    assert_eq!(u.parse("abc"), None);
}

#[test]
fn edges_are_numbered_from_one_in_first_use_order() {
    let m = loom_mesh::box_primitive(Vec3::splat(0.5));
    assert_eq!(m.edge_number(HalfEdgeId(0)), Some(1));
    let max = (0..m.half_edges().len() as u32).filter_map(|e| m.edge_number(HalfEdgeId(e))).max();
    assert_eq!(max, Some(12));
    let twin = m.half_edges()[0].twin;
    assert_eq!(m.edge_number(twin), Some(1), "both halves of an edge share its number");
}
