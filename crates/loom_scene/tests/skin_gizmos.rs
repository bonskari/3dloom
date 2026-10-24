//! Skin gizmos (Bulge Angle / Joint Angle) and Mirror Mode weight pasting.

use glam::Vec3;
use loom_scene::skin::{mirror_weights, GizmoKey, GizmoKind, SkinData, SkinGizmo};
use loom_scene::{Modifier, ModifierSlot, Node, NodeId, ObjectKind, Scene};

fn arm(s: &mut Scene) -> (NodeId, NodeId, NodeId) {
    let mesh = loom_mesh::cylinder_segmented(2.0, 20.0, 8, 1, 12, loom_mesh::Slice::default());
    let mut poly = Node::new("Arm", ObjectKind::EditablePoly(loom_scene::PolyObject::from_mesh(mesh)));
    poly.transform.rotation.set(Vec3::new(0.0, 90.0, 0.0));
    let arm = s.add_node(poly);
    let b0 = s.add_node(Node::new("Bone001", ObjectKind::bone(10.0)));
    let mut b1n = Node::new("Bone002", ObjectKind::bone(10.0));
    b1n.transform.position.set(Vec3::new(10.0, 0.0, 0.0));
    b1n.parent = Some(b0);
    let b1 = s.add_node(b1n);
    (arm, b0, b1)
}

#[test]
fn bulge_angle_gizmo_scales_the_joint_at_its_key_angle() {
    let mut s = Scene::new();
    let (arm, b0, b1) = arm(&mut s);
    let mut data = SkinData::bind(&s, arm, &[b0, b1]);
    data.gizmos.push(SkinGizmo { kind: GizmoKind::BulgeAngle, bone: 1, parent_bone: 0, keys: vec![GizmoKey { angle_deg: 0.0, deltas: vec![], bulge: 0.0 }, GizmoKey { angle_deg: 90.0, deltas: vec![], bulge: 1.0 }], influence: 1.0, enabled: true });
    s.node_mut(arm).unwrap().modifiers.push(ModifierSlot::new(Modifier::Skin(Box::new(data))));
    let w = s.object_matrix(arm);
    let straight = s.eval_mesh(arm).value.mesh.clone();
    s.node_mut(b1).unwrap().transform.rotation.set(Vec3::new(0.0, 0.0, 90.0));
    let bent = s.eval_mesh(arm).value.mesh.clone();
    // Vertices shared by both bones (around x ≈ 10 at rest) sit farther from the child bone's axis when bent.
    let joint: Vec<usize> = straight.vertices().iter().enumerate().filter(|(_, v)| (w.transform_point3(v.position).x - 10.0).abs() < 2.6).map(|(i, _)| i).collect();
    assert!(!joint.is_empty());
    let b1w = s.world_matrix(b1);
    let radial = |m: &loom_mesh::Mesh, i: usize| {
        let p = w.transform_point3(m.vertices()[i].position);
        let o = b1w.transform_point3(Vec3::ZERO);
        let d = b1w.transform_vector3(Vec3::X).normalize();
        let along = (p - o).dot(d);
        (p - (o + d * along)).length()
    };
    let grew = joint.iter().filter(|i| radial(&bent, **i) > radial(&straight, **i) * 1.2).count();
    assert!(grew * 2 >= joint.len(), "{grew} of {} joint vertices bulged", joint.len());
}

#[test]
fn joint_angle_gizmo_adds_its_deltas_at_the_key() {
    let mut s = Scene::new();
    let (arm, b0, b1) = arm(&mut s);
    let mut data = SkinData::bind(&s, arm, &[b0, b1]);
    data.gizmos.push(SkinGizmo { kind: GizmoKind::JointAngle, bone: 1, parent_bone: 0, keys: vec![GizmoKey { angle_deg: 0.0, deltas: vec![], bulge: 0.0 }, GizmoKey { angle_deg: 90.0, deltas: vec![(0, Vec3::Y * 3.0)], bulge: 0.0 }], influence: 1.0, enabled: true });
    s.node_mut(arm).unwrap().modifiers.push(ModifierSlot::new(Modifier::Skin(Box::new(data))));
    let before = s.eval_mesh(arm).value.mesh.vertices()[0].position;
    s.node_mut(b1).unwrap().transform.rotation.set(Vec3::new(0.0, 0.0, 45.0));
    let half = s.eval_mesh(arm).value.mesh.vertices()[0].position;
    s.node_mut(b1).unwrap().transform.rotation.set(Vec3::new(0.0, 0.0, 90.0));
    let full = s.eval_mesh(arm).value.mesh.vertices()[0].position;
    // Vertex 0 sits at the base (bone 1 only), so skinning leaves it; the gizmo adds its delta.
    assert!(((full - before).length() - 3.0).abs() < 0.2, "{:?}", full - before);
    assert!(((half - before).length() - 1.5).abs() < 0.2, "{:?}", half - before);
}

#[test]
fn mirror_pastes_weights_across_the_plane() {
    let mut s = Scene::new();
    let mesh = loom_mesh::box_segmented(20.0, 20.0, 20.0, [4, 1, 1]);
    let body = s.add_node(Node::new("Body", ObjectKind::EditablePoly(loom_scene::PolyObject::from_mesh(mesh))));
    let mut l = Node::new("Bone_L", ObjectKind::bone(5.0));
    l.transform.position.set(Vec3::new(-8.0, 0.0, 5.0));
    let mut r = Node::new("Bone_R", ObjectKind::bone(5.0));
    r.transform.position.set(Vec3::new(8.0, 0.0, 5.0));
    let (l, r) = (s.add_node(l), s.add_node(r));
    let mut data = SkinData::bind(&s, body, &[l, r]);
    data.explicit_all();
    let m = s.node(body).unwrap().object.mesh().into_owned();
    // Hand-weight the -X side fully to Bone_L.
    for (i, v) in m.vertices().iter().enumerate() {
        if v.position.x < -1.0 {
            data.weights[i] = vec![(0, 1.0)];
            data.explicit[i] = true;
        }
    }
    let pasted = mirror_weights(&mut data, &m, 0, 0.0, 0.5, true);
    assert!(pasted > 0);
    for (i, v) in m.vertices().iter().enumerate() {
        if v.position.x > 1.0 {
            assert_eq!(data.weights[i], vec![(1, 1.0)], "vertex {i} at {:?}", v.position);
            assert!(data.explicit[i]);
        }
    }
}
