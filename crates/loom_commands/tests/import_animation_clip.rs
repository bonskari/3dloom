use loom_commands::{execute, AppState, Command};
use loom_commands::exchange::FileContent;
use loom_scene::{NodeId, ObjectKind};
const MOTION: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../loom_fbx/tests/fixtures/sitting_idle.fbx");
const STATIC: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../loom_fbx/tests/fixtures/character_for_mixamo.fbx");

#[test]
fn dedicated_fbx_import_keeps_target_and_options_and_source_clip_is_reversible() {
    let mut s = AppState::new();
    execute(&mut s, Command::ImportFile { path: Some(STATIC.into()) }).unwrap();
    let targets: Vec<_> = s.scene.nodes().map(|n| (n.id, serde_json::to_value(n).unwrap())).collect();
    s.exchange.fbx_import.file_content = FileContent::UpdateAnimation;
    s.exchange.fbx_import.animation = false;
    s.exchange.fbx_import.fill_timeline = false;
    s.exchange.fbx_import.bones_as_dummies = true;
    s.exchange.fbx_import.deformations = false;
    s.exchange.fbx_import.skins = false;
    s.exchange.fbx_import.take = Some("a previous file take".into());
    let options = s.exchange.fbx_import.clone();
    s.fps = 24.0; s.frame_range = (100, 900); s.current_frame = 200;
    let timing = (s.fps, s.frame_range, s.current_frame);
    let steps = s.undo.len();
    let r = execute(&mut s, Command::ImportAnimationClip { path: Some(MOTION.into()) }).unwrap();
    assert_eq!(s.exchange.fbx_import, options);
    assert_eq!(s.fps, 24.0);
    assert_eq!(s.current_frame, 200);
    assert_eq!(s.undo.len(), steps + 1);
    assert_eq!(s.scene.animation_clips.len(), 1);
    let clip = s.scene.animation_clips[0].clone();
    assert_eq!(r["clip"].as_u64(), Some(clip.id));
    assert!(r["take"].as_str().is_some_and(|take| !take.is_empty()));
    assert!(!clip.nodes.is_empty());
    assert!(clip.end > clip.start);
    assert!(clip.nodes.iter().any(|id| matches!(s.scene.node(*id).unwrap().object, ObjectKind::Bone { .. })));
    for (id, node) in &targets { assert_eq!(serde_json::to_value(s.scene.node(*id).unwrap()).unwrap(), *node); }
    let imported_timing = (s.fps, s.frame_range, s.current_frame);
    assert_ne!(imported_timing, timing);
    let samples: Vec<_> = [clip.start, (clip.start + clip.end) * 0.5, clip.end].iter().map(|f| {
        s.scene.at_time(*f, || clip.nodes.iter().map(|id| s.scene.world_matrix(*id)).collect::<Vec<_>>())
    }).collect();
    execute(&mut s, Command::MoveAnimationClip { id: clip.id, start: clip.start + 10.25 }).unwrap();
    for (i, f) in [clip.start, (clip.start + clip.end) * 0.5, clip.end].iter().enumerate() {
        for (j, id) in clip.nodes.iter().enumerate() {
            assert!(samples[i][j].abs_diff_eq(s.scene.at_time(*f + 10.25, || s.scene.world_matrix(*id)), 1e-3), "node {} shifted pose", id.0);
        }
    }
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!((s.fps, s.frame_range, s.current_frame), timing);
    assert!(s.scene.animation_clips.is_empty());
    assert_eq!(s.scene.nodes().count(), targets.len());
    for (id, node) in &targets { assert_eq!(serde_json::to_value(s.scene.node(*id).unwrap()).unwrap(), *node); }
    execute(&mut s, Command::RedoSceneOperation).unwrap();
    assert_eq!(s.scene.animation_clips[0], clip);
    assert_eq!((s.fps, s.frame_range, s.current_frame), imported_timing);
}

#[test]
fn dedicated_animation_import_rejects_static_corrupt_and_other_formats_atomically() {
    let mut s = AppState::new();
    s.exchange.fbx_import.animation = false;
    s.exchange.fbx_import.file_content = FileContent::UpdateAnimation;
    let options = s.exchange.fbx_import.clone();
    let corrupt = std::env::temp_dir().join("loom-corrupt-animation-clip.fbx");
    std::fs::write(&corrupt, b"not an FBX").unwrap();
    for path in [STATIC.to_owned(), corrupt.to_string_lossy().into_owned(), "unsupported.lxaf".into()] {
        let before = serde_json::to_value(&s.scene).unwrap();
        let timing = (s.fps, s.frame_range, s.current_frame);
        let undo = s.undo.len();
        assert!(execute(&mut s, Command::ImportAnimationClip { path: Some(path) }).is_err());
        assert_eq!(serde_json::to_value(&s.scene).unwrap(), before);
        assert_eq!(s.exchange.fbx_import, options);
        assert_eq!((s.fps, s.frame_range, s.current_frame), timing);
        assert_eq!(s.undo.len(), undo);
    }
    std::fs::remove_file(corrupt).unwrap();
}

#[test]
fn legacy_fbx_update_does_not_create_or_reown_source_clips() {
    let mut s = AppState::new();
    execute(&mut s, Command::ImportFile { path: Some(MOTION.into()) }).unwrap();
    let clip = s.scene.animation_clips[0].clone();
    let count = s.scene.nodes().count();
    s.exchange.fbx_import.file_content = FileContent::UpdateAnimation;
    execute(&mut s, Command::ImportFile { path: Some(MOTION.into()) }).unwrap();
    assert_eq!(s.scene.animation_clips, vec![clip]);
    assert_eq!(s.scene.nodes().count(), count);
}

#[test]
fn dedicated_animation_picker_and_bvh_import_route() {
    let mut s = AppState::new();
    s.fps = 24.0;
    execute(&mut s, Command::ImportAnimationClip { path: None }).unwrap();
    assert_eq!(s.dialog, Some(loom_commands::Dialog::ImportAnimationClip));
    let path = std::env::temp_dir().join("loom-dedicated-animation.bvh");
    std::fs::write(&path, "HIERARCHY ROOT Root { OFFSET 0 0 0 CHANNELS 3 Zrotation Yrotation Xrotation End Site { OFFSET 0 10 0 } } MOTION Frames: 2 Frame Time: 0.05 0 0 0 20 0 0").unwrap();
    execute(&mut s, Command::ImportAnimationClip { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    assert_eq!(s.scene.animation_clips.len(), 1);
    assert_eq!(s.fps, 24.0, "an empty project's chosen frame rate is retained");
    assert!((s.scene.animation_clips[0].end - 1.2).abs() < 1e-5);
    assert!(s.scene.node(NodeId(s.scene.animation_clips[0].nodes[0].0)).is_some());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn imported_bvh_and_fbx_keep_project_fps_and_preserve_motion_in_seconds() {
    use loom_scene::anim::TangentType;
    let bvh = std::env::temp_dir().join("loom-import-30fps-animation.bvh");
    std::fs::write(&bvh, "HIERARCHY ROOT Root { OFFSET 0 0 0 CHANNELS 6 Xposition Yposition Zposition Zrotation Yrotation Xrotation End Site { OFFSET 0 10 0 } } MOTION Frames: 3 Frame Time: 0.03333333333333333 0 0 0 0 0 0 10 20 30 10 0 0 20 40 60 20 0 0").unwrap();
    for path in [bvh.to_string_lossy().into_owned(), MOTION.into()] {
        let mut reference = AppState::new();
        execute(&mut reference, Command::ImportFile { path: Some(path.clone()) }).unwrap();
        let reference_clip = reference.scene.animation_clips[0].clone();
        assert!((reference.fps - 30.0).abs() < 1e-4);
        let mut s = AppState::new();
        execute(&mut s, Command::ImportFile { path: Some(STATIC.into()) }).unwrap();
        let target = s.scene.nodes().next().unwrap().id;
        s.scene.node_mut(target).unwrap().transform.position.ensure_keys().unwrap().tracks[0].set_key(123.5, 42.0, (TangentType::Smooth, TangentType::Smooth));
        let targets: Vec<_> = s.scene.nodes().map(|n| (n.id, serde_json::to_value(n).unwrap())).collect();
        s.fps = 24.0; s.frame_range = (100, 900); s.current_frame = 200;
        execute(&mut s, Command::ImportAnimationClip { path: Some(path) }).unwrap();
        assert_eq!((s.fps, s.current_frame), (24.0, 200));
        let clip = s.scene.animation_clips[0].clone();
        let ratio = 24.0 / reference.fps;
        assert!((clip.end - reference_clip.end * ratio).abs() < 1e-4);
        assert_eq!(clip.nodes.len(), reference_clip.nodes.len());
        for f in [reference_clip.start, (reference_clip.start + reference_clip.end) * 0.5, reference_clip.end] {
            for (source, imported) in reference_clip.nodes.iter().zip(&clip.nodes) {
                let expected = reference.scene.at_time(f, || reference.scene.world_matrix(*source));
                let actual = s.scene.at_time(f * ratio, || s.scene.world_matrix(*imported));
                assert!(expected.abs_diff_eq(actual, 1e-3), "same world pose at source frame {f} / {}fps and targetframe {} / 24fps", reference.fps, f * ratio);
            }
        }
        for (id, node) in &targets { assert_eq!(serde_json::to_value(s.scene.node(*id).unwrap()).unwrap(), *node); }
        execute(&mut s, Command::UndoSceneOperation).unwrap();
        assert_eq!((s.fps, s.frame_range, s.current_frame), (24.0, (100, 900), 200));
        assert!(s.scene.animation_clips.is_empty());
        execute(&mut s, Command::RedoSceneOperation).unwrap();
        assert_eq!(s.scene.animation_clips, vec![clip]);
        assert_eq!((s.fps, s.current_frame), (24.0, 200));
    }
    std::fs::remove_file(bvh).unwrap();
}
