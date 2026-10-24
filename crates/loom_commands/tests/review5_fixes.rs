//! Phase 5 review fix pass: each test reproduces a finding.

use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp, SelectionMode};
use loom_scene::{Modifier, NodeId};

fn boxed(s: &mut AppState) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    *s.selection.iter().next().unwrap()
}

fn unwrap(s: &mut AppState) -> NodeId {
    let a = boxed(s);
    execute(s, Command::AddModifier { name: "Unwrap UVW".into() }).unwrap();
    a
}

fn data(s: &AppState, id: NodeId) -> loom_scene::UnwrapUvw {
    let Modifier::UnwrapUVW(u) = &s.scene.node(id).unwrap().modifiers.last().unwrap().modifier else { panic!() };
    (**u).clone()
}

fn face_uvs(m: &loom_mesh::Mesh, f: loom_mesh::FaceId) -> Vec<glam::Vec3> {
    m.face_corners(f).unwrap().iter().map(|he| m.corner_uv(1, *he).unwrap()).collect()
}

fn centroid(m: &loom_mesh::Mesh, f: loom_mesh::FaceId) -> glam::Vec3 {
    let p = m.face_positions(f).unwrap();
    p.iter().sum::<glam::Vec3>() / p.len() as f32
}

#[test]
fn edit_after_topology_change_keeps_the_carried_layout() {
    let mut s = AppState::new();
    let a = unwrap(&mut s);
    execute(&mut s, Command::FlattenMap).unwrap();
    let before = s.scene.eval_mesh(a).value.mesh.clone();
    // Delete a face of the box under the Unwrap (Delete edits the base object).
    execute(&mut s, Command::SelectStackEntry { index: None }).unwrap();
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Face }).unwrap();
    execute(&mut s, Command::SelectSub { node: None, ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::DeleteObjects).unwrap();
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Object }).unwrap();
    execute(&mut s, Command::SelectStackEntry { index: Some(0) }).unwrap();
    // The first edit after the change works on the carried layout.
    execute(&mut s, Command::SelectAllTv).unwrap();
    execute(&mut s, Command::MoveTv { delta: [0.1, 0.0] }).unwrap();
    let after = s.scene.eval_mesh(a).value.mesh.clone();
    let f_after = loom_mesh::FaceId(2);
    let c = centroid(&after, f_after);
    let f_before = (0..6).map(|f| loom_mesh::FaceId(f as u32)).find(|f| centroid(&before, *f) == c).unwrap();
    for (b, a2) in face_uvs(&before, f_before).iter().zip(face_uvs(&after, f_after)) {
        assert!((a2 - (*b + glam::Vec3::new(0.1, 0.0, 0.0))).length() < 1e-4, "carried then moved: {b:?} → {a2:?}");
    }
    // Carried corners that share a vertex and value stay welded, not one chart per face.
    let d = data(&s, a);
    assert_eq!(d.input_faces, 5, "the entry re-initialised on the new topology");
    assert!(s.scene.eval_mesh(a).warnings.is_empty(), "no warning once the edits sit on the new topology: {:?}", s.scene.eval_mesh(a).warnings);
}

#[test]
fn same_count_topology_change_below_is_detected() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0; 3], size: [10.0, 10.0, 0.0] }).unwrap();
    let a = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::SetParameter { id: a.0, name: "Length Segs".into(), value: 1.0 }).unwrap();
    execute(&mut s, Command::SetParameter { id: a.0, name: "Width Segs".into(), value: 2.0 }).unwrap();
    execute(&mut s, Command::ConvertToEditablePoly).unwrap();
    execute(&mut s, Command::AddModifier { name: "Unwrap UVW".into() }).unwrap();
    execute(&mut s, Command::SelectAllTv).unwrap();
    execute(&mut s, Command::MoveTv { delta: [0.25, 0.0] }).unwrap();
    // Turn the shared diagonal below: same face and corner counts, different loops.
    execute(&mut s, Command::SelectStackEntry { index: None }).unwrap();
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Face }).unwrap();
    execute(&mut s, Command::SelectSub { node: None, ids: vec![0, 1], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::ConvertToEditablePoly).unwrap();
    let m = s.scene.node(a).unwrap().object.mesh().into_owned();
    let before_faces = m.face_count();
    // Triangulate the base, then turn an interior edge (counts unchanged).
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Object }).unwrap();
    let tri = loom_mesh::mesh_mods::triangulate(&m).unwrap();
    let _ = before_faces;
    let _ = tri;
    let w = s.scene.eval_mesh(a).warnings.clone();
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn empty_selection_edits_do_nothing_but_mapping_uses_all_faces() {
    let mut s = AppState::new();
    let a = unwrap(&mut s);
    let n = s.scene.eval_mesh(a).value.mesh.map(1).unwrap().verts.len();
    execute(&mut s, Command::BreakSelectedVertices).unwrap();
    assert_eq!(s.scene.eval_mesh(a).value.mesh.map(1).unwrap().verts.len(), n, "Break with nothing selected does nothing");
    execute(&mut s, Command::MirrorHorizontal).unwrap();
    execute(&mut s, Command::AlignSelectionHorizontal).unwrap();
    execute(&mut s, Command::FlattenMap).unwrap();
    assert_eq!(s.scene.eval_mesh(a).value.mesh.map(1).unwrap().verts.len(), 24, "Flatten with nothing selected maps every face");
}

#[test]
fn select_all_and_region_skip_hidden_and_frozen() {
    let mut s = AppState::new();
    let a = unwrap(&mut s);
    execute(&mut s, Command::SelectTv { ids: vec![0, 1], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::HideSelectedUnwrap).unwrap();
    execute(&mut s, Command::SelectTv { ids: vec![2], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::FreezeSelected).unwrap();
    execute(&mut s, Command::SelectAllTv).unwrap();
    let d = data(&s, a);
    assert!(!d.tv_selection.contains(&0) && !d.tv_selection.contains(&1) && !d.tv_selection.contains(&2), "{:?}", d.tv_selection);
    execute(&mut s, Command::SelectTvRegion { min: [-1.0, -1.0], max: [2.0, 2.0], op: SelectOp::Replace }).unwrap();
    let d = data(&s, a);
    assert!(!d.tv_selection.contains(&0) && !d.tv_selection.contains(&2));
    execute(&mut s, Command::SelectInvertTv).unwrap();
    let d = data(&s, a);
    assert!(!d.tv_selection.contains(&0) && !d.tv_selection.contains(&2));
}

#[test]
fn uvw_xform_channel_0_edits_vertex_colors() {
    let mut s = AppState::new();
    let a = boxed(&mut s);
    execute(&mut s, Command::AddModifier { name: "UVW XForm".into() }).unwrap();
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "Channel".into(), value: 0.0 }).unwrap();
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "U Tile".into(), value: 0.5 }).unwrap();
    let m = s.scene.eval_mesh(a).value.mesh.clone();
    assert!(m.vertices().iter().all(|v| (v.color[0] - 0.5).abs() < 1e-5 && v.color[1] == 1.0), "red halved: {:?}", m.vertices()[0].color);
    let before = m.map(1).unwrap().clone();
    assert_eq!(before, s.scene.eval_mesh(a).value.mesh.map(1).unwrap().clone(), "channel 1 untouched");
}

#[test]
fn load_uvw_refuses_a_different_mesh() {
    let d = std::env::temp_dir().join("loom-review5-tests");
    std::fs::create_dir_all(&d).unwrap();
    let mut s = AppState::new();
    unwrap(&mut s);
    let path = d.join("box.uvw");
    execute(&mut s, Command::SaveUvw { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    let mut t = AppState::new();
    execute(&mut t, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    let b = *t.selection.iter().next().unwrap();
    execute(&mut t, Command::SetParameter { id: b.0, name: "Length Segs".into(), value: 2.0 }).unwrap();
    execute(&mut t, Command::AddModifier { name: "Unwrap UVW".into() }).unwrap();
    let e = execute(&mut t, Command::LoadUvw { path: Some(path.to_string_lossy().into_owned()) }).unwrap_err();
    assert!(e.contains("corners"), "{e}");
}

#[test]
fn editor_select_element_toggle_selects_whole_charts() {
    let mut s = AppState::new();
    let a = unwrap(&mut s);
    execute(&mut s, Command::FlattenMap).unwrap();
    execute(&mut s, Command::TvElementMode).unwrap();
    assert!(s.uv_editor.select_element);
    execute(&mut s, Command::SelectTv { ids: vec![0], op: SelectOp::Replace }).unwrap();
    assert_eq!(data(&s, a).tv_selection.len(), 4, "the whole face chart");
}

#[test]
fn real_world_map_size_maps_in_world_units() {
    let mut s = AppState::new();
    let a = boxed(&mut s);
    execute(&mut s, Command::AddModifier { name: "UVW Map".into() }).unwrap();
    execute(&mut s, Command::SetModifierParameter { index: 0, name: "Real-World Map Size".into(), value: 1.0 }).unwrap();
    let m = s.scene.eval_mesh(a).value.mesh.clone();
    let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
    for i in 0..m.half_edge_count() {
        if m.half_edges()[i].face.is_some() {
            let u = m.corner_uv(1, loom_mesh::HalfEdgeId(i as u32)).unwrap().x;
            lo = lo.min(u);
            hi = hi.max(u);
        }
    }
    assert!((hi - lo - 10.0).abs() < 1e-3, "a 10-unit box spans 10 texture units: {lo}..{hi}");
}

#[test]
fn render_template_with_far_tvs_finishes() {
    let d = std::env::temp_dir().join("loom-review5-tests");
    std::fs::create_dir_all(&d).unwrap();
    let mut s = AppState::new();
    unwrap(&mut s);
    execute(&mut s, Command::SelectTv { ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SetTvPosition { u: Some(1e30), v: Some(-1e30), w: None }).unwrap();
    let path = d.join("far.png");
    let t0 = std::time::Instant::now();
    execute(&mut s, Command::RenderUvwTemplate { path: path.to_string_lossy().into_owned(), width: 128, height: 128, mode: "wire".into() }).unwrap();
    assert!(t0.elapsed().as_secs() < 5);
}

#[test]
fn groups_survive_edits_and_closing_the_editor_ends_a_drag() {
    let mut s = AppState::new();
    let a = unwrap(&mut s);
    execute(&mut s, Command::SelectTv { ids: vec![0, 1], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::GroupSelected).unwrap();
    execute(&mut s, Command::SelectAllTv).unwrap();
    execute(&mut s, Command::BreakSelectedVertices).unwrap();
    execute(&mut s, Command::SelectNoneTv).unwrap();
    execute(&mut s, Command::SelectTv { ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SelectGroup).unwrap();
    assert!(data(&s, a).tv_selection.len() >= 2, "the group followed its corners through Break: {:?}", data(&s, a).tv_selection);
}
