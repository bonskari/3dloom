//! Edit Normals modifier (phase 9): Break / Unify / Specify / Make Explicit /
//! Reset / Average, Copy / Paste, explicit normals through edits and FBX.

use glam::Vec3;
use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp};
use loom_mesh::{HalfEdgeId, NORMALS_CHANNEL};
use loom_scene::NodeId;

fn boxed(s: &mut AppState) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    let b = *s.selection.iter().next().unwrap();
    execute(s, Command::AddModifier { name: "Edit Normals".into() }).unwrap();
    b
}

fn normals_count(s: &mut AppState) -> u64 {
    execute(s, Command::State).unwrap()["edit_normals"]["normals"].as_u64().unwrap()
}

/// The output normal at every corner of vertex `v`.
fn corner_normals_at(s: &AppState, id: NodeId, v: u32) -> Vec<Vec3> {
    let m = s.scene.eval_mesh(id).value.mesh.clone();
    let n = loom_mesh::mesh_mods::corner_normals(&m);
    m.half_edges().iter().enumerate().filter(|(_, h)| h.origin.0 == v && h.face.is_some()).map(|(i, _)| n[i]).collect()
}

#[test]
fn break_unify_and_specify() {
    let mut s = AppState::new();
    let b = boxed(&mut s);
    // A box has a smoothing group per side: three normals at each corner vertex.
    assert_eq!(normals_count(&mut s), 24);
    execute(&mut s, Command::EditNormalsSelectBy { level: 1 }).unwrap();
    execute(&mut s, Command::EditNormalsSelect { ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::UnifyNormals).unwrap();
    assert_eq!(normals_count(&mut s), 22);
    let ns = corner_normals_at(&s, b, 0);
    assert!(ns.windows(2).all(|w| w[0].distance(w[1]) < 1e-4), "unified: {ns:?}");
    assert!((ns[0].length() - 1.0).abs() < 1e-4);
    execute(&mut s, Command::BreakNormals).unwrap();
    assert_eq!(normals_count(&mut s), 24);
    // Make Explicit and set a direction: every corner of the selection points up.
    execute(&mut s, Command::EditNormalsSetNormal { direction: [0.0, 0.0, 1.0] }).unwrap();
    assert!(corner_normals_at(&s, b, 0).iter().all(|n| n.distance(Vec3::Z) < 1e-4));
    execute(&mut s, Command::ResetNormals).unwrap();
    assert!(corner_normals_at(&s, b, 0).iter().all(|n| n.distance(Vec3::Z) > 0.1), "reset to the face normals");
    // Undo brings the explicit normals back.
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(corner_normals_at(&s, b, 0).iter().all(|n| n.distance(Vec3::Z) < 1e-4));
}

#[test]
fn average_selected_and_copy_paste() {
    let mut s = AppState::new();
    let b = boxed(&mut s);
    execute(&mut s, Command::EditNormalsSelectBy { level: 1 }).unwrap();
    execute(&mut s, Command::EditNormalsSelect { ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::AverageNormals).unwrap();
    let ns = corner_normals_at(&s, b, 0);
    assert!(ns.windows(2).all(|w| w[0].distance(w[1]) < 1e-4), "averaged: {ns:?}");
    assert_eq!(normals_count(&mut s), 24, "Average keeps the normals separate (explicit)");
    // Copy a normal and paste it onto vertex 6's normals.
    execute(&mut s, Command::EditNormalsSelectBy { level: 0 }).unwrap();
    let r = execute(&mut s, Command::State).unwrap();
    let first: u64 = r["edit_normals"]["selected"][0].as_u64().unwrap();
    execute(&mut s, Command::EditNormalsSelect { ids: vec![first as u32], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::CopyNormal).unwrap();
    execute(&mut s, Command::EditNormalsSelectBy { level: 1 }).unwrap();
    execute(&mut s, Command::EditNormalsSelect { ids: vec![6], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::PasteNormal).unwrap();
    assert!(corner_normals_at(&s, b, 6).iter().all(|n| n.distance(ns[0]) < 1e-4));
}

#[test]
fn explicit_normals_survive_edits() {
    let mut s = AppState::new();
    let b = boxed(&mut s);
    execute(&mut s, Command::EditNormalsSelectBy { level: 1 }).unwrap();
    execute(&mut s, Command::EditNormalsSelect { ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::EditNormalsSetNormal { direction: [0.0, 0.0, 1.0] }).unwrap();
    // An Edit Poly above extrudes the top face; vertex 0 is at the bottom and keeps its normals.
    execute(&mut s, Command::AddModifier { name: "Edit Poly".into() }).unwrap();
    let m = s.scene.eval_mesh(b).value.mesh.clone();
    let top = (0..m.face_count() as u32).find(|f| m.face_normal(loom_mesh::FaceId(*f)).z > 0.9).unwrap();
    execute(&mut s, Command::SelectionMode { mode: loom_commands::SelectionMode::Face }).unwrap();
    execute(&mut s, Command::SelectSub { node: None, ids: vec![top], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::Extrude { height: 5.0, width: 0.0, kind: loom_commands::ExtrudeKind::Group }).unwrap();
    let m = s.scene.eval_mesh(b).value.mesh.clone();
    let bottom = (0..m.vertex_count() as u32).find(|v| m.vertex_position(loom_mesh::VertexId(*v)).unwrap().distance(Vec3::new(-5.0, -5.0, 0.0)) < 1e-3).unwrap();
    assert!(corner_normals_at(&s, b, bottom).iter().all(|n| n.distance(Vec3::Z) < 1e-4), "explicit normals kept through the extrude");
    // A topology change below drops the edit with a warning, never panics.
    execute(&mut s, Command::SelectionMode { mode: loom_commands::SelectionMode::Object }).unwrap();
    execute(&mut s, Command::SetParameter { id: b.0, name: "Height Segs".into(), value: 3.0 }).unwrap();
    let e = s.scene.eval_mesh(b);
    assert!(e.warnings.iter().any(|w| w.contains("Edit Normals")), "{:?}", e.warnings);
}

#[test]
fn fbx_writes_explicit_normals() {
    let mut s = AppState::new();
    let b = boxed(&mut s);
    execute(&mut s, Command::EditNormalsSelectBy { level: 1 }).unwrap();
    execute(&mut s, Command::EditNormalsSelect { ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::EditNormalsSetNormal { direction: [0.0, 0.0, 1.0] }).unwrap();
    let m = s.scene.eval_mesh(b).value.mesh.clone();
    assert!(m.map(NORMALS_CHANNEL).is_some());
    assert!(m.explicit_normal(HalfEdgeId(0)).is_some() || m.half_edges().iter().enumerate().any(|(i, _)| m.explicit_normal(HalfEdgeId(i as u32)).is_some()));
    let dir = std::env::temp_dir().join("loom-edit-normals");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("n.fbx");
    execute(&mut s, Command::ExportFile { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    let scene = ufbx::load_file(path.to_str().unwrap(), ufbx::LoadOpts::default()).unwrap();
    let mesh = scene.meshes.iter().next().unwrap();
    // Exactly the three normals at the box's first corner point up; the UV layers carry no normals channel.
    let up = mesh.vertex_normal.values.iter().filter(|n| n.z > 0.999 && n.x.abs() < 1e-3).count();
    assert!(up >= 3, "{up}");
    assert!(mesh.uv_sets.len() <= 1, "{} uv sets", mesh.uv_sets.len());
    let r = execute(&mut s, Command::State).unwrap();
    assert!(!r.to_string().contains(&NORMALS_CHANNEL.to_string()), "State does not list the normals channel");
}

#[test]
fn edit_normals_chart_rows_exist() {
    let reg: std::collections::HashSet<&str> = loom_commands::registry().iter().map(|i| i.name).collect();
    for (a, _) in loom_commands::keymap::edit_normals_chart_rows() {
        assert!(reg.contains(a), "{a}");
    }
    let k = loom_commands::keymap::Keymap::edit_normals();
    assert_eq!(k.lookup(loom_commands::keymap::parse_chord("U").unwrap()), Some("Unify Normals"));
}

#[test]
fn edit_normals_after_a_topology_change_below() {
    let mut s = AppState::new();
    let b = boxed(&mut s);
    execute(&mut s, Command::EditNormalsSelectBy { level: 1 }).unwrap();
    execute(&mut s, Command::EditNormalsSelect { ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::UnifyNormals).unwrap();
    // More segments below: the old edits no longer fit.
    execute(&mut s, Command::SetParameter { id: b.0, name: "Length Segs".into(), value: 2.0 }).unwrap();
    let before = normals_count(&mut s);
    // The first edit after the change acts on what was just selected.
    execute(&mut s, Command::EditNormalsSelect { ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::UnifyNormals).unwrap();
    assert_eq!(normals_count(&mut s), before - 2, "unified at once, not a second try");
}

#[test]
fn same_counts_different_topology_drops_the_edits() {
    use loom_scene::edit_normals::EditNormalsData;
    let a = loom_mesh::box_primitive(Vec3::splat(5.0));
    // The same box with one face started at another corner: equal counts, other corners.
    let mut im = loom_mesh::indexed::IndexedMesh::from_mesh(&a);
    im.faces[0].ids.rotate_left(1);
    let b = im.build().unwrap();
    assert_eq!((a.vertex_count(), a.face_count(), a.half_edge_count()), (b.vertex_count(), b.face_count(), b.half_edge_count()));
    let mut d = EditNormalsData::default();
    d.selection = d.resolve(&a, &[0], 1);
    d.unify_selected(&a);
    assert!(d.fits(&a));
    assert!(!d.fits(&b), "a reordered face is a different topology");
}

#[test]
fn copy_normal_is_not_an_undo_step() {
    let mut s = AppState::new();
    let b = boxed(&mut s);
    execute(&mut s, Command::EditNormalsSelectBy { level: 0 }).unwrap();
    execute(&mut s, Command::EditNormalsSelect { ids: vec![0], op: SelectOp::Replace }).unwrap();
    let steps = s.undo.len();
    let labels = s.undo.labels();
    execute(&mut s, Command::CopyNormal).unwrap();
    assert_eq!(s.undo.len(), steps, "Copy adds no undo step");
    assert_eq!(s.undo.labels(), labels);
    // Paste is an undo step, and Undo takes it back.
    execute(&mut s, Command::EditNormalsSelectBy { level: 1 }).unwrap();
    execute(&mut s, Command::EditNormalsSelect { ids: vec![6], op: SelectOp::Replace }).unwrap();
    let before = corner_normals_at(&s, b, 6);
    let steps = s.undo.len();
    execute(&mut s, Command::PasteNormal).unwrap();
    assert_eq!(s.undo.len(), steps + 1);
    assert_ne!(corner_normals_at(&s, b, 6), before);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    let back = corner_normals_at(&s, b, 6);
    assert!(back.iter().zip(&before).all(|(a, c)| a.distance(*c) < 1e-5), "{back:?} vs {before:?}");
    // The copied normal is still there after the undo: Paste works again.
    execute(&mut s, Command::PasteNormal).unwrap();
    assert_ne!(corner_normals_at(&s, b, 6), before);
}

/// Normal `g`'s direction and the world position of its vertex, from the entry.
fn normal_of(s: &AppState, id: NodeId, g: u32) -> (Vec3, Vec3) {
    let (_, index) = loom_commands::normals::entry(s).unwrap();
    let input = loom_commands::normals::input_mesh(s, id, index);
    let loom_scene::Modifier::EditNormals(d) = &s.scene.node(id).unwrap().modifiers[index].modifier else { panic!("not Edit Normals") };
    let groups = d.groups_for(&input);
    let corner = groups.iter().position(|x| *x == g).unwrap();
    let p = input.vertex_position(input.half_edges()[corner].origin).unwrap();
    (d.normal(&input, &groups, g), s.scene.object_matrix(id).transform_point3(p))
}

#[test]
fn applying_edit_normals_enters_the_normal_level() {
    let mut s = AppState::new();
    let b = boxed(&mut s);
    let n = s.scene.node(b).unwrap();
    assert_eq!(n.modifiers[n.active_entry.unwrap()].sub_level, 1, "the normals show at once, as in the reference");
}

#[test]
fn the_rotate_tool_turns_the_selected_normals_not_the_object() {
    let mut s = AppState::new();
    let b = boxed(&mut s);
    execute(&mut s, Command::SetTransform { id: b.0, position: Some([20.0, 0.0, 0.0]), rotation_deg: Some([0.0, 0.0, 90.0]), scale: None }).unwrap();
    execute(&mut s, Command::EditNormalsSelect { ids: vec![0], op: SelectOp::Replace }).unwrap();
    let (n0, at) = normal_of(&s, b, 0);
    // The transform gizmo sits on the selected normal.
    let f = loom_commands::frame_for(&s, 0, Some(b));
    assert!(f.origin.distance(at) < 1e-4, "gizmo at {} not on the normal at {at}", f.origin);
    let world = s.scene.world_matrix(b);
    execute(&mut s, Command::RotateSelection { axis: loom_commands::Axis::X, degrees: 30.0, system: Some(loom_commands::CoordSystem::World) }).unwrap();
    assert_eq!(s.scene.world_matrix(b), world, "the object stays put");
    let (n1, _) = normal_of(&s, b, 0);
    // A world X rotation of a normal on an object turned 90 degrees about Z.
    let r = glam::Quat::from_rotation_z(90f32.to_radians());
    let want = r.inverse() * glam::Quat::from_rotation_x(30f32.to_radians()) * r * n0;
    assert!(n1.distance(want) < 1e-4, "{n1} vs {want}");
    assert_eq!(execute(&mut s, Command::State).unwrap()["edit_normals"]["explicit"], 1, "a rotated normal is explicit");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(normal_of(&s, b, 0).0.distance(n0) < 1e-5, "undo turns it back");
}
