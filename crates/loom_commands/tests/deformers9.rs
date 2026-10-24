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

fn line(s: &mut AppState, pts: &[[f32; 3]]) -> NodeId {
    let r = execute(s, Command::LineStart { point: pts[0] }).unwrap();
    let id = r["id"].as_u64().unwrap();
    for p in &pts[1..] {
        execute(s, Command::LineAddKnot { node: id, point: *p, smooth: false }).unwrap();
    }
    execute(s, Command::LineEnd { node: id }).unwrap();
    select(s, NodeId(id));
    NodeId(id)
}

/// The helpers of the shape's Spline IK Control at stack entry 0.
fn ik_helpers(s: &AppState, id: NodeId) -> Vec<Option<NodeId>> {
    match &s.scene.node(id).unwrap().modifiers[0].modifier {
        loom_scene::Modifier::SplineIkControl(d) => d.helpers.clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn pathdeform_follows_the_paths_modifier_stack() {
    let mut s = AppState::new();
    let path = line(&mut s, &[[0.0, 0.0, 0.0], [100.0, 0.0, 0.0]]);
    // The path's stack bends its end up: a Spline IK Control helper moved 100 along Y.
    add(&mut s, "Spline IK Control");
    set(&mut s, 0, "Link Types", 2.0);
    execute(&mut s, Command::SplineIkControlCreateHelpers).unwrap();
    let end = ik_helpers(&s, path)[1].unwrap();
    select(&mut s, end);
    execute(&mut s, Command::MoveSelection { delta: [0.0, 100.0, 0.0], system: None }).unwrap();
    let b = prim(&mut s, PrimitiveKind::Box, [0.0; 3], [2.0, 2.0, 2.0]);
    add(&mut s, "PathDeform");
    execute(&mut s, Command::PickModifierNode { index: 0, node: path.0 }).unwrap();
    set(&mut s, 0, "Path Deform Axis", 2.0);
    set(&mut s, 0, "Percent", 50.0);
    // Halfway along the evaluated path (0,0)→(100,100), not the base line's
    // (50,0), in the path's own space (the box sits at the world origin).
    let local = |s: &AppState, p: Vec3| s.scene.object_matrix(path).inverse().transform_point3(p);
    let c = centroid(&verts(&s, b));
    assert!((c - local(&s, Vec3::new(50.0, 50.0, 0.0))).length() < 3.0, "on the evaluated path: {c}");
    // Moving the helper again re-evaluates the deformed box.
    select(&mut s, end);
    execute(&mut s, Command::MoveSelection { delta: [0.0, -100.0, 0.0], system: None }).unwrap();
    let c = centroid(&verts(&s, b));
    assert!((c - local(&s, Vec3::new(50.0, 0.0, 0.0))).length() < 3.0, "follows the path's change: {c}");
}

#[test]
fn pathdeform_cycle_does_not_hang() {
    let mut s = AppState::new();
    let a = line(&mut s, &[[0.0, 0.0, 0.0], [100.0, 0.0, 0.0]]);
    let c = line(&mut s, &[[0.0, 50.0, 0.0], [100.0, 50.0, 0.0]]);
    select(&mut s, a);
    add(&mut s, "PathDeform");
    execute(&mut s, Command::PickModifierNode { index: 0, node: c.0 }).unwrap();
    select(&mut s, c);
    add(&mut s, "PathDeform");
    execute(&mut s, Command::PickModifierNode { index: 0, node: a.0 }).unwrap();
    // Each path is being evaluated when the other asks for it: a warning, no recursion.
    let _ = s.scene.eval_mesh(a);
    let _ = s.scene.eval_mesh(c);
}

#[test]
fn pathdeform_rotation_turns_the_section() {
    let mut s = AppState::new();
    let path = line(&mut s, &[[0.0, 0.0, 0.0], [100.0, 0.0, 0.0]]);
    // A flat box: 2 across X, 10 across Y, 20 along Z (the deform axis).
    let b = prim(&mut s, PrimitiveKind::Box, [0.0; 3], [2.0, 10.0, 20.0]);
    add(&mut s, "PathDeform");
    execute(&mut s, Command::PickModifierNode { index: 0, node: path.0 }).unwrap();
    set(&mut s, 0, "Path Deform Axis", 2.0);
    let ext = |s: &AppState| {
        let (lo, hi) = s.scene.eval_mesh(b).value.mesh.aabb();
        hi - lo
    };
    let e0 = ext(&s);
    assert!((e0.x - 20.0).abs() < 1e-2, "laid along the path: {e0}");
    let mut section = [e0.y, e0.z];
    section.sort_by(f32::total_cmp);
    assert!((section[0] - 2.0).abs() < 1e-2 && (section[1] - 10.0).abs() < 1e-2, "the 2 x 10 section: {e0}");
    // Rotation 90: the section turns a quarter round the path.
    set(&mut s, 0, "Rotation", 90.0);
    let e90 = ext(&s);
    assert!((e90.x - 20.0).abs() < 1e-2, "{e90}");
    assert!((e90.y - e0.z).abs() < 1e-2 && (e90.z - e0.y).abs() < 1e-2, "section turned: {e0} -> {e90}");
    // Rotation 45: the diagonal of the section.
    set(&mut s, 0, "Rotation", 45.0);
    let e45 = ext(&s);
    let want = (2.0f32 + 10.0) / 2f32.sqrt();
    assert!((e45.y - want).abs() < 1e-2 && (e45.z - want).abs() < 1e-2, "{e45}");
}

#[test]
fn spline_ik_control_keeps_helpers_per_spline() {
    let mut s = AppState::new();
    let a = line(&mut s, &[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [20.0, 0.0, 0.0]]);
    let other = line(&mut s, &[[0.0, 50.0, 0.0], [20.0, 50.0, 0.0]]);
    select(&mut s, a);
    execute(&mut s, Command::SplineAttach { node: other.0, reorient: false }).unwrap();
    add(&mut s, "Spline IK Control");
    set(&mut s, 0, "Link Types", 2.0);
    execute(&mut s, Command::SplineIkControlCreateHelpers).unwrap();
    let helpers: Vec<NodeId> = ik_helpers(&s, a).into_iter().map(|h| h.unwrap()).collect();
    assert_eq!(helpers.len(), 5);
    // Lift the second spline's helpers.
    for h in &helpers[3..] {
        select(&mut s, *h);
        execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 7.0], system: None }).unwrap();
    }
    let world_spline = |s: &AppState, i: usize| -> Vec<Vec3> {
        let w = s.scene.object_matrix(a);
        s.scene.eval_mesh(a).value.shape.clone().unwrap().splines[i].knots.iter().map(|k| w.transform_point3(k.pos)).collect()
    };
    assert!(world_spline(&s, 1).iter().all(|p| (p.z - 7.0).abs() < 1e-3));
    // A knot added to the first spline below the modifier: the second
    // spline's knots still follow their own helpers, nothing shifts across.
    execute(&mut s, Command::SplineRefine { node: a.0, segment: 0, t: 0.5 }).unwrap();
    let e = s.scene.eval_mesh(a);
    let sh = e.value.shape.clone().unwrap();
    assert_eq!(sh.splines[0].knots.len(), 4);
    let k1 = world_spline(&s, 1);
    assert!((k1[0] - Vec3::new(0.0, 50.0, 7.0)).length() < 1e-3, "{k1:?}");
    assert!((k1[1] - Vec3::new(20.0, 50.0, 7.0)).length() < 1e-3, "{k1:?}");
    assert!(world_spline(&s, 0).iter().all(|p| p.z.abs() < 1e-3), "no helper of spline 2 lands on spline 1");
    assert!(e.warnings.iter().any(|w| w.contains("Spline IK Control")), "the count change is reported: {:?}", e.warnings);
    // Undoing the refine: the mapping is whole again, no warning.
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    let e = s.scene.eval_mesh(a);
    assert!(!e.warnings.iter().any(|w| w.contains("Spline IK Control")), "{:?}", e.warnings);
}
