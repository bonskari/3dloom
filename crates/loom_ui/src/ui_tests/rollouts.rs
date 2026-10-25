//! Command panel rollouts used as a person does: drag a title bar to move
//! the rollout, the right-click rollout menu (go to a rollout, Close All,
//! Open All, Reset Rollout Order), dragging the panel to pan it.

use super::*;
use egui::accesskit::Role;

/// The Create panel with the Box tool on: Object Type, Name and Color,
/// Creation Method, Keyboard Entry, Parameters.
fn box_panel(h: &mut Harness) {
    h.click_widget("Box");
    h.frames(3);
}

fn title_y(h: &Harness, title: &str) -> f32 {
    h.widget(title).center().y
}

fn menu_entry(h: &Harness, label: &str) -> egui::Pos2 {
    let open = menus(h);
    let all = h.find_all(Role::Button, label);
    all.iter().copied().find(|r| open.iter().any(|m| m.contains_rect(*r))).unwrap_or_else(|| panic!("no menu entry {label:?}")).center()
}

#[test]
fn dragging_a_rollout_title_moves_the_rollout_and_the_order_stays() {
    let mut h = Harness::new();
    box_panel(&mut h);
    assert!(title_y(&h, "Keyboard Entry") > title_y(&h, "Object Type"));
    let ke = h.widget("Keyboard Entry");
    let ot = h.widget("Object Type");
    h.drag(ke.center(), egui::pos2(ke.center().x, ot.min.y - 4.0));
    h.frames(4);
    let order = h.app.state.ui_custom.rollout_order.get("Create").cloned().expect("the order is kept for the Create panel");
    assert_eq!(order.first().map(String::as_str), Some("Keyboard Entry"), "{order:?}");
    assert!(title_y(&h, "Keyboard Entry") < title_y(&h, "Object Type"), "Keyboard Entry is drawn first now");
    // The rollouts still work where they are: open Keyboard Entry and see its Create button.
    h.click_widget("Keyboard Entry");
    h.frames(3);
    assert!(h.widgets_labelled("Length:").len() + h.widgets_labelled("Create").len() > 0, "its contents show under its title");
    // Reset Rollout Order from the right-click menu.
    let rc_at = h.widget("Object Type").center();
    right_click(&mut h, rc_at);
    h.click(menu_entry(&h, "Reset Rollout Order"));
    h.frames(3);
    assert!(!h.app.state.ui_custom.rollout_order.contains_key("Create"));
    assert!(title_y(&h, "Keyboard Entry") > title_y(&h, "Object Type"), "back in the program's order");
}

#[test]
fn the_rollout_menu_closes_and_opens_every_rollout() {
    let mut h = Harness::new();
    box_panel(&mut h);
    assert!(!h.widgets_labelled("AutoGrid").is_empty(), "Object Type is open");
    let rc_at = h.widget("Name and Color").center();
    right_click(&mut h, rc_at);
    h.click(menu_entry(&h, "Close All"));
    h.frames(3);
    assert!(h.widgets_labelled("AutoGrid").is_empty(), "Close All closed Object Type");
    let rc_at = h.widget("Creation Method").center();
    right_click(&mut h, rc_at);
    h.click(menu_entry(&h, "Open All"));
    h.frames(3);
    assert!(!h.widgets_labelled("AutoGrid").is_empty(), "Open All opened it again");
    // The menu lists the rollouts: picking one opens it.
    let rc_at = h.widget("Object Type").center();
    right_click(&mut h, rc_at);
    h.click(menu_entry(&h, "Close All"));
    h.frames(2);
    let rc_at = h.widget("Object Type").center();
    right_click(&mut h, rc_at);
    h.click(menu_entry(&h, "Object Type"));
    h.frames(3);
    assert!(!h.widgets_labelled("AutoGrid").is_empty(), "the picked rollout opens");
}

#[test]
fn dragging_the_panel_pans_it_when_it_is_taller_than_the_window() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 520.0));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    // The Display panel is taller than this window.
    h.app.panel_tab = crate::Tab::Display;
    h.frames(4);
    // A checkbox senses clicks only: a drag over it pans the panel.
    let panel = egui::containers::panel::PanelState::load(&h.ctx, egui::Id::new("command_panel")).unwrap().rect;
    let (text, grab) = h
        .widgets
        .iter()
        .filter(|(r, t, w)| *r == Role::CheckBox && !t.trim().is_empty() && panel.contains_rect(*w) && w.center().y > panel.center().y)
        .max_by(|a, b| a.2.center().y.total_cmp(&b.2.center().y))
        .map(|(_, t, w)| (t.clone(), *w))
        .unwrap_or_else(|| panic!("a label low in the panel: {:?}", h.widgets.iter().filter(|w| panel.contains_rect(w.2)).map(|w| (w.0, w.1.clone())).collect::<Vec<_>>()));
    let before = grab.center().y;
    h.drag(grab.center(), grab.center() - egui::vec2(0.0, 120.0));
    h.frames(2);
    let after = h.widgets.iter().filter(|(r, t, w)| *r == Role::CheckBox && *t == text && panel.contains_rect(*w)).map(|w| w.2.center().y).fold(f32::MAX, f32::min);
    assert!(after < before - 60.0, "the panel scrolled up under the drag: {text:?} {before} -> {after}");
}

#[test]
fn hierarchy_motion_and_display_panels_show_the_name_and_colour() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let id = *h.app.state.selection.iter().next().unwrap();
    let name = h.app.state.scene.node(id).unwrap().name.clone();
    for tab in [crate::Tab::Hierarchy, crate::Tab::Motion, crate::Tab::Display] {
        h.app.panel_tab = tab;
        h.frames(3);
        let cp = egui::containers::panel::PanelState::load(&h.ctx, egui::Id::new("command_panel")).unwrap().rect;
        assert!(h.text_rect(&name, cp).is_some(), "{tab:?} shows {name}");
        assert!(h.text_rect("Name and Color", cp).is_some(), "{tab:?} has the Name and Color rollout");
    }
}
