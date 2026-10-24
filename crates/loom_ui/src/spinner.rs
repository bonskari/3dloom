//! Spinners. Every numeric field is a `DragValue` made here, so a
//! right-click sets it to 0, or to the end of its range nearest 0, as in the
//! reference ("set a numeric field to 0.0 or its lowest value").

use eframe::egui;
use egui::emath::Numeric;
use std::ops::{Bound, RangeBounds};

/// A spinner for `value` limited to `range` (`..` for none). Add it right
/// away (`ui.add(spin(ui, ..))`): the right-click is read from the widget id
/// `ui` hands out next. The reset frame reports the response as changed.
pub fn spin<'a, N: Numeric>(ui: &egui::Ui, value: &'a mut N, range: impl RangeBounds<N>) -> egui::DragValue<'a> {
    let bound = |b: Bound<&N>, unbounded: N| match b {
        Bound::Included(v) | Bound::Excluded(v) => *v,
        Bound::Unbounded => unbounded,
    };
    // Unbounded floats stay infinite, as a plain DragValue's are.
    let (lo, hi) = if N::INTEGRAL { (N::MIN.to_f64(), N::MAX.to_f64()) } else { (f64::NEG_INFINITY, f64::INFINITY) };
    let lo = bound(range.start_bound(), N::from_f64(lo)).to_f64();
    let hi = bound(range.end_bound(), N::from_f64(hi)).to_f64();
    let reset = ui.is_enabled() && ui.ctx().read_response(ui.next_auto_id()).is_some_and(|r| r.secondary_clicked());
    let before = value.to_f64();
    if reset {
        *value = N::from_f64(0.0_f64.max(lo).min(hi));
    }
    // The first read reports the value from before the reset, so the
    // DragValue marks its response changed and the caller applies it.
    let mut first = reset;
    let dv = egui::DragValue::from_get_set(move |v| {
        if let Some(v) = v {
            *value = N::from_f64(v);
        }
        if std::mem::take(&mut first) {
            return before;
        }
        value.to_f64()
    })
    .range(lo..=hi);
    if N::INTEGRAL {
        dv.max_decimals(0).speed(0.25)
    } else {
        dv
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs one frame with `events`; returns whether the spinner reported a change and its rect.
    fn frame(ctx: &egui::Context, value: &mut u32, events: Vec<egui::Event>) -> (bool, egui::Rect) {
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 200.0))), events, ..Default::default() };
        let mut out = (false, egui::Rect::NOTHING);
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let r = ui.add(spin(ui, value, 2..=32));
                out = (r.changed(), r.rect);
            });
        });
        out
    }

    #[test]
    fn right_click_sets_the_minimum_and_reports_a_change() {
        let ctx = egui::Context::default();
        let mut v = 10_u32;
        let (_, rect) = frame(&ctx, &mut v, Vec::new());
        let p = rect.center();
        frame(&ctx, &mut v, vec![egui::Event::PointerMoved(p)]);
        let press = |pressed| egui::Event::PointerButton { pos: p, button: egui::PointerButton::Secondary, pressed, modifiers: egui::Modifiers::NONE };
        frame(&ctx, &mut v, vec![press(true)]);
        let mut changed = frame(&ctx, &mut v, vec![press(false)]).0;
        changed |= frame(&ctx, &mut v, Vec::new()).0;
        assert_eq!(v, 2, "0 is below the range: the minimum");
        assert!(changed, "the caller sees the change");
        assert!(!frame(&ctx, &mut v, Vec::new()).0, "and only once");
    }
}
