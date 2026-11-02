//! Preferences, Time Configuration, Units Setup, Configure User Paths and the
//! Custom UI and Defaults Switcher used as a person does, and the viewport
//! preferences they drive (world axis tripod, object name tooltips, large
//! toolbar buttons, click-and-hold flyouts).

use super::*;
use egui::accesskit::Role;

fn window(h: &Harness, id: &str) -> egui::Rect {
    h.ctx.memory(|m| m.area_rect(egui::Id::new(id))).unwrap_or_else(|| panic!("no window {id}"))
}

#[test]
fn preferences_cancel_puts_the_values_back() {
    let mut h = Harness::new();
    h.cmd(Command::Preferences);
    let win = window(&h, "Preference Settings");
    h.click(h.text_rect("Viewports", win).unwrap().center());
    let win = window(&h, "Preference Settings");
    assert!(h.app.state.prefs.display_world_axis);
    h.click(h.text_rect("Display World Axis", win).expect("the checkbox").center());
    assert!(!h.app.state.prefs.display_world_axis, "the change applies at once");
    h.click(h.text_rect("Cancel", window(&h, "Preference Settings")).unwrap().center());
    assert!(h.app.state.prefs.display_world_axis, "Cancel puts it back");
    assert_eq!(h.app.state.dialog, None);
}

#[test]
fn time_configuration_cancel_puts_the_range_back() {
    let mut h = Harness::new();
    h.cmd(Command::TimeConfiguration);
    h.cmd(Command::SetTimeConfiguration { start: Some(5), end: Some(50), fps: Some(25.0) });
    assert_eq!(h.app.state.frame_range, (5, 50));
    h.click(h.text_rect("Cancel", window(&h, "Time Configuration")).unwrap().center());
    assert_eq!((h.app.state.frame_range, h.app.state.fps), ((0, 100), 30.0));
    h.cmd(Command::TimeConfiguration);
    h.cmd(Command::SetTimeConfiguration { start: Some(5), end: None, fps: None });
    h.click(h.text_rect("OK", window(&h, "Time Configuration")).unwrap().center());
    assert_eq!(h.app.state.frame_range.0, 5, "OK keeps it");
}

#[test]
fn holding_a_flyout_button_opens_it_and_the_release_picks_an_entry() {
    let mut h = Harness::new();
    let orbit = h.find(Role::Button, "Orbit View Mode");
    let p = orbit.center();
    h.run(vec![egui::Event::PointerMoved(p)]);
    h.run(vec![egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE }]);
    h.frames(15); // half a second held
    let entry = h.find_all(Role::Button, "Orbit Selected").into_iter().find(|r| r.center().y < orbit.min.y).expect("the flyout opened above the button");
    h.run(vec![egui::Event::PointerMoved(entry.center())]);
    h.run(vec![egui::Event::PointerButton { pos: entry.center(), button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE }]);
    h.frames(2);
    let open: Vec<egui::Rect> = menus(&h);
    assert_eq!(h.app.state.nav_mode, loom_input::NavMode::OrbitSelected, "releasing over the entry {entry:?} picks it (popups after: {open:?})");
}

#[test]
fn the_world_axis_tripod_and_object_name_tooltips_show_in_the_views() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [30.0; 3] });
    let name = h.app.state.scene.nodes().next().unwrap().name.clone();
    h.cmd(Command::SelectNone);
    maximized(&mut h);
    let r = h.app.viewport_rects[h.app.state.active_viewport];
    let corner = egui::Rect::from_min_max(egui::pos2(r.left(), r.bottom() - 70.0), egui::pos2(r.left() + 70.0, r.bottom()));
    for axis in ["X", "Y", "Z"] {
        assert!(h.text_rect(axis, corner).is_some(), "the tripod labels {axis}");
    }
    h.cmd(Command::SetPreference { name: "Display World Axis".into(), value: 0.0 });
    assert!(h.text_rect("Z", corner).is_none(), "off: no tripod");
    // Rest the pointer on the box: its name shows.
    let p = h.screen_of(Vec3::ZERO);
    h.run(vec![egui::Event::PointerMoved(p)]);
    h.frames(25);
    assert!(h.text_rects(&name).iter().any(|t| (t.center() - p).length() < 80.0), "the name {name} shows by the pointer");
}

#[test]
fn large_toolbar_buttons_make_the_icons_bigger() {
    let mut h = Harness::new();
    let small = h.find(Role::Button, "Select and Move").height();
    h.cmd(Command::SetPreference { name: "Large Toolbar Buttons".into(), value: 1.0 });
    h.frames(2);
    assert!(h.find(Role::Button, "Select and Move").height() > small + 4.0);
}

#[test]
fn configure_user_paths_sets_the_scenes_folder() {
    let mut h = Harness::new();
    let dir = std::env::temp_dir().join("loom-ui-paths-test");
    std::fs::create_dir_all(&dir).unwrap();
    h.cmd(Command::ConfigureUserPaths);
    let win = window(&h, "Configure User Paths");
    h.click(h.text_rect("Scenes", win).unwrap().center());
    let field = h.text_rect("folder", window(&h, "Configure User Paths")).expect("the folder field");
    h.click(field.center());
    h.run(vec![egui::Event::Text(dir.to_string_lossy().into_owned())]);
    h.click(h.text_rect("Modify", window(&h, "Configure User Paths")).unwrap().center());
    assert_eq!(h.app.state.paths.dir("Scenes"), Some(dir.as_path()));
}

#[test]
fn system_unit_setup_opens_from_units_setup() {
    let mut h = Harness::new();
    h.cmd(Command::UnitsSetup);
    h.click(h.text_rect("System Unit Setup", window(&h, "Units Setup")).unwrap().center());
    h.frames(2);
    let w = window(&h, "System Unit Setup");
    assert!(h.text_rect("Respect System Units in Files", w).is_some());
    h.click(h.text_rect("Respect System Units in Files", w).unwrap().center());
    assert!(!h.app.state.default_units.respect_system_units);
}

#[test]
fn the_switcher_sets_the_modular_toolbars_scheme() {
    let mut h = Harness::new();
    h.cmd(Command::CustomUiDefaultsSwitcher);
    let win = window(&h, "Custom UI and Defaults Switcher");
    h.click(h.text_rect("Modular Toolbars UI", win).unwrap().center());
    h.click(h.text_rect("Set", window(&h, "Custom UI and Defaults Switcher")).unwrap().center());
    h.frames(2);
    assert!(loom_commands::ui_custom::is_shown(&h.app.state, "Axis Constraints"));
    assert!(h.has(Role::Button, "Restrict to X"), "its toolbar is on screen");
}

/// A point on a gizmo handle of the active tool, searched outward from the pivot.
fn gizmo_handle_point(h: &Harness, want: impl Fn(crate::gizmo::Handle) -> bool) -> Option<egui::Pos2> {
    let i = h.app.state.active_viewport;
    let rect = h.app.viewport_rects[i];
    let cam = h.app.camera_for(i);
    let projection = crate::gizmo::Projection::new(cam.view_matrix(), cam.projection_matrix(rect.width() / rect.height()), rect);
    let layout = crate::gizmo::layout_for(&h.app, i, rect.height());
    let o = h.screen_of(layout.origin);
    (0..72)
        .flat_map(|a| (4..250).map(move |d| (a, d)))
        .map(|(a, d)| o + egui::Vec2::angled((a as f32 * 5.0).to_radians()) * d as f32)
        .find(|p| crate::gizmo::hit(&h.app, &layout, &projection, *p).is_some_and(&want))
}

/// How far right of the pivot the active gizmo still answers the pointer.
fn gizmo_reach(h: &Harness) -> f32 {
    let i = h.app.state.active_viewport;
    let rect = h.app.viewport_rects[i];
    let cam = h.app.camera_for(i);
    let projection = crate::gizmo::Projection::new(cam.view_matrix(), cam.projection_matrix(rect.width() / rect.height()), rect);
    let layout = crate::gizmo::layout_for(&h.app, i, rect.height());
    let o = h.screen_of(layout.origin);
    (4..400).filter(|d| (0..72).any(|a| crate::gizmo::hit(&h.app, &layout, &projection, o + egui::Vec2::angled((a as f32 * 5.0).to_radians()) * *d as f32).is_some())).max().unwrap_or(0) as f32
}

/// Preferences > Gizmos: Plane Handles On off removes the move gizmo's plane
/// squares, Uniform 2-Axis Scaling makes a scale plane handle uniform, and
/// the Move Relative Size spinner sizes the move gizmo.
#[test]
fn gizmo_preferences_plane_handles_uniform_two_axis_and_relative_size() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1000.0));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [30.0; 3] });
    let id = *h.app.state.selection.iter().next().unwrap();
    framed(&mut h);
    h.key(egui::Key::W);
    let before = gizmo_reach(&h);
    h.cmd(Command::Preferences);
    h.click(h.text_rect("Gizmos", window(&h, "Preference Settings")).unwrap().center());
    h.frames(2);
    // Plane Handles "On" sits below the Transform Gizmos "On".
    let ons = h.find_all(Role::CheckBox, "On");
    assert_eq!(ons.len(), 2, "Transform Gizmos On and Plane Handles On");
    h.click(ons[1].center());
    assert!(!h.app.state.prefs.gizmos.move_plane_handles, "clicked {:?} in {:?}: {:?}", ons[1], window(&h, "Preference Settings"), h.widgets.iter().filter(|w| w.0 == Role::CheckBox || w.0 == Role::Label).map(|w| (w.1.clone(), w.2.min)).collect::<Vec<_>>());
    let r = h.find(Role::CheckBox, "Uniform 2-Axis Scaling");
    h.click(r.center());
    assert!(h.app.state.prefs.gizmos.uniform_two_axis_scaling, "clicked {r:?} in {:?}", window(&h, "Preference Settings"));
    // The first Relative Size spinner is the Move gizmo's.
    let spin = h.spinner_after("Relative Size (%):");
    h.drag(spin.center(), spin.center() + egui::vec2(0.0, -300.0));
    assert!(h.app.state.prefs.gizmos.move_size > 110.0, "dragging the spinner up grows it: {}", h.app.state.prefs.gizmos.move_size);
    h.click(h.text_rect("OK", window(&h, "Preference Settings")).unwrap().center());
    assert_eq!(h.app.state.dialog, None);

    assert!(gizmo_handle_point(&h, |g| matches!(g, crate::gizmo::Handle::Plane(_))).is_none(), "no move plane handles");
    let after = gizmo_reach(&h);
    assert!(after > before * 1.15, "the move gizmo grew: {before} -> {after}");

    h.key(egui::Key::R);
    h.cmd(Command::ScaleType { kind: loom_commands::ScaleKind::NonUniform });
    let p = gizmo_handle_point(&h, |g| matches!(g, crate::gizmo::Handle::Plane(_))).expect("a scale 2-axis handle");
    h.drag(p, p + egui::vec2(40.0, -40.0));
    let s = h.app.state.scene.node(id).unwrap().transform.scale.get();
    assert!(s.x > 1.01 && (s.x - s.y).abs() < 1e-4 && (s.x - s.z).abs() < 1e-4, "the 2-axis handle scaled uniformly: {s}");
}

/// Time Configuration: the FPS spinner is live only with Custom chosen;
/// the standard buttons set their rate.
#[test]
fn time_configuration_custom_frame_rate_radio() {
    let mut h = Harness::new();
    h.cmd(Command::TimeConfiguration);
    let spin = h.spinner_after("FPS:");
    h.drag(spin.center(), spin.center() + egui::vec2(0.0, -60.0));
    assert_eq!(h.app.state.fps, 30.0, "NTSC: the FPS spinner is off");
    h.click(h.find(Role::RadioButton, "Custom").center());
    let spin = h.spinner_after("FPS:");
    h.drag(spin.center(), spin.center() + egui::vec2(0.0, -60.0));
    assert!(h.app.state.fps > 30.0, "Custom: the spinner sets the rate ({})", h.app.state.fps);
    h.click(h.find(Role::RadioButton, "PAL").center());
    assert_eq!(h.app.state.fps, 25.0);
}

#[test]
fn preferences_inverse_kinematics_tab_sets_always_transform_children_of_the_world() {
    let mut h = Harness::new();
    h.cmd(Command::Preferences);
    h.click(h.text_rect("Inverse Kinematics", window(&h, "Preference Settings")).unwrap().center());
    let win = window(&h, "Preference Settings");
    assert!(h.text_rect("Applied IK", win).is_some() && h.text_rect("Interactive IK", win).is_some());
    assert!(h.app.state.prefs.ik_transform_world_children);
    h.click(h.text_rect("Always Transform Children of the World", win).expect("the checkbox").center());
    assert!(!h.app.state.prefs.ik_transform_world_children);
}

/// Viewports > Display Driver > Antialiasing: None or 4 samples, saved with
/// the preferences; the window takes it on the next start.
#[test]
fn viewport_antialiasing_preference_switches_to_none_and_back() {
    let mut h = Harness::new();
    h.cmd(Command::Preferences);
    h.click(h.text_rect("Viewports", window(&h, "Preference Settings")).unwrap().center());
    assert_eq!(h.app.state.prefs.viewport_antialiasing, 4);
    h.click(h.text_rect("None", window(&h, "Preference Settings")).expect("the None radio").center());
    assert_eq!(h.app.state.prefs.viewport_antialiasing, 1);
    let restart = h.text_rect("Takes effect when 3dLoom restarts.", window(&h, "Preference Settings")).is_some();
    assert_eq!(restart, crate::viewport_msaa() != 1, "the dialog says when the window has another sample count");
    h.click(h.text_rect("4 Samples", window(&h, "Preference Settings")).expect("the 4 Samples radio").center());
    assert_eq!(h.app.state.prefs.viewport_antialiasing, 4);
}

/// Preferences > General > UI Display: Display Stack Collapse Warning and
/// Save UI Configuration on Exit check boxes, and the Color Selector list.
#[test]
fn general_ui_display_stack_collapse_warning_and_save_ui_configuration() {
    let mut h = Harness::new();
    h.cmd(Command::Preferences);
    let win = window(&h, "Preference Settings");
    h.click(h.text_rect("Display Stack Collapse Warning", win).expect("the checkbox").center());
    assert!(!h.app.state.prefs.stack_collapse_warning);
    let win = window(&h, "Preference Settings");
    h.click(h.text_rect("Save UI Configuration on Exit", win).expect("the checkbox").center());
    assert!(!h.app.state.prefs.save_ui_on_exit);
    let win = window(&h, "Preference Settings");
    assert!(h.text_rect("Default Color Selector", win).is_some(), "the Color Selector list");
}

/// Preferences > Animation > Auto Key Default Frame: On and the frame spinner.
#[test]
fn animation_auto_key_default_frame_controls() {
    let mut h = Harness::new();
    h.cmd(Command::Preferences);
    let win = window(&h, "Preference Settings");
    h.click(h.text_rect("Animation", win).unwrap().center());
    let win = window(&h, "Preference Settings");
    let group = h.text_rect("Auto Key Default Frame", win).expect("the group");
    let spin = h.widgets.iter().filter(|(r, _, w)| *r == Role::SpinButton && w.min.y > group.max.y && win.contains(w.center())).min_by(|a, b| a.2.min.y.total_cmp(&b.2.min.y)).map(|w| w.2).expect("the frame spinner");
    h.click(egui::pos2(spin.max.x + crate::spinner::ARROW_WIDTH / 2.0, spin.center().y - spin.height() / 4.0));
    assert_eq!(h.app.state.prefs.auto_key_start_frame, 1, "the frame goes to 1");
    let on = h.find_all(Role::CheckBox, "On").into_iter().find(|r| r.min.y > group.max.y && win.contains(r.center())).expect("On");
    h.click(on.center());
    assert!(!h.app.state.prefs.auto_key_default_frame, "On turns off");
}
