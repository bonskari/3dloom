use glam::Vec3;
use loom_commands::{execute, AppState, Axis, CenterMode, Command, CoordSystem, PrimitiveKind, SelectOp};

fn state() -> AppState {
    AppState::new()
}

fn box_at(s: &mut AppState, x: f32) -> loom_scene::NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [x, 0.0, 0.0], size: [1.0, 1.0, 1.0] }).unwrap();
    *s.selection.iter().next().unwrap()
}

fn pos(s: &AppState, id: loom_scene::NodeId) -> Vec3 {
    s.scene.world_matrix(id).w_axis.truncate()
}

#[test]
fn view_system_in_front_view_maps_y_to_world_z() {
    let mut s = state();
    let id = box_at(&mut s, 0.0);
    execute(&mut s, Command::ActiveViewport { index: 1 }).unwrap(); // Front
    execute(&mut s, Command::MoveSelection { delta: [0.0, 1.0, 0.0], system: Some(CoordSystem::View) }).unwrap();
    assert!(pos(&s, id).abs_diff_eq(Vec3::new(0.0, 0.0, 1.0), 1e-4), "{}", pos(&s, id));
    // Perspective: View behaves as World.
    execute(&mut s, Command::ActiveViewport { index: 3 }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 1.0, 0.0], system: Some(CoordSystem::View) }).unwrap();
    assert!(pos(&s, id).abs_diff_eq(Vec3::new(0.0, 1.0, 1.0), 1e-4), "{}", pos(&s, id));
}

#[test]
fn local_system_uses_node_rotation() {
    let mut s = state();
    let id = box_at(&mut s, 0.0);
    execute(&mut s, Command::SetTransform { id: id.0, position: None, rotation_deg: Some([0.0, 0.0, 90.0]), scale: None }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [1.0, 0.0, 0.0], system: Some(CoordSystem::Local) }).unwrap();
    assert!(pos(&s, id).abs_diff_eq(Vec3::new(0.0, 1.0, 0.0), 1e-4), "{}", pos(&s, id));
}

#[test]
fn parent_system_uses_parent_frame() {
    let mut s = state();
    let p = box_at(&mut s, 0.0);
    execute(&mut s, Command::SetTransform { id: p.0, position: None, rotation_deg: Some([0.0, 0.0, 90.0]), scale: None }).unwrap();
    let c = box_at(&mut s, 2.0);
    execute(&mut s, Command::SetParent { id: c.0, parent: Some(p.0) }).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![c.0], op: SelectOp::Replace }).unwrap();
    let before = pos(&s, c);
    execute(&mut s, Command::MoveSelection { delta: [1.0, 0.0, 0.0], system: Some(CoordSystem::Parent) }).unwrap();
    assert!(pos(&s, c).abs_diff_eq(before + Vec3::new(0.0, 1.0, 0.0), 1e-4), "{}", pos(&s, c));
}

#[test]
fn pick_system_uses_picked_node() {
    let mut s = state();
    let ref_node = box_at(&mut s, 5.0);
    execute(&mut s, Command::SetTransform { id: ref_node.0, position: None, rotation_deg: Some([0.0, 0.0, 90.0]), scale: None }).unwrap();
    let id = box_at(&mut s, 0.0);
    execute(&mut s, Command::PickCoordinateSystem { id: ref_node.0 }).unwrap();
    assert_eq!(s.coord_system, CoordSystem::Pick);
    execute(&mut s, Command::MoveSelection { delta: [1.0, 0.0, 0.0], system: None }).unwrap();
    assert!(pos(&s, id).abs_diff_eq(Vec3::new(0.0, 1.0, 0.0), 1e-4), "{}", pos(&s, id));
}

#[test]
fn selection_centre_rotation_orbits_objects() {
    let mut s = state();
    let a = box_at(&mut s, -2.0);
    let b = box_at(&mut s, 2.0);
    execute(&mut s, Command::SelectNodes { ids: vec![a.0, b.0], op: SelectOp::Replace }).unwrap();
    assert_eq!(s.center_mode, CenterMode::Selection, "switches to Selection centre for multiple objects");
    execute(&mut s, Command::RotateSelection { axis: Axis::Z, degrees: 180.0, system: Some(CoordSystem::World) }).unwrap();
    assert!(pos(&s, a).abs_diff_eq(Vec3::new(2.0, 0.0, 0.0), 1e-3) && pos(&s, b).abs_diff_eq(Vec3::new(-2.0, 0.0, 0.0), 1e-3));
}

#[test]
fn pivot_centre_rotation_keeps_positions() {
    let mut s = state();
    let a = box_at(&mut s, -2.0);
    let b = box_at(&mut s, 2.0);
    execute(&mut s, Command::SelectNodes { ids: vec![a.0, b.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::UseCenter { mode: CenterMode::PivotPoint }).unwrap();
    execute(&mut s, Command::RotateSelection { axis: Axis::Z, degrees: 90.0, system: Some(CoordSystem::World) }).unwrap();
    assert!(pos(&s, a).abs_diff_eq(Vec3::new(-2.0, 0.0, 0.0), 1e-4) && pos(&s, b).abs_diff_eq(Vec3::new(2.0, 0.0, 0.0), 1e-4));
    // The centre is remembered per single/multiple selection.
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    assert_eq!(s.center_mode, CenterMode::PivotPoint);
    execute(&mut s, Command::UseCenter { mode: CenterMode::TransformCoordinate }).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![a.0, b.0], op: SelectOp::Replace }).unwrap();
    assert_eq!(s.center_mode, CenterMode::PivotPoint);
    execute(&mut s, Command::SelectNodes { ids: vec![b.0], op: SelectOp::Replace }).unwrap();
    assert_eq!(s.center_mode, CenterMode::TransformCoordinate);
}

#[test]
fn constraints_cycle_f8_xy_yz_zx() {
    let mut s = state();
    assert_eq!(format!("{:?}", s.axis_constraint), "XY");
    execute(&mut s, Command::TransformGizmoPlaneConstraintCycle).unwrap();
    assert_eq!(format!("{:?}", s.axis_constraint), "YZ");
    execute(&mut s, Command::TransformGizmoPlaneConstraintCycle).unwrap();
    assert_eq!(format!("{:?}", s.axis_constraint), "ZX");
    execute(&mut s, Command::TransformGizmoPlaneConstraintCycle).unwrap();
    assert_eq!(format!("{:?}", s.axis_constraint), "XY");
    execute(&mut s, Command::TransformGizmoXConstraint).unwrap();
    assert_eq!(format!("{:?}", s.axis_constraint), "X");
}

#[test]
fn rotation_accumulates_past_360() {
    let mut s = state();
    let id = box_at(&mut s, 0.0);
    execute(&mut s, Command::RotateSelection { axis: Axis::Z, degrees: 200.0, system: Some(CoordSystem::World) }).unwrap();
    execute(&mut s, Command::RotateSelection { axis: Axis::Z, degrees: 200.0, system: Some(CoordSystem::World) }).unwrap();
    let m = s.scene.world_matrix(id);
    let expected = glam::Mat4::from_rotation_z(400f32.to_radians());
    assert!(m.abs_diff_eq(expected, 1e-4));
    assert!((s.scene.node(id).unwrap().euler_deg().z - 400.0).abs() < 1e-3, "{}", s.scene.node(id).unwrap().euler_deg().z);
}

#[test]
fn rotate_selection_free_applies_quaternion() {
    let mut s = state();
    let id = box_at(&mut s, 0.0);
    let q = glam::Quat::from_rotation_x(30f32.to_radians());
    execute(&mut s, Command::RotateSelectionFree { quat: [q.x, q.y, q.z, q.w] }).unwrap();
    assert!(s.scene.world_matrix(id).abs_diff_eq(glam::Mat4::from_quat(q), 1e-4));
}

#[test]
fn gizmo_toggle_and_size_commands() {
    let mut s = state();
    assert!(s.prefs.gizmos.enabled);
    // Size is a percent of the viewport (1–100).
    assert_eq!(s.prefs.gizmos.size, 20.0);
    execute(&mut s, Command::TransformGizmoToggle).unwrap();
    assert!(!s.prefs.gizmos.enabled);
    execute(&mut s, Command::TransformGizmoSizeUp).unwrap();
    execute(&mut s, Command::TransformGizmoSizeUp).unwrap();
    assert_eq!(s.prefs.gizmos.size, 24.0);
    execute(&mut s, Command::TransformGizmoSizeDown).unwrap();
    assert_eq!(s.prefs.gizmos.size, 22.0);
    for _ in 0..100 {
        execute(&mut s, Command::TransformGizmoSizeUp).unwrap();
    }
    assert_eq!(s.prefs.gizmos.size, 100.0);
    execute(&mut s, Command::SetPreference { name: "Gizmo Size".into(), value: 0.0 }).unwrap();
    assert_eq!(s.prefs.gizmos.size, 1.0);
}

/// Prefs saved before the size became a percent still load, with the new default.
#[test]
fn old_pixel_gizmo_size_pref_loads() {
    let mut v = serde_json::to_value(loom_commands::Prefs::default()).unwrap();
    let g = v["gizmos"].as_object_mut().unwrap();
    g.remove("size");
    g.insert("size_px".into(), serde_json::json!(40.0));
    let p: loom_commands::Prefs = serde_json::from_value(v).unwrap();
    assert_eq!(p.gizmos.size, 20.0);
}

#[test]
fn clone_options_instance_shares_object() {
    use loom_commands::CloneKind;
    let mut s = state();
    let a = box_at(&mut s, 0.0);
    execute(&mut s, Command::CloneOptions { kind: CloneKind::Instance, copies: 1, name: None, offset: None, rotate_deg: None, scale: None }).unwrap();
    let b = *s.selection.iter().next().unwrap();
    assert_ne!(a, b);
    assert_eq!(s.scene.node(b).unwrap().instance_of, Some(a));
    execute(&mut s, Command::SetParameter { id: a.0, name: "Height".into(), value: 7.0 }).unwrap();
    let h = |s: &AppState, id| s.scene.node(id).unwrap().object.parameters().into_iter().find(|(n, _)| *n == "Height").unwrap().1;
    assert_eq!(h(&s, a), loom_scene::ParamValue::Float(7.0));
    assert_eq!(h(&s, b), loom_scene::ParamValue::Float(7.0));
    // Editing the instance edits the original too.
    execute(&mut s, Command::SetParameter { id: b.0, name: "Height".into(), value: 9.0 }).unwrap();
    assert_eq!(h(&s, a), loom_scene::ParamValue::Float(9.0));
    // A Copy is independent and lands offset copies in sequence.
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::CloneOptions { kind: CloneKind::Copy, copies: 2, name: Some("Wall".into()), offset: Some([5.0, 0.0, 0.0]), rotate_deg: None, scale: None }).unwrap();
    let ids: Vec<_> = s.selection.iter().copied().collect();
    assert_eq!(ids.len(), 2);
    let names: Vec<String> = ids.iter().map(|i| s.scene.node(*i).unwrap().name.clone()).collect();
    assert_eq!(names, vec!["Wall", "Wall001"]);
    let xs: Vec<f32> = ids.iter().map(|i| s.scene.node(*i).unwrap().transform.position.get().x).collect();
    assert_eq!(xs, vec![5.0, 10.0]);
    execute(&mut s, Command::SetParameter { id: ids[0].0, name: "Height".into(), value: 1.0 }).unwrap();
    assert_eq!(h(&s, a), loom_scene::ParamValue::Float(9.0));
    assert_eq!(s.scene.node(ids[0]).unwrap().instance_of, None);
}

#[test]
fn transform_type_in_absolute_rotation_uses_centre_mode() {
    let mut s = state();
    let a = box_at(&mut s, 10.0);
    let b = box_at(&mut s, -10.0);
    execute(&mut s, Command::SelectNodes { ids: vec![a.0, b.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::UseCenter { mode: CenterMode::Selection }).unwrap();
    execute(&mut s, Command::SelectAndRotate).unwrap();
    execute(&mut s, Command::TransformTypeIn { absolute: true, values: [0.0, 0.0, 90.0] }).unwrap();
    let pa = s.scene.node(a).unwrap().transform.position.get();
    assert!(pa.abs_diff_eq(Vec3::new(0.0, 10.0, 0.0), 1e-4), "{pa}");
    assert!((s.scene.node(a).unwrap().euler_deg().z - 90.0).abs() < 1e-3);
    // Absolute again to 90 is a no-op; offset 10 adds.
    execute(&mut s, Command::TransformTypeIn { absolute: true, values: [0.0, 0.0, 90.0] }).unwrap();
    assert!((s.scene.node(a).unwrap().euler_deg().z - 90.0).abs() < 1e-3);
    execute(&mut s, Command::TransformTypeIn { absolute: false, values: [0.0, 0.0, 10.0] }).unwrap();
    assert!((s.scene.node(a).unwrap().euler_deg().z - 100.0).abs() < 1e-3);
    // Move: absolute sets the selection centre; Scale: absolute is local percent.
    execute(&mut s, Command::SelectAndMove).unwrap();
    execute(&mut s, Command::TransformTypeIn { absolute: true, values: [0.0, 0.0, 5.0] }).unwrap();
    assert!((s.selection_pivot() - Vec3::new(0.0, 0.0, 5.0)).length() < 1e-4);
    execute(&mut s, Command::SelectAndScale).unwrap();
    execute(&mut s, Command::TransformTypeIn { absolute: true, values: [200.0, 100.0, 100.0] }).unwrap();
    assert_eq!(s.scene.node(a).unwrap().transform.scale.get(), Vec3::new(2.0, 1.0, 1.0));
    execute(&mut s, Command::TransformTypeIn { absolute: false, values: [100.0, 100.0, 50.0] }).unwrap();
    assert_eq!(s.scene.node(a).unwrap().transform.scale.get(), Vec3::new(2.0, 1.0, 0.5));
    // F12 opens the dialog, Spinner Snap toggles.
    execute(&mut s, Command::TransformTypeInDialogToggle).unwrap();
    assert_eq!(s.dialog, Some(loom_commands::Dialog::TransformTypeIn));
    execute(&mut s, Command::SpinnerSnapToggle).unwrap();
    assert!(s.prefs.spinner_snap);
    assert_eq!(s.prefs.spinner_snap_value, 1.0);
}
