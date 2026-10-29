use glam::Vec3;
use loom_scene::{Node, ObjectKind, Scene};
#[test]
fn characterization_requires_real_mapped_bones() {
    let mut scene = Scene::new();
    let root = scene.add_node(Node::new("root", ObjectKind::bone(1.0)));
    assert!(loom_scene::loomrig::HumanoidMapping::metahuman(&scene, root).is_err());
}
#[test]
fn control_offsets_drive_existing_bone_without_editing_transform() {
    let mut scene = Scene::new();
    let root = scene.add_node(Node::new("root", ObjectKind::bone(1.0)));
    let control = scene.add_node(Node::new("master", ObjectKind::dummy(1.0)));
    scene.loom_rigs.push(loom_scene::loomrig::LoomRig {
        id: 1,
        name: "Rig".into(),
        mapping: loom_scene::loomrig::HumanoidMapping {
            root,
            pelvis: root,
            spine: vec![],
            head: root,
            limbs: vec![],
        },
        master: control,
        bindings: vec![loom_scene::loomrig::ControlBinding {
            bone: root,
            control,
            rest: glam::Mat4::IDENTITY,
        }],
        limbs: vec![],
        pose_frames: Default::default(),
    });
    scene
        .node_mut(control)
        .unwrap()
        .transform
        .position
        .set(Vec3::Y * 3.0);
    assert!(
        (scene.world_matrix(root).transform_point3(Vec3::ZERO) - Vec3::Y * 3.0).length() < 1e-4
    );
    assert_eq!(
        scene.node(root).unwrap().transform.position.get(),
        Vec3::ZERO
    );
}
#[test]
fn characterization_checks_hierarchy_instead_of_accepting_unrelated_named_bones() {
    use loom_scene::loomrig::{HumanoidMapping, LimbMapping};
    let mut s = Scene::new();
    let root = s.add_node(Node::new("root", ObjectKind::bone(1.)));
    let a = s.add_node(Node::new("a", ObjectKind::bone(1.)));
    let b = s.add_node(Node::new("b", ObjectKind::bone(1.)));
    let m = HumanoidMapping {
        root,
        pelvis: root,
        spine: vec![],
        head: root,
        limbs: vec![LimbMapping {
            name: "Arm".into(),
            start: root,
            middle: a,
            end: b,
        }],
    };
    assert!(m.validate(&s).is_err());
}
#[test]
fn edit_revision_changes_on_edit_but_not_playback() {
    let mut scene = Scene::new();
    let initial = scene.revision();
    scene.set_time(35.);
    assert_eq!(scene.revision(), initial);
    let id = scene.add_node(Node::new("bone", ObjectKind::bone(1.)));
    assert!(scene.revision() > initial);
    let previous = scene.revision();
    scene.node_mut(id).unwrap().transform.position.set(Vec3::X);
    assert!(scene.revision() > previous);
    let edited = scene.revision();
    scene.at_time(77., || scene.world_matrix(id));
    assert_eq!(scene.revision(), edited);
}

#[test]
fn pose_frames_are_sorted_unique_validated_and_persisted() {
    use loom_scene::loomrig_pose::RigPoseFrames;
    let mut frames=RigPoseFrames::default();
    frames.insert(10.).unwrap();frames.insert(0.).unwrap();frames.insert(10.).unwrap();frames.insert(-0.).unwrap();
    assert_eq!(frames.times(),&[0.,10.]);
    assert!(frames.insert(f32::NAN).is_err());
    assert!(frames.insert(0.00001).is_err());
    assert!(frames.move_key(10.,0.00001).is_err());
    assert!(frames.move_key(0.,10.).is_err());
    assert_eq!(frames.times(),&[0.,10.]);
    frames.move_key(0.,5.).unwrap();
    let encoded=serde_json::to_string(&frames).unwrap();
    assert_eq!(serde_json::from_str::<RigPoseFrames>(&encoded).unwrap(),frames);
    for invalid in ["[10,0]","[0,0]","[0,-0.0]","[0,0.00001]"] {assert!(serde_json::from_str::<RigPoseFrames>(invalid).is_err());}
    frames.remove(5.).unwrap();assert_eq!(frames.times(),&[10.]);
    assert!(frames.remove(5.).is_err());
}
