//! Phase 9 deformers: Affect Region, PathDeform (+ WSM), Vertex Weld,
//! Substitute, Flex and Spline IK Control.

use glam::Vec3;
use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp, SelectionMode, ShapeKind};
use loom_scene::NodeId;

fn prim(s: &mut AppState, kind: PrimitiveKind, position: [f32; 3], size: [f32; 3]) -> NodeId {
    execute(s, Command::CreatePrimitive { kind, position, size }).unwrap();
    *s.selection.iter().next().unwrap()
}

fn select(s: &mut AppState, id: NodeId) {
    execute(s, Command::SelectNodes { ids: vec![id.0], op: SelectOp::Replace }).unwrap();
}

fn add(s: &mut AppState, name: &str) {
    execute(s, Command::AddModifier { name: name.into() }).unwrap();
}

fn set(s: &mut AppState, index: u32, name: &str, value: f64) {
    execute(s, Command::SetModifierParameter { index, name: name.into(), value }).unwrap();
}

fn verts(s: &AppState, id: NodeId) -> Vec<Vec3> {
    s.scene.eval_mesh(id).value.mesh.vertices().iter().map(|v| v.position).collect()
}

fn centroid(p: &[Vec3]) -> Vec3 {
    p.iter().copied().sum::<Vec3>() / p.len() as f32
}

#[test]
fn affect_region_pulls_neighbours() {
    let mut s = AppState::new();
    let p = prim(&mut s, PrimitiveKind::Plane, [0.0; 3], [100.0, 100.0, 0.0]);
    let before = verts(&s, p);
    add(&mut s, "Affect Region");
    set(&mut s, 0, "Falloff", 30.0);
    let after = verts(&s, p);
    let centre = before.iter().position(|v| v.length() < 1e-3).expect("the plane has a centre vertex");
    // The end point starts 20 units up: the centre vertex rides it fully.
    assert!((after[centre].z - 20.0).abs() < 1e-3, "{}", after[centre]);
    for (b, a) in before.iter().zip(&after) {
        let d = b.length();
        if d > 30.0 {
            assert!((a.z - b.z).abs() < 1e-5, "outside the falloff: {b} -> {a}");
        } else if d > 1.0 {
            assert!(a.z > 0.0 && a.z < 20.0, "partly pulled: {b} -> {a}");
        }
    }
    // Moving the points at the Points level moves the end point.
    execute(&mut s, Command::ModifierSubobjectLevel { index: 0, level: 1 }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 10.0], system: None }).unwrap();
    assert!((verts(&s, p)[centre].z - 30.0).abs() < 1e-3);
    // Ignore Back Facing: vertices facing away from the pull stay.
    let mut t = AppState::new();
    let sp = prim(&mut t, PrimitiveKind::Sphere, [0.0; 3], [20.0, 20.0, 20.0]);
    let base = verts(&t, sp);
    add(&mut t, "Affect Region");
    set(&mut t, 0, "Falloff", 50.0);
    set(&mut t, 0, "Ignore Back Facing", 1.0);
    let moved = verts(&t, sp);
    let bottom = base.iter().enumerate().min_by(|a, b| a.1.z.total_cmp(&b.1.z)).unwrap().0;
    let top = base.iter().enumerate().max_by(|a, b| a.1.z.total_cmp(&b.1.z)).unwrap().0;
    assert!((moved[bottom] - base[bottom]).length() < 1e-5, "the bottom faces away");
    assert!(moved[top].z > base[top].z + 1.0, "the top follows");
}

#[test]
fn pathdeform_wraps_on_closed_paths() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreateShape { kind: ShapeKind::Circle, position: [0.0; 3], size: [100.0, 100.0, 0.0] }).unwrap();
    let circle = *s.selection.iter().next().unwrap();
    let b = prim(&mut s, PrimitiveKind::Box, [0.0; 3], [2.0, 2.0, 10.0]);
    execute(&mut s, Command::SetParameter { id: b.0, name: "Height Segs".into(), value: 5.0 }).unwrap();
    add(&mut s, "PathDeform");
    execute(&mut s, Command::PickModifierNode { index: 0, node: circle.0 }).unwrap();
    set(&mut s, 0, "Path Deform Axis", 2.0);
    // Percent 0: the box sits on the path start (+X), bent along it.
    let c0 = centroid(&verts(&s, b));
    assert!((c0.length() - 50.0).abs() < 1.5, "on the circle: {c0}");
    assert!(c0.x > 45.0, "at the start: {c0}");
    // Percent 25 is a quarter turn; 125 wraps to the same place on a closed path.
    set(&mut s, 0, "Percent", 25.0);
    let q = verts(&s, b);
    assert!(centroid(&q).y > 45.0, "a quarter around: {}", centroid(&q));
    set(&mut s, 0, "Percent", 125.0);
    let w = verts(&s, b);
    for (a, c) in q.iter().zip(&w) {
        assert!(a.distance(*c) < 1e-2, "wraps, not extrapolates: {a} vs {c}");
    }
    // Stretch spreads the box further along the path.
    set(&mut s, 0, "Stretch", 2.0);
    let spread = |p: &[Vec3]| p.iter().map(|v| v.distance(centroid(p))).fold(0.0f32, f32::max);
    assert!(spread(&verts(&s, b)) > spread(&w) * 1.6, "stretched");
    set(&mut s, 0, "Stretch", 1.0);
    // Twist turns the cross-section along the path (degrees over the whole
    // path's length: 1800° over ~314 units is about a radian over the box's 10).
    set(&mut s, 0, "Twist", 1800.0);
    let tw = verts(&s, b);
    assert!(tw.iter().zip(&w).any(|(a, c)| a.distance(*c) > 0.5), "twisted");
    let r = |v: &Vec3| (Vec3::new(v.x, v.y, 0.0).length() - 50.0).hypot(v.z);
    for (a, c) in tw.iter().zip(&w) {
        assert!((r(a) - r(c)).abs() < 0.05, "the section turns round the path, keeping its size: {a} {c}");
    }
    // Flip reverses the direction along the axis.
    set(&mut s, 0, "Twist", 0.0);
    set(&mut s, 0, "Flip", 1.0);
    assert!(verts(&s, b).iter().zip(&w).any(|(a, c)| a.distance(*c) > 0.5), "flipped");
    // The WSM version places the box where the path is in the world.
    let mut t = AppState::new();
    execute(&mut t, Command::CreateShape { kind: ShapeKind::Circle, position: [200.0, 0.0, 0.0], size: [100.0, 100.0, 0.0] }).unwrap();
    let c2 = *t.selection.iter().next().unwrap();
    let b2 = prim(&mut t, PrimitiveKind::Box, [0.0; 3], [2.0, 2.0, 10.0]);
    add(&mut t, "PathDeform (WSM)");
    let top = t.scene.node(b2).unwrap().modifiers.len() as u32 - 1;
    execute(&mut t, Command::PickModifierNode { index: top, node: c2.0 }).unwrap();
    t.scene.node(b2).unwrap();
    let world = t.scene.object_matrix(b2);
    let wc = world.transform_point3(centroid(&verts(&t, b2)));
    assert!((wc - Vec3::new(250.0, 0.0, 0.0)).length() < 6.0, "on the path in world space: {wc}");
}

#[test]
fn vertex_weld_merges() {
    let mut s = AppState::new();
    let b = prim(&mut s, PrimitiveKind::Box, [0.0; 3], [10.0; 3]);
    execute(&mut s, Command::ConvertToEditablePoly).unwrap();
    let mesh = s.scene.node(b).unwrap().object.mesh().into_owned();
    let p0 = mesh.vertex_position(loom_mesh::VertexId(0)).unwrap();
    // The nearest other vertex, then move vertex 0 to within 0.05 of it.
    let (other, q) = mesh.vertices().iter().enumerate().skip(1).map(|(i, v)| (i, v.position)).min_by(|a, b| a.1.distance(p0).total_cmp(&b.1.distance(p0))).unwrap();
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Vertex }).unwrap();
    execute(&mut s, Command::SelectSub { node: None, ids: vec![0], op: SelectOp::Replace }).unwrap();
    let d = (q - p0) * (1.0 - 0.05 / q.distance(p0));
    execute(&mut s, Command::MoveSelection { delta: d.to_array(), system: None }).unwrap();
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Object }).unwrap();
    let _ = other;
    add(&mut s, "Vertex Weld");
    set(&mut s, 0, "Threshold", 0.01);
    assert_eq!(s.scene.eval_mesh(b).value.mesh.vertex_count(), 8);
    set(&mut s, 0, "Threshold", 0.1);
    let m = s.scene.eval_mesh(b).value.mesh.clone();
    assert_eq!(m.vertex_count(), 7, "the close pair welded");
    assert!(m.face_count() >= 5, "faces survive: {}", m.face_count());
}

#[test]
fn substitute_uses_the_picked_mesh() {
    let mut s = AppState::new();
    let sphere = prim(&mut s, PrimitiveKind::Sphere, [30.0, 0.0, 0.0], [10.0; 3]);
    let b = prim(&mut s, PrimitiveKind::Box, [0.0; 3], [10.0; 3]);
    add(&mut s, "Substitute");
    execute(&mut s, Command::PickModifierNode { index: 0, node: sphere.0 }).unwrap();
    let want = s.scene.node(sphere).unwrap().object.mesh().into_owned();
    let got = s.scene.eval_mesh(b).value.mesh.clone();
    assert_eq!(got.vertex_count(), want.vertex_count());
    assert_eq!(got.face_count(), want.face_count());
    // In the box's own space: the sphere's mesh, not moved to where the sphere is.
    let c = centroid(&verts(&s, b));
    assert!(c.length() < 1e-2, "{c}");
}

#[test]
fn flex_lags_behind_motion() {
    let mut s = AppState::new();
    let c = prim(&mut s, PrimitiveKind::Cylinder, [0.0; 3], [4.0, 4.0, 40.0]);
    execute(&mut s, Command::SetParameter { id: c.0, name: "Height Segments".into(), value: 8.0 }).unwrap();
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::SetFrame { frame: 10 }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [50.0, 0.0, 0.0], system: None }).unwrap();
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::SetFrame { frame: 0 }).unwrap();
    let rest = verts(&s, c);
    add(&mut s, "Flex");
    let at0 = verts(&s, c);
    for (a, b) in rest.iter().zip(&at0) {
        assert!(a.distance(*b) < 1e-4, "no motion yet: {a} {b}");
    }
    execute(&mut s, Command::SetFrame { frame: 5 }).unwrap();
    let mid = verts(&s, c);
    let top = rest.iter().enumerate().max_by(|a, b| a.1.z.total_cmp(&b.1.z)).unwrap().0;
    let bottom = rest.iter().enumerate().min_by(|a, b| a.1.z.total_cmp(&b.1.z)).unwrap().0;
    assert!(mid[top].x < rest[top].x - 0.5, "the top lags behind the move: {} vs {}", mid[top], rest[top]);
    assert!((mid[bottom].x - rest[bottom].x).abs() < (mid[top].x - rest[top].x).abs(), "the pivot end flexes least");
    // Flex 0 turns it off.
    set(&mut s, 0, "Flex", 0.0);
    for (a, b) in rest.iter().zip(&verts(&s, c)) {
        assert!(a.distance(*b) < 1e-4);
    }
}

#[test]
fn spline_ik_control_helpers_move_knots() {
    let mut s = AppState::new();
    let r = execute(&mut s, Command::LineStart { point: [0.0, 0.0, 0.0] }).unwrap();
    let id = r["id"].as_u64().unwrap();
    for p in [[10.0, 0.0, 0.0], [20.0, 0.0, 0.0]] {
        execute(&mut s, Command::LineAddKnot { node: id, point: p, smooth: false }).unwrap();
    }
    execute(&mut s, Command::LineEnd { node: id }).unwrap();
    let line = NodeId(id);
    select(&mut s, line);
    add(&mut s, "Spline IK Control");
    set(&mut s, 0, "Link Types", 0.0);
    execute(&mut s, Command::SplineIkControlCreateHelpers).unwrap();
    let helpers: Vec<NodeId> = match &s.scene.node(line).unwrap().modifiers[0].modifier {
        loom_scene::Modifier::SplineIkControl(d) => d.helpers.iter().map(|h| h.unwrap()).collect(),
        other => panic!("{other:?}"),
    };
    assert_eq!(helpers.len(), 3, "one helper per knot");
    let shape = |s: &AppState| s.scene.eval_mesh(line).value.shape.clone().unwrap();
    let world = |s: &AppState| s.scene.object_matrix(line);
    // Link All in Hierarchy: moving the first helper drags every knot.
    select(&mut s, helpers[0]);
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 5.0], system: None }).unwrap();
    let k: Vec<Vec3> = shape(&s).knot_positions().iter().map(|p| world(&s).transform_point3(*p)).collect();
    assert!(k.iter().all(|p| (p.z - 5.0).abs() < 1e-3), "{k:?}");
    // Moving the last helper moves only its knot.
    select(&mut s, helpers[2]);
    execute(&mut s, Command::MoveSelection { delta: [0.0, 10.0, 0.0], system: None }).unwrap();
    let k: Vec<Vec3> = shape(&s).knot_positions().iter().map(|p| world(&s).transform_point3(*p)).collect();
    assert!((k[2] - Vec3::new(20.0, 10.0, 5.0)).length() < 1e-3, "{k:?}");
    assert!((k[1] - Vec3::new(10.0, 0.0, 5.0)).length() < 1e-3, "{k:?}");
}

#[test]
fn wsm_stays_on_top() {
    let mut s = AppState::new();
    let b = prim(&mut s, PrimitiveKind::Box, [0.0; 3], [10.0; 3]);
    add(&mut s, "Bend");
    add(&mut s, "PathDeform (WSM)");
    // A world-space modifier sits above every object-space one, whatever is active.
    execute(&mut s, Command::SelectStackEntry { index: Some(0) }).unwrap();
    add(&mut s, "Taper");
    let names: Vec<String> = s.scene.node(b).unwrap().modifiers.iter().map(|m| m.modifier.name().to_string()).collect();
    assert_eq!(names, ["Bend", "Taper", "PathDeform (WSM)"]);
    // Adding with the WSM active still goes below it.
    let top = s.scene.node(b).unwrap().modifiers.len() as u32 - 1;
    execute(&mut s, Command::SelectStackEntry { index: Some(top) }).unwrap();
    add(&mut s, "Twist");
    let names: Vec<String> = s.scene.node(b).unwrap().modifiers.iter().map(|m| m.modifier.name().to_string()).collect();
    assert_eq!(names.last().unwrap(), "PathDeform (WSM)");
}

#[test]
fn wsm_cannot_move_below_object_space() {
    let mut s = AppState::new();
    prim(&mut s, PrimitiveKind::Box, [0.0; 3], [10.0; 3]);
    add(&mut s, "Bend");
    add(&mut s, "PathDeform (WSM)");
    assert!(execute(&mut s, Command::MoveModifier { from: 1, to: 0 }).is_err(), "a WSM stays above");
    assert!(execute(&mut s, Command::MoveModifier { from: 0, to: 1 }).is_err(), "an object-space modifier stays below");
}

#[test]
fn paste_instanced_keeps_wsm_on_top() {
    let mut s = AppState::new();
    let a = prim(&mut s, PrimitiveKind::Box, [0.0; 3], [10.0; 3]);
    add(&mut s, "Bend");
    let b = prim(&mut s, PrimitiveKind::Box, [20.0, 0.0, 0.0], [10.0; 3]);
    add(&mut s, "PathDeform (WSM)");
    // The WSM is active; Paste Instanced of a's Bend still goes below it.
    execute(&mut s, Command::InstanceModifier { source: a.0, index: 0 }).unwrap();
    let names: Vec<String> = s.scene.node(b).unwrap().modifiers.iter().map(|m| m.modifier.name().to_string()).collect();
    assert_eq!(names, ["Bend", "PathDeform (WSM)"]);
}
