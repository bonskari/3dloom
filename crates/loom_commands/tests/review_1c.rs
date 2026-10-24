//! Regression tests for the phase 1c review findings.
use glam::Vec3;
use loom_commands::{execute, AppState, Axis, CloneKind, Command, CoordSystem, PickMode, PrimitiveKind, SelectOp};
use loom_scene::NodeId;

fn state() -> AppState {
    AppState::new()
}

fn make_box(s: &mut AppState, pos: [f32; 3]) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: pos, size: [2.0, 2.0, 2.0] }).unwrap();
    *s.selection.iter().next().unwrap()
}

fn select(s: &mut AppState, ids: &[NodeId]) {
    execute(s, Command::SelectNodes { ids: ids.iter().map(|i| i.0).collect(), op: SelectOp::Replace }).unwrap();
}

#[test]
fn absolute_rotate_type_in_sets_the_typed_euler() {
    let mut s = state();
    let a = make_box(&mut s, [0.0; 3]);
    execute(&mut s, Command::SelectAndRotate).unwrap();
    execute(&mut s, Command::TransformTypeIn { absolute: true, values: [30.0, 0.0, 0.0] }).unwrap();
    execute(&mut s, Command::TransformTypeIn { absolute: true, values: [30.0, 40.0, 0.0] }).unwrap();
    let e = s.scene.node(a).unwrap().euler_deg();
    assert!(e.abs_diff_eq(Vec3::new(30.0, 40.0, 0.0), 1e-3), "{e}");
    assert!(loom_commands::type_in_values(&s).abs_diff_eq(Vec3::new(30.0, 40.0, 0.0), 1e-3));
}

#[test]
fn local_z_rotation_after_world_x_keeps_clean_euler() {
    let mut s = state();
    let a = make_box(&mut s, [0.0; 3]);
    execute(&mut s, Command::RotateSelection { axis: Axis::X, degrees: 90.0, system: Some(CoordSystem::World) }).unwrap();
    execute(&mut s, Command::RotateSelection { axis: Axis::Z, degrees: 270.0, system: Some(CoordSystem::Local) }).unwrap();
    let e = s.scene.node(a).unwrap().euler_deg();
    // A local Z rotation only changes the Z Euler component (XYZ order).
    assert!(e.abs_diff_eq(Vec3::new(90.0, 0.0, 270.0), 1e-2), "{e}");
    // A compound (non-Euler-axis) rotation falls back to the nearest wrap without inventing turns.
    execute(&mut s, Command::RotateSelectionFree { quat: glam::Quat::from_axis_angle(Vec3::new(1.0, 1.0, 0.0).normalize(), 0.5).to_array() }).unwrap();
    let e = s.scene.node(a).unwrap().euler_deg();
    assert!(e.y.abs() < 180.0 && e.z.abs() <= 360.0, "{e}");
}

#[test]
fn copy_of_an_instance_is_independent() {
    let mut s = state();
    let a = make_box(&mut s, [0.0; 3]);
    execute(&mut s, Command::CloneOptions { kind: CloneKind::Instance, copies: 1, name: None, offset: None, rotate_deg: None, scale: None }).unwrap();
    let b = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::CloneOptions { kind: CloneKind::Copy, copies: 1, name: None, offset: None, rotate_deg: None, scale: None }).unwrap();
    let c = *s.selection.iter().next().unwrap();
    assert_eq!(s.scene.node(c).unwrap().instance_of, None);
    assert_eq!(s.scene.node(b).unwrap().instance_of, Some(a));
}

#[test]
fn converting_an_instance_makes_it_unique() {
    let mut s = state();
    let a = make_box(&mut s, [0.0; 3]);
    execute(&mut s, Command::CloneOptions { kind: CloneKind::Instance, copies: 1, name: None, offset: None, rotate_deg: None, scale: None }).unwrap();
    let b = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::ConvertToEditablePoly).unwrap();
    assert_eq!(s.scene.node(b).unwrap().instance_of, None, "the converted member leaves the group");
    // The original still takes parameter edits.
    execute(&mut s, Command::SetParameter { id: a.0, name: "Length".into(), value: 5.0 }).unwrap();
    // Converting the root hands the group to another member.
    let c = make_box(&mut s, [9.0, 0.0, 0.0]);
    execute(&mut s, Command::CloneOptions { kind: CloneKind::Instance, copies: 2, name: None, offset: None, rotate_deg: None, scale: None }).unwrap();
    let members: Vec<NodeId> = s.selection.iter().copied().collect();
    select(&mut s, &[c]);
    execute(&mut s, Command::ConvertToEditablePoly).unwrap();
    assert_eq!(s.scene.node(c).unwrap().instance_of, None);
    let roots: Vec<Option<NodeId>> = members.iter().map(|m| s.scene.node(*m).unwrap().instance_of).collect();
    assert!(roots.iter().all(|r| *r != Some(c)), "{roots:?}");
    execute(&mut s, Command::SetParameter { id: members[0].0, name: "Length".into(), value: 7.0 }).unwrap();
    for m in &members {
        let len = s.scene.node(*m).unwrap().object.parameters().into_iter().find(|(n, _)| *n == "Length").unwrap().1;
        assert_eq!(len, loom_scene::ParamValue::Float(7.0));
    }
}

#[test]
fn cloning_a_hierarchy_keeps_it_together() {
    let mut s = state();
    let p = make_box(&mut s, [0.0; 3]);
    let c = make_box(&mut s, [5.0, 0.0, 0.0]);
    execute(&mut s, Command::SetParent { id: c.0, parent: Some(p.0) }).unwrap();
    select(&mut s, &[p, c]);
    execute(&mut s, Command::CloneOptions { kind: CloneKind::Copy, copies: 1, name: None, offset: Some([0.0, 10.0, 0.0]), rotate_deg: None, scale: None }).unwrap();
    let news: Vec<NodeId> = s.selection.iter().copied().collect();
    assert_eq!(news.len(), 2);
    let new_parent = news.iter().find(|n| s.scene.node(**n).unwrap().parent.is_none()).copied().expect("cloned parent");
    let new_child = news.iter().find(|n| **n != new_parent).copied().unwrap();
    assert_eq!(s.scene.node(new_child).unwrap().parent, Some(new_parent));
    assert!(s.scene.world_matrix(new_child).w_axis.truncate().abs_diff_eq(Vec3::new(5.0, 10.0, 0.0), 1e-4));
    assert_eq!(s.scene.node(c).unwrap().parent, Some(p), "the original child stays under the original parent");
}

#[test]
fn array_without_reorient_still_scales() {
    let mut s = state();
    make_box(&mut s, [0.0; 3]);
    execute(&mut s, Command::ArrayObjects { mv: [10.0, 0.0, 0.0], rotate: [0.0, 0.0, 45.0], scale: [50.0, 50.0, 50.0], totals: [false; 3], re_orient: false, uniform: true, kind: CloneKind::Copy, count: [2, 1, 1], row_2d: [0.0; 3], row_3d: [0.0; 3] }).unwrap();
    let copy = *s.selection.iter().next().unwrap();
    let n = s.scene.node(copy).unwrap();
    assert!(n.transform.scale.get().abs_diff_eq(Vec3::splat(0.5), 1e-4), "{:?}", n.transform.scale.get());
    assert_eq!(n.euler_deg(), Vec3::ZERO);
}

#[test]
fn array_rejects_non_positive_scale() {
    let mut s = state();
    make_box(&mut s, [0.0; 3]);
    for (scale, totals) in [([-100.0, 100.0, 100.0], [false, false, true]), ([0.0, 100.0, 100.0], [false; 3])] {
        let r = execute(&mut s, Command::ArrayObjects { mv: [0.0; 3], rotate: [0.0; 3], scale, totals, re_orient: true, uniform: false, kind: CloneKind::Copy, count: [3, 1, 1], row_2d: [0.0; 3], row_3d: [0.0; 3] });
        assert!(r.is_err(), "{scale:?}");
    }
    assert_eq!(s.scene.nodes().count(), 1);
}

#[test]
fn face_snap_takes_the_nearest_face_along_the_ray() {
    use loom_commands::snap::{self, SnapKind, SnapQuery};
    let mut s = state();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0, 0.0, 0.0], size: [10.0, 10.0, 10.0] }).unwrap();
    s.snaps_on = true;
    s.snaps.targets = snap::SnapTargets::none();
    s.snaps.targets.face = true;
    let project = |w: Vec3| Some(glam::Vec2::new(w.x, -w.y));
    let q = SnapQuery { pointer: glam::Vec2::new(1.0, -1.0), ray: (Vec3::new(1.0, 1.0, 100.0), -Vec3::Z), project: &project, plane: (Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::Z), start: None, exclude: &[], exclude_vertices: &[] };
    let hit = snap::resolve(&s, &q).expect("face hit");
    assert_eq!(hit.kind, SnapKind::Face);
    assert!((hit.point.z - 10.0).abs() < 1e-4, "top face, not the bottom: {:?}", hit.point);
}

#[test]
fn undo_force_closes_a_leaked_transaction() {
    let mut s = state();
    let a = make_box(&mut s, [0.0; 3]);
    s.undo.begin("Drag");
    s.undo.begin("Nested");
    execute(&mut s, Command::MoveSelection { delta: [1.0, 0.0, 0.0], system: Some(CoordSystem::World) }).unwrap();
    // A drag whose release was lost: Undo must revert the move, not the creation.
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(s.scene.node(a).is_some());
    assert_eq!(s.scene.node(a).unwrap().transform.position.get(), Vec3::ZERO);
    make_box(&mut s, [3.0, 0.0, 0.0]);
    assert_eq!(s.undo.labels().last().map(String::as_str), Some("Create Box002"), "later commits are not absorbed");
}

#[test]
fn spacing_pick_points_is_a_command() {
    let mut s = state();
    make_box(&mut s, [0.0; 3]);
    execute(&mut s, Command::SpacingPickPoints).unwrap();
    assert_eq!(s.pick_mode, Some(PickMode::SpacingPointA));
    assert_eq!(s.spacing_points, [None, None]);
}
