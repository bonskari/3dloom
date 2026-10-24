//! Bone, Dummy, Point and IK goal object kinds.

use loom_scene::{Node, ObjectKind, Scene};

#[test]
fn bone_mesh_points_along_x_and_tapers() {
    let kind = ObjectKind::bone(50.0);
    let m = kind.mesh().into_owned();
    let (lo, hi) = m.aabb();
    assert!((lo.x).abs() < 1e-4 && (hi.x - 50.0).abs() < 1e-4, "{lo:?} {hi:?}");
    assert!((hi.y - 2.0).abs() < 1e-4 && (lo.y + 2.0).abs() < 1e-4, "width 4 centred: {lo:?} {hi:?}");
    // The tip is 10 % of the base (taper 0.9).
    let tip: Vec<_> = m.vertices().iter().filter(|v| (v.position.x - 50.0).abs() < 1e-4).collect();
    assert_eq!(tip.len(), 4);
    assert!(tip.iter().all(|v| (v.position.y.abs() - 0.2).abs() < 1e-4), "{:?}", tip.iter().map(|v| v.position).collect::<Vec<_>>());
    assert_eq!(m.face_count(), 6);
    let mut with_fins = ObjectKind::bone(50.0);
    with_fins.set_parameter("Side Fins", 1.0).unwrap();
    assert!(with_fins.mesh().face_count() > 6, "fins add faces");
    assert_eq!(kind.base_name(), "Bone");
    assert!(kind.is_bone());
    assert!(!kind.is_helper());
}

#[test]
fn helpers_are_helpers_with_their_sizes() {
    let d = ObjectKind::dummy(10.0);
    assert!(d.is_helper());
    let (lo, hi) = d.mesh().aabb();
    assert!((hi.x - lo.x - 10.0).abs() < 1e-4);
    assert_eq!(d.base_name(), "Dummy");
    let p = ObjectKind::point(20.0);
    assert!(p.is_helper());
    assert_eq!(p.base_name(), "Point");
    assert!(ObjectKind::Group { open: true }.is_helper());
    assert!(ObjectKind::IkGoal { chain: 1 }.is_helper());
    let mut s = Scene::new();
    let id = s.add_node(Node::new("Bone001", ObjectKind::bone(10.0)));
    assert!(s.node(id).unwrap().object.is_bone());
}
