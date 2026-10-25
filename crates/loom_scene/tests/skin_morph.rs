//! Skin Morph: influence angle (the morph is gone half the influence angle
//! away), Ball / Planar joints, the custom falloff graph, Edit mode and the
//! soft selection weights.

use glam::Vec3;
use loom_scene::morph::{curve_value, SkinMorphData, SkinMorphTarget, FALLOFF_CUSTOM};
use loom_scene::{Modifier, ModifierSlot, Node, NodeId, ObjectKind, Scene};

/// A 20-unit arm along X with two 10-unit bones; the Skin Morph binds the forearm.
fn rig(s: &mut Scene) -> (NodeId, NodeId, NodeId) {
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

fn turn(s: &mut Scene, bone: NodeId, euler: Vec3) {
    s.node_mut(bone).unwrap().transform.rotation.set(euler);
}

fn weight(s: &Scene, d: &SkinMorphData, m: usize) -> f32 {
    d.morph_weight(m, &|id| Some(s.world_matrix(id)))
}

#[test]
fn the_morph_fades_out_at_half_the_influence_angle() {
    let mut s = Scene::new();
    let (arm, _, b1) = rig(&mut s);
    turn(&mut s, b1, Vec3::new(0.0, 0.0, 60.0));
    let mut d = SkinMorphData::bind(&s, &[b1]);
    let m = d.create_morph(&s, 0);
    d.morphs[m].deltas.push((0, Vec3::Y));
    assert_eq!(d.morphs[m].influence_deg, 90.0, "Influence Angle defaults to 90");
    assert!((weight(&s, &d, m) - 1.0).abs() < 1e-4, "full at the pose");
    turn(&mut s, b1, Vec3::new(0.0, 0.0, 60.0 - 22.5));
    assert!((weight(&s, &d, m) - 0.5).abs() < 1e-3, "half way to 45° away: {}", weight(&s, &d, m));
    turn(&mut s, b1, Vec3::new(0.0, 0.0, 60.0 - 45.0));
    assert!(weight(&s, &d, m).abs() < 1e-4, "gone 45° away");
    turn(&mut s, b1, Vec3::new(0.0, 0.0, 60.0 - 50.0));
    assert_eq!(weight(&s, &d, m), 0.0, "nothing beyond half the influence angle");
    // The evaluated mesh: the delta (bone frame) shows fully at the pose.
    turn(&mut s, b1, Vec3::new(0.0, 0.0, 60.0));
    let rest = s.eval_mesh(arm).value.mesh.vertices()[0].position;
    s.node_mut(arm).unwrap().modifiers.push(ModifierSlot::new(Modifier::SkinMorph(Box::new(d))));
    let morphed = s.eval_mesh(arm).value.mesh.vertices()[0].position;
    assert!(((morphed - rest).length() - 1.0).abs() < 1e-3, "{:?}", morphed - rest);
}

#[test]
fn ball_joints_track_twist_and_planar_joints_only_the_parent_plane() {
    let mut s = Scene::new();
    let (_, _, b1) = rig(&mut s);
    let mut d = SkinMorphData::bind(&s, &[b1]);
    let m = d.create_morph(&s, 0);
    assert!(!d.bones[0].planar, "Joint Type defaults to Ball Joint");
    // A twist about the bone's own axis.
    turn(&mut s, b1, Vec3::new(40.0, 0.0, 0.0));
    let ball = weight(&s, &d, m);
    assert!((ball - (1.0 - 40.0 / 45.0)).abs() < 1e-3, "Ball sees the twist: {ball}");
    d.bones[0].planar = true;
    assert!((weight(&s, &d, m) - 1.0).abs() < 1e-4, "Planar ignores a twist");
    // A swing in the parent's XY plane: both see it.
    turn(&mut s, b1, Vec3::new(0.0, 0.0, 30.0));
    assert!((weight(&s, &d, m) - (1.0 - 30.0 / 45.0)).abs() < 1e-3);
    d.bones[0].planar = false;
    assert!((weight(&s, &d, m) - (1.0 - 30.0 / 45.0)).abs() < 1e-3);
    // A swing out of the plane (about the parent's Y): only Ball sees it.
    turn(&mut s, b1, Vec3::new(0.0, 30.0, 0.0));
    assert!(weight(&s, &d, m) < 0.5);
    d.bones[0].planar = true;
    assert!((weight(&s, &d, m) - 1.0).abs() < 1e-3, "Planar: no rotation in the parent plane");
}

#[test]
fn custom_falloff_follows_the_graph_and_edit_mode_holds_the_morph_full() {
    let mut s = Scene::new();
    let (_, _, b1) = rig(&mut s);
    let mut d = SkinMorphData::bind(&s, &[b1]);
    let m = d.create_morph(&s, 0);
    // The default graph is the Sinual falloff.
    let sinual = |t: f32| 0.5 - 0.5 * (std::f32::consts::PI * t).cos();
    assert!((curve_value(&d.morphs[m].curve, 0.5) - sinual(0.5)).abs() < 1e-4);
    d.morphs[m].falloff = FALLOFF_CUSTOM;
    d.morphs[m].curve = vec![[0.0, 0.0], [0.5, 0.9], [1.0, 1.0]];
    turn(&mut s, b1, Vec3::new(0.0, 0.0, 22.5));
    assert!((weight(&s, &d, m) - 0.9).abs() < 1e-3, "{}", weight(&s, &d, m));
    turn(&mut s, b1, Vec3::new(0.0, 0.0, 80.0));
    assert_eq!(weight(&s, &d, m), 0.0);
    d.editing = Some(m);
    assert_eq!(weight(&s, &d, m), 1.0, "Edit holds the edited morph at 100%");
    d.editing = None;
    d.morphs[m].enabled = false;
    turn(&mut s, b1, Vec3::ZERO);
    assert_eq!(weight(&s, &d, m), 0.0, "a disabled morph does nothing");
}

#[test]
fn soft_selection_weights_fall_off_within_the_radius_and_the_edge_limit() {
    let mesh = loom_mesh::plane_segmented(40.0, 40.0, 8, 8); // 5-unit grid
    let mut d = SkinMorphData::default();
    let centre = mesh.vertices().iter().position(|v| v.position.length() < 1e-4).unwrap() as u32;
    d.selection = vec![centre];
    let w = d.soft_weights(&mesh, glam::Mat4::IDENTITY);
    assert_eq!(w[centre as usize], 1.0);
    assert!(w.iter().enumerate().all(|(i, x)| i == centre as usize || *x == 0.0), "no soft selection when it is off");
    d.soft.on = true;
    d.soft.radius = 12.0;
    let w = d.soft_weights(&mesh, glam::Mat4::IDENTITY);
    let at = |x: f32, y: f32| mesh.vertices().iter().position(|v| (v.position - Vec3::new(x, y, 0.0)).length() < 1e-4).unwrap();
    assert!(w[at(5.0, 0.0)] > w[at(10.0, 0.0)] && w[at(10.0, 0.0)] > 0.0, "{} {}", w[at(5.0, 0.0)], w[at(10.0, 0.0)]);
    assert_eq!(w[at(15.0, 0.0)], 0.0, "outside the radius");
    d.soft.edge_limit_on = true;
    d.soft.edge_limit = 1;
    let w = d.soft_weights(&mesh, glam::Mat4::IDENTITY);
    assert!(w[at(5.0, 0.0)] > 0.0 && w[at(10.0, 0.0)] == 0.0, "two edges away is past an Edge Limit of 1");
}

#[test]
fn an_old_morph_without_a_pose_still_uses_its_angle() {
    let mut s = Scene::new();
    let (_, _, b1) = rig(&mut s);
    let mut d = SkinMorphData::bind(&s, &[b1]);
    d.morphs.push(SkinMorphTarget { bone: 0, name: "old".into(), angle_deg: 90.0, ..Default::default() });
    turn(&mut s, b1, Vec3::new(0.0, 0.0, 90.0));
    assert!((weight(&s, &d, 0) - 1.0).abs() < 1e-3);
    turn(&mut s, b1, Vec3::new(0.0, 0.0, 67.5));
    assert!((weight(&s, &d, 0) - 0.5).abs() < 1e-3);
}
