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
    let p = h.find(Role::Button, "Spinner Snap Toggle").center();
    right_click(&mut h, p);
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

/// The time slider's "frame / end" thumb.
fn slider_thumb(h: &Harness) -> egui::Pos2 {
    let label = format!("{} / {}", h.app.state.current_frame, h.app.state.frame_range.1);
    h.text_rects(&label).first().unwrap_or_else(|| panic!("no thumb {label:?}")).center()
}

#[test]
fn right_click_on_the_time_slider_opens_create_key() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let p = slider_thumb(&h);
    right_click(&mut h, p);
    let d = h.app.track_bar.create_key.expect("the Create Key dialog opens");
    assert_eq!((d.source, d.destination), (0, 0));
    h.click_text_anywhere("Cancel");
    assert!(h.app.track_bar.create_key.is_none(), "Cancel closes it");
    assert!(loom_commands::anim_cmds::selection_key_times(&h.app.state).is_empty(), "and creates nothing");
    // A right-drag: source is where it started, destination where it ended.
    let from = slider_thumb(&h);
    h.drag_with(egui::PointerButton::Secondary, from, from + egui::vec2(200.0, 0.0));
    let d = h.app.track_bar.create_key.expect("the dialog opens after a right-drag");
    assert_eq!(d.source, 0);
    assert!(d.destination > 0 && d.destination == h.app.state.current_frame, "{d:?}");
    h.click_text_anywhere("OK");
    assert!(h.app.track_bar.create_key.is_none());
    let times = loom_commands::anim_cmds::selection_key_times(&h.app.state);
    assert!(times.contains(&(d.destination as f32)), "a key at the destination: {times:?}");
}

#[test]
fn the_track_bar_opens_and_closes_the_mini_curve_editor() {
    let mut h = Harness::new();
    h.click(h.find(Role::Button, "Open Mini Curve Editor").center());
    assert!(h.app.track_bar.mini_curve_editor);
    assert!(h.has(Role::Button, "Close Mini Curve Editor") && !h.has(Role::Button, "Open Mini Curve Editor"), "it replaces the track bar");
    assert!(!h.text_rects("Mini Curve Editor").is_empty());
    h.click(h.find(Role::Button, "Close Mini Curve Editor").center());
    assert!(!h.app.track_bar.mini_curve_editor && h.has(Role::Button, "Open Mini Curve Editor"));
}

#[test]
fn right_click_on_a_time_control_opens_time_configuration() {
    let mut h = Harness::new();
    let p = h.find(Role::Button, "Go to Start Frame").center();
    right_click(&mut h, p);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::TimeConfiguration));
    assert!(!h.app.state.playing);
}

/// Opens a menu bar menu and its submenus, then clicks the item (entries
/// inside open menus only, not panel buttons of the same name).
fn menu_path(h: &mut Harness, path: &[&str]) {
    let title = h.find_all(Role::Button, path[0]).into_iter().find(|r| r.max.y < 30.0).unwrap_or_else(|| panic!("no menu {}", path[0]));
    h.click(title.center());
    for (i, label) in path[1..].iter().enumerate() {
        let open = menus(h);
        let all = h.find_all(Role::Button, label);
        let p = all.iter().copied().find(|r| open.iter().any(|m| m.contains_rect(*r))).unwrap_or_else(|| panic!("no menu entry {label:?} in {open:?}: {all:?}")).center();
        if i + 2 < path.len() {
            h.run(vec![egui::Event::PointerMoved(p)]);
            h.frames(2);
        } else {
            h.click(p);
        }
    }
}

#[test]
fn menu_bar_menus_are_in_the_reference_order() {
    let h = Harness::new();
    let order = ["File", "Edit", "Tools", "Group", "Views", "Create", "Modifiers", "Animation", "Graph Editors", "Rendering", "Customize", "Help"];
    let xs: Vec<f32> = order.iter().map(|m| h.find_all(Role::Button, m).into_iter().find(|r| r.max.y < 30.0).unwrap_or_else(|| panic!("no menu {m}")).min.x).collect();
    assert!(xs.windows(2).all(|w| w[0] < w[1]), "{order:?} at {xs:?}");
}

#[test]
fn edit_menu_select_by_color_region_and_instances() {
    let mut h = Harness::new();
    menu_path(&mut h, &["Edit", "Select By", "Color"]);
    assert_eq!(h.app.state.pick_mode, Some(loom_commands::PickMode::SelectByColor));
    h.cmd(Command::ExitPickMode);
    menu_path(&mut h, &["Edit", "Region", "Circular Region"]);
    assert_eq!(h.app.state.selection_region, loom_commands::SelectionRegion::Circular);
    let window = h.app.state.window_selection;
    menu_path(&mut h, &["Edit", "Region", if window { "Crossing" } else { "Window" }]);
    assert_ne!(h.app.state.window_selection, window);
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let a = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::CloneOptions { kind: loom_commands::CloneKind::Instance, copies: 1, name: None, offset: Some([20.0, 0.0, 0.0]), rotate_deg: None, scale: None });
    h.cmd(Command::SelectNodes { ids: vec![a.0], op: loom_commands::SelectOp::Replace });
    menu_path(&mut h, &["Edit", "Select Instances"]);
    assert_eq!(h.app.state.selection.len(), 2, "the box and its instance");
    menu_path(&mut h, &["Edit", "Rotate"]);
    assert_eq!(h.app.state.active_tool, loom_commands::Tool::Rotate);
}

#[test]
fn tools_menu_align_holds_the_spacing_tool_and_grids_snaps_use_axis_constraints() {
    let mut h = Harness::new();
    let axis = h.app.state.snaps.use_axis_constraints;
    menu_path(&mut h, &["Tools", "Grids and Snaps", "Snaps Use Axis Constraints"]);
    assert_ne!(h.app.state.snaps.use_axis_constraints, axis);
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    menu_path(&mut h, &["Tools", "Align", "Spacing Tool..."]);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::SpacingTool));
}

#[test]
fn views_menu_toggles_the_transform_gizmo() {
    let mut h = Harness::new();
    let on = h.app.state.prefs.gizmos.enabled;
    menu_path(&mut h, &["Views", "Show Transform Gizmo"]);
    assert_ne!(h.app.state.prefs.gizmos.enabled, on);
    assert!(menus(&h).is_empty(), "the menu closed");
}
