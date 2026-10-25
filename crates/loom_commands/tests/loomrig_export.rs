use glam::{Quat, Vec3};
use loom_commands::{execute, AppState, Command, PrimitiveKind};
use loom_scene::{
    anim::{Interp, TangentType},
    loomrig::{HumanoidMapping, LimbMapping},
    Modifier, Node, NodeId, ObjectKind,
};
fn bone(s: &mut AppState, name: &str, parent: Option<NodeId>, position: Vec3) -> NodeId {
    let mut n = Node::new(name, ObjectKind::bone(10.));
    n.parent = parent;
    n.transform.position.set(position);
    s.scene.add_node(n)
}
#[test]
fn default_fbx_export_bakes_fk_and_ik_on_original_bones_without_helpers() {
    let mut s = AppState::new();
    s.frame_range = (0, 20);
    let root = bone(&mut s, "root", None, Vec3::ZERO);
    let pelvis = bone(&mut s, "pelvis", Some(root), Vec3::ZERO);
    let upper = bone(&mut s, "upper", Some(pelvis), Vec3::X * 5.);
    let lower = bone(&mut s, "lower", Some(upper), Vec3::X * 10.);
    let hand = bone(&mut s, "hand", Some(lower), Vec3::X * 10.);
    let head = bone(&mut s, "head", Some(pelvis), Vec3::Z * 10.);
    let bones = [root, pelvis, upper, lower, hand, head];
    execute(
        &mut s,
        Command::CreatePrimitive {
            kind: PrimitiveKind::Box,
            position: [10., 0., 0.],
            size: [12., 2., 2.],
        },
    )
    .unwrap();
    let mesh = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::SkinModifier).unwrap();
    execute(
        &mut s,
        Command::SkinAddBones {
            ids: vec![upper.0, lower.0, hand.0, head.0],
        },
    )
    .unwrap();
    if let Modifier::Skin(skin) = &mut s.scene.node_mut(mesh).unwrap().modifiers[0].modifier {
        skin.weights[0] = vec![(3, 1.)];
        skin.explicit[0] = true;
    }
    execute(
        &mut s,
        Command::CreateLoomRig {
            root: root.0,
            mapping: Some(HumanoidMapping {
                root,
                pelvis,
                head,
                spine: vec![],
                limbs: vec![LimbMapping {
                    name: "Arm".into(),
                    start: upper,
                    middle: lower,
                    end: hand,
                }],
            }),
        },
    )
    .unwrap();
    let head_control = s.scene.loom_rigs[0]
        .bindings
        .iter()
        .find(|b| b.bone == head)
        .unwrap()
        .control;
    s.scene
        .node_mut(head_control)
        .unwrap()
        .transform
        .set_rotation_quat(Quat::from_rotation_z(0.4));
    execute(
        &mut s,
        Command::SetLoomRigLimb {
            rig: 1,
            limb: "Arm".into(),
            solver: "limb".into(),
            blend: 1.,
        },
    )
    .unwrap();
    let goal = s.scene.loom_rigs[0].limbs[0].goal;
    let start = s.scene.node(goal).unwrap().transform.position.get();
    let keys = s
        .scene
        .node_mut(goal)
        .unwrap()
        .transform
        .position
        .ensure_keys()
        .unwrap();
    for track in &mut keys.tracks {
        track.interp = Interp::Linear;
    }
    for (time, position) in [(0., start), (10., Vec3::new(15., 8., 3.)), (20., start)] {
        for (axis, value) in position.to_array().into_iter().enumerate() {
            keys.tracks[axis].set_key(time, value, (TangentType::Linear, TangentType::Linear));
        }
    }
    assert!(s.exchange.fbx_export.bake_animation);
    let before = serde_json::to_value(&s.scene).unwrap();
    let poses: Vec<_> = [0., 5., 10., 15., 20.]
        .into_iter()
        .map(|time| {
            s.scene.at_time(time, || {
                bones
                    .iter()
                    .map(|id| s.scene.world_matrix(*id))
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    let path = std::env::temp_dir().join(format!("loomrig-export-{}.fbx", std::process::id()));
    let selected: Vec<_> = bones.iter().copied().chain([mesh]).collect();
    loom_commands::fbx_io::export(&s, &selected, &path).unwrap();
    assert_eq!(serde_json::to_value(&s.scene).unwrap(), before);
    let doc = loom_fbx::read(&std::fs::read(&path).unwrap(), 0.0254).unwrap();
    assert!(!doc.nodes.iter().any(|n| n.name.starts_with("LoomRig")));
    assert_eq!(
        doc.nodes
            .iter()
            .filter(|n| matches!(n.kind, loom_fbx::FbxKind::Bone { .. }))
            .count(),
        bones.len()
    );
    assert!(doc.nodes.iter().any(|n| n
        .skin
        .as_ref()
        .is_some_and(|skin| skin.bones.len() == 4 && !skin.weights.is_empty())));
    let mut back = AppState::new();
    loom_commands::fbx_io::import(&mut back, &path).unwrap();
    for (time, expected) in [0., 5., 10., 15., 20.].into_iter().zip(poses) {
        for (id, want) in bones.iter().zip(expected) {
            let name = &s.scene.node(*id).unwrap().name;
            let got = back.scene.nodes().find(|n| &n.name == name).unwrap().id;
            let actual = back.scene.at_time(time, || back.scene.world_matrix(got));
            assert!(
                actual.abs_diff_eq(want, 0.01),
                "{name} at{time}: {actual:?} != {want:?}"
            );
        }
    }
    let back_mesh = back
        .scene
        .nodes()
        .find(|n| n.name == s.scene.node(mesh).unwrap().name)
        .unwrap()
        .id;
    for time in [0., 5., 10., 15., 20.] {
        let expected = s.scene.at_time(time, || {
            let world = s.scene.object_matrix(mesh);
            s.scene
                .eval_mesh(mesh)
                .value
                .mesh
                .vertices()
                .iter()
                .map(|v| world.transform_point3(v.position))
                .collect::<Vec<_>>()
        });
        let actual = back.scene.at_time(time, || {
            let world = back.scene.object_matrix(back_mesh);
            back.scene
                .eval_mesh(back_mesh)
                .value
                .mesh
                .vertices()
                .iter()
                .map(|v| world.transform_point3(v.position))
                .collect::<Vec<_>>()
        });
        assert_eq!(actual.len(), expected.len());
        for (got, want) in actual.iter().zip(expected) {
            assert!(
                got.distance(want) < 0.01,
                "skin vertex at{time}: {got:?} != {want:?}"
            );
        }
    }
    assert!(back.scene.nodes().any(|n| n
        .modifiers
        .iter()
        .any(|m| matches!(m.modifier, Modifier::Skin(_)))));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn ordinary_parented_bones_keep_their_pose_without_a_control_rig() {
    let mut s = AppState::new();
    let root = bone(&mut s, "ordinary_root", None, Vec3::new(3., 4., 5.));
    s.scene
        .node_mut(root)
        .unwrap()
        .transform
        .set_rotation_quat(Quat::from_rotation_z(0.3));
    let child = bone(&mut s, "ordinary_child", Some(root), Vec3::X * 8.);
    s.scene
        .node_mut(child)
        .unwrap()
        .transform
        .set_rotation_quat(Quat::from_rotation_y(0.2));
    let expected = s.scene.world_matrix(child);
    let before = serde_json::to_value(&s.scene).unwrap();
    let path = std::env::temp_dir().join(format!(
        "loomrig-ordinary-export-{}.fbx",
        std::process::id()
    ));
    loom_commands::fbx_io::export(&s, &[root, child], &path).unwrap();
    let mut back = AppState::new();
    loom_commands::fbx_io::import(&mut back, &path).unwrap();
    let actual = back.scene.world_matrix(
        back.scene
            .nodes()
            .find(|n| n.name == "ordinary_child")
            .unwrap()
            .id,
    );
    assert!(actual.abs_diff_eq(expected, 0.001));
    assert_eq!(serde_json::to_value(&s.scene).unwrap(), before);
    std::fs::remove_file(path).unwrap();
}

#[test]
#[ignore = "requires LOOM_RETARGET_FBX real Ray MetaHuman export"]
fn real_ray342_export150_frames_bakes_rig_without_helpers() {
    use std::{hash::Hasher, io::Write, time::Instant};
    #[derive(Default)]
    struct Fingerprint(std::collections::hash_map::DefaultHasher);
    impl Write for Fingerprint {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.write(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    fn hash(scene: &loom_scene::Scene) -> u64 {
        let mut out = Fingerprint::default();
        serde_json::to_writer(&mut out, scene).unwrap();
        out.0.finish()
    }
    let input = std::env::var("LOOM_RETARGET_FBX").unwrap();
    let mut s = AppState::new();
    s.exchange.fbx_import.metahuman_body_only = true;
    let original = loom_commands::fbx_io::import(&mut s, std::path::Path::new(&input)).unwrap();
    let bones: Vec<_> = original
        .iter()
        .copied()
        .filter(|id| s.scene.node(*id).unwrap().object.is_bone())
        .collect();
    assert_eq!(bones.len(), 342);
    let root = *bones
        .iter()
        .find(|id| s.scene.node(**id).unwrap().name == "root")
        .unwrap();
    execute(
        &mut s,
        Command::CreateLoomRig {
            root: root.0,
            mapping: None,
        },
    )
    .unwrap();
    execute(
        &mut s,
        Command::SetLoomRigLimb {
            rig: 1,
            limb: "Left Leg".into(),
            solver: "limb".into(),
            blend: 1.,
        },
    )
    .unwrap();
    let leg = s.scene.loom_rigs[0]
        .limbs
        .iter()
        .find(|l| l.mapping.name == "Left Leg")
        .unwrap()
        .clone();
    let start = s.scene.node(leg.goal).unwrap().transform.position.get();
    let hip = s.scene.world_matrix(leg.mapping.start).w_axis.truncate();
    let keys = s
        .scene
        .node_mut(leg.goal)
        .unwrap()
        .transform
        .position
        .ensure_keys()
        .unwrap();
    for track in &mut keys.tracks {
        track.interp = Interp::Linear;
    }
    for (time, position) in [(0., start), (75., start.lerp(hip, 0.05)), (149., start)] {
        for (axis, value) in position.to_array().into_iter().enumerate() {
            keys.tracks[axis].set_key(time, value, (TangentType::Linear, TangentType::Linear));
        }
    }
    let head = s.scene.loom_rigs[0].mapping.head;
    let head_control = s.scene.loom_rigs[0]
        .bindings
        .iter()
        .find(|b| b.bone == head)
        .unwrap()
        .control;
    s.scene
        .node_mut(head_control)
        .unwrap()
        .transform
        .set_rotation_quat(Quat::from_rotation_z(0.1));
    s.frame_range = (0, 149);
    let before = hash(&s.scene);
    let samples: Vec<_> = [0., 37., 75., 112., 149.]
        .into_iter()
        .map(|f| {
            s.scene.at_time(f, || {
                bones
                    .iter()
                    .map(|id| s.scene.world_matrix(*id))
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    let path =
        std::env::temp_dir().join(format!("loomrig-ray342-export-{}.fbx", std::process::id()));
    let started = Instant::now();
    loom_commands::fbx_io::export(&s, &original, &path).unwrap();
    let duration = started.elapsed();
    eprintln!(
        "REAL_RAY342_EXPORT150_SECONDS {:.6}; FBX_BYTES {}",
        duration.as_secs_f64(),
        std::fs::metadata(&path).unwrap().len()
    );
    assert_eq!(hash(&s.scene), before);
    let data = loom_fbx::read(&std::fs::read(&path).unwrap(), 0.0254).unwrap();
    assert_eq!(
        data.nodes
            .iter()
            .filter(|n| matches!(n.kind, loom_fbx::FbxKind::Bone { .. }))
            .count(),
        342
    );
    assert!(!data.nodes.iter().any(|n| n.name.starts_with("LoomRig")));
    assert!(data.nodes.iter().any(|n| n.skin.is_some()));
    let mut back = AppState::new();
    loom_commands::fbx_io::import(&mut back, &path).unwrap();
    for (time, expected) in [0., 37., 75., 112., 149.].into_iter().zip(samples) {
        for (id, want) in bones.iter().zip(expected) {
            let name = &s.scene.node(*id).unwrap().name;
            let other = back.scene.nodes().find(|n| &n.name == name).unwrap().id;
            let actual = back.scene.at_time(time, || back.scene.world_matrix(other));
            assert!(
                actual.abs_diff_eq(want, 0.02),
                "{name} at{time}: export pose differs"
            );
        }
    }
    std::fs::remove_file(path).unwrap();
}
#[test]
fn ordinary_animated_bone_keeps_evaluated_default_and_baked_motion() {
    let mut s = AppState::new();
    s.frame_range = (0, 20);
    let root = bone(&mut s, "animated_root", None, Vec3::ZERO);
    let child = bone(&mut s, "animated_child", Some(root), Vec3::Z * 4.);
    let keys = s
        .scene
        .node_mut(child)
        .unwrap()
        .transform
        .rotation
        .ensure_keys()
        .unwrap();
    keys.tracks[2].interp = Interp::Linear;
    for (t, degrees) in [(0., 10.), (20., 50.)] {
        keys.tracks[2].set_key(t, degrees, (TangentType::Linear, TangentType::Linear));
    }
    s.current_frame = 10;
    s.scene.set_time(10.);
    let before = serde_json::to_value(&s.scene).unwrap();
    let path = std::env::temp_dir().join(format!(
        "loomrig-animated-export-{}.fbx",
        std::process::id()
    ));
    loom_commands::fbx_io::export(&s, &[root, child], &path).unwrap();
    let doc = loom_fbx::read(&std::fs::read(&path).unwrap(), 0.0254).unwrap();
    let node = doc
        .nodes
        .iter()
        .find(|n| n.name == "animated_child")
        .unwrap();
    assert!(
        (node.rotation_deg.z - 30.).abs() < 0.001,
        "evaluated default {:?}",
        node.rotation_deg
    );
    let mut back = AppState::new();
    loom_commands::fbx_io::import(&mut back, &path).unwrap();
    let imported = back
        .scene
        .nodes()
        .find(|n| n.name == "animated_child")
        .unwrap()
        .id;
    for time in [0., 5., 10., 15., 20.] {
        assert!(back
            .scene
            .at_time(time, || back.scene.world_matrix(imported))
            .abs_diff_eq(s.scene.at_time(time, || s.scene.world_matrix(child)), 0.001));
    }
    assert_eq!(serde_json::to_value(&s.scene).unwrap(), before);
    std::fs::remove_file(path).unwrap();
}
