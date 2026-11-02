//! Spinner arrows: click steps (by Spinner Snap when it is on), holding
//! repeats, dragging changes the value (Ctrl faster), a right-click sets the
//! minimum.

use super::*;

/// A new Box in the Modify panel: its Length 10 and Length Segs 1.
fn box_in_modify() -> Harness {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1400.0));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    h.app.panel_tab = crate::Tab::Modify;
    h.frames(2);
    h
}

/// The spinner's up and down arrows (right of its field).
fn arrows(field: egui::Rect) -> (egui::Pos2, egui::Pos2) {
    let x = field.max.x + crate::spinner::ARROW_WIDTH / 2.0;
    (egui::pos2(x, field.center().y - field.height() / 4.0), egui::pos2(x, field.center().y + field.height() / 4.0))
}

fn press(h: &mut Harness, p: egui::Pos2, pressed: bool) {
    let m = h.mods;
    h.run(vec![egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: m }]);
}

#[test]
fn arrow_clicks_step_the_value_and_spinner_snap_sets_the_step() {
    let mut h = box_in_modify();
    let (up, down) = arrows(h.spinner_after("Length Segs"));
    h.click(up);
    assert_eq!(param(&h, "Length Segs"), 2.0, "the up arrow adds one");
    h.click(up);
    h.click(down);
    assert_eq!(param(&h, "Length Segs"), 2.0, "the down arrow takes one away");

    // Spinner Snap on: each click moves Length by the snap value.
    h.app.state.prefs.spinner_snap = true;
    h.app.state.prefs.spinner_snap_value = 5.0;
    h.frames(1);
    let (up, down) = arrows(h.spinner_after("Length"));
    h.click(up);
    assert!((param(&h, "Length") - 15.0).abs() < 1e-4, "10 + snap 5: {}", param(&h, "Length"));
    h.click(down);
    h.click(down);
    assert!((param(&h, "Length") - 5.0).abs() < 1e-4, "two steps down: {}", param(&h, "Length"));
    // A drag is not snapped: 10 points move Length by its own speed, not by 5 a point.
    let field = h.spinner_after("Length");
    h.drag(field.center(), field.center() + egui::vec2(10.0, 0.0));
    let moved = param(&h, "Length") - 5.0;
    assert!(moved > 0.0 && moved < 10.0, "drag speed unchanged by Spinner Snap: {moved}");
}

#[test]
fn holding_an_arrow_repeats_and_dragging_it_changes_the_value() {
    let mut h = box_in_modify();
    let (up, _) = arrows(h.spinner_after("Length Segs"));
    h.run(vec![egui::Event::PointerMoved(up)]);
    press(&mut h, up, true);
    assert_eq!(param(&h, "Length Segs"), 2.0, "the press steps at once");
    h.frames(8);
    assert_eq!(param(&h, "Length Segs"), 2.0, "no repeat before the hold time");
    h.frames(20);
    press(&mut h, up, false);
    assert!(param(&h, "Length Segs") >= 6.0, "held: it keeps stepping: {}", param(&h, "Length Segs"));

    // Dragging up from an arrow increases the value with the pointer's height; Ctrl faster.
    let drag = |h: &mut Harness, mods: egui::Modifiers| {
        let (up, _) = arrows(h.spinner_after("Length"));
        let before = param(h, "Length");
        h.mods = mods;
        h.drag(up, up - egui::vec2(0.0, 40.0));
        h.mods = egui::Modifiers::NONE;
        param(h, "Length") - before
    };
    let plain = drag(&mut h, egui::Modifiers::NONE);
    let fast = drag(&mut h, egui::Modifiers::CTRL);
    assert!(plain > 0.0, "dragging up increases it: {plain}");
    assert!(fast > plain * 5.0, "Ctrl drags faster: {fast} vs {plain}");
    let (_, down) = arrows(h.spinner_after("Length"));
    let before = param(&h, "Length");
    h.drag(down, down + egui::vec2(0.0, 40.0));
    assert!(param(&h, "Length") < before, "dragging down decreases it");
}

#[test]
fn right_click_on_the_arrows_sets_the_minimum() {
    let mut h = box_in_modify();
    let (up, _) = arrows(h.spinner_after("Length Segs"));
    h.click(up);
    h.click(up);
    assert_eq!(param(&h, "Length Segs"), 3.0);
    right_click(&mut h, up);
    assert_eq!(param(&h, "Length Segs"), 1.0, "Length Segs' minimum");
    let (_, down) = arrows(h.spinner_after("Length"));
    right_click(&mut h, down);
    assert_eq!(param(&h, "Length"), 0.0, "Length goes to 0");
}

/// Every spinner has its arrows: a dialog's raw number field too (Render
/// Setup's output Width).
#[test]
fn dialog_number_fields_have_arrows() {
    let mut h = Harness::new();
    h.cmd(Command::RenderSetup { width: None, height: None, image_aspect: None, pixel_aspect: None, lock_image_aspect: None, lock_pixel_aspect: None, render_hidden: None, force_two_sided: None, atmospherics: None, effects: None, save_file: None, output: None });
    h.frames(2);
    let width = h.app.state.scene.render.width;
    let (up, _) = arrows(h.spinner_after("Width:"));
    h.click(up);
    assert_eq!(h.app.state.scene.render.width, width + 1, "the Width arrow steps it");
}

/// Modeling tutorial: typing in a spinner field and pressing Tab commits it
/// and moves the focus to the next field (Length → Width).
#[test]
fn tab_moves_from_one_spinner_field_to_the_next() {
    let mut h = box_in_modify();
    let length = h.spinner_after("Length");
    h.click(length.center());
    h.mods = egui::Modifiers::COMMAND;
    h.key(egui::Key::A);
    h.mods = egui::Modifiers::NONE;
    h.run(vec![egui::Event::Text("24".into())]);
    h.key(egui::Key::Tab);
    h.mods = egui::Modifiers::COMMAND;
    h.key(egui::Key::A);
    h.mods = egui::Modifiers::NONE;
    h.run(vec![egui::Event::Text("17".into())]);
    h.key(egui::Key::Enter);
    h.frames(2);
    assert!((param(&h, "Length") - 24.0).abs() < 1e-4, "Tab committed Length: {}", param(&h, "Length"));
    assert!((param(&h, "Width") - 17.0).abs() < 1e-4, "the typing went on in Width: {}", param(&h, "Width"));
}
