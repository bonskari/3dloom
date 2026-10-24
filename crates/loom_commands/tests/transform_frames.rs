//! Transform managers per tool, Gimbal, Pick from the list, Auto Key
//! centre, sub-object Local, Slice Plane type-in, the Hierarchy panel's
//! pivot / transform tools and Reset XForm.

use glam::{Mat4, Quat, Vec3};
use loom_commands::{execute, AppState, Axis, CenterMode, Command, CoordSystem, PickMode, PrimitiveKind, SelectOp, SelectionMode};
use loom_commands::AxisConstraint;
use loom_scene::{Modifier, NodeId};

fn box_at(s: &mut AppState, x: f32) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [x, 0.0, 0.0], size: [2.0, 2.0, 2.0] }).unwrap();
    *s.selection.iter().next().unwrap()
}

fn pos(s: &AppState, id: NodeId) -> Vec3 {
    s.scene.world_matrix(id).w_axis.truncate()
}

fn select(s: &mut AppState, ids: &[NodeId]) {
    execute(s, Command::SelectNodes { ids: ids.iter().map(|i| i.0).collect(), op: SelectOp::Replace }).unwrap();
}

fn rot(s: &mut AppState, id: NodeId, deg: [f32; 3]) {
    execute(s, Command::SetTransform { id: id.0, position: None, rotation_deg: Some(deg), scale: None }).unwrap();
}

fn world_rot(s: &AppState, id: NodeId) -> Quat {
    s.scene.world_matrix(id).to_scale_rotation_translation().1
}

/// World positions of the evaluated mesh's vertices.
fn world_verts(s: &AppState, id: NodeId) -> Vec<Vec3> {
    let m = s.scene.object_matrix(id);
    s.scene.evaluated(id, None).value.mesh.vertices().iter().map(|v| m.transform_point3(v.position)).collect()
}

fn same_points(a: &[Vec3], b: &[Vec3]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(p, q)| p.abs_diff_eq(*q, 1e-3))
}

#[test]
fn transform_managers_are_remembered_per_transform_tool() {
    let mut s = AppState::new();
    box_at(&mut s, 0.0);
    execute(&mut s, Command::SelectAndMove).unwrap();
    execute(&mut s, Command::UseCenter { mode: CenterMode::Selection }).unwrap();
    execute(&mut s, Command::TransformGizmoYConstraint).unwrap();
    execute(&mut s, Command::SelectAndRotate).unwrap();
    assert_eq!(s.center_mode, CenterMode::PivotPoint, "Rotate has its own centre");
    assert_eq!(s.axis_constraint, AxisConstraint::XY, "and its own constraint");
    execute(&mut s, Command::UseCenter { mode: CenterMode::TransformCoordinate }).unwrap();
    execute(&mut s, Command::TransformGizmoZConstraint).unwrap();
    execute(&mut s, Command::SelectAndMove).unwrap();
    assert_eq!((s.center_mode, s.axis_constraint), (CenterMode::Selection, AxisConstraint::Y));
    execute(&mut s, Command::SelectAndRotate).unwrap();
    assert_eq!((s.center_mode, s.axis_constraint), (CenterMode::TransformCoordinate, AxisConstraint::Z));
    // Preferences > Constant keeps one set for every transform.
    execute(&mut s, Command::SetPreference { name: "Reference Coordinate System Constant".into(), value: 1.0 }).unwrap();
    execute(&mut s, Command::SelectAndMove).unwrap();
    assert_eq!((s.center_mode, s.axis_constraint), (CenterMode::TransformCoordinate, AxisConstraint::Z));
}

#[test]
fn gimbal_rotation_changes_only_its_own_euler_track() {
    let mut s = AppState::new();
    let id = box_at(&mut s, 0.0);
    rot(&mut s, id, [30.0, 40.0, 50.0]);
    for (axis, i) in [(Axis::X, 0), (Axis::Y, 1), (Axis::Z, 2)] {
        let before = s.scene.node(id).unwrap().euler_deg();
        execute(&mut s, Command::RotateSelection { axis, degrees: 15.0, system: Some(CoordSystem::Gimbal) }).unwrap();
        let after = s.scene.node(id).unwrap().euler_deg();
        let mut expected = before;
        expected[i] += 15.0;
        assert!(after.abs_diff_eq(expected, 1e-2), "{axis:?}: {before} -> {after}");
    }
}

#[test]
fn gimbal_move_and_scale_use_the_parent_axes() {
    let mut s = AppState::new();
    let id = box_at(&mut s, 0.0);
    rot(&mut s, id, [0.0, 0.0, 90.0]);
    execute(&mut s, Command::MoveSelection { delta: [1.0, 0.0, 0.0], system: Some(CoordSystem::Gimbal) }).unwrap();
    assert!(pos(&s, id).abs_diff_eq(Vec3::X, 1e-4), "Gimbal move = Parent (world here): {}", pos(&s, id));
}

#[test]
fn choosing_pick_in_the_list_picks_an_object_and_keeps_four_recent_names() {
    let mut s = AppState::new();
    let ids: Vec<NodeId> = (0..5).map(|i| box_at(&mut s, i as f32 * 5.0)).collect();
    rot(&mut s, ids[0], [0.0, 0.0, 90.0]);
    execute(&mut s, Command::SelectAndMove).unwrap();
    execute(&mut s, Command::ReferenceCoordinateSystem { system: CoordSystem::Pick }).unwrap();
    assert_eq!(s.pick_mode, Some(PickMode::CoordSystem), "Pick waits for a click");
    execute(&mut s, Command::PickHit { id: ids[0].0 }).unwrap();
    assert_eq!(s.pick_mode, None);
    assert_eq!((s.coord_system, s.pick_coord_node), (CoordSystem::Pick, Some(ids[0])));
    for id in &ids[1..] {
        execute(&mut s, Command::ReferenceCoordinateSystem { system: CoordSystem::Pick }).unwrap();
        execute(&mut s, Command::PickHit { id: id.0 }).unwrap();
    }
    assert_eq!(s.pick_coord_recent, vec![ids[4], ids[3], ids[2], ids[1]], "the four most recent, newest first");
    // Choosing a remembered name uses it again.
    execute(&mut s, Command::PickCoordinateSystem { id: ids[2].0 }).unwrap();
    assert_eq!(s.pick_coord_recent[0], ids[2]);
    assert_eq!(s.pick_coord_node, Some(ids[2]));
}

#[test]
fn auto_key_rotate_uses_each_pivot() {
    let mut s = AppState::new();
    let a = box_at(&mut s, -5.0);
    let b = box_at(&mut s, 5.0);
    select(&mut s, &[a, b]);
    execute(&mut s, Command::SelectAndRotate).unwrap();
    execute(&mut s, Command::UseCenter { mode: CenterMode::Selection }).unwrap();
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::SetFrame { frame: 10 }).unwrap();
    execute(&mut s, Command::RotateSelection { axis: Axis::Z, degrees: 90.0, system: Some(CoordSystem::World) }).unwrap();
    assert!(pos(&s, a).abs_diff_eq(Vec3::new(-5.0, 0.0, 0.0), 1e-3) && pos(&s, b).abs_diff_eq(Vec3::new(5.0, 0.0, 0.0), 1e-3), "Auto Key forces Use Pivot Point");
    // Preferences > Animation > Local Center During Animate off: the flyout's centre applies.
    execute(&mut s, Command::SetPreference { name: "Local Center During Animate".into(), value: 0.0 }).unwrap();
    execute(&mut s, Command::RotateSelection { axis: Axis::Z, degrees: 90.0, system: Some(CoordSystem::World) }).unwrap();
    assert!(pos(&s, a).abs_diff_eq(Vec3::new(0.0, -5.0, 0.0), 1e-3), "{}", pos(&s, a));
}

/// Local at a sub-object level: Z is the selection's normal.
#[test]
fn local_at_sub_object_level_uses_the_selection_normal() {
    let mut s = AppState::new();
    let id = box_at(&mut s, 0.0);
    execute(&mut s, Command::ConvertToEditablePoly).unwrap();
    rot(&mut s, id, [0.0, 30.0, 0.0]);
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Face }).unwrap();
    let mesh = s.scene.node(id).unwrap().cage_mesh().into_owned();
    let face = (0..mesh.face_count() as u32).find(|f| mesh.face_normal(loom_mesh::FaceId(*f)).z > 0.9).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![face], op: SelectOp::Replace }).unwrap();
    let f = loom_commands::transform::frame_for_system(&s, s.active_viewport, None, CoordSystem::Local);
    let expected = s.scene.object_matrix(id).transform_vector3(Vec3::Z).normalize();
    assert!(f.axes.z_axis.abs_diff_eq(expected, 1e-4), "{} vs {expected}", f.axes.z_axis);
    // Moving 1 along local Z moves the face out along its normal.
    let centre = s.selection_pivot();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 1.0], system: Some(CoordSystem::Local) }).unwrap();
    assert!((s.selection_pivot() - centre).abs_diff_eq(expected, 1e-3));
}

/// The tutorial types 90 in the status bar's Z with Rotate to stand the Slice Plane up.
#[test]
fn status_bar_rotate_type_in_rotates_the_slice_plane_only() {
    let mut s = AppState::new();
    let id = box_at(&mut s, 0.0);
    execute(&mut s, Command::ConvertToEditablePoly).unwrap();
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Face }).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SlicePlaneMode).unwrap();
    let node_before = s.scene.node(id).unwrap().transform.clone();
    let origin = s.slice_plane.origin;
    execute(&mut s, Command::SelectAndRotate).unwrap();
    execute(&mut s, Command::TransformTypeIn { absolute: true, values: [0.0, 0.0, 90.0] }).unwrap();
    assert!(s.slice_plane.rotation.abs_diff_eq(Quat::from_rotation_z(90f32.to_radians()), 1e-4), "{:?}", s.slice_plane.rotation);
    assert!(s.slice_plane.origin.abs_diff_eq(origin, 1e-4), "rotates about its own centre");
    assert_eq!(s.scene.node(id).unwrap().transform, node_before, "the object does not rotate");
    assert!(loom_commands::type_in_values(&s).abs_diff_eq(Vec3::new(0.0, 0.0, 90.0), 1e-3), "the fields show the plane");
    // Absolute Move places the plane.
    execute(&mut s, Command::SelectAndMove).unwrap();
    execute(&mut s, Command::TransformTypeIn { absolute: true, values: [3.0, 0.0, 1.0] }).unwrap();
    assert!(s.slice_plane.origin.abs_diff_eq(Vec3::new(3.0, 0.0, 1.0), 1e-4));
    assert_eq!(s.scene.node(id).unwrap().transform, node_before);
}

fn parent_and_child(s: &mut AppState) -> (NodeId, NodeId) {
    let p = box_at(s, 0.0);
    let c = box_at(s, 10.0);
    execute(s, Command::SetParent { id: c.0, parent: Some(p.0) }).unwrap();
    select(s, &[p]);
    (p, c)
}

#[test]
fn affect_pivot_only_ignores_scale() {
    let mut s = AppState::new();
    let id = box_at(&mut s, 0.0);
    execute(&mut s, Command::AffectPivotOnly).unwrap();
    let before = s.scene.node(id).unwrap().clone();
    execute(&mut s, Command::ScaleSelection { factors: [2.0, 2.0, 2.0], system: None }).unwrap();
    let after = s.scene.node(id).unwrap();
    assert_eq!((&after.transform, &after.object_offset), (&before.transform, &before.object_offset));
}

#[test]
fn affect_hierarchy_only_rotates_the_links_not_the_objects() {
    let mut s = AppState::new();
    let (p, c) = parent_and_child(&mut s);
    execute(&mut s, Command::AffectHierarchyOnly).unwrap();
    assert!(s.affect_hierarchy_only && !s.affect_pivot_only && !s.affect_object_only);
    execute(&mut s, Command::AffectPivotOnly).unwrap();
    assert!(!s.affect_hierarchy_only, "the three are exclusive");
    execute(&mut s, Command::AffectHierarchyOnly).unwrap();
    execute(&mut s, Command::RotateSelection { axis: Axis::Z, degrees: 90.0, system: Some(CoordSystem::World) }).unwrap();
    assert!(world_rot(&s, p).abs_diff_eq(Quat::IDENTITY, 1e-4), "the parent keeps its orientation");
    assert!(pos(&s, c).abs_diff_eq(Vec3::new(0.0, 10.0, 0.0), 1e-3), "the child swings round: {}", pos(&s, c));
    assert!(world_rot(&s, c).abs_diff_eq(Quat::IDENTITY, 1e-4), "the child keeps its orientation");
    execute(&mut s, Command::ScaleSelection { factors: [0.5, 0.5, 0.5], system: Some(CoordSystem::World) }).unwrap();
    assert!(pos(&s, c).abs_diff_eq(Vec3::new(0.0, 5.0, 0.0), 1e-3), "scale shortens the link: {}", pos(&s, c));
    assert!(s.scene.world_matrix(c).to_scale_rotation_translation().0.abs_diff_eq(Vec3::ONE, 1e-4), "the child keeps its size");
    let p_before = pos(&s, p);
    execute(&mut s, Command::MoveSelection { delta: [1.0, 0.0, 0.0], system: Some(CoordSystem::World) }).unwrap();
    assert_eq!(pos(&s, p), p_before, "Affect Hierarchy Only does not move");
}

#[test]
fn alignment_buttons_follow_the_affect_mode() {
    let mut s = AppState::new();
    let id = box_at(&mut s, 0.0);
    // Turn the mesh 45° and move it off the pivot (Affect Object Only).
    execute(&mut s, Command::AffectObjectOnly).unwrap();
    execute(&mut s, Command::RotateSelection { axis: Axis::Z, degrees: 45.0, system: Some(CoordSystem::World) }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [3.0, 0.0, 0.0], system: Some(CoordSystem::World) }).unwrap();
    // Affect Object Only: Center to Pivot / Align to Pivot move the object.
    execute(&mut s, Command::CenterObjectToPivot).unwrap();
    let (lo, hi) = s.object_bounds(id).unwrap();
    assert!(((lo + hi) * 0.5).abs_diff_eq(pos(&s, id), 1e-3), "the object's centre is at the pivot");
    execute(&mut s, Command::AlignObjectToPivot).unwrap();
    let off = &s.scene.node(id).unwrap().object_offset;
    assert!(off.rotation_quat().abs_diff_eq(Quat::IDENTITY, 1e-4), "object axes = pivot axes");
    // Pivot turned 30° alone, then Affect Object Only > Align to World turns the object back to the world.
    execute(&mut s, Command::AffectPivotOnly).unwrap();
    execute(&mut s, Command::RotateSelection { axis: Axis::Z, degrees: 30.0, system: Some(CoordSystem::World) }).unwrap();
    execute(&mut s, Command::AffectObjectOnly).unwrap();
    let pivot_rot = world_rot(&s, id);
    execute(&mut s, Command::AlignPivotToWorld).unwrap();
    assert!(world_rot(&s, id).abs_diff_eq(pivot_rot, 1e-4), "the pivot stays");
    let obj_rot = s.scene.object_matrix(id).to_scale_rotation_translation().1;
    assert!(obj_rot.abs_diff_eq(Quat::IDENTITY, 1e-4), "the object is world aligned");
    // Affect Pivot Only > Align to Object: the pivot turns to the object's axes.
    execute(&mut s, Command::AffectObjectOnly).unwrap();
    execute(&mut s, Command::RotateSelection { axis: Axis::Z, degrees: 20.0, system: Some(CoordSystem::World) }).unwrap();
    execute(&mut s, Command::AffectPivotOnly).unwrap();
    let mesh_before = world_verts(&s, id);
    execute(&mut s, Command::AlignPivotToObject).unwrap();
    let obj_rot = s.scene.object_matrix(id).to_scale_rotation_translation().1;
    assert!(world_rot(&s, id).abs_diff_eq(obj_rot, 1e-4), "pivot axes = object axes");
    assert!(same_points(&world_verts(&s, id), &mesh_before), "the mesh stays");
}

#[test]
fn dont_affect_children_keeps_the_children_in_place() {
    let mut s = AppState::new();
    let (_p, c) = parent_and_child(&mut s);
    execute(&mut s, Command::DontAffectChildren).unwrap();
    assert!(s.dont_affect_children);
    let before = s.scene.world_matrix(c);
    execute(&mut s, Command::MoveSelection { delta: [0.0, 4.0, 0.0], system: Some(CoordSystem::World) }).unwrap();
    execute(&mut s, Command::RotateSelection { axis: Axis::Z, degrees: 30.0, system: Some(CoordSystem::World) }).unwrap();
    execute(&mut s, Command::ScaleSelection { factors: [2.0, 2.0, 2.0], system: Some(CoordSystem::World) }).unwrap();
    assert!(s.scene.world_matrix(c).abs_diff_eq(before, 1e-3));
}

#[test]
fn reset_transform_and_reset_scale_keep_the_look_and_the_children() {
    let mut s = AppState::new();
    let (p, c) = parent_and_child(&mut s);
    rot(&mut s, p, [0.0, 0.0, 45.0]);
    execute(&mut s, Command::SetTransform { id: p.0, position: None, rotation_deg: None, scale: Some([2.0, 1.0, 1.0]) }).unwrap();
    let (mesh, child) = (world_verts(&s, p), s.scene.world_matrix(c));
    execute(&mut s, Command::ResetScale).unwrap();
    assert_eq!(s.scene.node(p).unwrap().transform.scale.get(), Vec3::ONE);
    assert!(same_points(&world_verts(&s, p), &mesh), "no visual change");
    assert!(s.scene.world_matrix(c).abs_diff_eq(child, 1e-3));
    execute(&mut s, Command::ResetTransform).unwrap();
    assert!(world_rot(&s, p).abs_diff_eq(Quat::IDENTITY, 1e-4), "local axes = world axes");
    assert!(same_points(&world_verts(&s, p), &mesh));
    assert!(s.scene.world_matrix(c).abs_diff_eq(child, 1e-3), "descendants stay");
}

#[test]
fn reset_xform_puts_rotation_and_scale_in_an_xform_modifier() {
    let mut s = AppState::new();
    let id = box_at(&mut s, 4.0);
    rot(&mut s, id, [0.0, 0.0, 30.0]);
    execute(&mut s, Command::SetTransform { id: id.0, position: None, rotation_deg: None, scale: Some([2.0, 1.0, 3.0]) }).unwrap();
    let base_before = s.scene.node(id).unwrap().object.clone();
    let mesh = world_verts(&s, id);
    execute(&mut s, Command::ResetXForm).unwrap();
    let node = s.scene.node(id).unwrap();
    assert_eq!(node.object, base_before, "the base object is untouched");
    assert!(matches!(node.modifiers.last().map(|m| &m.modifier), Some(Modifier::XForm { .. })), "XForm on top of the stack");
    assert_eq!((node.transform.rotation.get(), node.transform.scale.get()), (Vec3::ZERO, Vec3::ONE));
    assert!(s.scene.world_matrix(id).abs_diff_eq(Mat4::from_translation(Vec3::new(4.0, 0.0, 0.0)), 1e-4));
    assert!(same_points(&world_verts(&s, id), &mesh), "no visual change");
}

/// Hierarchy > Working Pivot: Edit Working Pivot moves the working pivot with
/// the transform tools; Use Working Pivot rotates the selection about it in
/// its axes (the Working system, Use Transform Coordinate Center).
#[test]
fn working_pivot_edit_then_use() {
    let mut s = AppState::new();
    let id = box_at(&mut s, 10.0);
    execute(&mut s, Command::EditWorkingPivot).unwrap();
    assert!(s.edit_working_pivot);
    assert_eq!(s.coord_system, CoordSystem::Local, "editing starts in Local");
    execute(&mut s, Command::MoveSelection { delta: [0.0, 5.0, 0.0], system: Some(CoordSystem::World) }).unwrap();
    assert!(pos(&s, id).abs_diff_eq(Vec3::new(10.0, 0.0, 0.0), 1e-4), "the object stays while editing");
    assert!(s.working_pivot.origin.abs_diff_eq(Vec3::new(0.0, 5.0, 0.0), 1e-4));
    execute(&mut s, Command::RotateSelection { axis: Axis::Z, degrees: 90.0, system: Some(CoordSystem::World) }).unwrap();
    assert!(s.working_pivot.origin.abs_diff_eq(Vec3::new(0.0, 5.0, 0.0), 1e-4), "rotates about itself");
    execute(&mut s, Command::EditWorkingPivot).unwrap();
    assert!(!s.edit_working_pivot);
    execute(&mut s, Command::SelectAndRotate).unwrap();
    execute(&mut s, Command::UseWorkingPivot).unwrap();
    assert!(s.use_working_pivot);
    assert_eq!((s.coord_system, s.center_mode), (CoordSystem::Working, CenterMode::TransformCoordinate));
    // 90° about the working pivot's Z (world Z here): (10,0,0) swings round (0,5,0).
    execute(&mut s, Command::RotateSelection { axis: Axis::Z, degrees: 90.0, system: None }).unwrap();
    assert!(pos(&s, id).abs_diff_eq(Vec3::new(5.0, 15.0, 0.0), 1e-3), "{}", pos(&s, id));
    // Moving in the Working system follows its X (world Y after the 90° turn).
    execute(&mut s, Command::SelectAndMove).unwrap();
    assert_eq!(s.coord_system, CoordSystem::Working, "Use Working Pivot holds for every transform");
    execute(&mut s, Command::MoveSelection { delta: [1.0, 0.0, 0.0], system: None }).unwrap();
    assert!(pos(&s, id).abs_diff_eq(Vec3::new(5.0, 16.0, 0.0), 1e-3), "{}", pos(&s, id));
    execute(&mut s, Command::UseWorkingPivot).unwrap();
    assert_eq!(s.coord_system, CoordSystem::View, "leaving restores the tool's own system");
}

#[test]
fn working_pivot_reset_and_place() {
    let mut s = AppState::new();
    let id = box_at(&mut s, 7.0);
    execute(&mut s, Command::WorkingPivotReset).unwrap();
    assert!(s.working_pivot.origin.abs_diff_eq(pos(&s, id), 1e-4), "Reset goes to the selected pivot");
    // Place Pivot To Surface: the clicked point, Z along the normal.
    execute(&mut s, Command::PlacePivotToSurface).unwrap();
    assert_eq!(s.pick_mode, Some(PickMode::WorkingPivotSurface));
    execute(&mut s, Command::PickSurface { id: id.0, point: [7.0, -1.0, 1.0], normal: [0.0, -1.0, 0.0] }).unwrap();
    assert_eq!(s.pick_mode, None);
    assert!(s.working_pivot.origin.abs_diff_eq(Vec3::new(7.0, -1.0, 1.0), 1e-4));
    assert!((s.working_pivot.rotation * Vec3::Z).abs_diff_eq(-Vec3::Y, 1e-4));
    // Align To View in the Top view: the XY plane faces the view.
    execute(&mut s, Command::ActiveViewport { index: 0 }).unwrap();
    assert_eq!(s.viewports[0].kind, loom_commands::ViewKind::Top);
    execute(&mut s, Command::WorkingPivotAlignToView).unwrap();
    assert!((s.working_pivot.rotation * Vec3::Z).abs_diff_eq(Vec3::Z, 1e-4));
}
