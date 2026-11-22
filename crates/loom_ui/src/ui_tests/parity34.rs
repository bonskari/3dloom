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
