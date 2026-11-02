//! Colour swatches and the Color Selector they open: the reference's
//! modeless dialog with its three colour models kept in step. Hue /
//! Blackness / Whiteness: the hue rainbow (hue across, blackness down), the
//! Blackness slider left of it and the Whiteness bar right of it; Red /
//! Green / Blue and Hue / Sat. / Value: sliders with spinners, 0-255. The
//! Color Output box shows the colour from before the dialog opened (left)
//! and the current one (right); Reset goes back to the first, Close keeps
//! the second. Changes apply while the dialog is open.

use eframe::egui;

/// The open dialog's state: the colour it opened with, and the hue and
/// HBW / HSV values kept apart from the RGB (a grey has no hue of its own).
#[derive(Clone, Copy)]
struct Open {
    original: [u8; 3],
    hue: f32,
}

const SQUARE: f32 = 180.0;
const BAR: f32 = 14.0;
const SLIDER: f32 = 150.0;

/// A swatch for an sRGB colour; clicking it opens the Color Selector. The
/// response is changed on frames the dialog changes the colour.
pub fn srgb(ui: &mut egui::Ui, rgb: &mut [u8; 3]) -> egui::Response {
    let (rect, mut r) = ui.allocate_exact_size(ui.spacing().interact_size, egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let v = ui.style().interact(&r);
        ui.painter().rect_filled(rect.shrink(1.0), 2.0, egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]));
        ui.painter().rect_stroke(rect.shrink(0.5), 2.0, v.fg_stroke, egui::StrokeKind::Inside);
    }
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ColorButton, ui.is_enabled(), "Color"));
    let id = r.id.with("color_selector");
    let ctx = ui.ctx().clone();
    if r.clicked() {
        let (h, _, _) = to_hsv(*rgb);
        ctx.data_mut(|d| d.insert_temp(id, Open { original: *rgb, hue: h }));
    }
    let Some(mut open) = ctx.data(|d| d.get_temp::<Open>(id)) else { return r };
    let mut shown = true;
    let mut close = false;
    let before = *rgb;
    egui::Window::new("Color Selector").id(id).open(&mut shown).collapsible(false).resizable(false).default_pos(rect.right_top() + egui::vec2(12.0, 0.0)).show(&ctx, |ui| {
        body(ui, rgb, &mut open, &mut close);
    });
    if shown && !close {
        ctx.data_mut(|d| d.insert_temp(id, open));
    } else {
        ctx.data_mut(|d| d.remove::<Open>(id));
    }
    if *rgb != before {
        r.mark_changed();
    }
    r
}

/// [`srgb`] for a linear-light colour (0-1 per channel).
pub fn rgb(ui: &mut egui::Ui, c: &mut [f32; 3]) -> egui::Response {
    let mut s = egui::Color32::from(egui::Rgba::from_rgb(c[0], c[1], c[2])).to_array();
    let mut v = [s[0], s[1], s[2]];
    let r = srgb(ui, &mut v);
    if r.changed() {
        s = [v[0], v[1], v[2], 255];
        let l = egui::Rgba::from(egui::Color32::from_rgb(s[0], s[1], s[2]));
        *c = [l.r(), l.g(), l.b()];
    }
    r
}

/// [`srgb`] for a colour with alpha: the alpha stays.
pub fn srgba(ui: &mut egui::Ui, c: &mut egui::Color32) -> egui::Response {
    let mut v = [c.r(), c.g(), c.b()];
    let r = srgb(ui, &mut v);
    if r.changed() {
        *c = egui::Color32::from_rgba_unmultiplied(v[0], v[1], v[2], c.a());
    }
    r
}

/// [`srgb`] for an opaque linear `Rgba`.
pub fn rgba(ui: &mut egui::Ui, c: &mut egui::Rgba) -> egui::Response {
    let mut v = [c.r(), c.g(), c.b()];
    let r = rgb(ui, &mut v);
    if r.changed() {
        *c = egui::Rgba::from_rgb(v[0], v[1], v[2]);
    }
    r
}

fn body(ui: &mut egui::Ui, rgb: &mut [u8; 3], open: &mut Open, close: &mut bool) {
    let f = |c: [u8; 3]| c.map(|x| x as f32 / 255.0);
    let to8 = |c: [f32; 3]| c.map(|x| (x.clamp(0.0, 1.0) * 255.0).round() as u8);
    let [r, g, b] = f(*rgb);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    // Hue / Blackness / Whiteness.
    let (mut white, mut black) = (min, 1.0 - max);
    let mut hue = open.hue;
    let mut hbw = false;
    ui.horizontal(|ui| {
        hbw |= bar(ui, "Blackness", &mut black, egui::vec2(BAR, SQUARE), true, |t| {
            let c = hbw_rgb(hue, white, t);
            egui::Color32::from_rgb(c[0], c[1], c[2])
        });
        // The hue rainbow: hue across, blackness down, at the current whiteness.
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(SQUARE, SQUARE), egui::Sense::click_and_drag());
        if let Some(p) = resp.interact_pointer_pos().filter(|_| resp.is_pointer_button_down_on()) {
            hue = ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            black = ((p.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
            hbw = true;
        }
        let mut mesh = egui::Mesh::default();
        let n = 36;
        for i in 0..=n {
            let t = i as f32 / n as f32;
            let x = rect.left() + t * rect.width();
            let top = hbw_rgb(t, white, 0.0);
            let bottom = hbw_rgb(t, white, 1.0);
            mesh.colored_vertex(egui::pos2(x, rect.top()), egui::Color32::from_rgb(top[0], top[1], top[2]));
            mesh.colored_vertex(egui::pos2(x, rect.bottom()), egui::Color32::from_rgb(bottom[0], bottom[1], bottom[2]));
            if i > 0 {
                let k = (i as u32) * 2;
                mesh.add_triangle(k - 2, k - 1, k);
                mesh.add_triangle(k - 1, k + 1, k);
            }
        }
        ui.painter().add(mesh);
        let at = egui::pos2(rect.left() + hue * rect.width(), rect.top() + black * rect.height());
        ui.painter().circle_stroke(at, 4.0, egui::Stroke::new(1.0, egui::Color32::WHITE));
        ui.add_space(4.0);
        hbw |= bar(ui, "Whiteness", &mut white, egui::vec2(BAR, SQUARE), true, |t| {
            let c = hbw_rgb(hue, t, black);
            egui::Color32::from_rgb(c[0], c[1], c[2])
        });
    });
    if hbw {
        if white + black > 1.0 {
            let s = white + black;
            white /= s;
            black /= s;
        }
        *rgb = hbw_rgb(hue, white, black);
        open.hue = hue;
    }
    // Red / Green / Blue and Hue / Sat. / Value, 0-255.
    let (h, s, v) = to_hsv(*rgb);
    let h = if s > 0.0 { h } else { open.hue };
    let mut hsv = [h, s, v];
    let mut c = f(*rgb);
    let (mut rgb_changed, mut hsv_changed) = (false, false);
    egui::Grid::new("color_selector_sliders").num_columns(3).show(ui, |ui| {
        for (i, name) in ["Red:", "Green:", "Blue:"].iter().enumerate() {
            ui.label(*name);
            let base = c;
            rgb_changed |= bar(ui, name, &mut c[i], egui::vec2(SLIDER, BAR), false, |t| {
                let mut x = base;
                x[i] = t;
                let x = to8(x);
                egui::Color32::from_rgb(x[0], x[1], x[2])
            });
            let mut n = (c[i] * 255.0).round() as i32;
            if ui.add(crate::spinner::spin(&mut n, 0..=255)).changed() {
                c[i] = n as f32 / 255.0;
                rgb_changed = true;
            }
            ui.end_row();
        }
        for (i, name) in ["Hue:", "Sat.:", "Value:"].iter().enumerate() {
            ui.label(*name);
            let base = hsv;
            hsv_changed |= bar(ui, name, &mut hsv[i], egui::vec2(SLIDER, BAR), false, |t| {
                let mut x = base;
                x[i] = t;
                if i == 0 {
                    x[1] = 1.0;
                    x[2] = 1.0;
                }
                let x = from_hsv(x[0], x[1], x[2]);
                egui::Color32::from_rgb(x[0], x[1], x[2])
            });
            let mut n = (hsv[i] * 255.0).round() as i32;
            if ui.add(crate::spinner::spin(&mut n, 0..=255)).changed() {
                hsv[i] = n as f32 / 255.0;
                hsv_changed = true;
            }
            ui.end_row();
        }
    });
    if rgb_changed {
        *rgb = to8(c);
        let (h, s, _) = to_hsv(*rgb);
        if s > 0.0 {
            open.hue = h;
        }
    } else if hsv_changed {
        *rgb = from_hsv(hsv[0], hsv[1], hsv[2]);
        open.hue = hsv[0];
    }
    ui.horizontal(|ui| {
        // Color Output: the colour it opened with, then the current one.
        let (rect, _) = ui.allocate_exact_size(egui::vec2(120.0, 24.0), egui::Sense::hover());
        let (left, right) = rect.split_left_right_at_fraction(0.5);
        let o = open.original;
        ui.painter().rect_filled(left, 0.0, egui::Color32::from_rgb(o[0], o[1], o[2]));
        ui.painter().rect_filled(right, 0.0, egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]));
        if ui.button("Reset").clicked() {
            *rgb = open.original;
            open.hue = to_hsv(*rgb).0;
        }
        if ui.button("Close").clicked() {
            *close = true;
        }
    });
}

/// A colour bar with a marker at `value` (0-1): vertical bars run 0 at the
/// top; `color` gives the colour at each position. True when dragged.
fn bar(ui: &mut egui::Ui, name: &str, value: &mut f32, size: egui::Vec2, vertical: bool, color: impl Fn(f32) -> egui::Color32) -> bool {
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Slider, true, name));
    let mut changed = false;
    if let Some(p) = resp.interact_pointer_pos().filter(|_| resp.is_pointer_button_down_on()) {
        *value = if vertical { (p.y - rect.top()) / rect.height() } else { (p.x - rect.left()) / rect.width() }.clamp(0.0, 1.0);
        changed = true;
    }
    let mut mesh = egui::Mesh::default();
    let n = 24;
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let c = color(t);
        let (a, b) = if vertical {
            let y = rect.top() + t * rect.height();
            (egui::pos2(rect.left(), y), egui::pos2(rect.right(), y))
        } else {
            let x = rect.left() + t * rect.width();
            (egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom()))
        };
        mesh.colored_vertex(a, c);
        mesh.colored_vertex(b, c);
        if i > 0 {
            let k = (i as u32) * 2;
            mesh.add_triangle(k - 2, k - 1, k);
            mesh.add_triangle(k - 1, k + 1, k);
        }
    }
    ui.painter().add(mesh);
    let stroke = egui::Stroke::new(1.5, ui.visuals().strong_text_color());
    if vertical {
        let y = rect.top() + *value * rect.height();
        ui.painter().line_segment([egui::pos2(rect.left() - 2.0, y), egui::pos2(rect.right() + 2.0, y)], stroke);
    } else {
        let x = rect.left() + *value * rect.width();
        ui.painter().line_segment([egui::pos2(x, rect.top() - 2.0), egui::pos2(x, rect.bottom() + 2.0)], stroke);
    }
    changed
}

/// Hue (0-1), whiteness and blackness to sRGB: the pure hue with white and
/// black mixed in.
fn hbw_rgb(hue: f32, white: f32, black: f32) -> [u8; 3] {
    let pure = hsv(hue, 1.0, 1.0);
    let k = (1.0 - white - black).max(0.0);
    pure.map(|p| ((p * k + white).clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// sRGB to hue, saturation and value, each 0-1.
fn to_hsv(c: [u8; 3]) -> (f32, f32, f32) {
    let [r, g, b] = c.map(|x| x as f32 / 255.0);
    let max = r.max(g).max(b);
    let d = max - r.min(g).min(b);
    let h = if d <= 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    (h, if max > 0.0 { d / max } else { 0.0 }, max)
}

fn from_hsv(h: f32, s: f32, v: f32) -> [u8; 3] {
    hsv(h, s, v).map(|x| (x.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// Hue, saturation and value (0-1) to RGB 0-1.
fn hsv(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h6 = (h.rem_euclid(1.0)) * 6.0;
    let c = v * s;
    let x = c * (1.0 - (h6 % 2.0 - 1.0).abs());
    let (r, g, b) = match h6 as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r + m, g + m, b + m]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_models_agree() {
        for c in [[255, 0, 0], [12, 200, 90], [128, 128, 128], [0, 0, 0], [255, 255, 255], [250, 30, 200]] {
            let (h, s, v) = to_hsv(c);
            assert_eq!(from_hsv(h, s, v), c, "HSV round trip of {c:?}");
            let f = c.map(|x| x as f32 / 255.0);
            let (w, b) = (f[0].min(f[1]).min(f[2]), 1.0 - f[0].max(f[1]).max(f[2]));
            assert_eq!(hbw_rgb(h, w, b), c, "HBW round trip of {c:?}");
        }
    }
}
