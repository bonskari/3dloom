use glam::{Quat, Vec3};
use loom_commands::state::{CoordSystem, Tool};
use loom_commands::{execute, AppState, Axis, Command};
use loom_scene::{loomrig::HumanoidMapping, Node, ObjectKind};
fn setup(animated: bool) -> (AppState, loom_scene::NodeId, loom_scene::NodeId) {
    let mut s = AppState::new();
    let mut parent = Node::new("parent", ObjectKind::dummy(1.));
    parent.transform.position.set(Vec3::new(4., 9., 2.));
    parent
        .transform
        .set_rotation_quat(Quat::from_rotation_z(0.6));
    parent.transform.scale.set(Vec3::splat(2.));
    let p = s.scene.add_node(parent);
    let mut root = Node::new("root", ObjectKind::bone(1.));
    root.parent = Some(p);
    let root = s.scene.add_node(root);
    let mut pelvis = Node::new("pelvis", ObjectKind::bone(10.));
    pelvis.parent = Some(root);
    pelvis.transform.position.set(Vec3::X * 3.);
    if animated {
        let k = pelvis.transform.position.ensure_keys().unwrap();
        for tr in &mut k.tracks {
            tr.interp = loom_scene::anim::Interp::Linear;
        }
        for (t, x) in [(0., 3.), (20., 7.)] {
            k.tracks[0].set_key(
                t,
                x,
                (
                    loom_scene::anim::TangentType::Linear,
                    loom_scene::anim::TangentType::Linear,
                ),
            );
        }
    }
    let bone = s.scene.add_node(pelvis);
    let mut head = Node::new("head", ObjectKind::bone(1.));
    head.parent = Some(bone);
    head.transform.position.set(Vec3::X * 10.);
    let head = s.scene.add_node(head);
    execute(
        &mut s,
        Command::CreateLoomRig {
            root: root.0,
            mapping: Some(HumanoidMapping {
                root,
                pelvis: bone,
                head,
                spine: vec![],
                limbs: vec![],
            }),
        },
    )
    .unwrap();
    let control = s.scene.loom_rigs[0]
        .bindings
        .iter()
        .find(|b| b.bone == bone)
        .unwrap()
        .control;
    s.selection = [control].into();
    s.coord_system = CoordSystem::World;
    (s, bone, control)
}
#[test]
fn repeated_world_moves_accumulate_on_fk_helper() {
    for animated in [false, true] {
        let (mut s, b, c) = setup(animated);
        s.current_frame = 10;
        execute(&mut s, Command::SetFrame { frame: 10 }).unwrap();
        s.auto_key = animated;
        let before = s.scene.world_matrix(b).w_axis.truncate();
        let original = s.scene.node(b).unwrap().transform.clone();
        for step in 1..=3 {
            execute(
                &mut s,
                Command::MoveSelection {
                    delta: [10., 0., 0.],
                    system: Some(CoordSystem::World),
                },
            )
            .unwrap();
            let expected = before + Vec3::X * (10. * step as f32);
            let actual = s.scene.world_matrix(b).w_axis.truncate();
            assert!(
                actual.distance(expected) < 0.001,
                "animated {animated} step {step}: {actual:?} != {expected:?}"
            );
            assert!(s
                .scene
                .world_matrix(c)
                .abs_diff_eq(s.scene.world_matrix(b), 0.001));
        }
        if animated {
            let later = s
                .scene
                .at_time(20., || s.scene.world_matrix(b).w_axis.truncate());
            let native = s
                .scene
                .at_time(20., || s.scene.fk_world_matrix(b).w_axis.truncate());
            assert!(later.distance(native + Vec3::X * 30.) < 0.001);
            let first = s.scene.at_time(0., || s.scene.world_matrix(b));
            assert!(first.abs_diff_eq(s.scene.at_time(0., || s.scene.fk_world_matrix(b)), 0.001));
        }
        assert_eq!(s.scene.node(b).unwrap().transform, original);
    }
}
#[test]
fn repeated_world_rotations_accumulate_on_fk_helper() {
    let (mut s, b, _) = setup(true);
    let before = s.scene.world_matrix(b).to_scale_rotation_translation().1;
    for i in 1..=3 {
        execute(
            &mut s,
            Command::RotateSelection {
                axis: Axis::Z,
                degrees: 15.,
                system: Some(CoordSystem::World),
            },
        )
        .unwrap();
        let actual = s.scene.world_matrix(b).to_scale_rotation_translation().1;
        let expected = Quat::from_rotation_z((15. * i as f32).to_radians()) * before;
        assert!(
            actual.abs_diff_eq(expected, 0.001),
            "rotation step {i}: {actual:?} != {expected:?}"
        );
    }
}
#[test]
fn absolute_world_typein_reaches_each_requested_position() {
    let (mut s, b, _) = setup(true);
    s.active_tool = Tool::Move;
    for target in [[10., 15., 2.], [25., 20., 2.], [35., 30., 2.]] {
        execute(
            &mut s,
            Command::TransformTypeIn {
                absolute: true,
                values: target,
            },
        )
        .unwrap();
        let actual = s.scene.world_matrix(b).w_axis.truncate();
        assert!(
            actual.distance(Vec3::from(target)) < 0.001,
            "{actual:?} != {target:?}"
        );
    }
}

#[test]
fn absolute_world_rotation_and_direct_local_offsets_are_repeatable() {
    let (mut s, b, c) = setup(true);
    s.active_tool = Tool::Rotate;
    for rotation in [[10., 20., 30.], [25., 35., 45.], [-15., 5., 80.]] {
        execute(
            &mut s,
            Command::TransformTypeIn {
                absolute: true,
                values: rotation,
            },
        )
        .unwrap();
        let q = Quat::from_euler(
            glam::EulerRot::XYZ,
            rotation[0].to_radians(),
            rotation[1].to_radians(),
            rotation[2].to_radians(),
        );
        assert!(s
            .scene
            .world_matrix(b)
            .to_scale_rotation_translation()
            .1
            .abs_diff_eq(q, 0.001));
    }
    for position in [[1., 2., 3.], [4., 5., 6.], [4., 5., 6.]] {
        execute(
            &mut s,
            Command::SetTransform {
                id: c.0,
                position: Some(position),
                rotation_deg: Some([0.; 3]),
                scale: None,
            },
        )
        .unwrap();
        let expected = s.scene.fk_world_matrix(b) * s.scene.node(c).unwrap().transform.matrix();
        assert!(s.scene.world_matrix(b).abs_diff_eq(expected, 0.001));
        assert!(s.scene.world_matrix(c).abs_diff_eq(expected, 0.001));
    }
}
#[test]
fn fk_world_edits_undo_and_redo_the_accumulated_offset() {
    let (mut s, b, _) = setup(false);
    let original = s.scene.world_matrix(b);
    for _ in 0..3 {
        execute(
            &mut s,
            Command::MoveSelection {
                delta: [10., 0., 0.],
                system: Some(CoordSystem::World),
            },
        )
        .unwrap();
    }
    for remaining in (0..3).rev() {
        execute(&mut s, Command::UndoSceneOperation).unwrap();
        assert!(
            s.scene
                .world_matrix(b)
                .w_axis
                .truncate()
                .distance(original.w_axis.truncate() + Vec3::X * (remaining as f32 * 10.))
                < 0.001
        );
    }
    execute(&mut s, Command::RedoSceneOperation).unwrap();
    assert!(
        s.scene
            .world_matrix(b)
            .w_axis
            .truncate()
            .distance(original.w_axis.truncate() + Vec3::X * 10.)
            < 0.001
    );
}
