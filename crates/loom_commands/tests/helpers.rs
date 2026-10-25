//! Helpers: Tape, Protractor, Compass, Grid (with Activate Grid), Point
//! display options, and Tools > Measure Distance.

use glam::Vec3;
use loom_commands::{execute, AppState, Command, PickMode};
use loom_scene::{NodeId, ObjectKind, ParamValue};

fn param(s: &AppState, id: NodeId, name: &str) -> ParamValue {
    s.scene.node(id).unwrap().object.parameters().into_iter().find(|(n, _)| *n == name).unwrap_or_else(|| panic!("no {name}")).1
}

fn helper(s: &mut AppState, kind: &str, position: [f32; 3], size: f32) -> NodeId {
    let r = execute(s, Command::CreateHelper { kind: kind.into(), position, size }).unwrap();
    NodeId(r["node"].as_u64().unwrap())
}

fn tape(s: &mut AppState, position: [f32; 3], target: [f32; 3]) -> (NodeId, NodeId) {
    let r = execute(s, Command::CreateTape { position, target }).unwrap();
    (NodeId(r["id"].as_u64().unwrap()), NodeId(r["target"].as_u64().unwrap()))
}

fn move_to(s: &mut AppState, id: NodeId, p: [f32; 3]) {
    execute(s, Command::SetTransform { id: id.0, position: Some(p), rotation_deg: None, scale: None }).unwrap();
}

fn readout(s: &mut AppState, id: NodeId) -> serde_json::Value {
    execute(s, Command::HelperReadout { id: id.0 }).unwrap()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-3
}

#[test]
fn tape_measures_the_distance_to_its_target_and_follows_it() {
    let mut s = AppState::new();
    let (t, target) = tape(&mut s, [0.0, 0.0, 0.0], [30.0, 40.0, 0.0]);
    assert_eq!(s.scene.node(t).unwrap().name, "Tape001");
    assert_eq!(s.scene.node(target).unwrap().name, "Tape001.Target");
    assert!(matches!(s.scene.node(t).unwrap().object, ObjectKind::Tape { target: Some(x), .. } if x == target));
    let r = readout(&mut s, t);
    assert!(close(r["length"].as_f64().unwrap(), 50.0), "{r}");
    move_to(&mut s, target, [0.0, 0.0, 20.0]);
    let r = readout(&mut s, t);
    assert!(close(r["length"].as_f64().unwrap(), 20.0), "{r}");
    // Straight up: 90° to X and Y, 0° to Z; 90° to the XY plane, 0° to YZ and ZX.
    let a = &r["to_axis"];
    assert!(close(a[0].as_f64().unwrap(), 90.0) && close(a[1].as_f64().unwrap(), 90.0) && close(a[2].as_f64().unwrap(), 0.0), "{r}");
    let p = &r["to_plane"];
    assert!(close(p[0].as_f64().unwrap(), 90.0) && close(p[1].as_f64().unwrap(), 0.0) && close(p[2].as_f64().unwrap(), 0.0), "{r}");
    // The tape's -Z aims at its target.
    let fwd = -s.scene.world_matrix(t).z_axis.truncate().normalize();
    assert!(fwd.distance(Vec3::Z) < 1e-4, "{fwd}");
}

#[test]
fn tape_specify_length_reports_the_typed_length() {
    let mut s = AppState::new();
    let (t, _) = tape(&mut s, [0.0; 3], [10.0, 0.0, 0.0]);
    execute(&mut s, Command::SetParameter { id: t.0, name: "Length".into(), value: 25.0 }).unwrap();
    assert!(close(readout(&mut s, t)["length"].as_f64().unwrap(), 10.0), "Length only counts with Specify Length on");
    execute(&mut s, Command::SetParameter { id: t.0, name: "Specify Length".into(), value: 1.0 }).unwrap();
    assert!(close(readout(&mut s, t)["length"].as_f64().unwrap(), 25.0));
    assert_eq!(param(&s, t, "Specify Length"), ParamValue::Bool(true));
}

#[test]
fn deleting_either_end_of_a_tape_deletes_both() {
    let mut s = AppState::new();
    let (t, target) = tape(&mut s, [0.0; 3], [10.0, 0.0, 0.0]);
    execute(&mut s, Command::SelectNodes { ids: vec![target.0], op: loom_commands::SelectOp::Replace }).unwrap();
    execute(&mut s, Command::DeleteObjects).unwrap();
    assert!(s.scene.node(t).is_none() && s.scene.node(target).is_none());
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(s.scene.node(t).is_some() && s.scene.node(target).is_some());
}

#[test]
fn protractor_measures_the_angle_between_two_picked_objects() {
    let mut s = AppState::new();
    let a = helper(&mut s, "point", [10.0, 0.0, 0.0], 5.0);
    let b = helper(&mut s, "point", [0.0, 10.0, 0.0], 5.0);
    let p = helper(&mut s, "protractor", [0.0; 3], 0.0);
    assert_eq!(s.scene.node(p).unwrap().name, "Protractor001");
    execute(&mut s, Command::ProtractorPickObject { which: 1 }).unwrap();
    assert_eq!(s.pick_mode, Some(PickMode::ProtractorObject(1)));
    execute(&mut s, Command::PickHit { id: a.0 }).unwrap();
    assert_eq!(s.pick_mode, None);
    assert_eq!(s.selection.iter().copied().collect::<Vec<_>>(), vec![p], "the protractor stays selected");
    execute(&mut s, Command::ProtractorPickObject { which: 2 }).unwrap();
    execute(&mut s, Command::PickHit { id: b.0 }).unwrap();
    let r = readout(&mut s, p);
    assert!(close(r["angle"].as_f64().unwrap(), 90.0), "{r}");
    assert_eq!(r["object1"], "Point001");
    assert_eq!(r["object2"], "Point002");
    move_to(&mut s, b, [10.0, 10.0, 0.0]);
    assert!(close(readout(&mut s, p)["angle"].as_f64().unwrap(), 45.0));
    // Undo takes the pick back.
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(readout(&mut s, p)["angle"].is_null());
}

#[test]
fn compass_has_rose_and_radius() {
    let mut s = AppState::new();
    let c = helper(&mut s, "compass", [0.0; 3], 30.0);
    assert_eq!(s.scene.node(c).unwrap().name, "Compass001");
    assert_eq!(param(&s, c, "Show Compass Rose"), ParamValue::Bool(true));
    assert_eq!(param(&s, c, "Radius"), ParamValue::Float(30.0));
    let (min, max) = s.scene.node(c).unwrap().object.mesh().aabb();
    assert!((max.y - 30.0).abs() < 1e-3 && (min.x + 30.0).abs() < 1e-3, "the rose spans the radius");
}

#[test]
fn point_has_all_display_options() {
    let mut s = AppState::new();
    let p = helper(&mut s, "point", [0.0; 3], 20.0);
    for name in ["Center Marker", "Constant Screen Size", "Draw On Top"] {
        assert_eq!(param(&s, p, name), ParamValue::Bool(false), "{name}");
        execute(&mut s, Command::SetParameter { id: p.0, name: name.into(), value: 1.0 }).unwrap();
        assert_eq!(param(&s, p, name), ParamValue::Bool(true), "{name}");
    }
    let names: Vec<&str> = s.scene.node(p).unwrap().object.parameters().iter().map(|(n, _)| *n).collect();
    assert_eq!(names, ["Center Marker", "Axis Tripod", "Cross", "Box", "Size", "Constant Screen Size", "Draw On Top"]);
}

#[test]
fn point_lines_follow_the_display_options() {
    let mut s = AppState::new();
    let p = helper(&mut s, "point", [0.0; 3], 20.0);
    // Defaults: Axis Tripod (3 lines) and Cross (3 lines).
    let lines = loom_commands::helpers::helper_lines(&s, p, &|_| 1.0);
    assert_eq!(lines.len(), 6);
    execute(&mut s, Command::SetParameter { id: p.0, name: "Box".into(), value: 1.0 }).unwrap();
    assert_eq!(loom_commands::helpers::helper_lines(&s, p, &|_| 1.0).len(), 18);
    // Constant Screen Size: Size counts in pixels at the point.
    execute(&mut s, Command::SetParameter { id: p.0, name: "Constant Screen Size".into(), value: 1.0 }).unwrap();
    let span = |lines: &[(Vec3, Vec3, loom_commands::helpers::LineKind)]| lines.iter().map(|(a, b, _)| a.max(*b).x).fold(0.0f32, f32::max);
    let near = span(&loom_commands::helpers::helper_lines(&s, p, &|_| 1.0));
    let far = span(&loom_commands::helpers::helper_lines(&s, p, &|_| 3.0));
    assert!((far / near - 3.0).abs() < 1e-3, "{near} {far}");
}

#[test]
fn grid_object_parameters_and_activation() {
    let mut s = AppState::new();
    let g = helper(&mut s, "grid", [0.0, 0.0, 50.0], 100.0);
    assert_eq!(s.scene.node(g).unwrap().name, "Grid001");
    assert_eq!(param(&s, g, "Length"), ParamValue::Float(100.0));
    assert_eq!(param(&s, g, "Width"), ParamValue::Float(100.0));
    assert_eq!(param(&s, g, "Grid"), ParamValue::Float(10.0));
    assert_eq!(param(&s, g, "Display"), ParamValue::Int(0));
    // The home grid is the construction plane until the grid is activated.
    assert_eq!(s.construction_plane(0).0, Vec3::ZERO);
    assert_eq!(s.active_grid(), None);
    execute(&mut s, Command::ActivateGridObject).unwrap();
    assert_eq!(s.active_grid(), Some(g));
    let (o, _, _, n) = s.construction_plane(0);
    assert!(o.distance(Vec3::new(0.0, 0.0, 50.0)) < 1e-4 && n.distance(Vec3::Z) < 1e-4);
    // Every viewport builds on it, the Front view too.
    let front = s.viewports.iter().position(|v| v.kind == loom_commands::ViewKind::Front).unwrap();
    assert!(s.construction_plane(front).3.distance(Vec3::Z) < 1e-4);
    // Rotated grid: the plane turns with it.
    execute(&mut s, Command::SetTransform { id: g.0, position: None, rotation_deg: Some([90.0, 0.0, 0.0]), scale: None }).unwrap();
    assert!(s.construction_plane(0).3.distance(-Vec3::Y) < 1e-4, "{:?}", s.construction_plane(0));
    execute(&mut s, Command::ActivateHomeGrid).unwrap();
    assert_eq!(s.active_grid(), None);
    assert_eq!(s.construction_plane(0).3, Vec3::Z);
}

#[test]
fn activate_grid_needs_a_selected_grid_and_deleting_it_returns_the_home_grid() {
    let mut s = AppState::new();
    helper(&mut s, "point", [0.0; 3], 20.0);
    assert!(execute(&mut s, Command::ActivateGridObject).is_err());
    let g = helper(&mut s, "grid", [0.0; 3], 100.0);
    execute(&mut s, Command::ActivateGridObject).unwrap();
    assert_eq!(s.active_grid(), Some(g));
    execute(&mut s, Command::DeleteObjects).unwrap();
    assert_eq!(s.active_grid(), None);
}

#[test]
fn active_grid_spacing_drives_grid_snaps() {
    let mut s = AppState::new();
    let g = helper(&mut s, "grid", [0.0, 0.0, 5.0], 100.0);
    execute(&mut s, Command::SetParameter { id: g.0, name: "Grid".into(), value: 7.0 }).unwrap();
    execute(&mut s, Command::ActivateGridObject).unwrap();
    assert_eq!(s.grid_spacing(), 7.0);
    let plane = s.construction_plane(0);
    let p = loom_commands::snap::grid_point(Vec3::new(8.0, 13.0, 5.0), plane, s.grid_spacing());
    assert!(p.distance(Vec3::new(7.0, 14.0, 5.0)) < 1e-4, "{p}");
}

#[test]
fn display_plane_chooses_the_construction_plane() {
    let mut s = AppState::new();
    let g = helper(&mut s, "grid", [0.0; 3], 100.0);
    execute(&mut s, Command::SetParameter { id: g.0, name: "Display".into(), value: 1.0 }).unwrap();
    execute(&mut s, Command::ActivateGridObject).unwrap();
    assert!(s.construction_plane(0).3.distance(Vec3::X) < 1e-4, "YZ plane: normal +X");
}

#[test]
fn measure_distance_reports_distance_and_deltas() {
    let mut s = AppState::new();
    execute(&mut s, Command::MeasureDistance).unwrap();
    assert_eq!(s.pick_mode, Some(PickMode::MeasureDistanceA));
    execute(&mut s, Command::PickPoint { point: [1.0, 2.0, 3.0] }).unwrap();
    assert_eq!(s.pick_mode, Some(PickMode::MeasureDistanceB));
    let r = execute(&mut s, Command::PickPoint { point: [4.0, 6.0, 3.0] }).unwrap();
    assert_eq!(s.pick_mode, None);
    assert!(close(r["distance"].as_f64().unwrap(), 5.0), "{r}");
    assert_eq!(r["delta"], serde_json::json!([3.0, 4.0, 0.0]));
    let msg = s.status_prompt.clone();
    assert!(msg.contains("Distance: 5") && msg.contains("Delta X: 3") && msg.contains("Delta Y: 4") && msg.contains("Delta Z: 0"), "{msg}");
}
