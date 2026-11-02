//! The Color Selector a colour swatch opens: its RGB spinners, the hue
//! rainbow, Reset and Close, changing the colour while it is open.

use super::Harness;
use eframe::egui;
use egui::accesskit::Role;

#[test]
fn a_swatch_opens_the_color_selector_and_its_controls_set_the_colour() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1000.0));
    h.key(egui::Key::Num8);
    h.frames(2);
    let label = h.find_all(Role::Label, "Color:").into_iter().min_by(|a, b| a.min.y.total_cmp(&b.min.y)).expect("Background > Color:");
    let swatch = h.widgets.iter().filter(|(r, _, w)| *r == Role::ColorWell && w.min.x >= label.max.x - 1.0 && (w.center().y - label.center().y).abs() < label.height()).min_by(|a, b| a.2.min.x.total_cmp(&b.2.min.x)).map(|w| w.2).expect("the Background colour swatch");
    h.click(swatch.center());
    h.frames(2);
    assert!(h.has(Role::Label, "Red:") && h.has(Role::Label, "Value:"), "the Color Selector shows RGB and HSV");
    let background = |h: &Harness| h.app.state.scene.environment.background.get().to_array();

    // The Red spinner's up arrow: one step of 255.
    let red = h.spinner_after("Red:");
    h.click(egui::pos2(red.max.x + crate::spinner::ARROW_WIDTH / 2.0, red.center().y - red.height() / 4.0));
    assert!((background(&h)[0] - 1.0 / 255.0).abs() < 1e-4, "red up one: {:?}", background(&h));

    // The hue rainbow's top-left corner: pure red (hue 0, no blackness; a
    // couple of pixels in, a trace of green).
    let black = h.find(Role::Slider, "Blackness");
    h.click(egui::pos2(black.max.x + 10.0, black.min.y + 1.0));
    let c = background(&h);
    assert!(c[0] > 0.95 && c[1] < 0.1 && c[2] < 0.05, "pure red: {c:?}");

    // Reset: the colour the dialog opened with; Close keeps the colour and closes.
    let below = |h: &Harness, name: &str| h.find_all(Role::Button, name).into_iter().find(|b| b.min.y > red.max.y).unwrap_or_else(|| panic!("the selector's {name}"));
    h.click(below(&h, "Reset").center());
    assert_eq!(background(&h), [0.0; 3], "back to black");
    h.click(below(&h, "Close").center());
    h.frames(1);
    assert!(!h.has(Role::Label, "Red:"), "closed");
}
