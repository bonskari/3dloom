//! Quad menus, spinners and the time controls, used as a person does.

use super::*;
use egui::accesskit::Role;

/// The quad menu entry `label` (a button inside an open quadrant).
fn quad_item(h: &Harness, label: &str) -> egui::Pos2 {
    let open = menus(h);
    let all = h.find_all(Role::Button, label);
    all.iter().copied().find(|r| open.iter().any(|m| m.contains_rect(*r))).unwrap_or_else(|| panic!("no quad entry {label:?} in {open:?}: {all:?}")).center()
}

/// A selected box in a maximized view, and a spot left of it for right-clicks.
fn box_scene(h: &mut Harness) -> egui::Pos2 {
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [30.0; 3] });
    maximized(h);
    h.app.viewport_rects[h.app.state.active_viewport].center() + egui::vec2(-150.0, 50.0)
}

#[test]
fn a_quadrant_title_repeats_its_last_command() {
    let mut h = Harness::new();
    let at = box_scene(&mut h);
    right_click(&mut h, at);
    h.click(quad_item(&h, "Rotate"));
    assert_eq!(h.app.state.active_tool, loom_commands::Tool::Rotate, "the item runs");
    assert!(h.app.quad_menu.is_none());
    h.cmd(Command::SelectAndMove);
    right_click(&mut h, at);
    h.click(quad_item(&h, "transform"));
    assert_eq!(h.app.state.active_tool, loom_commands::Tool::Rotate, "the transform title repeats Rotate");
    assert!(h.app.quad_menu.is_none(), "and closes the menu");
    // The display title has no last command of its own: a click there runs nothing.
    right_click(&mut h, at);
    h.click(quad_item(&h, "display"));
    let id = *h.app.state.selection.iter().next().unwrap();
    assert!(!h.app.state.scene.node(id).unwrap().hidden && h.app.state.active_tool == loom_commands::Tool::Rotate);
}

#[test]
fn the_move_settings_icon_opens_transform_type_in() {
    let mut h = Harness::new();
    let at = box_scene(&mut h);
    h.cmd(Command::SelectAndRotate);
    right_click(&mut h, at);
    h.click(quad_item(&h, "Move Settings"));
    assert_eq!(h.app.state.active_tool, loom_commands::Tool::Move);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::TransformTypeIn));
    // Again with the dialog open: it stays open.
    right_click(&mut h, at);
    h.click(quad_item(&h, "Scale Settings"));
    assert_eq!((h.app.state.active_tool, h.app.state.dialog), (loom_commands::Tool::Scale, Some(loom_commands::Dialog::TransformTypeIn)));
}

#[test]
fn ctrl_alt_right_click_opens_the_render_quad() {
    let mut h = Harness::new();
    let at = box_scene(&mut h);
    let id = *h.app.state.selection.iter().next().unwrap();
    h.mods = egui::Modifiers::CTRL | egui::Modifiers::ALT;
    right_click(&mut h, at);
    h.mods = egui::Modifiers::NONE;
    assert_eq!(h.app.quad_menu.map(|o| o.kind), Some(crate::quad_menu::QuadKind::Render));
    quad_item(&h, "Render Setup...");
    h.click(quad_item(&h, "✔ Cast Shadows"));
    assert!(!h.app.state.scene.node(id).unwrap().render.cast_shadows, "Cast Shadows turned off for the selection");
}

#[test]
fn alt_right_click_quad_sets_the_reference_coordinate_system() {
    let mut h = Harness::new();
    let at = box_scene(&mut h);
    h.mods = egui::Modifiers::ALT;
    right_click(&mut h, at);
    h.mods = egui::Modifiers::NONE;
    h.click(quad_item(&h, "Local"));
    assert_eq!(h.app.state.coord_system, loom_commands::CoordSystem::Local);
}

#[test]
fn right_click_on_spinner_snap_opens_preferences() {
    let mut h = Harness::new();
    right_click(&mut h, h.find(Role::Button, "Spinner Snap Toggle").center());
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::Preferences));
    assert!(!h.app.state.prefs.spinner_snap, "the right-click does not toggle it");
}

/// Drags the status bar X field `dx` to the right with `mods` held; returns the box's X.
fn drag_x_field(mods: egui::Modifiers, dx: f32) -> f32 {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    h.key(egui::Key::W);
    let x = h.spinner_after("X:").center();
    h.mods = mods;
    h.drag(x, x + egui::vec2(dx, 0.0));
    h.mods = egui::Modifiers::NONE;
    let id = *h.app.state.selection.iter().next().unwrap();
    h.app.state.scene.node(id).unwrap().transform.position.get().x
}

#[test]
fn ctrl_drag_on_a_spinner_is_faster_and_alt_drag_slower() {
    let plain = drag_x_field(egui::Modifiers::NONE, 40.0);
    let fast = drag_x_field(egui::Modifiers::CTRL, 40.0);
    let slow = drag_x_field(egui::Modifiers::ALT, 40.0);
    assert!(plain > 0.5, "a plain drag moves the box: {plain}");
    assert!(fast > plain * 5.0, "Ctrl: {fast} vs {plain}");
    assert!(slow < plain * 0.5 && slow > 0.0, "Alt: {slow} vs {plain}");
}
