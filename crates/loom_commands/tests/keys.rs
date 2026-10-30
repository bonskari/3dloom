//! Key commands: Set Key mode, filters, Key Position…, default tangents,
//! track bar edits, playback, time rescale, animation clips (phase 8).

use glam::Vec3;
use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp};
use loom_scene::anim::TangentType;
use loom_scene::tracks::{list_tracks, TrackOwner};
use loom_scene::NodeId;

fn boxed(s: &mut AppState) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    *s.selection.iter().next().unwrap()
}

fn frame(s: &mut AppState, f: i32) {
    execute(s, Command::SetFrame { frame: f }).unwrap();
}

fn mv(s: &mut AppState, d: [f32; 3]) {
    execute(s, Command::MoveSelection { delta: d, system: None }).unwrap();
}

fn pos_keys(s: &AppState, id: NodeId) -> Vec<Vec<f32>> {
    let p = &s.scene.node(id).unwrap().transform.position;
    p.keys().map(|k| k.tracks.iter().map(|t| t.keys.iter().map(|k| k.time).collect()).collect()).unwrap_or_default()
}

fn auto_keyed_box(s: &mut AppState) -> NodeId {
    let b = boxed(s);
    execute(s, Command::AutoKeyModeToggle).unwrap();
    frame(s, 10);
    mv(s, [20.0, 0.0, 0.0]);
    frame(s, 30);
    mv(s, [0.0, 0.0, 0.0]);
    mv(s, [10.0, 0.0, 0.0]);
    execute(s, Command::AutoKeyModeToggle).unwrap();
    b
}

#[test]
fn tracks_lists_object_and_modifier_params() {
    let mut s = AppState::new();
    let b = boxed(&mut s);
    execute(&mut s, Command::AddModifier { name: "Bend".into() }).unwrap();
    let t = list_tracks(&s.scene, b);
    let names: Vec<&str> = t.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"Position") && names.contains(&"Rotation") && names.contains(&"Scale"), "{names:?}");
    assert!(t.iter().any(|t| t.owner == TrackOwner::Object && t.name == "Height"), "{names:?}");
    assert!(t.iter().any(|t| t.owner == TrackOwner::Modifier(0) && t.name == "Bend / Angle"), "{names:?}");
}

#[test]
fn set_keys_respects_filters() {
    let mut s = AppState::new();
    let b = boxed(&mut s);
    frame(&mut s, 5);
    // Default filters: Position, Rotation, Scale, IK Parameters.
    execute(&mut s, Command::SetKeys).unwrap();
    let n = s.scene.node(b).unwrap();
    assert!(n.transform.position.keys().unwrap().tracks.iter().all(|t| t.keys.len() == 1));
    assert!(n.transform.rotation.keys().is_some() && n.transform.scale.keys().is_some());
    let height = list_tracks(&s.scene, b).into_iter().find(|t| t.name == "Height").unwrap();
    assert!(!loom_scene::tracks::read(&s.scene, &height).unwrap().is_animated(), "Object Params filter is off");
    execute(&mut s, Command::SetKeyObjectParamsFilter).unwrap();
    execute(&mut s, Command::SetKeyPositionFilter).unwrap();
    execute(&mut s, Command::SetKeyRotationFilter).unwrap();
    execute(&mut s, Command::SetKeyScaleFilter).unwrap();
    frame(&mut s, 15);
    execute(&mut s, Command::SetKeys).unwrap();
    assert!(loom_scene::tracks::read(&s.scene, &height).unwrap().is_animated());
    assert_eq!(pos_keys(&s, b)[0], vec![5.0], "Position filter off: no key at 15");
}

#[test]
fn set_key_mode_buffer_is_discarded_on_frame_change_and_kept_by_k() {
    let mut s = AppState::new();
    let b = boxed(&mut s);
    execute(&mut s, Command::SetKeyMode).unwrap();
    frame(&mut s, 10);
    mv(&mut s, [20.0, 0.0, 0.0]);
    assert!((s.scene.world_matrix(b).w_axis.x - 20.0).abs() < 1e-4, "the pose shows");
    frame(&mut s, 20);
    assert!(s.scene.world_matrix(b).w_axis.x.abs() < 1e-4, "changing frame discards the pose");
    assert!(!s.scene.node(b).unwrap().transform.position.is_animated());
    mv(&mut s, [30.0, 0.0, 0.0]);
    execute(&mut s, Command::SetKeys).unwrap();
    let p = &s.scene.node(b).unwrap().transform.position;
    assert!((p.get_at(20.0).x - 30.0).abs() < 1e-4);
    assert_eq!(pos_keys(&s, b)[0], vec![20.0], "K keyed the pose at 20");
    // The pose was turned into keys, so moving on keeps it.
    frame(&mut s, 40);
    assert!((s.scene.world_matrix(b).w_axis.x - 30.0).abs() < 1e-4);
    // Clear Set Key Mode Buffer drops a pose.
    mv(&mut s, [5.0, 0.0, 0.0]);
    execute(&mut s, Command::ClearSetKeyModeBuffer).unwrap();
    assert!((s.scene.world_matrix(b).w_axis.x - 30.0).abs() < 1e-4);
    // Auto Key and Set Key exclude each other.
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    assert!(s.auto_key && !s.set_key_mode);
}

#[test]
fn key_position_x_keys_one_axis() {
    let mut s = AppState::new();
    let b = boxed(&mut s);
    frame(&mut s, 7);
    execute(&mut s, Command::KeyPositionX).unwrap();
    assert_eq!(pos_keys(&s, b), vec![vec![7.0], vec![], vec![]]);
    execute(&mut s, Command::KeyRotation).unwrap();
    let r = s.scene.node(b).unwrap().transform.rotation.keys().unwrap().tracks.iter().map(|t| t.keys.len()).collect::<Vec<_>>();
    assert_eq!(r, vec![1, 1, 1]);
    execute(&mut s, Command::KeyAll).unwrap();
    let height = list_tracks(&s.scene, b).into_iter().find(|t| t.name == "Height").unwrap();
    assert!(loom_scene::tracks::read(&s.scene, &height).unwrap().is_animated());
    // Create Key copies the frame-7 transform to frame 50.
    execute(&mut s, Command::CreateKey { source: Some(7), destination: Some(50), position: Some(true), rotation: Some(false), scale: Some(false) }).unwrap();
    assert!(pos_keys(&s, b)[0].contains(&50.0));
    // A Position Lock Key stops drift: flat tangents.
    frame(&mut s, 60);
    execute(&mut s, Command::CreatePositionLockKey).unwrap();
    let k = s.scene.node(b).unwrap().transform.position.keys().unwrap().tracks[0].keys.iter().find(|k| k.time == 60.0).unwrap().clone();
    assert_eq!((k.in_type, k.out_type, k.in_tan, k.out_tan), (TangentType::Custom, TangentType::Custom, 0.0, 0.0));
}

#[test]
fn default_tangent_applies_to_new_keys() {
    let mut s = AppState::new();
    let b = boxed(&mut s);
    execute(&mut s, Command::SetDefaultTangentAsLinear).unwrap();
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    frame(&mut s, 10);
    mv(&mut s, [20.0, 0.0, 0.0]);
    let k = &s.scene.node(b).unwrap().transform.position.keys().unwrap().tracks[0].keys;
    assert!(k.iter().all(|k| k.in_type == TangentType::Linear && k.out_type == TangentType::Linear));
    execute(&mut s, Command::SetDefaultTangentAsStepped).unwrap();
    frame(&mut s, 20);
    mv(&mut s, [5.0, 0.0, 0.0]);
    let k = &s.scene.node(b).unwrap().transform.position.keys().unwrap().tracks[0].keys;
    assert_eq!(k[2].in_type, TangentType::Step);
    let r = execute(&mut s, Command::State).unwrap();
    assert_eq!(r["animation"]["default_tangent"], "Step");
}

#[test]
fn next_previous_key_step_over_selection_keys() {
    let mut s = AppState::new();
    let _b = auto_keyed_box(&mut s);
    frame(&mut s, 0);
    execute(&mut s, Command::GoToNextKey).unwrap();
    assert_eq!(s.current_frame, 10);
    execute(&mut s, Command::GoToNextKey).unwrap();
    assert_eq!(s.current_frame, 30);
    execute(&mut s, Command::GoToPreviousKey).unwrap();
    assert_eq!(s.current_frame, 10);
    // Key Mode makes , and . step between keys.
    execute(&mut s, Command::NextPreviousKeyModeToggle).unwrap();
    execute(&mut s, Command::ForwardTimeOneUnit).unwrap();
    assert_eq!(s.current_frame, 30);
    execute(&mut s, Command::BackupTimeOneUnit).unwrap();
    assert_eq!(s.current_frame, 10);
}

#[test]
fn track_bar_move_copy_delete_keys() {
    let mut s = AppState::new();
    let b = auto_keyed_box(&mut s);
    execute(&mut s, Command::MoveKeysInTime { from: 10, to: 10, delta: 5 }).unwrap();
    assert_eq!(pos_keys(&s, b)[0], vec![0.0, 15.0, 30.0]);
    let x15 = s.scene.node(b).unwrap().transform.position.get_at(15.0).x;
    assert!((x15 - 20.0).abs() < 1e-4);
    execute(&mut s, Command::CopyKeys { from: 15, to: 15, delta: 25 }).unwrap();
    assert_eq!(pos_keys(&s, b)[0], vec![0.0, 15.0, 30.0, 40.0]);
    execute(&mut s, Command::DeleteKeys { from: 14, to: 31 }).unwrap();
    assert_eq!(pos_keys(&s, b)[0], vec![0.0, 40.0]);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(pos_keys(&s, b)[0], vec![0.0, 15.0, 30.0, 40.0]);
}

#[test]
fn rescale_time_scales_keys() {
    let mut s = AppState::new();
    let b = auto_keyed_box(&mut s);
    execute(&mut s, Command::RescaleTime { start: 0, end: 200 }).unwrap();
    assert_eq!(s.frame_range, (0, 200));
    assert_eq!(pos_keys(&s, b)[0], vec![0.0, 20.0, 60.0]);
}

#[test]
fn playback_advances_and_loops() {
    let mut s = AppState::new();
    execute(&mut s, Command::SetTimeConfiguration { start: Some(0), end: Some(10), fps: Some(30.0) }).unwrap();
    execute(&mut s, Command::PlayAnimation).unwrap();
    assert!(s.playing);
    // Real Time: one second advances 30 frames, looping in 0..10.
    execute(&mut s, Command::TickPlayback { seconds: 0.2 }).unwrap();
    assert_eq!(s.current_frame, 6);
    execute(&mut s, Command::TickPlayback { seconds: 0.2 }).unwrap();
    assert_eq!(s.current_frame, 1, "12 wraps to 1 in an 11-frame loop");
    // Every frame, no loop: stops at the end.
    execute(&mut s, Command::SetPlayback { real_time: Some(false), loop_playback: Some(false), speed: None, direction: None }).unwrap();
    for _ in 0..20 {
        execute(&mut s, Command::TickPlayback { seconds: 0.01 }).unwrap();
    }
    assert_eq!(s.current_frame, 10);
    assert!(!s.playing, "stops at the end without Loop");
    execute(&mut s, Command::PlayAnimation).unwrap();
    execute(&mut s, Command::PlayAnimation).unwrap();
    assert!(!s.playing, "/ toggles");
}

#[test]
fn save_and_load_animation_clip() {
    let dir = std::env::temp_dir().join("loom-keys-tests");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("clip.lxaf").to_string_lossy().into_owned();
    let mut s = AppState::new();
    let b = auto_keyed_box(&mut s);
    execute(&mut s, Command::SelectNodes { ids: vec![b.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SaveAnimation { path: Some(path.clone()) }).unwrap();
    let mut t = AppState::new();
    let c = boxed(&mut t);
    assert_eq!(t.scene.node(c).unwrap().name, s.scene.node(b).unwrap().name);
    execute(&mut t, Command::LoadAnimation { path: Some(path), offset: Some(100) }).unwrap();
    let p = &t.scene.node(c).unwrap().transform.position;
    assert!((p.get_at(110.0) - Vec3::new(20.0, 0.0, 0.0)).length() < 1e-4, "{:?}", p.get_at(110.0));
    assert!((p.get_at(130.0).x - 30.0).abs() < 1e-4);
}

#[test]
fn key_shortcuts_are_bound() {
    let k = loom_commands::keymap::Keymap::main_ui();
    let name = |c: &str| k.lookup(loom_commands::keymap::parse_chord(c).unwrap());
    assert_eq!(name("K"), Some("Set Keys"));
    assert_eq!(name("'"), Some("Set Key Mode"));
    assert_eq!(name("/"), Some("Play Animation"));
    assert_eq!(name("Shift+."), Some("Go To Next Key"));
    assert_eq!(name("Shift+,"), Some("Go To Previous Key"));
}

/// The Selection List next to Auto Key / Set Key: Set Keys keys the chosen
/// named selection set instead of the current selection.
#[test]
fn set_keys_keys_the_selection_list_set() {
    let mut s = AppState::new();
    let a = boxed(&mut s);
    let b = boxed(&mut s);
    execute(&mut s, Command::SelectNodes { ids: vec![b.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::NamedSelectionSet { name: "Rig".into() }).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SetKeySelectionList { name: Some("Rig".into()) }).unwrap();
    assert_eq!(s.key_selection_list.as_deref(), Some("Rig"));
    frame(&mut s, 5);
    execute(&mut s, Command::SetKeys).unwrap();
    assert!(pos_keys(&s, a).iter().all(|t| t.is_empty()), "the selected box is not in the list");
    assert!(pos_keys(&s, b).iter().any(|t| t.contains(&5.0)), "the Rig set is keyed");

    assert!(execute(&mut s, Command::SetKeySelectionList { name: Some("Nope".into()) }).is_err());
    execute(&mut s, Command::SetKeySelectionList { name: None }).unwrap();
    frame(&mut s, 8);
    execute(&mut s, Command::SetKeys).unwrap();
    assert!(pos_keys(&s, a).iter().any(|t| t.contains(&8.0)), "back to Selected");
}

fn key_steps(s: &mut AppState, track_bar: bool, selected_only: bool, current: bool, prs: [bool; 3]) {
    execute(s, Command::SetKeySteps { use_track_bar: Some(track_bar), selected_only: Some(selected_only), use_current_transform: Some(current), position: Some(prs[0]), rotation: Some(prs[1]), scale: Some(prs[2]) }).unwrap();
}

fn next_from(s: &mut AppState, f: i32) -> i32 {
    frame(s, f);
    execute(s, Command::GoToNextKey).unwrap();
    s.current_frame
}

#[test]
fn key_steps_pick_the_keys_key_mode_steps_between() {
    let mut s = AppState::new();
    let _b = auto_keyed_box(&mut s); // position keys at 10 and 30
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    frame(&mut s, 20);
    execute(&mut s, Command::RotateSelection { axis: loom_commands::Axis::Z, degrees: 30.0, system: None }).unwrap();
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    assert_eq!(next_from(&mut s, 0), 10, "Use TrackBar (default): every key");
    // Use Current Transform: the Rotate tool steps over rotation keys only.
    key_steps(&mut s, false, true, true, [true; 3]);
    execute(&mut s, Command::SelectAndRotate).unwrap();
    assert_eq!(next_from(&mut s, 0), 20);
    execute(&mut s, Command::SelectAndMove).unwrap();
    assert_eq!(next_from(&mut s, 10), 30, "the Move tool skips the rotation key at 20");
    // Position / Rotation / Scale toggles when Use Current Transform is off.
    key_steps(&mut s, false, true, false, [false, true, false]);
    assert_eq!(next_from(&mut s, 0), 20);
    // Selected Objects Only off: the keys of unselected objects count too.
    execute(&mut s, Command::SelectNone).unwrap();
    assert_eq!(next_from(&mut s, 0), 0, "nothing selected: nowhere to go");
    key_steps(&mut s, false, false, false, [true, false, false]);
    assert_eq!(next_from(&mut s, 0), 10);
}

#[test]
fn time_display_and_key_steps_are_saved_with_the_scene() {
    use loom_commands::time_display::TimeDisplay;
    let dir = std::env::temp_dir().join("loom-time-display-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("t.loom").to_string_lossy().into_owned();
    let mut s = AppState::new();
    execute(&mut s, Command::SetTimeDisplay { mode: TimeDisplay::Smpte }).unwrap();
    key_steps(&mut s, false, false, true, [true, false, true]);
    execute(&mut s, Command::SaveFileAs { path: Some(path.clone()) }).unwrap();
    let mut t = AppState::new();
    execute(&mut t, Command::OpenFile { path: Some(path) }).unwrap();
    assert_eq!(t.time_config, s.time_config);
    let r = execute(&mut t, Command::SetTimeDisplay { mode: TimeDisplay::MmSsTicks }).unwrap();
    assert_eq!(r["display"], "MmSsTicks");
    execute(&mut t, Command::ResetFile).unwrap();
    assert_eq!(t.time_config, Default::default(), "Reset puts the defaults back");
}
