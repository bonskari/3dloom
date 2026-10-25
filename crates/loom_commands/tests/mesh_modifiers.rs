use loom_commands::keymap::{parse_chord, Keymap};
use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp, SelectionMode};
use loom_scene::{Modifier, NodeId};

fn boxed(s: &mut AppState, size: [f32; 3], segs: u32) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size }).unwrap();
    let id = *s.selection.iter().next().unwrap();
    for n in ["Length Segs", "Width Segs", "Height Segs"] {
        execute(s, Command::SetParameter { id: id.0, name: n.into(), value: segs as f64 }).unwrap();
    }
    id
}

fn add(s: &mut AppState, name: &str) {
    execute(s, Command::AddModifier { name: name.into() }).unwrap();
}

fn set(s: &mut AppState, index: u32, name: &str, value: f64) {
    execute(s, Command::SetModifierParameter { index, name: name.into(), value }).unwrap();
}

#[test]
fn smoothing_shell_and_reduction_modifiers_change_counts() {
    let mut s = AppState::new();
    let a = boxed(&mut s, [10.0; 3], 1);
    add(&mut s, "TurboSmooth");
    set(&mut s, 0, "Iterations", 2.0);
    let m = s.scene.eval_mesh(a).value.mesh.clone();
    m.check_invariants().unwrap();
    assert_eq!(m.face_count(), 6 * 16);
    execute(&mut s, Command::DeleteModifier { index: 0 }).unwrap();
    add(&mut s, "MeshSmooth");
    set(&mut s, 0, "Iterations", 1.0);
    let corner = |s: &AppState| s.scene.eval_mesh(a).value.mesh.vertex_position(loom_mesh::VertexId(0)).unwrap();
    let nurms = corner(&s);
    set(&mut s, 0, "Subdivision Method", 0.0); // Classic: linear, the box keeps its corners
    let classic = corner(&s);
    assert!((classic.x.abs() - 5.0).abs() < 1e-4 && nurms.x.abs() < 4.9, "{nurms:?} {classic:?}");
    execute(&mut s, Command::DeleteModifier { index: 0 }).unwrap();
    add(&mut s, "Shell");
    set(&mut s, 0, "Inner Amount", 1.0);
    set(&mut s, 0, "Override Inner Mat ID", 1.0);
    set(&mut s, 0, "Inner Mat ID", 4.0);
    let m = s.scene.eval_mesh(a).value.mesh.clone();
    m.check_invariants().unwrap();
    assert_eq!(m.face_count(), 12);
    assert_eq!(m.faces().iter().filter(|f| f.material_id == 4).count(), 6);
    execute(&mut s, Command::DeleteModifier { index: 0 }).unwrap();
    // Reduction.
    execute(&mut s, Command::SetParameter { id: a.0, name: "Length Segs".into(), value: 3.0 }).unwrap();
    add(&mut s, "Optimize");
    let m = s.scene.eval_mesh(a).value.mesh.clone();
    assert_eq!(m.face_count(), 12, "a cube in triangles");
    execute(&mut s, Command::DeleteModifier { index: 0 }).unwrap();
    add(&mut s, "ProOptimizer");
    set(&mut s, 0, "Vertex %", 50.0);
    let before = s.scene.eval_mesh(a).value.mesh.vertex_count();
    set(&mut s, 0, "Calculate", 1.0);
    let after = s.scene.eval_mesh(a).value.mesh.vertex_count();
    assert!(after < before, "{before} → {after}");
    execute(&mut s, Command::DeleteModifier { index: 0 }).unwrap();
    add(&mut s, "Turn To Poly");
    assert_eq!(s.scene.eval_mesh(a).value.mesh.face_count(), 6);
    add(&mut s, "Turn To Mesh");
    let t = s.scene.eval_mesh(a).value.mesh.clone();
    // Four 8-vertex sides (6 triangles each) and two quads (2 each); the merged sides keep their collinear vertices.
    assert_eq!(t.face_count(), 4 * 6 + 2 * 2);
    assert!((0..t.face_count()).all(|f| t.face_vertex_ids(loom_mesh::FaceId(f as u32)).unwrap().len() == 3));
    add(&mut s, "Subdivide");
    set(&mut s, 2, "Size", 4.0);
    assert!(s.scene.eval_mesh(a).value.mesh.face_count() > 12);
    add(&mut s, "Tessellate");
    let tess = s.scene.eval_mesh(a).value.mesh.clone();
    let spread = |m: &loom_mesh::Mesh| m.vertices().iter().map(|v| v.position.x.abs() + v.position.y.abs()).sum::<f32>();
    add(&mut s, "Relax");
    set(&mut s, 4, "Relax Value", 1.0);
    set(&mut s, 4, "Iterations", 3.0);
    let r = s.scene.eval_mesh(a).value.mesh.clone();
    assert_eq!(r.face_count(), tess.face_count());
    assert!(spread(&r) < spread(&tess), "relaxed inward");
    assert!(s.scene.eval_mesh(a).warnings.is_empty(), "{:?}", s.scene.eval_mesh(a).warnings);
}

#[test]
fn holes_checks_materials_and_deletion_follow_the_selection() {
    let mut s = AppState::new();
    let a = boxed(&mut s, [10.0; 3], 1);
    execute(&mut s, Command::ConvertToEditablePoly).unwrap();
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Face }).unwrap();
    execute(&mut s, Command::SelectSub { node: None, ids: vec![0], op: SelectOp::Replace }).unwrap();
    add(&mut s, "DeleteMesh");
    let m = s.scene.eval_mesh(a).value.mesh.clone();
    assert_eq!(m.face_count(), 5, "the selected polygon went");
    add(&mut s, "STL Check");
    set(&mut s, 1, "Check", 1.0);
    set(&mut s, 1, "Select Faces", 1.0);
    let e = s.scene.eval_mesh(a);
    assert_eq!(e.value.selection.as_ref().map(|(_, ids)| ids.len()), Some(4), "faces on open edges pass up");
    let report = s.scene.node(a).unwrap().modifiers[1].modifier.stl_report(&m);
    assert_eq!(report.open_edges, 4);
    add(&mut s, "Cap Holes");
    assert_eq!(s.scene.eval_mesh(a).value.mesh.face_count(), 6);
    add(&mut s, "Material");
    set(&mut s, 3, "Material ID", 7.0);
    assert!(s.scene.eval_mesh(a).value.mesh.faces().iter().all(|f| f.material_id == 7));
    add(&mut s, "Material By Element");
    set(&mut s, 4, "ID Count", 3.0);
    assert!(s.scene.eval_mesh(a).value.mesh.faces().iter().all(|f| f.material_id == 1), "one element, first ID");
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Object }).unwrap();
    execute(&mut s, Command::DeleteModifier { index: 0 }).unwrap(); // DeleteMesh goes; the box is whole again
    execute(&mut s, Command::SelectStackEntry { index: None }).unwrap();
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Face }).unwrap();
    execute(&mut s, Command::SelectSub { node: None, ids: vec![1], op: SelectOp::Replace }).unwrap();
    add(&mut s, "Face Extrude");
    assert_eq!(s.scene.node(a).unwrap().modifiers[0].name, "Face Extrude", "added just above the base object");
    set(&mut s, 0, "Amount", 3.0);
    let m = s.scene.eval_mesh(a).value.mesh.clone();
    m.check_invariants().unwrap();
    assert_eq!(m.face_count(), 6 + 4, "one polygon extruded: {}", m.face_count());
}

#[test]
fn vertex_paint_colours_vertices_near_the_stroke() {
    let mut s = AppState::new();
    let a = boxed(&mut s, [10.0; 3], 1);
    add(&mut s, "VertexPaint");
    execute(&mut s, Command::VertexPaintStroke { node: a.0, points: vec![[5.0, 5.0, 10.0]], rgb: [255, 0, 0], size: 3.0, opacity: 1.0 }).unwrap();
    let m = s.scene.eval_mesh(a).value.mesh.clone();
    let red = m.vertices().iter().filter(|v| v.color[0] > 0.99 && v.color[1] < 0.01).count();
    assert_eq!(red, 1, "only the corner under the brush");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(s.scene.eval_mesh(a).value.mesh.vertices().iter().all(|v| v.color[1] > 0.99));
}

#[test]
fn edit_poly_above_a_box_edits_the_input_and_warns_on_topology_change() {
    let mut s = AppState::new();
    let a = boxed(&mut s, [10.0; 3], 1);
    add(&mut s, "Edit Poly");
    assert!(s.poly_selected(), "the Editable Poly keymap is live on an Edit Poly entry");
    assert_eq!(s.hotkey(parse_chord("4").unwrap()), Some("Face Level"));
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Face }).unwrap();
    execute(&mut s, Command::SelectSub { node: None, ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::Extrude { height: 5.0, width: 0.0, kind: loom_commands::ExtrudeKind::Group }).unwrap();
    let m = s.scene.eval_mesh(a).value.mesh.clone();
    m.check_invariants().unwrap();
    assert_eq!(m.face_count(), 10, "the extrusion lives in the Edit Poly: {}", m.face_count());
    assert!(matches!(s.scene.node(a).unwrap().object, loom_scene::ObjectKind::Box { .. }), "the base stays parametric");
    // Changing the box below moves the edited vertices along.
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Object }).unwrap();
    execute(&mut s, Command::SetParameter { id: a.0, name: "Height".into(), value: 20.0 }).unwrap();
    let m2 = s.scene.eval_mesh(a).value.mesh.clone();
    assert_eq!(m2.face_count(), 10);
    assert!(m2.aabb().1.z >= 20.0 - 1e-3, "{:?}", m2.aabb());
    assert!(s.scene.eval_mesh(a).warnings.is_empty());
    // A topology change below drops the edits with a warning.
    execute(&mut s, Command::SetParameter { id: a.0, name: "Height Segs".into(), value: 3.0 }).unwrap();
    let e = s.scene.eval_mesh(a);
    assert_eq!(e.value.mesh.face_count(), 6 + 4 * 2, "the input passes through");
    assert!(e.warnings.iter().any(|w| w.contains("Edit Poly")), "{:?}", e.warnings);
    // Edit Mesh is its own modifier (triangles with edge visibility).
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    add(&mut s, "Edit Mesh");
    let slot = &s.scene.node(a).unwrap().modifiers[1];
    assert!(matches!(slot.modifier, Modifier::EditMesh { .. }));
    assert_eq!(slot.name, "Edit Mesh");
    // The chart's Edit Poly table is live on the entry: Break is Shift+B there.
    assert_eq!(Keymap::edit_poly().lookup(parse_chord("Shift+B").unwrap()), Some("Break"));
    assert_eq!(s.hotkey(parse_chord("Shift+B").unwrap()), Some("Break"));
    execute(&mut s, Command::SelectStackEntry { index: None }).unwrap();
    assert_eq!(s.hotkey(parse_chord("Shift+B").unwrap()), None, "the base object has no Break chord");
}
