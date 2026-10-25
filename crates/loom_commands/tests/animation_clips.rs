use loom_commands::{execute, AppState, Command};
use loom_scene::{AnimationClip, NodeId};

fn import(s: &mut AppState, name: &str) {
    let path = std::env::temp_dir().join(format!("loom-animation-clip-{name}.bvh"));
    std::fs::write(&path, "HIERARCHY ROOT Root { OFFSET 0 0 0 CHANNELS 6 Xposition Yposition Zposition Zrotation Yrotation Xrotation JOINT Hips { OFFSET 0 100 0 CHANNELS 3 Zrotation Yrotation Xrotation End Site { OFFSET 0 20 0 } } } MOTION Frames: 2 Frame Time: 0.05 0 0 0 0 0 0 10 0 0 1 2 3 20 0 0 20 0 0").unwrap();
    execute(s, Command::ImportFile { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn bvh_clip_moves_fractional_native_keys_atomically_and_saves() {
    let mut s = AppState::new();
    let undo = s.undo.len();
    import(&mut s, "move");
    assert_eq!(s.undo.len(), undo + 1);
    let clip = s.scene.animation_clips[0].clone();
    assert_eq!((clip.start, clip.end), (0.0, 1.0));
    assert_eq!(clip.nodes.len(), 2);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(s.scene.animation_clips.is_empty());
    assert_eq!(s.scene.nodes().count(), 0);
    execute(&mut s, Command::RedoSceneOperation).unwrap();
    let root = clip.nodes[0];
    // Direct key editing stays available, including fractional times.
    let p = &mut s.scene.node_mut(root).unwrap().transform.position;
    p.keys_mut().unwrap().tracks[0].keys[0].time = 0.25;
    p.keys_mut().unwrap().tracks[0].keys[1].time = 0.75;
    let original = s.scene.node(root).unwrap().transform.clone();
    let original_world = s.scene.at_time(0.5, || s.scene.world_matrix(root));
    import(&mut s, "other");
    let other = s.scene.animation_clips[1].clone();
    let other_node = serde_json::to_value(s.scene.node(other.nodes[0]).unwrap()).unwrap();
    let timing = (s.fps, s.current_frame);
    let steps = s.undo.len();
    execute(&mut s, Command::MoveAnimationClip { id: clip.id, start: 10.5 }).unwrap();
    assert_eq!(s.undo.len(), steps + 1);
    assert_eq!((s.fps, s.current_frame), timing);
    assert_eq!(s.frame_range, (0, 12));
    assert_eq!((s.scene.animation_clips[0].start, s.scene.animation_clips[0].end), (10.5, 11.5));
    assert_eq!(s.scene.animation_clips[1], other);
    assert_eq!(serde_json::to_value(s.scene.node(other.nodes[0]).unwrap()).unwrap(), other_node);
    let moved = &s.scene.node(root).unwrap().transform;
    for (before, after) in [(&original.position, &moved.position), (&original.rotation, &moved.rotation)] {
        let mut expected = before.clone();
        for tr in &mut expected.keys_mut().unwrap().tracks { for k in &mut tr.keys { k.time += 10.5; } }
        assert_eq!(&expected, after);
    }
    assert!(original_world.abs_diff_eq(s.scene.at_time(11.0, || s.scene.world_matrix(root)), 1e-5));
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.node(root).unwrap().transform, original);
    assert_eq!(s.frame_range, (0, 1));
    execute(&mut s, Command::RedoSceneOperation).unwrap();
    assert_eq!(s.frame_range, (0, 12));
    let path = std::env::temp_dir().join("loom-animation-clip-save.loom");
    execute(&mut s, Command::SaveFileAs { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    let expected = s.scene.animation_clips.clone();
    execute(&mut s, Command::OpenFile { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    assert_eq!(s.scene.animation_clips, expected);
    assert_eq!(s.scene.node(root).unwrap().transform.position.keys().unwrap().tracks[0].keys[0].time, 10.75);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn invalid_clip_moves_are_atomic_and_do_not_touch_unrelated_animation() {
    let mut s = AppState::new();
    import(&mut s, "invalid");
    let id = s.scene.animation_clips[0].id;
    for (target, start) in [(id, f32::NAN), (id, f32::INFINITY), (999, 5.0), (id, f32::MAX)] {
        let before = serde_json::to_value(&s.scene).unwrap();
        let undo = s.undo.len();
        assert!(execute(&mut s, Command::MoveAnimationClip { id: target, start }).is_err());
        assert_eq!(serde_json::to_value(&s.scene).unwrap(), before);
        assert_eq!(s.undo.len(), undo);
    }
    s.scene.animation_clips[0].nodes.push(NodeId(999));
    let before = serde_json::to_value(&s.scene).unwrap();
    assert!(execute(&mut s, Command::MoveAnimationClip { id, start: 5.0 }).is_err());
    assert_eq!(serde_json::to_value(&s.scene).unwrap(), before);
    s.scene.animation_clips[0].nodes.pop();
    s.scene.animation_clips.push(AnimationClip { id: 99, name: "Shared".into(), nodes: s.scene.animation_clips[0].nodes.clone(), start: 0.0, end: 1.0 });
    assert!(execute(&mut s, Command::MoveAnimationClip { id, start: 5.0 }).is_err());
}

#[test]
fn moving_keeps_list_object_visibility_and_offset_keys_editable() {
    use loom_scene::{Controller, ObjectKind, Param, param::ListItem};
    use loom_scene::anim::TangentType;
    let mut s = AppState::new();
    import(&mut s, "all-keys");
    let clip = s.scene.animation_clips[0].clone();
    let n = s.scene.node_mut(clip.nodes[0]).unwrap();
    n.transform.rotation.controller = Controller::List(vec![ListItem { name: "Imported".into(), weight: 100.0, controller: n.transform.rotation.controller.clone() }]);
    if let ObjectKind::Bone { length, .. } = &mut n.object {
        length.ensure_keys().unwrap().tracks[0].set_key(0.125, 12.0, (TangentType::Smooth, TangentType::Smooth));
    }
    let mut visibility = Param::new(1.0f32);
    visibility.ensure_keys().unwrap().tracks[0].set_key(0.375, 0.5, (TangentType::Linear, TangentType::Linear));
    n.visibility = Some(visibility);
    n.object_offset.position.ensure_keys().unwrap().tracks[0].set_key(0.625, 2.0, (TangentType::Linear, TangentType::Linear));
    let before = serde_json::to_value(&*n).unwrap();
    let mut expected = before.clone();
    fn shifted(v: &mut serde_json::Value) {
        match v {
            serde_json::Value::Object(o) => {
                if let Some(time) = o.get_mut("time") { *time = serde_json::json!(time.as_f64().unwrap() - 2.5); }
                for v in o.values_mut() { shifted(v); }
            }
            serde_json::Value::Array(a) => for v in a { shifted(v); },
            _ => {},
        }
    }
    shifted(&mut expected);
    execute(&mut s, Command::MoveAnimationClip { id: clip.id, start: -2.5 }).unwrap();
    assert_eq!(s.frame_range, (-3, 1));
    assert_eq!(serde_json::to_value(s.scene.node(clip.nodes[0]).unwrap()).unwrap(), expected);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(serde_json::to_value(s.scene.node(clip.nodes[0]).unwrap()).unwrap(), before);
    assert_eq!(s.frame_range, (0, 1));
}

#[test]
fn old_scene_without_clip_metadata_loads_and_unchanged_move_has_no_undo_step() {
    let mut s = AppState::new();
    let mut old = serde_json::to_value(&s.scene).unwrap();
    old.as_object_mut().unwrap().remove("animation_clips");
    let back: loom_scene::Scene = serde_json::from_value(old).unwrap();
    assert!(back.animation_clips.is_empty());
    import(&mut s, "no-op");
    let steps = s.undo.len();
    let id = s.scene.animation_clips[0].id;
    execute(&mut s, Command::MoveAnimationClip { id, start: 0.0 }).unwrap();
    assert_eq!(s.undo.len(), steps);
}

#[test]
fn deleting_clip_members_prunes_metadata_and_undo_redo_restores_membership() {
    let mut s = AppState::new();
    import(&mut s, "delete-members");
    let clip = s.scene.animation_clips[0].clone();
    let hip = clip.nodes[1];
    let original_hip = serde_json::to_value(s.scene.node(hip).unwrap()).unwrap();
    s.selection = [clip.nodes[0]].into_iter().collect();
    execute(&mut s, Command::DeleteObjects).unwrap();
    assert_eq!(s.scene.animation_clips[0].nodes, vec![hip]);
    assert_eq!(s.scene.node(hip).unwrap().parent, None);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.animation_clips, vec![clip.clone()]);
    assert_eq!(serde_json::to_value(s.scene.node(hip).unwrap()).unwrap(), original_hip);
    execute(&mut s, Command::RedoSceneOperation).unwrap();
    assert_eq!(s.scene.animation_clips[0].nodes, vec![hip]);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    s.selection = clip.nodes.iter().copied().collect();
    execute(&mut s, Command::DeleteObjects).unwrap();
    assert!(s.scene.animation_clips.is_empty());
    assert!(s.scene.is_empty());
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.animation_clips, vec![clip.clone()]);
    assert_eq!(s.scene.len(), clip.nodes.len());
    execute(&mut s, Command::RedoSceneOperation).unwrap();
    assert!(s.scene.animation_clips.is_empty());
}

#[test]
fn new_scene_and_new_keep_do_not_leave_ghost_clips() {
    let mut s = AppState::new();
    import(&mut s, "new-empty");
    let clip = s.scene.animation_clips[0].clone();
    execute(&mut s, Command::NewScene).unwrap();
    assert!(s.scene.is_empty());
    assert!(s.scene.animation_clips.is_empty());
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.animation_clips, vec![clip]);
    for keep in ["objects", "objects_and_hierarchy"] {
        let mut s = AppState::new();
        import(&mut s, &format!("new-{keep}"));
        let instance = s.scene.instance_id();
        execute(&mut s, Command::NewSceneKeep { keep: keep.into() }).unwrap();
        assert_ne!(s.scene.instance_id(), instance);
        assert_eq!(s.scene.len(), 2);
        assert!(s.scene.animation_clips.is_empty());
        assert!(!s.undo.can_undo());
        assert!(s.undo.is_saved());
        for n in s.scene.nodes() { assert!(!n.transform.position.is_animated() && !n.transform.rotation.is_animated()); }
    }
}

#[test]
fn save_selected_contains_only_exported_clip_members_and_keeps_original_scene() {
    let mut s = AppState::new();
    import(&mut s, "selected-first");
    let clip = s.scene.animation_clips[0].clone();
    import(&mut s, "selected-other");
    let original = s.scene.animation_clips.clone();
    s.selection = [clip.nodes[0]].into_iter().collect();
    let path = std::env::temp_dir().join("loom-clips-save-selected.loom");
    execute(&mut s, Command::SaveSelected { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    let doc = loom_commands::file::read(&path).unwrap();
    assert_eq!(doc.scene.len(), 1);
    assert_eq!(doc.scene.animation_clips.len(), 1);
    assert_eq!(doc.scene.animation_clips[0].id, clip.id);
    assert_eq!(doc.scene.animation_clips[0].nodes, vec![clip.nodes[0]]);
    assert!(doc.scene.animation_clips.iter().flat_map(|c| &c.nodes).all(|id| doc.scene.node(*id).is_some()));
    assert_eq!(s.scene.animation_clips, original);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn new_keep_renews_empty_scene_identity_too() {
    for keep in ["objects", "objects_and_hierarchy"] {
        let mut s = AppState::new();
        let instance = s.scene.instance_id();
        execute(&mut s, Command::NewSceneKeep { keep: keep.into() }).unwrap();
        assert_ne!(s.scene.instance_id(), instance);
        assert!(s.scene.is_empty());
        assert!(s.scene.animation_clips.is_empty());
    }
}
