//! Expression, Spring, Smooth Rotation, Audio and Noise controllers:
//! assigned, evaluated, edited, undone, saved and opened.

use glam::Vec3;
use loom_commands::{execute, AppState, Command, PrimitiveKind};
use loom_scene::NodeId;

fn boxed(s: &mut AppState, at: [f32; 3]) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: at, size: [10.0; 3] }).unwrap();
    *s.selection.iter().next().unwrap()
}

fn tmp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("loom-ctl23-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn pos_at(s: &mut AppState, id: NodeId, f: i32) -> Vec3 {
    execute(s, Command::SetFrame { frame: f }).unwrap();
    s.scene.world_matrix(id).w_axis.truncate()
}

fn assign(s: &mut AppState, id: NodeId, track: &str, c: &str) {
    execute(s, Command::AssignController { id: id.0, track: track.into(), controller: c.into() }).unwrap();
}

fn undo(s: &mut AppState) {
    execute(s, Command::UndoSceneOperation).unwrap();
}

#[test]
fn position_expression_circles_once_per_segment_with_a_constant_radius() {
    let mut s = AppState::new();
    let b = boxed(&mut s, [7.0, 8.0, 9.0]);
    assign(&mut s, b, "Position", "Position Expression");
    // The new controller holds the current value and opens its dialog.
    assert_eq!(s.dialog, Some(loom_commands::Dialog::ExpressionController));
    assert_eq!(s.ctl_track, Some((b.0, "Position".into())));
    assert!(pos_at(&mut s, b, 30).distance(Vec3::new(7.0, 8.0, 9.0)) < 1e-4);
    let t = |s: &AppState| s.scene.node(b).unwrap().clone();
    let r = loom_scene::tracks::AnyParam::Vec3(t(&s).transform.position.clone()).controller_name(loom_scene::tracks::Role::Position);
    assert_eq!(r, "Position Expression");
    let set = |s: &mut AppState, e: &str| execute(s, Command::ExpressionSet { id: b.0, track: "Position".into(), expression: Some(e.into()), description: None });
    set(&mut s, "[100*cos(360*NT), 100*sin(360*NT), 0]").unwrap();
    assert!(pos_at(&mut s, b, 0).distance(Vec3::new(100.0, 0.0, 0.0)) < 1e-3);
    assert!(pos_at(&mut s, b, 25).distance(Vec3::new(0.0, 100.0, 0.0)) < 1e-3, "a quarter of the 0-100 segment");
    // radius: a scalar holding 150.
    execute(&mut s, Command::ExpressionCreateVariable { id: b.0, track: "Position".into(), name: "radius".into(), vector: false }).unwrap();
    assert!(execute(&mut s, Command::ExpressionCreateVariable { id: b.0, track: "Position".into(), name: "radius".into(), vector: true }).is_err(), "names are unique");
    assert!(execute(&mut s, Command::ExpressionCreateVariable { id: b.0, track: "Position".into(), name: "NT".into(), vector: false }).is_err(), "predefined names are taken");
    execute(&mut s, Command::ExpressionAssignConstant { id: b.0, track: "Position".into(), name: "radius".into(), value: [150.0, 0.0, 0.0] }).unwrap();
    set(&mut s, "[radius*cos(360*NT), radius*sin(360*NT), 0]").unwrap();
    assert!(pos_at(&mut s, b, 50).distance(Vec3::new(-150.0, 0.0, 0.0)) < 1e-2);
    // A syntax error is refused with the expression up to the error.
    assert_eq!(set(&mut s, "[radius*cos(360*NT), oops, 0]"), Err("[radius*cos(360*NT), oops".into()));
    assert!(set(&mut s, "radius").is_err(), "a position needs a vector");
    // Undo takes back the last edit only.
    undo(&mut s);
    assert!(pos_at(&mut s, b, 50).distance(Vec3::new(-100.0, 0.0, 0.0)) < 1e-2, "back to the 100 expression");
    execute(&mut s, Command::RedoSceneOperation).unwrap();
    // Rename and delete.
    execute(&mut s, Command::ExpressionRenameVariable { id: b.0, track: "Position".into(), name: "radius".into(), new_name: "r".into() }).unwrap();
    let e: loom_scene::expr::ExprCtrl = loom_commands::ctl_props::read_ctl(&s, b, "Position", "Expression").unwrap().1;
    assert_eq!(e.vars[0].name, "r");
    execute(&mut s, Command::ExpressionDeleteVariable { id: b.0, track: "Position".into(), name: "r".into() }).unwrap();
    // Save writes the text; Load brings it back (without variables).
    let x = tmp("circle.xpr");
    execute(&mut s, Command::ExpressionSave { id: b.0, track: "Position".into(), path: x.to_string_lossy().into() }).unwrap();
    assert_eq!(std::fs::read_to_string(&x).unwrap(), "[radius*cos(360*NT), radius*sin(360*NT), 0]");
    set(&mut s, "[0,0,0]").unwrap();
    execute(&mut s, Command::ExpressionLoad { id: b.0, track: "Position".into(), path: x.to_string_lossy().into() }).unwrap();
    let e: loom_scene::expr::ExprCtrl = loom_commands::ctl_props::read_ctl(&s, b, "Position", "Expression").unwrap().1;
    assert_eq!(e.expression, "[radius*cos(360*NT), radius*sin(360*NT), 0]");
    assert!(e.vars.is_empty());
}

#[test]
fn a_vector_variable_follows_another_objects_position_track() {
    let mut s = AppState::new();
    let leader = boxed(&mut s, [0.0, 0.0, 0.0]);
    // The leader moves from 0 to 100 in X over frames 0-20.
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::SetFrame { frame: 20 }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [100.0, 0.0, 0.0], system: None }).unwrap();
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    let b = boxed(&mut s, [0.0, 0.0, 0.0]);
    assign(&mut s, b, "Position", "Position Expression");
    let tr = "Position".to_owned();
    execute(&mut s, Command::ExpressionCreateVariable { id: b.0, track: tr.clone(), name: "boxposn".into(), vector: true }).unwrap();
    assert!(execute(&mut s, Command::ExpressionAssignController { id: b.0, track: tr.clone(), name: "boxposn".into(), source_id: leader.0, source_track: "Height".into() }).is_err(), "a vector reads a vector track");
    assert!(execute(&mut s, Command::ExpressionAssignController { id: b.0, track: tr.clone(), name: "boxposn".into(), source_id: b.0, source_track: "Position".into() }).is_err(), "not its own track");
    execute(&mut s, Command::ExpressionAssignController { id: b.0, track: tr.clone(), name: "boxposn".into(), source_id: leader.0, source_track: "Position".into() }).unwrap();
    execute(&mut s, Command::ExpressionSet { id: b.0, track: tr.clone(), expression: Some("[0, 0, 30] + boxposn".into()), description: Some("rides on the leader".into()) }).unwrap();
    assert!(pos_at(&mut s, b, 20).distance(Vec3::new(100.0, 0.0, 30.0)) < 1e-3);
    assert!(pos_at(&mut s, b, 0).distance(Vec3::new(0.0, 0.0, 30.0)) < 1e-3);
    // The leader's animation changes: the follower reads the new motion.
    execute(&mut s, Command::SetFrame { frame: 20 }).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![leader.0], op: loom_commands::SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 50.0, 0.0], system: None }).unwrap();
    assert!(pos_at(&mut s, b, 20).distance(Vec3::new(100.0, 50.0, 30.0)) < 1e-3, "{:?}", pos_at(&mut s, b, 20));
    // Tick Offset: 4800 ticks = one second = 30 frames later.
    execute(&mut s, Command::ExpressionSetTickOffset { id: b.0, track: tr.clone(), name: "boxposn".into(), ticks: -1600 }).unwrap();
    assert!(pos_at(&mut s, b, 30).distance(Vec3::new(100.0, 50.0, 30.0)) < 1e-3, "frame 30 reads the leader at frame 20");
    // No cycle: the leader cannot read the follower now.
    assign(&mut s, leader, "Position", "Position Expression");
    execute(&mut s, Command::ExpressionCreateVariable { id: leader.0, track: tr.clone(), name: "back".into(), vector: true }).unwrap();
    assert!(execute(&mut s, Command::ExpressionAssignController { id: leader.0, track: tr.clone(), name: "back".into(), source_id: b.0, source_track: "Position".into() }).is_err());
    // Saved and opened, the follower still reads its source.
    let path = tmp("follow.loom");
    execute(&mut s, Command::SaveFileAs { path: Some(path.to_string_lossy().into()) }).unwrap();
    let mut t = AppState::new();
    execute(&mut t, Command::OpenFile { path: Some(path.to_string_lossy().into()) }).unwrap();
    let e: loom_scene::expr::ExprCtrl = loom_commands::ctl_props::read_ctl(&t, b, "Position", "Expression").unwrap().1;
    assert_eq!(e.description, "rides on the leader");
    assert_eq!(e.vars[0].source.as_ref().unwrap().label, format!("{} \\ Position", t.scene.node(leader).unwrap().name));
}

#[test]
fn float_expression_drives_an_object_parameter() {
    let mut s = AppState::new();
    let b = boxed(&mut s, [0.0; 3]);
    assign(&mut s, b, "Height", "Float Expression");
    execute(&mut s, Command::ExpressionSet { id: b.0, track: "Height".into(), expression: Some("10 + F".into()), description: None }).unwrap();
    execute(&mut s, Command::SetFrame { frame: 15 }).unwrap();
    let h = loom_commands::controllers::find_track(&s, b, "Height").unwrap();
    assert_eq!(loom_scene::tracks::read(&s.scene, &h).unwrap().comps_at(15.0), vec![25.0]);
    assert!(execute(&mut s, Command::ExpressionSet { id: b.0, track: "Height".into(), expression: Some("[1,2,3]".into()), description: None }).is_err(), "a float needs a scalar");
    // The evaluated mesh follows: 25 high at frame 15.
    let m = s.scene.eval_mesh(b).value.mesh.clone();
    let top = (0..m.vertex_count() as u32).filter_map(|v| m.vertex_position(loom_mesh::VertexId(v))).fold(f32::MIN, |a, p| a.max(p.z));
    assert!((top - 25.0).abs() < 1e-3, "{top}");
}

#[test]
fn spring_overshoots_after_the_original_motion_stops() {
    let mut s = AppState::new();
    let b = boxed(&mut s, [0.0; 3]);
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::SetFrame { frame: 10 }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [100.0, 0.0, 0.0], system: None }).unwrap();
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    assign(&mut s, b, "Position", "Spring");
    assert_eq!(s.dialog, Some(loom_commands::Dialog::SpringController));
    let sp: loom_scene::param::SpringCtrl<Vec3> = loom_commands::ctl_props::read_ctl(&s, b, "Position", "Spring").unwrap().1;
    assert_eq!((sp.drag, sp.start, sp.iterations, sp.effect), (1.0, 0.0, 2, [100.0; 3]), "reference defaults");
    // The spring lags behind and then swings past the stop at 100.
    let xs: Vec<f32> = (0..=60).map(|f| pos_at(&mut s, b, f).x).collect();
    assert!(xs[5] < 50.0, "lags: {}", xs[5]);
    assert!(xs.iter().cloned().fold(f32::MIN, f32::max) > 101.0, "overshoots: {xs:?}");
    assert!((xs[0] - 0.0).abs() < 1e-3, "starts at the original motion");
    // Higher drag subdues the bounce.
    let peak = |s: &mut AppState| (0..=60).map(|f| pos_at(s, b, f).x).fold(f32::MIN, f32::max);
    let p0 = peak(&mut s);
    execute(&mut s, Command::SpringSetParameter { id: b.0, track: "Position".into(), name: "Drag".into(), value: 10.0, index: None, relative: None }).unwrap();
    assert!(peak(&mut s) < p0);
    // X Effect 0: X follows the original motion exactly.
    execute(&mut s, Command::SpringSetParameter { id: b.0, track: "Position".into(), name: "X Effect".into(), value: 0.0, index: None, relative: None }).unwrap();
    assert!((pos_at(&mut s, b, 5).x - 50.0).abs() < 1e-2, "{}", pos_at(&mut s, b, 5).x);
    // Relative Tension adds.
    execute(&mut s, Command::SpringSetParameter { id: b.0, track: "Position".into(), name: "Tension".into(), value: 1.0, index: Some(0), relative: Some(true) }).unwrap();
    let sp: loom_scene::param::SpringCtrl<Vec3> = loom_commands::ctl_props::read_ctl(&s, b, "Position", "Spring").unwrap().1;
    assert_eq!(sp.tension, 3.0);
    // Moving the object moves its original motion.
    execute(&mut s, Command::SetFrame { frame: 0 }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 20.0], system: None }).unwrap();
    assert!((pos_at(&mut s, b, 0).z - 20.0).abs() < 1e-3);
    // Another object as a spring; Self Influence cannot be removed.
    let other = boxed(&mut s, [0.0, 50.0, 0.0]);
    execute(&mut s, Command::SpringAddSpring { id: b.0, track: "Position".into(), source_id: other.0 }).unwrap();
    assert!(execute(&mut s, Command::SpringRemoveSpring { id: b.0, track: "Position".into(), index: 0 }).is_err());
    execute(&mut s, Command::SpringRemoveSpring { id: b.0, track: "Position".into(), index: 1 }).unwrap();
    undo(&mut s);
    let sp: loom_scene::param::SpringCtrl<Vec3> = loom_commands::ctl_props::read_ctl(&s, b, "Position", "Spring").unwrap().1;
    assert_eq!(sp.springs.len(), 1);
    assert!(sp.springs[0].source.copy.is_some(), "the spring object's motion is copied in");
}

#[test]
fn smooth_rotation_keys_are_auto_and_have_no_key_properties() {
    let mut s = AppState::new();
    let b = boxed(&mut s, [0.0; 3]);
    assign(&mut s, b, "Rotation", "Smooth Rotation");
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::SetFrame { frame: 10 }).unwrap();
    execute(&mut s, Command::RotateSelection { axis: loom_commands::Axis::Z, degrees: 45.0, system: None }).unwrap();
    execute(&mut s, Command::SetFrame { frame: 20 }).unwrap();
    execute(&mut s, Command::RotateSelection { axis: loom_commands::Axis::Z, degrees: 45.0, system: None }).unwrap();
    let n = s.scene.node(b).unwrap();
    let k = n.transform.rotation.keys().unwrap();
    assert!(k.smooth && k.linked);
    assert!(k.tracks.iter().flat_map(|t| &t.keys).all(|key| key.in_type == loom_scene::anim::TangentType::Auto && key.out_type == loom_scene::anim::TangentType::Auto));
    assert_eq!(loom_scene::tracks::AnyParam::Vec3(n.transform.rotation.clone()).controller_name(loom_scene::tracks::Role::Rotation), "Smooth Rotation");
    // Curved: halfway between keys 0 and 90 at frame 5 is not the linear 45.
    let z5 = s.scene.node(b).unwrap().transform.rotation.get_at(5.0).z;
    assert!(z5 > 0.0 && (z5 - 45.0).abs() > 0.5, "{z5}");
    execute(&mut s, Command::OpenCurveEditor).unwrap();
    execute(&mut s, Command::TrackViewHighlight { items: vec![format!("{}:Rotation", b.0)], op: loom_commands::SelectOp::Replace }).unwrap();
    assert!(execute(&mut s, Command::PropertiesTrackView).is_err());
}

/// A 16-bit mono WAV at 30 samples a second.
fn wav(samples: &[i16]) -> Vec<u8> {
    let data: Vec<u8> = samples.iter().flat_map(|x| x.to_le_bytes()).collect();
    let mut f = b"RIFF".to_vec();
    f.extend((36 + data.len() as u32).to_le_bytes());
    f.extend(b"WAVEfmt ");
    for x in [16u32.to_le_bytes().to_vec(), 1u16.to_le_bytes().to_vec(), 1u16.to_le_bytes().to_vec(), 30u32.to_le_bytes().to_vec(), 60u32.to_le_bytes().to_vec(), 2u16.to_le_bytes().to_vec(), 16u16.to_le_bytes().to_vec()] {
        f.extend(x);
    }
    f.extend(b"data");
    f.extend((data.len() as u32).to_le_bytes());
    f.extend(data);
    f
}

#[test]
fn audio_scale_follows_the_sound_amplitude() {
    let mut s = AppState::new();
    let b = boxed(&mut s, [0.0; 3]);
    let path = tmp("beat.wav");
    std::fs::write(&path, wav(&[0, 8192, 16384, 32767, 0])).unwrap();
    assign(&mut s, b, "Scale", "Audio Scale");
    assert_eq!(s.dialog, Some(loom_commands::Dialog::AudioController));
    let tr = "Scale".to_owned();
    assert!(execute(&mut s, Command::AudioChooseSound { id: b.0, track: tr.clone(), path: Some(tmp("missing.wav").to_string_lossy().into()) }).is_err());
    execute(&mut s, Command::AudioChooseSound { id: b.0, track: tr.clone(), path: Some(path.to_string_lossy().into()) }).unwrap();
    // The example: Base Z 0, Target Z 600%.
    execute(&mut s, Command::AudioSetParameter { id: b.0, track: tr.clone(), name: "Base".into(), value: 0.0, index: Some(2) }).unwrap();
    execute(&mut s, Command::AudioSetParameter { id: b.0, track: tr.clone(), name: "Target".into(), value: 6.0, index: Some(2) }).unwrap();
    let z = |s: &mut AppState, f: i32| {
        execute(s, Command::SetFrame { frame: f }).unwrap();
        s.scene.node(b).unwrap().transform.scale.get().z
    };
    assert_eq!(z(&mut s, 0), 0.0);
    assert!((z(&mut s, 2) - 3.0).abs() < 1e-3, "half amplitude: halfway");
    assert!((z(&mut s, 3) - 6.0 * 32767.0 / 32768.0).abs() < 1e-3);
    // Absolute Value: the loudest sample is the maximum.
    execute(&mut s, Command::AudioSetParameter { id: b.0, track: tr.clone(), name: "Absolute Value".into(), value: 1.0, index: None }).unwrap();
    assert!((z(&mut s, 3) - 6.0).abs() < 1e-3);
    execute(&mut s, Command::AudioSetParameter { id: b.0, track: tr.clone(), name: "Threshold".into(), value: 0.6, index: None }).unwrap();
    assert_eq!(z(&mut s, 2), 0.0, "below Threshold");
    assert_eq!(s.scene.node(b).unwrap().transform.scale.get_at(2.0).x, 1.0, "X keeps base = target = 100%");
    execute(&mut s, Command::AudioChooseSound { id: b.0, track: tr.clone(), path: None }).unwrap();
    assert_eq!(z(&mut s, 3), 0.0, "no sound: the base");
}

#[test]
fn noise_positive_constraint_keeps_values_between_zero_and_strength() {
    let mut s = AppState::new();
    let b = boxed(&mut s, [0.0; 3]);
    assign(&mut s, b, "Position", "Noise Position");
    assert_eq!(s.dialog, Some(loom_commands::Dialog::NoiseController));
    let tr = "Position".to_owned();
    for i in 0..3 {
        execute(&mut s, Command::NoiseSetParameter { id: b.0, track: tr.clone(), name: ">0".into(), value: 1.0, index: Some(i) }).unwrap();
    }
    execute(&mut s, Command::NoiseSetParameter { id: b.0, track: tr.clone(), name: "Frequency".into(), value: 1.0, index: None }).unwrap();
    let p = s.scene.node(b).unwrap().transform.position.clone();
    for f in 0..100 {
        let v = p.get_at(f as f32);
        assert!(v.min_element() >= 0.0 && v.max_element() <= 50.0, "{v:?}");
    }
    assert!(execute(&mut s, Command::NoiseSetParameter { id: b.0, track: tr, name: "Bogus".into(), value: 1.0, index: None }).is_err());
}

#[test]
fn ring_array_places_boxes_on_an_animatable_ring() {
    let mut s = AppState::new();
    let r = execute(&mut s, Command::CreateRingArray { position: [10.0, 0.0, 0.0], radius: 40.0 }).unwrap();
    let dummy = NodeId(r["dummy"].as_u64().unwrap());
    let boxes: Vec<NodeId> = r["boxes"].as_array().unwrap().iter().map(|b| NodeId(b.as_u64().unwrap())).collect();
    assert_eq!(boxes.len(), 4, "four boxes by default");
    assert!(boxes.iter().all(|b| s.scene.node(*b).unwrap().parent == Some(dummy)));
    // Evenly spaced on a flat circle around the dummy.
    assert!(pos_at(&mut s, boxes[0], 0).distance(Vec3::new(50.0, 0.0, 0.0)) < 1e-3);
    assert!(pos_at(&mut s, boxes[1], 0).distance(Vec3::new(10.0, 40.0, 0.0)) < 1e-3);
    // Amplitude and Cycles 1: a tilted ring; Phase shifts the wave.
    execute(&mut s, Command::SetRingArray { id: boxes[2].0, name: "Amplitude".into(), value: 10.0 }).unwrap();
    execute(&mut s, Command::SetRingArray { id: boxes[2].0, name: "Cycles".into(), value: 1.0 }).unwrap();
    assert!((pos_at(&mut s, boxes[1], 0).z - 10.0).abs() < 1e-3);
    assert!((pos_at(&mut s, boxes[3], 0).z + 10.0).abs() < 1e-3);
    execute(&mut s, Command::SetRingArray { id: dummy.0, name: "Phase".into(), value: 0.25 }).unwrap();
    assert!((pos_at(&mut s, boxes[0], 0).z - 10.0).abs() < 1e-3, "a quarter wave on");
    // The dummy carries the ring.
    execute(&mut s, Command::SelectNodes { ids: vec![dummy.0], op: loom_commands::SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 5.0], system: None }).unwrap();
    assert!((pos_at(&mut s, boxes[0], 0).z - 15.0).abs() < 1e-3);
    // Auto Key animates the radius.
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::SetFrame { frame: 50 }).unwrap();
    execute(&mut s, Command::SetRingArray { id: boxes[0].0, name: "Radius".into(), value: 80.0 }).unwrap();
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    assert!((pos_at(&mut s, boxes[0], 50).x - 90.0).abs() < 1e-3);
    assert!((pos_at(&mut s, boxes[0], 0).x - 50.0).abs() < 1e-3);
    // Number: six boxes, then back to four (one undo step each).
    execute(&mut s, Command::SetRingArray { id: dummy.0, name: "Number".into(), value: 6.0 }).unwrap();
    assert_eq!(loom_commands::ring::members(&s, dummy).unwrap().1.len(), 6);
    let (_, six) = loom_commands::ring::members(&s, dummy).unwrap();
    assert!(pos_at(&mut s, six[3], 0).distance(Vec3::new(10.0 - 40.0, 0.0, 5.0 + 10.0 * (std::f32::consts::PI + std::f32::consts::FRAC_PI_2).sin())) < 1e-2, "{:?}", pos_at(&mut s, six[3], 0));
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(loom_commands::ring::members(&s, dummy).unwrap().1.len(), 4);
    assert!(execute(&mut s, Command::SetRingArray { id: dummy.0, name: "Bogus".into(), value: 1.0 }).is_err());
}

#[test]
fn noise_scale_varies_around_the_assigned_scale_and_adds_as_a_list_entry() {
    let mut s = AppState::new();
    let b = boxed(&mut s, [0.0; 3]);
    assign(&mut s, b, "Scale", "Noise Scale");
    // Strength 50 %: the scale stays within 75..125 % instead of the bare
    // noise (which shrank the object to under a quarter of its size).
    let sc = s.scene.node(b).unwrap().transform.scale.clone();
    let mut moved = false;
    for f in 0..100 {
        let v = sc.get_at(f as f32);
        assert!(v.min_element() >= 0.75 - 1e-4 && v.max_element() <= 1.25 + 1e-4, "frame {f}: {v:?}");
        moved |= (v - Vec3::ONE).abs().max_element() > 0.01;
    }
    assert!(moved, "the noise animates");
    // In a Scale List the noise entry starts from zero: ScaleXYZ 100 % plus
    // the noise offset, not 200 %.
    let c = boxed(&mut s, [30.0, 0.0, 0.0]);
    assign(&mut s, c, "Scale", "Scale List");
    execute(&mut s, Command::ListAddController { id: c.0, track: "Scale".into(), controller: "Noise Scale".into() }).unwrap();
    let sc = s.scene.node(c).unwrap().transform.scale.clone();
    for f in 0..100 {
        let v = sc.get_at(f as f32);
        assert!(v.min_element() >= 0.75 - 1e-4 && v.max_element() <= 1.25 + 1e-4, "list, frame {f}: {v:?}");
    }
}
