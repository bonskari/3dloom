use glam::{Quat, Vec3};
use loom_commands::{execute, AppState, Command};
use loom_scene::{NodeId, ObjectKind};

// Minimal format conformance data, not a generated motion asset.
const BVH: &str = "HIERARCHY ROOT Root { OFFSET 0 0 0 CHANNELS 6 Xposition Yposition Zposition Zrotation Yrotation Xrotation JOINT Hips { OFFSET 0 100 0 CHANNELS 6 Xposition Yposition Zposition Zrotation Xrotation Yrotation End Site { OFFSET 0 20 0 } } } MOTION Frames: 2 Frame Time: 0.05 10 20 30 0 0 0 1 2 3 30 40 50 20 30 40 0 0 0 2 3 4 40 50 60";
fn source(name: &str, text: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("loom-bvh-tests").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("motion.BVH");
    std::fs::write(&path, text).unwrap();
    path
}
fn import(s: &mut AppState, path: &std::path::Path) -> Result<serde_json::Value, String> {
    execute(s, Command::ImportFile { path: Some(path.to_string_lossy().into_owned()) })
}
#[test]
fn hierarchy_units_channel_order_timeline_undo_and_save() {
    let path = source("hierarchy", BVH);
    let mut s = AppState::new();
    let scale = (0.01 / (s.units.system_unit.metres() * s.units.system_scale as f64)) as f32;
    let before = s.undo.len();
    let result = import(&mut s, &path).unwrap();
    let ids: Vec<u64> = serde_json::from_value(result["nodes"].clone()).unwrap();
    assert_eq!(ids.len(), 2);
    assert_eq!(s.undo.len(), before + 1);
    assert_eq!(s.fps, 20.0);
    assert_eq!(s.frame_range, (0, 1));
    let root = s.scene.node(NodeId(ids[0])).unwrap();
    let hips = s.scene.node(NodeId(ids[1])).unwrap();
    assert_eq!(hips.parent, Some(root.id));
    assert!(matches!(hips.object, ObjectKind::Bone { .. }));
    if let ObjectKind::Bone { length, axis, .. } = &hips.object {
        assert!((length.get() - 20.0 * scale).abs() < 1e-4);
        assert_eq!(axis.get(), 2, "Y-up End Site points along scene +Z");
    }
    assert_eq!(hips.transform.rotation.keys().unwrap().tracks[0].interp, loom_scene::anim::Interp::Linear);
    assert!((hips.transform.position.get() - Vec3::new(1.0, -3.0, 2.0) * scale).length() < 1e-4);
    assert!((root.transform.position.get() - Vec3::new(10.0, -30.0, 20.0) * scale).length() < 1e-4);
    let c = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    let expected = c * Quat::from_rotation_z(30f32.to_radians()) * Quat::from_rotation_x(40f32.to_radians()) * Quat::from_rotation_y(50f32.to_radians()) * c.inverse();
    assert!(hips.transform.rotation_quat().dot(expected).abs() > 0.99999);
    assert!((hips.transform.position.keys().unwrap().rest[2] - 100.0 * scale).abs() < 1e-4);
    let saved = path.with_extension("loom");
    execute(&mut s, Command::SaveFileAs { path: Some(saved.to_string_lossy().into_owned()) }).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.nodes().count(), 0);
    execute(&mut s, Command::OpenFile { path: Some(saved.to_string_lossy().into_owned()) }).unwrap();
    assert_eq!(s.scene.nodes().count(), 2);
    assert_eq!(s.fps, 20.0);
    execute(&mut s, Command::SetFrame { frame: 1 }).unwrap();
    let hips = s.scene.node(NodeId(ids[1])).unwrap();
    assert!((hips.transform.position.get() - Vec3::new(2.0, -4.0, 3.0) * scale).length() < 1e-4);
}
#[test]
fn malformed_motion_is_atomic() {
    for (i, text) in [BVH.replacen("Zrotation", "Wrotation", 1), BVH[..BVH.len()-3].to_owned(), BVH.replace("0.05", "0"), BVH.replace("OFFSET 0 100 0", "OFFSET 0 NaN 0"), BVH.replace("Frames: 2", "Frames: 3")].iter().enumerate() {
        let path = source(&format!("invalid{i}"), text);
        let mut s = AppState::new();
        assert!(import(&mut s, &path).is_err());
        assert_eq!(s.scene.nodes().count(), 0);
        assert_eq!(s.undo.len(), 0);
    }
}
#[test]
fn rotations_do_not_jump_at_wrap() {
    let text = "HIERARCHY ROOT Root { OFFSET 0 0 0 CHANNELS 3 Zrotation Yrotation Xrotation End Site { OFFSET 0 10 0 } } MOTION Frames: 2 Frame Time: 0.03333333333333333 179 0 0 -179 0 0";
    let mut s = AppState::new();
    import(&mut s, &source("wrap", text)).unwrap();
    let r = s.scene.nodes().next().unwrap().transform.rotation.keys().unwrap();
    for track in &r.tracks {
        assert!((track.keys[1].value - track.keys[0].value).abs() < 3.0);
    }
}

/// Run against a real generated Kimodo motion without putting private assets
/// into the repository: LOOM_KIMODO_BVH=/path/to/motion.bvh cargo test ...
#[test]
#[ignore = "requires a real local Kimodo BVH supplied via LOOM_KIMODO_BVH"]
fn external_kimodo_motion() {
    let path = std::env::var("LOOM_KIMODO_BVH").expect("set LOOM_KIMODO_BVH to a real Kimodo motion");
    let mut s = AppState::new();
    let scale = (0.01 / (s.units.system_unit.metres() * s.units.system_scale as f64)) as f32;
    import(&mut s, std::path::Path::new(&path)).unwrap();
    assert!(s.scene.nodes().count() > 50);
    let hips = s.scene.nodes().find(|n| n.name == "Hips").unwrap();
    // Source channel values independently read from this file. Position
    // channels replace the neutral OFFSET, including the animated Hips.
    let text = std::fs::read_to_string(path).unwrap();
    let line = text.lines().skip_while(|l| !l.starts_with("Frame Time:")).nth(1).unwrap();
    let values: Vec<f32> = line.split_whitespace().map(|v| v.parse().unwrap()).collect();
    let expected = Vec3::new(values[6], -values[8], values[7]) * scale;
    assert!((hips.transform.position.get() - expected).length() < 1e-4);
    let rest_z = hips.transform.position.keys().unwrap().rest[2];
    assert!((rest_z - 100.0 * scale).abs() < 1e-4);
    assert!((s.fps - 30.0).abs() < 1e-4);
    assert_eq!(s.frame_range, (0, 149));
}

#[test]
fn scene_system_scale_applies_once_and_names_stay_unique() {
    let path = source("units", BVH);
    let mut s = AppState::new();
    s.units.system_scale = 2.0;
    let scale = (0.01 / (s.units.system_unit.metres() * 2.0)) as f32;
    import(&mut s, &path).unwrap();
    import(&mut s, &path).unwrap();
    assert_eq!(s.scene.nodes().count(), 4);
    let names: std::collections::BTreeSet<_> = s.scene.nodes().map(|n| &n.name).collect();
    assert_eq!(names.len(), 4);
    for root in s.scene.nodes().filter(|n| n.parent.is_none()) {
        assert!((root.transform.position.get() - Vec3::new(10.0, -30.0, 20.0) * scale).length() < 1e-4);
    }
}

#[test]
fn undo_redo_restores_import_timing_with_original_nondefault_timeline() {
    let (header, motion) = BVH.split_once("0.05 ").unwrap();
    let first_frame = motion.split_whitespace().take(12).collect::<Vec<_>>().join(" ") + " ";
    let text = format!("{}0.03333333333333333 {}", header.replace("Frames: 2", "Frames: 150"), first_frame.repeat(150));
    let path = source("timing-undo", &text);
    let mut s = AppState::new();
    s.fps = 24.0;
    s.frame_range = (100, 900);
    execute(&mut s, Command::SetFrame { frame: 200 }).unwrap();
    import(&mut s, &path).unwrap();
    assert!((s.fps - 30.0).abs() < 1e-4);
    assert_eq!((s.frame_range, s.current_frame), ((0, 149), 0));
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!((s.fps, s.frame_range, s.current_frame), (24.0, (100, 900), 200));
    assert_eq!(s.scene.time(), 200.0);
    execute(&mut s, Command::RedoSceneOperation).unwrap();
    assert!((s.fps - 30.0).abs() < 1e-4);
    assert_eq!((s.frame_range, s.current_frame), ((0, 149), 0));
    assert_eq!(s.scene.time(), 0.0);
}

#[test]
fn branched_and_evicted_import_timing_never_attaches_to_another_step() {
    let path = source("timing-branch", BVH);
    let mut s = AppState::new();
    s.fps = 24.0;
    s.frame_range = (100, 900);
    s.current_frame = 200;
    import(&mut s, &path).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    // Direct stack commits model interactive edits that do not go through execute.
    let n = loom_scene::Node::new("Branch", ObjectKind::dummy(1.0));
    s.undo.commit("Import BVH", loom_scene::Change::AddNode(n), &mut s.scene).unwrap();
    s.fps = 48.0;
    s.frame_range = (50, 80);
    s.current_frame = 60;
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::RedoSceneOperation).unwrap();
    assert_eq!((s.fps, s.frame_range, s.current_frame), (48.0, (50, 80), 60));
    import(&mut s, &path).unwrap();
    // Lowering the limit evicts earlier entries and must retain this import's identity.
    s.undo.set_limit(1);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!((s.fps, s.frame_range, s.current_frame), (48.0, (50, 80), 60));
    execute(&mut s, Command::RedoSceneOperation).unwrap();
    assert_eq!((s.fps, s.frame_range, s.current_frame), (20.0, (0, 1), 0));
}
