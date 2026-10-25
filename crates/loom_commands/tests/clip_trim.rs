use glam::Vec3;
use loom_commands::{execute, AppState, Command};
use loom_scene::{
    anim::{Interp, TangentType},
    AnimationClip, Node, ObjectKind,
};

fn scene() -> AppState {
    let mut s = AppState::new();
    let mut n = Node::new("Source", ObjectKind::bone(10.0));
    let keys = n.transform.position.ensure_keys().unwrap();
    for tr in &mut keys.tracks {
        tr.interp = Interp::Linear;
    }
    for (t, v) in [
        (0.0, Vec3::ZERO),
        (5.0, Vec3::new(3.0, 7.0, 1.0)),
        (10.0, Vec3::new(9.0, 2.0, 4.0)),
    ] {
        for (i, x) in v.to_array().into_iter().enumerate() {
            keys.tracks[i].set_key(t, x, (TangentType::Linear, TangentType::Linear));
        }
    }
    let id = s.scene.add_node(n);
    s.scene.animation_clips.push(AnimationClip {
        id: 1,
        name: "Motion".into(),
        nodes: vec![id],
        start: 0.0,
        end: 10.0,
    });
    s
}

#[test]
fn trim_preserves_fractional_interior_poses_and_undo_restores_original_keys() {
    let mut s = scene();
    let id = s.scene.animation_clips[0].nodes[0];
    let before = serde_json::to_value(&s.scene).unwrap();
    let samples: Vec<_> = [2.25, 3.0, 5.0, 7.75]
        .into_iter()
        .map(|t| s.scene.at_time(t, || s.scene.world_matrix(id)))
        .collect();
    execute(
        &mut s,
        Command::TrimAnimationClip {
            id: 1,
            start: 2.25,
            end: 7.75,
        },
    )
    .unwrap();
    assert_eq!(
        (
            s.scene.animation_clips[0].start,
            s.scene.animation_clips[0].end
        ),
        (2.25, 7.75)
    );
    for (t, expected) in [2.25, 3.0, 5.0, 7.75].into_iter().zip(samples) {
        assert!(s
            .scene
            .at_time(t, || s.scene.world_matrix(id))
            .abs_diff_eq(expected, 1e-5));
    }
    let track = &s
        .scene
        .node(id)
        .unwrap()
        .transform
        .position
        .keys()
        .unwrap()
        .tracks[0];
    assert_eq!(
        track.keys.iter().map(|k| k.time).collect::<Vec<_>>(),
        vec![2.25, 5.0, 7.75]
    );
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(serde_json::to_value(&s.scene).unwrap(), before);
    execute(&mut s, Command::RedoSceneOperation).unwrap();
    execute(&mut s, Command::MoveAnimationClip { id: 1, start: 20.0 }).unwrap();
    assert!((s.scene.animation_clips[0].end - 25.5).abs() < 1e-5);
}

#[test]
fn invalid_and_unsupported_trims_leave_scene_and_history_unchanged() {
    let mut s = scene();
    for (start, end) in [
        (5.0, 4.0),
        (-1.0, 8.0),
        (0.0, 11.0),
        (f32::NAN, 5.0),
        (1.0, f32::INFINITY),
    ] {
        let before = serde_json::to_value(&s.scene).unwrap();
        let steps = s.undo.len();
        assert!(execute(&mut s, Command::TrimAnimationClip { id: 1, start, end }).is_err());
        assert_eq!(serde_json::to_value(&s.scene).unwrap(), before);
        assert_eq!(s.undo.len(), steps);
    }
    let id = s.scene.animation_clips[0].nodes[0];
    s.scene
        .node_mut(id)
        .unwrap()
        .transform
        .position
        .keys_mut()
        .unwrap()
        .tracks[0]
        .interp = Interp::Bezier;
    let before = serde_json::to_value(&s.scene).unwrap();
    assert!(execute(
        &mut s,
        Command::TrimAnimationClip {
            id: 1,
            start: 1.0,
            end: 9.0
        }
    )
    .is_err());
    assert_eq!(serde_json::to_value(&s.scene).unwrap(), before);
}

#[test]
fn trim_one_character_clip_preserves_neighbor_motion_on_shared_bones() {
    let mut s=scene();let node=s.scene.animation_clips[0].nodes[0];
    let keys=s.scene.node_mut(node).unwrap().transform.position.keys_mut().unwrap();
    for (time,value) in [(20.,100.),(30.,200.)]{keys.tracks[0].set_key(time,value,(TangentType::Linear,TangentType::Linear));}
    s.scene.animation_clips.push(AnimationClip{id:2,name:"Next motion".into(),nodes:vec![node],start:20.,end:30.});
    let before=serde_json::to_value(&s.scene).unwrap();
    let poses:Vec<_>=[2.,5.,8.,20.,25.,30.].into_iter().map(|t|s.scene.at_time(t,||s.scene.world_matrix(node))).collect();
    execute(&mut s,Command::TrimAnimationClip{id:1,start:2.,end:8.}).unwrap();
    for (t,expected) in [2.,5.,8.,20.,25.,30.].into_iter().zip(poses){assert!(s.scene.at_time(t,||s.scene.world_matrix(node)).abs_diff_eq(expected,1e-5));}
    assert_eq!(s.scene.node(node).unwrap().transform.position.keys().unwrap().tracks[0].keys.iter().map(|k|k.time).collect::<Vec<_>>(),vec![2.,5.,8.,20.,30.]);
    assert_eq!((s.scene.animation_clips[1].start,s.scene.animation_clips[1].end),(20.,30.));
    execute(&mut s,Command::UndoSceneOperation).unwrap();assert_eq!(serde_json::to_value(&s.scene).unwrap(),before);
}
