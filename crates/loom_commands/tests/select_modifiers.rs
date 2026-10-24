//! Selection and spline helper modifiers (phase 9): Mesh Select, Vol. Select,
//! Spline Select, Delete Spline, Normalize Spline, Renderable Spline.

use glam::Vec3;
use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp, SelectionMode, ShapeKind};
use loom_scene::NodeId;

fn add(s: &mut AppState, name: &str) {
    execute(s, Command::AddModifier { name: name.into() }).unwrap();
}

fn set(s: &mut AppState, index: u32, name: &str, value: f64) {
    execute(s, Command::SetModifierParameter { index, name: name.into(), value }).unwrap();
}

fn boxed(s: &mut AppState) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    *s.selection.iter().next().unwrap()
}

#[test]
fn mesh_select_passes_selection_up() {
    let mut s = AppState::new();
    let b = boxed(&mut s);
    add(&mut s, "Mesh Select");
    execute(&mut s, Command::ModifierSubobjectLevel { index: 0, level: 1 }).unwrap();
    execute(&mut s, Command::SelectSub { node: None, ids: vec![4, 5, 6, 7], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Object }).unwrap();
    add(&mut s, "XForm");
    // XForm moves only the selection that Mesh Select passes up.
    let (_, hi0) = s.scene.eval_mesh(b).value.mesh.aabb();
    execute(&mut s, Command::ModifierSubobjectLevel { index: 1, level: 1 }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 5.0], system: None }).unwrap();
    let m = s.scene.eval_mesh(b).value.mesh.clone();
    let (lo, hi) = m.aabb();
    assert!((hi.z - (hi0.z + 5.0)).abs() < 1e-3 && lo.z.abs() < 1e-3, "{lo} {hi}");
    let r = execute(&mut s, Command::State).unwrap();
    assert!(r["nodes"][0]["stack"].to_string().contains("Mesh Select"));
}

#[test]
fn vol_select_box_and_sphere() {
    let mut s = AppState::new();
    let b = boxed(&mut s);
    execute(&mut s, Command::SetParameter { id: b.0, name: "Height Segs".into(), value: 4.0 }).unwrap();
    add(&mut s, "Vol. Select");
    set(&mut s, 0, "Stack Selection Level", 1.0); // Vertex
    // The gizmo fits the object; move it down so it holds the lower half.
    execute(&mut s, Command::ModifierSubobjectLevel { index: 0, level: 1 }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, -5.0], system: None }).unwrap();
    let v = s.scene.eval_mesh(b).value.clone();
    let (_, ids) = v.selection.clone().expect("Vol. Select passes a vertex selection up");
    let m = &v.mesh;
    assert!(!ids.is_empty());
    assert!(ids.iter().all(|i| m.vertex_position(loom_mesh::VertexId(*i)).unwrap().z <= 5.0 + 1e-3));
    let lower = (0..m.vertex_count() as u32).filter(|i| m.vertex_position(loom_mesh::VertexId(*i)).unwrap().z <= 5.0 + 1e-3).count();
    assert_eq!(ids.len(), lower);
    // Invert selects the rest.
    set(&mut s, 0, "Invert", 1.0);
    let (_, inv) = s.scene.eval_mesh(b).value.selection.clone().unwrap();
    assert_eq!(inv.len() + ids.len(), m.vertex_count());
    // A sphere volume at the default fit.
    set(&mut s, 0, "Invert", 0.0);
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 5.0], system: None }).unwrap();
    set(&mut s, 0, "Volume", 1.0);
    let (_, sph) = s.scene.eval_mesh(b).value.selection.clone().unwrap();
    assert!(sph.len() < m.vertex_count(), "the box corners lie outside the fitted sphere");
}

#[test]
fn spline_select_and_delete_spline() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreateShape { kind: ShapeKind::Rectangle, position: [0.0; 3], size: [10.0, 10.0, 0.0] }).unwrap();
    let r = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::CreateShape { kind: ShapeKind::Circle, position: [20.0, 0.0, 0.0], size: [6.0, 6.0, 0.0] }).unwrap();
    let c = *s.selection.iter().next().unwrap();
    // Attach the circle so the shape has two splines.
    execute(&mut s, Command::SelectNodes { ids: vec![r.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::ConvertToEditableSpline).unwrap();
    execute(&mut s, Command::SplineAttach { node: c.0, reorient: false }).unwrap();
    add(&mut s, "Spline Select");
    execute(&mut s, Command::SplineSelectSet { level: 3, ids: vec![1], op: SelectOp::Replace }).unwrap();
    add(&mut s, "Delete Spline");
    let sh = s.scene.eval_mesh(r).value.shape.clone().expect("the shape flows through Spline Select and Delete Spline");
    assert_eq!(sh.splines.len(), 1, "the selected spline is gone");
    assert_eq!(sh.splines[0].knots.len(), 4, "the rectangle stays");
}

#[test]
fn normalize_spline_even_segments() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreateShape { kind: ShapeKind::Rectangle, position: [0.0; 3], size: [40.0, 20.0, 0.0] }).unwrap();
    let r = *s.selection.iter().next().unwrap();
    add(&mut s, "Normalize Spline");
    set(&mut s, 0, "Seg Length", 10.0);
    let sh = s.scene.eval_mesh(r).value.shape.clone().unwrap();
    // Perimeter 120 at 10 per segment: 12 knots, evenly spaced.
    assert_eq!(sh.splines[0].knots.len(), 12);
    let k: Vec<Vec3> = sh.splines[0].knots.iter().map(|k| k.pos).collect();
    for i in 0..k.len() {
        let d = k[i].distance(k[(i + 1) % k.len()]);
        assert!((d - 10.0).abs() < 0.5, "segment {i}: {d}");
    }
}

#[test]
fn renderable_spline_makes_a_tube() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreateShape { kind: ShapeKind::Circle, position: [0.0; 3], size: [20.0, 20.0, 0.0] }).unwrap();
    let c = *s.selection.iter().next().unwrap();
    assert_eq!(s.scene.eval_mesh(c).value.mesh.face_count(), 0);
    add(&mut s, "Renderable Spline");
    set(&mut s, 0, "Thickness", 2.0);
    set(&mut s, 0, "Sides", 8.0);
    let m = s.scene.eval_mesh(c).value.mesh.clone();
    assert!(m.face_count() > 0, "a viewport tube");
    let (lo, hi) = m.aabb();
    assert!((hi.z - lo.z - 2.0).abs() < 0.1, "thickness 2: {lo} {hi}");
}
