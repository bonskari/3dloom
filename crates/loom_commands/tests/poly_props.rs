use loom_commands::{execute, AppState, Command, PaintMode, PrimitiveKind, SelectOp, SelectionMode};
use loom_scene::NodeId;

fn poly_box(s: &mut AppState) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    execute(s, Command::ConvertToEditablePoly).unwrap();
    *s.selection.iter().next().unwrap()
}

#[test]
fn material_id_set_and_select() {
    let mut s = AppState::new();
    let a = poly_box(&mut s);
    execute(&mut s, Command::FaceLevel).unwrap();
    // A box carries material ids 1-6 per side already.
    let before2 = s.scene.node(a).unwrap().object.mesh().faces()[2].material_id;
    let before0 = s.scene.node(a).unwrap().object.mesh().faces()[0].material_id;
    execute(&mut s, Command::SelectSub { node: Some(a.0), ids: vec![0, 1], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SetMaterialId { id: 9 }).unwrap();
    let m = s.scene.node(a).unwrap().object.mesh().into_owned();
    assert_eq!(m.faces()[0].material_id, 8);
    assert_eq!(m.faces()[1].material_id, 8);
    assert_eq!(m.faces()[2].material_id, before2);
    execute(&mut s, Command::SelectNone).unwrap();
    execute(&mut s, Command::SelectByMaterialId { id: 9, clear: true }).unwrap();
    assert_eq!(s.sub_selection.len(), 2);
    // Undo: Select By Material ID, Select None, then Set Material ID.
    for _ in 0..3 {
        execute(&mut s, Command::UndoSceneOperation).unwrap();
    }
    assert_eq!(s.scene.node(a).unwrap().object.mesh().faces()[0].material_id, before0);
}

#[test]
fn smoothing_groups_set_select_clear_auto() {
    let mut s = AppState::new();
    let a = poly_box(&mut s);
    execute(&mut s, Command::FaceLevel).unwrap();
    execute(&mut s, Command::SelectAll).unwrap();
    execute(&mut s, Command::ClearSmoothingGroups).unwrap();
    assert!(s.scene.node(a).unwrap().object.mesh().faces().iter().all(|f| f.smoothing_groups == 0));
    execute(&mut s, Command::SelectSub { node: Some(a.0), ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SetSmoothingGroups { bits: 1 << 4, add: false }).unwrap();
    execute(&mut s, Command::SetSmoothingGroups { bits: 1 << 6, add: true }).unwrap();
    assert_eq!(s.scene.node(a).unwrap().object.mesh().faces()[0].smoothing_groups, (1 << 4) | (1 << 6));
    execute(&mut s, Command::SelectBySmoothingGroup { bits: 1 << 6, clear: true }).unwrap();
    assert_eq!(s.sub_selection.len(), 1);
    execute(&mut s, Command::SelectAll).unwrap();
    execute(&mut s, Command::AutoSmoothThreshold { degrees: 45.0 }).unwrap();
    execute(&mut s, Command::AutoSmooth).unwrap();
    let m = s.scene.node(a).unwrap().object.mesh().into_owned();
    assert!(m.faces().iter().all(|f| f.smoothing_groups != 0));
    // 90° box edges: adjacent faces never share a group.
    for he in m.half_edges() {
        if let (Some(f1), Some(f2)) = (he.face, m.half_edges()[he.twin.0 as usize].face) {
            assert_eq!(m.faces()[f1.0 as usize].smoothing_groups & m.faces()[f2.0 as usize].smoothing_groups, 0);
        }
    }
}

#[test]
fn vertex_color_set_and_render_tint() {
    let mut s = AppState::new();
    let a = poly_box(&mut s);
    execute(&mut s, Command::VertexLevel).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(a.0), ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SetVertexColor { rgb: [255, 0, 0] }).unwrap();
    execute(&mut s, Command::SetVertexAlpha { alpha: 50.0 }).unwrap();
    let m = s.scene.node(a).unwrap().object.mesh().into_owned();
    assert_eq!(m.vertices()[0].color, [1.0, 0.0, 0.0]);
    assert!((m.vertices()[0].alpha - 0.5).abs() < 1e-6);
    execute(&mut s, Command::SelectNone).unwrap();
    execute(&mut s, Command::SelectByVertexColor { rgb: [255, 0, 0], range: [10, 10, 10], illumination: false }).unwrap();
    assert_eq!(s.sub_selection.len(), 1);
    // Vertex Channel Display tints the render buffer.
    let (verts, _) = loom_render::triangulate_with_face_normals(&m, [1.0, 1.0, 1.0, 1.0], true);
    assert!(verts.iter().any(|v| v.color[1] < 0.01 && v.color[0] > 0.9), "a red-tinted vertex exists");
    let (plain, _) = loom_render::triangulate_with_face_normals(&m, [1.0, 1.0, 1.0, 1.0], false);
    assert!(plain.iter().all(|v| v.color[1] > 0.9));
}

#[test]
fn nurms_display_mesh_has_more_faces_base_unchanged() {
    let mut s = AppState::new();
    let a = poly_box(&mut s);
    execute(&mut s, Command::SubdivisionSurface { use_nurms: Some(true), smooth_result: None, isoline_display: None, show_cage: None, iterations: Some(2), smoothness: None, separate_by_smoothing: None, separate_by_materials: None }).unwrap();
    let p = s.scene.node(a).unwrap().object.poly().unwrap().clone();
    assert!(p.subdiv.use_nurms.get());
    assert_eq!(p.mesh.face_count(), 6, "the base cage is untouched");
    assert_eq!(p.display_mesh().face_count(), 96);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(!s.scene.node(a).unwrap().object.poly().unwrap().subdiv.use_nurms.get());
}

#[test]
fn nurms_keeps_open_border_and_attributes() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0; 3], size: [10.0, 10.0, 0.0] }).unwrap();
    execute(&mut s, Command::ConvertToEditablePoly).unwrap();
    let a = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::FaceLevel).unwrap();
    execute(&mut s, Command::SelectAll).unwrap();
    execute(&mut s, Command::SetMaterialId { id: 5 }).unwrap();
    execute(&mut s, Command::SubdivisionSurface { use_nurms: Some(true), smooth_result: None, isoline_display: None, show_cage: None, iterations: Some(1), smoothness: Some(1.0), separate_by_smoothing: None, separate_by_materials: None }).unwrap();
    let d = s.scene.node(a).unwrap().object.poly().unwrap().display_mesh().into_owned();
    assert!(d.faces().iter().all(|f| f.material_id == 4));
    let (lo, hi) = d.aabb();
    assert!((hi.x - lo.x) > 7.5, "the open border stays near the cage, {lo:?} {hi:?}");
}

#[test]
fn paint_push_pull_moves_along_normal() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0; 3], size: [20.0, 20.0, 0.0] }).unwrap();
    let a = *s.selection.iter().next().unwrap();
    for n in ["Length Segs", "Width Segs"] {
        execute(&mut s, Command::SetParameter { id: a.0, name: n.into(), value: 4.0 }).unwrap();
    }
    execute(&mut s, Command::ConvertToEditablePoly).unwrap();
    execute(&mut s, Command::PaintDeformSettings { value: Some(10.0), size: Some(6.0), strength: Some(1.0), direction: Some("OriginalNormals".into()) }).unwrap();
    execute(&mut s, Command::PaintDeformPushPull).unwrap();
    assert_eq!(s.paint_deform, Some(PaintMode::PushPull));
    execute(&mut s, Command::PaintDeformStroke { node: a.0, points: vec![[0.0, 0.0, 0.0]], mode: PaintMode::PushPull, invert: false }).unwrap();
    let m = s.scene.node(a).unwrap().object.mesh().into_owned();
    let centre = (0..25u32).find(|v| { let p = m.vertex_position(loom_mesh::VertexId(*v)).unwrap(); p.x.abs() < 1e-3 && p.y.abs() < 1e-3 }).unwrap();
    let z = m.vertex_position(loom_mesh::VertexId(centre)).unwrap().z;
    assert!((z - 1.0).abs() < 1e-4, "centre pulled by value*0.1 = 1.0, got {z}");
    let corner = m.vertex_position(loom_mesh::VertexId(0)).unwrap().z;
    assert!(corner.abs() < 1e-6, "outside the brush nothing moves");
    execute(&mut s, Command::PaintDeformStroke { node: a.0, points: vec![[0.0, 0.0, 0.0]], mode: PaintMode::PushPull, invert: true }).unwrap();
    let z2 = s.scene.node(a).unwrap().object.mesh().vertex_position(loom_mesh::VertexId(centre)).unwrap().z;
    // The brush centre is still at the plane: the raised vertex sits 1 unit from it, so the falloff is 5/6.
    assert!(z2 < 0.2 && z2 >= 0.0, "Ctrl inverts, got {z2}");
    execute(&mut s, Command::PaintDeformStroke { node: a.0, points: vec![[0.0, 0.0, 0.0]], mode: PaintMode::PushPull, invert: false }).unwrap();
    execute(&mut s, Command::PaintDeformCancel).unwrap();
    assert!(s.scene.node(a).unwrap().object.mesh().vertex_position(loom_mesh::VertexId(centre)).unwrap().z.abs() < 1e-6, "Cancel restores");
    assert_eq!(s.selection_mode, SelectionMode::Object);
}

#[test]
fn paint_relax_smooths() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0; 3], size: [20.0, 20.0, 0.0] }).unwrap();
    let a = *s.selection.iter().next().unwrap();
    for n in ["Length Segs", "Width Segs"] {
        execute(&mut s, Command::SetParameter { id: a.0, name: n.into(), value: 2.0 }).unwrap();
    }
    execute(&mut s, Command::ConvertToEditablePoly).unwrap();
    execute(&mut s, Command::PaintDeformSettings { value: Some(30.0), size: Some(6.0), strength: Some(1.0), direction: Some("Z".into()) }).unwrap();
    execute(&mut s, Command::PaintDeformStroke { node: a.0, points: vec![[0.0, 0.0, 0.0]], mode: PaintMode::PushPull, invert: false }).unwrap();
    let m = s.scene.node(a).unwrap().object.mesh().into_owned();
    let centre = (0..9u32).find(|v| m.vertex_position(loom_mesh::VertexId(*v)).unwrap().length() < 3.1).unwrap();
    let z = m.vertex_position(loom_mesh::VertexId(centre)).unwrap().z;
    assert!(z > 2.9, "{z}");
    execute(&mut s, Command::PaintDeformStroke { node: a.0, points: vec![[0.0, 0.0, 0.0]], mode: PaintMode::Relax, invert: false }).unwrap();
    let z2 = s.scene.node(a).unwrap().object.mesh().vertex_position(loom_mesh::VertexId(centre)).unwrap().z;
    assert!(z2 < z * 0.6, "relax pulls it back toward the flat neighbours, got {z2} from {z}");
    execute(&mut s, Command::PaintDeformCommit).unwrap();
    assert!(s.scene.node(a).unwrap().object.poly().unwrap().paint.original.is_none());
}
