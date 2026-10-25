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
