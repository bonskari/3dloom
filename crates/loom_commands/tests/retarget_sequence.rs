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

#[test]
fn master_rotation_and_scale_do_not_change_baked_motion_when_applied_before_retarget() {
    use loom_scene::loomrig::HumanoidMapping;
    let (mut s, pelvis, map) = setup();
    let mut wrapper = Node::new("character", ObjectKind::bone(1.));
    wrapper.transform.position = Param::new(Vec3::new(6., -4., 0.));
    wrapper.transform.rotation = Param::new(Vec3::new(0., 0., 37.));
    wrapper.transform.scale = Param::new(Vec3::splat(1.3));
    let root = s.scene.add_node(wrapper);
    s.scene.node_mut(pelvis).unwrap().parent = Some(root);
    let source = s.scene.node_mut(map[0].source).unwrap();
    let p = source.transform.position.get();
    for (axis, tr) in source
        .transform
        .position
        .ensure_keys()
        .unwrap()
        .tracks
        .iter_mut()
        .enumerate()
    {
        tr.interp = loom_scene::anim::Interp::Linear;
        for (time, value) in [
            (0., p[axis]),
            (1., p[axis] + if axis == 0 { 1. } else { 0. }),
        ] {
            tr.set_key(
                time,
                value,
                (
                    loom_scene::anim::TangentType::Linear,
                    loom_scene::anim::TangentType::Linear,
                ),
            );
        }
    }
    let mut earlier = AppState::new();
    earlier.scene = s.scene.clone();
    let mapping = HumanoidMapping {
        root,
        pelvis,
        spine: vec![],
        head: map[1].target,
        limbs: vec![],
    };
    for state in [&mut s, &mut earlier] {
        execute(
            state,
            Command::CreateLoomRig {
                root: root.0,
                mapping: Some(mapping.clone()),
            },
        )
        .unwrap();
    }
    fn master_offset(s: &mut AppState) {
        let master = s.scene.loom_rigs[0].master;
        let n = s.scene.node_mut(master).unwrap();
        let mut r = n.transform.rotation.get();
        r.z += 90.;
        n.transform.rotation = Param::new(r);
        let scale = n.transform.scale.get() * 1.5;
        n.transform.scale = Param::new(scale);
    }
    retarget(&mut s, 1, root, &map).unwrap();
    master_offset(&mut s);
    master_offset(&mut earlier);
    retarget(&mut earlier, 1, root, &map).unwrap();
    for time in [0., 0.25, 1.] {
        for pair in &map {
            let expected = s.scene.at_time(time, || s.scene.world_matrix(pair.target));
            let actual = earlier
                .scene
                .at_time(time, || earlier.scene.world_matrix(pair.target));
            assert!(
                expected.abs_diff_eq(actual, 1e-4),
                "frame {time}: master changed the baked source trajectory"
            );
        }
    }
}
