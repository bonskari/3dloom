//! Parameters rollouts of the standard primitives: names, defaults, ranges
//! and what they do to the generated mesh.

use loom_scene::{ObjectKind, ParamValue};

fn get(o: &ObjectKind, name: &str) -> ParamValue {
    o.parameters().into_iter().find(|(n, _)| *n == name).unwrap_or_else(|| panic!("no parameter {name}")).1
}

#[test]
fn round_primitives_have_smooth_on_by_default_and_off_facets_them() {
    for mut o in [
        ObjectKind::cone_object(10.0, 5.0, 20.0),
        ObjectKind::sphere_object(10.0),
        ObjectKind::geosphere_object(10.0),
        ObjectKind::cylinder_object(10.0, 20.0),
        ObjectKind::tube_object(10.0, 5.0, 20.0),
        ObjectKind::teapot_object(10.0),
    ] {
        assert_eq!(get(&o, "Smooth"), ParamValue::Bool(true), "{}", o.base_name());
        assert!(o.mesh().faces().iter().any(|f| f.smoothing_groups != 0));
        o.set_parameter("Smooth", 0.0).unwrap();
        assert!(o.mesh().faces().iter().all(|f| f.smoothing_groups == 0), "{} faceted", o.base_name());
    }
}

#[test]
fn cone_cylinder_and_tube_take_a_negative_height() {
    for mut o in [ObjectKind::cone_object(10.0, 5.0, 20.0), ObjectKind::cylinder_object(10.0, 20.0), ObjectKind::tube_object(10.0, 5.0, 20.0)] {
        o.set_parameter("Height", -15.0).unwrap();
        assert_eq!(get(&o, "Height"), ParamValue::Float(-15.0), "{}", o.base_name());
        let (min, max) = o.mesh().aabb();
        assert!((min.z + 15.0).abs() < 1e-4 && max.z.abs() < 1e-4, "{} below the plane: {min} {max}", o.base_name());
    }
}

#[test]
fn sphere_has_chop_squash_slice_and_base_to_pivot() {
    let mut o = ObjectKind::sphere_object(10.0);
    assert_eq!(get(&o, "Chop/Squash"), ParamValue::Int(0), "Chop is the default");
    assert_eq!(o.choices("Chop/Squash"), Some(&["Chop", "Squash"][..]));
    assert_eq!(get(&o, "Slice On"), ParamValue::Bool(false));
    assert_eq!(get(&o, "Base To Pivot"), ParamValue::Bool(false));
    let full_verts = o.mesh().vertex_count();
    o.set_parameter("Hemisphere", 0.5).unwrap();
    o.set_parameter("Chop/Squash", 1.0).unwrap();
    assert_eq!(o.mesh().vertex_count(), full_verts, "squash keeps the vertex count");
    o.set_parameter("Hemisphere", 0.0).unwrap();
    o.set_parameter("Base To Pivot", 1.0).unwrap();
    let (min, max) = o.mesh().aabb();
    assert!(min.z.abs() < 1e-4 && (max.z - 20.0).abs() < 1e-4, "base to pivot lifts the sphere by its radius: {min} {max}");
    o.set_parameter("Base To Pivot", 0.0).unwrap();
    o.set_parameter("Slice On", 1.0).unwrap();
    o.set_parameter("Slice To", 180.0).unwrap();
    let (min, _) = o.mesh().aabb();
    assert!(min.y > -1e-3, "sliced to the +Y half: {min}");
}

#[test]
fn geosphere_base_type_is_a_choice_segments_cap_at_200_and_base_to_pivot_lifts() {
    let mut o = ObjectKind::geosphere_object(10.0);
    assert_eq!(o.choices("Geodesic Base Type"), Some(&["Tetra", "Octa", "Icosa"][..]));
    assert_eq!(get(&o, "Geodesic Base Type"), ParamValue::Int(2), "Icosa is the default");
    o.set_parameter("Segments", 500.0).unwrap();
    assert_eq!(get(&o, "Segments"), ParamValue::Int(200));
    o.set_parameter("Segments", 2.0).unwrap();
    assert_eq!(get(&o, "Base To Pivot"), ParamValue::Bool(false));
    o.set_parameter("Base To Pivot", 1.0).unwrap();
    let (min, _) = o.mesh().aabb();
    assert!(min.z.abs() < 1e-3, "pivot at the bottom: {min}");
}

#[test]
fn torus_smooth_is_all_sides_none_segments_with_all_the_default() {
    let mut o = ObjectKind::torus_object(20.0, 5.0);
    assert_eq!(o.choices("Smooth"), Some(&["All", "Sides", "None", "Segments"][..]));
    assert_eq!(get(&o, "Smooth"), ParamValue::Int(0));
    o.set_parameter("Smooth", 2.0).unwrap();
    assert!(o.mesh().faces().iter().all(|f| f.smoothing_groups == 0));
}

#[test]
fn documented_defaults() {
    let i = |o: &ObjectKind, n: &str| get(o, n);
    let b = ObjectKind::box_object(1.0, 1.0, 1.0);
    for n in ["Length Segs", "Width Segs", "Height Segs"] {
        assert_eq!(i(&b, n), ParamValue::Int(1), "box {n}");
    }
    let c = ObjectKind::cone_object(1.0, 0.0, 1.0);
    assert_eq!((i(&c, "Height Segments"), i(&c, "Cap Segments"), i(&c, "Sides")), (ParamValue::Int(5), ParamValue::Int(1), ParamValue::Int(24)));
    let c = ObjectKind::cylinder_object(1.0, 1.0);
    assert_eq!((i(&c, "Height Segments"), i(&c, "Cap Segments"), i(&c, "Sides")), (ParamValue::Int(5), ParamValue::Int(1), ParamValue::Int(18)));
    let t = ObjectKind::tube_object(1.0, 0.5, 1.0);
    assert_eq!((i(&t, "Height Segments"), i(&t, "Sides")), (ParamValue::Int(5), ParamValue::Int(18)));
    assert_eq!(i(&ObjectKind::sphere_object(1.0), "Segments"), ParamValue::Int(32));
    let t = ObjectKind::torus_object(1.0, 0.5);
    assert_eq!((i(&t, "Segments"), i(&t, "Sides")), (ParamValue::Int(24), ParamValue::Int(12)));
    let p = ObjectKind::plane_object(1.0, 1.0);
    assert_eq!((i(&p, "Length Segs"), i(&p, "Width Segs")), (ParamValue::Int(4), ParamValue::Int(4)));
}
