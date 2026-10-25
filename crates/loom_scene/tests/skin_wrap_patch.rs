//! Skin Wrap Patch: a patch object deforms a mesh through samples of its surface.

use glam::Vec3;
use loom_scene::morph::SkinWrapPatchData;
use loom_scene::{Modifier, ModifierSlot, Node, ObjectKind, PatchObject, Scene};

#[test]
fn a_patch_object_deforms_the_wrapped_mesh() {
    let mut s = Scene::new();
    let pm = loom_mesh::patch::PatchMesh::quad_grid(20.0, 20.0, 2, 2);
    let patch = s.add_node(Node::new("Patch", ObjectKind::EditablePatch(PatchObject::from_patch(pm.clone()))));
    let mut cloth = Node::new("Cloth", ObjectKind::EditablePoly(loom_scene::PolyObject::from_mesh(loom_mesh::plane_segmented(16.0, 16.0, 4, 4))));
    cloth.transform.position.set(Vec3::Z);
    let cloth = s.add_node(cloth);
    let mut d = SkinWrapPatchData { patch: Some(patch), ..Default::default() };
    assert_eq!(d.sample_rate, 5);
    d.resample(&s, cloth, 0);
    assert_eq!(d.binds.len(), 25);
    assert!(d.binds.iter().all(|b| !b.is_empty()), "every point has patch samples");
    s.node_mut(cloth).unwrap().modifiers.push(ModifierSlot::new(Modifier::SkinWrapPatch(Box::new(d))));
    let rest = s.eval_mesh(cloth).value.mesh.clone();
    let plain = loom_mesh::plane_segmented(16.0, 16.0, 4, 4);
    for (a, b) in rest.vertices().iter().zip(plain.vertices()) {
        assert!((a.position - b.position).length() < 1e-3, "unchanged at rest: {:?} vs {:?}", a.position, b.position);
    }
    // Moving the patch object carries the mesh along.
    s.node_mut(patch).unwrap().transform.position.set(Vec3::new(0.0, 0.0, 5.0));
    let lifted = s.eval_mesh(cloth).value.mesh.clone();
    for (a, b) in rest.vertices().iter().zip(lifted.vertices()) {
        assert!(((b.position - a.position) - Vec3::Z * 5.0).length() < 1e-3);
    }
    s.node_mut(patch).unwrap().transform.position.set(Vec3::ZERO);
    // Raising one lattice corner bends the mesh near it, not far from it.
    let corner = pm.verts.iter().enumerate().max_by(|a, b| (a.1.x + a.1.y).total_cmp(&(b.1.x + b.1.y))).unwrap().0;
    let mut raised = pm.clone();
    raised.verts[corner].z += 6.0;
    s.node_mut(patch).unwrap().object = ObjectKind::EditablePatch(PatchObject::from_patch(raised));
    let bent = s.eval_mesh(cloth).value.mesh.clone();
    let lift = |at: Vec3| {
        let i = plain.vertices().iter().position(|v| (v.position - at).length() < 1e-3).unwrap();
        bent.vertices()[i].position.z - rest.vertices()[i].position.z
    };
    assert!(lift(Vec3::new(8.0, 8.0, 0.0)) > 1.0, "the near corner rises: {}", lift(Vec3::new(8.0, 8.0, 0.0)));
    assert!(lift(Vec3::new(-8.0, -8.0, 0.0)).abs() < 1e-2, "the far corner stays");
    let e = s.eval_mesh(cloth);
    assert!(e.warnings.is_empty(), "{:?}", e.warnings);
}

#[test]
fn a_changed_patch_topology_warns_until_resampled() {
    let mut s = Scene::new();
    let patch = s.add_node(Node::new("Patch", ObjectKind::EditablePatch(PatchObject::from_patch(loom_mesh::patch::PatchMesh::quad_grid(20.0, 20.0, 1, 1)))));
    let cloth = s.add_node(Node::new("Cloth", ObjectKind::EditablePoly(loom_scene::PolyObject::from_mesh(loom_mesh::plane_segmented(10.0, 10.0, 1, 1)))));
    let mut d = SkinWrapPatchData { patch: Some(patch), sample_rate: 3, ..Default::default() };
    d.resample(&s, cloth, 0);
    s.node_mut(cloth).unwrap().modifiers.push(ModifierSlot::new(Modifier::SkinWrapPatch(Box::new(d))));
    s.node_mut(patch).unwrap().object = ObjectKind::EditablePatch(PatchObject::from_patch(loom_mesh::patch::PatchMesh::quad_grid(20.0, 20.0, 2, 1)));
    let e = s.eval_mesh(cloth);
    assert!(e.warnings.iter().any(|w| w.contains("Resample")), "{:?}", e.warnings);
}
