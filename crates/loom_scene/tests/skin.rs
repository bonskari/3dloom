//! The Skin modifier: envelopes, linear blend skinning, topology changes below.

use glam::Vec3;
use loom_scene::skin::{envelope_weight, Envelope, SkinData};
use loom_scene::{Modifier, ModifierSlot, Node, NodeId, ObjectKind, Scene};

fn arm_mesh() -> loom_mesh::Mesh {
    loom_mesh::cylinder_segmented(2.0, 20.0, 8, 1, 12, loom_mesh::Slice::default())
}

/// A 20-unit cylinder along X skinned to two 10-unit bones.
fn rig(s: &mut Scene) -> (NodeId, NodeId, NodeId) {
    let mut poly = Node::new("Arm", ObjectKind::EditablePoly(loom_scene::PolyObject::from_mesh(arm_mesh())));
    poly.transform.rotation.set(Vec3::new(0.0, 90.0, 0.0)); // cylinder height along Z → along X
    let arm = s.add_node(poly);
    let b0 = s.add_node(Node::new("Bone001", ObjectKind::bone(10.0)));
    let mut b1n = Node::new("Bone002", ObjectKind::bone(10.0));
    b1n.transform.position.set(Vec3::new(10.0, 0.0, 0.0));
    b1n.parent = Some(b0);
    let b1 = s.add_node(b1n);
    (arm, b0, b1)
}

#[test]
fn envelope_weights_fall_off_between_inner_and_outer() {
    let e = Envelope { start: Vec3::ZERO, end: Vec3::X * 10.0, inner: [1.0, 1.0], outer: [3.0, 3.0], falloff: 0, abs_effect: 1.0, squash: 1.0, ..Default::default() };
    assert!((envelope_weight(&e, Vec3::new(5.0, 0.5, 0.0)) - 1.0).abs() < 1e-5);
    assert!((envelope_weight(&e, Vec3::new(5.0, 2.0, 0.0)) - 0.5).abs() < 1e-5);
    assert!(envelope_weight(&e, Vec3::new(5.0, 4.0, 0.0)).abs() < 1e-5);
    assert!(envelope_weight(&e, Vec3::new(-5.0, 0.0, 0.0)).abs() < 1e-5, "beyond the capsule end");
}

#[test]
fn skin_follows_a_bone_move_without_editing_the_node() {
    let mut s = Scene::new();
    let (arm, b0, b1) = rig(&mut s);
    let data = SkinData::bind(&s, arm, &[b0, b1]);
    assert_eq!(data.bones.len(), 2);
    s.node_mut(arm).unwrap().modifiers.push(ModifierSlot::new(Modifier::Skin(Box::new(data))));
    let rest = s.eval_mesh(arm).value.mesh.clone();
    let w = s.object_matrix(arm);
    // Bend the second bone 90° about Z: vertices past x = 10 swing up.
    s.node_mut(b1).unwrap().transform.rotation.set(Vec3::new(0.0, 0.0, 90.0));
    let bent = s.eval_mesh(arm).value.mesh.clone();
    let far: Vec<(Vec3, Vec3)> = rest.vertices().iter().zip(bent.vertices()).map(|(a, b)| (w.transform_point3(a.position), w.transform_point3(b.position))).filter(|(a, _)| a.x > 18.0).collect();
    assert!(!far.is_empty());
    for (a, b) in &far {
        assert!(b.y > 7.0, "tip vertex {a:?} moved to {b:?}");
    }
    let near: Vec<_> = rest.vertices().iter().zip(bent.vertices()).filter(|(a, _)| w.transform_point3(a.position).x < 2.0).collect();
    assert!(!near.is_empty());
    assert!(near.iter().all(|(a, b)| (a.position - b.position).length() < 1e-3), "base vertices stay");
    assert!(s.eval_mesh(arm).warnings.is_empty(), "{:?}", s.eval_mesh(arm).warnings);
}

#[test]
fn skin_keeps_weights_by_vertex_when_faces_are_deleted_below() {
    let mut s = Scene::new();
    let (arm, b0, b1) = rig(&mut s);
    let mut data = SkinData::bind(&s, arm, &[b0, b1]);
    data.explicit_all();
    let n = data.weights.len();
    s.node_mut(arm).unwrap().modifiers.push(ModifierSlot::new(Modifier::Skin(Box::new(data))));
    // Delete the top ring of vertices in the base object: fewer vertices below the Skin.
    let top: Vec<loom_mesh::VertexId> = {
        let m = arm_mesh();
        m.vertices().iter().enumerate().filter(|(_, v)| v.position.z > 19.9).map(|(i, _)| loom_mesh::VertexId(i as u32)).collect()
    };
    assert!(!top.is_empty());
    if let ObjectKind::EditablePoly(p) = &mut s.node_mut(arm).unwrap().object {
        let mut m = p.mesh.clone();
        loom_mesh::edit_ops::delete_vertices(&mut m, &top).unwrap();
        *p = p.with_mesh(m);
    }
    let e = s.eval_mesh(arm);
    assert!(e.warnings.iter().any(|w| w.contains("Topology changed below Skin")), "{:?}", e.warnings);
    assert!(e.value.mesh.vertex_count() < n);
    s.node_mut(b1).unwrap().transform.rotation.set(Vec3::new(0.0, 0.0, 90.0));
    let bent = s.eval_mesh(arm).value.mesh.clone();
    let w = s.object_matrix(arm);
    assert!(bent.vertices().iter().any(|v| w.transform_point3(v.position).y > 5.0), "still skinned");
}
