//! Biped footsteps: Footstep Mode, Create Multiple Footsteps (walk, run,
//! jump), Create Footsteps (append), activation keys that make the biped
//! walk over the footsteps, deactivation, deletion and the viewport display.

use glam::Vec3;
use loom_commands::{execute, AppState, Command};
use loom_scene::biped::BipedStructure;
use loom_scene::{NodeId, ObjectKind};
use serde_json::{json, Value};

/// A command by registry name; `{}` stands for no arguments.
fn cmd(name: &str, args: Value) -> Command {
    let v = if args == json!({}) { json!({ "cmd": name }) } else { json!({ "cmd": name, "args": args }) };
    serde_json::from_value(v.clone()).or_else(|_| serde_json::from_value(json!({ "cmd": name, "args": args }))).unwrap_or_else(|e| panic!("{v}: {e}"))
}

fn create(s: &mut AppState) -> NodeId {
    let r = execute(s, cmd("Create Biped", json!({ "position": [0.0, 0.0, 0.0] }))).unwrap();
    NodeId(r["root"].as_u64().unwrap())
}

fn structure(s: &AppState, root: NodeId) -> BipedStructure {
    match &s.scene.node(root).unwrap().object {
        ObjectKind::Biped(b) => (**b).clone(),
        _ => panic!("not a biped"),
    }
}

fn id_of(s: &AppState, name: &str) -> NodeId {
    s.scene.nodes().find(|n| n.name == name).unwrap_or_else(|| panic!("no node {name}")).id
}

fn pos(s: &AppState, name: &str) -> Vec3 {
    s.scene.world_matrix(id_of(s, name)).transform_point3(Vec3::ZERO)
}

/// A biped in Footstep Mode with `n` walking footsteps.
fn walking(s: &mut AppState, n: u32) -> NodeId {
    let root = create(s);
    execute(s, cmd("Biped Footstep Mode", json!({ "on": true }))).unwrap();
    execute(s, cmd("Biped Create Multiple Footsteps", json!({ "count": n }))).unwrap();
    root
}

#[test]
fn footstep_mode_is_needed_and_leaves_figure_mode() {
    let mut s = AppState::new();
    let root = create(&mut s);
    assert!(execute(&mut s, cmd("Biped Create Multiple Footsteps", json!({}))).is_err(), "footsteps need Footstep Mode");
    execute(&mut s, cmd("Biped Footstep Mode", json!({}))).unwrap();
    let b = structure(&s, root);
    assert!(b.footstep_mode && !b.figure_mode, "Footstep Mode turns Figure Mode off");
    execute(&mut s, Command::BipedFigureMode { on: Some(true) }).unwrap();
    assert!(!structure(&s, root).footstep_mode, "and Figure Mode turns Footstep Mode off");
}

#[test]
fn create_multiple_footsteps_walk_alternates_with_walk_timing() {
    let mut s = AppState::new();
    let root = walking(&mut s, 4);
    let b = structure(&s, root);
    let f = &b.footsteps;
    assert_eq!(f.len(), 4);
    assert_eq!(f.iter().map(|f| f.left).collect::<Vec<_>>(), vec![true, false, true, false], "Start Left, alternating");
    assert!(f.iter().all(|f| !f.active), "new footsteps are inactive");
    // Walk Footstep 15, Double Support 3: each starts 12 frames after the previous.
    assert_eq!(f.iter().map(|f| (f.start, f.end)).collect::<Vec<_>>(), vec![(0.0, 15.0), (12.0, 27.0), (24.0, 39.0), (36.0, 51.0)]);
    // Forward is -Y; a stride of 0.75 leg lengths each, a pelvis width apart.
    let h = b.height.get();
    let leg = (loom_scene::biped::COM_HEIGHT - 0.03) * h;
    for (i, fs) in f.iter().enumerate() {
        assert!((fs.position.y + 0.75 * leg * i as f32).abs() < 1e-3, "footstep {i} at {}", fs.position);
        assert!((fs.position.x - if fs.left { 0.055 } else { -0.055 } * h).abs() < 1e-3);
        assert!(fs.position.z.abs() < 1e-3, "on the ground");
    }
    // Appending continues after the last one: the next foot, the next times, further on.
    execute(&mut s, cmd("Biped Create Multiple Footsteps", json!({ "count": 2 }))).unwrap();
    let f = structure(&s, root).footsteps;
    assert_eq!(f.len(), 6);
    assert!(f[4].left && (f[4].start - 48.0).abs() < 1e-4);
    assert!((f[4].position.y + 4.0 * 0.75 * leg).abs() < 1e-3);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(structure(&s, root).footsteps.len(), 4, "one undo");
}

#[test]
fn run_and_jump_timing() {
    let mut s = AppState::new();
    let root = create(&mut s);
    execute(&mut s, cmd("Biped Footstep Mode", json!({ "on": true }))).unwrap();
    execute(&mut s, cmd("Biped Footstep Creation", json!({ "gait": "Run", "run_footstep": 8, "run_airborne": 4 }))).unwrap();
    execute(&mut s, cmd("Biped Create Multiple Footsteps", json!({ "count": 3, "start_left": false }))).unwrap();
    let f = structure(&s, root).footsteps;
    assert_eq!(f.iter().map(|f| (f.left, f.start, f.end)).collect::<Vec<_>>(), vec![(false, 0.0, 8.0), (true, 12.0, 20.0), (false, 24.0, 32.0)], "airborne between footsteps");
    let leg = (loom_scene::biped::COM_HEIGHT - 0.03) * structure(&s, root).height.get();
    assert!((f[1].position.y + 1.5 * leg).abs() < 1e-3, "run stride 1.5 leg lengths");
    execute(&mut s, cmd("Biped Delete Footsteps", json!({}))).unwrap();
    assert!(structure(&s, root).footsteps.is_empty());
    // A jump: both feet down together for 2 Feet Down, then airborne.
    execute(&mut s, cmd("Biped Create Multiple Footsteps", json!({ "gait": "Jump", "count": 4 }))).unwrap();
    let f = structure(&s, root).footsteps;
    assert_eq!(f.iter().map(|f| (f.left, f.start, f.end)).collect::<Vec<_>>(), vec![(true, 0.0, 15.0), (false, 0.0, 15.0), (true, 28.0, 43.0), (false, 28.0, 43.0)]);
    assert!((f[0].position.y - f[1].position.y).abs() < 1e-4 && (f[2].position.y + 2.4 * leg).abs() < 1e-3);
}

#[test]
fn activation_walks_the_biped_over_its_footsteps() {
    let mut s = AppState::new();
    let root = walking(&mut s, 6);
    let h = structure(&s, root).height.get();
    let com0 = pos(&s, "Bip001");
    execute(&mut s, cmd("Biped Create Keys for Inactive Footsteps", json!({}))).unwrap();
    let b = structure(&s, root);
    assert!(b.footsteps.iter().all(|f| f.active), "every footstep is active");
    assert!(s.frame_range.1 >= 75, "the active time segment holds the footsteps: {:?}", s.frame_range);
    let ankle = loom_scene::biped::ANKLE_HEIGHT * h;
    // Mid-way through each footstep the foot is on it.
    for (i, f) in b.footsteps.iter().enumerate() {
        let mid = ((f.start + f.end) / 2.0).round() as i32;
        execute(&mut s, Command::SetFrame { frame: mid }).unwrap();
        let foot = pos(&s, if f.left { "Bip001 L Foot" } else { "Bip001 R Foot" });
        let target = f.position + Vec3::Z * ankle;
        assert!(foot.distance(target) < 0.03 * h, "footstep {i} at frame {mid}: foot {foot}, footstep {target}");
    }
    // Between its footsteps a foot swings off the ground.
    let (a, c) = (&b.footsteps[1], &b.footsteps[3]);
    let swing = ((a.end + c.start) / 2.0).round() as i32;
    execute(&mut s, Command::SetFrame { frame: swing }).unwrap();
    assert!(pos(&s, "Bip001 R Foot").z > ankle + 0.02 * h, "the right foot is lifted at frame {swing}");
    // The centre of mass travels forward with the feet and stays up.
    execute(&mut s, Command::SetFrame { frame: 0 }).unwrap();
    assert!(pos(&s, "Bip001").distance(com0) < 0.03 * h, "it starts where it stood");
    execute(&mut s, Command::SetFrame { frame: 60 }).unwrap();
    let com = pos(&s, "Bip001");
    assert!(com.y < com0.y - 0.5 * h, "it walked forward (-Y): {com} from {com0}");
    assert!(com.z > 0.4 * h && com.z < com0.z + 0.01 * h, "upright: {com}");
    // Undo removes the keys in one step.
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(pos(&s, "Bip001").distance(com0) < 1e-3, "back where it stood at frame 60");
    assert!(structure(&s, root).footsteps.iter().all(|f| !f.active));
}

#[test]
fn deactivate_and_delete_remove_the_keys() {
    let mut s = AppState::new();
    let root = walking(&mut s, 4);
    let com0 = pos(&s, "Bip001");
    execute(&mut s, cmd("Biped Create Keys for Inactive Footsteps", json!({}))).unwrap();
    execute(&mut s, Command::SetFrame { frame: 40 }).unwrap();
    assert!(pos(&s, "Bip001").distance(com0) > 1.0);
    execute(&mut s, cmd("Biped Deactivate Footsteps", json!({}))).unwrap();
    assert!(pos(&s, "Bip001").distance(com0) < 1e-3, "no footstep keys left");
    let b = structure(&s, root);
    assert_eq!(b.footsteps.len(), 4);
    assert!(b.footsteps.iter().all(|f| !f.active));
    execute(&mut s, cmd("Biped Create Keys for Inactive Footsteps", json!({}))).unwrap();
    execute(&mut s, cmd("Biped Delete Footsteps", json!({}))).unwrap();
    assert!(structure(&s, root).footsteps.is_empty());
    assert!(pos(&s, "Bip001").distance(com0) < 1e-3, "deleting active footsteps removes their keys");
    // Activation needs Figure Mode off.
    execute(&mut s, cmd("Biped Create Multiple Footsteps", json!({}))).unwrap();
    execute(&mut s, Command::BipedFigureMode { on: Some(true) }).unwrap();
    assert!(execute(&mut s, cmd("Biped Create Keys for Inactive Footsteps", json!({}))).is_err());
}

#[test]
fn create_footsteps_append_places_clicked_footsteps() {
    let mut s = AppState::new();
    let root = create(&mut s);
    execute(&mut s, cmd("Biped Footstep Mode", json!({ "on": true }))).unwrap();
    execute(&mut s, cmd("Biped Create Footsteps (Append)", json!({}))).unwrap();
    assert_eq!(s.pick_mode, Some(loom_commands::PickMode::BipedFootstep));
    for p in [[5.0, -10.0, 0.0], [-5.0, -30.0, 0.0], [5.0, -50.0, 0.0]] {
        execute(&mut s, Command::PickPoint { point: p }).unwrap();
    }
    let f = structure(&s, root).footsteps;
    assert_eq!(f.iter().map(|f| f.left).collect::<Vec<_>>(), vec![false, true, false], "the first click makes a right footstep");
    assert_eq!(f[1].position, Vec3::new(-5.0, -30.0, 0.0));
    assert_eq!((f[1].start, f[2].start), (12.0, 24.0), "walk timing");
    assert!((f[2].heading - loom_commands::biped_footsteps::heading_of(Vec3::new(10.0, -20.0, 0.0))).abs() < 1e-3, "facing along the path");
}

#[test]
fn footsteps_draw_as_coloured_footprints_with_numbers() {
    use loom_commands::biped_footsteps::{LEFT_ACTIVE, LEFT_INACTIVE, RIGHT_INACTIVE};
    use loom_commands::helpers::{helper_labels, helper_lines, LineKind};
    let mut s = AppState::new();
    let root = walking(&mut s, 3);
    let lines = helper_lines(&s, root, &|_| 1.0);
    let colour = |c: [u8; 3]| lines.iter().filter(|(_, _, k)| *k == LineKind::Rgb(c)).count();
    assert!(colour(LEFT_INACTIVE) > 10 && colour(RIGHT_INACTIVE) > 10, "left blue, right green");
    let labels: Vec<String> = helper_labels(&s, root).into_iter().map(|(_, t)| t).collect();
    assert_eq!(labels, vec!["0", "1", "2"]);
    execute(&mut s, cmd("Biped Create Keys for Inactive Footsteps", json!({}))).unwrap();
    let lines = helper_lines(&s, root, &|_| 1.0);
    assert!(lines.iter().any(|(_, _, k)| *k == LineKind::Rgb(LEFT_ACTIVE)), "active footsteps turn pastel");
}
