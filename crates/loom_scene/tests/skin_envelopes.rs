//! Skin envelopes and deformation options: Absolute / Relative, cross
//! sections, spline and primitive envelope shapes, Rigid Vertices (All),
//! Back Transform Vertices, Ignore Bone Scale, Squash, Fast Updates,
//! Animatable Envelopes and Skin on a patch (Rigid Handles).

use glam::Vec3;
use loom_scene::skin::{envelope_sample, radius_profile, Envelope, SkinData};
use loom_scene::{Modifier, ModifierSlot, Node, NodeId, ObjectKind, Scene};

/// A one-vertex-ring-at-a-time cylinder along X skinned to one 10-unit bone at the origin.
fn one_bone(s: &mut Scene) -> (NodeId, NodeId) {
    let mesh = loom_mesh::cylinder_segmented(2.0, 10.0, 8, 1, 4, loom_mesh::Slice::default());
    let mut poly = Node::new("Arm", ObjectKind::EditablePoly(loom_scene::PolyObject::from_mesh(mesh)));
    poly.transform.rotation.set(Vec3::new(0.0, 90.0, 0.0));
    let arm = s.add_node(poly);
    let b = s.add_node(Node::new("Bone001", ObjectKind::bone(10.0)));
    (arm, b)
}

fn set_skin(s: &mut Scene, arm: NodeId, d: SkinData) {
    let n = s.node_mut(arm).unwrap();
    if n.modifiers.is_empty() {
        n.modifiers.push(ModifierSlot::new(Modifier::Skin(Box::new(d))));
    } else {
        n.modifiers[0].modifier = Modifier::Skin(Box::new(d));
    }
}

fn world_points(s: &Scene, arm: NodeId) -> Vec<Vec3> {
    let w = s.object_matrix(arm);
    s.eval_mesh(arm).value.mesh.vertices().iter().map(|v| w.transform_point3(v.position)).collect()
}

#[test]
fn a_relative_envelope_keeps_partial_weight_and_the_rest_stays_put() {
    let mut data = SkinData::default();
    let e = Envelope { start: Vec3::ZERO, end: Vec3::X * 10.0, inner: [1.0; 2], outer: [3.0; 2], ..Default::default() };
    data.bones.push(loom_scene::skin::SkinBone { node: NodeId(9), bind_inverse: glam::Mat4::IDENTITY, envelope: e.clone(), cross_sections: Vec::new(), enabled: true, envelope_visible: false });
    data.excluded.push(Default::default());
    // Absolute: a point between the radii of the only envelope is fully that bone's.
    let p = Vec3::new(5.0, 2.0, 0.0);
    assert_eq!(data.envelope_weights(p, None), vec![(0, 1.0)]);
    // Relative: it keeps the falloff weight (0.5), not scaled up to 1.
    data.bones[0].envelope.relative = true;
    let w = data.envelope_weights(p, None);
    assert_eq!(w.len(), 1);
    assert!((w[0].1 - 0.5).abs() < 1e-5, "{w:?}");
    // Inside the inner radius it is locked at full weight.
    assert_eq!(data.envelope_weights(Vec3::new(5.0, 0.5, 0.0), None), vec![(0, 1.0)]);
    assert!(envelope_sample(&e, &[], Vec3::new(5.0, 0.5, 0.0)).inside_inner);
}

#[test]
fn a_relative_vertex_moves_part_of_the_way_with_its_bone() {
    let mut s = Scene::new();
    let (arm, b) = one_bone(&mut s);
    let mut data = SkinData::bind(&s, arm, &[b]);
    data.weight_all_vertices = false;
    // Inner 1, outer 3: the cylinder's surface (radius 2) sits halfway between.
    data.bones[0].envelope = Envelope { start: Vec3::ZERO, end: Vec3::X * 10.0, inner: [1.0; 2], outer: [3.0; 2], ..Default::default() };
    set_skin(&mut s, arm, data.clone());
    let rest = world_points(&s, arm);
    s.node_mut(b).unwrap().transform.position.set(Vec3::new(0.0, 0.0, 10.0));
    let moved = world_points(&s, arm);
    let i = rest.iter().position(|p| p.x > 4.0 && p.x < 6.0).unwrap();
    assert!((moved[i].z - rest[i].z - 10.0).abs() < 1e-3, "Absolute: the whole way {:?} -> {:?}", rest[i], moved[i]);
    data.bones[0].envelope.relative = true;
    set_skin(&mut s, arm, data);
    let half = world_points(&s, arm);
    assert!((half[i].z - rest[i].z - 5.0).abs() < 1e-2, "Relative: half way {:?} -> {:?}", rest[i], half[i]);
}

#[test]
fn cross_sections_shape_the_radius_along_the_envelope() {
    let e = Envelope { start: Vec3::ZERO, end: Vec3::X * 10.0, inner: [1.0; 2], outer: [2.0; 2], ..Default::default() };
    let sections = [(0.5, 4.0, 6.0)];
    assert_eq!(radius_profile(&e, &sections, 0.5), (4.0, 6.0));
    let (i, o) = radius_profile(&e, &sections, 0.25);
    assert!((i - 2.5).abs() < 1e-5 && (o - 4.0).abs() < 1e-5, "{i} {o}");
    // A point 3 from the axis at the middle is inside the widened inner radius.
    assert!(envelope_sample(&e, &sections, Vec3::new(5.0, 3.0, 0.0)).inside_inner);
    assert_eq!(envelope_sample(&e, &[], Vec3::new(5.0, 3.0, 0.0)).weight, 0.0);
}

#[test]
fn a_spline_bone_gets_an_envelope_along_its_curve_and_a_box_along_its_longest_side() {
    let mut s = Scene::new();
    let (arm, _) = one_bone(&mut s);
    let shape = loom_shape::Shape::polyline_shape(&[Vec3::ZERO, Vec3::new(10.0, 0.0, 0.0), Vec3::new(10.0, 10.0, 0.0)]);
    let spline = s.add_node(Node::new("Line001", ObjectKind::line_object(shape)));
    let mut bx = Node::new("Box001", ObjectKind::box_object(2.0, 2.0, 12.0));
    bx.transform.position.set(Vec3::new(30.0, 0.0, 0.0));
    let bx = s.add_node(bx);
    let data = SkinData::bind(&s, arm, &[spline, bx]);
    let e = &data.bones[0].envelope;
    assert!(e.path.len() >= 3, "the envelope follows the spline: {:?}", e.path);
    // The corner of the L is on the envelope's axis.
    let (_, closest) = e.closest(Vec3::new(10.0, 5.0, 1.0));
    assert!((closest - Vec3::new(10.0, 5.0, 0.0)).length() < 1e-3, "{closest:?}");
    // The box's longest side is its height (local Z).
    let eb = &data.bones[1].envelope;
    let dir = (eb.end - eb.start).normalize();
    assert!(dir.abs().z > 0.99, "{dir:?}");
}

#[test]
fn rigid_vertices_all_follow_the_strongest_bone_only() {
    let mut data = SkinData::default();
    data.weights = vec![vec![(0, 0.3), (1, 0.7)]];
    data.explicit = vec![true];
    data.rigid = vec![false];
    for i in 0..2 {
        data.bones.push(loom_scene::skin::SkinBone { node: NodeId(10 + i), bind_inverse: glam::Mat4::IDENTITY, envelope: Envelope::default(), cross_sections: Vec::new(), enabled: true, envelope_visible: false });
        data.excluded.push(Default::default());
    }
    assert_eq!(data.vertex_weights(0, Vec3::ZERO), vec![(0, 0.3), (1, 0.7)]);
    data.rigid_all = true;
    assert_eq!(data.vertex_weights(0, Vec3::ZERO), vec![(1, 1.0)]);
    // The stored weights stay, so turning it off brings the blend back.
    assert_eq!(data.weights[0], vec![(0, 0.3), (1, 0.7)]);
}

#[test]
fn back_transform_vertices_off_moves_a_linked_mesh_twice() {
    let mut s = Scene::new();
    let (arm, b) = one_bone(&mut s);
    let data = SkinData::bind(&s, arm, &[b]);
    set_skin(&mut s, arm, data.clone());
    s.node_mut(arm).unwrap().parent = Some(b);
    let rest = world_points(&s, arm);
    s.node_mut(b).unwrap().transform.position.set(Vec3::new(0.0, 5.0, 0.0));
    let on = world_points(&s, arm);
    assert!((on[0] - rest[0] - Vec3::Y * 5.0).length() < 1e-3, "on: once {:?}", on[0] - rest[0]);
    let mut off = data;
    off.back_transform = false;
    set_skin(&mut s, arm, off);
    let twice = world_points(&s, arm);
    assert!((twice[0] - rest[0] - Vec3::Y * 10.0).length() < 1e-3, "off: twice {:?}", twice[0] - rest[0]);
}

#[test]
fn ignore_bone_scale_and_squash() {
    let mut s = Scene::new();
    let (arm, b) = one_bone(&mut s);
    let data = SkinData::bind(&s, arm, &[b]);
    set_skin(&mut s, arm, data.clone());
    let rest = world_points(&s, arm);
    // Scale the bone 2× along its length: the mesh stretches with it.
    s.node_mut(b).unwrap().transform.scale.set(Vec3::new(2.0, 1.0, 1.0));
    let far = rest.iter().enumerate().max_by(|a, b| a.1.x.total_cmp(&b.1.x)).unwrap().0;
    let scaled = world_points(&s, arm);
    assert!((scaled[far].x - rest[far].x * 2.0).abs() < 1e-3);
    let mut ignore = data.clone();
    ignore.ignore_bone_scale = true;
    set_skin(&mut s, arm, ignore);
    assert!((world_points(&s, arm)[far] - rest[far]).length() < 1e-3, "Ignore Bone Scale: unaffected");
    // Stretch = Squash: across the bone the mesh thins by 1/sqrt(2) (Squash 1), not with Squash 0.
    set_skin(&mut s, arm, data.clone());
    if let ObjectKind::Bone { stretch, .. } = &mut s.node_mut(b).unwrap().object {
        stretch.set(2);
    }
    let radial = |p: Vec3| (p.y * p.y + p.z * p.z).sqrt();
    let thin = world_points(&s, arm);
    assert!((radial(thin[far]) - radial(rest[far]) / 2f32.sqrt()).abs() < 1e-3, "{} vs {}", radial(thin[far]), radial(rest[far]));
    let mut none = data;
    none.bones[0].envelope.squash = 0.0;
    set_skin(&mut s, arm, none);
    assert!((radial(world_points(&s, arm)[far]) - radial(rest[far])).abs() < 1e-3);
}

#[test]
fn always_deform_off_leaves_the_mesh_undeformed() {
    let mut s = Scene::new();
    let (arm, b) = one_bone(&mut s);
    let mut data = SkinData::bind(&s, arm, &[b]);
    set_skin(&mut s, arm, data.clone());
    let rest = world_points(&s, arm);
    s.node_mut(b).unwrap().transform.position.set(Vec3::new(0.0, 5.0, 0.0));
    assert!((world_points(&s, arm)[0] - rest[0]).length() > 4.0);
    data.always_deform = false;
    set_skin(&mut s, arm, data);
    assert!((world_points(&s, arm)[0] - rest[0]).length() < 1e-4);
}

#[test]
fn fast_updates_preview_rigidly_but_render_in_full() {
    let mut s = Scene::new();
    let (arm, b0) = one_bone(&mut s);
    let mut b1n = Node::new("Bone002", ObjectKind::bone(10.0));
    b1n.transform.position.set(Vec3::new(10.0, 0.0, 0.0));
    b1n.parent = Some(b0);
    let b1 = s.add_node(b1n);
    let mut data = SkinData::bind(&s, arm, &[b0, b1]);
    // One vertex half on each bone.
    data.explicit_all();
    data.weights[0] = vec![(0, 0.4), (1, 0.6)];
    set_skin(&mut s, arm, data.clone());
    let rest = world_points(&s, arm);
    s.node_mut(b1).unwrap().transform.position.set(Vec3::new(10.0, 10.0, 0.0));
    let blended = world_points(&s, arm)[0];
    assert!((blended.y - rest[0].y - 6.0).abs() < 1e-3);
    data.fast_updates = true;
    set_skin(&mut s, arm, data);
    assert!((world_points(&s, arm)[0].y - rest[0].y - 10.0).abs() < 1e-3, "rigid preview: the strongest bone");
    let render = loom_scene::anim::with_rendering(|| world_points(&s, arm)[0]);
    assert!((render.y - rest[0].y - 6.0).abs() < 1e-3, "renders deform fully");
}

#[test]
fn animatable_envelope_radii_key_and_play_back() {
    let mut e = Envelope { inner: [1.0; 2], outer: [2.0; 2], ..Default::default() };
    let p = |x: f32| loom_scene::Param::new(x);
    e.keys = Some(Box::new(loom_scene::skin::EnvelopeKeys { inner: [p(1.0), p(1.0)], outer: [p(2.0), p(2.0)] }));
    loom_scene::anim::with(10.0, true, || e.set_radius(0, true, 6.0));
    let at = |t: f32| loom_scene::anim::with(t, false, || e.radii().1[0]);
    assert!((at(10.0) - 6.0).abs() < 1e-4, "{}", at(10.0));
    assert!((at(0.0) - 2.0).abs() < 1e-4, "{}", at(0.0));
}

#[test]
fn skin_on_a_patch_moves_the_lattice_and_rigid_handles_follow_one_bone() {
    let mut s = Scene::new();
    let patch = loom_mesh::patch::PatchMesh::quad_grid(20.0, 4.0, 2, 1);
    let mut node = Node::new("Patch", ObjectKind::EditablePatch(loom_scene::PatchObject::from_patch(patch.clone())));
    node.transform.position.set(Vec3::ZERO);
    let p = s.add_node(node);
    let mut b0n = Node::new("Bone001", ObjectKind::bone(10.0));
    b0n.transform.position.set(Vec3::new(-10.0, 0.0, 0.0));
    let b0 = s.add_node(b0n);
    let mut b1n = Node::new("Bone002", ObjectKind::bone(10.0));
    b1n.transform.position.set(Vec3::new(0.0, 0.0, 0.0));
    let b1 = s.add_node(b1n);
    let cage = patch.cage();
    let mut data = SkinData::bind_mesh(&s, p, &[b0, b1], &cage);
    assert_eq!(data.weights.len(), patch.verts.len() + patch.vecs.len(), "vertices and handles are weighted");
    // The middle vertex: half and half; its handles hand-weighted fully to bone 0.
    let mid = patch.verts.iter().position(|v| v.y.abs() < 1e-3).unwrap();
    data.explicit_all();
    data.weights[mid] = vec![(0, 0.3), (1, 0.7)];
    let owners = loom_scene::skin::handle_owners(&patch);
    let handles: Vec<usize> = owners.iter().enumerate().filter(|(_, o)| **o == Some(mid)).map(|(h, _)| h).collect();
    assert!(!handles.is_empty());
    for h in &handles {
        data.weights[patch.verts.len() + h] = vec![(0, 1.0)];
    }
    s.node_mut(p).unwrap().modifiers.push(ModifierSlot::new(Modifier::Skin(Box::new(data.clone()))));
    let before = s.eval_mesh(p).value.patch.clone().expect("the patch flows up through Skin");
    s.node_mut(b1).unwrap().transform.position.set(Vec3::new(0.0, 0.0, 10.0));
    let ev = s.eval_mesh(p);
    let moved = ev.value.patch.clone().unwrap();
    assert!((moved.verts[mid].z - before.verts[mid].z - 7.0).abs() < 1e-3, "{:?}", moved.verts[mid]);
    let h = patch.verts.len() + handles[0];
    assert!((moved.vecs[handles[0]].z - before.vecs[handles[0]].z).abs() < 1e-3, "handle {h} stays with bone 0");
    // The surface is tessellated from the moved lattice.
    assert!(ev.value.mesh.vertices().iter().any(|v| v.position.z > 3.0));
    // Rigid Handles on the middle vertex: its handles take its strongest bone (bone 1) fully.
    data.rigid_handles = vec![false; data.weights.len()];
    data.rigid_handles[mid] = true;
    s.node_mut(p).unwrap().modifiers[0].modifier = Modifier::Skin(Box::new(data.clone()));
    let rigid = s.eval_mesh(p).value.patch.clone().unwrap();
    assert!((rigid.vecs[handles[0]].z - before.vecs[handles[0]].z - 10.0).abs() < 1e-3, "{:?}", rigid.vecs[handles[0]]);
    // Rigid Patch Handles (All): handles take their vertex's weights (0.3 / 0.7).
    data.rigid_handles[mid] = false;
    data.rigid_patch_handles_all = true;
    s.node_mut(p).unwrap().modifiers[0].modifier = Modifier::Skin(Box::new(data));
    let all = s.eval_mesh(p).value.patch.clone().unwrap();
    assert!((all.vecs[handles[0]].z - before.vecs[handles[0]].z - 7.0).abs() < 1e-3, "{:?}", all.vecs[handles[0]]);
}
