//! Main Toolbar: the Scale flyout and the Named Selection Sets field.

use loom_commands::{execute, named_sub_sets, AppState, Command, PrimitiveKind, ScaleKind, SelectOp, Tool};
use loom_scene::NodeId;

fn poly_plane(s: &mut AppState, x: f32) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [x, 0.0, 0.0], size: [30.0, 30.0, 0.0] }).unwrap();
    execute(s, Command::ConvertToEditablePoly).unwrap();
    *s.selection.iter().next().unwrap()
}

#[test]
fn scale_flyout_picks_the_scale_type_and_scale_cycle_steps_through_it() {
    let mut s = AppState::new();
    assert_eq!(s.scale_kind, ScaleKind::Uniform, "the flyout starts on Uniform Scale");
    execute(&mut s, Command::ScaleType { kind: ScaleKind::NonUniform }).unwrap();
    assert_eq!((s.active_tool, s.scale_kind), (Tool::Scale, ScaleKind::NonUniform), "choosing a flyout button turns on Scale");
    execute(&mut s, Command::ScaleCycle).unwrap();
    assert_eq!(s.scale_kind, ScaleKind::Squash);
    execute(&mut s, Command::ScaleCycle).unwrap();
    assert_eq!(s.scale_kind, ScaleKind::Uniform);
    // From another tool, Scale Cycle first turns on Scale with the current type.
    execute(&mut s, Command::SelectAndMove).unwrap();
    execute(&mut s, Command::ScaleCycle).unwrap();
    assert_eq!((s.active_tool, s.scale_kind), (Tool::Scale, ScaleKind::Uniform));
}

#[test]
fn sub_object_sets_belong_to_their_object_and_level() {
    let mut s = AppState::new();
    let a = poly_plane(&mut s, 0.0);
    execute(&mut s, Command::VertexLevel).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(a.0), ids: vec![0, 1], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::NamedSubSelectionCopy { name: "Rim".into() }).unwrap();
    assert_eq!(named_sub_sets(&s), vec!["Rim".to_owned()]);
    // Another level of the same object does not list it.
    execute(&mut s, Command::FaceLevel).unwrap();
    assert!(named_sub_sets(&s).is_empty());
    // Polygons under the same name are a set of their own; the vertices stay.
    execute(&mut s, Command::SelectSub { node: Some(a.0), ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::NamedSubSelectionCopy { name: "Rim".into() }).unwrap();
    execute(&mut s, Command::VertexLevel).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(a.0), ids: vec![3], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::NamedSubSelectionPaste { name: "Rim".into() }).unwrap();
    assert_eq!(s.sub_selection.len(), 2, "the vertex set survived the polygon set of the same name");
    // A second object does not list the first one's sets, and keeps its own.
    execute(&mut s, Command::ObjectLevel).unwrap();
    let b = poly_plane(&mut s, 100.0);
    execute(&mut s, Command::VertexLevel).unwrap();
    assert!(named_sub_sets(&s).is_empty(), "b has no sets");
    execute(&mut s, Command::SelectSub { node: Some(b.0), ids: vec![2], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::NamedSubSelectionCopy { name: "Rim".into() }).unwrap();
    execute(&mut s, Command::ObjectLevel).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::VertexLevel).unwrap();
    execute(&mut s, Command::NamedSubSelectionPaste { name: "Rim".into() }).unwrap();
    assert_eq!(s.sub_selection.len(), 2, "copying on b kept a's vertices");
}
