//! Fixes from the phase 8 final review: each test reproduces one finding.

use glam::Vec3;
use loom_commands::{execute, AppState, Axis, Command, PrimitiveKind, SelectOp};
use loom_scene::NodeId;

fn boxed(s: &mut AppState, at: [f32; 3]) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: at, size: [4.0; 3] }).unwrap();
    *s.selection.iter().next().unwrap()
}

fn dummy(s: &mut AppState, at: [f32; 3]) -> NodeId {
    execute(s, Command::CreateHelper { kind: "dummy".into(), position: at, size: 1.0 }).unwrap();
    *s.selection.iter().next().unwrap()
}

fn select(s: &mut AppState, id: NodeId) {
    execute(s, Command::SelectNodes { ids: vec![id.0], op: SelectOp::Replace }).unwrap();
}

fn world(s: &mut AppState, id: NodeId, f: i32) -> glam::Mat4 {
    execute(s, Command::SetFrame { frame: f }).unwrap();
    s.scene.world_matrix(id)
}

/// I1: linking under Auto Key is not an animated edit.
#[test]
fn linking_under_auto_key_does_not_key() {
    let mut s = AppState::new();
    let a = boxed(&mut s, [0.0; 3]);
    execute(&mut s, Command::RotateSelection { axis: Axis::Z, degrees: 45.0, system: None }).unwrap();
    let b = boxed(&mut s, [50.0, 0.0, 0.0]);
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::SetFrame { frame: 10 }).unwrap();
    select(&mut s, b);
    execute(&mut s, Command::SelectAndLink).unwrap();
    execute(&mut s, Command::PickHit { id: a.0 }).unwrap();
    let n = s.scene.node(b).unwrap();
    assert_eq!(n.parent, Some(a));
    assert!(!n.transform.position.is_animated() && !n.transform.rotation.is_animated(), "no keys from linking");
    assert!((world(&mut s, b, 0).w_axis.truncate() - Vec3::new(50.0, 0.0, 0.0)).length() < 1e-3);
    execute(&mut s, Command::SetFrame { frame: 10 }).unwrap();
    select(&mut s, b);
    execute(&mut s, Command::UnlinkSelection).unwrap();
    assert!(!s.scene.node(b).unwrap().transform.position.is_animated());
}

/// I2: in Set Key mode, a frame change drops only the pose, not other work.
#[test]
fn set_key_mode_keeps_objects_created_while_posing() {
    let mut s = AppState::new();
    let b = boxed(&mut s, [0.0; 3]);
    execute(&mut s, Command::SetKeyMode).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [10.0, 0.0, 0.0], system: None }).unwrap();
    let c = boxed(&mut s, [30.0, 0.0, 0.0]);
    execute(&mut s, Command::SetFrame { frame: 20 }).unwrap();
    assert!(s.scene.node(c).is_some(), "the new box stays");
    assert_eq!(s.scene.len(), 2);
    let _ = b;
}

/// I3: Link plus LookAt: the look-at works in the link frame.
#[test]
fn link_and_lookat_together() {
    let mut s = AppState::new();
    let carrier = dummy(&mut s, [0.0; 3]);
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::SetFrame { frame: 10 }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 30.0], system: None }).unwrap();
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::SetFrame { frame: 0 }).unwrap();
    let target = dummy(&mut s, [0.0, 50.0, 0.0]);
    let b = boxed(&mut s, [0.0; 3]);
    execute(&mut s, Command::LinkConstraint { target: Some(carrier.0) }).unwrap();
    execute(&mut s, Command::LookAtConstraint { target: target.0 }).unwrap();
    let m = world(&mut s, b, 10);
    assert!((m.w_axis.truncate() - Vec3::new(0.0, 0.0, 30.0)).length() < 1e-3, "{:?}", m.w_axis);
    let want = (Vec3::new(0.0, 50.0, 0.0) - Vec3::new(0.0, 0.0, 30.0)).normalize();
    assert!(m.x_axis.truncate().normalize().distance(want) < 1e-3, "{:?} vs {want:?}", m.x_axis);
}

/// I4: a linked object follows its target when the target is moved without keys.
#[test]
fn link_follows_an_unkeyed_move_of_the_target() {
    let mut s = AppState::new();
    let d = dummy(&mut s, [0.0; 3]);
    let b = boxed(&mut s, [10.0, 0.0, 0.0]);
    execute(&mut s, Command::LinkConstraint { target: Some(d.0) }).unwrap();
    select(&mut s, d);
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 30.0], system: None }).unwrap();
    assert!((world(&mut s, b, 0).w_axis.truncate() - Vec3::new(10.0, 0.0, 30.0)).length() < 1e-3, "{:?}", world(&mut s, b, 0).w_axis);
    assert!((world(&mut s, b, 50).w_axis.truncate() - Vec3::new(10.0, 0.0, 30.0)).length() < 1e-3);
}

/// I5: Paste Time copies a component's keys only into that component.
#[test]
fn paste_time_keeps_components_apart() {
    let mut s = AppState::new();
    let b = boxed(&mut s, [0.0; 3]);
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::SetFrame { frame: 10 }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [10.0, 0.0, 0.0], system: None }).unwrap();
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::OpenDopeSheet).unwrap();
    execute(&mut s, Command::TrackViewHighlight { items: vec![format!("{}:Position", b.0)], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::TrackViewSelectTime { start: 0.0, end: 10.0 }).unwrap();
    execute(&mut s, Command::CopyTime).unwrap();
    execute(&mut s, Command::TrackViewPasteTime { at: Some(50.0), relative: Some(false) }).unwrap();
    let k = s.scene.node(b).unwrap().transform.position.keys().unwrap().clone();
    assert_eq!(k.tracks[0].keys.len(), 4);
    assert!(k.tracks[1].keys.is_empty() && k.tracks[2].keys.is_empty(), "Y and Z stay unkeyed");
}

/// I6: the track bar's key times are cached between repaints.
#[test]
fn track_bar_key_times_are_cached() {
    let mut s = AppState::new();
    let rig = concat!(env!("CARGO_MANIFEST_DIR"), "/../loom_fbx/tests/fixtures/sitting_idle.fbx");
    execute(&mut s, Command::ImportFile { path: Some(rig.into()) }).unwrap();
    let mesh = s.scene.nodes().find(|n| matches!(n.object, loom_scene::ObjectKind::EditablePoly(_))).unwrap().id;
    select(&mut s, mesh);
    let _ = loom_commands::anim_cmds::selection_key_times(&s);
    let t0 = std::time::Instant::now();
    for _ in 0..20 {
        let _ = loom_commands::anim_cmds::selection_key_times(&s);
    }
    assert!(t0.elapsed().as_millis() < 100, "{:?} for 20 repaints", t0.elapsed());
    // Still right after an edit: a new key shows.
    execute(&mut s, Command::SetFrame { frame: 7 }).unwrap();
    execute(&mut s, Command::KeyPosition).unwrap();
    assert!(loom_commands::anim_cmds::selection_key_times(&s).contains(&7.0));
}

/// I6 follow-up: opening another file does not show the previous scene's cached keys.
#[test]
fn key_time_cache_is_per_scene() {
    let dir = std::env::temp_dir().join("loom-review8");
    std::fs::create_dir_all(&dir).unwrap();
    let mk = |frame: i32, name: &str| {
        let mut s = AppState::new();
        let b = boxed(&mut s, [0.0; 3]);
        execute(&mut s, Command::SetFrame { frame }).unwrap();
        execute(&mut s, Command::KeyPosition).unwrap();
        let p = dir.join(name);
        execute(&mut s, Command::SaveFileAs { path: Some(p.to_string_lossy().into_owned()) }).unwrap();
        (p, b)
    };
    let (a, b) = mk(10, "a.loom");
    let (c, _) = mk(30, "c.loom");
    let mut s = AppState::new();
    execute(&mut s, Command::OpenFile { path: Some(a.to_string_lossy().into_owned()) }).unwrap();
    select(&mut s, b);
    assert_eq!(loom_commands::anim_cmds::selection_key_times(&s), vec![10.0]);
    execute(&mut s, Command::OpenFile { path: Some(c.to_string_lossy().into_owned()) }).unwrap();
    select(&mut s, b);
    assert_eq!(loom_commands::anim_cmds::selection_key_times(&s), vec![30.0]);
}
