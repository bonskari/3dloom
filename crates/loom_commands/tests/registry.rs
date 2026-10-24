use loom_commands::{execute, execute_json, registry, AppState, Command, CoordSystem, Group, PrimitiveKind, SelectOp};
use serde_json::json;

fn state() -> AppState {
    AppState::new()
}

fn create_box(s: &mut AppState) {
    execute(
        s,
        Command::CreatePrimitive {
            kind: PrimitiveKind::Box,
            position: [0.0, 0.0, 0.0],
            size: [1.0, 1.0, 1.0],
        },
    )
    .unwrap();
}

#[test]
fn every_variant_is_in_registry_and_roundtrips_json() {
    let reg = registry();
    assert!(reg.len() > 40, "registry has {} entries", reg.len());
    for info in reg {
        assert!(!info.description.is_empty(), "{} lacks description", info.name);
        assert!(info.args_schema.is_object(), "{} schema", info.name);
        // Unit commands must deserialize from just their name.
        if info.args_schema["properties"].as_object().map_or(true, |p| p.is_empty()) {
            let cmd: Command = serde_json::from_value(json!({ "cmd": info.name })).unwrap();
            let back = serde_json::to_value(&cmd).unwrap();
            assert_eq!(back["cmd"], info.name);
        }
    }
    let names: std::collections::BTreeSet<_> = reg.iter().map(|i| i.name).collect();
    assert_eq!(names.len(), reg.len(), "duplicate command names");
    assert!(reg.iter().any(|i| i.name == "Select and Move" && i.group == Group::MainUI));
    assert!(reg.iter().any(|i| i.name == "Create Primitive" && i.group == Group::Loom));
}

#[test]
fn delete_with_empty_selection_pushes_no_undo() {
    let mut s = state();
    execute(&mut s, Command::DeleteObjects).unwrap();
    assert!(!s.undo.can_undo());
}

#[test]
fn create_box_then_undo_removes_it() {
    let mut s = state();
    create_box(&mut s);
    assert_eq!(s.scene.len(), 1);
    assert_eq!(s.scene.nodes().next().unwrap().name, "Box001");
    assert_eq!(s.selection.len(), 1, "new object is selected");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.len(), 0);
    assert!(s.selection.is_empty());
}

#[test]
fn move_selection_is_undoable_in_one_step() {
    let mut s = state();
    create_box(&mut s);
    execute(&mut s, Command::MoveSelection { delta: [2.0, 0.0, 0.0], system: Some(CoordSystem::World) }).unwrap();
    let id = *s.selection.iter().next().unwrap();
    assert_eq!(s.scene.node(id).unwrap().transform.position.get().x, 2.0);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.node(id).unwrap().transform.position.get().x, 0.0);
    assert_eq!(s.scene.len(), 1, "one undo step undoes only the move");
}

#[test]
fn clone_names_box002() {
    let mut s = state();
    create_box(&mut s);
    execute(&mut s, Command::Clone).unwrap();
    assert_eq!(s.dialog, Some(loom_commands::Dialog::CloneOptions));
    execute(&mut s, Command::CloneOptions { kind: loom_commands::CloneKind::Copy, copies: 1, name: None, offset: None, rotate_deg: None, scale: None }).unwrap();
    let mut names: Vec<_> = s.scene.nodes().map(|n| n.name.clone()).collect();
    names.sort();
    assert_eq!(names, vec!["Box001", "Box002"]);
}

#[test]
fn execute_json_bad_args_returns_error_not_panic() {
    let mut s = state();
    let r = execute_json(&mut s, &json!({ "cmd": "Move Selection", "args": { "delta": "no" } }));
    assert_eq!(r["ok"], false);
    assert!(r["error"].as_str().unwrap().len() > 3);
    let r = execute_json(&mut s, &json!({ "cmd": "No Such Command" }));
    assert_eq!(r["ok"], false);
    let r = execute_json(&mut s, &json!({ "cmd": "State" }));
    assert_eq!(r["ok"], true);
    let r = execute_json(&mut s, &json!({ "cmd": "State", "args": {} }));
    assert_eq!(r["ok"], true, "empty args object must be accepted for unit commands: {r}");
    let r = execute_json(&mut s, &json!({ "cmd": "Select All", "args": null }));
    assert_eq!(r["ok"], true, "{r}");
}

#[test]
fn view_commands_set_view_kind_and_viewport_undo() {
    let mut s = state();
    execute(&mut s, Command::TopView).unwrap();
    assert_eq!(format!("{:?}", s.active().kind), "Top");
    execute(&mut s, Command::FrontView).unwrap();
    execute(&mut s, Command::UndoViewportOperation).unwrap();
    assert_eq!(format!("{:?}", s.active().kind), "Top");
    assert!(!s.undo.can_undo(), "view changes are not scene undo");
}

#[test]
fn hide_selection_and_unhide_all() {
    let mut s = state();
    create_box(&mut s);
    execute(&mut s, Command::HideSelection).unwrap();
    assert!(s.scene.nodes().all(|n| n.hidden));
    assert!(s.selection.is_empty());
    execute(&mut s, Command::UnhideAll).unwrap();
    assert!(s.scene.nodes().all(|n| !n.hidden));
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(s.scene.nodes().all(|n| n.hidden));
}

#[test]
fn select_sub_subtract_removes_ids() {
    let mut s = state();
    create_box(&mut s);
    let id = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::SelectionMode { mode: loom_commands::SelectionMode::Face }).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![0, 1, 2], op: SelectOp::Replace }).unwrap();
    assert_eq!(s.sub_selection.len(), 3);
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![1], op: SelectOp::Subtract }).unwrap();
    assert_eq!(s.sub_selection.len(), 2);
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![5], op: SelectOp::Add }).unwrap();
    assert_eq!(s.sub_selection.len(), 3);
}

#[test]
fn test_room_creates_five_editable_polys() {
    let mut s = state();
    execute(&mut s, Command::TestRoom).unwrap();
    assert_eq!(s.scene.len(), 5);
    assert!(s.scene.nodes().all(|n| matches!(n.object, loom_scene::ObjectKind::EditablePoly(_))));
}

#[test]
fn extrude_converts_parametric_box_to_editable_poly() {
    let mut s = state();
    create_box(&mut s);
    let id = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::SelectFacesFacing { node: Some(id.0), direction: [0.0, 0.0, 1.0], tolerance: 0.9 }).unwrap();
    execute(&mut s, Command::ExtrudeFaces { distance: 1.0 }).unwrap();
    let node = s.scene.node(id).unwrap();
    assert!(matches!(node.object, loom_scene::ObjectKind::EditablePoly(_)));
    assert_eq!(node.object.mesh().face_count(), 10);
    assert_eq!(node.object.mesh().aabb().1.z, 2.0);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(matches!(s.scene.node(id).unwrap().object, loom_scene::ObjectKind::Box { .. }));
}

#[test]
fn hotkey_runs_the_bound_command() {
    let mut s = state();
    execute(&mut s, Command::Hotkey { chord: "W".into() }).unwrap();
    assert_eq!(s.active_tool, loom_commands::Tool::Move);
    execute(&mut s, Command::Hotkey { chord: "T".into() }).unwrap();
    assert_eq!(s.active().kind, loom_commands::ViewKind::Top);
    assert!(execute(&mut s, Command::Hotkey { chord: "Shift+Q".into() }).is_err(), "unbound chord is an error");
}

fn box_at(s: &mut AppState, x: f32) -> loom_scene::NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [x, 0.0, 0.0], size: [1.0, 1.0, 1.0] }).unwrap();
    *s.selection.iter().next().unwrap()
}

#[test]
fn link_undo_restores_child_position() {
    let mut s = state();
    let p = box_at(&mut s, 5.0);
    let c = box_at(&mut s, 0.0);
    execute(&mut s, Command::SetParent { id: c.0, parent: Some(p.0) }).unwrap();
    assert_eq!(s.scene.node(c).unwrap().transform.position.get().x, -5.0);
    assert!(s.scene.world_matrix(c).w_axis.x.abs() < 1e-5);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.node(c).unwrap().parent, None);
    assert!(s.scene.node(c).unwrap().transform.position.get().x.abs() < 1e-5, "{:?}", s.scene.node(c).unwrap().transform.position.get());
}

#[test]
fn delete_parent_undo_restores_hierarchy() {
    let mut s = state();
    let p = box_at(&mut s, 5.0);
    let c = box_at(&mut s, 0.0);
    execute(&mut s, Command::SetParent { id: c.0, parent: Some(p.0) }).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![p.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::DeleteObjects).unwrap();
    assert_eq!(s.scene.node(c).unwrap().parent, None);
    assert!(s.scene.world_matrix(c).w_axis.x.abs() < 1e-5, "child keeps world position after parent delete");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.node(c).unwrap().parent, Some(p));
    assert_eq!(s.scene.node(c).unwrap().transform.position.get().x, -5.0);
    execute(&mut s, Command::NewScene).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.node(c).unwrap().parent, Some(p));
}

#[test]
fn long_drag_is_one_undo_step_and_keeps_history() {
    let mut s = state();
    let id = box_at(&mut s, 0.0);
    s.undo.begin("Move");
    for _ in 0..150 {
        execute(&mut s, Command::MoveSelection { delta: [0.01, 0.0, 0.0], system: Some(CoordSystem::World) }).unwrap();
    }
    s.undo.end();
    assert_eq!(s.undo.labels(), vec!["Create Box001", "Move"]);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(s.scene.node(id).unwrap().transform.position.get().x.abs() < 1e-5);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.len(), 0);
}

#[test]
fn moving_parent_and_child_together_moves_child_once() {
    let mut s = state();
    let p = box_at(&mut s, 0.0);
    let c = box_at(&mut s, 2.0);
    execute(&mut s, Command::SetParent { id: c.0, parent: Some(p.0) }).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![p.0, c.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [1.0, 0.0, 0.0], system: Some(CoordSystem::World) }).unwrap();
    assert!((s.scene.world_matrix(c).w_axis.x - 3.0).abs() < 1e-5, "{}", s.scene.world_matrix(c).w_axis.x);
}

#[test]
fn delete_and_move_at_sub_object_level_with_nothing_selected_are_no_ops() {
    let mut s = state();
    box_at(&mut s, 0.0);
    execute(&mut s, Command::SelectionMode { mode: loom_commands::SelectionMode::Face }).unwrap();
    let before = s.undo.labels().len();
    execute(&mut s, Command::DeleteObjects).unwrap();
    assert_eq!(s.scene.len(), 1);
    execute(&mut s, Command::MoveSelection { delta: [1.0, 0.0, 0.0], system: Some(CoordSystem::World) }).unwrap();
    assert_eq!(s.scene.nodes().next().unwrap().transform.position.get().x, 0.0);
    assert_eq!(s.undo.labels().len(), before);
}

#[test]
fn undo_clears_stale_sub_selection() {
    let mut s = state();
    let id = box_at(&mut s, 0.0);
    execute(&mut s, Command::SelectFacesFacing { node: Some(id.0), direction: [0.0, 0.0, 1.0], tolerance: 0.9 }).unwrap();
    execute(&mut s, Command::ExtrudeFaces { distance: 1.0 }).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(s.sub_selection.is_empty());
}

#[test]
fn optional_only_struct_commands_accept_empty_args() {
    let mut s = state();
    let r = execute_json(&mut s, &json!({ "cmd": "Orbit", "args": {} }));
    assert_eq!(r["ok"], true, "{r}");
    let r = execute_json(&mut s, &json!({ "cmd": "Overlays" }));
    assert_eq!(r["ok"], true, "{r}");
}

#[test]
fn set_parameter_rebuilds_mesh_and_is_undoable() {
    let mut s = state();
    let id = box_at(&mut s, 0.0);
    execute(&mut s, Command::SetParameter { id: id.0, name: "Height".into(), value: 2.0 }).unwrap();
    assert!((s.scene.node(id).unwrap().object.mesh().aabb().1.z - 2.0).abs() < 1e-5);
    execute(&mut s, Command::SetParameter { id: id.0, name: "Height Segs".into(), value: 3.0 }).unwrap();
    assert_eq!(s.scene.node(id).unwrap().object.mesh().face_count(), 2 + 4 * 3);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.node(id).unwrap().object.mesh().face_count(), 6);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!((s.scene.node(id).unwrap().object.mesh().aabb().1.z - 1.0).abs() < 1e-5);
    assert!(execute(&mut s, Command::SetParameter { id: id.0, name: "Nope".into(), value: 1.0 }).is_err());
}

#[test]
fn set_parameter_clamps_to_minimum() {
    let mut s = state();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Cylinder, position: [0.0; 3], size: [2.0, 2.0, 2.0] }).unwrap();
    let id = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::SetParameter { id: id.0, name: "Sides".into(), value: 1.0 }).unwrap();
    let params = s.scene.node(id).unwrap().object.parameters();
    let sides = params.iter().find(|(n, _)| *n == "Sides").unwrap().1.as_f64();
    assert_eq!(sides, 3.0);
    execute(&mut s, Command::SetParameter { id: id.0, name: "Radius".into(), value: -5.0 }).unwrap();
    let r = s.scene.node(id).unwrap().object.parameters().iter().find(|(n, _)| *n == "Radius").unwrap().1.as_f64();
    assert_eq!(r, 0.0);
}

#[test]
fn convert_to_editable_poly_keeps_mesh_and_undoes() {
    let mut s = state();
    let id = box_at(&mut s, 0.0);
    let faces = s.scene.node(id).unwrap().object.mesh().face_count();
    execute(&mut s, Command::ConvertToEditablePoly).unwrap();
    assert!(matches!(s.scene.node(id).unwrap().object, loom_scene::ObjectKind::EditablePoly(_)));
    assert_eq!(s.scene.node(id).unwrap().object.mesh().face_count(), faces);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(matches!(s.scene.node(id).unwrap().object, loom_scene::ObjectKind::Box { .. }));
}

#[test]
fn every_standard_primitive_can_be_created_and_named() {
    let mut s = state();
    let kinds = [
        (PrimitiveKind::Box, "Box001"), (PrimitiveKind::Cone, "Cone001"), (PrimitiveKind::Sphere, "Sphere001"),
        (PrimitiveKind::GeoSphere, "GeoSphere001"), (PrimitiveKind::Cylinder, "Cylinder001"), (PrimitiveKind::Tube, "Tube001"),
        (PrimitiveKind::Torus, "Torus001"), (PrimitiveKind::Pyramid, "Pyramid001"), (PrimitiveKind::Teapot, "Teapot001"),
        (PrimitiveKind::Plane, "Plane001"),
    ];
    for (kind, name) in kinds {
        let r = execute(&mut s, Command::CreatePrimitive { kind, position: [0.0; 3], size: [2.0, 2.0, 2.0] }).unwrap();
        assert_eq!(r["name"], name);
        let id = *s.selection.iter().next().unwrap();
        assert!(s.scene.node(id).unwrap().object.mesh().face_count() > 0, "{name}");
    }
    assert_eq!(s.scene.len(), 10);
}

#[test]
fn set_wire_color_is_undoable() {
    let mut s = state();
    let id = box_at(&mut s, 0.0);
    execute(&mut s, Command::SetWireColor { id: id.0, rgb: [255, 0, 0] }).unwrap();
    assert_eq!(s.scene.node(id).unwrap().wire_color, [255, 0, 0]);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_ne!(s.scene.node(id).unwrap().wire_color, [255, 0, 0]);
}

#[test]
fn created_primitive_meshes_transform_cleanly() {
    let mut s = state();
    for kind in PrimitiveKind::ALL {
        execute(&mut s, Command::CreatePrimitive { kind, position: [1.0, 2.0, 0.0], size: [2.5, 2.5, 2.5] }).unwrap();
        let id = *s.selection.iter().next().unwrap();
        let node = s.scene.node(id).unwrap();
        let mut mesh = node.object.mesh().into_owned();
        loom_mesh::edit_ops::transform_mesh(&mut mesh, s.scene.world_matrix(id)).unwrap_or_else(|e| panic!("{:?}: {e}", kind));
        assert!(mesh.vertices().iter().all(|v| v.position.is_finite()));
    }
}

// ---- plan 1b Task 5 ------------------------------------------------------

#[test]
fn affect_pivot_only_moves_pivot_not_mesh() {
    let mut s = state();
    let id = box_at(&mut s, 0.0);
    let before = s.object_bounds(id).unwrap();
    execute(&mut s, Command::AffectPivotOnly).unwrap();
    assert!(s.affect_pivot_only);
    execute(&mut s, Command::MoveSelection { delta: [2.0, 0.0, 0.0], system: Some(CoordSystem::World) }).unwrap();
    let node = s.scene.node(id).unwrap();
    assert_eq!(node.transform.position.get().x, 2.0, "pivot moved");
    let after = s.object_bounds(id).unwrap();
    assert!(before.0.abs_diff_eq(after.0, 1e-5) && before.1.abs_diff_eq(after.1, 1e-5), "mesh stayed: {before:?} {after:?}");
    execute(&mut s, Command::AffectPivotOnly).unwrap();
    execute(&mut s, Command::CenterPivotToObject).unwrap();
    let c = s.scene.world_matrix(id).w_axis;
    assert!((c.x - 0.0).abs() < 1e-5 && (c.z - 0.5).abs() < 1e-5, "pivot at bounds centre: {c}");
    execute(&mut s, Command::ResetPivot).unwrap();
    assert!(s.scene.node(id).unwrap().object_offset == loom_scene::Transform::default());
}

#[test]
fn locks_block_move_on_locked_axes() {
    let mut s = state();
    let id = box_at(&mut s, 0.0);
    execute(&mut s, Command::SetNodeLocks { id: id.0, move_axes: Some([true, false, false]), rotate_axes: None, scale_axes: None }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [3.0, 4.0, 0.0], system: Some(CoordSystem::World) }).unwrap();
    let p = s.scene.node(id).unwrap().transform.position.get();
    assert_eq!((p.x, p.y), (0.0, 4.0));
}

#[test]
fn inherit_off_ignores_parent_position() {
    let mut s = state();
    let p = box_at(&mut s, 5.0);
    let c = box_at(&mut s, 0.0);
    execute(&mut s, Command::SetParent { id: c.0, parent: Some(p.0) }).unwrap();
    execute(&mut s, Command::SetNodeInherit { id: c.0, move_axes: Some([false, true, true]), rotate_axes: None, scale_axes: None }).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![p.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [2.0, 0.0, 0.0], system: Some(CoordSystem::World) }).unwrap();
    assert!((s.scene.world_matrix(c).w_axis.x - (-5.0)).abs() < 1e-5, "child keeps its local x without inheriting the parent's x: {}", s.scene.world_matrix(c).w_axis.x);
}

#[test]
fn reset_xform_bakes_transform() {
    let mut s = state();
    let id = box_at(&mut s, 0.0);
    execute(&mut s, Command::SetTransform { id: id.0, position: Some([1.0, 2.0, 0.0]), rotation_deg: Some([0.0, 0.0, 90.0]), scale: Some([2.0, 1.0, 1.0]) }).unwrap();
    let before = s.object_bounds(id).unwrap();
    execute(&mut s, Command::ResetXForm).unwrap();
    let node = s.scene.node(id).unwrap();
    assert!(matches!(node.object, loom_scene::ObjectKind::EditablePoly(_)));
    assert_eq!(node.transform.rotation.get(), glam::Vec3::ZERO);
    assert_eq!(node.transform.scale.get(), glam::Vec3::ONE);
    assert_eq!(node.transform.position.get(), glam::Vec3::new(1.0, 2.0, 0.0), "position is kept");
    let after = s.object_bounds(id).unwrap();
    assert!(before.0.abs_diff_eq(after.0, 1e-4) && before.1.abs_diff_eq(after.1, 1e-4), "{before:?} {after:?}");
}

#[test]
fn measure_reports_box_volume() {
    let mut s = state();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [2.0, 3.0, 4.0] }).unwrap();
    let r = execute(&mut s, Command::Measure).unwrap();
    assert!((r["volume"].as_f64().unwrap() - 24.0).abs() < 1e-3, "{r}");
    assert!((r["surface_area"].as_f64().unwrap() - 52.0).abs() < 1e-3, "{r}");
    assert_eq!(r["dimensions"], json!([2.0, 3.0, 4.0]));
}

#[test]
fn border_level_selects_open_edges() {
    let mut s = state();
    let id = box_at(&mut s, 0.0);
    execute(&mut s, Command::SelectFacesFacing { node: Some(id.0), direction: [0.0, 0.0, 1.0], tolerance: 0.9 }).unwrap();
    execute(&mut s, Command::DeleteObjects).unwrap(); // removes the top face → 4 open edges
    execute(&mut s, Command::SelectionMode { mode: loom_commands::SelectionMode::Border }).unwrap();
    execute(&mut s, Command::SelectAll).unwrap();
    assert_eq!(s.sub_selection.len(), 4);
}

#[test]
fn element_level_selects_connected_faces() {
    let mut s = state();
    let a = box_at(&mut s, 0.0);
    let b = box_at(&mut s, 5.0);
    execute(&mut s, Command::SelectNodes { ids: vec![a.0, b.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::ConvertToEditablePoly).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    // Attach b into a would need phase 2; instead test the element flood on one box:
    execute(&mut s, Command::SelectionMode { mode: loom_commands::SelectionMode::Element }).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(a.0), ids: vec![0], op: SelectOp::Replace }).unwrap();
    assert_eq!(s.sub_selection.len(), 6, "one face of a box selects the whole element");
}

#[test]
fn display_as_box_flag_roundtrips() {
    let mut s = state();
    let id = box_at(&mut s, 0.0);
    execute(&mut s, Command::SetDisplayProperty { id: id.0, name: "Display as Box".into(), value: true }).unwrap();
    assert!(s.scene.node(id).unwrap().display.display_as_box);
    execute(&mut s, Command::SetDisplayProperty { id: id.0, name: "Ignore Extents".into(), value: true }).unwrap();
    assert!(s.scene_bounds().is_none(), "ignored extents are left out of Zoom Extents");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(!s.scene.node(id).unwrap().display.display_as_box);
}

#[test]
fn hide_by_hit_mode_hides_clicked_node() {
    let mut s = state();
    let id = box_at(&mut s, 0.0);
    execute(&mut s, Command::HideByHit).unwrap();
    assert_eq!(format!("{:?}", s.pick_mode), "Some(HideByHit)");
    execute(&mut s, Command::PickHit { id: id.0 }).unwrap();
    assert!(s.scene.node(id).unwrap().hidden);
    assert!(s.pick_mode.is_some(), "the mode stays until right-click");
}

// ---- plan 1b Task 6 ------------------------------------------------------

#[test]
fn selection_filter_limits_select_all_and_picks() {
    let mut s = state();
    box_at(&mut s, 0.0);
    execute(&mut s, Command::CreateCamera { target: false }).unwrap();
    execute(&mut s, Command::SelectionFilter { filter: loom_commands::SelectionFilter::Cameras }).unwrap();
    execute(&mut s, Command::SelectAll).unwrap();
    assert_eq!(s.selection.len(), 1);
    let id = *s.selection.iter().next().unwrap();
    assert!(matches!(s.scene.node(id).unwrap().object, loom_scene::ObjectKind::Camera { .. }));
    let boxes: Vec<u64> = s.scene.nodes().filter(|n| n.name.starts_with("Box")).map(|n| n.id.0).collect();
    execute(&mut s, Command::SelectNodes { ids: boxes, op: SelectOp::Replace }).unwrap();
    assert!(s.selection.is_empty(), "filtered-out kinds cannot be selected from the viewport path");
}

#[test]
fn link_mode_sets_parent_and_unlink_clears_it() {
    let mut s = state();
    let child = box_at(&mut s, 0.0);
    let parent = box_at(&mut s, 3.0);
    execute(&mut s, Command::SelectNodes { ids: vec![child.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SelectAndLink).unwrap();
    assert_eq!(format!("{:?}", s.pick_mode), "Some(Link)");
    execute(&mut s, Command::PickHit { id: parent.0 }).unwrap();
    assert_eq!(s.scene.node(child).unwrap().parent, Some(parent));
    execute(&mut s, Command::SelectNodes { ids: vec![child.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::UnlinkSelection).unwrap();
    assert_eq!(s.scene.node(child).unwrap().parent, None);
}

#[test]
fn named_selection_sets_round_trip() {
    let mut s = state();
    let a = box_at(&mut s, 0.0);
    let b = box_at(&mut s, 3.0);
    execute(&mut s, Command::SelectNodes { ids: vec![a.0, b.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::NamedSelectionSet { name: "walls".into() }).unwrap();
    execute(&mut s, Command::SelectNone).unwrap();
    execute(&mut s, Command::SelectNamedSet { name: "walls".into() }).unwrap();
    assert_eq!(s.selection.len(), 2);
    execute(&mut s, Command::DeleteNamedSet { name: "walls".into() }).unwrap();
    assert!(execute(&mut s, Command::SelectNamedSet { name: "walls".into() }).is_err());
}

#[test]
fn hide_and_freeze_nodes_by_id() {
    let mut s = state();
    let a = box_at(&mut s, 0.0);
    execute(&mut s, Command::HideNodes { ids: vec![a.0] }).unwrap();
    assert!(s.scene.node(a).unwrap().hidden);
    execute(&mut s, Command::UnhideNodes { ids: vec![a.0] }).unwrap();
    execute(&mut s, Command::FreezeNodes { ids: vec![a.0] }).unwrap();
    assert!(s.scene.node(a).unwrap().frozen);
    execute(&mut s, Command::UnfreezeNodes { ids: vec![a.0] }).unwrap();
    assert!(!s.scene.node(a).unwrap().frozen);
}

#[test]
fn window_crossing_and_region_state_toggle() {
    let mut s = state();
    assert!(!s.window_selection, "defaults to Crossing");
    execute(&mut s, Command::WindowCrossingSelectionToggle).unwrap();
    assert!(s.window_selection);
    execute(&mut s, Command::SelectionRegion { shape: loom_commands::SelectionRegion::Lasso }).unwrap();
    assert_eq!(format!("{:?}", s.selection_region), "Lasso");
    execute(&mut s, Command::ReferenceCoordinateSystem { system: loom_commands::CoordSystem::Local }).unwrap();
    assert_eq!(format!("{:?}", s.coord_system), "Local");
    execute(&mut s, Command::UseCenter { mode: loom_commands::CenterMode::Selection }).unwrap();
    assert_eq!(format!("{:?}", s.center_mode), "Selection");
    execute(&mut s, Command::KeyboardShortcutOverrideToggle).unwrap();
    assert!(!s.keyboard_override);
}

// ---- plan 1b Task 7 ------------------------------------------------------

#[test]
fn set_time_configuration_clamps_frame() {
    let mut s = state();
    execute(&mut s, Command::SetFrame { frame: 80 }).unwrap();
    assert_eq!(s.current_frame, 80);
    execute(&mut s, Command::SetTimeConfiguration { start: Some(0), end: Some(50), fps: Some(24.0) }).unwrap();
    assert_eq!(s.frame_range, (0, 50));
    assert_eq!(s.current_frame, 50, "current frame clamps into the new segment");
    assert_eq!(s.fps, 24.0);
    assert!(execute(&mut s, Command::SetTimeConfiguration { start: Some(10), end: Some(5), fps: None }).is_err());
    execute(&mut s, Command::SetFrame { frame: -5 }).unwrap();
    assert_eq!(s.current_frame, 0);
}

#[test]
fn fov_mode_changes_fov_not_distance() {
    let mut s = state();
    let d = s.active().camera.distance;
    execute(&mut s, Command::SetFov { degrees: 60.0 }).unwrap();
    assert!((s.active().fov_y_radians.to_degrees() - 60.0).abs() < 1e-4);
    assert_eq!(s.active().camera.distance, d);
    execute(&mut s, Command::SetFov { degrees: 500.0 }).unwrap();
    assert!(s.active().fov_y_radians.to_degrees() <= 175.0);
    execute(&mut s, Command::UndoViewportOperation).unwrap();
    assert!((s.active().fov_y_radians.to_degrees() - 60.0).abs() < 1e-4, "FOV change is a viewport undo step");
}

#[test]
fn coordinate_display_toggle_and_orbit_flyout() {
    let mut s = state();
    assert!(!s.offset_mode);
    execute(&mut s, Command::AbsoluteOffsetToggle).unwrap();
    assert!(s.offset_mode);
    execute(&mut s, Command::OrbitSelectedMode).unwrap();
    assert_eq!(format!("{:?}", s.nav_mode), "OrbitSelected");
    execute(&mut s, Command::OrbitSubObjectMode).unwrap();
    assert_eq!(format!("{:?}", s.nav_mode), "OrbitSubObject");
    execute(&mut s, Command::ZoomAll).unwrap();
    assert_eq!(format!("{:?}", s.nav_mode), "ZoomAll");
}

#[test]
fn zoom_extents_all_selected_frames_every_viewport() {
    let mut s = state();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [20.0, 0.0, 0.0], size: [1.0, 1.0, 1.0] }).unwrap();
    execute(&mut s, Command::ZoomExtentsAllSelected).unwrap();
    for v in &s.viewports {
        assert!((v.camera.target.x - 20.0).abs() < 1e-4, "{:?}", v.kind);
    }
}

// ---- plan 1b final review fixes --------------------------------------------

fn child_of(s: &mut AppState, parent: loom_scene::NodeId, x: f32) -> loom_scene::NodeId {
    let c = box_at(s, x);
    execute(s, Command::SetParent { id: c.0, parent: Some(parent.0) }).unwrap();
    c
}

#[test]
fn pivot_edits_never_move_children() {
    let mut s = state();
    let p = box_at(&mut s, 0.0);
    let c = child_of(&mut s, p, 3.0);
    execute(&mut s, Command::SelectNodes { ids: vec![p.0], op: SelectOp::Replace }).unwrap();
    let before = s.scene.object_matrix(c);
    execute(&mut s, Command::AffectPivotOnly).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 2.0], system: Some(CoordSystem::World) }).unwrap();
    assert!(before.abs_diff_eq(s.scene.object_matrix(c), 1e-4), "Affect Pivot Only moved the child");
    execute(&mut s, Command::AffectPivotOnly).unwrap();
    for cmd in [Command::CenterPivotToObject, Command::AlignPivotToWorld, Command::ResetPivot, Command::ResetXForm] {
        execute(&mut s, cmd.clone()).unwrap();
        assert!(before.abs_diff_eq(s.scene.object_matrix(c), 1e-4), "{cmd:?} moved the child");
    }
    execute(&mut s, Command::SetPivot { id: p.0, position: Some([1.0, 1.0, 1.0]), rotation_deg: Some([0.0, 0.0, 45.0]) }).unwrap();
    assert!(before.abs_diff_eq(s.scene.object_matrix(c), 1e-4), "Set Pivot moved the child");
}

#[test]
fn moves_respect_inherit_flags() {
    let mut s = state();
    let p = box_at(&mut s, 5.0);
    let c = child_of(&mut s, p, 0.0);
    execute(&mut s, Command::SetNodeInherit { id: c.0, move_axes: Some([false, true, true]), rotate_axes: None, scale_axes: None }).unwrap();
    let wx = s.scene.object_matrix(c).w_axis.x;
    execute(&mut s, Command::SelectNodes { ids: vec![c.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 1.0, 0.0], system: Some(CoordSystem::World) }).unwrap();
    let w = s.scene.object_matrix(c).w_axis;
    assert!((w.x - wx).abs() < 1e-4, "a +Y move changed X: {w}");
    execute(&mut s, Command::CenterPivotToObject).unwrap();
    assert!((s.scene.object_matrix(c).w_axis.x - wx).abs() < 1e-4, "Center to Object moved the mesh under inherit");
}

#[test]
fn hidden_and_frozen_nodes_cannot_be_selected() {
    let mut s = state();
    let a = box_at(&mut s, 0.0);
    execute(&mut s, Command::FreezeNodes { ids: vec![a.0] }).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    assert!(s.selection.is_empty());
    execute(&mut s, Command::UnfreezeNodes { ids: vec![a.0] }).unwrap();
    execute(&mut s, Command::HideNodes { ids: vec![a.0] }).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    assert!(s.selection.is_empty());
    execute(&mut s, Command::UnhideNodes { ids: vec![a.0] }).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::HideGeometryToggle).unwrap();
    assert!(s.selection.is_empty(), "hiding a category deselects its objects");
}

#[test]
fn slice_zero_to_zero_keeps_the_full_object() {
    let full = loom_mesh::torus_primitive(2.0, 0.5, 12, 8, None);
    let sliced = loom_mesh::torus_primitive(2.0, 0.5, 12, 8, Some((0.0, 0.0)));
    assert_eq!(full.face_count(), sliced.face_count());
    let cyl = loom_mesh::cylinder_segmented(1.0, 2.0, 1, 1, 8, Some((0.0, 360.0)));
    assert_eq!(cyl.face_count(), loom_mesh::cylinder_segmented(1.0, 2.0, 1, 1, 8, None).face_count());
}

#[test]
fn torus_twist_rotates_cross_sections_not_shear() {
    // With a 360° twist each cross-section comes back to itself: the mesh
    // bounds equal the untwisted torus.
    let a = loom_mesh::torus_full(2.0, 0.5, 0.0, 0.0, 24, 12, None);
    let b = loom_mesh::torus_full(2.0, 0.5, 0.0, 360.0, 24, 12, None);
    let (amin, amax) = a.aabb();
    let (bmin, bmax) = b.aabb();
    assert!(amin.abs_diff_eq(bmin, 1e-3) && amax.abs_diff_eq(bmax, 1e-3));
    b.check_invariants().unwrap();
    // A 90° twist over the ring still keeps every vertex on the torus surface.
    let c = loom_mesh::torus_full(2.0, 0.5, 0.0, 90.0, 24, 12, None);
    for v in c.vertices() {
        let r = (v.position.x * v.position.x + v.position.y * v.position.y).sqrt();
        let d = ((r - 2.0).powi(2) + v.position.z.powi(2)).sqrt();
        assert!((d - 0.5).abs() < 1e-3, "{}", v.position);
    }
}

#[test]
fn reset_xform_leaves_cameras_alone() {
    let mut s = state();
    execute(&mut s, Command::CreateCamera { target: true }).unwrap();
    let cam = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::ResetXForm).unwrap();
    assert!(matches!(s.scene.node(cam).unwrap().object, loom_scene::ObjectKind::Camera { target: Some(_), .. }));
}

#[test]
fn fov_is_per_viewport() {
    let mut s = state();
    execute(&mut s, Command::SetFov { degrees: 80.0 }).unwrap();
    assert!((s.viewports[3].fov_y_radians.to_degrees() - 80.0).abs() < 1e-4);
    execute(&mut s, Command::ActiveViewport { index: 0 }).unwrap();
    execute(&mut s, Command::UndoViewportOperation).unwrap();
    assert!((s.viewports[3].fov_y_radians.to_degrees() - 80.0).abs() < 1e-4, "undo in another viewport must not touch it");
}

#[test]
fn see_through_toggle_sets_the_property_on_the_selection() {
    let mut s = state();
    let a = box_at(&mut s, 0.0);
    execute(&mut s, Command::DisplayAsSeeThroughToggle).unwrap();
    assert!(s.scene.node(a).unwrap().display.see_through);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(!s.scene.node(a).unwrap().display.see_through);
}

#[test]
fn camera_view_reverts_when_camera_is_deleted() {
    let mut s = state();
    execute(&mut s, Command::CreateCamera { target: false }).unwrap();
    let cam = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::CameraView).unwrap();
    assert_eq!(s.active().kind, loom_commands::ViewKind::Camera(cam));
    assert_eq!(s.view_label(s.active_viewport), "Camera001");
    execute(&mut s, Command::DeleteObjects).unwrap();
    assert_eq!(s.active().kind, loom_commands::ViewKind::Perspective);
}

#[test]
fn select_invert_respects_filter_and_filter_keeps_selection() {
    let mut s = state();
    let a = box_at(&mut s, 0.0);
    execute(&mut s, Command::CreateCamera { target: false }).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SelectionFilter { filter: loom_commands::SelectionFilter::Cameras }).unwrap();
    assert_eq!(s.selection.len(), 1, "changing the filter keeps the selection");
    execute(&mut s, Command::SelectInvert).unwrap();
    assert!(s.selection.iter().all(|id| matches!(s.scene.node(*id).unwrap().object, loom_scene::ObjectKind::Camera { .. })));
}

#[test]
fn cycle_selection_method_walks_the_region_flyout() {
    let mut s = state();
    let order: Vec<String> = (0..6)
        .map(|_| {
            execute(&mut s, Command::CycleSelectionMethod).unwrap();
            format!("{:?}", s.selection_region)
        })
        .collect();
    assert_eq!(order, vec!["Circular", "Fence", "Lasso", "Paint", "Rectangular", "Circular"]);
}
