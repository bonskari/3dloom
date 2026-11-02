//! Spinners. Every numeric field is a `DragValue` made here, so a
//! right-click sets it to 0, or to the end of its range nearest 0, as in the
//! reference ("set a numeric field to 0.0 or its lowest value"). Ctrl while
//! dragging changes the value faster, Alt slower, and with Preferences > Wrap
//! Cursor Near Spinner the pointer wraps to the opposite window edge, so a
//! drag never runs out of room. A typed value may be an expression
//! (`2*pi`, `(10+5)/3`), and `R<n>` offsets the current value by n. Up / down
//! arrows beside the field step the value (by Spinner Snap when it is on),
//! repeat while held and change it when dragged.

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

/// Ticks per second (the TPS constant of a typed expression).
pub const TPS: f64 = 4800.0;

/// A typed spinner value: an arithmetic expression (+ - * / ^, parentheses,
/// pi, e, TPS), or `R<expr>` for `current` + expr. `number` reads one
/// literal (a plain number, or a length with its unit).
pub fn typed(text: &str, current: f64, number: &dyn Fn(&str) -> Option<f64>) -> Option<f64> {
    let t = text.trim();
    if let Some(rest) = t.strip_prefix(['r', 'R']).filter(|r| !r.trim().is_empty()) {
        return Some(current + typed(rest, 0.0, number)?);
    }
    if let Some(v) = number(t) {
        return Some(v);
    }
    let mut p = Expr { s: t.as_bytes(), i: 0, number };
    let v = p.sum()?;
    p.skip();
    (p.i == p.s.len() && v.is_finite()).then_some(v)
}

struct Expr<'a> {
    s: &'a [u8],
    i: usize,
    number: &'a dyn Fn(&str) -> Option<f64>,
}

impl Expr<'_> {
    fn skip(&mut self) {
        while self.s.get(self.i).is_some_and(|c| c.is_ascii_whitespace()) {
            self.i += 1;
        }
    }
    fn eat(&mut self, c: u8) -> bool {
        self.skip();
        let hit = self.s.get(self.i) == Some(&c);
        self.i += hit as usize;
        hit
    }
    fn sum(&mut self) -> Option<f64> {
        let mut v = self.product()?;
        loop {
            if self.eat(b'+') {
                v += self.product()?;
            } else if self.eat(b'-') {
                v -= self.product()?;
            } else {
                return Some(v);
            }
        }
    }
    fn product(&mut self) -> Option<f64> {
        let mut v = self.power()?;
        loop {
            if self.eat(b'*') {
                v *= self.power()?;
            } else if self.eat(b'/') {
                v /= self.power()?;
            } else {
                return Some(v);
            }
        }
    }
    fn power(&mut self) -> Option<f64> {
        let base = self.unary()?;
        if self.eat(b'^') {
            return Some(base.powf(self.power()?));
        }
        Some(base)
    }
    fn unary(&mut self) -> Option<f64> {
        if self.eat(b'-') {
            return Some(-self.unary()?);
        }
        if self.eat(b'+') {
            return self.unary();
        }
        if self.eat(b'(') {
            let v = self.sum()?;
            return self.eat(b')').then_some(v);
        }
        self.skip();
        let start = self.i;
        while self.s.get(self.i).is_some_and(|c| !b"+-*/^() ".contains(c) || (b"+-".contains(c) && self.i > start && matches!(self.s[self.i - 1], b'e' | b'E') && self.s[start].is_ascii_digit())) {
            self.i += 1;
        }
        let word = std::str::from_utf8(&self.s[start..self.i]).ok()?;
        match word.to_ascii_lowercase().as_str() {
            "" => None,
            "pi" => Some(std::f64::consts::PI),
            "e" => Some(std::f64::consts::E),
            "tps" => Some(TPS),
            _ => (self.number)(word),
        }
    }
}

/// A drag in progress: the last value the DragValue set (`raw`) and the
/// spinner's own value, which takes the DragValue's moves scaled by Ctrl/Alt.
#[derive(Clone, Copy)]
struct Track {
    raw: f64,
    value: f64,
}

/// The arrows' width beside the field, in points.
pub const ARROW_WIDTH: f32 = 10.0;
/// An arrow held this long (seconds) repeats its step every [`REPEAT`] seconds.
pub const HOLD: f64 = 0.4;
pub const REPEAT: f64 = 0.05;
/// Pointer travel (points) that turns an arrow press into a drag.
const DRAG_START: f32 = 3.0;

fn snap_id() -> egui::Id {
    egui::Id::new("spinner_snap")
}

/// Spinner Snap: Preferences > Spinners > Snap while Use Snap is on (None
/// off): what an arrow click adds or takes away. The app sets it each frame.
pub fn set_snap(ctx: &egui::Context, snap: Option<f64>) {
    ctx.data_mut(|d| d.insert_temp(snap_id(), snap));
}

/// An arrow press in progress: which arrow, when it began and last stepped,
/// the pointer's height at the press and last frame, and whether it has
/// become a drag.
#[derive(Clone, Copy)]
struct Press {
    up: bool,
    at: f64,
    last: f64,
    y0: f32,
    y: f32,
    dragging: bool,
}

type Formatter<'a> = Box<dyn 'a + Fn(f64, std::ops::RangeInclusive<usize>) -> String>;
type Parser<'a> = Box<dyn 'a + Fn(&str) -> Option<f64>>;

/// A spinner: the numeric field and its up / down arrows. Clicking an arrow
/// steps the value (by the Spinner Snap value when it is on, else by the
/// spinner's speed; integers by at least 1), holding it repeats, dragging on
/// it changes the value with the pointer's height (Ctrl faster, Alt slower),
/// and a right-click on the field or the arrows sets 0 or the end of the
/// range nearest 0 (Shift+ and Ctrl+right-click are left to the caller).
pub struct Spin<'a, N: Numeric> {
    value: &'a mut N,
    lo: f64,
    hi: f64,
    speed: Option<f64>,
    min_decimals: Option<usize>,
    max_decimals: Option<usize>,
    prefix: String,
    suffix: String,
    formatter: Option<Formatter<'a>>,
    parser: Option<Parser<'a>>,
}

/// A spinner for `value` limited to `range` (`..` for none). The reset and
/// arrow frames report the response as changed.
pub fn spin<'a, N: Numeric>(value: &'a mut N, range: impl RangeBounds<N>) -> Spin<'a, N> {
    let bound = |b: Bound<&N>, unbounded: N| match b {
        Bound::Included(v) | Bound::Excluded(v) => *v,
        Bound::Unbounded => unbounded,
    };
    // Unbounded floats stay infinite, as a plain DragValue's are.
    let (lo, hi) = if N::INTEGRAL { (N::MIN.to_f64(), N::MAX.to_f64()) } else { (f64::NEG_INFINITY, f64::INFINITY) };
    let lo = bound(range.start_bound(), N::from_f64(lo)).to_f64();
    let hi = bound(range.end_bound(), N::from_f64(hi)).to_f64();
    Spin { value, lo, hi, speed: None, min_decimals: None, max_decimals: None, prefix: String::new(), suffix: String::new(), formatter: None, parser: None }
}

impl<'a, N: Numeric> Spin<'a, N> {
    /// Value change per point dragged (and per arrow click without Spinner Snap).
    pub fn speed(mut self, speed: impl Into<f64>) -> Self {
        self.speed = Some(speed.into());
        self
    }
    pub fn fixed_decimals(mut self, n: usize) -> Self {
        self.min_decimals = Some(n);
        self.max_decimals = Some(n);
        self
    }
    pub fn max_decimals(mut self, n: usize) -> Self {
        self.max_decimals = Some(n);
        self
    }
    pub fn min_decimals(mut self, n: usize) -> Self {
        self.min_decimals = Some(n);
        self
    }
    pub fn prefix(mut self, prefix: impl ToString) -> Self {
        self.prefix = prefix.to_string();
        self
    }
    pub fn suffix(mut self, suffix: impl ToString) -> Self {
        self.suffix = suffix.to_string();
        self
    }
    /// Limits the value to `range` (replaces the range given to [`spin`]).
    pub fn range<M: Numeric>(mut self, range: std::ops::RangeInclusive<M>) -> Self {
        self.lo = range.start().to_f64();
        self.hi = range.end().to_f64();
        self
    }
    pub fn custom_formatter(mut self, f: impl 'a + Fn(f64, std::ops::RangeInclusive<usize>) -> String) -> Self {
        self.formatter = Some(Box::new(f));
        self
    }
    pub fn custom_parser(mut self, f: impl 'a + Fn(&str) -> Option<f64>) -> Self {
        self.parser = Some(Box::new(f));
        self
    }
}

impl<N: Numeric> egui::Widget for Spin<'_, N> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        // The arrows sit right of the field in right-to-left rows too.
        let rtl = ui.layout().prefer_right_to_left();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            // The arrows come out of the field's minimum width, so rows keep their width.
            let min = ui.spacing().interact_size.x;
            ui.spacing_mut().interact_size.x = (min - ARROW_WIDTH).max(ARROW_WIDTH);
            let height = ui.spacing().interact_size.y;
            let arrows = rtl.then(|| ui.allocate_exact_size(egui::vec2(ARROW_WIDTH, height), egui::Sense::hover()).0);
            let id = ui.next_auto_id();
            let (lo, hi) = (self.lo, self.hi);
            let speed = self.speed.unwrap_or(if N::INTEGRAL { 0.25 } else { 1.0 });
            let value = self.value;
            let mut r = field(ui, id, &mut *value, lo, hi, speed, self.min_decimals, self.max_decimals, self.prefix, self.suffix, self.formatter, self.parser);
            let rect = arrows.unwrap_or_else(|| ui.allocate_exact_size(egui::vec2(ARROW_WIDTH, r.rect.height()), egui::Sense::hover()).0);
            let a = ui.interact(rect, id.with("arrows"), egui::Sense::click_and_drag());
            crate::cursor::note_spinner(ui.ctx(), a.id);
            let step = ui.ctx().data(|d| d.get_temp::<Option<f64>>(snap_id())).flatten().unwrap_or(speed);
            let step = if N::INTEGRAL { step.round().max(1.0) } else { step };
            let before = value.to_f64();
            let mut v = arrow_input(ui, &a, id.with("arrow_press"), before, step, speed);
            if ui.is_enabled() && a.secondary_clicked() && !ui.input(|i| i.modifiers.shift || i.modifiers.ctrl) {
                v = 0.0;
            }
            let v = v.clamp(lo, hi);
            let v = if N::INTEGRAL { v.round() } else { v };
            if v != before {
                *value = N::from_f64(v);
                r.mark_changed();
            }
            paint_arrows(ui, &a, ui.ctx().data(|d| d.get_temp::<Press>(id.with("arrow_press"))).map(|p| p.up));
            r | a
        })
        .inner
    }
}

/// The arrows' press, hold and drag: the value after this frame's input.
fn arrow_input(ui: &egui::Ui, a: &egui::Response, press_id: egui::Id, value: f64, step: f64, speed: f64) -> f64 {
    let ctx = ui.ctx();
    let (now, pointer, pressed, down) = ui.input(|i| (i.time, i.pointer.interact_pos(), i.pointer.primary_pressed(), i.pointer.primary_down()));
    let mut press = ctx.data(|d| d.get_temp::<Press>(press_id));
    let mut v = value;
    if pressed && ui.is_enabled() && a.hovered() {
        if let Some(p) = pointer.filter(|p| a.rect.contains(*p)) {
            let up = p.y < a.rect.center().y;
            v += if up { step } else { -step };
            press = Some(Press { up, at: now, last: now, y0: p.y, y: p.y, dragging: false });
        }
    } else if let (Some(mut p), true) = (press, down) {
        let y = pointer.map_or(p.y, |q| q.y);
        if !p.dragging && (y - p.y0).abs() > DRAG_START {
            p.dragging = true;
        }
        if p.dragging {
            // Up increases, as on the field; Ctrl faster, Alt slower.
            let factor = ui.input(|i| if i.modifiers.ctrl { FAST } else if i.modifiers.alt { SLOW } else { 1.0 });
            v -= (y - p.y) as f64 * speed * factor;
        } else if now - p.at >= HOLD && now - p.last >= REPEAT {
            v += if p.up { step } else { -step };
            p.last = now;
        }
        p.y = y;
        press = Some(p);
        ctx.request_repaint();
    } else {
        press = None;
    }
    ctx.data_mut(|d| match press {
        Some(p) => d.insert_temp(press_id, p),
        None => d.remove::<Press>(press_id),
    });
    v
}

/// The two arrow buttons; `pressed` the one held down.
fn paint_arrows(ui: &egui::Ui, a: &egui::Response, pressed: Option<bool>) {
    if !ui.is_rect_visible(a.rect) {
        return;
    }
    let r = a.rect;
    let halves = [(true, egui::Rect::from_min_max(r.min, egui::pos2(r.max.x, r.center().y))), (false, egui::Rect::from_min_max(egui::pos2(r.min.x, r.center().y), r.max))];
    let hover = ui.input(|i| i.pointer.hover_pos());
    for (up, h) in halves {
        let visuals = if pressed == Some(up) {
            &ui.visuals().widgets.active
        } else if a.hovered() && hover.is_some_and(|p| h.contains(p)) {
            &ui.visuals().widgets.hovered
        } else {
            ui.style().interact(a)
        };
        let painter = ui.painter();
        painter.rect_filled(h.shrink(0.5), 1.0, visuals.weak_bg_fill);
        let c = h.center();
        let (w, t) = (2.5, 1.5);
        let tip = if up { -t } else { t };
        painter.add(egui::Shape::convex_polygon(vec![egui::pos2(c.x - w, c.y - tip), egui::pos2(c.x + w, c.y - tip), egui::pos2(c.x, c.y + tip)], visuals.fg_stroke.color, egui::Stroke::NONE));
    }
}

/// The numeric field: a DragValue (id `id`) whose right-click resets and
/// whose drag Ctrl / Alt scale, with the wrap-cursor behaviour.
#[allow(clippy::too_many_arguments)]
fn field<N: Numeric>(
    ui: &mut egui::Ui,
    id: egui::Id,
    value: &mut N,
    lo: f64,
    hi: f64,
    speed: f64,
    min_decimals: Option<usize>,
    max_decimals: Option<usize>,
    prefix: String,
    suffix: String,
    formatter: Option<Formatter<'_>>,
    parser: Option<Parser<'_>>,
) -> egui::Response {
    let ctx = ui.ctx().clone();
    crate::cursor::note_spinner(&ctx, id);
    // Shift+right-click keys the parameter, Ctrl+right-click is the
    // caller's too (a weight cell's 1): no reset.
    let reset = ui.is_enabled() && ctx.read_response(id).is_some_and(|r| r.secondary_clicked()) && !ui.input(|i| i.modifiers.shift || i.modifiers.ctrl);
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
    let ctx2 = ctx.clone();
    let mut dv = egui::DragValue::from_get_set(move |v| {
        if let Some(mut v) = v {
            if dragging {
                let t = ctx2.data(|d| d.get_temp::<Track>(track_id)).unwrap_or(Track { raw: before, value: before });
                let moved = if jumped { 0.0 } else { (v - t.raw) * factor };
                let raw = v;
                v = (t.value + moved).clamp(lo, hi);
                ctx2.data_mut(|d| d.insert_temp(track_id, Track { raw, value: v }));
            }
            *value = N::from_f64(v);
        }
        if std::mem::take(&mut first) {
            return before;
        }
        value.to_f64()
    })
    .range(lo..=hi)
    .speed(speed)
    .prefix(prefix)
    .suffix(suffix);
    dv = match parser {
        Some(p) => dv.custom_parser(p),
        None => dv.custom_parser(move |t| typed(t, before, &|s| s.parse().ok())),
    };
    if let Some(f) = formatter {
        dv = dv.custom_formatter(f);
    }
    if N::INTEGRAL {
        dv = dv.max_decimals(0);
    }
    if let Some(n) = min_decimals {
        dv = dv.min_decimals(n);
    }
    if let Some(n) = max_decimals {
        dv = dv.max_decimals(n);
    }
    ui.add(dv)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_values_are_expressions_and_r_offsets_the_current_value() {
        let plain = |s: &str| s.parse::<f64>().ok();
        assert_eq!(typed("12.5", 3.0, &plain), Some(12.5));
        assert_eq!(typed("(10+5)/3", 0.0, &plain), Some(5.0));
        assert_eq!(typed("2^3*-1", 0.0, &plain), Some(-8.0));
        assert!((typed("2*pi", 0.0, &plain).unwrap() - std::f64::consts::TAU).abs() < 1e-12);
        assert_eq!(typed("TPS/160", 0.0, &plain), Some(30.0));
        assert_eq!(typed("1e-2+1", 0.0, &plain), Some(1.01));
        assert_eq!(typed("R10", 5.0, &plain), Some(15.0));
        assert_eq!(typed("r-2*3", 5.0, &plain), Some(-1.0));
        assert_eq!(typed("2+", 0.0, &plain), None);
        assert_eq!(typed("1/0", 0.0, &plain), None);
        // A unit-aware literal reader (lengths): 2.5cm + 1 in scene units.
        let cm = |s: &str| s.strip_suffix("cm").and_then(|n| n.parse::<f64>().ok()).or_else(|| s.parse().ok());
        assert_eq!(typed("2.5cm+1", 0.0, &cm), Some(3.5));
    }

    /// Runs one frame with `events`; returns whether the spinner reported a change and its rect.
    fn frame(ctx: &egui::Context, value: &mut u32, events: Vec<egui::Event>) -> (bool, egui::Rect) {
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 200.0))), events, ..Default::default() };
        let mut out = (false, egui::Rect::NOTHING);
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let r = ui.add(spin(value, 2..=32));
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
                rect = ui.add(spin(value, ..).speed(1.0)).rect;
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
