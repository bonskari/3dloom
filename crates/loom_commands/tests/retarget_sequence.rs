use glam::Vec3;
use loom_commands::{
    execute,
    retarget::{retarget, BoneMapping},
    AppState, Command,
};
use loom_scene::{AnimationClip, Node, NodeId, ObjectKind, Param};

fn setup() -> (AppState, NodeId, Vec<BoneMapping>) {
    let mut s = AppState::new();
    let mut ids = Vec::new();
    for (name, parent, p) in [
        ("source", None, Vec3::Z),
        ("source_child", Some(0), Vec3::Z),
        ("target", None, Vec3::new(5., 0., 2.)),
        ("target_child", Some(2), Vec3::Z * 2.),
    ] {
        let mut n = Node::new(name, ObjectKind::bone(1.));
        n.parent = parent.map(|i| ids[i]);
        n.transform.position = Param::new(p);
        ids.push(s.scene.add_node(n));
    }
    for id in &ids[..2] {
        let p = &mut s.scene.node_mut(*id).unwrap().transform.rotation;
        let k = p.ensure_keys().unwrap();
        for (axis, track) in k.tracks.iter_mut().enumerate() {
            track.interp = loom_scene::anim::Interp::Linear;
            for (time, value) in [(0., 0.), (1., if axis == 1 { 45. } else { 0. })] {
                track.set_key(
                    time,
                    value,
                    (
                        loom_scene::anim::TangentType::Linear,
                        loom_scene::anim::TangentType::Linear,
                    ),
                );
            }
        }
    }
    s.scene.animation_clips.push(AnimationClip {
        id: 1,
        name: "first".into(),
        nodes: ids[..2].to_vec(),
        start: 0.,
        end: 1.,
    });
    let mapping = vec![
        BoneMapping {
            source: ids[0],
            target: ids[2],
        },
        BoneMapping {
            source: ids[1],
            target: ids[3],
        },
    ];
    (s, ids[2], mapping)
}
#[test]
fn disjoint_target_clips_append_and_move_only_their_native_keys() {
    let (mut s, root, map) = setup();
    let first = retarget(&mut s, 1, root, &map).unwrap()["clip"]
        .as_u64()
        .unwrap();
    let original: Vec<_> = map
        .iter()
        .map(|m| s.scene.node(m.target).unwrap().transform.clone())
        .collect();
    execute(
        &mut s,
        Command::MoveAnimationClip {
            id: 1,
            start: 10.25,
        },
    )
    .unwrap();
    let second = retarget(&mut s, 1, root, &map).unwrap()["clip"]
        .as_u64()
        .unwrap();
    for (m, old) in map.iter().zip(&original) {
        let now = &s.scene.node(m.target).unwrap().transform.rotation;
        for (a, b) in old
            .rotation
            .keys()
            .unwrap()
            .tracks
            .iter()
            .zip(&now.keys().unwrap().tracks)
        {
            assert_eq!(a.keys, b.keys[..a.keys.len()]);
        }
    }
    execute(
        &mut s,
        Command::MoveAnimationClip {
            id: second,
            start: 20.5,
        },
    )
    .unwrap();
    for (m, old) in map.iter().zip(&original) {
        let now = &s.scene.node(m.target).unwrap().transform.rotation;
        for (a, b) in old
            .rotation
            .keys()
            .unwrap()
            .tracks
            .iter()
            .zip(&now.keys().unwrap().tracks)
        {
            assert_eq!(a.keys, b.keys[..a.keys.len()]);
            assert_eq!(b.keys.last().unwrap().time, 21.5);
        }
    }
    let before = serde_json::to_value(&s.scene).unwrap();
    let undo = s.undo.len();
    for start in [0., 1., -1.] {
        assert!(execute(&mut s, Command::MoveAnimationClip { id: second, start }).is_err());
        assert_eq!(serde_json::to_value(&s.scene).unwrap(), before);
        assert_eq!(s.undo.len(), undo);
    }
    assert_eq!(
        s.scene
            .animation_clips
            .iter()
            .find(|c| c.id == first)
            .unwrap()
            .start,
        0.
    );
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(
        s.scene
            .animation_clips
            .iter()
            .find(|c| c.id == second)
            .unwrap()
            .start,
        10.25
    );
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    for (m, old) in map.iter().zip(&original) {
        assert_eq!(&s.scene.node(m.target).unwrap().transform, old);
    }
}
#[test]
fn user_keys_outside_owned_clips_reject_append_without_mutation() {
    let (mut s, root, map) = setup();
    retarget(&mut s, 1, root, &map).unwrap();
    execute(&mut s, Command::MoveAnimationClip { id: 1, start: 10. }).unwrap();
    s.scene
        .node_mut(root)
        .unwrap()
        .transform
        .rotation
        .keys_mut()
        .unwrap()
        .tracks[0]
        .set_key(
            5.,
            60.,
            (
                loom_scene::anim::TangentType::Linear,
                loom_scene::anim::TangentType::Linear,
            ),
        );
    let before = serde_json::to_value(&s.scene).unwrap();
    assert!(retarget(&mut s, 1, root, &map)
        .unwrap_err()
        .contains("unowned"));
    assert_eq!(serde_json::to_value(&s.scene).unwrap(), before);
}
#[test]
fn varying_source_scale_rejects_without_mutation() {
    let (mut s, root, map) = setup();
    s.scene
        .node_mut(map[0].source)
        .unwrap()
        .transform
        .scale
        .ensure_keys()
        .unwrap()
        .tracks[0]
        .set_key(
            1.,
            2.,
            (
                loom_scene::anim::TangentType::Linear,
                loom_scene::anim::TangentType::Linear,
            ),
        );
    let before = serde_json::to_value(&s.scene).unwrap();
    assert!(retarget(&mut s, 1, root, &map)
        .unwrap_err()
        .contains("scale"));
    assert_eq!(serde_json::to_value(&s.scene).unwrap(), before);
}
