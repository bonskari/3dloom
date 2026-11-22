//! The parity34 round.

use super::*;
use egui::accesskit::Role;

fn line(h: &mut Harness, pts: &[[f32; 3]]) -> loom_scene::NodeId {
    let r = h.app.dispatch(Command::LineStart { point: pts[0] }).unwrap();
    let id = r["id"].as_u64().unwrap();
    for p in &pts[1..] {
        h.cmd(Command::LineAddKnot { node: id, point: *p, smooth: false });
    }
    h.cmd(Command::LineEnd { node: id });
    loom_scene::NodeId(id)
}

fn red_boxes(h: &Harness) -> usize {
    fn walk(s: &egui::Shape) -> usize {
        match s {
            egui::Shape::Vec(v) => v.iter().map(walk).sum(),
            egui::Shape::Rect(r) if r.stroke.color == crate::viewport_widget::SHAPE_CHECK_RED => 1,
            _ => 0,
        }
    }
    h.shapes.iter().map(|s| walk(&s.shape)).sum()
}

/// Shape Check stops drawing when another utility button is clicked, as its
/// Close does, and a Pick Object still waiting is dropped too.
#[test]
fn shape_check_ends_when_another_utility_is_opened() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1000.0));
    maximized(&mut h);
    let id = line(&mut h, &[[0.0, 0.0, 0.0], [40.0, 40.0, 0.0], [40.0, 0.0, 0.0], [0.0, 40.0, 0.0]]);
    h.cmd(Command::SelectNone);
    h.cmd(Command::ZoomExtentsAll);
    h.app.panel_tab = crate::Tab::Utilities;
    h.app.utility = crate::command_panel::utilities::Utility::ShapeCheck;
    h.frames(2);
    h.click(h.find(Role::Button, "Pick Object").center());
    h.click(h.screen_of(Vec3::new(40.0, 20.0, 0.0)));
    assert_eq!(h.app.state.shape_check, Some(id));
    assert_eq!(red_boxes(&h), 1);
    h.click_widget("Measure");
    h.frames(2);
    assert_eq!(h.app.utility, crate::command_panel::utilities::Utility::Measure);
    assert_eq!(h.app.state.shape_check, None, "another utility ends the check");
    assert_eq!(red_boxes(&h), 0, "no red boxes left");
    // Back to Shape Check with a pick waiting, then away: the pick ends.
    h.app.utility = crate::command_panel::utilities::Utility::ShapeCheck;
    h.frames(2);
    h.click(h.find(Role::Button, "Pick Object").center());
    assert_eq!(h.app.state.pick_mode, Some(loom_commands::PickMode::ShapeCheck));
    h.click_widget("Collapse");
    assert_eq!(h.app.state.pick_mode, None, "the waiting pick ends");
    assert_eq!(h.app.state.shape_check, None);
}

/// Bone Tools > Object Properties on a box (Bone On for any object): the
/// Bone On checkbox turns the box into a bone and enables the rest; with
/// Freeze Length off, a Move-gizmo drag of its linked child turns the box to
/// aim at the child and stretches its geometry to reach it.
#[test]
fn bone_on_for_a_box_makes_a_dragged_child_turn_and_stretch_it() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1000.0));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let a = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [40.0, 0.0, 0.0], size: [4.0; 3] });
    let b = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::SetParent { id: b.0, parent: Some(a.0) });
    h.cmd(Command::SelectNodes { ids: vec![a.0], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::BoneTools);
    h.frames(2);
    h.click_text_anywhere("Bone Editing Tools");
    h.frames(2);
    h.click_text_anywhere("Fin Adjustment Tools");
    h.frames(2);
    let bone = |h: &Harness| h.app.state.scene.node(a).unwrap().bone.clone();
    assert!(!bone(&h).bone_on, "off for a box by default");
    h.click(h.find(Role::CheckBox, "Bone On").center());
    assert!(bone(&h).bone_on, "the checkbox turns it on");
    assert!(bone(&h).freeze_length);
    h.click(h.find(Role::CheckBox, "Freeze Length").center());
    assert!(!bone(&h).freeze_length);
    h.app.state.dialog = None;
    maximized(&mut h);
    h.cmd(Command::SelectNodes { ids: vec![b.0], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::ZoomExtentsAll);
    h.frames(2);
    h.key(egui::Key::W);
    let from = h.screen_of(Vec3::new(40.0, 0.0, 0.0));
    let to = h.screen_of(Vec3::new(40.0, 40.0, 0.0));
    h.drag(from, to);
    let child = h.app.state.scene.world_matrix(b).transform_point3(Vec3::ZERO);
    assert!(child.y > 5.0, "the drag moved the child: {child:?}");
    let x = h.app.state.scene.world_matrix(a).transform_vector3(Vec3::X).normalize();
    let to_child = child.normalize();
    assert!(x.abs_diff_eq(to_child, 1e-3), "the box aims at its child: {x:?} vs {to_child:?}");
    let f = bone(&h).stretch_factor.x;
    assert!((f - child.length() / 40.0).abs() < 1e-3, "stretched by the distance ratio: {f}");
}

fn bone_chain(h: &mut Harness) -> Vec<loom_scene::NodeId> {
    // A bent chain: root up +Z, mid along +X, child up +Z again, then a nub.
    let r = h.app.dispatch(Command::CreateBoneChain { points: vec![[0.0; 3], [0.0, 0.0, 20.0], [20.0, 0.0, 20.0], [20.0, 0.0, 40.0]], name: None }).unwrap();
    r["nodes"].as_array().unwrap().iter().map(|v| loom_scene::NodeId(v.as_u64().unwrap())).collect()
}

/// Bone Tools > Remove: the parent bone turns and stretches to reach where
/// the removed bone ended, its child links to the parent and keeps its place,
/// and an IK chain through it stays intact; Delete instead makes the chain
/// invalid, so it goes with its goal.
#[test]
fn bone_tools_remove_stretches_the_parent_and_keeps_ik_while_delete_breaks_it() {
    let mut h = Harness::new();
    let ids = bone_chain(&mut h);
    let (root, mid, child) = (ids[0], ids[1], ids[2]);
    h.cmd(Command::CreateIkChain { start: root.0, end: child.0, solver: "hi".into(), name: None });
    let child_world = h.app.state.scene.world_matrix(child);
    h.cmd(Command::SelectNodes { ids: vec![mid.0], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::BoneTools);
    h.frames(2);
    h.click_text_anywhere("Remove");
    let s = &h.app.state.scene;
    assert!(s.node(mid).is_none(), "removed");
    assert_eq!(s.node(child).unwrap().parent, Some(root), "the child links to the parent");
    assert!(s.world_matrix(child).abs_diff_eq(child_world, 1e-3), "and keeps its place");
    let base = s.world_matrix(root).transform_point3(Vec3::ZERO);
    let reach = child_world.transform_point3(Vec3::ZERO);
    let len = s.node(root).unwrap().object.bone_length().unwrap();
    assert!((len - base.distance(reach)).abs() < 1e-3, "the parent stretches to reach it: {len}");
    let tip = s.world_matrix(root).transform_point3(s.node(root).unwrap().object.bone_tip_local().unwrap());
    assert!(tip.distance(reach) < 1e-3, "and points at it: {tip:?}");
    assert_eq!(s.ik_chains.len(), 1, "the IK chain stays");
    assert_eq!((s.ik_chains[0].start, s.ik_chains[0].end), (root, child));
    // Delete on the child: the chain through it is invalid and goes.
    let goal = s.ik_chains[0].goal;
    h.cmd(Command::SelectNodes { ids: vec![child.0], op: loom_commands::SelectOp::Replace });
    h.frames(2);
    h.click_text_anywhere("Delete");
    let s = &h.app.state.scene;
    assert!(s.node(child).is_none());
    assert!(s.ik_chains.is_empty(), "the chain is gone");
    assert!(s.node(goal).is_none(), "with its goal");
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(h.app.state.scene.ik_chains.len(), 1, "one undo step brings it back");
}
