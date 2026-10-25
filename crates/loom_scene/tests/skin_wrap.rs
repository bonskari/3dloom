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

/// A 20×20 control plane (5-unit grid) and a tiny wrapped plane at `at`.
fn control_and_point(s: &mut Scene, at: Vec3) -> (loom_scene::NodeId, loom_scene::NodeId) {
    let base = s.add_node(Node::new("Control", ObjectKind::EditablePoly(loom_scene::PolyObject::from_mesh(loom_mesh::plane_segmented(20.0, 20.0, 4, 4)))));
    let mut n = Node::new("Hi", ObjectKind::EditablePoly(loom_scene::PolyObject::from_mesh(loom_mesh::plane_segmented(0.2, 0.2, 1, 1))));
    n.transform.position.set(at);
    let hi = s.add_node(n);
    (base, hi)
}

fn bound(d: &SkinWrapData) -> Vec<usize> {
    d.binds.iter().map(|b| b.len()).collect()
}

#[test]
fn skin_wrap_defaults_follow_the_reference() {
    let d = SkinWrapData::default();
    assert_eq!(d.engine, 1, "Vertex Deformation is the default engine");
    assert_eq!((d.falloff, d.distance_influence, d.face_limit, d.threshold, d.blend_distance), (1.0, 1.2, 3, 5.0, 5.0));
    assert!(!d.blend_to_base && !d.weight_all_points);
    assert!(d.display.loops && d.display.axis && d.display.face_limit && !d.display.unassigned && d.display.control_verts);
}

#[test]
fn distance_influence_is_a_radius_of_average_edge_lengths() {
    let mut s = Scene::new();
    let (base, hi) = control_and_point(&mut s, Vec3::new(2.5, 2.5, 0.5));
    let mut d = SkinWrapData { bases: vec![base], ..Default::default() };
    d.rebind(&s, hi, 0);
    // 1.2 × 5 = 6 units: the four corners of the cell (about 3.6 away) reach the points.
    assert!(bound(&d).iter().all(|n| *n == 4), "{:?}", bound(&d));
    d.distance_influence = 0.5; // 2.5 units: no control vertex reaches
    d.rebind(&s, hi, 0);
    assert!(bound(&d).iter().all(|n| *n == 0), "{:?}", bound(&d));
    assert!(d.closest.iter().all(|c| c.is_some()), "the points still have a closest face (red box when shown)");
    d.weight_all_points = true;
    d.rebind(&s, hi, 0);
    assert!(bound(&d).iter().all(|n| *n >= 1), "Weight All Points weights every point");
}

#[test]
fn face_limit_counts_faces_from_the_closest_face() {
    let mut s = Scene::new();
    let (base, hi) = control_and_point(&mut s, Vec3::new(2.5, 2.5, 0.5));
    let mut d = SkinWrapData { bases: vec![base], distance_influence: 10.0, face_limit: 0, ..Default::default() };
    d.rebind(&s, hi, 0);
    assert!(bound(&d).iter().all(|n| *n == 4), "Face Limit 0: the closest face's corners only: {:?}", bound(&d));
    d.face_limit = 1;
    d.rebind(&s, hi, 0);
    assert!(bound(&d).iter().all(|n| *n == 16), "one face further: a 4×4 block of control vertices: {:?}", bound(&d));
}

#[test]
fn face_engine_is_rigid_at_the_lowest_falloff() {
    let mut s = Scene::new();
    let (base, hi) = control_and_point(&mut s, Vec3::new(2.5, 2.5, 0.5));
    let mut d = SkinWrapData { bases: vec![base], engine: 0, falloff: 0.001, ..Default::default() };
    d.rebind(&s, hi, 0);
    assert!(bound(&d).iter().all(|n| *n == 1), "rigid: one face each: {:?}", bound(&d));
    d.falloff = 1.0;
    d.rebind(&s, hi, 0);
    assert!(bound(&d).iter().all(|n| *n > 1), "with falloff the neighbouring faces blend in: {:?}", bound(&d));
}

/// Moves the control object up by `dz` and returns how far the wrapped points moved (mean Z).
fn lift(s: &mut Scene, base: loom_scene::NodeId, hi: loom_scene::NodeId, d: SkinWrapData, dz: f32) -> f32 {
    s.node_mut(hi).unwrap().modifiers = vec![ModifierSlot::new(Modifier::SkinWrap(Box::new(d)))];
    let before = s.eval_mesh(hi).value.mesh.clone();
    s.node_mut(base).unwrap().transform.position.set(Vec3::Z * dz);
    let after = s.eval_mesh(hi).value.mesh.clone();
    s.node_mut(base).unwrap().transform.position.set(Vec3::ZERO);
    let n = before.vertex_count() as f32;
    before.vertices().iter().zip(after.vertices()).map(|(a, b)| b.position.z - a.position.z).sum::<f32>() / n
}

#[test]
fn local_strength_scales_and_flips_the_pull() {
    let mut s = Scene::new();
    let (base, hi) = control_and_point(&mut s, Vec3::new(2.5, 2.5, 0.5));
    let mut d = SkinWrapData { bases: vec![base], ..Default::default() };
    d.rebind(&s, hi, 0);
    assert!((lift(&mut s, base, hi, d.clone(), 1.0) - 1.0).abs() < 1e-3);
    for l in d.locals[0].iter_mut() {
        l.strength = 2.0;
    }
    assert!((lift(&mut s, base, hi, d.clone(), 1.0) - 2.0).abs() < 1e-3, "Local Strength 2 pulls twice as far");
    for l in d.locals[0].iter_mut() {
        l.strength = -1.0;
    }
    assert!((lift(&mut s, base, hi, d.clone(), 1.0) + 1.0).abs() < 1e-3, "a negative strength pushes away");
}

#[test]
fn local_scale_and_axes_shape_the_volume_of_influence() {
    let mut s = Scene::new();
    let (base, hi) = control_and_point(&mut s, Vec3::new(2.5, 2.5, 3.0));
    let mut d = SkinWrapData { bases: vec![base], ..Default::default() };
    d.rebind(&s, hi, 0);
    assert!(bound(&d).iter().all(|n| *n > 0));
    for l in d.locals[0].iter_mut() {
        l.z = 0.4; // the volume flattens along the normal: 3 units up is outside
    }
    d.rebind(&s, hi, 0);
    assert!(bound(&d).iter().all(|n| *n == 0), "{:?}", bound(&d));
    for l in d.locals[0].iter_mut() {
        *l = Default::default();
        l.scale = 0.1;
    }
    d.rebind(&s, hi, 0);
    assert!(bound(&d).iter().all(|n| *n == 0), "Local Scale Mult shrinks the volume");
}

#[test]
fn blend_to_base_mesh_weakens_points_far_from_the_control_faces() {
    let mut s = Scene::new();
    let (base, hi) = control_and_point(&mut s, Vec3::new(2.5, 2.5, 4.0));
    let mut d = SkinWrapData { bases: vec![base], distance_influence: 2.0, ..Default::default() };
    d.rebind(&s, hi, 0);
    assert!((lift(&mut s, base, hi, d.clone(), 1.0) - 1.0).abs() < 1e-3);
    d.blend_to_base = true;
    d.blend_distance = 1.0; // 1 × the 5-unit edge: 4 units away keeps a fifth
    d.rebind(&s, hi, 0);
    let moved = lift(&mut s, base, hi, d.clone(), 1.0);
    assert!((moved - 0.2).abs() < 1e-2, "{moved}");
    d.blend_distance = 5.0;
    d.rebind(&s, hi, 0);
    let moved = lift(&mut s, base, hi, d.clone(), 1.0);
    assert!((moved - (1.0 - 4.0 / 25.0)).abs() < 1e-2, "a larger Blend Distance deforms more: {moved}");
}
