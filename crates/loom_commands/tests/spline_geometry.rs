//! Spline geometry (phase 9): Fillet/Chamfer, Trim/Extend, CrossSection,
//! Surface and Bevel Profile modifiers; Editable Spline Trim, Extend, Cross
//! Insert, Bind / Unbind, Cross Section, tangent Copy / Paste, Soft Selection.

use glam::Vec3;
use loom_commands::{execute, AppState, Command, SelectOp, SelectionMode, ShapeKind};
use loom_scene::NodeId;

fn line(s: &mut AppState, pts: &[[f32; 3]]) -> NodeId {
    let r = execute(s, Command::LineStart { point: pts[0] }).unwrap();
    let id = r["id"].as_u64().unwrap();
    for p in &pts[1..] {
        execute(s, Command::LineAddKnot { node: id, point: *p, smooth: false }).unwrap();
    }
    execute(s, Command::LineEnd { node: id }).unwrap();
    execute(s, Command::SelectNodes { ids: vec![id], op: SelectOp::Replace }).unwrap();
    NodeId(id)
}

fn attach(s: &mut AppState, to: NodeId, other: NodeId) {
    execute(s, Command::SelectNodes { ids: vec![to.0], op: SelectOp::Replace }).unwrap();
    execute(s, Command::SplineAttach { node: other.0, reorient: false }).unwrap();
}

fn add(s: &mut AppState, name: &str) {
    execute(s, Command::AddModifier { name: name.into() }).unwrap();
}

fn set(s: &mut AppState, index: u32, name: &str, value: f64) {
    execute(s, Command::SetModifierParameter { index, name: name.into(), value }).unwrap();
}

/// Knot positions of an Editable Spline in world space.
fn world_knots(s: &AppState, id: NodeId) -> Vec<Vec3> {
    let m = s.scene.object_matrix(id);
    s.scene.node(id).unwrap().edit_shape().unwrap().knot_positions().iter().map(|p| m.transform_point3(*p)).collect()
}

/// World X of every knot of the stack's output shape.
fn world_xs(s: &AppState, id: NodeId) -> Vec<f32> {
    let m = s.scene.object_matrix(id);
    shape(s, id).splines.iter().flat_map(|sp| sp.knots.iter().map(|k| m.transform_point3(k.pos).x).collect::<Vec<_>>()).collect()
}

fn shape(s: &AppState, id: NodeId) -> loom_shape::Shape {
    s.scene.eval_mesh(id).value.shape.clone().expect("a shape flows out")
}

/// Two lines crossing like a plus sign, as one shape: -10..10 on X and on Y.
fn plus(s: &mut AppState) -> NodeId {
    let a = line(s, &[[-10.0, 0.0, 0.0], [10.0, 0.0, 0.0]]);
    let b = line(s, &[[0.0, -10.0, 0.0], [0.0, 10.0, 0.0]]);
    attach(s, a, b);
    a
}

#[test]
fn fillet_and_chamfer_corners() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreateShape { kind: ShapeKind::Rectangle, position: [0.0; 3], size: [20.0, 20.0, 0.0] }).unwrap();
    let r = *s.selection.iter().next().unwrap();
    add(&mut s, "Fillet/Chamfer");
    execute(&mut s, Command::SplineSelectSet { level: 1, ids: vec![0], op: SelectOp::Replace }).unwrap();
    set(&mut s, 0, "Radius", 3.0);
    execute(&mut s, Command::FilletChamferApply { fillet: true }).unwrap();
    assert_eq!(shape(&s, r).splines[0].knots.len(), 5);
    execute(&mut s, Command::SplineSelectSet { level: 1, ids: vec![3], op: SelectOp::Replace }).unwrap();
    set(&mut s, 0, "Distance", 2.0);
    execute(&mut s, Command::FilletChamferApply { fillet: false }).unwrap();
    assert_eq!(shape(&s, r).splines[0].knots.len(), 6);
    // The operations are stored: they replay when the rectangle below changes size.
    execute(&mut s, Command::SetParameter { id: r.0, name: "Length".into(), value: 40.0 }).unwrap();
    assert_eq!(shape(&s, r).splines[0].knots.len(), 6);
}

#[test]
fn trim_extend_edge_cases() {
    let mut s = AppState::new();
    let p = plus(&mut s);
    add(&mut s, "Trim/Extend");
    // Trim the right arm of the horizontal line: it ends at the crossing.
    execute(&mut s, Command::TrimExtendPick { point: [7.0, 0.0, 0.0], extend: false }).unwrap();
    let xs = world_xs(&s, p);
    assert!(xs.iter().all(|x| *x <= 1e-3), "{xs:?}");
    // Lines that only touch at end knots: nothing to trim, nothing breaks.
    let mut t = AppState::new();
    let a = line(&mut t, &[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]]);
    let b = line(&mut t, &[[10.0, 0.0, 0.0], [10.0, 10.0, 0.0]]);
    attach(&mut t, a, b);
    add(&mut t, "Trim/Extend");
    let before = shape(&t, a);
    execute(&mut t, Command::TrimExtendPick { point: [5.0, 0.0, 0.0], extend: false }).unwrap();
    assert_eq!(shape(&t, a), before, "end-to-end contact is not a crossing");
    // Extend a short line to meet a wall.
    let mut u = AppState::new();
    let c = line(&mut u, &[[0.0, 0.0, 0.0], [5.0, 0.0, 0.0]]);
    let wall = line(&mut u, &[[12.0, -10.0, 0.0], [12.0, 10.0, 0.0]]);
    attach(&mut u, c, wall);
    add(&mut u, "Trim/Extend");
    execute(&mut u, Command::TrimExtendPick { point: [4.0, 0.0, 0.0], extend: true }).unwrap();
    let n0 = shape(&u, c).splines[0].knots.len();
    let end = world_xs(&u, c)[..n0].iter().copied().fold(f32::MIN, f32::max);
    assert!((end - 12.0).abs() < 1e-2, "extended to the wall: {end}");
}

#[test]
fn cross_section_connects_splines() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreateShape { kind: ShapeKind::Rectangle, position: [0.0; 3], size: [10.0, 10.0, 0.0] }).unwrap();
    let a = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::ConvertToEditableSpline).unwrap();
    execute(&mut s, Command::CreateShape { kind: ShapeKind::Rectangle, position: [0.0, 0.0, 10.0], size: [10.0, 10.0, 0.0] }).unwrap();
    let b = *s.selection.iter().next().unwrap();
    attach(&mut s, a, b);
    add(&mut s, "CrossSection");
    let sh = shape(&s, a);
    assert_eq!(sh.splines.len(), 2 + 4, "two rectangles and four connections");
    // Surface over the cage: six four-sided patches.
    add(&mut s, "Surface");
    set(&mut s, 1, "Steps", 2.0);
    let m = s.scene.eval_mesh(a).value.mesh.clone();
    assert_eq!(m.face_count(), 6 * 3 * 3, "6 patches of (steps+1)^2 quads");
}

#[test]
fn bevel_profile_along_rectangle() {
    let mut s = AppState::new();
    let profile = line(&mut s, &[[0.0, 0.0, 0.0], [2.0, 5.0, 0.0], [0.0, 10.0, 0.0]]);
    execute(&mut s, Command::CreateShape { kind: ShapeKind::Rectangle, position: [30.0, 0.0, 0.0], size: [20.0, 20.0, 0.0] }).unwrap();
    let r = *s.selection.iter().next().unwrap();
    add(&mut s, "Bevel Profile");
    execute(&mut s, Command::PickModifierNode { index: 0, node: profile.0 }).unwrap();
    let m = s.scene.eval_mesh(r).value.mesh.clone();
    assert!(m.face_count() > 0);
    let (lo, hi) = m.aabb();
    assert!((hi.z - lo.z - 10.0).abs() < 1e-2, "profile height 10: {lo} {hi}");
    assert!((hi.x - lo.x - 24.0).abs() < 0.5, "the outline grows by the profile's 2: {lo} {hi}");
}

#[test]
fn editable_spline_trim_extend_cross_insert() {
    let mut s = AppState::new();
    let p = plus(&mut s);
    execute(&mut s, Command::SplineCrossInsert { threshold: 0.1, point: None }).unwrap();
    let sh = s.scene.node(p).unwrap().edit_shape().unwrap();
    assert_eq!(sh.knot_count(), 6, "a knot at the crossing on each line");
    execute(&mut s, Command::SplineTrim { point: [7.0, 0.0, 0.0] }).unwrap();
    assert!(world_knots(&s, p).iter().all(|k| k.x <= 1e-3));
    execute(&mut s, Command::SplineExtend { point: [-9.0, 0.0, 0.0] }).unwrap_err();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(world_knots(&s, p).iter().any(|k| k.x > 5.0));
}

#[test]
fn bind_follows_segment() {
    let mut s = AppState::new();
    let a = line(&mut s, &[[0.0, 20.0, 0.0], [0.0, 5.0, 0.0]]);
    let b = line(&mut s, &[[-10.0, 0.0, 0.0], [10.0, 0.0, 0.0]]);
    attach(&mut s, a, b);
    // Knot 1 (the end of the first line) binds to segment 1 (the second line).
    execute(&mut s, Command::SplineBind { knot: 1, segment: 1 }).unwrap();
    let k = world_knots(&s, a)[1];
    assert!(k.distance(Vec3::ZERO) < 1e-3, "snapped to the segment's middle: {k}");
    // Move the second line up: the bound knot follows.
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Vertex }).unwrap();
    execute(&mut s, Command::SelectSub { node: None, ids: vec![2, 3], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 4.0, 0.0], system: None }).unwrap();
    let k = world_knots(&s, a)[1];
    assert!(k.distance(Vec3::new(0.0, 4.0, 0.0)) < 1e-3, "{k}");
    execute(&mut s, Command::SelectSub { node: None, ids: vec![1], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::UnbindSpline).unwrap();
    assert!(s.scene.node(a).unwrap().edit_shape().unwrap().binds.is_empty());
}

#[test]
fn cross_section_and_tangent_copy_paste() {
    let mut s = AppState::new();
    let a = line(&mut s, &[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]]);
    let b = line(&mut s, &[[0.0, 0.0, 10.0], [10.0, 0.0, 10.0]]);
    attach(&mut s, a, b);
    execute(&mut s, Command::SplineCrossSection { splines: vec![0, 1], kind: 0 }).unwrap();
    assert_eq!(s.scene.node(a).unwrap().edit_shape().unwrap().splines.len(), 4);
    // Give knot 0 a handle, copy it, paste it on knot 2.
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Vertex }).unwrap();
    execute(&mut s, Command::SelectSub { node: None, ids: vec![0, 1, 2, 3], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SetVertexType { kind: "Bezier Corner".into() }).unwrap();
    execute(&mut s, Command::SelectSub { node: None, ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SetKnotHandle { node: a.0, knot: 0, side_in: false, point: [3.0, 3.0, 0.0] }).unwrap();
    execute(&mut s, Command::SplineTangentCopy { knot: 0, side_in: false }).unwrap();
    execute(&mut s, Command::SplineTangentPaste { knot: 2, side_in: false, length: true }).unwrap();
    let sh = s.scene.node(a).unwrap().edit_shape().unwrap();
    let h0 = sh.splines[0].knots[0].out_vec;
    let h2 = sh.splines[1].knots[0].out_vec;
    assert!(h0.length() > 1.0, "the handle was set: {h0}");
    assert!(h0.distance(h2) < 1e-4, "Paste Length copies the whole handle: {h0} {h2}");
    // Paste Length off: knot 3 takes the angle and keeps its own handle length.
    execute(&mut s, Command::SetKnotHandle { node: a.0, knot: 3, side_in: true, point: [10.0, 0.0, 9.0] }).unwrap();
    let before = s.scene.node(a).unwrap().edit_shape().unwrap().splines[1].knots[1].in_vec.length();
    execute(&mut s, Command::SplineTangentPaste { knot: 3, side_in: true, length: false }).unwrap();
    let h3 = s.scene.node(a).unwrap().edit_shape().unwrap().splines[1].knots[1].in_vec;
    assert!((h3.length() - before).abs() < 1e-3, "{h3} keeps {before}");
    assert!(h3.normalize().dot(h0.normalize()) > 0.999, "{h3} points like {h0}");
}

#[test]
fn spline_soft_selection_falloff() {
    let mut s = AppState::new();
    let a = line(&mut s, &[[0.0, 0.0, 0.0], [5.0, 0.0, 0.0], [10.0, 0.0, 0.0], [40.0, 0.0, 0.0]]);
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Vertex }).unwrap();
    execute(&mut s, Command::UseSoftSelection).unwrap();
    execute(&mut s, Command::SoftSelectionSettings { falloff: Some(12.0), pinch: None, bubble: None, edge_distance: None, affect_backfacing: None }).unwrap();
    execute(&mut s, Command::SelectSub { node: None, ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 10.0, 0.0], system: None }).unwrap();
    let k = world_knots(&s, a);
    assert!((k[0].y - 10.0).abs() < 1e-3);
    assert!(k[1].y > 0.5 && k[1].y < 10.0, "partly: {}", k[1].y);
    assert!(k[2].y < k[1].y);
    assert!(k[3].y.abs() < 1e-4, "outside the falloff");
    // Edge Distance 1: only the next vertex along the spline follows.
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(world_knots(&s, a)[0].y.abs() < 1e-4, "undone");
    execute(&mut s, Command::SoftSelectionSettings { falloff: None, pinch: None, bubble: None, edge_distance: Some(1), affect_backfacing: None }).unwrap();
    execute(&mut s, Command::SelectSub { node: None, ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 10.0, 0.0], system: None }).unwrap();
    let k = world_knots(&s, a);
    assert!(k[1].y > 0.5, "one step away: {}", k[1].y);
    assert!(k[2].y.abs() < 1e-4, "two steps away: {}", k[2].y);
}

#[test]
fn a_missed_trim_or_extend_changes_nothing() {
    let mut s = AppState::new();
    let p = plus(&mut s);
    // Hide the second spline, then click where nothing can be trimmed or extended.
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Face }).unwrap();
    execute(&mut s, Command::SelectSub { node: None, ids: vec![1], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::HideSpline).unwrap();
    let before = s.scene.node(p).unwrap().edit_spline().unwrap().clone();
    let redo = s.undo.can_redo();
    execute(&mut s, Command::SplineExtend { point: [-9.0, 0.0, 0.0] }).unwrap_err();
    let after = s.scene.node(p).unwrap().edit_spline().unwrap().clone();
    assert_eq!(after, before, "the shape and its hidden flags are untouched");
    assert_eq!(s.undo.can_redo(), redo, "no phantom redo entry");
    // Two parallel lines, one hidden: trimming the other finds no crossing.
    let mut t = AppState::new();
    let a = line(&mut t, &[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]]);
    let b = line(&mut t, &[[0.0, 5.0, 0.0], [10.0, 5.0, 0.0]]);
    attach(&mut t, a, b);
    execute(&mut t, Command::SelectionMode { mode: SelectionMode::Face }).unwrap();
    execute(&mut t, Command::SelectSub { node: None, ids: vec![1], op: SelectOp::Replace }).unwrap();
    execute(&mut t, Command::HideSpline).unwrap();
    let before = t.scene.node(a).unwrap().edit_spline().unwrap().clone();
    execute(&mut t, Command::SplineTrim { point: [5.0, 0.0, 0.0] }).unwrap_err();
    assert_eq!(t.scene.node(a).unwrap().edit_spline().unwrap().clone(), before, "a missed trim keeps the hidden spline hidden");
    assert!(!t.undo.can_redo());
}
