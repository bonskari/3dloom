//! Select Similar by layer and material, Page Down / Ctrl+Page Down,
//! Select Sibling Next / Previous, Link
//! Inheritance (Selected) and Freeze Transform / Transform To Zero.

use glam::Vec3;
use loom_commands::command::SelectOp;
use loom_commands::{execute, AppState, Command, PrimitiveKind};
use loom_scene::NodeId;

fn dummy(s: &mut AppState, name: &str, parent: Option<NodeId>) -> NodeId {
    let r = execute(s, Command::CreateHelper { kind: "dummy".into(), position: [0.0; 3], size: 5.0 }).unwrap();
    let id = NodeId(r["node"].as_u64().unwrap());
    execute(s, Command::Rename { id: id.0, name: name.into() }).unwrap();
    if let Some(p) = parent {
        execute(s, Command::SetParent { id: id.0, parent: Some(p.0) }).unwrap();
    }
    id
}

fn select(s: &mut AppState, ids: &[NodeId]) {
    execute(s, Command::SelectNodes { ids: ids.iter().map(|i| i.0).collect(), op: SelectOp::Replace }).unwrap();
}

fn selected(s: &AppState) -> Vec<NodeId> {
    s.selection.iter().copied().collect()
}

fn boxed(s: &mut AppState, x: f32) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [x, 0.0, 0.0], size: [10.0; 3] }).unwrap();
    *s.selection.iter().next().unwrap()
}

#[test]
fn select_similar_needs_the_same_type_layer_and_material() {
    let mut s = AppState::new();
    let (a, b, c) = (boxed(&mut s, 0.0), boxed(&mut s, 20.0), boxed(&mut s, 40.0));
    execute(&mut s, Command::CreateLayer { name: Some("Other".into()) }).unwrap();
    let layer = s.scene.layers.iter().find(|l| l.name == "Other").unwrap().id;
    select(&mut s, &[c]);
    execute(&mut s, Command::MoveToLayer { layer }).unwrap();
    select(&mut s, &[a]);
    execute(&mut s, Command::SelectSimilar).unwrap();
    assert_eq!(selected(&s), vec![a, b], "the box on another layer is not similar");
    let _ = c;
}

#[test]
fn select_by_layer_selects_the_objects_of_several_layers() {
    let mut s = AppState::new();
    let a = boxed(&mut s, 0.0);
    execute(&mut s, Command::CreateLayer { name: Some("L1".into()) }).unwrap();
    let b = boxed(&mut s, 20.0);
    execute(&mut s, Command::CreateLayer { name: Some("L2".into()) }).unwrap();
    let c = boxed(&mut s, 40.0);
    let id = |s: &AppState, n: &str| s.scene.layers.iter().find(|l| l.name == n).unwrap().id;
    let r = execute(&mut s, Command::SelectByLayer { layers: None }).unwrap();
    assert_eq!(r["dialog"], "SelectByLayer");
    let (l1, l2) = (id(&s, "L1"), id(&s, "L2"));
    execute(&mut s, Command::SelectByLayer { layers: Some(vec![l1, l2]) }).unwrap();
    let want: Vec<NodeId> = s.scene.nodes().filter(|n| n.layer == l1 || n.layer == l2).map(|n| n.id).collect();
    assert_eq!(selected(&s), want);
    assert!(!want.is_empty());
    let _ = (a, b, c);
}

#[test]
fn view_undo_names_the_change_and_keeps_twenty() {
    let mut s = AppState::new();
    boxed(&mut s, 0.0);
    execute(&mut s, Command::ZoomViewportIn).unwrap();
    assert_eq!(s.active().view_undo.undo_label(), Some("Zoom"));
    execute(&mut s, Command::ZoomExtents).unwrap();
    assert_eq!(s.active().view_undo.undo_label(), Some("Zoom Extents"));
    execute(&mut s, Command::UndoViewportOperation).unwrap();
    assert_eq!(s.active().view_undo.redo_label(), Some("Zoom Extents"));
    for _ in 0..30 {
        execute(&mut s, Command::ZoomViewportIn).unwrap();
    }
    assert_eq!(s.active().view_undo.len(), loom_commands::state::VIEW_UNDO_DEPTH);
    assert_eq!(loom_commands::state::VIEW_UNDO_DEPTH, 20);
}

#[test]
fn update_during_spinner_drag_toggles() {
    let mut s = AppState::new();
    assert!(s.update_during_spinner_drag);
    execute(&mut s, Command::UpdateDuringSpinnerDragToggle).unwrap();
    assert!(!s.update_during_spinner_drag);
}

#[test]
fn paint_selection_size_up_and_down_change_the_brush() {
    let mut s = AppState::new();
    let start = s.prefs.paint_brush_size;
    execute(&mut s, Command::PaintSelectionSizeUp).unwrap();
    assert_eq!(s.prefs.paint_brush_size, start + 1.0);
    execute(&mut s, Command::PaintSelectionSizeDown).unwrap();
    execute(&mut s, Command::PaintSelectionSizeDown).unwrap();
    assert_eq!(s.prefs.paint_brush_size, start - 1.0);
}

#[test]
fn page_down_selects_immediate_children_and_ctrl_page_down_all_descendants() {
    let mut s = AppState::new();
    let root = dummy(&mut s, "Root", None);
    let a = dummy(&mut s, "A", Some(root));
    let b = dummy(&mut s, "B", Some(root));
    let a1 = dummy(&mut s, "A1", Some(a));
    select(&mut s, &[root]);
    execute(&mut s, Command::SelectChild).unwrap();
    assert_eq!(selected(&s), vec![a, b]);
    select(&mut s, &[root]);
    execute(&mut s, Command::SelectChildren).unwrap();
    assert_eq!(selected(&s), vec![a, b, a1]);
}

#[test]
fn siblings_follow_the_reference_definition_in_an_uneven_hierarchy() {
    // Dummy01 has Dummy03 and Dummy05; Dummy03 has Dummy02 and Dummy08; Dummy05 has Dummy06.
    let mut s = AppState::new();
    let d01 = dummy(&mut s, "Dummy01", None);
    let d02 = dummy(&mut s, "Dummy02", None);
    let d03 = dummy(&mut s, "Dummy03", Some(d01));
    let d05 = dummy(&mut s, "Dummy05", Some(d01));
    let d06 = dummy(&mut s, "Dummy06", Some(d05));
    let d08 = dummy(&mut s, "Dummy08", None);
    execute(&mut s, Command::SetParent { id: d02.0, parent: Some(d03.0) }).unwrap();
    execute(&mut s, Command::SetParent { id: d08.0, parent: Some(d03.0) }).unwrap();
    select(&mut s, &[d06]);
    execute(&mut s, Command::SelectSiblingNext).unwrap();
    assert_eq!(selected(&s), vec![d08]);
    select(&mut s, &[d06]);
    execute(&mut s, Command::SelectSiblingPrevious).unwrap();
    assert_eq!(selected(&s), vec![d02]);
    // From Dummy02 both commands go to Dummy08 (its nearest branch is Dummy03).
    select(&mut s, &[d02]);
    execute(&mut s, Command::SelectSiblingNext).unwrap();
    assert_eq!(selected(&s), vec![d08]);
    execute(&mut s, Command::SelectSiblingPrevious).unwrap();
    assert_eq!(selected(&s), vec![d02]);
    // A hidden sibling is skipped.
    execute(&mut s, Command::HideNodes { ids: vec![d08.0] }).unwrap();
    {
        select(&mut s, &[d06]);
        execute(&mut s, Command::SelectSiblingNext).unwrap();
        assert_eq!(selected(&s), vec![d02]);
    }
}

#[test]
fn link_inheritance_applies_to_every_selected_object() {
    let mut s = AppState::new();
    let p = dummy(&mut s, "P", None);
    let a = dummy(&mut s, "A", Some(p));
    let b = dummy(&mut s, "B", Some(p));
    select(&mut s, &[a, b]);
    execute(&mut s, Command::LinkInheritanceSelected { move_axes: None, rotate_axes: Some([false, false, true]), scale_axes: None }).unwrap();
    for id in [a, b] {
        assert_eq!(s.scene.node(id).unwrap().inherit.rotate_axes, [false, false, true]);
    }
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.node(a).unwrap().inherit.rotate_axes, [true; 3], "one undo step");
}

#[test]
fn freeze_transform_makes_the_current_pose_the_zero_pose() {
    let mut s = AppState::new();
    let b = boxed(&mut s, 0.0);
    execute(&mut s, Command::SetTransform { id: b.0, position: Some([10.0, 20.0, 30.0]), rotation_deg: Some([0.0, 0.0, 45.0]), scale: None }).unwrap();
    select(&mut s, &[b]);
    execute(&mut s, Command::FreezeTransform).unwrap();
    let pos = |s: &AppState| s.scene.world_matrix(b).w_axis.truncate();
    assert!(pos(&s).distance(Vec3::new(10.0, 20.0, 30.0)) < 1e-4, "freezing does not move it");
    let n = s.scene.node(b).unwrap();
    assert!(matches!(n.transform.position.controller, loom_scene::Controller::List(_)));
    // Moving edits the zero layer; Transform To Zero returns to the frozen pose.
    execute(&mut s, Command::SetTransform { id: b.0, position: Some([50.0, 20.0, 30.0]), rotation_deg: Some([0.0, 0.0, 90.0]), scale: None }).unwrap();
    assert!(pos(&s).distance(Vec3::new(50.0, 20.0, 30.0)) < 1e-3, "{}", pos(&s));
    let loom_scene::Controller::List(items) = &s.scene.node(b).unwrap().transform.position.controller else { panic!("still a list") };
    assert_eq!(items[0].controller, loom_scene::Controller::Constant(Vec3::new(40.0, 0.0, 0.0)), "the move went to the zero layer");
    execute(&mut s, Command::TransformToZero).unwrap();
    assert!(pos(&s).distance(Vec3::new(10.0, 20.0, 30.0)) < 1e-3);
    assert!((s.scene.node(b).unwrap().transform.rotation.get().z - 45.0).abs() < 1e-3);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(pos(&s).distance(Vec3::new(50.0, 20.0, 30.0)) < 1e-3, "one undo step");
    // Freeze Position leaves rotation alone; Position To Zero on an unfrozen rotation keeps it.
    let c = boxed(&mut s, 0.0);
    execute(&mut s, Command::SetTransform { id: c.0, position: Some([5.0, 0.0, 0.0]), rotation_deg: Some([0.0, 0.0, 30.0]), scale: None }).unwrap();
    select(&mut s, &[c]);
    execute(&mut s, Command::FreezePosition).unwrap();
    assert!(matches!(s.scene.node(c).unwrap().transform.rotation.controller, loom_scene::Controller::Constant(_)));
    execute(&mut s, Command::RotationToZero).unwrap();
    assert!(s.scene.node(c).unwrap().transform.rotation.get().length() < 1e-4, "a rotation never frozen goes to zero");
}
