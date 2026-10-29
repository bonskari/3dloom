use glam::{Quat, Vec3};
use loom_commands::{execute, AppState, Command};
use loom_scene::loomrig::{HumanoidMapping, LimbMapping};
use loom_scene::loomrig_contact::{ContactKey, ContactReference, ContactSpace, ContactTrack};
use loom_scene::{Node, NodeId, ObjectKind};

fn rigged() -> (AppState, NodeId, NodeId) {
    let mut s = AppState::new();
    let root = s.scene.add_node(Node::new("root", ObjectKind::bone(1.)));
    let mut ids = vec![];
    for i in 0..3 {
        let mut n = Node::new("joint", ObjectKind::bone(10.));
        n.parent = Some(if i == 0 { root } else { ids[i - 1] });
        if i > 0 {
            n.transform.position.set(Vec3::X * 10.);
        }
        ids.push(s.scene.add_node(n));
    }
    let m = HumanoidMapping {
        root,
        pelvis: ids[0],
        spine: vec![],
        head: ids[2],
        limbs: vec![LimbMapping {
            name: "Arm".into(),
            start: ids[0],
            middle: ids[1],
            end: ids[2],
        }],
    };
    execute(
        &mut s,
        Command::CreateLoomRig {
            root: root.0,
            mapping: Some(m),
        },
    )
    .unwrap();
    (s, root, ids[2])
}
fn reference(space: ContactSpace, position: Vec3) -> ContactReference {
    ContactReference {
        space,
        position,
        rotation: Quat::from_rotation_x(0.3),
        pole_position: Vec3::new(0., 0., 20.),
    }
}
fn key(time: f32, reference: u64, blend: f32) -> ContactKey {
    ContactKey {
        time,
        reference,
        blend,
    }
}
fn point(s: &AppState, id: NodeId, time: f32) -> Vec3 {
    s.scene.at_time(time, || {
        s.scene.world_matrix(id).transform_point3(Vec3::ZERO)
    })
}

#[test]
fn world_contact_holds_body_contact_follows_root_and_free_releases() {
    for space in [ContactSpace::World, ContactSpace::Body] {
        let (mut s, root, end) = rigged();
        let target = Vec3::new(12., 8., 0.);
        let tangents = (
            loom_scene::anim::TangentType::Linear,
            loom_scene::anim::TangentType::Linear,
        );
        let keys = s
            .scene
            .node_mut(root)
            .unwrap()
            .transform
            .position
            .ensure_keys()
            .unwrap();
        keys.tracks[1].interp = loom_scene::anim::Interp::Linear;
        keys.tracks[1].set_key(0., 0., tangents);
        keys.tracks[1].set_key(40., 2., tangents);
        let bones: Vec<_> = s
            .scene
            .nodes()
            .filter(|n| n.object.is_bone())
            .cloned()
            .collect();
        let contacts = ContactTrack::new(
            [
                (7, reference(space.clone(), target)),
                (9, reference(ContactSpace::Body, target)),
            ]
            .into(),
            vec![key(10., 7, 1.), key(20., 7, 1.), key(30., 9, 0.)],
        )
        .unwrap();
        s.scene.loom_rigs[0].limbs[0].contacts = contacts;
        // Changing rig metadata directly is fixture setup, before the first evaluation.
        for time in [0., 10., 15.5, 20., 30., 10., 1., 19.25] {
            let actual = point(&s, end, time);
            let root_y = time * 0.05;
            let expected = if time < 10. || time >= 30. {
                Vec3::new(20., root_y, 0.)
            } else {
                target
                    + if space == ContactSpace::Body {
                        Vec3::Y * root_y
                    } else {
                        Vec3::ZERO
                    }
            };
            assert!(
                actual.distance(expected) < 0.001,
                "{space:?} at {time}: {actual:?}, expected {expected:?}"
            );
        }
        for bone in bones {
            assert_eq!(s.scene.node(bone.id).unwrap(), &bone);
        }
    }
}

#[test]
fn contact_targets_slide_between_independent_references_at_fractional_times() {
    let (mut s, _, end) = rigged();
    let a = Vec3::new(12., 8., 0.);
    let b = Vec3::new(8., 12., 0.);
    s.scene.loom_rigs[0].limbs[0].contacts = ContactTrack::new(
        [
            (7, reference(ContactSpace::World, a)),
            (9, reference(ContactSpace::World, b)),
        ]
        .into(),
        vec![key(10., 7, 1.), key(20., 9, 1.)],
    )
    .unwrap();
    for time in [0_f32, 10., 15.5, 20., 12.25, 1., 25.] {
        let expected = if time < 10. {
            Vec3::X * 20.
        } else {
            a.lerp(b, ((time - 10.) / 10.).clamp(0., 1.))
        };
        let actual = point(&s, end, time);
        assert!(
            actual.distance(expected) < 0.001,
            "at {time}: {actual:?} != {expected:?}"
        );
    }
}

#[test]
fn captured_contact_has_no_pop_and_preserves_stretch_under_affine_parents() {
    use glam::{Mat3, Mat4};
    for sign in [1., -1.] {
        let (mut s, root, end) = rigged();
        let limb = s.scene.loom_rigs[0].limbs[0].clone();
        let mut parent = Node::new("scaled parent", ObjectKind::dummy(1.));
        parent.transform.scale.set(Vec3::new(sign * 2., 3., 1.));
        parent
            .transform
            .set_rotation_quat(Quat::from_rotation_z(0.3));
        let parent = s.scene.add_node(parent);
        s.scene.node_mut(root).unwrap().parent = Some(parent);
        s.scene
            .node_mut(root)
            .unwrap()
            .transform
            .set_rotation_quat(Quat::from_rotation_z(0.4));
        s.scene
            .node_mut(limb.mapping.start)
            .unwrap()
            .transform
            .set_rotation_quat(Quat::from_rotation_z(0.4));
        s.scene
            .node_mut(limb.mapping.middle)
            .unwrap()
            .transform
            .set_rotation_quat(Quat::from_rotation_z(-0.8));
        let ids = [root, limb.mapping.start, limb.mapping.middle, end];
        let original: Vec<_> = ids.iter().map(|id| s.scene.world_matrix(*id)).collect();
        let target = original[3].transform_point3(Vec3::ZERO);
        let middle = original[2].transform_point3(Vec3::ZERO);
        let captured = ContactReference {
            space: ContactSpace::World,
            position: target,
            rotation: Quat::from_rotation_z(0.3),
            pole_position: middle + Vec3::Z * 20.,
        };
        // Use the existing bend plane, not an arbitrary new elbow direction.
        let start = original[1].transform_point3(Vec3::ZERO);
        let axis = (target - start).normalize();
        let bend = middle - start - axis * (middle - start).dot(axis);
        let captured = ContactReference {
            pole_position: middle + bend * 3.,
            ..captured
        };
        s.scene.loom_rigs[0].limbs[0].contacts =
            ContactTrack::new([(7, captured.clone())].into(), vec![key(0., 7, 1.)]).unwrap();
        s.scene.bump_version();
        for (id, expected) in ids.iter().zip(&original) {
            let actual = s.scene.world_matrix(*id);
            assert!(
                actual.abs_diff_eq(*expected, 0.002),
                "sign{sign} node{id:?}: {actual:?} != {expected:?}"
            );
        }
        let mut edited = captured;
        edited.rotation = Quat::from_rotation_z(0.7);
        s.scene.loom_rigs[0].limbs[0]
            .contacts
            .update_reference(7, edited)
            .unwrap();
        s.scene.bump_version();
        let rotated = s.scene.world_matrix(end);
        let expected = Mat4::from_quat(Quat::from_rotation_z(0.4)) * original[3];
        for axis in 0..3 {
            assert!(
                rotated.col(axis).abs_diff_eq(expected.col(axis), 0.002),
                "sign{sign} axis{axis}: {rotated:?} != {expected:?}"
            );
        }
        assert!(rotated.w_axis.abs_diff_eq(original[3].w_axis, 0.002));
        let a = Mat3::from_mat4(original[3]);
        let b = Mat3::from_mat4(rotated);
        assert!((a.transpose() * a).abs_diff_eq(b.transpose() * b, 0.005));
    }
}

#[test]
fn invalid_contact_frames_are_reported_and_recover_after_the_frame_is_fixed() {
    let (mut s, root, end) = rigged();
    s.scene
        .node_mut(root)
        .unwrap()
        .transform
        .scale
        .set(Vec3::new(0., 1., 1.));
    s.scene.loom_rigs[0].limbs[0].contacts = ContactTrack::new(
        [(7, reference(ContactSpace::Body, Vec3::new(12., 8., 0.)))].into(),
        vec![key(0., 7, 1.)],
    )
    .unwrap();
    assert!(s.scene.world_matrix(end).is_finite());
    let errors = s.scene.contact_errors();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].rig, 1);
    assert_eq!(errors[0].limb, "Arm");
    assert!(errors[0].message.contains("invertible"));
    s.scene
        .node_mut(root)
        .unwrap()
        .transform
        .scale
        .set(Vec3::ONE);
    assert!(s.scene.contact_errors().is_empty());
    assert!(point(&s, end, 0.).distance(Vec3::new(12., 8., 0.)) < 0.001);
}

#[test]
fn contact_blend_interpolates_and_master_motion_does_not_move_a_world_contact() {
    let (mut s, _, end) = rigged();
    let limb = s.scene.loom_rigs[0].limbs[0].clone();
    let target = Vec3::new(12., 8., 0.);
    let reference = reference(ContactSpace::World, target);
    s.scene
        .node_mut(limb.goal)
        .unwrap()
        .transform
        .position
        .set(target);
    s.scene
        .node_mut(limb.goal)
        .unwrap()
        .transform
        .set_rotation_quat(reference.rotation);
    s.scene
        .node_mut(limb.pole)
        .unwrap()
        .transform
        .position
        .set(reference.pole_position);
    s.scene.loom_rigs[0].limbs[0].blend.set(0.5);
    s.scene.bump_version();
    let expected = s.scene.world_matrix(end);
    // The contact track, rather than the legacy blend, owns authored contact IK/FK.
    s.scene.loom_rigs[0].limbs[0].blend.set(1.);
    s.scene.loom_rigs[0].limbs[0].contacts = ContactTrack::new(
        [(7, reference)].into(),
        vec![key(10., 7, 0.), key(20., 7, 1.)],
    )
    .unwrap();
    s.scene.bump_version();
    let half = s.scene.at_time(15., || s.scene.world_matrix(end));
    assert!(
        half.abs_diff_eq(expected, 0.001),
        "{half:?} != {expected:?}"
    );
    assert!(point(&s, end, 10.).abs_diff_eq(Vec3::X * 20., 0.001));
    let master = s.scene.loom_rigs[0].master;
    s.scene
        .node_mut(master)
        .unwrap()
        .transform
        .position
        .set(Vec3::Y * 2.);
    assert!(point(&s, end, 20.).distance(target) < 0.001);
    assert_eq!(s.scene.loom_rigs[0].limbs[0].blend.get(), 1.);
}
