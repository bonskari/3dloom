use loom_commands::{execute, AppState, Command, ShapeKind};
use loom_scene::{NodeId, ObjectKind};

fn created(s: &AppState) -> NodeId {
    *s.selection.iter().next().unwrap()
}

#[test]
fn every_shape_kind_creates_a_parametric_shape_with_an_outline() {
    let mut s = AppState::new();
    for kind in ShapeKind::ALL {
        let r = execute(&mut s, Command::CreateShape { kind, position: [0.0; 3], size: [20.0, 10.0, 15.0] }).unwrap();
        let id = created(&s);
        let node = s.scene.node(id).unwrap();
        assert_eq!(node.object.base_name(), kind.label(), "{r}");
        assert!(node.name.starts_with(kind.label()));
        let shape = node.object.shape().expect("a shape");
        let pls = shape.polylines();
        assert!(!pls.is_empty() && pls.iter().all(|p| p.len() >= 2), "{kind:?}");
        // Without Enable In Viewport the mesh is just the outline's vertices (bounds / picking).
        let m = node.object.mesh();
        assert_eq!(m.face_count(), 0);
        assert!(m.vertex_count() >= 2);
        assert!(s.selection_bounds().is_some(), "{kind:?} has bounds");
        execute(&mut s, Command::SetParameter { id: id.0, name: "Enable In Viewport".into(), value: 1.0 }).unwrap();
        let tube = s.scene.node(id).unwrap().object.mesh().into_owned();
        tube.check_invariants().unwrap();
        assert!(tube.face_count() > 0, "{kind:?} renders a tube");
    }
    assert_eq!(s.scene.nodes().count(), ShapeKind::ALL.len());
    // Donut has two splines, Arc is open, Helix climbs.
    execute(&mut s, Command::CreateShape { kind: ShapeKind::Donut, position: [0.0; 3], size: [20.0, 20.0, 0.0] }).unwrap();
    assert_eq!(s.scene.node(created(&s)).unwrap().object.shape().unwrap().splines.len(), 2);
    execute(&mut s, Command::CreateShape { kind: ShapeKind::Arc, position: [0.0; 3], size: [20.0, 20.0, 0.0] }).unwrap();
    assert!(!s.scene.node(created(&s)).unwrap().object.shape().unwrap().splines[0].closed);
    execute(&mut s, Command::CreateShape { kind: ShapeKind::Helix, position: [0.0; 3], size: [20.0, 20.0, 30.0] }).unwrap();
    let (lo, hi) = s.scene.node(created(&s)).unwrap().object.shape().unwrap().aabb();
    assert!(hi.z - lo.z > 29.0);
}

#[test]
fn convert_to_editable_spline_keeps_the_outline_and_rendering() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreateShape { kind: ShapeKind::Circle, position: [0.0; 3], size: [20.0, 20.0, 0.0] }).unwrap();
    let id = created(&s);
    execute(&mut s, Command::SetParameter { id: id.0, name: "Thickness".into(), value: 3.0 }).unwrap();
    execute(&mut s, Command::SetParameter { id: id.0, name: "Steps".into(), value: 2.0 }).unwrap();
    let before = s.scene.node(id).unwrap().object.shape().unwrap().polylines();
    execute(&mut s, Command::ConvertToEditableSpline).unwrap();
    let node = s.scene.node(id).unwrap();
    assert!(matches!(node.object, ObjectKind::EditableSpline(_)));
    assert_eq!(node.object.stack_name(), "Editable Spline");
    let after = node.object.shape().unwrap();
    assert_eq!(after.polylines(), before);
    assert_eq!(after.render.thickness, 3.0);
    assert_eq!(after.steps, 2);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(matches!(s.scene.node(id).unwrap().object, ObjectKind::Circle { .. }));
}

#[test]
fn line_tool_adds_knots_removes_the_last_and_closes() {
    let mut s = AppState::new();
    execute(&mut s, Command::LineStart { point: [0.0, 0.0, 0.0] }).unwrap();
    let id = created(&s);
    assert_eq!(s.scene.node(id).unwrap().name, "Line001");
    assert_eq!(s.scene.node(id).unwrap().object.stack_name(), "Line");
    execute(&mut s, Command::LineAddKnot { node: id.0, point: [10.0, 0.0, 0.0], smooth: false }).unwrap();
    execute(&mut s, Command::LineAddKnot { node: id.0, point: [10.0, 10.0, 0.0], smooth: true }).unwrap();
    execute(&mut s, Command::LineSetHandle { node: id.0, point: [14.0, 12.0, 0.0] }).unwrap();
    let shape = s.scene.node(id).unwrap().object.shape().unwrap();
    assert_eq!(shape.splines[0].knots.len(), 3);
    assert_eq!(shape.splines[0].knots[2].kind, loom_shape::KnotKind::Bezier, "dragging makes a Bezier knot");
    assert!((shape.splines[0].knots[2].out_vec - glam::Vec3::new(4.0, 2.0, 0.0)).length() < 1e-5);
    assert!(!shape.splines[0].closed);
    execute(&mut s, Command::LineAddKnot { node: id.0, point: [0.0, 10.0, 0.0], smooth: false }).unwrap();
    execute(&mut s, Command::LineRemoveLastKnot { node: id.0 }).unwrap();
    assert_eq!(s.scene.node(id).unwrap().object.shape().unwrap().splines[0].knots.len(), 3);
    execute(&mut s, Command::LineClose { node: id.0 }).unwrap();
    assert!(s.scene.node(id).unwrap().object.shape().unwrap().splines[0].closed);
    // The whole line is one undo entry per command, like the other creation steps.
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(!s.scene.node(id).unwrap().object.shape().unwrap().splines[0].closed);
    // Shapes pass the Shapes selection filter and hide with Hide Shapes.
    execute(&mut s, Command::SelectionFilter { filter: loom_commands::SelectionFilter::Shapes }).unwrap();
    assert!(s.is_selectable(s.scene.node(id).unwrap()));
    execute(&mut s, Command::HideShapesToggle).unwrap();
    assert!(!s.is_visible(s.scene.node(id).unwrap()));
}
