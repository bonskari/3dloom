use loom_commands::{execute, AppState, Command};
use loom_scene::{
    loomrig::{HumanoidMapping, LimbMapping},
    Node, ObjectKind,
};
fn rig() -> AppState {
    let mut s = AppState::new();
    let root = s.scene.add_node(Node::new("root", ObjectKind::bone(1.)));
    let mut ids = vec![];
    for i in 0..3 {
        let mut n = Node::new(&format!("joint{i}"), ObjectKind::bone(10.));
        n.parent = Some(if i == 0 { root } else { ids[i - 1] });
        n.transform.position.set(glam::Vec3::X * 10.);
        ids.push(s.scene.add_node(n));
    }
    execute(
        &mut s,
        Command::CreateLoomRig {
            root: root.0,
            mapping: Some(HumanoidMapping {
                root,
                pelvis: ids[0],
                head: ids[2],
                spine: vec![],
                limbs: vec![LimbMapping {
                    name: "Arm".into(),
                    start: ids[0],
                    middle: ids[1],
                    end: ids[2],
                }],
            }),
        },
    )
    .unwrap();
    s
}
#[test]
fn explicit_blend_keys_edit_move_delete_and_undo_without_changing_source_bones() {
    let mut s = rig();
    let bones: Vec<_> = s
        .scene
        .nodes()
        .filter(|n| n.object.is_bone())
        .map(|n| serde_json::to_value(n).unwrap())
        .collect();
    execute(
        &mut s,
        Command::SetLoomRigBlendKey {
            rig: 1,
            limb: "Arm".into(),
            time: 0.,
            value: Some(0.),
        },
    )
    .unwrap();
    execute(
        &mut s,
        Command::SetLoomRigBlendKey {
            rig: 1,
            limb: "Arm".into(),
            time: 20.,
            value: Some(1.),
        },
    )
    .unwrap();
    assert!((s.scene.loom_rigs[0].limbs[0].blend.get_at(10.) - 0.5).abs() < 1e-6);
    execute(
        &mut s,
        Command::MoveLoomRigBlendKey {
            rig: 1,
            limb: "Arm".into(),
            from: 20.,
            to: 30.5,
        },
    )
    .unwrap();
    assert_eq!(
        s.scene.loom_rigs[0].limbs[0].blend.keys().unwrap().tracks[0].keys[1].time,
        30.5
    );
    execute(
        &mut s,
        Command::SetLoomRigBlendKey {
            rig: 1,
            limb: "Arm".into(),
            time: 30.5,
            value: None,
        },
    )
    .unwrap();
    assert_eq!(
        s.scene.loom_rigs[0].limbs[0].blend.keys().unwrap().tracks[0]
            .keys
            .len(),
        1
    );
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(
        s.scene.loom_rigs[0].limbs[0].blend.keys().unwrap().tracks[0].keys[1].time,
        20.
    );
    assert_eq!(
        bones,
        s.scene
            .nodes()
            .filter(|n| n.object.is_bone())
            .map(|n| serde_json::to_value(n).unwrap())
            .collect::<Vec<_>>()
    );
}
#[test]
fn invalid_key_moves_are_atomic_and_never_overwrite_another_transition() {
    let mut s = rig();
    for time in [0., 20.] {
        execute(
            &mut s,
            Command::SetLoomRigBlendKey {
                rig: 1,
                limb: "Arm".into(),
                time,
                value: Some(0.5),
            },
        )
        .unwrap();
    }
    let before = serde_json::to_value(&s.scene).unwrap();
    let steps = s.undo.len();
    for command in [
        Command::SetLoomRigBlendKey {
            rig: 1,
            limb: "Arm".into(),
            time: 10.,
            value: Some(2.),
        },
        Command::MoveLoomRigBlendKey {
            rig: 1,
            limb: "Arm".into(),
            from: 20.,
            to: 0.,
        },
    ] {
        assert!(execute(&mut s, command).is_err());
        assert_eq!(serde_json::to_value(&s.scene).unwrap(), before);
        assert_eq!(s.undo.len(), steps);
    }
}
