//! Command panel rollouts as in the reference: drag a rollout's title bar to
//! move it up or down the panel (the order is kept per panel in the user's
//! settings); right-click a title bar or the panel's empty area for the
//! rollout list (a click scrolls to that rollout and opens it), Close All,
//! Open All and Reset Rollout Order. Dragging the panel's empty area pans it.
//!
//! Rollouts draw in the code's order; with a custom order each one is
//! placed in its slot (from the heights measured the frame before), which
//! needs the panel's rollouts to follow one another with nothing between.

use eframe::egui;
use std::cell::RefCell;

/// One rollout as drawn: title, header id, whole height.
#[derive(Debug, Clone)]
struct Drawn {
    title: String,
    header: egui::Id,
    height: f32,
    /// The title bar's rect on screen (where drops land, the menu scrolls to).
    header_rect: egui::Rect,
}

/// What the panel remembers between frames (egui memory, per panel).
#[derive(Debug, Clone, Default)]
struct Memory {
    /// Last frame's rollouts in the code's order.
    natural: Vec<Drawn>,
    /// Last frame's rollouts followed one another (custom order possible).
    contiguous: bool,
}

/// A request from a rollout menu or a drag, run by the panel (which can dispatch).
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    Order(Vec<String>),
    Reset,
}

/// The frame being drawn: set while the command panel draws its tab.
struct Frame {
    panel: String,
    /// The custom order, if the user set one.
    order: Option<Vec<String>>,
    memory: Memory,
    drawn: Vec<Drawn>,
    /// Where the rollout run starts, its left edge and width (custom order only).
    top: Option<(f32, f32, f32)>,
    /// Where the next rollout should begin if the run is unbroken.
    expect_y: Option<f32>,
    contiguous: bool,
    /// Placement fell back to the code's order this frame.
    fallback: bool,
    request: Option<Request>,
    scroll_to: Option<String>,
}

thread_local! {
    static FRAME: RefCell<Option<Frame>> = const { RefCell::new(None) };
}

fn memory_id(panel: &str) -> egui::Id {
    egui::Id::new(("rollout_memory", panel.to_owned()))
}

fn scroll_id() -> egui::Id {
    egui::Id::new("rollout_scroll_to")
}

/// Starts the command panel tab `panel`'s rollouts (`order`: the user's order).
pub fn begin(ctx: &egui::Context, panel: &str, order: Option<Vec<String>>) {
    let memory: Memory = ctx.data(|d| d.get_temp(memory_id(panel))).unwrap_or_default();
    let scroll_to = ctx.data_mut(|d| d.remove_temp::<String>(scroll_id()));
    FRAME.with(|f| {
        *f.borrow_mut() = Some(Frame { panel: panel.to_owned(), order, memory, drawn: Vec::new(), top: None, expect_y: None, contiguous: true, fallback: false, request: None, scroll_to });
    });
}

/// Ends the tab's rollouts: remembers this frame's and returns a request to run.
pub fn end(ctx: &egui::Context) -> Option<Request> {
    let frame = FRAME.with(|f| f.borrow_mut().take())?;
    // Placed rollouts are listed in the order they were drawn: the code's order.
    let changed = frame.memory.natural.iter().map(|d| (&d.title, d.height.round() as i32)).ne(frame.drawn.iter().map(|d| (&d.title, d.height.round() as i32)));
    if changed || frame.fallback {
        ctx.request_repaint();
    }
    let memory = Memory { natural: frame.drawn, contiguous: frame.contiguous };
    ctx.data_mut(|d| d.insert_temp(memory_id(&frame.panel), memory));
    frame.request
}

/// The order rollouts show in: the user's order for the titles it names,
/// then the others in the code's order.
fn shown_order(natural: &[String], order: &[String]) -> Vec<String> {
    let rank = |t: &String| order.iter().position(|o| o == t).unwrap_or(order.len());
    let mut v: Vec<(usize, usize, String)> = natural.iter().enumerate().map(|(i, t)| (rank(t), i, t.clone())).collect();
    v.sort();
    v.into_iter().map(|(_, _, t)| t).collect()
}

/// A rollout: a collapsible section with a title bar. In the command panel
/// its title bar drags it to another place and right-clicks open the rollout menu.
pub fn rollout(ui: &mut egui::Ui, title: &str, default_open: bool, body: impl FnOnce(&mut egui::Ui)) {
    let in_panel = FRAME.with(|f| f.borrow().is_some());
    if !in_panel {
        egui::CollapsingHeader::new(egui::RichText::new(title).strong()).default_open(default_open).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            body(ui);
        });
        return;
    }
    let salt = ("rollout", title.to_owned());
    // Where this rollout goes: its slot in the user's order, or here.
    let placement = FRAME.with(|f| {
        let mut f = f.borrow_mut();
        let f = f.as_mut().expect("in the panel");
        let k = f.drawn.len();
        let order = f.order.clone()?;
        if f.fallback || !f.memory.contiguous || f.memory.natural.get(k).is_none_or(|d| d.title != title) {
            f.fallback = true;
            return None;
        }
        let spacing = ui.spacing().item_spacing.y;
        let cursor = ui.cursor().min;
        if let Some(y) = f.expect_y {
            if (cursor.y - y).abs() > 0.5 {
                // Something sits between rollouts: they keep the code's order.
                f.contiguous = false;
                f.fallback = true;
                return None;
            }
        }
        let (top, left, width) = *f.top.get_or_insert((cursor.y, cursor.x, ui.available_width()));
        let natural: Vec<String> = f.memory.natural.iter().map(|d| d.title.clone()).collect();
        let shown = shown_order(&natural, &order);
        let height = |t: &str| f.memory.natural.iter().find(|d| d.title == t).map(|d| d.height).unwrap_or(0.0);
        let at = shown.iter().position(|t| t == title).unwrap_or(k);
        let y = top + shown[..at].iter().map(|t| height(t) + spacing).sum::<f32>();
        let slot = height(&shown[k]);
        Some((egui::pos2(left, y), width, slot))
    });
    let draw = |ui: &mut egui::Ui| {
        let r = egui::CollapsingHeader::new(egui::RichText::new(title).strong()).id_salt(title).default_open(default_open).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            body(ui);
        });
        (r.header_response.id, r.header_response)
    };
    let (header_id, header, height) = match placement {
        Some((at, width, slot)) => {
            let mut child = ui.new_child(egui::UiBuilder::new().id_salt(salt).max_rect(egui::Rect::from_min_size(at, egui::vec2(width, f32::INFINITY))).layout(egui::Layout::top_down(egui::Align::Min)));
            let (id, header) = draw(&mut child);
            let h = child.min_rect().height();
            ui.allocate_space(egui::vec2(width, slot));
            FRAME.with(|f| {
                if let Some(f) = f.borrow_mut().as_mut() {
                    f.expect_y = Some(ui.cursor().min.y);
                }
            });
            (id, header, h)
        }
        None => {
            let r = ui.scope_builder(egui::UiBuilder::new().id_salt(salt), draw);
            (r.inner.0, r.inner.1, r.response.rect.height())
        }
    };
    menu_and_drag(ui, title, &header);
    FRAME.with(|f| {
        if let Some(f) = f.borrow_mut().as_mut() {
            if f.scroll_to.as_deref() == Some(title) {
                egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), header_id, default_open).tap_open(true, ui.ctx());
                ui.scroll_to_rect(header.rect, Some(egui::Align::Min));
                f.scroll_to = None;
            }
            f.drawn.push(Drawn { title: title.to_owned(), header: header_id, height, header_rect: header.rect });
        }
    });
}

trait TapOpen {
    fn tap_open(self, open: bool, ctx: &egui::Context);
}

impl TapOpen for egui::collapsing_header::CollapsingState {
    fn tap_open(mut self, open: bool, ctx: &egui::Context) {
        self.set_open(open);
        self.store(ctx);
    }
}

/// The title bar: dragging moves the rollout, right-click opens the rollout menu.
fn menu_and_drag(ui: &mut egui::Ui, title: &str, header: &egui::Response) {
    let r = header.interact(egui::Sense::drag());
    if r.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        if let Some(p) = ui.ctx().pointer_interact_pos() {
            let x = r.rect.x_range();
            ui.painter().with_clip_rect(ui.clip_rect()).hline(x, p.y, egui::Stroke::new(2.0, ui.visuals().selection.stroke.color));
        }
    }
    if r.drag_stopped() {
        if let Some(p) = ui.ctx().pointer_interact_pos() {
            FRAME.with(|f| {
                if let Some(f) = f.borrow_mut().as_mut() {
                    // The rollouts on screen in their shown order (last frame's places).
                    let mut shown: Vec<(String, f32)> = f.memory.natural.iter().map(|d| (d.title.clone(), d.header_rect.center().y)).collect();
                    shown.sort_by(|a, b| a.1.total_cmp(&b.1));
                    let mut titles: Vec<String> = shown.into_iter().map(|(t, _)| t).collect();
                    let centers: Vec<f32> = {
                        let mut c: Vec<f32> = f.memory.natural.iter().map(|d| d.header_rect.center().y).collect();
                        c.sort_by(f32::total_cmp);
                        c
                    };
                    if let Some(from) = titles.iter().position(|t| t == title) {
                        let to = centers.iter().filter(|c| **c < p.y).count();
                        let t = titles.remove(from);
                        let to = if to > from { to - 1 } else { to };
                        titles.insert(to.min(titles.len()), t);
                        f.request = Some(Request::Order(titles));
                    }
                }
            });
        }
    }
    header.context_menu(|ui| menu(ui));
}

/// The rollout menu: every rollout (a click scrolls to it and opens it),
/// Close All, Open All, Reset Rollout Order.
pub fn menu(ui: &mut egui::Ui) {
    ui.set_min_width(170.0);
    let items: Vec<(String, egui::Id)> = FRAME.with(|f| f.borrow().as_ref().map(|f| f.memory.natural.iter().map(|d| (d.title.clone(), d.header)).collect()).unwrap_or_default());
    for (t, _) in &items {
        if ui.button(t).clicked() {
            ui.ctx().data_mut(|d| d.insert_temp(scroll_id(), t.clone()));
            ui.close_menu();
        }
    }
    ui.separator();
    for (label, open) in [("Close All", false), ("Open All", true)] {
        if ui.button(label).clicked() {
            for (_, id) in &items {
                egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), *id, !open).tap_open(open, ui.ctx());
            }
            ui.close_menu();
        }
    }
    if ui.button("Reset Rollout Order").clicked() {
        FRAME.with(|f| {
            if let Some(f) = f.borrow_mut().as_mut() {
                f.request = Some(Request::Reset);
            }
        });
        ui.close_menu();
    }
}

#[cfg(test)]
mod tests {
    use super::shown_order;

    #[test]
    fn the_users_order_comes_first_then_the_codes() {
        let n = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(shown_order(&n(&["A", "B", "C", "D"]), &n(&["C", "A"])), n(&["C", "A", "B", "D"]));
        assert_eq!(shown_order(&n(&["A", "B"]), &n(&["X"])), n(&["A", "B"]));
    }
}
