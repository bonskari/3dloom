//! Morpher (channels, progressive targets, limits) and Skin Morph (bone-angle driven).

use glam::Vec3;
use loom_scene::morph::{MorphChannel, MorphTarget, MorpherData, SkinMorphData, SkinMorphTarget};
use loom_scene::{Modifier, ModifierSlot, Node, ObjectKind, Param, Scene};

#[test]
fn morpher_blends_progressive_targets_by_value() {
    let mut s = Scene::new();
    let base = loom_mesh::box_segmented(10.0, 10.0, 10.0, [1, 1, 1]);
    let n = base.vertex_count();
    let id = s.add_node(Node::new("Head", ObjectKind::EditablePoly(loom_scene::PolyObject::from_mesh(base))));
    let mut data = MorpherData::default();
    let half = MorphTarget { node: None, offsets: vec![Vec3::Z * 1.0; n], percent: 50.0 };
    let full = MorphTarget { node: None, offsets: vec![Vec3::Z * 4.0; n], percent: 100.0 };
    *data.channel(0) = MorphChannel { name: "up".into(), targets: vec![half, full], value: Param::new(0.0), active: true, use_limits: true, min: 0.0, max: 100.0, tension: 0.5, valid: true };
    s.node_mut(id).unwrap().modifiers.push(ModifierSlot::new(Modifier::Morpher(Box::new(data))));
    let z_at = |s: &mut Scene, v: f32| {
        if let Modifier::Morpher(d) = &mut s.node_mut(id).unwrap().modifiers[0].modifier {
            d.channel(0).value.set(v);
        }
        s.eval_mesh(id).value.mesh.vertices()[0].position.z
    };
    let z0 = z_at(&mut s, 0.0);
    assert!((z_at(&mut s, 50.0) - z0 - 1.0).abs() < 1e-4, "at the 50 % target");
    assert!((z_at(&mut s, 75.0) - z0 - 2.5).abs() < 1e-4, "halfway between the targets");
    assert!((z_at(&mut s, 100.0) - z0 - 4.0).abs() < 1e-4);
    assert!((z_at(&mut s, 150.0) - z0 - 4.0).abs() < 1e-4, "limits clamp");
    assert!((z_at(&mut s, 25.0) - z0 - 0.5).abs() < 1e-4, "below the first target it scales");
}

#[test]
fn morpher_skips_a_target_with_a_different_vertex_count() {
    let mut s = Scene::new();
    let base = loom_mesh::box_segmented(10.0, 10.0, 10.0, [1, 1, 1]);
    let id = s.add_node(Node::new("Head", ObjectKind::EditablePoly(loom_scene::PolyObject::from_mesh(base))));
    let target = s.add_node(Node::new("Target", ObjectKind::EditablePoly(loom_scene::PolyObject::from_mesh(loom_mesh::box_segmented(10.0, 10.0, 10.0, [2, 1, 1])))));
    let mut data = MorpherData::default();
    data.pick_target(&s, id, None, 0, target);
    assert!(!data.channel(0).valid);
    assert_eq!(data.channel(0).name, "Target");
    data.channel(0).value.set(100.0);
    s.node_mut(id).unwrap().modifiers.push(ModifierSlot::new(Modifier::Morpher(Box::new(data))));
    let e = s.eval_mesh(id);
    assert_eq!(e.value.mesh.vertex_count(), 8);
    assert!(e.warnings.iter().any(|w| w.contains("wrong vertex count")), "{:?}", e.warnings);
    // A matching target picked from a moved copy morphs toward it.
    let mut moved = Node::new("Up", ObjectKind::EditablePoly(loom_scene::PolyObject::from_mesh(loom_mesh::box_segmented(10.0, 10.0, 10.0, [1, 1, 1]))));
    moved.transform.position.set(Vec3::new(50.0, 0.0, 0.0));
    let up = s.add_node(moved);
    let mut d2 = match &s.node(id).unwrap().modifiers[0].modifier {
        Modifier::Morpher(d) => (**d).clone(),
        _ => unreachable!(),
    };
    d2.pick_target(&s, id, None, 1, up);
    assert!(d2.channel(1).valid);
    d2.channel(1).value.set(50.0);
    s.node_mut(id).unwrap().modifiers[0].modifier = Modifier::Morpher(Box::new(d2));
    let p = s.eval_mesh(id).value.mesh.vertices()[0].position;
    assert!((p.x - (-5.0)).abs() < 1e-3, "the target is captured in object space (no node offset): {p:?}");
}

#[test]
fn skin_morph_fires_at_the_bone_angle() {
    let mut s = Scene::new();
    let mesh = loom_mesh::cylinder_segmented(2.0, 20.0, 8, 1, 12, loom_mesh::Slice::default());
    let mut poly = Node::new("Arm", ObjectKind::EditablePoly(loom_scene::PolyObject::from_mesh(mesh)));
    poly.transform.rotation.set(Vec3::new(0.0, 90.0, 0.0));
    let arm = s.add_node(poly);
    let b0 = s.add_node(Node::new("Bone001", ObjectKind::bone(10.0)));
    let mut b1n = Node::new("Bone002", ObjectKind::bone(10.0));
    b1n.transform.position.set(Vec3::new(10.0, 0.0, 0.0));
    b1n.parent = Some(b0);
    let b1 = s.add_node(b1n);
    let mut data = SkinMorphData::bind(&s, &[b0, b1]);
    data.morphs.push(SkinMorphTarget { bone: 1, name: "elbow".into(), angle_deg: 90.0, influence_deg: 90.0, falloff: 0, enabled: true, deltas: vec![(0, Vec3::Y * 3.0)], ..Default::default() });
    s.node_mut(arm).unwrap().modifiers.push(ModifierSlot::new(Modifier::SkinMorph(Box::new(data))));
    let rest = s.eval_mesh(arm).value.mesh.vertices()[0].position;
    // Half of the morph halfway to half the influence angle away (22.5° of 45°).
    s.node_mut(b1).unwrap().transform.rotation.set(Vec3::new(0.0, 0.0, 67.5));
    let half = s.eval_mesh(arm).value.mesh.vertices()[0].position;
    s.node_mut(b1).unwrap().transform.rotation.set(Vec3::new(0.0, 0.0, 90.0));
    let full = s.eval_mesh(arm).value.mesh.vertices()[0].position;
    assert!(((full - rest).length() - 3.0).abs() < 0.1, "{:?}", full - rest);
    assert!(((half - rest).length() - 1.5).abs() < 0.1, "{:?}", half - rest);
}
