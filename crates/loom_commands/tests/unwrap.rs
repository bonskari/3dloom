//! Unwrap UVW modifier: editing commands, topology carry, seams, files.

use loom_commands::{execute, parse_chord, AppState, Command, PrimitiveKind, SelectOp, SelectionMode, DEFERRED_UNWRAP};
use loom_scene::{Modifier, NodeId};
use std::path::PathBuf;

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
    let Modifier::UnwrapUVW(u) = &s.scene.node(id).unwrap().modifiers.last().unwrap().modifier else { panic!("no Unwrap UVW") };
    (**u).clone()
}

fn tv_count(s: &AppState, id: NodeId) -> usize {
    s.scene.eval_mesh(id).value.mesh.map(1).map(|m| m.verts.len()).unwrap_or(0)
}

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join("loom-unwrap-tests").join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn unwrap_on_mesh_without_uvs() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    let a = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::SetParameter { id: a.0, name: "Generate Mapping Coords.".into(), value: 0.0 }).unwrap();
    assert!(s.scene.eval_mesh(a).value.mesh.map(1).is_none());
    execute(&mut s, Command::AddModifier { name: "Unwrap UVW".into() }).unwrap();
    let m = s.scene.eval_mesh(a).value.mesh.clone();
    assert!(m.map(1).is_some(), "the entry creates channel 1");
    m.check_invariants().unwrap();
    execute(&mut s, Command::EditUvws).unwrap();
    assert!(s.uv_editor.open);
    // Editing the empty channel works: select everything, flatten.
    execute(&mut s, Command::SelectTv { ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::FlattenMap).unwrap();
    let m = s.scene.eval_mesh(a).value.mesh.clone();
    assert!(m.map(1).unwrap().verts.len() >= 8);
    assert!(s.scene.eval_mesh(a).warnings.is_empty(), "{:?}", s.scene.eval_mesh(a).warnings);
}

#[test]
fn topology_change_below_unwrap_keeps_surviving_uvs() {
    let mut s = AppState::new();
    let a = unwrap(&mut s);
    execute(&mut s, Command::FlattenMap).unwrap();
    let before = s.scene.eval_mesh(a).value.mesh.clone();
    // Edit below: an Edit Poly inserted under the Unwrap deletes a face.
    execute(&mut s, Command::SelectStackEntry { index: None }).unwrap();
    execute(&mut s, Command::AddModifier { name: "Edit Poly".into() }).unwrap();
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Face }).unwrap();
    execute(&mut s, Command::SelectSub { node: None, ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::DeleteObjects).unwrap();
    let after = s.scene.eval_mesh(a).value.mesh.clone();
    assert_eq!(after.face_count(), 5);
    let w = s.scene.eval_mesh(a).warnings.clone();
    assert!(w.iter().any(|w| w.contains("Topology changed below Unwrap UVW")), "{w:?}");
    // A surviving face keeps its flattened uvs (matched by centroid).
    let centroid = |m: &loom_mesh::Mesh, f: loom_mesh::FaceId| { let p = m.face_positions(f).unwrap(); p.iter().sum::<glam::Vec3>() / p.len() as f32 };
    let f_after = loom_mesh::FaceId(2);
    let c = centroid(&after, f_after);
    let f_before = (0..6).map(|f| loom_mesh::FaceId(f as u32)).find(|f| centroid(&before, *f) == c).unwrap();
    let uv_before: Vec<glam::Vec3> = before.face_corners(f_before).unwrap().iter().map(|he| before.corner_uv(1, *he).unwrap()).collect();
    let uv_after: Vec<glam::Vec3> = after.face_corners(f_after).unwrap().iter().map(|he| after.corner_uv(1, *he).unwrap()).collect();
    assert_eq!(uv_before, uv_after);
}

#[test]
fn edit_uvws_commands_are_undoable_as_one_entry() {
    let mut s = AppState::new();
    let a = unwrap(&mut s);
    let n0 = tv_count(&s, a);
    let undo_len = s.undo.len();
    execute(&mut s, Command::SelectTv { ids: (0..n0 as u32).collect(), op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::BreakSelectedVertices).unwrap();
    assert_eq!(s.undo.len(), undo_len + 2, "one entry for the selection, one for Break");
    assert_eq!(tv_count(&s, a), 24);
    execute(&mut s, Command::SelectTv { ids: (0..24).collect(), op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MoveTv { delta: [0.25, 0.0] }).unwrap();
    let m = s.scene.eval_mesh(a).value.mesh.clone();
    assert!(m.map(1).unwrap().verts.iter().all(|v| v.x >= 0.25 - 1e-5));
    for _ in 0..3 {
        execute(&mut s, Command::UndoSceneOperation).unwrap();
    }
    assert_eq!(tv_count(&s, a), n0, "undoing Move, the reselect and Break restores the welded channel");
    // Weld / Target Weld / Stitch / Mirror / Flip / Align / Rotate / Scale run.
    execute(&mut s, Command::SelectTv { ids: (0..n0 as u32).collect(), op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::BreakSelectedVertices).unwrap();
    execute(&mut s, Command::SelectTv { ids: (0..24).collect(), op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::TextureVertexWeldSelected).unwrap();
    assert!(tv_count(&s, a) < 24);
    execute(&mut s, Command::RotateTv { degrees: 90.0 }).unwrap();
    execute(&mut s, Command::ScaleTv { factor: [2.0, 2.0] }).unwrap();
    execute(&mut s, Command::MirrorHorizontal).unwrap();
    execute(&mut s, Command::AlignSelectionHorizontal).unwrap();
    let m = s.scene.eval_mesh(a).value.mesh.clone();
    let ys: Vec<f32> = m.map(1).unwrap().verts.iter().map(|v| v.y).collect();
    assert!(ys.iter().all(|y| (y - ys[0]).abs() < 1e-4), "aligned: {ys:?}");
    assert_eq!(data(&s, a).tv_selection.len(), tv_count(&s, a));
}

#[test]
fn flatten_and_pack_tolerate_degenerate_faces() {
    let mut s = AppState::new();
    let a = unwrap(&mut s);
    // Collapse the box to zero height: the side faces become zero-area.
    execute(&mut s, Command::SelectStackEntry { index: None }).unwrap();
    execute(&mut s, Command::SetParameter { id: a.0, name: "Height".into(), value: 0.0 }).unwrap();
    execute(&mut s, Command::SelectStackEntry { index: Some(0) }).unwrap();
    execute(&mut s, Command::FlattenMap).unwrap();
    execute(&mut s, Command::Pack).unwrap();
    execute(&mut s, Command::RelaxUnwrap).unwrap();
    execute(&mut s, Command::NormalMap).unwrap();
    execute(&mut s, Command::UnfoldMap).unwrap();
    execute(&mut s, Command::PeltMap).unwrap();
    let m = s.scene.eval_mesh(a).value.mesh.clone();
    m.check_invariants().unwrap();
    assert!(m.map(1).unwrap().verts.iter().all(|v| v.x.is_finite() && v.y.is_finite()));
}

#[test]
fn render_template_writes_png_and_refuses_huge() {
    let d = dir("template");
    let mut s = AppState::new();
    let a = unwrap(&mut s);
    execute(&mut s, Command::FlattenMap).unwrap();
    let path = d.join("t.png");
    execute(&mut s, Command::RenderUvwTemplate { path: path.to_string_lossy().into_owned(), width: 256, height: 256, mode: "wire".into() }).unwrap();
    let img = image::open(&path).unwrap().to_rgba8();
    assert_eq!((img.width(), img.height()), (256, 256));
    assert!(img.pixels().any(|p| p[0] > 200), "edges drawn");
    assert!(execute(&mut s, Command::RenderUvwTemplate { path: path.to_string_lossy().into_owned(), width: 9000, height: 256, mode: "solid".into() }).is_err());
    execute(&mut s, Command::RenderUvwTemplate { path: d.join("s.png").to_string_lossy().into_owned(), width: 64, height: 64, mode: "solid".into() }).unwrap();
    let _ = a;
}

#[test]
fn save_and_load_uvw() {
    let d = dir("uvw");
    let mut s = AppState::new();
    let a = unwrap(&mut s);
    execute(&mut s, Command::FlattenMap).unwrap();
    let before = s.scene.eval_mesh(a).value.mesh.map(1).cloned().unwrap();
    let path = d.join("box.uvw");
    execute(&mut s, Command::SaveUvw { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    execute(&mut s, Command::ResetUvws).unwrap();
    assert_ne!(s.scene.eval_mesh(a).value.mesh.map(1).cloned().unwrap(), before, "Reset UVWs restores the input mapping");
    execute(&mut s, Command::LoadUvw { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    assert_eq!(s.scene.eval_mesh(a).value.mesh.map(1).cloned().unwrap(), before);
    execute(&mut s, Command::SaveUvw { path: None }).unwrap();
    assert_eq!(s.dialog, Some(loom_commands::Dialog::SaveUvw));
}

#[test]
fn seams_from_edge_selection_and_point_to_point() {
    let mut s = AppState::new();
    let a = unwrap(&mut s);
    execute(&mut s, Command::ModifierSubobjectLevel { index: 0, level: 2 }).unwrap();
    execute(&mut s, Command::SelectSub { node: None, ids: vec![0, 4], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::EdgeSelToPeltSeamReplace).unwrap();
    assert_eq!(data(&s, a).seams.len(), 2);
    execute(&mut s, Command::PointToPointEdgeSelection).unwrap();
    assert_eq!(s.pick_mode, Some(loom_commands::PickMode::SeamPointA));
    execute(&mut s, Command::SeamPoint { node: a.0, vertex: 0 }).unwrap();
    execute(&mut s, Command::SeamPoint { node: a.0, vertex: 7 }).unwrap();
    assert!(data(&s, a).seams.len() > 2, "{:?}", data(&s, a).seams);
    execute(&mut s, Command::PeltSeamToEdgeSelReplace).unwrap();
    assert!(s.sub_selection.len() >= 3);
    // Seam to Edge Sel is a selection step: undo it, then the path.
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(data(&s, a).seams.len(), 2, "one undo removes the point-to-point path");
}

#[test]
fn every_chart_row_is_a_command_or_deferred() {
    let registered: std::collections::HashSet<&str> = loom_commands::registry().iter().map(|i| i.name).collect();
    let deferred: std::collections::HashSet<&str> = DEFERRED_UNWRAP.iter().map(|(n, _)| *n).collect();
    let rows = loom_commands::unwrap_chart_rows();
    assert!(rows.len() > 190, "{}", rows.len());
    for (action, chord) in &rows {
        for c in chord.split(", ").filter(|c| !c.is_empty()) {
            parse_chord(c).unwrap_or_else(|e| panic!("{c}: {e}"));
        }
        assert!(registered.contains(action) || deferred.contains(action), "unclassified Unwrap UVW action {action}");
    }
    let s = AppState::new();
    let mut t = s;
    unwrap(&mut t);
    execute(&mut t, Command::EditUvws).unwrap();
    assert_eq!(t.hotkey(parse_chord("Ctrl+B").unwrap()), Some("Break Selected Vertices"), "the Unwrap group is live while the editor is open");
    assert_eq!(t.hotkey(parse_chord("W").unwrap()), Some("Texture Vertex Move Mode"));
    execute(&mut t, Command::CloseUvws).unwrap();
    assert_eq!(t.hotkey(parse_chord("W").unwrap()), Some("Select and Move"));
}
