//! Transform controllers and constraints, Assign Controller (phase 8).

use glam::{Mat4, Vec3};
use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp};
use loom_scene::NodeId;

fn boxed(s: &mut AppState, at: [f32; 3]) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: at, size: [2.0; 3] }).unwrap();
    *s.selection.iter().next().unwrap()
}

fn dummy(s: &mut AppState, at: [f32; 3]) -> NodeId {
    execute(s, Command::CreateHelper { kind: "dummy".into(), position: at, size: 1.0 }).unwrap();
    *s.selection.iter().next().unwrap()
}

fn line(s: &mut AppState, pts: &[[f32; 3]]) -> NodeId {
    let r = execute(s, Command::LineStart { point: pts[0] }).unwrap();
    let id = r["id"].as_u64().unwrap();
    for p in &pts[1..] {
        execute(s, Command::LineAddKnot { node: id, point: *p, smooth: false }).unwrap();
    }
    execute(s, Command::LineEnd { node: id }).unwrap();
    NodeId(id)
}

fn select(s: &mut AppState, id: NodeId) {
    execute(s, Command::SelectNodes { ids: vec![id.0], op: SelectOp::Replace }).unwrap();
}

fn world_at(s: &mut AppState, id: NodeId, f: i32) -> Mat4 {
    execute(s, Command::SetFrame { frame: f }).unwrap();
    s.scene.world_matrix(id)
}

fn pos(m: Mat4) -> Vec3 {
    m.w_axis.truncate()
}

#[test]
fn path_constraint_follows_the_shape() {
    let mut s = AppState::new();
    let path = line(&mut s, &[[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [100.0, 100.0, 0.0]]);
    let b = boxed(&mut s, [5.0, 5.0, 5.0]);
    execute(&mut s, Command::PathConstraint { target: path.0 }).unwrap();
    assert!(pos(world_at(&mut s, b, 0)).distance(Vec3::ZERO) < 1e-3, "{:?}", pos(world_at(&mut s, b, 0)));
    assert!(pos(world_at(&mut s, b, 100)).distance(Vec3::new(100.0, 100.0, 0.0)) < 1e-2);
    // The default % keys ease in and out; Linear Float makes the timing even,
    // and Constant Velocity makes half the time half the length.
    execute(&mut s, Command::AssignController { id: b.0, track: "Path Constraint / Percent".into(), controller: "Linear Float".into() }).unwrap();
    execute(&mut s, Command::SetConstraintParameter { id: b.0, name: "Constant Velocity".into(), value: 1.0 }).unwrap();
    assert!(pos(world_at(&mut s, b, 25)).distance(Vec3::new(50.0, 0.0, 0.0)) < 0.5, "{:?}", pos(world_at(&mut s, b, 25)));
    // Follow: the X axis runs along the path.
    execute(&mut s, Command::SetConstraintParameter { id: b.0, name: "Follow".into(), value: 1.0 }).unwrap();
    let m = world_at(&mut s, b, 75);
    assert!(m.x_axis.truncate().normalize().distance(Vec3::Y) < 1e-2, "{:?}", m.x_axis);
    // % Along Path is a keyed track: 0 at the start frame, 100 at the end.
    execute(&mut s, Command::SetConstraintParameter { id: b.0, name: "% Along Path".into(), value: 50.0 }).unwrap();
    let r = execute(&mut s, Command::State).unwrap();
    assert!(r.to_string().contains("Path Constraint"));
}

#[test]
fn position_constraint_weights_targets() {
    let mut s = AppState::new();
    let a = dummy(&mut s, [0.0, 0.0, 0.0]);
    let c = dummy(&mut s, [10.0, 0.0, 0.0]);
    let b = boxed(&mut s, [3.0, 3.0, 3.0]);
    execute(&mut s, Command::PositionConstraint { target: a.0 }).unwrap();
    execute(&mut s, Command::PositionConstraint { target: c.0 }).unwrap();
    assert!(pos(world_at(&mut s, b, 0)).distance(Vec3::new(5.0, 0.0, 0.0)) < 1e-3);
    execute(&mut s, Command::SetConstraintWeight { id: b.0, rotation: false, index: 1, weight: 100.0 }).unwrap();
    assert!((pos(world_at(&mut s, b, 0)).x - 20.0 / 3.0).abs() < 1e-3);
    // Moving a target moves the constrained object.
    select(&mut s, c);
    execute(&mut s, Command::MoveSelection { delta: [0.0, 30.0, 0.0], system: None }).unwrap();
    assert!((pos(world_at(&mut s, b, 0)).y - 20.0).abs() < 1e-3);
    // Undo removes the second target.
    execute(&mut s, Command::DeleteConstraintTarget { id: b.0, rotation: false, index: 1 }).unwrap();
    assert!(pos(world_at(&mut s, b, 0)).distance(Vec3::ZERO) < 1e-3);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!((pos(world_at(&mut s, b, 0)).y - 20.0).abs() < 1e-3);
}

#[test]
fn lookat_aims_the_axis() {
    let mut s = AppState::new();
    let t = dummy(&mut s, [0.0, 10.0, 0.0]);
    let b = boxed(&mut s, [0.0, 0.0, 0.0]);
    execute(&mut s, Command::LookAtConstraint { target: t.0 }).unwrap();
    let m = world_at(&mut s, b, 0);
    assert!(m.x_axis.truncate().normalize().distance(Vec3::Y) < 1e-3, "{:?}", m.x_axis);
    execute(&mut s, Command::SetConstraintParameter { id: b.0, name: "Axis".into(), value: 2.0 }).unwrap();
    execute(&mut s, Command::SetConstraintParameter { id: b.0, name: "Flip".into(), value: 1.0 }).unwrap();
    let m = world_at(&mut s, b, 0);
    assert!(m.z_axis.truncate().normalize().distance(-Vec3::Y) < 1e-3, "{:?}", m.z_axis);
}

#[test]
fn orientation_blends_targets() {
    let mut s = AppState::new();
    let a = dummy(&mut s, [0.0, 0.0, 0.0]);
    let c = dummy(&mut s, [5.0, 0.0, 0.0]);
    select(&mut s, c);
    execute(&mut s, Command::RotateSelection { axis: loom_commands::Axis::Z, degrees: 90.0, system: None }).unwrap();
    let b = boxed(&mut s, [0.0, 0.0, 0.0]);
    execute(&mut s, Command::OrientationConstraint { target: a.0 }).unwrap();
    execute(&mut s, Command::OrientationConstraint { target: c.0 }).unwrap();
    let m = world_at(&mut s, b, 0);
    let x = m.x_axis.truncate().normalize();
    assert!(x.distance(Vec3::new(1.0, 1.0, 0.0).normalize()) < 1e-3, "{x:?}");
}

#[test]
fn link_constraint_switches_parent_over_time() {
    let mut s = AppState::new();
    let a = dummy(&mut s, [0.0, 0.0, 0.0]);
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::SetFrame { frame: 100 }).unwrap();
    execute(&mut s, Command::RotateSelection { axis: loom_commands::Axis::Z, degrees: 90.0, system: None }).unwrap();
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    // A linear spin makes the halfway angle exactly 45°.
    execute(&mut s, Command::AssignController { id: a.0, track: "Rotation".into(), controller: "Linear Rotation".into() }).unwrap();
    execute(&mut s, Command::SetFrame { frame: 0 }).unwrap();
    let b = boxed(&mut s, [10.0, 0.0, 0.0]);
    execute(&mut s, Command::LinkConstraint { target: Some(a.0) }).unwrap();
    execute(&mut s, Command::SetFrame { frame: 50 }).unwrap();
    execute(&mut s, Command::LinkConstraint { target: None }).unwrap();
    let h = 10.0 / 2f32.sqrt();
    assert!(pos(world_at(&mut s, b, 25)).distance(Vec3::new(10.0 * 22.5f32.to_radians().cos(), 10.0 * 22.5f32.to_radians().sin(), 0.0)) < 1e-2, "{:?}", pos(world_at(&mut s, b, 25)));
    assert!(pos(world_at(&mut s, b, 50)).distance(Vec3::new(h, h, 0.0)) < 1e-2, "{:?}", pos(world_at(&mut s, b, 50)));
    assert!(pos(world_at(&mut s, b, 100)).distance(Vec3::new(h, h, 0.0)) < 1e-2, "stays where it was let go: {:?}", pos(world_at(&mut s, b, 100)));
}

#[test]
fn attachment_sticks_to_a_face() {
    let mut s = AppState::new();
    let base = boxed(&mut s, [0.0, 0.0, 0.0]);
    let b = boxed(&mut s, [20.0, 0.0, 0.0]);
    execute(&mut s, Command::AttachmentConstraint { target: base.0, face: Some(0), a: Some(0.0), b: Some(0.0) }).unwrap();
    let mesh = s.scene.eval_mesh(base).value.mesh.clone();
    let v0 = s.scene.object_matrix(base).transform_point3(mesh.vertex_position(mesh.face_vertex_ids(loom_mesh::FaceId(0)).unwrap()[0]).unwrap());
    assert!(pos(world_at(&mut s, b, 0)).distance(v0) < 1e-3, "{:?} vs {v0:?}", pos(world_at(&mut s, b, 0)));
    select(&mut s, base);
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 7.0], system: None }).unwrap();
    assert!(pos(world_at(&mut s, b, 0)).distance(v0 + Vec3::Z * 7.0) < 1e-3);
}

#[test]
fn assign_tcb_and_noise_to_position() {
    let mut s = AppState::new();
    let b = boxed(&mut s, [0.0, 0.0, 0.0]);
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::SetFrame { frame: 10 }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [10.0, 0.0, 0.0], system: None }).unwrap();
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::AssignController { id: b.0, track: "Position".into(), controller: "TCB Position".into() }).unwrap();
    let p = &s.scene.node(b).unwrap().transform.position;
    let k = p.keys().unwrap();
    assert!(k.linked && k.tracks.iter().all(|t| t.interp == loom_scene::anim::Interp::Tcb));
    assert_eq!(k.tracks[1].keys.len(), 2, "a TCB Position keys all axes together");
    assert!((p.get_at(10.0).x - 10.0).abs() < 1e-4);
    let r = execute(&mut s, Command::State).unwrap();
    assert!(r.to_string().contains("TCB Position"));
    execute(&mut s, Command::AssignController { id: b.0, track: "Position".into(), controller: "Noise Position".into() }).unwrap();
    let p = &s.scene.node(b).unwrap().transform.position;
    assert!(p.get_at(3.0) != p.get_at(17.0));
    execute(&mut s, Command::AssignController { id: b.0, track: "Position".into(), controller: "Position XYZ".into() }).unwrap();
    assert!(!s.scene.node(b).unwrap().transform.position.is_animated());
    assert!(execute(&mut s, Command::AssignController { id: b.0, track: "Position".into(), controller: "Bezier Float".into() }).is_err(), "float controllers do not fit a position");
}

#[test]
fn list_controller_adds_noise_to_keys() {
    let mut s = AppState::new();
    let b = boxed(&mut s, [0.0, 0.0, 0.0]);
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::SetFrame { frame: 10 }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [10.0, 0.0, 0.0], system: None }).unwrap();
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    let keyed = s.scene.node(b).unwrap().transform.position.clone();
    execute(&mut s, Command::AssignController { id: b.0, track: "Position".into(), controller: "Position List".into() }).unwrap();
    assert_eq!(s.scene.node(b).unwrap().transform.position.get_at(5.0), keyed.get_at(5.0), "the keys stay as the first entry");
    execute(&mut s, Command::ListAddController { id: b.0, track: "Position".into(), controller: "Noise Position".into() }).unwrap();
    let p = s.scene.node(b).unwrap().transform.position.clone();
    let diff = (p.get_at(7.0) - keyed.get_at(7.0)).length();
    assert!(diff > 1e-3, "noise adds on top");
    execute(&mut s, Command::ListSetWeight { id: b.0, track: "Position".into(), index: 1, weight: 0.0 }).unwrap();
    assert!((s.scene.node(b).unwrap().transform.position.get_at(7.0) - keyed.get_at(7.0)).length() < 1e-5);
    // Moving with Auto Key off still edits the keyed (first) entry.
    execute(&mut s, Command::SetFrame { frame: 10 }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 5.0, 0.0], system: None }).unwrap();
    assert!((s.scene.node(b).unwrap().transform.position.get_at(10.0).y - 5.0).abs() < 1e-3);
}
