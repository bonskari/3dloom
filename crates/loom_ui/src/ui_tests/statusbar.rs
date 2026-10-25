//! The status bar as a person sees and uses it: the status line's count and
//! type, the coordinate display following the cursor, the grid size in
//! units, button tooltips on the prompt line and the Time Tag box.

use super::*;
use egui::accesskit::Role;

fn statusbar(h: &Harness) -> egui::Rect {
    egui::containers::panel::PanelState::load(&h.ctx, egui::Id::new("statusbar")).expect("the status bar").rect
}

fn shows(h: &Harness, text: &str) -> bool {
    h.text_rect(text, statusbar(h)).is_some()
}

#[test]
fn the_status_line_counts_the_selection_by_type() {
    let mut h = Harness::new();
    assert!(shows(&h, "None Selected"));
    h.cmd(Command::CreateCamera { target: false });
    assert!(shows(&h, "1 Camera Selected"), "one camera");
    let cam = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::CreateCamera { target: false });
    let cam2 = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::SelectNodes { ids: vec![cam.0, cam2.0], op: loom_commands::SelectOp::Replace });
    assert!(shows(&h, "2 Cameras Selected"), "all of one type: that type");
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let b = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::SelectNodes { ids: vec![cam.0, b.0], op: loom_commands::SelectOp::Replace });
    assert!(shows(&h, "2 Objects Selected"), "mixed types: objects");
}

#[test]
fn the_grid_size_shows_in_the_display_units() {
    let mut h = Harness::new();
    h.cmd(Command::SetUnits { system_unit: None, system_scale: None, display: Some("Centimeters".into()), custom_name: None, custom_value: None, custom_unit: None, fraction: None, default_feet: None, respect_system_units: None });
    let expect = format!("Grid = {}", h.app.state.format_units(h.app.state.home_grid.spacing));
    assert!(expect.contains("cm"), "{expect}");
    assert!(shows(&h, &expect));
}

#[test]
fn with_nothing_selected_the_coordinates_follow_the_cursor() {
    let mut h = Harness::new();
    maximized(&mut h);
    let p = h.screen_of(Vec3::new(20.0, 10.0, 0.0));
    h.run(vec![egui::Event::PointerMoved(p)]);
    h.frames(2);
    let w = crate::viewport_widget::cursor_world(&h.app, p).expect("the cursor is over the active viewport");
    assert!((w.x - 20.0).abs() < 0.5 && (w.y - 10.0).abs() < 0.5, "{w:?}");
    let x = h.app.state.format_units(w.x);
    assert!(shows(&h, &x), "the X field shows {x}");
}

#[test]
fn a_toolbar_buttons_tooltip_shows_on_the_prompt_line() {
    let mut h = Harness::new();
    let mv = h.find(Role::Button, "Select and Move");
    h.run(vec![egui::Event::PointerMoved(mv.center())]);
    h.frames(3);
    assert!(shows(&h, "Select and Move (W)"), "the prompt line repeats the tooltip");
    h.run(vec![egui::Event::PointerMoved(egui::pos2(600.0, 400.0))]);
    h.frames(3);
    assert!(!shows(&h, "Select and Move (W)"));
}

#[test]
fn the_time_tag_box_adds_a_tag_and_jumps_to_it() {
    let mut h = Harness::new();
    h.cmd(Command::SetFrame { frame: 40 });
    let combo = h.find(Role::ComboBox, "Time Tag");
    h.click(combo.center());
    h.click(h.find(Role::Button, "Add Tag").center());
    h.frames(2);
    let win = h.ctx.memory(|m| m.area_rect(egui::Id::new("Add Time Tag"))).expect("the Add Time Tag dialog");
    h.click(h.text_rect("OK", win).unwrap().center());
    assert_eq!(h.app.state.time_tags.len(), 1);
    assert_eq!(loom_commands::time_tags::tag_time(&h.app.state.time_tags, "Tag 1"), Some(40));
    h.cmd(Command::SetFrame { frame: 5 });
    h.click(combo.center());
    h.click(h.find(Role::Button, "Tag 1  (40)").center());
    assert_eq!(h.app.state.current_frame, 40, "picking the tag jumps to it");
}
