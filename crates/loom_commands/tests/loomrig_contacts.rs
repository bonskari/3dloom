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

#[test]
fn set_keys_preserves_authored_contact_keys_and_target_edits_as_one_undo() {
    let (mut s, _, _) = rigged();
    let rig = s.scene.loom_rigs[0].id;
    let limb = s.scene.loom_rigs[0].limbs[0].clone();
    execute(&mut s, Command::SetFrame { frame: 10 }).unwrap();
    execute(&mut s, Command::CreateLoomRigContactKey {
        rig, limb: limb.mapping.name.clone(), time: 10., preset: "planted".into(),
    }).unwrap();
    let before = s.scene.loom_rigs.clone();
    execute(&mut s, Command::SetKeyMode).unwrap();
    let target = s.scene.world_matrix(limb.goal).w_axis.truncate() + Vec3::Y;
    execute(&mut s, Command::SetTransform {
        id: limb.goal.0, position: Some(target.to_array()), rotation_deg: None, scale: None,
    }).unwrap();
    let posed = s.scene.loom_rigs.clone();
    let position = s.set_key_buffer.as_ref().unwrap().position;
    execute(&mut s, Command::SetKeys).unwrap();
    assert_eq!(s.scene.loom_rigs, posed, "Set Keys must preserve explicit contact authoring");
    assert_eq!(s.undo.position(), position + 1);
    execute(&mut s, Command::SetKeyMode).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.loom_rigs, before);
    execute(&mut s, Command::RedoSceneOperation).unwrap();
    assert_eq!(s.scene.loom_rigs, posed);
}

#[test]
fn contact_pose_buffer_respects_ik_filter_and_explicit_discard() {
    for discard in [false, true] {
        let (mut s, _, _) = rigged();
        let rig = s.scene.loom_rigs[0].id;
        execute(&mut s, Command::CreateLoomRigContactKey {
            rig, limb: "Arm".into(), time: 0., preset: "planted".into(),
        }).unwrap();
        let before = s.scene.loom_rigs.clone();
        let goal = s.scene.loom_rigs[0].limbs[0].goal;
        execute(&mut s, Command::SetKeyMode).unwrap();
        let target = s.scene.world_matrix(goal).w_axis.truncate() + Vec3::Y;
        execute(&mut s, Command::SetTransform {
            id: goal.0, position: Some(target.to_array()), rotation_deg: None, scale: None,
        }).unwrap();
        assert_ne!(s.scene.loom_rigs, before);
        if discard {
            execute(&mut s, Command::ClearSetKeyModeBuffer).unwrap();
        } else {
            s.key_filters.ik_parameters = false;
            execute(&mut s, Command::SetKeys).unwrap();
        }
        assert_eq!(s.scene.loom_rigs, before);
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
    for (sign, author) in [(1., false), (-1., false), (1., true), (-1., true)] {
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
        let reference = if author {
            execute(
                &mut s,
                Command::CreateLoomRigContactKey {
                    rig: 1,
                    limb: "Arm".into(),
                    time: 0.,
                    preset: "planted".into(),
                },
            )
            .unwrap();
            s.scene.loom_rigs[0].limbs[0].contacts.keys()[0].reference
        } else {
            s.scene.loom_rigs[0].limbs[0].contacts =
                ContactTrack::new([(7, captured)].into(), vec![key(0., 7, 1.)]).unwrap();
            s.scene.bump_version();
            7
        };
        for (id, expected) in ids.iter().zip(&original) {
            let actual = s.scene.world_matrix(*id);
            assert!(
                actual.abs_diff_eq(*expected, 0.002),
                "sign{sign} node{id:?}: {actual:?} != {expected:?}"
            );
        }
        let mut edited = s.scene.loom_rigs[0].limbs[0].contacts.references()[&reference].clone();
        edited.rotation = Quat::from_rotation_z(0.7);
        s.scene.loom_rigs[0].limbs[0]
            .contacts
            .update_reference(reference, edited)
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
    // The contact track, rather than the legacy blend, owns authored contact IK/FK.
    s.scene.loom_rigs[0].limbs[0].blend.set(1.);
    s.scene.loom_rigs[0].limbs[0].contacts = ContactTrack::new(
        [(7, reference.clone())].into(),
        vec![key(10., 7, 0.), key(20., 7, 1.)],
    )
    .unwrap();
    s.scene.bump_version();
    // Interpolate between FK and the roll-preserving contact solution, not
    // the legacy solver's canonical axial roll.
    let ik = s.scene.at_time(20., || {
        [limb.mapping.start, limb.mapping.middle]
            .map(|id| s.scene.local_matrix(id).to_scale_rotation_translation().1)
    });
    let q0 = s
        .scene
        .fk_local_matrix(limb.mapping.start)
        .to_scale_rotation_translation()
        .1
        .slerp(ik[0], 0.5);
    let q1 = s
        .scene
        .fk_local_matrix(limb.mapping.middle)
        .to_scale_rotation_translation()
        .1
        .slerp(ik[1], 0.5);
    let parent =
        glam::Mat4::from_quat(q0) * glam::Mat4::from_rotation_translation(q1, Vec3::X * 10.);
    let expected = glam::Mat4::from_rotation_translation(
        (q0 * q1).slerp(reference.rotation, 0.5),
        parent.transform_point3(Vec3::X * 10.),
    );
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

#[test]
fn contact_authoring_captures_solved_pose_and_edits_are_atomic_and_undoable() {
    let (mut s, root, end) = rigged();
    let limb = s.scene.loom_rigs[0].limbs[0].clone();
    s.scene
        .node_mut(limb.mapping.start)
        .unwrap()
        .transform
        .set_rotation_quat(Quat::from_euler(glam::EulerRot::XYZ, 0.25, 0.1, 0.4));
    s.scene
        .node_mut(limb.mapping.middle)
        .unwrap()
        .transform
        .set_rotation_quat(Quat::from_euler(glam::EulerRot::XYZ, -0.15, 0.2, -0.8));
    let source: Vec<_> = s.scene.nodes().cloned().collect();
    let before = s.scene.loom_rigs.clone();
    let undo = s.undo.position();
    let ids = [root, limb.mapping.start, limb.mapping.middle, end];
    let visible: Vec<_> = s.scene.at_time(10., || {
        ids.iter().map(|id| s.scene.world_matrix(*id)).collect()
    });
    execute(
        &mut s,
        Command::CreateLoomRigContactKey {
            rig: 1,
            limb: "Arm".into(),
            time: 10.,
            preset: "planted".into(),
        },
    )
    .unwrap();
    assert_eq!(s.undo.position(), undo + 1);
    assert_eq!(s.scene.time(), 0.);
    for (id, expected) in ids.into_iter().zip(visible) {
        let actual = s.scene.at_time(10., || s.scene.world_matrix(id));
        assert!(
            actual.abs_diff_eq(expected, 0.001),
            "{id:?}: {actual:?} != {expected:?}"
        );
    }
    let created = s.scene.loom_rigs.clone();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.loom_rigs, before);
    execute(&mut s, Command::RedoSceneOperation).unwrap();
    assert_eq!(s.scene.loom_rigs, created);
    for (time, preset) in [(20., "planted"), (30., "sliding"), (40., "free")] {
        execute(
            &mut s,
            Command::CreateLoomRigContactKey {
                rig: 1,
                limb: "Arm".into(),
                time,
                preset: preset.into(),
            },
        )
        .unwrap();
    }
    let track = &s.scene.loom_rigs[0].limbs[0].contacts;
    assert_eq!(track.keys()[0].reference, track.keys()[1].reference);
    assert_ne!(track.keys()[1].reference, track.keys()[2].reference);
    assert_eq!(track.keys()[3].blend, 0.);
    assert_eq!(
        track.references()[&track.keys()[3].reference].space,
        ContactSpace::Body
    );
    let reference = track.keys()[0].reference;
    let mut pose = track.references()[&reference].clone();
    pose.position.y += 1.;
    execute(
        &mut s,
        Command::EditLoomRigContactReference {
            rig: 1,
            limb: "Arm".into(),
            reference,
            pose: pose.clone(),
        },
    )
    .unwrap();
    assert_eq!(
        s.scene.loom_rigs[0].limbs[0].contacts.references()[&reference],
        pose
    );
    let mut changed = s.scene.loom_rigs[0].limbs[0].contacts.keys()[2].clone();
    changed.blend = 0.7;
    execute(
        &mut s,
        Command::EditLoomRigContactKey {
            rig: 1,
            limb: "Arm".into(),
            key: changed.clone(),
        },
    )
    .unwrap();
    assert_eq!(s.scene.loom_rigs[0].limbs[0].contacts.keys()[2], changed);
    execute(
        &mut s,
        Command::MoveLoomRigContactKey {
            rig: 1,
            limb: "Arm".into(),
            from: 20.,
            to: 25.,
        },
    )
    .unwrap();
    let snapshot = s.scene.loom_rigs.clone();
    let position = s.undo.position();
    assert!(execute(
        &mut s,
        Command::MoveLoomRigContactKey {
            rig: 1,
            limb: "Arm".into(),
            from: 10.,
            to: 25.
        }
    )
    .is_err());
    assert_eq!(s.scene.loom_rigs, snapshot);
    assert_eq!(s.undo.position(), position);
    execute(
        &mut s,
        Command::DeleteLoomRigContactKey {
            rig: 1,
            limb: "Arm".into(),
            time: 10.,
        },
    )
    .unwrap();
    assert!(s.scene.loom_rigs[0].limbs[0]
        .contacts
        .references()
        .contains_key(&reference));
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.loom_rigs, snapshot);
    assert_eq!(s.scene.nodes().cloned().collect::<Vec<_>>(), source);
}

#[test]
fn invalid_contact_authoring_leaves_the_scene_and_undo_unchanged() {
    let (mut s, _, _) = rigged();
    let before = serde_json::to_value(&s.scene).unwrap();
    let undo = s.undo.position();
    for (rig, time, preset) in [
        (1, f32::NAN, "planted"),
        (1, f32::INFINITY, "planted"),
        (99, 0., "planted"),
        (1, 0., "invalid"),
    ] {
        assert!(execute(
            &mut s,
            Command::CreateLoomRigContactKey {
                rig,
                limb: "Arm".into(),
                time,
                preset: preset.into()
            }
        )
        .is_err());
        assert_eq!(serde_json::to_value(&s.scene).unwrap(), before);
        assert_eq!(s.undo.position(), undo);
    }
}

#[test]
fn contact_commands_publish_typed_schemas_and_roundtrip_through_command_json() {
    let (mut s, _, _) = rigged();
    let create = Command::CreateLoomRigContactKey {
        rig: 1,
        limb: "Arm".into(),
        time: 10.,
        preset: "planted".into(),
    };
    let decoded: Command = serde_json::from_value(serde_json::to_value(create).unwrap()).unwrap();
    execute(&mut s, decoded).unwrap();
    let registry = loom_commands::registry();
    let key = registry
        .iter()
        .find(|entry| entry.name == "Edit LoomRig Contact Key")
        .unwrap();
    assert_eq!(
        key.args_schema["properties"]["key"]["required"],
        serde_json::json!(["time", "reference", "blend"])
    );
    let reference = registry
        .iter()
        .find(|entry| entry.name == "Edit LoomRig Contact Reference")
        .unwrap();
    assert_eq!(
        reference.args_schema["properties"]["pose"]["properties"]["space"]["enum"],
        serde_json::json!(["body", "world"])
    );
    assert_eq!(
        reference.args_schema["properties"]["pose"]["properties"]["rotation"]["minItems"],
        4
    );
    let reference_id = s.scene.loom_rigs[0].limbs[0].contacts.keys()[0].reference;
    let pose = s.scene.loom_rigs[0].limbs[0].contacts.references()[&reference_id].clone();
    let edit = Command::EditLoomRigContactReference {
        rig: 1,
        limb: "Arm".into(),
        reference: reference_id,
        pose,
    };
    let undo = s.undo.position();
    let decoded: Command = serde_json::from_value(serde_json::to_value(edit).unwrap()).unwrap();
    execute(&mut s, decoded).unwrap();
    assert_eq!(
        s.undo.position(),
        undo,
        "an unchanged reference must not add an undo step"
    );
}

#[test]
fn new_plant_after_free_captures_a_new_stance_and_duplicate_creation_is_atomic() {
    let (mut s, _, end) = rigged();
    for (time, preset) in [(10., "planted"), (20., "free")] {
        execute(
            &mut s,
            Command::CreateLoomRigContactKey {
                rig: 1,
                limb: "Arm".into(),
                time,
                preset: preset.into(),
            },
        )
        .unwrap();
    }
    let master = s.scene.loom_rigs[0].master;
    s.scene
        .node_mut(master)
        .unwrap()
        .transform
        .position
        .set(Vec3::Y * 2.);
    let expected = point(&s, end, 30.);
    execute(
        &mut s,
        Command::CreateLoomRigContactKey {
            rig: 1,
            limb: "Arm".into(),
            time: 30.,
            preset: "planted".into(),
        },
    )
    .unwrap();
    let track = &s.scene.loom_rigs[0].limbs[0].contacts;
    assert_ne!(track.keys()[0].reference, track.keys()[2].reference);
    assert!(point(&s, end, 30.).distance(expected) < 0.001);
    let before = s.scene.loom_rigs.clone();
    let undo = s.undo.position();
    assert!(execute(
        &mut s,
        Command::CreateLoomRigContactKey {
            rig: 1,
            limb: "Arm".into(),
            time: 30.,
            preset: "sliding".into()
        }
    )
    .is_err());
    assert_eq!(s.scene.loom_rigs, before);
    assert_eq!(s.undo.position(), undo);
    let reference = before[0].limbs[0].contacts.keys()[0].reference;
    let mut pose = before[0].limbs[0].contacts.references()[&reference].clone();
    pose.rotation = Quat::from_xyzw(0., 0., 0., 0.);
    assert!(execute(
        &mut s,
        Command::EditLoomRigContactReference {
            rig: 1,
            limb: "Arm".into(),
            reference,
            pose
        }
    )
    .is_err());
    assert_eq!(s.scene.loom_rigs, before);
    assert_eq!(s.undo.position(), undo);
}

#[test]
#[ignore = "requires LOOM_RAY_CONTACT_SOURCE production scene"]
fn production_ray_contact_capture_preserves_source_data_and_solved_pose() {
    use std::hash::Hasher;
    struct Fingerprint(std::collections::hash_map::DefaultHasher);
    impl std::io::Write for Fingerprint {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.write(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    fn source(s: &AppState) -> u64 {
        let mut out = Fingerprint(Default::default());
        for node in s.scene.nodes() {
            serde_json::to_writer(&mut out, node).unwrap();
        }
        serde_json::to_writer(&mut out, &s.scene.materials).unwrap();
        out.0.finish()
    }
    let path = std::env::var("LOOM_RAY_CONTACT_SOURCE").unwrap();
    let mut s = AppState::new();
    execute(&mut s, Command::OpenFile { path: Some(path) }).unwrap();
    execute(&mut s, Command::SetFrame { frame: 39 }).unwrap();
    let before = source(&s);
    let rig = s.scene.loom_rigs[0].id;
    for name in ["Left Leg", "Right Leg", "Left Arm", "Right Arm"] {
        let limb = s.scene.loom_rigs[0]
            .limbs
            .iter()
            .find(|l| l.mapping.name == name)
            .unwrap()
            .clone();
        let ids = [limb.mapping.start, limb.mapping.middle, limb.mapping.end];
        let expected: Vec<_> = ids.iter().map(|id| s.scene.world_matrix(*id)).collect();
        execute(
            &mut s,
            Command::CreateLoomRigContactKey {
                rig,
                limb: name.into(),
                time: 39.,
                preset: "planted".into(),
            },
        )
        .unwrap();
        for (id, expected) in ids.into_iter().zip(expected) {
            let actual = s.scene.world_matrix(id);
            assert!(
                actual.abs_diff_eq(expected, 0.003),
                "{name} {id:?} solver {:?}: capture changed solved pose; expected {expected:?}; actual {actual:?}", limb.solver
            );
        }
        assert!(s.scene.contact_errors().is_empty());
        assert_eq!(
            source(&s),
            before,
            "capture changed a source node, Skin data or materials"
        );
        execute(&mut s, Command::UndoSceneOperation).unwrap();
        assert_eq!(source(&s), before);
    }
    for name in ["Left Leg", "Right Leg", "Left Arm", "Right Arm"] {
        execute(
            &mut s,
            Command::CreateLoomRigContactKey {
                rig,
                limb: name.into(),
                time: 39.,
                preset: "planted".into(),
            },
        )
        .unwrap();
    }
    let expected: Vec<_> = s
        .scene
        .nodes()
        .map(|node| (node.id, s.scene.world_matrix(node.id)))
        .collect();
    let materials = serde_json::to_value(&s.scene.materials).unwrap();
    execute(
        &mut s,
        Command::NewSceneKeep {
            keep: "objects_and_hierarchy".into(),
        },
    )
    .unwrap();
    assert!(s.scene.loom_rigs[0]
        .limbs
        .iter()
        .all(|limb| limb.contacts.keys().is_empty() && limb.contacts.constant_state().is_some()));
    assert_eq!(serde_json::to_value(&s.scene.materials).unwrap(), materials);
    for time in [0., 60., 149.] {
        s.scene.at_time(time, || {
            for (id, world) in &expected {
                assert!(
                    s.scene.world_matrix(*id).abs_diff_eq(*world, 0.003),
                    "freeze changed {id:?} at {time}"
                );
            }
        });
    }
    assert!(s.scene.contact_errors().is_empty());
}

#[test]
fn new_keep_freezes_contacts_at_current_pose_without_retaining_contact_animation() {
    for (frame, scale) in [0, 10, 15, 30].into_iter().flat_map(|frame| {
        [Vec3::ONE, Vec3::new(2., 3., 1.), Vec3::new(-2., 3., 1.)].map(|scale| (frame, scale))
    }) {
        let (mut s, root, end) = rigged();
        let mut parent = Node::new("affine parent", ObjectKind::dummy(1.));
        parent.transform.scale.set(scale);
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
        s.scene.loom_rigs[0].limbs[0].contacts = ContactTrack::new(
            [
                (1, reference(ContactSpace::World, Vec3::new(12., 8., 0.))),
                (2, reference(ContactSpace::Body, Vec3::new(8., 12., 0.))),
            ]
            .into(),
            vec![key(10., 1, 1.), key(20., 2, 0.5)],
        )
        .unwrap();
        s.scene.bump_version();
        execute(&mut s, Command::SetFrame { frame }).unwrap();
        let expected = s.scene.world_matrix(end);
        execute(
            &mut s,
            Command::NewSceneKeep {
                keep: "objects_and_hierarchy".into(),
            },
        )
        .unwrap();
        assert!(s.scene.loom_rigs[0].limbs[0].contacts.keys().is_empty());
        let path = std::env::temp_dir().join(format!(
            "loom-contact-freeze-{}-{frame}-{}.loom",
            std::process::id(),
            scale.x.to_bits()
        ));
        execute(
            &mut s,
            Command::SaveFileAsCopy {
                path: Some(path.to_string_lossy().into_owned()),
            },
        )
        .unwrap();
        let mut reopened = AppState::new();
        execute(
            &mut reopened,
            Command::OpenFile {
                path: Some(path.to_string_lossy().into_owned()),
            },
        )
        .unwrap();
        std::fs::remove_file(path).unwrap();
        assert!(reopened
            .scene
            .world_matrix(end)
            .abs_diff_eq(expected, 0.001));
        let reads = loom_scene::anim::animated_reads();
        for time in [-100., 0., 10., 15., 50.] {
            let actual = s.scene.at_time(time, || s.scene.world_matrix(end));
            assert!(
                actual.abs_diff_eq(expected, 0.001),
                "frame{frame},time{time}: {actual:?} != {expected:?}"
            );
        }
        assert_eq!(
            loom_scene::anim::animated_reads(),
            reads,
            "frozen Scene retained contact time dependence"
        );
    }
}

#[test]
fn deleting_selected_control_animation_freezes_only_its_limb_in_one_undo_step() {
    let (mut s, _, end) = rigged();
    s.scene.loom_rigs[0].limbs[0].contacts = ContactTrack::new(
        [
            (1, reference(ContactSpace::World, Vec3::new(12., 8., 0.))),
            (2, reference(ContactSpace::Body, Vec3::new(8., 12., 0.))),
        ]
        .into(),
        vec![key(10., 1, 1.), key(20., 2, 0.5)],
    )
    .unwrap();
    s.scene.bump_version();
    let root = s.scene.loom_rigs[0].mapping.root;
    let mut joints = Vec::new();
    for i in 0..3 {
        let mut node = Node::new("other limb", ObjectKind::bone(10.));
        node.parent = Some(if i == 0 { root } else { joints[i - 1] });
        node.transform
            .position
            .set(if i == 0 { Vec3::Y * 30. } else { Vec3::X * 10. });
        joints.push(s.scene.add_node(node));
    }
    let mut mapping = s.scene.loom_rigs[0].mapping.clone();
    mapping.limbs.push(LimbMapping {
        name: "Other".into(),
        start: joints[0],
        middle: joints[1],
        end: joints[2],
    });
    execute(&mut s, Command::SetLoomRigMapping { rig: 1, mapping }).unwrap();
    s.scene.loom_rigs[0].limbs[1].contacts = s.scene.loom_rigs[0].limbs[0].contacts.clone();
    let doc = loom_commands::file::Document {
        format: loom_commands::file::FORMAT,
        app: "3dloom".into(),
        scene: s.scene.clone(),
        settings: loom_commands::file::SceneSettings::from_state(&s),
    };
    loom_commands::file::merge_map(&mut s, &doc, None, &[], "Other rig").unwrap();
    execute(&mut s, Command::SetFrame { frame: 15 }).unwrap();
    let goal = s.scene.loom_rigs[0].limbs[0].goal;
    let linear = (
        loom_scene::anim::TangentType::Linear,
        loom_scene::anim::TangentType::Linear,
    );
    let keys = s
        .scene
        .node_mut(goal)
        .unwrap()
        .transform
        .position
        .ensure_keys()
        .unwrap();
    keys.tracks[0].set_key(0., 0., linear);
    keys.tracks[0].set_key(30., 10., linear);
    let expected = s.scene.world_matrix(end);
    let rigs = s.scene.loom_rigs.clone();
    let node = s.scene.node(goal).unwrap().clone();
    let undo = s.undo.position();
    s.selection = [goal].into();
    execute(&mut s, Command::DeleteSelectedAnimation).unwrap();
    assert_eq!(s.undo.position(), undo + 1);
    assert!(s.scene.loom_rigs[0].limbs[0].contacts.keys().is_empty());
    assert!(s
        .scene
        .node(goal)
        .unwrap()
        .transform
        .position
        .keys()
        .is_none());
    assert_eq!(s.scene.loom_rigs[1], rigs[1]);
    assert_eq!(s.scene.loom_rigs[0].limbs[1], rigs[0].limbs[1]);
    for time in [0., 15., 30.] {
        assert!(s
            .scene
            .at_time(time, || s.scene.world_matrix(end))
            .abs_diff_eq(expected, 0.001));
    }
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.loom_rigs, rigs);
    assert_eq!(s.scene.node(goal).unwrap(), &node);
    execute(&mut s, Command::RedoSceneOperation).unwrap();
    assert!(s.scene.loom_rigs[0].limbs[0].contacts.keys().is_empty());
}

#[test]
fn invalid_contact_freeze_is_atomic_even_when_selected_node_tracks_would_change() {
    let (mut s, root, _) = rigged();
    let goal = s.scene.loom_rigs[0].limbs[0].goal;
    let linear = (
        loom_scene::anim::TangentType::Linear,
        loom_scene::anim::TangentType::Linear,
    );
    s.scene
        .node_mut(goal)
        .unwrap()
        .transform
        .position
        .ensure_keys()
        .unwrap()
        .tracks[0]
        .set_key(0., 2., linear);
    s.scene
        .node_mut(root)
        .unwrap()
        .transform
        .scale
        .set(Vec3::new(0., 1., 1.));
    s.scene.loom_rigs[0].limbs[0].contacts = ContactTrack::new(
        [(1, reference(ContactSpace::Body, Vec3::X))].into(),
        vec![key(0., 1, 1.)],
    )
    .unwrap();
    s.scene.bump_version();
    s.selection = [goal].into();
    let before = serde_json::to_value(&s.scene).unwrap();
    let undo = s.undo.position();
    assert!(execute(&mut s, Command::DeleteSelectedAnimation).is_err());
    assert_eq!(serde_json::to_value(&s.scene).unwrap(), before);
    assert_eq!(s.undo.position(), undo);
}

#[test]
fn ik_cache_drops_old_time_dependence_and_recovers_it_for_new_native_animation() {
    let (mut s, _, end) = rigged();
    let master = s.scene.loom_rigs[0].master;
    let animate = |s: &mut AppState| {
        let keys = s
            .scene
            .node_mut(master)
            .unwrap()
            .transform
            .position
            .ensure_keys()
            .unwrap();
        keys.tracks[0].interp = loom_scene::anim::Interp::Linear;
        let linear = (
            loom_scene::anim::TangentType::Linear,
            loom_scene::anim::TangentType::Linear,
        );
        keys.tracks[0].set_key(0., 0., linear);
        keys.tracks[0].set_key(10., 2., linear);
    };
    animate(&mut s);
    assert!(point(&s, end, 0.).abs_diff_eq(Vec3::X * 20., 0.001));
    assert!(point(&s, end, 10.).abs_diff_eq(Vec3::X * 22., 0.001));
    s.scene.node_mut(master).unwrap().transform.position = loom_scene::Param::new(Vec3::X);
    let reads = loom_scene::anim::animated_reads();
    for time in [0., 10., 5.] {
        assert!(point(&s, end, time).abs_diff_eq(Vec3::X * 21., 0.001));
    }
    assert_eq!(
        loom_scene::anim::animated_reads(),
        reads,
        "removed animation kept the IK cache time-dependent"
    );
    animate(&mut s);
    assert!(point(&s, end, 0.).abs_diff_eq(Vec3::X * 20., 0.001));
    assert!(point(&s, end, 10.).abs_diff_eq(Vec3::X * 22., 0.001));
    assert!(loom_scene::anim::animated_reads() > reads);
}

#[test]
fn deleting_body_contact_animation_keeps_body_inheritance_and_source_motion() {
    let (mut s, root, end) = rigged();
    let linear = (
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
    keys.tracks[1].set_key(0., 0., linear);
    keys.tracks[1].set_key(40., 2., linear);
    s.scene.loom_rigs[0].limbs[0].contacts = ContactTrack::new(
        [
            (1, reference(ContactSpace::Body, Vec3::new(12., 8., 0.))),
            (2, reference(ContactSpace::Body, Vec3::new(8., 12., 0.))),
        ]
        .into(),
        vec![key(10., 1, 1.), key(20., 2, 1.)],
    )
    .unwrap();
    s.scene.bump_version();
    execute(&mut s, Command::SetFrame { frame: 15 }).unwrap();
    let expected = s.scene.world_matrix(end).w_axis.truncate();
    let root_before = s.scene.node(root).unwrap().transform.clone();
    s.selection = [s.scene.loom_rigs[0].limbs[0].goal].into();
    execute(&mut s, Command::DeleteSelectedAnimation).unwrap();
    assert_eq!(s.scene.node(root).unwrap().transform, root_before);
    for time in [0., 15., 30.] {
        let target = expected + Vec3::Y * ((time - 15.) * 0.05);
        assert!(
            point(&s, end, time).distance(target) < 0.001,
            "body inheritance lost at {time}"
        );
    }
}

#[test]
fn contact_controller_world_poses_follow_the_effective_sample_without_changing_raw_helpers() {
    let (mut s, root, _) = rigged();
    s.scene
        .node_mut(root)
        .unwrap()
        .transform
        .position
        .set(Vec3::Y * 2.);
    let limb = s.scene.loom_rigs[0].limbs[0].clone();
    let raw = s.scene.node(limb.goal).unwrap().transform.clone();
    s.scene.loom_rigs[0].limbs[0].contacts = ContactTrack::new(
        [
            (1, reference(ContactSpace::World, Vec3::new(12., 8., 0.))),
            (2, reference(ContactSpace::Body, Vec3::new(8., 12., 0.))),
        ]
        .into(),
        vec![key(10., 1, 1.), key(20., 2, 1.)],
    )
    .unwrap();
    s.scene.bump_version();
    for time in [0., 10., 15., 20., 12.5] {
        s.scene.at_time(time, || {
            let sample = s.scene.loom_rigs[0].limbs[0]
                .contacts
                .sample(time, s.scene.world_matrix(root))
                .unwrap();
            let goal = s.scene.world_matrix(limb.goal);
            let pole = s.scene.world_matrix(limb.pole);
            if let Some(sample) = sample {
                assert!(goal.w_axis.truncate().distance(sample.position) < 0.001);
                assert!(goal
                    .to_scale_rotation_translation()
                    .1
                    .abs_diff_eq(sample.rotation, 0.001));
                assert!(pole.w_axis.truncate().distance(sample.pole_position) < 0.001);
            } else {
                assert!(goal.abs_diff_eq(raw.matrix(), 0.001));
            }
        });
    }
    assert_eq!(s.scene.node(limb.goal).unwrap().transform, raw);
}

#[test]
fn set_transform_edits_contact_goals_and_poles_with_atomic_undo_and_keeps_source_tracks() {
    let (mut s, root, end) = rigged();
    s.scene
        .node_mut(root)
        .unwrap()
        .transform
        .position
        .set(Vec3::Y * 2.);
    let limb = s.scene.loom_rigs[0].limbs[0].clone();
    s.scene.loom_rigs[0].limbs[0].contacts = ContactTrack::new(
        [
            (1, reference(ContactSpace::World, Vec3::new(12., 8., 0.))),
            (2, reference(ContactSpace::Body, Vec3::new(8., 12., 0.))),
        ]
        .into(),
        vec![key(10., 1, 1.), key(20., 2, 1.)],
    )
    .unwrap();
    s.scene.bump_version();
    execute(&mut s, Command::SetFrame { frame: 15 }).unwrap();
    let source: Vec<_> = s.scene.nodes().cloned().collect();
    let before = s.scene.loom_rigs.clone();
    let undo = s.undo.position();
    let sample = before[0].limbs[0]
        .contacts
        .sample(15., s.scene.world_matrix(root))
        .unwrap()
        .unwrap();
    let position = sample.position + Vec3::new(1., -2., 0.5);
    let rotation = Quat::from_rotation_y(0.4) * sample.rotation;
    let (x, y, z) = rotation.to_euler(glam::EulerRot::XYZ);
    execute(
        &mut s,
        Command::SetTransform {
            id: limb.goal.0,
            position: Some(position.to_array()),
            rotation_deg: Some([x.to_degrees(), y.to_degrees(), z.to_degrees()]),
            scale: None,
        },
    )
    .unwrap();
    assert_eq!(s.undo.position(), undo + 1);
    assert!(
        s.scene
            .world_matrix(limb.goal)
            .w_axis
            .truncate()
            .distance(position)
            < 0.001
    );
    assert!(
        s.scene
            .world_matrix(end)
            .w_axis
            .truncate()
            .distance(position)
            < 0.001
    );
    assert_eq!(s.scene.nodes().cloned().collect::<Vec<_>>(), source);
    let edited = s.scene.loom_rigs.clone();
    assert_ne!(edited, before);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.loom_rigs, before);
    execute(&mut s, Command::RedoSceneOperation).unwrap();
    assert_eq!(s.scene.loom_rigs, edited);
    let goal = s.scene.world_matrix(limb.goal);
    let middle = s.scene.world_matrix(limb.mapping.middle);
    let pole = sample.pole_position + Vec3::new(7., -5., -8.);
    execute(
        &mut s,
        Command::SetTransform {
            id: limb.pole.0,
            position: Some(pole.to_array()),
            rotation_deg: None,
            scale: None,
        },
    )
    .unwrap();
    assert!(
        s.scene
            .world_matrix(limb.pole)
            .w_axis
            .truncate()
            .distance(pole)
            < 0.001
    );
    assert!(s.scene.world_matrix(limb.goal).abs_diff_eq(goal, 0.001));
    assert!(
        s.scene
            .world_matrix(end)
            .w_axis
            .truncate()
            .distance(position)
            < 0.001
    );
    assert!(!s
        .scene
        .world_matrix(limb.mapping.middle)
        .abs_diff_eq(middle, 0.001));
    assert_eq!(s.scene.nodes().cloned().collect::<Vec<_>>(), source);
    let before = s.scene.loom_rigs.clone();
    let undo = s.undo.position();
    assert!(execute(
        &mut s,
        Command::SetTransform {
            id: limb.goal.0,
            position: Some([f32::NAN, 0., 0.]),
            rotation_deg: None,
            scale: None
        }
    )
    .is_err());
    assert_eq!(s.scene.loom_rigs, before);
    assert_eq!(s.undo.position(), undo);
}

#[test]
fn parented_contact_transform_uses_local_fields_without_losing_affine_body_orientation() {
    for sign in [1., -1.] {
        let (mut s, root, end) = rigged();
        let limb = s.scene.loom_rigs[0].limbs[0].clone();
        let mut parent = Node::new("body frame", ObjectKind::dummy(1.));
        parent.transform.scale.set(Vec3::new(sign * 2., 3., 1.));
        parent
            .transform
            .set_rotation_quat(Quat::from_rotation_z(0.4));
        parent.transform.position.set(Vec3::new(3., 4., 5.));
        let parent = s.scene.add_node(parent);
        s.scene.node_mut(root).unwrap().parent = Some(parent);
        s.scene.node_mut(limb.goal).unwrap().parent = Some(parent);
        s.scene.node_mut(limb.pole).unwrap().parent = Some(parent);
        s.scene.loom_rigs[0].limbs[0].contacts =
            ContactTrack::constant(reference(ContactSpace::Body, Vec3::new(12., 8., 0.)), 1.)
                .unwrap();
        s.scene.bump_version();
        let raw = s.scene.node(limb.goal).unwrap().transform.clone();
        let frame = s.scene.world_matrix(parent);
        let position = Vec3::new(1., 2., 0.);
        let rotation = Quat::from_euler(
            glam::EulerRot::XYZ,
            10_f32.to_radians(),
            20_f32.to_radians(),
            30_f32.to_radians(),
        );
        let expected_rotation =
            loom_scene::loomrig_contact::affine_rotation(frame * glam::Mat4::from_quat(rotation))
                .unwrap();
        execute(
            &mut s,
            Command::SetTransform {
                id: limb.goal.0,
                position: Some(position.to_array()),
                rotation_deg: Some([10., 20., 30.]),
                scale: None,
            },
        )
        .unwrap();
        let goal = s.scene.world_matrix(limb.goal);
        assert!(
            goal.w_axis
                .truncate()
                .distance(frame.transform_point3(position))
                < 0.001
        );
        let actual_rotation = goal.to_scale_rotation_translation().1;
        assert!(
            actual_rotation.abs_diff_eq(expected_rotation, 0.001)
                || actual_rotation.abs_diff_eq(-expected_rotation, 0.001)
        );
        assert!(
            s.scene
                .world_matrix(end)
                .w_axis
                .truncate()
                .distance(goal.w_axis.truncate())
                < 0.003
        );
        assert_eq!(s.scene.node(limb.goal).unwrap().transform, raw);
        let before = s.scene.loom_rigs.clone();
        let foot = s.scene.world_matrix(end);
        execute(
            &mut s,
            Command::SetTransform {
                id: limb.goal.0,
                position: None,
                rotation_deg: None,
                scale: Some([2., 3., 4.]),
            },
        )
        .unwrap();
        assert_eq!(s.scene.loom_rigs, before);
        assert_eq!(
            s.scene.node(limb.goal).unwrap().transform.scale.get(),
            Vec3::new(2., 3., 4.)
        );
        assert!(s.scene.world_matrix(end).abs_diff_eq(foot, 0.003));
        let pole = Vec3::new(4., 1., 6.);
        execute(
            &mut s,
            Command::SetTransform {
                id: limb.pole.0,
                position: Some(pole.to_array()),
                rotation_deg: None,
                scale: None,
            },
        )
        .unwrap();
        assert!(
            s.scene
                .world_matrix(limb.pole)
                .w_axis
                .truncate()
                .distance(frame.transform_point3(pole))
                < 0.001
        );
    }
}

#[test]
fn cross_rig_contact_dependency_cycle_is_rejected_without_recursion() {
    let (mut s, root, _) = rigged();
    let goal = s.scene.loom_rigs[0].limbs[0].goal;
    let other_root = s.scene.add_node(Node::new("other root", ObjectKind::bone(1.)));
    let mut other_goal_node = s.scene.node(goal).unwrap().clone();
    other_goal_node.id = NodeId(0);
    let other_goal = s.scene.add_node(other_goal_node);
    assert_ne!(goal, other_goal);
    let mut other = s.scene.loom_rigs[0].clone();
    other.id = 2;
    other.mapping.root = other_root;
    other.limbs[0].goal = other_goal;
    s.scene.loom_rigs.push(other);
    for rig in &mut s.scene.loom_rigs {
        rig.limbs[0].contacts = ContactTrack::constant(
            reference(ContactSpace::World, Vec3::new(12., 8., 0.)), 1.,
        ).unwrap();
    }
    s.scene.node_mut(root).unwrap().parent = Some(other_goal);
    s.scene.node_mut(other_root).unwrap().parent = Some(goal);
    s.scene.bump_version();
    assert!(s.scene.contact_control(goal).is_err());
    assert!(s.scene.contact_control(other_goal).is_err());
    assert!(s.scene.world_matrix(goal).is_finite());
    assert!(s.scene.contact_errors().iter().any(|error|error.message.contains("cyclic")));
    s.scene.node_mut(other_root).unwrap().parent = None;
    s.scene.bump_version();
    assert!(s.scene.contact_control(goal).unwrap().is_some());
    assert!(s.scene.contact_control(other_goal).unwrap().is_some());
}

#[test]
fn contact_controller_dependency_cycle_reports_error_without_recursive_world_queries() {
    let (mut s, root, _) = rigged();
    let goal = s.scene.loom_rigs[0].limbs[0].goal;
    s.scene.node_mut(root).unwrap().parent = Some(goal);
    s.scene.loom_rigs[0].limbs[0].contacts =
        ContactTrack::constant(reference(ContactSpace::World, Vec3::new(12., 8., 0.)), 1.).unwrap();
    s.scene.bump_version();
    assert!(s.scene.contact_control(goal).is_err());
    assert!(s.scene.world_matrix(goal).is_finite());
    assert!(s
        .scene
        .contact_errors()
        .iter()
        .any(|error| error.message.contains("ancestor")));
    let before = s.scene.loom_rigs.clone();
    let undo = s.undo.position();
    assert!(execute(
        &mut s,
        Command::SetTransform {
            id: goal.0,
            position: Some([1., 2., 3.]),
            rotation_deg: None,
            scale: None
        }
    )
    .is_err());
    assert_eq!(s.scene.loom_rigs, before);
    assert_eq!(s.undo.position(), undo);
}

#[test]
fn selection_transforms_move_goal_and_pole_in_one_contact_edit_and_respect_locks() {
    let (mut s, _, end) = rigged();
    let limb = s.scene.loom_rigs[0].limbs[0].clone();
    s.scene.loom_rigs[0].limbs[0].contacts = ContactTrack::new(
        [
            (1, reference(ContactSpace::World, Vec3::new(12., 8., 0.))),
            (2, reference(ContactSpace::World, Vec3::new(8., 12., 0.))),
        ]
        .into(),
        vec![key(10., 1, 1.), key(20., 2, 1.)],
    )
    .unwrap();
    s.scene.bump_version();
    execute(&mut s, Command::SetFrame { frame: 15 }).unwrap();
    let goal = s.scene.world_matrix(limb.goal).w_axis.truncate();
    let pole = s.scene.world_matrix(limb.pole).w_axis.truncate();
    let nodes: Vec<_> = s.scene.nodes().cloned().collect();
    let rigs = s.scene.loom_rigs.clone();
    let undo = s.undo.position();
    s.selection = [limb.goal, limb.pole].into();
    let delta = Vec3::new(0.5, -1., 2.);
    execute(
        &mut s,
        Command::MoveSelection {
            delta: delta.to_array(),
            system: Some(loom_commands::CoordSystem::World),
        },
    )
    .unwrap();
    assert_eq!(s.undo.position(), undo + 1);
    assert!(
        s.scene
            .world_matrix(limb.goal)
            .w_axis
            .truncate()
            .distance(goal + delta)
            < 0.001
    );
    assert!(
        s.scene
            .world_matrix(limb.pole)
            .w_axis
            .truncate()
            .distance(pole + delta)
            < 0.001
    );
    assert!(
        s.scene
            .world_matrix(end)
            .w_axis
            .truncate()
            .distance(goal + delta)
            < 0.001
    );
    assert_eq!(s.scene.nodes().cloned().collect::<Vec<_>>(), nodes);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.loom_rigs, rigs);
    s.selection = [limb.goal].into();
    s.scene.node_mut(limb.goal).unwrap().locks.move_axes[0] = true;
    execute(
        &mut s,
        Command::MoveSelection {
            delta: [2., 1., 0.],
            system: Some(loom_commands::CoordSystem::World),
        },
    )
    .unwrap();
    assert!(
        s.scene
            .world_matrix(limb.goal)
            .w_axis
            .truncate()
            .distance(goal + Vec3::Y)
            < 0.001
    );
}

#[test]
fn transform_type_in_reads_and_edits_the_effective_contact_rotation() {
    let (mut s, _, _) = rigged();
    let limb = s.scene.loom_rigs[0].limbs[0].clone();
    s.scene.loom_rigs[0].limbs[0].contacts =
        ContactTrack::constant(reference(ContactSpace::World, Vec3::new(12., 8., 0.)), 1.).unwrap();
    s.scene.bump_version();
    s.selection = [limb.goal].into();
    s.active_tool = loom_commands::Tool::Rotate;
    s.coord_system = loom_commands::CoordSystem::World;
    assert!((loom_commands::type_in_values(&s).x - 0.3_f32.to_degrees()).abs() < 0.001);
    let before = s.scene.loom_rigs.clone();
    let node = s.scene.node(limb.goal).unwrap().clone();
    let undo = s.undo.position();
    execute(
        &mut s,
        Command::TransformTypeIn {
            absolute: true,
            values: [15., 25., 35.],
        },
    )
    .unwrap();
    assert!(loom_commands::type_in_values(&s).abs_diff_eq(Vec3::new(15., 25., 35.), 0.001));
    assert_eq!(s.undo.position(), undo + 1);
    assert_eq!(s.scene.node(limb.goal).unwrap(), &node);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.loom_rigs, before);
}

#[test]
fn moving_master_and_body_contact_together_does_not_apply_the_motion_twice() {
    let (mut s, _, end) = rigged();
    let rig = s.scene.loom_rigs[0].clone();
    let goal = rig.limbs[0].goal;
    s.scene.loom_rigs[0].limbs[0].contacts =
        ContactTrack::constant(reference(ContactSpace::Body, Vec3::new(12., 8., 0.)), 1.).unwrap();
    s.scene.bump_version();
    let before = s.scene.world_matrix(goal).w_axis.truncate();
    let rigs = s.scene.loom_rigs.clone();
    let node = s.scene.node(rig.master).unwrap().transform.clone();
    let undo = s.undo.position();
    s.selection = [rig.master, goal].into();
    let delta = Vec3::new(1., 2., 0.);
    execute(
        &mut s,
        Command::MoveSelection {
            delta: delta.to_array(),
            system: Some(loom_commands::CoordSystem::World),
        },
    )
    .unwrap();
    assert!(
        s.scene
            .world_matrix(goal)
            .w_axis
            .truncate()
            .distance(before + delta)
            < 0.001
    );
    assert!(
        s.scene
            .world_matrix(end)
            .w_axis
            .truncate()
            .distance(before + delta)
            < 0.001
    );
    assert_eq!(s.undo.position(), undo + 1);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.loom_rigs, rigs);
    assert_eq!(s.scene.node(rig.master).unwrap().transform, node);
}

#[test]
fn world_type_in_rotation_remains_exact_under_a_mirrored_controller_parent() {
    let (mut s, _, _) = rigged();
    let goal = s.scene.loom_rigs[0].limbs[0].goal;
    let mut parent = Node::new("controller parent", ObjectKind::dummy(1.));
    parent.transform.scale.set(Vec3::new(-2., 3., 1.));
    parent
        .transform
        .set_rotation_quat(Quat::from_rotation_z(0.4));
    let parent = s.scene.add_node(parent);
    s.scene.node_mut(goal).unwrap().parent = Some(parent);
    s.scene.loom_rigs[0].limbs[0].contacts =
        ContactTrack::constant(reference(ContactSpace::World, Vec3::new(12., 8., 0.)), 1.).unwrap();
    s.scene.bump_version();
    s.selection = [goal].into();
    s.active_tool = loom_commands::Tool::Rotate;
    s.coord_system = loom_commands::CoordSystem::World;
    execute(
        &mut s,
        Command::TransformTypeIn {
            absolute: true,
            values: [15., 25., 35.],
        },
    )
    .unwrap();
    assert!(loom_commands::type_in_values(&s).abs_diff_eq(Vec3::new(15., 25., 35.), 0.001));
}
