//! Selection changes are undoable, as in the reference: Edit > Undo after a
//! selection change restores the previous selection, and Ctrl+Z walks
//! selection and scene operations in the order they happened.

use loom_commands::{execute, AppState, Command, ExtrudeKind, PrimitiveKind, SelectOp, SelectionMode, SubObjectId};
use loom_scene::NodeId;

fn boxed(s: &mut AppState, x: f32) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [x, 0.0, 0.0], size: [10.0; 3] }).unwrap();
    *s.selection.iter().next().unwrap()
}

fn sel(s: &AppState) -> Vec<NodeId> {
    s.selection.iter().copied().collect()
}

fn faces(s: &AppState) -> Vec<u32> {
    let mut v: Vec<u32> = s.sub_selection.iter().filter_map(|(_, id)| match id {
        SubObjectId::Face(f) => Some(f.0),
        _ => None,
    }).collect();
    v.sort();
    v
}

fn undo(s: &mut AppState) {
    execute(s, Command::UndoSceneOperation).unwrap();
}

fn redo(s: &mut AppState) {
    execute(s, Command::RedoSceneOperation).unwrap();
}

#[test]
fn undo_restores_the_previous_selection_and_redo_reapplies_it() {
    let mut s = AppState::new();
    let a = boxed(&mut s, 0.0);
    let b = boxed(&mut s, 20.0);
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    assert_eq!(sel(&s), vec![a]);
    undo(&mut s);
    assert_eq!(sel(&s), vec![b], "Undo Select brings back the selection before it");
    assert_eq!(s.scene.len(), 2, "and leaves the scene alone");
    redo(&mut s);
    assert_eq!(sel(&s), vec![a]);
    assert_eq!(s.undo.labels().last().map(String::as_str), Some("Select"));
}

#[test]
fn undo_walks_selection_and_scene_operations_in_order() {
    let mut s = AppState::new();
    let a = boxed(&mut s, 0.0);
    execute(&mut s, Command::MoveSelection { delta: [5.0, 0.0, 0.0], system: Some(loom_commands::CoordSystem::World) }).unwrap();
    execute(&mut s, Command::SelectNone).unwrap();
    assert!(s.selection.is_empty());
    undo(&mut s);
    assert_eq!(sel(&s), vec![a], "first undo: the Select None");
    let x = s.scene.world_matrix(a).w_axis.x;
    assert!((x - 5.0).abs() < 1e-4, "the move is still there: {x}");
    undo(&mut s);
    assert!(s.scene.world_matrix(a).w_axis.x.abs() < 1e-4, "second undo: the move");
    assert_eq!(sel(&s), vec![a]);
    undo(&mut s);
    assert_eq!(s.scene.len(), 0, "third undo: the creation");
}

#[test]
fn a_selection_that_does_not_change_adds_no_undo_step() {
    let mut s = AppState::new();
    let a = boxed(&mut s, 0.0);
    let n = s.undo.len();
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    assert_eq!(s.undo.len(), n);
}

#[test]
fn a_new_selection_after_undo_clears_redo() {
    let mut s = AppState::new();
    let a = boxed(&mut s, 0.0);
    let b = boxed(&mut s, 20.0);
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    undo(&mut s);
    execute(&mut s, Command::SelectNone).unwrap();
    redo(&mut s);
    assert!(s.selection.is_empty(), "nothing to redo after a new selection");
    let _ = b;
}

#[test]
fn selecting_does_not_mark_the_scene_unsaved() {
    let dir = std::env::temp_dir().join(format!("loom-selundo-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("a.loom").to_string_lossy().into_owned();
    let mut s = AppState::new();
    let a = boxed(&mut s, 0.0);
    execute(&mut s, Command::SaveFileAs { path: Some(path) }).unwrap();
    assert!(!s.dirty());
    execute(&mut s, Command::SelectNone).unwrap();
    assert!(!s.dirty(), "a selection change is not an edit of the file");
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    assert!(!s.dirty());
    let _ = std::fs::remove_dir_all(&dir);
}

/// 3×3-segment poly plane, polygon level.
fn plane(s: &mut AppState) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0; 3], size: [30.0, 30.0, 0.0] }).unwrap();
    let id = *s.selection.iter().next().unwrap();
    execute(s, Command::SetParameter { id: id.0, name: "Length Segs".into(), value: 3.0 }).unwrap();
    execute(s, Command::SetParameter { id: id.0, name: "Width Segs".into(), value: 3.0 }).unwrap();
    execute(s, Command::ConvertToEditablePoly).unwrap();
    execute(s, Command::FaceLevel).unwrap();
    assert_eq!(s.selection_mode, SelectionMode::Face);
    id
}

#[test]
fn sub_object_selection_undoes_like_object_selection() {
    let mut s = AppState::new();
    let id = plane(&mut s);
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![4], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![0], op: SelectOp::Replace }).unwrap();
    undo(&mut s);
    assert_eq!(faces(&s), vec![4]);
    undo(&mut s);
    assert!(faces(&s).is_empty());
    redo(&mut s);
    assert_eq!(faces(&s), vec![4]);
}

#[test]
fn undoing_an_extrude_brings_back_the_polygons_it_extruded() {
    let mut s = AppState::new();
    let id = plane(&mut s);
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![4], op: SelectOp::Replace }).unwrap();
    let before = s.scene.node(id).unwrap().cage_mesh().face_count();
    execute(&mut s, Command::Extrude { height: 5.0, width: 0.0, kind: ExtrudeKind::Group }).unwrap();
    assert!(s.scene.node(id).unwrap().cage_mesh().face_count() > before);
    undo(&mut s);
    assert_eq!(s.scene.node(id).unwrap().cage_mesh().face_count(), before);
    assert_eq!(faces(&s), vec![4], "the face selected before the extrude is selected again");
    redo(&mut s);
    assert!(s.scene.node(id).unwrap().cage_mesh().face_count() > before);
    assert!(!faces(&s).is_empty(), "redo shows the selection the extrude left");
}

#[test]
fn undoing_a_sub_object_move_keeps_the_sub_object_selection() {
    let mut s = AppState::new();
    let id = plane(&mut s);
    execute(&mut s, Command::SelectSub { node: Some(id.0), ids: vec![4], op: SelectOp::Replace }).unwrap();
    // An interactive drag: commits inside a transaction, as the gizmo does.
    s.undo.begin("Move");
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 3.0], system: Some(loom_commands::CoordSystem::World) }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 3.0], system: Some(loom_commands::CoordSystem::World) }).unwrap();
    s.undo.end();
    undo(&mut s);
    assert_eq!(faces(&s), vec![4], "the move kept the topology, so the face ids are still valid");
    undo(&mut s);
    assert!(faces(&s).is_empty(), "the next undo is the face selection");
}
