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
    assert_eq!(h.app.state.nav_mode, loom_input::NavMode::OrbitSelected, "releasing over the entry picks it");
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
