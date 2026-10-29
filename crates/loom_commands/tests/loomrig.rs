use glam::Vec3;
use loom_commands::{execute, AppState, Command};
use loom_scene::loomrig::{HumanoidMapping, LimbMapping};
use loom_scene::{Node, ObjectKind};
fn setup() -> (AppState, HumanoidMapping) {
    let mut s = AppState::new();
    let character_root = s
        .scene
        .add_node(Node::new("character_root", ObjectKind::bone(1.)));
    let mut ids = vec![];
    for i in 0..3 {
        let mut n = Node::new(&format!("joint{i}"), ObjectKind::bone(10.));
        if i > 0 {
            n.parent = Some(ids[i - 1]);
            n.transform.position.set(Vec3::X * 10.);
        }
        if i == 0 {
            n.parent = Some(character_root);
        }
        ids.push(s.scene.add_node(n));
    }
    let m = HumanoidMapping {
        root: character_root,
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
    (s, m)
}

#[test]
fn applying_mapping_preserves_authored_limb_settings_and_keys() {
    for change_head in [false, true] {
        let (mut state, mut mapping) = setup();
        execute(&mut state, Command::CreateLoomRig { root: mapping.root.0, mapping: Some(mapping.clone()) }).unwrap();
        execute(&mut state, Command::SetLoomRigLimb { rig: 1, limb: "Arm".into(), solver: "hi".into(), blend: 0.4 }).unwrap();
        for (time, value) in [(5., 0.3), (15., 0.8)] {
            execute(&mut state, Command::SetLoomRigBlendKey { rig: 1, limb: "Arm".into(), time, value: Some(value) }).unwrap();
        }
        let before = state.scene.loom_rigs[0].limbs[0].clone();
        let bones: Vec<_> = state.scene.nodes().filter(|n| n.object.is_bone()).cloned().collect();
        if change_head {
            let mut head = Node::new("new head role", ObjectKind::bone(1.));
            head.parent = Some(mapping.root);
            mapping.head = state.scene.add_node(head);
        }
        execute(&mut state, Command::SetLoomRigMapping { rig: 1, mapping }).unwrap();
        let after = &state.scene.loom_rigs[0].limbs[0];
        assert_eq!(after.solver, before.solver, "mapping apply reset the authored solver");
        assert_eq!(after.blend, before.blend, "mapping apply discarded native IK/FK keys");
        assert_eq!(after.pose_offsets, before.pose_offsets, "mapping apply discarded matched pose offsets");
        assert_eq!((after.goal, after.pole), (before.goal, before.pole));
        for bone in bones { assert_eq!(state.scene.node(bone.id).unwrap(), &bone); }
    }
}

#[test]
fn rig_is_undoable_and_controls_drive_original_chain() {
    let (mut s, m) = setup();
    let end = m.head;
    let original = s.scene.node(end).unwrap().clone();
    execute(
        &mut s,
        Command::CreateLoomRig {
            root: m.root.0,
            mapping: Some(m),
        },
    )
    .unwrap();
    assert_eq!(s.scene.loom_rigs.len(), 1);
    let goal = s.scene.loom_rigs[0].limbs[0].goal;
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
    s.scene
        .node_mut(goal)
        .unwrap()
        .transform
        .position
        .set(Vec3::new(10., 10., 0.));
    let endpos = s.scene.world_matrix(end).transform_point3(Vec3::ZERO);
    assert!(
        (endpos - Vec3::new(10., 10., 0.)).length() < 0.01,
        "{endpos:?}"
    );
    assert_eq!(s.scene.node(end).unwrap().transform, original.transform);
    let saved = serde_json::to_string(&s.scene).unwrap();
    let back: loom_scene::Scene = serde_json::from_str(&saved).unwrap();
    assert_eq!(back.loom_rigs.len(), 1);
    execute(&mut s, Command::RemoveLoomRig { rig: 1 }).unwrap();
    assert!(s.scene.loom_rigs.is_empty());
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.loom_rigs.len(), 1);
}
#[test]
fn switching_solver_keeps_current_visible_pose() {
    let (mut s, m) = setup();
    let ids = [m.root, m.limbs[0].middle, m.head];
    execute(
        &mut s,
        Command::CreateLoomRig {
            root: m.root.0,
            mapping: Some(m),
        },
    )
    .unwrap();
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
    let g = s.scene.loom_rigs[0].limbs[0].goal;
    s.scene
        .node_mut(g)
        .unwrap()
        .transform
        .position
        .set(Vec3::new(12., 8., 0.));
    let before: Vec<_> = ids.iter().map(|id| s.scene.world_matrix(*id)).collect();
    execute(
        &mut s,
        Command::SetLoomRigLimb {
            rig: 1,
            limb: "Arm".into(),
            solver: "hi".into(),
            blend: 0.,
        },
    )
    .unwrap();
    for (i, id) in ids.iter().enumerate() {
        let after = s.scene.world_matrix(*id);
        assert!(before[i].abs_diff_eq(after, 0.001));
    }
}
#[test]
fn master_moves_in_world_under_rotated_scaled_parent_and_fk_control_is_not_doubled() {
    let (mut s, m) = setup();
    let root = m.root;
    let mut parent = Node::new("SceneParent", ObjectKind::dummy(1.));
    parent.transform.position.set(Vec3::new(7., -3., 11.));
    parent
        .transform
        .set_rotation_quat(glam::Quat::from_rotation_z(0.7));
    parent.transform.scale.set(Vec3::splat(2.));
    let p = s.scene.add_node(parent);
    s.scene.node_mut(root).unwrap().parent = Some(p);
    let original = s.scene.world_matrix(root);
    execute(
        &mut s,
        Command::CreateLoomRig {
            root: root.0,
            mapping: Some(m),
        },
    )
    .unwrap();
    assert!(s.scene.world_matrix(root).abs_diff_eq(original, 1e-4));
    let master = s.scene.loom_rigs[0].master;
    let mut t = s.scene.node(master).unwrap().transform.clone();
    t.position.set(t.position.get() + Vec3::Y * 5.);
    s.scene.node_mut(master).unwrap().transform = t;
    let moved = s.scene.world_matrix(root);
    assert!(
        (moved.transform_point3(Vec3::ZERO) - original.transform_point3(Vec3::ZERO) - Vec3::Y * 5.)
            .length()
            < 1e-4
    );
    let before_rotation = s.scene.world_matrix(root);
    let master_before = s.scene.node(master).unwrap().transform.matrix();
    let mut rotated = s.scene.node(master).unwrap().transform.clone();
    rotated.set_rotation_quat(glam::Quat::from_rotation_x(0.4) * rotated.rotation_quat());
    let expected = rotated.matrix() * master_before.inverse() * before_rotation;
    s.scene.node_mut(master).unwrap().transform = rotated;
    assert!(s.scene.world_matrix(root).abs_diff_eq(expected, 1e-4));
    let binding = s.scene.loom_rigs[0]
        .bindings
        .iter()
        .find(|b| b.bone != root)
        .unwrap()
        .clone();
    s.scene
        .node_mut(binding.control)
        .unwrap()
        .transform
        .position
        .set(Vec3::Z * 2.);
    assert!(s
        .scene
        .world_matrix(binding.control)
        .abs_diff_eq(s.scene.world_matrix(binding.bone), 1e-4));
}
#[test]
fn ik_goal_rotates_end_joint_and_pole_moves_bend() {
    let (mut s, m) = setup();
    let end = m.head;
    let middle = m.limbs[0].middle;
    execute(
        &mut s,
        Command::CreateLoomRig {
            root: m.root.0,
            mapping: Some(m),
        },
    )
    .unwrap();
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
    let limb = s.scene.loom_rigs[0].limbs[0].clone();
    s.scene
        .node_mut(limb.goal)
        .unwrap()
        .transform
        .position
        .set(Vec3::new(10., 10., 0.));
    let q = glam::Quat::from_rotation_x(0.5);
    s.scene
        .node_mut(limb.goal)
        .unwrap()
        .transform
        .set_rotation_quat(q);
    let world = s.scene.world_matrix(end);
    assert!(world
        .to_scale_rotation_translation()
        .1
        .abs_diff_eq(q, 0.001));
    let before = s.scene.world_matrix(middle).transform_point3(Vec3::ZERO);
    s.scene
        .node_mut(limb.pole)
        .unwrap()
        .transform
        .position
        .set(Vec3::new(0., 0., 30.));
    let after = s.scene.world_matrix(middle).transform_point3(Vec3::ZERO);
    assert!(before.distance(after) > 1.);
    assert!(
        s.scene
            .world_matrix(end)
            .transform_point3(Vec3::ZERO)
            .distance(Vec3::new(10., 10., 0.))
            < 0.01
    );
}
#[test]
fn neutral_controls_preserve_original_animated_fk_at_all_frames() {
    use loom_scene::anim::{Interp, TangentType};
    let (mut s, m) = setup();
    let root = m.root;
    let end = m.head;
    let keys = s
        .scene
        .node_mut(root)
        .unwrap()
        .transform
        .position
        .ensure_keys()
        .unwrap();
    for tr in &mut keys.tracks {
        tr.interp = Interp::Linear;
    }
    for (t, p) in [(0., Vec3::ZERO), (10., Vec3::new(4., 7., 2.))] {
        for (i, v) in p.to_array().into_iter().enumerate() {
            keys.tracks[i].set_key(t, v, (TangentType::Linear, TangentType::Linear));
        }
    }
    let original = s.scene.node(root).unwrap().transform.clone();
    let samples: Vec<_> = [0., 2.5, 6., 10.]
        .into_iter()
        .map(|t| s.scene.at_time(t, || s.scene.world_matrix(end)))
        .collect();
    execute(
        &mut s,
        Command::CreateLoomRig {
            root: root.0,
            mapping: Some(m),
        },
    )
    .unwrap();
    for (t, expected) in [0., 2.5, 6., 10.].into_iter().zip(samples) {
        assert!(
            s.scene
                .at_time(t, || s.scene.world_matrix(end))
                .abs_diff_eq(expected, 1e-4),
            "frame {t}"
        );
    }
    assert_eq!(s.scene.node(root).unwrap().transform, original);
}
#[test]
fn ik_respects_rotated_scaled_skeleton_parent() {
    let (mut s, m) = setup();
    let end = m.head;
    let mut p = Node::new("parent", ObjectKind::dummy(1.));
    p.transform.position.set(Vec3::new(4., 9., 2.));
    p.transform
        .set_rotation_quat(glam::Quat::from_rotation_z(0.5));
    p.transform.scale.set(Vec3::splat(2.));
    let parent = s.scene.add_node(p);
    s.scene.node_mut(m.root).unwrap().parent = Some(parent);
    execute(
        &mut s,
        Command::CreateLoomRig {
            root: m.root.0,
            mapping: Some(m),
        },
    )
    .unwrap();
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
    let g = s.scene.loom_rigs[0].limbs[0].goal;
    let target = s
        .scene
        .world_matrix(parent)
        .transform_point3(Vec3::new(10., 10., 0.));
    s.scene.node_mut(g).unwrap().transform.position.set(target);
    let actual = s.scene.world_matrix(end).transform_point3(Vec3::ZERO);
    assert!(actual.distance(target) < 0.01, "{actual:?} != {target:?}");
}

#[test]
fn mapping_edits_preserve_authored_controls_and_keys() {
    let (mut s, m) = setup();
    execute(
        &mut s,
        Command::CreateLoomRig {
            root: m.root.0,
            mapping: Some(m.clone()),
        },
    )
    .unwrap();
    let goal = s.scene.loom_rigs[0].limbs[0].goal;
    let keys = s
        .scene
        .node_mut(goal)
        .unwrap()
        .transform
        .position
        .ensure_keys()
        .unwrap();
    keys.tracks[0].set_key(
        4.,
        17.,
        (
            loom_scene::anim::TangentType::Linear,
            loom_scene::anim::TangentType::Linear,
        ),
    );
    let before = s.scene.node(goal).unwrap().clone();
    let bindings = s.scene.loom_rigs[0].bindings.clone();
    execute(&mut s, Command::SetLoomRigMapping { rig: 1, mapping: m }).unwrap();
    assert_eq!(s.scene.loom_rigs[0].limbs[0].goal, goal);
    assert_eq!(s.scene.node(goal).unwrap().transform, before.transform);
    assert_eq!(s.scene.loom_rigs[0].bindings, bindings);
}
#[test]
fn ambiguous_role_mappings_are_rejected_atomically() {
    let (mut s, m) = setup();
    let before = serde_json::to_value(&s.scene).unwrap();
    let mut duplicated = m.clone();
    duplicated.limbs.push(duplicated.limbs[0].clone());
    assert!(execute(
        &mut s,
        Command::CreateLoomRig {
            root: m.root.0,
            mapping: Some(duplicated)
        }
    )
    .is_err());
    let mut same_role = m.clone();
    same_role.pelvis = same_role.root;
    assert!(execute(
        &mut s,
        Command::CreateLoomRig {
            root: m.root.0,
            mapping: Some(same_role)
        }
    )
    .is_err());
    let mut shared = m.clone();
    let mut second = shared.limbs[0].clone();
    second.name = "Other Arm".into();
    shared.limbs.push(second);
    assert!(execute(
        &mut s,
        Command::CreateLoomRig {
            root: m.root.0,
            mapping: Some(shared)
        }
    )
    .is_err());
    assert_eq!(serde_json::to_value(&s.scene).unwrap(), before);
}
#[test]
fn autokey_blend_and_pose_match_offsets_preserve_prior_frames_and_source_keys() {
    let (mut s, m) = setup();
    let root = m.root;
    let originals: Vec<_> = s
        .scene
        .nodes()
        .map(|n| (n.id, n.transform.clone()))
        .collect();
    execute(
        &mut s,
        Command::CreateLoomRig {
            root: root.0,
            mapping: Some(m),
        },
    )
    .unwrap();
    let original0 = s.scene.world_matrix(root);
    s.current_frame = 20;
    s.auto_key = true;
    s.default_tangent = (
        loom_scene::anim::TangentType::Linear,
        loom_scene::anim::TangentType::Linear,
    );
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
    let p = &s.scene.loom_rigs[0].limbs[0].blend;
    assert!(p.is_animated());
    assert!((s.scene.at_time(0., || p.get()) - 0.).abs() < 1e-5);
    assert!((s.scene.at_time(10., || p.get()) - 0.5).abs() < 1e-5);
    assert!((s.scene.at_time(20., || p.get()) - 1.).abs() < 1e-5);
    assert!(s
        .scene
        .at_time(0., || s.scene.world_matrix(root))
        .abs_diff_eq(original0, 1e-4));
    for (id, transform) in originals {
        assert_eq!(s.scene.node(id).unwrap().transform, transform);
    }
}
#[test]
fn matched_solver_switch_keeps_later_goals_constrained() {
    let (mut s, m) = setup();
    let end = m.head;
    execute(
        &mut s,
        Command::CreateLoomRig {
            root: m.root.0,
            mapping: Some(m),
        },
    )
    .unwrap();
    execute(
        &mut s,
        Command::SetLoomRigLimb {
            rig: 1,
            limb: "Arm".into(),
            solver: "hi".into(),
            blend: 1.,
        },
    )
    .unwrap();
    let goal = s.scene.loom_rigs[0].limbs[0].goal;
    s.scene
        .node_mut(goal)
        .unwrap()
        .transform
        .position
        .set(Vec3::new(12., 7., 3.));
    let before = s.scene.world_matrix(end);
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
    assert!(s.scene.world_matrix(end).abs_diff_eq(before, 0.001));
    let target = Vec3::new(8., 11., -2.);
    s.scene
        .node_mut(goal)
        .unwrap()
        .transform
        .position
        .set(target);
    assert!(
        s.scene
            .world_matrix(end)
            .transform_point3(Vec3::ZERO)
            .distance(target)
            < 0.01
    );
}
#[test]
fn keyed_full_ik_goal_stays_planted_while_native_root_motion_continues() {
    use loom_scene::anim::{Interp, TangentType};
    let (mut s, m) = setup();
    let end = m.head;
    s.scene
        .node_mut(m.limbs[0].start)
        .unwrap()
        .transform
        .set_rotation_quat(glam::Quat::from_rotation_z(0.4));
    s.scene
        .node_mut(m.limbs[0].middle)
        .unwrap()
        .transform
        .set_rotation_quat(glam::Quat::from_rotation_z(-0.8));
    let keys = s
        .scene
        .node_mut(m.root)
        .unwrap()
        .transform
        .position
        .ensure_keys()
        .unwrap();
    keys.tracks[1].interp = Interp::Linear;
    keys.tracks[1].set_key(0., 0., (TangentType::Linear, TangentType::Linear));
    keys.tracks[1].set_key(40., 4., (TangentType::Linear, TangentType::Linear));
    let original = s.scene.node(m.root).unwrap().transform.clone();
    execute(
        &mut s,
        Command::CreateLoomRig {
            root: m.root.0,
            mapping: Some(m.clone()),
        },
    )
    .unwrap();
    s.current_frame = 20;
    s.auto_key = true;
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
    let planted = s.scene.world_matrix(end).transform_point3(Vec3::ZERO);
    let future = s.scene.at_time(30., || {
        s.scene.world_matrix(end).transform_point3(Vec3::ZERO)
    });
    assert!(planted.distance(future) < 0.01, "{planted:?} -> {future:?}");
    assert_eq!(s.scene.node(m.root).unwrap().transform, original);
}
#[test]
fn hi_pole_uses_final_solved_bend_plane() {
    let (mut s, m) = setup();
    let middle = m.limbs[0].middle;
    let start = m.limbs[0].start;
    let end = m.head;
    execute(
        &mut s,
        Command::CreateLoomRig {
            root: m.root.0,
            mapping: Some(m),
        },
    )
    .unwrap();
    execute(
        &mut s,
        Command::SetLoomRigLimb {
            rig: 1,
            limb: "Arm".into(),
            solver: "hi".into(),
            blend: 1.,
        },
    )
    .unwrap();
    let l = s.scene.loom_rigs[0].limbs[0].clone();
    let target = Vec3::new(8., 11., 4.);
    let pole = Vec3::new(-4., 3., 25.);
    s.scene
        .node_mut(l.goal)
        .unwrap()
        .transform
        .position
        .set(target);
    s.scene
        .node_mut(l.pole)
        .unwrap()
        .transform
        .position
        .set(pole);
    let root = s.scene.world_matrix(start).transform_point3(Vec3::ZERO);
    let mid = s.scene.world_matrix(middle).transform_point3(Vec3::ZERO);
    let axis = (target - root).normalize();
    let actual = (mid - root - axis * (mid - root).dot(axis)).normalize();
    let expected = (pole - root - axis * (pole - root).dot(axis)).normalize();
    assert!(
        actual.dot(expected) > 0.9999,
        "actual {actual:?}, expected {expected:?}, dot {}",
        actual.dot(expected)
    );
    let actual_end=s.scene.world_matrix(end).transform_point3(Vec3::ZERO);
    assert!(actual_end.distance(target)<0.001,"end {actual_end:?}, target {target:?}, distance {}",actual_end.distance(target));
}
#[test]
fn saved_rig_evaluates_same_pose_without_needing_an_edit() {
    let (mut s, m) = setup();
    let end = m.head;
    execute(
        &mut s,
        Command::CreateLoomRig {
            root: m.root.0,
            mapping: Some(m),
        },
    )
    .unwrap();
    let master = s.scene.loom_rigs[0].master;
    s.scene
        .node_mut(master)
        .unwrap()
        .transform
        .position
        .set(Vec3::new(2., 3., 4.));
    let expected = s.scene.world_matrix(end);
    let json = serde_json::to_string(&s.scene).unwrap();
    let loaded: loom_scene::Scene = serde_json::from_str(&json).unwrap();
    assert!(loaded.world_matrix(end).abs_diff_eq(expected, 1e-4));
}
