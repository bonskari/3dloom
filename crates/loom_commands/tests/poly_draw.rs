//! Freeform > PolyDraw > Branches: a polygon grows a multi-segment,
//! tapering extrusion along a dragged path.

use glam::Vec3;
use loom_commands::{execute, AppState, Command, PrimitiveKind};
use loom_mesh::FaceId;
use loom_scene::NodeId;

fn poly_box(s: &mut AppState, pos: [f32; 3]) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: pos, size: [10.0; 3] }).unwrap();
    execute(s, Command::ConvertToEditablePoly).unwrap();
    *s.selection.iter().next().unwrap()
}

/// The box's top polygon (all corners at the highest z).
fn top_face(s: &AppState, id: NodeId) -> FaceId {
    let mesh = s.scene.node(id).unwrap().object.mesh().into_owned();
    (0..mesh.face_count() as u32).map(FaceId).find(|f| mesh.face_positions(*f).unwrap().iter().all(|p| p.z > 9.99)).expect("a top face")
}

/// World corners of polygon `f`.
fn corners(s: &AppState, id: NodeId, f: FaceId) -> Vec<Vec3> {
    let m = s.scene.object_matrix(id);
    s.scene.node(id).unwrap().object.mesh().face_positions(f).unwrap().into_iter().map(|p| m.transform_point3(p)).collect()
}

fn centre(ps: &[Vec3]) -> Vec3 {
    ps.iter().copied().sum::<Vec3>() / ps.len() as f32
}

#[test]
fn a_branch_extrudes_one_segment_per_path_point_and_tapers_its_end() {
    let mut s = AppState::new();
    let id = poly_box(&mut s, [20.0, 0.0, 0.0]);
    let f = top_face(&s, id);
    let start = centre(&corners(&s, id, f));
    let path = vec![start, start + Vec3::new(0.0, 0.0, 10.0), start + Vec3::new(5.0, 0.0, 20.0), start + Vec3::new(10.0, 0.0, 25.0)];
    let faces_before = s.scene.node(id).unwrap().object.mesh().face_count();
    let undo = s.undo.len();
    execute(&mut s, Command::Branch { node: id.0, faces: vec![f.0], path: path.iter().map(|p| p.to_array()).collect(), taper: -0.5 }).unwrap();
    let mesh = s.scene.node(id).unwrap().object.mesh().into_owned();
    assert_eq!(mesh.face_count(), faces_before + 4 * 3, "three segments of four side polygons each");
    // The cap keeps the polygon's id, sits on the last path point and is half the size.
    let cap = corners(&s, id, f);
    assert!(centre(&cap).distance(path[3]) < 1e-3, "the cap follows the path's end: {:?}", centre(&cap));
    let width = cap.iter().map(|p| p.x).fold(f32::MIN, f32::max) - cap.iter().map(|p| p.x).fold(f32::MAX, f32::min);
    assert!((width - 5.0).abs() < 1e-3, "taper -0.5 halves the 10-unit polygon: {width}");
    assert_eq!(s.undo.len(), undo + 1, "one undo entry");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.node(id).unwrap().object.mesh().face_count(), faces_before);
}

#[test]
fn a_taper_below_minus_one_shrinks_to_a_point_then_grows_again() {
    let mut s = AppState::new();
    let id = poly_box(&mut s, [0.0; 3]);
    let f = top_face(&s, id);
    let start = centre(&corners(&s, id, f));
    let path: Vec<[f32; 3]> = (0..=4).map(|k| (start + Vec3::Z * 5.0 * k as f32).to_array()).collect();
    execute(&mut s, Command::Branch { node: id.0, faces: vec![f.0], path, taper: -2.0 }).unwrap();
    let cap = corners(&s, id, f);
    let width = cap.iter().map(|p| p.x).fold(f32::MIN, f32::max) - cap.iter().map(|p| p.x).fold(f32::MAX, f32::min);
    assert!((width - 10.0).abs() < 1e-3, "1 + (-2) = -1: the end is as large as the start again: {width}");
}

#[test]
fn branches_mode_toggles_and_ends_the_other_poly_modes() {
    let mut s = AppState::new();
    poly_box(&mut s, [0.0; 3]);
    execute(&mut s, Command::ExtrudeMode).unwrap();
    assert!(s.poly_drag.is_some());
    execute(&mut s, Command::BranchesMode).unwrap();
    assert!(s.branches, "Branches is on");
    assert!(s.poly_drag.is_none(), "Extrude mode ended");
    execute(&mut s, Command::BranchesMode).unwrap();
    assert!(!s.branches);
    execute(&mut s, Command::BranchSettings { taper: Some(-1.0), min_distance: Some(4.0) }).unwrap();
    assert_eq!((s.poly_settings.branch_taper, s.poly_settings.branch_min_distance), (-1.0, 4.0));
}

#[test]
fn a_branch_needs_a_path() {
    let mut s = AppState::new();
    let id = poly_box(&mut s, [0.0; 3]);
    let f = top_face(&s, id);
    assert!(execute(&mut s, Command::Branch { node: id.0, faces: vec![f.0], path: vec![[0.0, 0.0, 10.0]], taper: 0.0 }).is_err());
}
