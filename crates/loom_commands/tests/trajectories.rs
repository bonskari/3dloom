//! Motion panel > Trajectories, the track bar filter, constraint pick mode,
//! Delete Selected Animation (phase 8).

use glam::Vec3;
use loom_commands::{execute, AppState, Axis, Command, PrimitiveKind, SelectOp};
use loom_scene::NodeId;

fn boxed(s: &mut AppState, at: [f32; 3]) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: at, size: [2.0; 3] }).unwrap();
    *s.selection.iter().next().unwrap()
}

fn select(s: &mut AppState, id: NodeId) {
    execute(s, Command::SelectNodes { ids: vec![id.0], op: SelectOp::Replace }).unwrap();
}

fn world(s: &mut AppState, id: NodeId, f: i32) -> Vec3 {
    execute(s, Command::SetFrame { frame: f }).unwrap();
    s.scene.world_matrix(id).w_axis.truncate()
}

fn keyed(s: &mut AppState) -> NodeId {
    let b = boxed(s, [0.0; 3]);
    execute(s, Command::AutoKeyModeToggle).unwrap();
    execute(s, Command::SetFrame { frame: 50 }).unwrap();
    execute(s, Command::MoveSelection { delta: [50.0, 20.0, 0.0], system: None }).unwrap();
    execute(s, Command::SetFrame { frame: 100 }).unwrap();
    execute(s, Command::MoveSelection { delta: [50.0, -20.0, 10.0], system: None }).unwrap();
    execute(s, Command::AutoKeyModeToggle).unwrap();
    execute(s, Command::SetFrame { frame: 0 }).unwrap();
    b
}

#[test]
fn trajectory_convert_to_and_from_round_trip() {
    let mut s = AppState::new();
    let b = keyed(&mut s);
    let r = execute(&mut s, Command::TrajectoryConvertTo { start: Some(0), end: Some(100), samples: Some(10) }).unwrap();
    let shape = NodeId(r["id"].as_u64().unwrap());
    let sh = s.scene.node(shape).unwrap().object.shape().unwrap();
    assert_eq!(sh.splines[0].knots.len(), 11);
    let om = s.scene.object_matrix(shape);
    let k5 = om.transform_point3(sh.splines[0].knots[5].pos);
    assert!(k5.distance(world(&mut s, b, 50)) < 1e-3, "{k5:?}");
    // Convert From puts a fresh box on the same path.
    let c = boxed(&mut s, [0.0, 0.0, 30.0]);
    execute(&mut s, Command::TrajectoryConvertFrom { shape: shape.0, start: Some(0), end: Some(100), samples: Some(10) }).unwrap();
    let keys = s.scene.node(c).unwrap().transform.position.keys().unwrap().tracks[0].keys.len();
    assert_eq!(keys, 11);
    for f in [0, 50, 100] {
        let (pb, pc) = (world(&mut s, b, f), world(&mut s, c, f));
        assert!(pb.distance(pc) < 0.5, "frame {f}: {pb:?} vs {pc:?}");
    }
}

#[test]
fn collapse_bakes_constraints_into_keys() {
    let mut s = AppState::new();
    let a = keyed(&mut s);
    let b = boxed(&mut s, [0.0, 40.0, 0.0]);
    execute(&mut s, Command::PositionConstraint { target: a.0 }).unwrap();
    let before: Vec<Vec3> = [0, 30, 70, 100].iter().map(|f| world(&mut s, b, *f)).collect();
    select(&mut s, b);
    execute(&mut s, Command::TrajectoryCollapse { start: Some(0), end: Some(100), samples: Some(10), position: Some(true), rotation: Some(true), scale: Some(true) }).unwrap();
    assert!(s.scene.node(b).unwrap().constraints.position.is_none(), "the constraint is baked away");
    let after: Vec<Vec3> = [0, 30, 70, 100].iter().map(|f| world(&mut s, b, *f)).collect();
    for (x, y) in before.iter().zip(&after) {
        assert!(x.distance(*y) < 0.05, "{x:?} vs {y:?}");
    }
}

#[test]
fn track_bar_filter_current_transform() {
    let mut s = AppState::new();
    let b = keyed(&mut s);
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::SetFrame { frame: 50 }).unwrap();
    execute(&mut s, Command::RotateSelection { axis: Axis::Z, degrees: 45.0, system: None }).unwrap();
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::SetTrackBarFilter { filter: loom_commands::anim_cmds::TrackBarFilter::CurrentTransform }).unwrap();
    execute(&mut s, Command::SelectAndRotate).unwrap();
    execute(&mut s, Command::DeleteKeys { from: 50, to: 50 }).unwrap();
    let n = s.scene.node(b).unwrap();
    assert!(n.transform.rotation.keys().unwrap().tracks[2].keys.iter().all(|k| k.time != 50.0), "rotation key gone");
    assert!(n.transform.position.keys().unwrap().tracks[0].keys.iter().any(|k| k.time == 50.0), "position key stays");
    let r = execute(&mut s, Command::State).unwrap();
    assert_eq!(r["animation"]["track_bar_filter"], "CurrentTransform");
}

#[test]
fn constraint_pick_mode_assigns_on_click() {
    let mut s = AppState::new();
    let t = boxed(&mut s, [10.0, 0.0, 0.0]);
    let b = boxed(&mut s, [0.0; 3]);
    execute(&mut s, Command::StartConstraintPick { kind: "LookAt Constraint".into() }).unwrap();
    assert!(s.pick_mode.is_some());
    execute(&mut s, Command::PickHit { id: t.0 }).unwrap();
    assert!(s.pick_mode.is_none());
    assert_eq!(s.scene.node(b).unwrap().constraints.rotation_name(), Some("LookAt Constraint"));
    assert!(execute(&mut s, Command::StartConstraintPick { kind: "Bogus".into() }).is_err());
}

#[test]
fn delete_selected_animation_and_toggles() {
    let mut s = AppState::new();
    let b = keyed(&mut s);
    select(&mut s, b);
    execute(&mut s, Command::DeleteSelectedAnimation).unwrap();
    let p = &s.scene.node(b).unwrap().transform.position;
    assert!(!p.is_animated());
    assert!((p.get_at(0.0) - Vec3::ZERO).length() < 1e-4, "it holds the current frame's pose");
    execute(&mut s, Command::ShowTrajectoriesToggle).unwrap();
    execute(&mut s, Command::ShowTrackBarToggle).unwrap();
    assert!(s.show_trajectories && !s.show_track_bar);
}

#[test]
fn trajectory_sub_object_keys_move() {
    let mut s = AppState::new();
    let b = keyed(&mut s);
    execute(&mut s, Command::TrajectorySubObjectKeys).unwrap();
    execute(&mut s, Command::TrajectorySelectKeys { frames: vec![50], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::TrajectoryMoveKeys { delta: [0.0, 0.0, 5.0] }).unwrap();
    assert!((world(&mut s, b, 50) - Vec3::new(50.0, 20.0, 10.0)).length() < 1e-3, "the eased Z (5) plus 5");
    assert!((world(&mut s, b, 100) - Vec3::new(100.0, 0.0, 10.0)).length() < 1e-3);
}

#[test]
fn move_tool_moves_trajectory_keys_in_sub_object_keys() {
    let mut s = AppState::new();
    let b = keyed(&mut s);
    execute(&mut s, Command::TrajectorySubObjectKeys).unwrap();
    execute(&mut s, Command::TrajectorySelectKeys { frames: vec![100], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 7.0, 0.0], system: None }).unwrap();
    assert!((world(&mut s, b, 100) - Vec3::new(100.0, 7.0, 10.0)).length() < 1e-3, "{:?}", world(&mut s, b, 100));
    assert!((world(&mut s, b, 0) - Vec3::ZERO).length() < 1e-3, "the object itself did not move");
}
