use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp, SelectionMode, ShapeKind};
use loom_scene::{Modifier, NodeId};

fn shape(s: &mut AppState, kind: ShapeKind, size: [f32; 3]) -> NodeId {
    execute(s, Command::CreateShape { kind, position: [0.0; 3], size }).unwrap();
    *s.selection.iter().next().unwrap()
}

fn add(s: &mut AppState, name: &str) {
    execute(s, Command::AddModifier { name: name.into() }).unwrap();
}

fn set(s: &mut AppState, index: u32, name: &str, value: f64) {
    execute(s, Command::SetModifierParameter { index, name: name.into(), value }).unwrap();
}

#[test]
fn extrude_lathe_bevel_and_sweep_turn_shapes_into_meshes() {
    let mut s = AppState::new();
    let r = shape(&mut s, ShapeKind::Rectangle, [10.0, 20.0, 0.0]);
    add(&mut s, "Extrude");
    set(&mut s, 0, "Amount", 5.0);
    let m = s.scene.eval_mesh(r).value.mesh.clone();
    m.check_invariants().unwrap();
    assert_eq!(m.face_count(), 4 + 2 + 2);
    assert!((m.aabb().1.z - 5.0).abs() < 1e-5);
    assert!(s.scene.eval_mesh(r).warnings.is_empty());
    set(&mut s, 0, "Segments", 3.0);
    set(&mut s, 0, "Cap End", 0.0);
    assert_eq!(s.scene.eval_mesh(r).value.mesh.face_count(), 12 + 2);
    // Lathe a line profile 360° about Y (Min alignment) into a closed can.
    execute(&mut s, Command::LineStart { point: [0.0, 0.0, 0.0] }).unwrap();
    let line = *s.selection.iter().next().unwrap();
    for p in [[5.0, 0.0, 0.0], [5.0, 10.0, 0.0], [0.0, 10.0, 0.0]] {
        execute(&mut s, Command::LineAddKnot { node: line.0, point: p, smooth: false }).unwrap();
    }
    add(&mut s, "Lathe");
    set(&mut s, 0, "Direction", 1.0);
    set(&mut s, 0, "Align", 0.0);
    set(&mut s, 0, "Segments", 16.0);
    set(&mut s, 0, "Weld Core", 1.0);
    let l = s.scene.eval_mesh(line).value.mesh.clone();
    l.check_invariants().unwrap();
    assert!(loom_mesh::select_ops::border_edges(&l).is_empty(), "welded core closes the can");
    assert_eq!(l.face_count(), 48);
    set(&mut s, 0, "Degrees", 180.0);
    assert_eq!(s.scene.eval_mesh(line).value.mesh.face_count(), 48, "a half turn keeps 16 segments");
    // Bevel: three levels over a circle.
    let c = shape(&mut s, ShapeKind::Circle, [10.0, 10.0, 0.0]);
    add(&mut s, "Bevel");
    set(&mut s, 0, "Level 1 Height", 2.0);
    set(&mut s, 0, "Level 1 Outline", 1.0);
    set(&mut s, 0, "Level 2", 1.0);
    set(&mut s, 0, "Level 2 Height", 1.0);
    let b = s.scene.eval_mesh(c).value.mesh.clone();
    b.check_invariants().unwrap();
    assert!((b.aabb().1.z - 3.0).abs() < 1e-5, "{:?}", b.aabb());
    assert!(b.aabb().1.x > 5.9, "level 1 outline grows the circle: {:?}", b.aabb());
    // Sweep a Bar section along an arc.
    let a = shape(&mut s, ShapeKind::Arc, [20.0, 20.0, 0.0]);
    add(&mut s, "Sweep");
    set(&mut s, 0, "Section", 1.0);
    set(&mut s, 0, "Length", 2.0);
    set(&mut s, 0, "Width", 4.0);
    let sw = s.scene.eval_mesh(a).value.mesh.clone();
    sw.check_invariants().unwrap();
    assert!(loom_mesh::select_ops::border_edges(&sw).is_empty(), "capped ends");
    assert!(sw.face_count() > 8);
    // Shape modifiers need a shape: a box refuses them with a message.
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    let err = execute(&mut s, Command::AddModifier { name: "Lathe".into() }).unwrap_err();
    assert!(err.contains("shape"), "{err}");
}

#[test]
fn edit_spline_modifier_edits_the_shape_below_and_feeds_extrude() {
    let mut s = AppState::new();
    let c = shape(&mut s, ShapeKind::Circle, [10.0, 10.0, 0.0]);
    add(&mut s, "Edit Spline");
    let node = s.scene.node(c).unwrap();
    assert!(matches!(node.modifiers[0].modifier, Modifier::EditSpline { .. }));
    assert!(matches!(node.object, loom_scene::ObjectKind::Circle { .. }), "the circle stays parametric");
    assert!(s.spline_selected(), "the spline tools are live on an Edit Spline entry");
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Edge }).unwrap();
    execute(&mut s, Command::SplineRefine { node: c.0, segment: 0, t: 0.5 }).unwrap();
    assert_eq!(s.scene.node(c).unwrap().edit_shape().unwrap().splines[0].knots.len(), 5);
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Vertex }).unwrap();
    execute(&mut s, Command::SelectSub { node: None, ids: vec![1], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 4.0], system: None }).unwrap();
    let sh = s.scene.node(c).unwrap().edit_shape().unwrap();
    assert!((sh.splines[0].knots[1].pos.z - 4.0).abs() < 1e-5);
    // The edit flows up: Extrude above it uses the edited shape (5 knots → more sides).
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Object }).unwrap();
    add(&mut s, "Extrude");
    set(&mut s, 1, "Amount", 3.0);
    let e = s.scene.eval_mesh(c);
    assert!(e.warnings.is_empty(), "{:?}", e.warnings);
    assert!(e.value.mesh.face_count() > 24);
    // A radius change below carries through the Edit Spline's deltas.
    execute(&mut s, Command::SetParameter { id: c.0, name: "Radius".into(), value: 8.0 }).unwrap();
    let e2 = s.scene.eval_mesh(c);
    assert!(e2.warnings.is_empty());
    assert!(e2.value.mesh.aabb().1.x > 7.5, "{:?}", e2.value.mesh.aabb());
}
