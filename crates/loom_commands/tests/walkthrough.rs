//! Animation > Walkthrough Assistant: a camera on a path, eye level, head
//! turn and tilt (keyed under Auto Key), targeted camera's target.

use glam::{Mat4, Vec3};
use loom_commands::{execute, AppState, Command, PrimitiveKind};
use loom_scene::{NodeId, ObjectKind};

fn line(s: &mut AppState, pts: &[[f32; 3]]) -> NodeId {
    let r = execute(s, Command::LineStart { point: pts[0] }).unwrap();
    let id = r["id"].as_u64().unwrap();
    for p in &pts[1..] {
        execute(s, Command::LineAddKnot { node: id, point: *p, smooth: false }).unwrap();
    }
    execute(s, Command::LineEnd { node: id }).unwrap();
    NodeId(id)
}

fn world_at(s: &mut AppState, id: NodeId, f: i32) -> Mat4 {
    execute(s, Command::SetFrame { frame: f }).unwrap();
    s.scene.world_matrix(id)
}

/// The camera's view direction (local -Z) in the world.
fn look(m: Mat4) -> Vec3 {
    (-m.z_axis.truncate()).normalize()
}

#[test]
fn walkthrough_camera_follows_its_path_at_eye_level_and_turns_its_head() {
    let mut s = AppState::new();
    execute(&mut s, Command::WalkthroughAssistant).unwrap();
    assert_eq!(s.dialog, Some(loom_commands::Dialog::WalkthroughAssistant));
    let path = line(&mut s, &[[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [100.0, 100.0, 0.0]]);
    let r = execute(&mut s, Command::WalkthroughCreateCamera { targeted: false }).unwrap();
    assert_eq!(r["name"], "Walkthrough_Cam01");
    let cam = NodeId(r["id"].as_u64().unwrap());
    assert!(matches!(s.scene.node(cam).unwrap().object, ObjectKind::Camera { target: None, .. }));
    assert_eq!(execute(&mut s, Command::WalkthroughCreateCamera { targeted: false }).unwrap()["name"], "Walkthrough_Cam02");
    execute(&mut s, Command::WalkthroughSelectCamera { id: cam.0 }).unwrap();

    // Pick Path: the next clicked shape.
    execute(&mut s, Command::WalkthroughPickPath { target: false }).unwrap();
    execute(&mut s, Command::PickHit { id: path.0 }).unwrap();
    assert_eq!(s.pick_mode, None);
    assert_eq!(loom_commands::walkthrough::path_of(&s, cam), Some(path));
    let (s0, s1) = s.frame_range;
    let m = world_at(&mut s, cam, s0);
    assert!(m.w_axis.truncate().distance(Vec3::ZERO) < 1e-2, "{:?}", m.w_axis);
    // Looking down the path, level.
    assert!(look(m).distance(Vec3::X) < 1e-2, "{:?}", look(m));
    assert!(m.y_axis.truncate().normalize().distance(Vec3::Z) < 1e-2, "up stays up: {:?}", m.y_axis);
    let m = world_at(&mut s, cam, s1);
    assert!(m.w_axis.truncate().distance(Vec3::new(100.0, 100.0, 0.0)) < 1e-1);
    assert!(look(m).distance(Vec3::Y) < 1e-2, "turns with the path: {:?}", look(m));

    // Move Path to Eye Level and back.
    execute(&mut s, Command::WalkthroughEyeLevel { camera: cam.0, on: true, level: 60.0 }).unwrap();
    assert!((world_at(&mut s, cam, s0).w_axis.z - 60.0).abs() < 1e-3);
    execute(&mut s, Command::WalkthroughEyeLevel { camera: cam.0, on: false, level: 60.0 }).unwrap();
    assert!(world_at(&mut s, cam, s0).w_axis.z.abs() < 1e-3);

    // Turn Head left (keyed under Auto Key) and tilt it up.
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::SetFrame { frame: s0 + 10 }).unwrap();
    execute(&mut s, Command::WalkthroughHead { camera: cam.0, turn: Some(90.0), tilt: Some(30.0) }).unwrap();
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    let d = look(world_at(&mut s, cam, s0 + 10));
    // Along X at this frame, so left is +Y; 30 degrees up.
    assert!(d.distance(Vec3::new(0.0, 30f32.to_radians().cos(), 30f32.to_radians().sin())) < 1e-2, "{d:?}");
    assert!(s.scene.node(cam).unwrap().transform.rotation.is_animated(), "keyed under Auto Key");
    let (turn, tilt) = loom_commands::walkthrough::head_angles(s.scene.node(cam).unwrap().transform.rotation_quat());
    assert!((turn - 90.0).abs() < 1e-2 && (tilt - 30.0).abs() < 1e-2, "{turn} {tilt}");
    // Remove All Head Animation: level, down the path, no keys.
    execute(&mut s, Command::WalkthroughRemoveHeadAnimation { camera: cam.0 }).unwrap();
    assert!(!s.scene.node(cam).unwrap().transform.rotation.is_animated());
    assert!(look(world_at(&mut s, cam, s0 + 10)).distance(Vec3::X) < 1e-2);
    // Clear Path.
    execute(&mut s, Command::WalkthroughSetPath { camera: cam.0, path: None }).unwrap();
    assert_eq!(loom_commands::walkthrough::path_of(&s, cam), None);
}

#[test]
fn walkthrough_targeted_camera_looks_at_the_picked_object() {
    let mut s = AppState::new();
    let r = execute(&mut s, Command::WalkthroughCreateCamera { targeted: true }).unwrap();
    let cam = NodeId(r["id"].as_u64().unwrap());
    let t = NodeId(r["target"].as_u64().unwrap());
    assert_eq!(s.scene.node(t).unwrap().name, "Walkthrough_Cam01.Target");
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0, 200.0, 0.0], size: [10.0; 3] }).unwrap();
    let b = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::WalkthroughPickPath { target: true }).unwrap();
    execute(&mut s, Command::PickHit { id: b.0 }).unwrap();
    let m = world_at(&mut s, cam, 0);
    assert!(look(m).distance(Vec3::Y) < 1e-2, "{:?}", look(m));
}
