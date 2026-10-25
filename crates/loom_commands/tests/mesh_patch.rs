//! Editable Mesh / Edit Mesh (triangles with invisible edges) and Editable
//! Patch / Edit Patch / Patch Grids.

use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp, SelectionMode, ShapeKind};
use loom_mesh::Mesh;
use loom_scene::{Modifier, NodeId, ObjectKind};

fn sel(s: &AppState) -> NodeId {
    *s.selection.iter().next().unwrap()
}

fn box_mesh(s: &mut AppState) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    execute(s, Command::ConvertToEditableMesh).unwrap();
    sel(s)
}

fn mesh_of(s: &AppState, id: NodeId) -> Mesh {
    s.scene.node(id).unwrap().edit_poly().expect("an Editable Mesh").mesh.clone()
}

fn faces_selected(s: &AppState) -> usize {
    s.sub_selection.len()
}

fn tris(m: &Mesh) -> bool {
    (0..m.face_count() as u32).all(|f| m.face_vertex_ids(loom_mesh::FaceId(f)).unwrap().len() == 3)
}

/// The face whose normal points most along `dir`.
fn face_facing(m: &Mesh, dir: glam::Vec3) -> u32 {
    (0..m.face_count() as u32).max_by(|a, b| m.face_normal(loom_mesh::FaceId(*a)).dot(dir).total_cmp(&m.face_normal(loom_mesh::FaceId(*b)).dot(dir))).unwrap()
}

#[test]
fn convert_to_editable_mesh_cuts_polygons_with_invisible_edges() {
    let mut s = AppState::new();
    let id = box_mesh(&mut s);
    let node = s.scene.node(id).unwrap();
    assert!(matches!(node.object, ObjectKind::EditableMesh(_)));
    assert_eq!(node.object.stack_name(), "Editable Mesh");
    let m = mesh_of(&s, id);
    assert_eq!(m.face_count(), 12);
    assert!(tris(&m));
    assert_eq!(m.invisible_edges().len(), 6);
    assert_eq!(s.scene.eval_mesh(id).value.mesh.unique_edges().len(), 12, "the wireframe shows the box's edges only");
    let levels: Vec<&str> = loom_commands::base_sub_levels(&node.object).into_iter().map(|(n, _)| n).collect();
    assert_eq!(levels, ["Vertex", "Edge", "Face", "Polygon", "Element"]);
    // Undo brings the box back.
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(matches!(s.scene.node(id).unwrap().object, ObjectKind::Box { .. }));
}

#[test]
fn face_and_polygon_levels_and_ignore_visible_edges() {
    let mut s = AppState::new();
    let id = box_mesh(&mut s);
    execute(&mut s, Command::SubObjectLevel { level: 3 }).unwrap();
    assert_eq!(s.selection_mode, SelectionMode::Face);
    assert_eq!(s.level_label(s.selection_mode), "Face");
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![0], op: SelectOp::Replace }).unwrap();
    assert_eq!(faces_selected(&s), 1, "Face picks one triangle");
    execute(&mut s, Command::SubObjectLevel { level: 4 }).unwrap();
    assert_eq!(s.level_label(s.selection_mode), "Polygon");
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![0], op: SelectOp::Replace }).unwrap();
    assert_eq!(faces_selected(&s), 2, "Polygon picks the side's two triangles");
    // Every edge visible: a polygon is one triangle, unless Ignore Visible Edges.
    execute(&mut s, Command::SubObjectLevel { level: 2 }).unwrap();
    let all: Vec<u32> = (0..mesh_of(&s, id).half_edge_count() as u32).collect();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: all, op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MeshEdgeVisibility { visible: true }).unwrap();
    assert_eq!(s.scene.eval_mesh(id).value.mesh.unique_edges().len(), 18);
    execute(&mut s, Command::SubObjectLevel { level: 4 }).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![0], op: SelectOp::Replace }).unwrap();
    assert_eq!(faces_selected(&s), 1);
    execute(&mut s, Command::MeshPatchSetting { name: "Ignore Visible Edges".into(), value: 1.0 }).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![0], op: SelectOp::Replace }).unwrap();
    assert_eq!(faces_selected(&s), 2, "coplanar within Planar Thresh");
    execute(&mut s, Command::MeshPatchSetting { name: "Planar Thresh".into(), value: 90.0 }).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![0], op: SelectOp::Replace }).unwrap();
    assert_eq!(faces_selected(&s), 12);
    // Keys 1-5 follow the mesh's own list: 3 is Face, again returns to the top.
    execute(&mut s, Command::BorderLevel).unwrap();
    assert_eq!(s.level_label(s.selection_mode), "Face");
    execute(&mut s, Command::BorderLevel).unwrap();
    assert_eq!(s.selection_mode, SelectionMode::Object);
}

#[test]
fn auto_edge_turn_and_divide() {
    let mut s = AppState::new();
    let id = box_mesh(&mut s);
    execute(&mut s, Command::SubObjectLevel { level: 2 }).unwrap();
    let all: Vec<u32> = (0..mesh_of(&s, id).half_edge_count() as u32).collect();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: all.clone(), op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MeshEdgeVisibility { visible: true }).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: all, op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MeshAutoEdge).unwrap();
    assert_eq!(mesh_of(&s, id).invisible_edges().len(), 6, "coplanar edges hidden at 24 degrees");
    // Turn the top's hidden diagonal.
    let m = mesh_of(&s, id);
    let diag = (0..m.half_edge_count() as u32).find(|h| !m.edge_visible(loom_mesh::HalfEdgeId(*h)) && m.half_edges()[*h as usize].face.is_some()).unwrap();
    let (a, b) = m.edge_vertices(loom_mesh::HalfEdgeId(diag)).unwrap();
    execute(&mut s, Command::MeshTurnEdge { edges: vec![diag] }).unwrap();
    let t = mesh_of(&s, id);
    assert_eq!(t.face_count(), 12);
    assert_eq!(t.invisible_edges().len(), 6);
    let (pa, pb) = (m.vertex_position(a).unwrap(), m.vertex_position(b).unwrap());
    assert!(!t.invisible_edges().iter().any(|(x, y)| (*x == pa && *y == pb) || (*x == pb && *y == pa)), "the old diagonal is gone");
    // Divide an edge: both faces split, the spokes invisible.
    let border = (0..t.half_edge_count() as u32).find(|h| t.edge_visible(loom_mesh::HalfEdgeId(*h))).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![border], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MeshDivide).unwrap();
    let d = mesh_of(&s, id);
    assert_eq!(d.face_count(), 14);
    assert_eq!(d.vertex_count(), 9);
    assert_eq!(d.invisible_edges().len(), 8);
    // Divide a face: three triangles round a centre vertex.
    execute(&mut s, Command::SubObjectLevel { level: 3 }).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MeshDivide).unwrap();
    assert_eq!(mesh_of(&s, id).face_count(), 16);
}

#[test]
fn extrude_bevel_tessellate_and_normals() {
    let mut s = AppState::new();
    let id = box_mesh(&mut s);
    let top = face_facing(&mesh_of(&s, id), glam::Vec3::Z);
    execute(&mut s, Command::SubObjectLevel { level: 4 }).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![top], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MeshExtrude { amount: 5.0 }).unwrap();
    let m = mesh_of(&s, id);
    assert!(tris(&m));
    assert_eq!(m.face_count(), 20);
    let (lo, hi) = m.aabb();
    assert!((hi.z - lo.z - 15.0).abs() < 1e-3);
    assert_eq!(faces_selected(&s), 2, "the extruded faces stay selected");
    execute(&mut s, Command::MeshBevel { height: 2.0, outline: -1.0 }).unwrap();
    let b = mesh_of(&s, id);
    assert_eq!(b.face_count(), 28);
    let top_xs: Vec<f32> = b.vertices().iter().filter(|v| (v.position.z - b.aabb().1.z).abs() < 1e-3).map(|v| v.position.x.abs()).collect();
    assert!(top_xs.iter().all(|x| (x - 4.0).abs() < 1e-3), "{top_xs:?}");
    // Tessellate by edge: 2 → 8 faces and the neighbours cut at the midpoints.
    execute(&mut s, Command::MeshPatchSetting { name: "Tension".into(), value: 0.0 }).unwrap();
    execute(&mut s, Command::MeshTessellate).unwrap();
    let t = mesh_of(&s, id);
    assert!(tris(&t));
    assert!(t.check_invariants().is_ok());
    assert_eq!(faces_selected(&s), 8);
    // Flip all, then Unify turns them back outward.
    execute(&mut s, Command::SubObjectLevel { level: 5 }).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MeshFlipNormals).unwrap();
    let f = mesh_of(&s, id);
    let up = face_facing(&t, glam::Vec3::Z);
    assert!(f.face_normal(loom_mesh::FaceId(up)).z < -0.9);
    execute(&mut s, Command::MeshUnifyNormals).unwrap();
    assert!(mesh_of(&s, id).face_normal(loom_mesh::FaceId(up)).z > 0.9);
}

#[test]
fn explode_to_objects_and_elements() {
    let mut s = AppState::new();
    let id = box_mesh(&mut s);
    execute(&mut s, Command::SubObjectLevel { level: 3 }).unwrap();
    execute(&mut s, Command::MeshExplode { name: Some("Side".into()) }).unwrap();
    assert_eq!(s.scene.len(), 7, "the box and its six sides");
    assert_eq!(mesh_of(&s, id).face_count(), 0);
    let sides: Vec<_> = s.scene.nodes().filter(|n| n.name.starts_with("Side")).collect();
    assert_eq!(sides.len(), 6);
    assert!(sides.iter().all(|n| matches!(n.object, ObjectKind::EditableMesh(_)) && n.edit_poly().unwrap().mesh.face_count() == 2));
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.len(), 1);
    execute(&mut s, Command::MeshPatchSetting { name: "Explode To Objects".into(), value: 0.0 }).unwrap();
    execute(&mut s, Command::MeshExplode { name: None }).unwrap();
    let m = mesh_of(&s, id);
    assert_eq!(m.vertex_count(), 24);
    assert_eq!(m.face_count(), 12);
}

#[test]
fn polygon_tools_keep_an_editable_mesh_triangles() {
    let mut s = AppState::new();
    let id = box_mesh(&mut s);
    execute(&mut s, Command::SubObjectLevel { level: 4 }).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::DeleteObjects).unwrap();
    let m = mesh_of(&s, id);
    assert!(matches!(s.scene.node(id).unwrap().object, ObjectKind::EditableMesh(_)));
    assert_eq!(m.face_count(), 10);
    assert!(tris(&m));
    assert_eq!(m.invisible_edges().len(), 5);
    // Attach a box: its sides arrive as triangles with hidden diagonals.
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Object }).unwrap();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [30.0, 0.0, 0.0], size: [10.0; 3] }).unwrap();
    let other = sel(&s);
    execute(&mut s, Command::SelectNodes { ids: vec![id.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::AttachNode { id: other.0 }).unwrap();
    let a = mesh_of(&s, id);
    assert_eq!(a.face_count(), 22);
    assert_eq!(a.invisible_edges().len(), 11);
    // Convert to Editable Poly merges the triangles back.
    execute(&mut s, Command::ConvertToEditablePoly).unwrap();
    let p = s.scene.node(id).unwrap().object.poly().expect("an Editable Poly").mesh.clone();
    assert_eq!(p.face_count(), 11);
    assert!(!p.has_invisible_edges());
}

#[test]
fn edit_mesh_modifier_is_its_own_modifier() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    let id = sel(&s);
    execute(&mut s, Command::AddModifier { name: "Edit Mesh".into() }).unwrap();
    let node = s.scene.node(id).unwrap();
    assert!(matches!(node.modifiers[0].modifier, Modifier::EditMesh { .. }));
    assert_eq!(node.modifiers[0].modifier.sub_levels(), &["Vertex", "Edge", "Face", "Polygon", "Element"]);
    assert_eq!(s.scene.eval_mesh(id).value.mesh.face_count(), 12, "the input as triangles");
    execute(&mut s, Command::ModifierSubobjectLevel { index: 0, level: 4 }).unwrap();
    assert_eq!(s.level_label(s.selection_mode), "Polygon");
    let top = face_facing(&s.scene.eval_mesh(id).value.mesh, glam::Vec3::Z);
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![top], op: SelectOp::Replace }).unwrap();
    assert_eq!(faces_selected(&s), 2);
    execute(&mut s, Command::MeshExtrude { amount: 5.0 }).unwrap();
    let out = s.scene.eval_mesh(id).value.mesh.clone();
    assert_eq!(out.face_count(), 20);
    assert!(matches!(s.scene.node(id).unwrap().object, ObjectKind::Box { .. }), "the base object is untouched");
    // Delete at the Edit Mesh entry edits the entry, not the base.
    execute(&mut s, Command::DeleteObjects).unwrap();
    assert_eq!(s.scene.eval_mesh(id).value.mesh.face_count(), 18);
    assert!(matches!(s.scene.node(id).unwrap().object, ObjectKind::Box { .. }));
    // Edit Poly stays an Edit Poly.
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Object }).unwrap();
    execute(&mut s, Command::AddModifier { name: "Edit Poly".into() }).unwrap();
    assert!(matches!(s.scene.node(id).unwrap().modifiers[1].modifier, Modifier::EditPoly { .. }));
    assert_eq!(s.scene.eval_mesh(id).value.mesh.face_count(), 9, "Edit Poly takes the Edit Mesh result as polygons");
}

#[test]
fn patch_grids_and_editable_patch() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePatchGrid { quad: true, position: [0.0; 3], length: 20.0, width: 30.0 }).unwrap();
    let id = sel(&s);
    assert_eq!(s.scene.node(id).unwrap().name, "QuadPatch001");
    let m = s.scene.eval_mesh(id).value.mesh.clone();
    assert_eq!(m.face_count(), 36);
    assert_eq!(m.unique_edges().len(), 24, "interior edges hidden");
    execute(&mut s, Command::SetParameter { id: id.0, name: "Length Segs".into(), value: 2.0 }).unwrap();
    assert_eq!(s.scene.eval_mesh(id).value.mesh.face_count(), 72);
    execute(&mut s, Command::ConvertToEditablePatch).unwrap();
    let node = s.scene.node(id).unwrap();
    assert_eq!(node.object.stack_name(), "Editable Patch");
    assert_eq!(node.object.patch().unwrap().patch.patches.len(), 2);
    let levels: Vec<&str> = loom_commands::base_sub_levels(&node.object).into_iter().map(|(n, _)| n).collect();
    assert_eq!(levels, ["Vertex", "Handle", "Edge", "Patch", "Element"]);
    // View Steps.
    execute(&mut s, Command::SetParameter { id: id.0, name: "View Steps".into(), value: 2.0 }).unwrap();
    assert_eq!(s.scene.eval_mesh(id).value.mesh.face_count(), 18);
    // Move a vertex: the surface follows.
    execute(&mut s, Command::SubObjectLevel { level: 1 }).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 5.0], system: None }).unwrap();
    let pm = s.scene.node(id).unwrap().object.patch().unwrap().patch.clone();
    assert!((pm.verts[0].z - 5.0).abs() < 1e-4);
    assert!(s.scene.eval_mesh(id).value.mesh.aabb().1.z > 4.9);
    // Handle level picks handles only, and moving one bends an edge.
    execute(&mut s, Command::SubObjectLevel { level: 2 }).unwrap();
    assert_eq!(s.level_label(s.selection_mode), "Handle");
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![0], op: SelectOp::Replace }).unwrap();
    assert_eq!(s.sub_selection.len(), 0, "a vertex is no handle");
    let nv = pm.verts.len() as u32;
    let h = pm.edges.iter().find(|e| e.v[0] == 1 || e.v[1] == 1).unwrap();
    let hs = if pm.edges.iter().position(|e| e == h).is_some() && h.v[0] == 1 { h.vec[0] } else { h.vec[1] };
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![nv + hs], op: SelectOp::Replace }).unwrap();
    assert_eq!(s.sub_selection.len(), 1);
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, -9.0], system: None }).unwrap();
    let pm2 = s.scene.node(id).unwrap().object.patch().unwrap().patch.clone();
    assert!((pm2.vecs[hs as usize].z - pm.vecs[hs as usize].z + 9.0).abs() < 1e-4);
    assert_eq!(pm2.verts, pm.verts, "moving a handle moves no vertex");
    // Patch level: extrude one patch.
    execute(&mut s, Command::SubObjectLevel { level: 4 }).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::PatchExtrude { amount: 4.0, outline: 0.0 }).unwrap();
    let pm3 = s.scene.node(id).unwrap().object.patch().unwrap().patch.clone();
    assert_eq!(pm3.patches.len(), 6);
    assert!(s.scene.eval_mesh(id).value.mesh.check_invariants().is_ok());
    // Delete it again, then flip the other.
    execute(&mut s, Command::PatchDelete).unwrap();
    assert_eq!(s.scene.node(id).unwrap().object.patch().unwrap().patch.patches.len(), 5);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.node(id).unwrap().object.patch().unwrap().patch.patches.len(), 6);
}

#[test]
fn add_quad_weld_and_relax() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePatchGrid { quad: true, position: [0.0; 3], length: 10.0, width: 10.0 }).unwrap();
    let id = sel(&s);
    execute(&mut s, Command::ConvertToEditablePatch).unwrap();
    execute(&mut s, Command::SubObjectLevel { level: 3 }).unwrap();
    let cage = s.scene.node(id).unwrap().cage_mesh().into_owned();
    let open = (0..cage.half_edge_count() as u32).find(|h| cage.half_edges()[*h as usize].face.is_some()).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![open], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::PatchAddQuad).unwrap();
    let pm = s.scene.node(id).unwrap().object.patch().unwrap().patch.clone();
    assert_eq!(pm.patches.len(), 2);
    assert_eq!(pm.verts.len(), 6);
    assert_eq!(s.scene.eval_mesh(id).value.mesh.face_count(), 72);
    // Tri on another open edge.
    let cage = s.scene.node(id).unwrap().cage_mesh().into_owned();
    let open = (0..cage.half_edge_count() as u32).find(|h| {
        let t = cage.half_edges()[*h as usize].twin;
        cage.half_edges()[*h as usize].face.is_some() && cage.half_edges()[t.0 as usize].face.is_none()
    }).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![open], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::PatchAddTri).unwrap();
    assert_eq!(s.scene.node(id).unwrap().object.patch().unwrap().patch.patches.len(), 3);
    // Weld two vertices moved together.
    execute(&mut s, Command::SubObjectLevel { level: 1 }).unwrap();
    let pm = s.scene.node(id).unwrap().object.patch().unwrap().patch.clone();
    let n = pm.verts.len();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![0], op: SelectOp::Replace }).unwrap();
    let to = pm.verts[1] - pm.verts[0];
    execute(&mut s, Command::MoveSelection { delta: to.to_array(), system: None }).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![0, 1], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::PatchWeld).unwrap();
    assert_eq!(s.scene.node(id).unwrap().object.patch().unwrap().patch.verts.len(), n - 1);
    // Relax runs on every vertex when none is selected.
    s.sub_selection.clear();
    execute(&mut s, Command::PatchRelax).unwrap();
}

#[test]
fn edit_patch_edits_the_surface_patches() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreateShape { kind: ShapeKind::Rectangle, position: [0.0; 3], size: [20.0, 20.0, 0.0] }).unwrap();
    let id = sel(&s);
    execute(&mut s, Command::ConvertToEditableSpline).unwrap();
    execute(&mut s, Command::AddModifier { name: "Surface".into() }).unwrap();
    let surf = s.scene.eval_mesh(id).value.clone();
    assert_eq!(surf.patch.as_ref().map(|p| p.patches.len()), Some(1), "Surface passes its patch up");
    execute(&mut s, Command::AddModifier { name: "Edit Patch".into() }).unwrap();
    let node = s.scene.node(id).unwrap();
    assert!(matches!(node.modifiers[1].modifier, Modifier::EditPatch { .. }));
    assert_eq!(node.modifiers[1].modifier.edit_patch().unwrap().patch.patches.len(), 1);
    execute(&mut s, Command::ModifierSubobjectLevel { index: 1, level: 4 }).unwrap();
    assert_eq!(s.level_label(s.selection_mode), "Patch");
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::PatchExtrude { amount: 6.0, outline: 0.0 }).unwrap();
    let node = s.scene.node(id).unwrap();
    assert_eq!(node.modifiers[1].modifier.edit_patch().unwrap().patch.patches.len(), 5);
    let (lo, hi) = s.scene.eval_mesh(id).value.mesh.aabb();
    assert!((hi - lo).max_element() > 19.0 && (hi - lo).min_element() > 5.9, "{lo} {hi}");
    assert!(node.object.spline().is_some(), "the spline cage is untouched");
    // Collapse gives an Editable Patch? The base is a spline: it collapses to a poly.
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Object }).unwrap();
    execute(&mut s, Command::ConvertToEditablePatch).unwrap();
    assert_eq!(s.scene.node(id).unwrap().object.patch().unwrap().patch.patches.len(), 5);
}

#[test]
fn editable_mesh_and_patch_save_and_export() {
    let dir = std::env::temp_dir().join("loom-mesh-patch-tests");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut s = AppState::new();
    let m = box_mesh(&mut s);
    execute(&mut s, Command::CreatePatchGrid { quad: true, position: [40.0, 0.0, 0.0], length: 10.0, width: 10.0 }).unwrap();
    execute(&mut s, Command::ConvertToEditablePatch).unwrap();
    let p = sel(&s);
    let path = dir.join("scene.loom");
    execute(&mut s, Command::SaveFileAs { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    let mut t = AppState::new();
    execute(&mut t, Command::OpenFile { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    assert_eq!(t.scene.node(m).unwrap(), s.scene.node(m).unwrap());
    assert_eq!(mesh_of(&t, m).invisible_edges().len(), 6, "edge flags survive the file");
    assert_eq!(t.scene.node(p).unwrap(), s.scene.node(p).unwrap());
    // FBX: the mesh as its triangles, the patch as its surface.
    let fbx = dir.join("scene.fbx");
    execute(&mut s, Command::SelectNodes { ids: vec![m.0, p.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::ExportSelected { path: Some(fbx.to_string_lossy().into_owned()) }).unwrap();
    let doc = loom_fbx::read(&std::fs::read(&fbx).unwrap(), 0.0254).unwrap();
    let faces: Vec<usize> = doc.nodes.iter().filter_map(|n| match &n.kind { loom_fbx::FbxKind::Mesh { mesh, .. } => Some(mesh.face_count()), _ => None }).collect();
    assert_eq!(faces.len(), 2);
    assert!(faces.contains(&12) && faces.contains(&36), "{faces:?}");
}
