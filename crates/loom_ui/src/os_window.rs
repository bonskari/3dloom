//! Floating windows that the reference shows as real windows of their own
//! (the Material Editor, Track View, Rendered Frame Window, Render Setup, ...):
//! with Preferences > Floating windows as separate OS windows on, each opens
//! as a native window (an egui immediate viewport) that can be moved outside
//! the main window; where the backend can't (the headless tests, a backend
//! without multiple viewports) or with the preference off, it is an in-app
//! `egui::Window` as before. Where each OS window was left is kept in the
//! settings file.

use eframe::egui;
use std::collections::BTreeMap;

/// By window, its outer position and inner size: `[x, y, w, h]`.
pub type Rects = BTreeMap<String, [f32; 4]>;

fn enabled_id() -> egui::Id {
    egui::Id::new("os_windows_enabled")
}

fn rects_id() -> egui::Id {
    egui::Id::new("os_window_rects")
}

/// Before the floating windows of a frame: the preference and the saved places.
pub fn begin(ctx: &egui::Context, enabled: bool, rects: &Rects) {
    ctx.data_mut(|d| {
        d.insert_temp(enabled_id(), enabled);
        d.insert_temp(rects_id(), rects.clone());
    });
}

/// After them: where the OS windows are now.
pub fn rects(ctx: &egui::Context) -> Rects {
    ctx.data(|d| d.get_temp(rects_id())).unwrap_or_default()
}

/// Notes where the window `key` is (the viewport does this each frame).
pub fn record(ctx: &egui::Context, key: &str, rect: [f32; 4]) {
    ctx.data_mut(|d| {
        let mut all: Rects = d.get_temp(rects_id()).unwrap_or_default();
        if all.get(key) != Some(&rect) {
            all.insert(key.to_owned(), rect);
            d.insert_temp(rects_id(), all);
        }
    });
}

/// A point `p` of the window being drawn, in the main window's coordinates
/// (the same point when it is an in-app window).
pub fn to_main(ctx: &egui::Context, p: egui::Pos2) -> egui::Pos2 {
    if ctx.viewport_id() == egui::ViewportId::ROOT {
        return p;
    }
    let own = ctx.input(|i| i.viewport().inner_rect);
    let main = ctx.input_for(egui::ViewportId::ROOT, |i| i.viewport().inner_rect);
    match (own, main) {
        (Some(own), Some(main)) => p + (own.min - main.min),
        _ => p,
    }
}

/// What a floating window showed: its rect (in its own window) and the contents' result.
pub struct Shown<R> {
    pub rect: egui::Rect,
    pub inner: Option<R>,
}

pub struct OsWindow {
    key: String,
    title: String,
    size: egui::Vec2,
    resizable: bool,
    scroll: bool,
}

impl OsWindow {
    /// The window `key` (its settings name) titled `title`, first opening at `size`.
    pub fn new(key: impl Into<String>, title: impl Into<String>, size: impl Into<egui::Vec2>) -> Self {
        Self { key: key.into(), title: title.into(), size: size.into(), resizable: true, scroll: false }
    }

    /// Not resizable: the OS window grows to fit its contents.
    pub fn fixed(mut self) -> Self {
        self.resizable = false;
        self
    }

    /// The contents scroll vertically in the OS window.
    pub fn scroll(mut self) -> Self {
        self.scroll = true;
        self
    }

    /// Shows the contents `add` in an OS window, or in `window` (an in-app
    /// window built without `.open`) where that isn't possible. The close
    /// button clears `open`.
    pub fn show<R>(self, ctx: &egui::Context, open: &mut bool, window: egui::Window<'_>, add: impl FnOnce(&mut egui::Ui) -> R) -> Option<Shown<R>> {
        let enabled = ctx.data(|d| d.get_temp(enabled_id())).unwrap_or(false);
        if !enabled || !*open {
            return window.open(open).show(ctx, add).map(|r| Shown { rect: r.response.rect, inner: r.inner });
        }
        // Opening (not shown last frame): where it was left, else its default size.
        let pass = ctx.cumulative_pass_nr();
        let start_id = egui::Id::new("os_window_start").with(&self.key);
        let saved = rects(ctx).get(&self.key).copied();
        let start = match ctx.data(|d| d.get_temp::<(Option<[f32; 4]>, u64)>(start_id)) {
            Some((start, last)) if last + 1 >= pass => start,
            _ => saved,
        };
        ctx.data_mut(|d| d.insert_temp(start_id, (start, pass)));
        let mut builder = egui::ViewportBuilder::default().with_title(self.title.clone()).with_resizable(self.resizable).with_inner_size(start.map_or(self.size, |r| egui::vec2(r[2], r[3])));
        if let Some(r) = start {
            builder = builder.with_position([r[0], r[1]]);
        }
        let id = egui::ViewportId::from_hash_of(("os_window", &self.key));
        let mut window = Some(window);
        let mut add = Some(add);
        ctx.show_viewport_immediate(id, builder, |ctx, class| {
            let add = add.take().expect("drawn once");
            if class == egui::ViewportClass::Embedded {
                return window.take().expect("drawn once").open(open).show(ctx, add).map(|r| Shown { rect: r.response.rect, inner: r.inner });
            }
            if ctx.input(|i| i.viewport().close_requested()) {
                *open = false;
            }
            let r = egui::CentralPanel::default().show(ctx, |ui| {
                let inner = if self.scroll { egui::ScrollArea::vertical().auto_shrink(false).show(ui, add).inner } else { add(ui) };
                (inner, ui.min_rect())
            });
            let (inner, used) = r.inner;
            let (outer, now) = ctx.input(|i| (i.viewport().outer_rect, i.viewport().inner_rect));
            if !self.resizable {
                // Fit the contents (plus the panel's margin).
                let want = (used.max.to_vec2() + egui::vec2(8.0, 8.0)).round();
                if now.is_some_and(|n| (n.width() - want.x).abs() > 1.0 || (n.height() - want.y).abs() > 1.0) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(want));
                }
            }
            if let (Some(o), Some(n)) = (outer, now) {
                record(ctx, &self.key, [o.min.x, o.min.y, n.width(), n.height()]);
            }
            Some(Shown { rect: r.response.rect, inner: Some(inner) })
        })
    }
}
