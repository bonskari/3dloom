//! Skin Wrap: a mesh bound to the faces of a base mesh follows the base's deformation.

use glam::Vec3;
use loom_scene::morph::SkinWrapData;
use loom_scene::{Modifier, ModifierSlot, Node, ObjectKind, Scene};

#[test]
fn skin_wrap_follows_the_deformed_base() {
    let mut s = Scene::new();
    // Base: a 20×20 plane with 4×4 segments; cloth: a smaller plane one unit above it.
    let base = s.add_node(Node::new("Base", ObjectKind::EditablePoly(loom_scene::PolyObject::from_mesh(loom_mesh::plane_segmented(20.0, 20.0, 4, 4)))));
    let mut cloth_n = Node::new("Cloth", ObjectKind::EditablePoly(loom_scene::PolyObject::from_mesh(loom_mesh::plane_segmented(10.0, 10.0, 2, 2))));
    cloth_n.transform.position.set(Vec3::Z * 1.0);
    let cloth = s.add_node(cloth_n);
    let data = SkinWrapData::bind(&s, cloth, &[base]);
    assert_eq!(data.binds.len(), 9);
    assert!(data.binds.iter().all(|b| !b.is_empty()), "every cloth vertex found base faces");
    s.node_mut(cloth).unwrap().modifiers.push(ModifierSlot::new(Modifier::SkinWrap(Box::new(data))));
    let rest = s.eval_mesh(cloth).value.mesh.clone();
    // Unbent, the wrap leaves the cloth where it is.
    let cloth_mesh = loom_mesh::plane_segmented(10.0, 10.0, 2, 2);
    for (a, b) in rest.vertices().iter().zip(cloth_mesh.vertices()) {
        assert!((a.position - b.position).length() < 1e-3, "{:?} vs {:?}", a.position, b.position);
    }
    // Bend the base 90° about X: the cloth follows.
    let mut bend = Modifier::by_name("Bend").unwrap();
    bend.set_parameter("Angle", 90.0).unwrap();
    bend.set_parameter("Bend Axis", 1.0).unwrap(); // along Y: a flat plane has no Z extent to bend
    s.node_mut(base).unwrap().modifiers.push(ModifierSlot::new(bend));
    let bent_base = s.eval_mesh(base).value.mesh.clone();
    let base_moved = loom_mesh::plane_segmented(20.0, 20.0, 4, 4).vertices().iter().zip(bent_base.vertices()).filter(|(a, b)| (a.position - b.position).length() > 0.5).count();
    assert!(base_moved > 0, "the base itself bent");
    let bent = s.eval_mesh(cloth).value.mesh.clone();
    let moved = rest.vertices().iter().zip(bent.vertices()).filter(|(a, b)| (a.position - b.position).length() > 0.5).count();
    assert!(moved >= 3, "{moved} cloth vertices followed the bend");
    let e = s.eval_mesh(cloth);
    assert!(e.warnings.is_empty(), "{:?}", e.warnings);
    // A cloth vertex stays about one unit off the bent base's surface.
    let w = s.object_matrix(cloth);
    let bw = s.object_matrix(base);
    for v in bent.vertices() {
        let p = w.transform_point3(v.position);
        let d = bent_base.vertices().iter().map(|b| bw.transform_point3(b.position).distance(p)).fold(f32::INFINITY, f32::min);
        assert!(d < 4.0, "cloth vertex {p:?} is {d} from the base");
    }
}
