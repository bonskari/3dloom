//! Spinners. Every numeric field is a `DragValue` made here, so a
//! right-click sets it to 0, or to the end of its range nearest 0, as in the
//! reference ("set a numeric field to 0.0 or its lowest value"). Ctrl while
//! dragging changes the value faster, Alt slower, and with Preferences > Wrap
//! Cursor Near Spinner the pointer wraps to the opposite window edge, so a
//! drag never runs out of room.

use eframe::egui;
use egui::emath::Numeric;
use std::ops::{Bound, RangeBounds};

/// Ctrl-drag and Alt-drag rates (the reference names no factor).
pub const FAST: f64 = 10.0;
pub const SLOW: f64 = 0.1;

fn wrap_id() -> egui::Id {
    egui::Id::new("spinner_wrap_cursor")
}

/// Preferences > Spinners > Wrap Cursor Near Spinner; the app sets it each frame.
pub fn set_wrap_cursor(ctx: &egui::Context, on: bool) {
    ctx.data_mut(|d| d.insert_temp(wrap_id(), on));
}

/// A drag in progress: the last value the DragValue set (`raw`) and the
/// spinner's own value, which takes the DragValue's moves scaled by Ctrl/Alt.
#[derive(Clone, Copy)]
struct Track {
    raw: f64,
    value: f64,
}

/// A spinner for `value` limited to `range` (`..` for none). Add it right
/// away (`ui.add(spin(ui, ..))`): the right-click and the drag are read from
/// the widget id `ui` hands out next. The reset frame reports the response as changed.
pub fn spin<'a, N: Numeric>(ui: &egui::Ui, value: &'a mut N, range: impl RangeBounds<N>) -> egui::DragValue<'a> {
    let bound = |b: Bound<&N>, unbounded: N| match b {
        Bound::Included(v) | Bound::Excluded(v) => *v,
        Bound::Unbounded => unbounded,
    };
    // Unbounded floats stay infinite, as a plain DragValue's are.
    let (lo, hi) = if N::INTEGRAL { (N::MIN.to_f64(), N::MAX.to_f64()) } else { (f64::NEG_INFINITY, f64::INFINITY) };
    let lo = bound(range.start_bound(), N::from_f64(lo)).to_f64();
    let hi = bound(range.end_bound(), N::from_f64(hi)).to_f64();
    let id = ui.next_auto_id();
    let ctx = ui.ctx().clone();
    crate::cursor::note_spinner(&ctx, id);
    let reset = ui.is_enabled() && ctx.read_response(id).is_some_and(|r| r.secondary_clicked());
    let before = value.to_f64();
    let dragging = ctx.is_being_dragged(id);
    let track_id = id.with("spin_track");
    let factor = ui.input(|i| if i.modifiers.ctrl { FAST } else if i.modifiers.alt { SLOW } else { 1.0 });
    // After a wrap the pointer jumps across the window: that move is not a drag.
    let (screen, pointer, delta) = ui.input(|i| (i.screen_rect(), i.pointer.latest_pos(), i.pointer.delta()));
    let jumped = delta.x.abs() > screen.width() / 2.0 || delta.y.abs() > screen.height() / 2.0;
    if !dragging {
        ctx.data_mut(|d| d.remove::<Track>(track_id));
    } else if let Some(p) = pointer.filter(|_| ctx.data(|d| d.get_temp::<bool>(wrap_id())).unwrap_or(false)) {
        let (edge, inset) = (2.0, 12.0);
        let mut to = p;
        if p.y <= screen.min.y + edge {
            to.y = screen.max.y - inset;
        } else if p.y >= screen.max.y - edge {
            to.y = screen.min.y + inset;
        }
        if p.x <= screen.min.x + edge {
            to.x = screen.max.x - inset;
        } else if p.x >= screen.max.x - edge {
            to.x = screen.min.x + inset;
        }
        if to != p {
            ctx.send_viewport_cmd(egui::ViewportCommand::CursorPosition(to));
        }
    }
    if reset {
        *value = N::from_f64(0.0_f64.max(lo).min(hi));
    }
    // The first read reports the value from before the reset, so the
    // DragValue marks its response changed and the caller applies it.
    let mut first = reset;
    let dv = egui::DragValue::from_get_set(move |v| {
        if let Some(mut v) = v {
            if dragging {
                let t = ctx.data(|d| d.get_temp::<Track>(track_id)).unwrap_or(Track { raw: before, value: before });
                let moved = if jumped { 0.0 } else { (v - t.raw) * factor };
                let raw = v;
                v = (t.value + moved).clamp(lo, hi);
                ctx.data_mut(|d| d.insert_temp(track_id, Track { raw, value: v }));
            }
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

    const SCREEN: egui::Rect = egui::Rect { min: egui::Pos2::ZERO, max: egui::pos2(400.0, 200.0) };

    /// One frame of an unbounded float spinner (speed 1) with `mods` held;
    /// returns the spinner's rect and the frame's viewport commands.
    fn float_frame(ctx: &egui::Context, value: &mut f64, mods: egui::Modifiers, events: Vec<egui::Event>) -> (egui::Rect, Vec<egui::ViewportCommand>) {
        let input = egui::RawInput { screen_rect: Some(SCREEN), modifiers: mods, events, ..Default::default() };
        let mut rect = egui::Rect::NOTHING;
        let out = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                rect = ui.add(spin(ui, value, ..).speed(1.0)).rect;
            });
        });
        let cmds = out.viewport_output.get(&egui::ViewportId::ROOT).map(|v| v.commands.clone()).unwrap_or_default();
        (rect, cmds)
    }

    /// Drags the spinner `dx` points to the right in ten moves with `mods` held.
    fn drag_by(mods: egui::Modifiers, dx: f32) -> f64 {
        let ctx = egui::Context::default();
        let mut v = 0.0;
        let p = float_frame(&ctx, &mut v, mods, Vec::new()).0.center();
        float_frame(&ctx, &mut v, mods, vec![egui::Event::PointerMoved(p)]);
        float_frame(&ctx, &mut v, mods, vec![egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: mods }]);
        for i in 1..=10 {
            float_frame(&ctx, &mut v, mods, vec![egui::Event::PointerMoved(p + egui::vec2(dx * i as f32 / 10.0, 0.0))]);
        }
        float_frame(&ctx, &mut v, mods, vec![egui::Event::PointerButton { pos: p + egui::vec2(dx, 0.0), button: egui::PointerButton::Primary, pressed: false, modifiers: mods }]);
        v
    }

    #[test]
    fn ctrl_drag_is_faster_and_alt_drag_slower() {
        let plain = drag_by(egui::Modifiers::NONE, 40.0);
        let fast = drag_by(egui::Modifiers::CTRL, 40.0);
        let slow = drag_by(egui::Modifiers::ALT, 40.0);
        assert!(plain > 20.0, "a plain drag moves the value: {plain}");
        assert!((fast / plain - FAST).abs() < 0.5, "Ctrl: {fast} vs {plain}");
        assert!((slow / plain - SLOW).abs() < 0.05, "Alt: {slow} vs {plain}");
    }

    #[test]
    fn wrap_cursor_moves_the_pointer_to_the_other_edge_and_the_jump_is_not_a_drag() {
        let none = egui::Modifiers::NONE;
        for wrap in [false, true] {
            let ctx = egui::Context::default();
            set_wrap_cursor(&ctx, wrap);
            let mut v = 0.0;
            let p = float_frame(&ctx, &mut v, none, Vec::new()).0.center();
            float_frame(&ctx, &mut v, none, vec![egui::Event::PointerMoved(p)]);
            float_frame(&ctx, &mut v, none, vec![egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: none }]);
            let mut warps = Vec::new();
            for y in [p.y - 20.0, 1.0, 0.0] {
                set_wrap_cursor(&ctx, wrap);
                let (_, cmds) = float_frame(&ctx, &mut v, none, vec![egui::Event::PointerMoved(egui::pos2(p.x, y))]);
                warps.extend(cmds.into_iter().filter_map(|c| match c {
                    egui::ViewportCommand::CursorPosition(to) => Some(to),
                    _ => None,
                }));
            }
            if !wrap {
                assert!(warps.is_empty(), "no wrap without the preference: {warps:?}");
                continue;
            }
            let to = *warps.last().expect("the pointer wraps at the top edge");
            assert!(to.y > SCREEN.max.y - 20.0 && (to.x - p.x).abs() < 1.0, "to the bottom edge: {to:?}");
            let at_edge = v;
            set_wrap_cursor(&ctx, wrap);
            float_frame(&ctx, &mut v, none, vec![egui::Event::PointerMoved(to)]);
            assert_eq!(v, at_edge, "the jump across the window does not change the value");
            set_wrap_cursor(&ctx, wrap);
            float_frame(&ctx, &mut v, none, vec![egui::Event::PointerMoved(to - egui::vec2(0.0, 10.0))]);
            assert!(v > at_edge, "dragging on up keeps increasing it: {v} after {at_edge}");
        }
    }
}
