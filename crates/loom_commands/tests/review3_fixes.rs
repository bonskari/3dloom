use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp, SelectionMode, ShapeKind};
use loom_scene::{Modifier, NodeId};

fn boxed(s: &mut AppState, segs: u32) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0, 10.0, 10.0] }).unwrap();
    let id = *s.selection.iter().next().unwrap();
    for n in ["Length Segs", "Width Segs", "Height Segs"] {
        execute(s, Command::SetParameter { id: id.0, name: n.into(), value: segs as f64 }).unwrap();
    }
    id
}

fn set(s: &mut AppState, index: u32, name: &str, value: f64) {
    execute(s, Command::SetModifierParameter { index, name: name.into(), value }).unwrap();
}

#[test]
fn stale_selection_through_a_topology_changer_is_dropped_not_indexed() {
    let mut s = AppState::new();
    let a = boxed(&mut s, 3);
    execute(&mut s, Command::ConvertToEditablePoly).unwrap();
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Vertex }).unwrap();
    execute(&mut s, Command::SelectAll).unwrap();
    execute(&mut s, Command::AddModifier { name: "Slice".into() }).unwrap();
    set(&mut s, 0, "Slice Type", 2.0);
    execute(&mut s, Command::AddModifier { name: "Bend".into() }).unwrap();
    set(&mut s, 1, "Angle", 45.0);
    let e = s.scene.eval_mesh(a);
    assert!(e.value.mesh.vertex_count() > 0);
    execute(&mut s, Command::AddModifier { name: "Push".into() }).unwrap();
    set(&mut s, 2, "Push Value", 1.0);
    let _ = s.scene.eval_mesh(a);
    execute(&mut s, Command::AddModifier { name: "FFD 2x2x2".into() }).unwrap();
    let _ = s.scene.eval_mesh(a);
}

#[test]
fn extrude_along_a_one_knot_line_reports_and_a_short_line_is_an_error_not_a_panic() {
    let mut s = AppState::new();
    let b = boxed(&mut s, 1);
    execute(&mut s, Command::ConvertToEditablePoly).unwrap();
    execute(&mut s, Command::LineStart { point: [0.0, 0.0, 20.0] }).unwrap();
    let line = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![b.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Face }).unwrap();
    execute(&mut s, Command::SelectSub { node: None, ids: vec![0], op: SelectOp::Replace }).unwrap();
    let err = execute(&mut s, Command::ExtrudeAlongSpline { spline: line.0, segments: 4, taper: 0.0, taper_curve: 0.0, twist: 0.0, rotation: 0.0, align: true }).unwrap_err();
    assert!(err.contains("knots"), "{err}");
    // Ending a one-knot line removes it.
    execute(&mut s, Command::SelectNodes { ids: vec![line.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::LineEnd { node: line.0 }).unwrap();
    assert!(s.scene.node(line).is_none());
}

#[test]
fn divide_spaces_knots_evenly_and_hide_is_undoable() {
    let mut s = AppState::new();
    execute(&mut s, Command::LineStart { point: [0.0, 0.0, 0.0] }).unwrap();
    let line = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::LineAddKnot { node: line.0, point: [9.0, 0.0, 0.0], smooth: false }).unwrap();
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Edge }).unwrap();
    execute(&mut s, Command::SelectSub { node: None, ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SplineDivide { n: 2 }).unwrap();
    let sh = s.scene.node(line).unwrap().edit_shape().unwrap();
    let xs: Vec<f32> = sh.splines[0].knots.iter().map(|k| k.pos.x).collect();
    assert!((xs[1] - 3.0).abs() < 1e-2 && (xs[2] - 6.0).abs() < 1e-2, "{xs:?}");
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Face }).unwrap();
    execute(&mut s, Command::SelectSub { node: None, ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::HideSpline).unwrap();
    assert!(s.scene.node(line).unwrap().edit_spline().unwrap().hidden.contains(&0));
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(s.scene.node(line).unwrap().edit_spline().unwrap().hidden.is_empty(), "Hide is one undo entry");
}

#[test]
fn collapse_on_an_object_instance_is_one_undo_entry_and_lathe_clamps_degrees() {
    let mut s = AppState::new();
    let a = boxed(&mut s, 1);
    execute(&mut s, Command::AddModifier { name: "Bend".into() }).unwrap();
    execute(&mut s, Command::CloneOptions { kind: loom_commands::CloneKind::Instance, copies: 1, name: None, offset: Some([20.0, 0.0, 0.0]), rotate_deg: None, scale: None }).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    let before = s.undo.labels().len();
    execute(&mut s, Command::CollapseAll).unwrap();
    assert_eq!(s.undo.labels().len(), before + 1, "{:?}", s.undo.labels());
    execute(&mut s, Command::CreateShape { kind: ShapeKind::Circle, position: [0.0; 3], size: [10.0, 10.0, 0.0] }).unwrap();
    let c = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::AddModifier { name: "Lathe".into() }).unwrap();
    set(&mut s, 0, "Degrees", 720.0);
    let Modifier::Lathe { degrees, .. } = &s.scene.node(c).unwrap().modifiers[0].modifier else { panic!() };
    assert_eq!(degrees.get(), 360.0);
}

#[test]
fn stack_edits_on_a_source_keep_the_instances_pointing_at_the_same_modifier() {
    let mut s = AppState::new();
    let a = boxed(&mut s, 1);
    execute(&mut s, Command::AddModifier { name: "Normal".into() }).unwrap();
    set(&mut s, 0, "Flip Normals", 1.0);
    execute(&mut s, Command::AddModifier { name: "Smooth".into() }).unwrap();
    let b = boxed(&mut s, 1);
    execute(&mut s, Command::InstanceModifier { source: a.0, index: 0 }).unwrap();
    let flipped = |s: &AppState, id: NodeId| s.scene.eval_mesh(id).value.mesh.face_normal(loom_mesh::FaceId(0)).z;
    let before = flipped(&s, b);
    // Inserting below, moving and collapsing the source keep B on the Normal modifier.
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SelectStackEntry { index: None }).unwrap();
    execute(&mut s, Command::AddModifier { name: "Push".into() }).unwrap();
    assert_eq!(s.scene.node(b).unwrap().modifiers[0].instance_of, Some((a, 1)), "the instance follows the insert");
    execute(&mut s, Command::MoveModifier { from: 1, to: 2 }).unwrap();
    assert_eq!(s.scene.node(b).unwrap().modifiers[0].instance_of, Some((a, 2)));
    assert_eq!(flipped(&s, b), before);
    execute(&mut s, Command::CollapseTo { index: 2 }).unwrap();
    let slot = &s.scene.node(b).unwrap().modifiers[0];
    assert_eq!(slot.instance_of, None, "a collapsed source leaves B its own copy");
    assert!(matches!(slot.modifier, Modifier::Normal { .. }));
    assert_eq!(flipped(&s, b), before);
    // Deleting a source entry gives the instance its own copy too.
    let c = boxed(&mut s, 1);
    execute(&mut s, Command::AddModifier { name: "Bend".into() }).unwrap();
    set(&mut s, 0, "Angle", 30.0);
    let d = boxed(&mut s, 1);
    execute(&mut s, Command::InstanceModifier { source: c.0, index: 0 }).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![c.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::DeleteModifier { index: 0 }).unwrap();
    let slot = &s.scene.node(d).unwrap().modifiers[0];
    assert_eq!(slot.instance_of, None);
    assert!(matches!(&slot.modifier, Modifier::Bend { angle, .. } if (angle.get() - 30.0).abs() < 1e-6));
}

#[test]
fn gizmo_edits_belong_to_each_instance_node() {
    let mut s = AppState::new();
    let a = boxed(&mut s, 1);
    execute(&mut s, Command::AddModifier { name: "XForm".into() }).unwrap();
    let b = boxed(&mut s, 1);
    execute(&mut s, Command::InstanceModifier { source: a.0, index: 0 }).unwrap();
    execute(&mut s, Command::ModifierSubobjectLevel { index: 0, level: 1 }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [5.0, 0.0, 0.0], system: None }).unwrap();
    assert!((s.scene.eval_mesh(b).value.mesh.aabb().0.x - 0.0).abs() < 1e-4, "B's gizmo moved B: {:?}", s.scene.eval_mesh(b).value.mesh.aabb());
    assert!((s.scene.eval_mesh(a).value.mesh.aabb().0.x + 5.0).abs() < 1e-4, "A stays");
}

#[test]
fn moving_an_object_does_not_re_evaluate_its_stack() {
    let mut s = AppState::new();
    let a = boxed(&mut s, 1);
    execute(&mut s, Command::AddModifier { name: "TurboSmooth".into() }).unwrap();
    let _ = s.scene.eval_mesh(a);
    let n = s.scene.eval_count.get();
    for _ in 0..5 {
        execute(&mut s, Command::MoveSelection { delta: [1.0, 0.0, 0.0], system: None }).unwrap();
        let _ = s.scene.eval_mesh(a);
    }
    assert_eq!(s.scene.eval_count.get(), n, "transforms do not touch the stack result");
    execute(&mut s, Command::SelectStackEntry { index: Some(0) }).unwrap();
    let _ = s.scene.eval_mesh(a);
    assert_eq!(s.scene.eval_count.get(), n);
}

#[test]
fn edit_poly_deltas_follow_vertices_after_its_own_deletes() {
    let mut s = AppState::new();
    let a = boxed(&mut s, 3);
    execute(&mut s, Command::AddModifier { name: "Bend".into() }).unwrap();
    execute(&mut s, Command::AddModifier { name: "Edit Poly".into() }).unwrap();
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Vertex }).unwrap();
    execute(&mut s, Command::SelectSub { node: None, ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::DeleteObjects).unwrap();
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Object }).unwrap();
    set(&mut s, 0, "Angle", 90.0);
    let bent = s.scene.evaluated(a, Some(0)).value.mesh.clone();
    let out = s.scene.eval_mesh(a).value.mesh.clone();
    // Every output vertex sits on a bent input vertex (the mapping survived the delete).
    for v in out.vertices() {
        let d = bent.vertices().iter().map(|b| b.position.distance(v.position)).fold(f32::INFINITY, f32::min);
        assert!(d < 1e-3, "{d}");
    }
}

#[test]
fn compound_results_are_cached_and_self_intersecting_outlines_still_build() {
    let mut s = AppState::new();
    let a = boxed(&mut s, 1);
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [3.0, 0.0, 0.0], size: [6.0; 3] }).unwrap();
    let b = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::CreateBoolean { operand_b: b.0, operation: 2, move_operand: true }).unwrap();
    let node = s.scene.node(a).unwrap();
    let t = std::time::Instant::now();
    for _ in 0..50 {
        let _ = node.object.mesh();
        let _ = node.object.compound_error();
    }
    assert!(t.elapsed().as_millis() < 500, "cached compound reads: {:?}", t.elapsed());
    // A figure-8 extrudes (uncapped where the outline crosses itself).
    execute(&mut s, Command::LineStart { point: [0.0, 0.0, 0.0] }).unwrap();
    let l = *s.selection.iter().next().unwrap();
    for p in [[10.0, 10.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0]] {
        execute(&mut s, Command::LineAddKnot { node: l.0, point: p, smooth: false }).unwrap();
    }
    execute(&mut s, Command::LineClose { node: l.0 }).unwrap();
    execute(&mut s, Command::AddModifier { name: "Extrude".into() }).unwrap();
    set(&mut s, 0, "Amount", 2.0);
    let e = s.scene.eval_mesh(l);
    assert!(e.warnings.is_empty(), "{:?}", e.warnings);
    e.value.mesh.check_invariants().unwrap();
    assert!(e.value.mesh.face_count() >= 4);
}

#[test]
fn normal_has_no_dead_unify_switch_and_displace_decays() {
    let mut s = AppState::new();
    let a = boxed(&mut s, 1);
    execute(&mut s, Command::AddModifier { name: "Normal".into() }).unwrap();
    let names: Vec<&str> = s.scene.node(a).unwrap().modifiers[0].modifier.parameters().into_iter().map(|(n, _)| n).collect();
    assert_eq!(names, vec!["Flip Normals"], "a manifold mesh is always unified");
    execute(&mut s, Command::DeleteModifier { index: 0 }).unwrap();
    execute(&mut s, Command::AddModifier { name: "Displace".into() }).unwrap();
    set(&mut s, 0, "Strength", 2.0);
    let flat = s.scene.eval_mesh(a).value.mesh.aabb();
    set(&mut s, 0, "Decay", 1.0);
    let decayed = s.scene.eval_mesh(a).value.mesh.aabb();
    assert!(decayed.1.x < flat.1.x, "decay weakens the far vertices: {flat:?} {decayed:?}");
    for name in ["Generate Mapping Coords", "Use Shape IDs", "Prevent Indirect Smoothing", "Constant Cross-Section"] {
        assert!(execute(&mut s, Command::SetModifierParameter { index: 0, name: name.into(), value: 1.0 }).is_err(), "{name} is gone");
    }
}
