use loom_commands::{execute, AppState, Command, PrimitiveKind};

fn state() -> AppState {
    AppState::new()
}

#[test]
fn convert_to_poly_keeps_primitive_smoothing() {
    let mut s = state();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Sphere, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    execute(&mut s, Command::ConvertToEditablePoly).unwrap();
    let id = *s.selection.iter().next().unwrap();
    let node = s.scene.node(id).unwrap();
    let poly = node.object.poly().expect("an Editable Poly");
    let first = poly.mesh.faces()[0].smoothing_groups;
    assert!(first != 0 && poly.mesh.faces().iter().all(|f| f.smoothing_groups == first), "a sphere is one smoothing group");
    assert_eq!(poly.subdiv.iterations.get(), 1);
    assert!(!poly.subdiv.use_nurms.get());
    assert_eq!(poly.soft.falloff.get(), 20.0);
    // Parameters rollout shows the Subdivision Surface spinners by their names.
    let names: Vec<&str> = node.object.parameters().into_iter().map(|(n, _)| n).collect();
    assert!(names.contains(&"Iterations") && names.contains(&"Smoothness"));
    // A box stays faceted.
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    execute(&mut s, Command::ConvertToEditablePoly).unwrap();
    let id = *s.selection.iter().next().unwrap();
    let mesh = &s.scene.node(id).unwrap().object.poly().unwrap().mesh;
    let g0 = mesh.faces()[0].smoothing_groups;
    assert!(mesh.faces().iter().any(|f| f.smoothing_groups != g0));
}

#[test]
fn mesh_edits_keep_poly_settings() {
    let mut s = state();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    execute(&mut s, Command::ConvertToEditablePoly).unwrap();
    let id = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::SetParameter { id: id.0, name: "Iterations".into(), value: 2.0 }).unwrap();
    execute(&mut s, Command::SelectionMode { mode: loom_commands::SelectionMode::Face }).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![0], op: loom_commands::SelectOp::Replace }).unwrap();
    execute(&mut s, Command::ExtrudeFaces { distance: 1.0 }).unwrap();
    let poly = s.scene.node(id).unwrap().object.poly().unwrap();
    assert_eq!(poly.subdiv.iterations.get(), 2, "an extrude must not reset the Subdivision Surface settings");
    assert_eq!(poly.mesh.face_count(), 10);
}
