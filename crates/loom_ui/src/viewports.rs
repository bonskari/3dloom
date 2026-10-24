//! Lays the viewports out per `AppState::layout`, draws the active frame
//! and the draggable splitters, and dispatches each viewport's drawing to
//! `viewport_widget::show_one`.

use crate::{viewport_widget, LoomApp};
use eframe::egui;
use loom_commands::Command;

const GAP: f32 = 2.0;
pub const ACTIVE_FRAME: egui::Color32 = egui::Color32::from_rgb(255, 230, 0);

/// Screen rects of every viewport cell for the given panel rect.
pub fn cell_rects(app: &LoomApp, panel: egui::Rect) -> Vec<egui::Rect> {
    if app.state.maximized {
        let mut v: Vec<egui::Rect> = app.state.viewports.iter().map(|_| egui::Rect::NOTHING).collect();
        v[app.state.active_viewport] = panel;
        return v;
    }
    app.state
        .layout
        .cells(&app.state.splits)
        .iter()
        .map(|c| {
            let r = egui::Rect::from_min_max(
                egui::pos2(panel.min.x + c[0] * panel.width(), panel.min.y + c[1] * panel.height()),
                egui::pos2(panel.min.x + c[2] * panel.width(), panel.min.y + c[3] * panel.height()),
            );
            r.shrink(GAP * 0.5)
        })
        .collect()
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp) {
    let animating = ctx.input(|i| app.update_view_animation(i.stable_dt.clamp(0.001, 0.1)));
    if animating {
        ctx.request_repaint();
    }
    egui::CentralPanel::default()
        .frame(egui::Frame::central_panel(&ctx.style()).fill(egui::Color32::from_rgb(80, 80, 80)).inner_margin(0.0))
        .show(ctx, |ui| {
            let panel = ui.available_rect_before_wrap();
            let rects = cell_rects(app, panel);
            app.viewport_rects = rects.clone();
            for (i, rect) in rects.iter().enumerate() {
                if rect.width() < 1.0 || rect.height() < 1.0 {
                    continue;
                }
                viewport_widget::show_one(ui, app, i, *rect);
            }
            if let Some(active) = rects.get(app.state.active_viewport) {
                if active.width() >= 1.0 {
                    // Auto Key draws the active viewport's frame red.
                    let c = if app.state.auto_key { egui::Color32::from_rgb(220, 30, 30) } else { ACTIVE_FRAME };
                    ui.painter().rect_stroke(*active, 0.0, egui::Stroke::new(if app.state.auto_key { 3.0 } else { 1.5 }, c));
                }
            }
            if !app.state.maximized && !app.state.ui_locked {
                splitters(ui, app, panel);
            }
        });
}

/// Drag handles on the layout's split lines → `Set Splits`.
fn splitters(ui: &mut egui::Ui, app: &mut LoomApp, panel: egui::Rect) {
    use loom_commands::ViewportLayout as L;
    let layout = app.state.layout;
    let uses_x = matches!(layout, L::TwoV | L::ThreeLeftBig | L::ThreeTopBig | L::ThreeRightBig | L::ThreeBottomBig | L::FourGrid | L::FourLeftBig | L::FourRightBig);
    let uses_y = matches!(layout, L::TwoH | L::ThreeLeftBig | L::ThreeTopBig | L::ThreeRightBig | L::ThreeBottomBig | L::FourGrid | L::FourTopBig | L::FourBottomBig);
    let s = app.state.splits;
    if uses_x {
        let x = panel.min.x + s.x * panel.width();
        let r = egui::Rect::from_center_size(egui::pos2(x, panel.center().y), egui::vec2(6.0, panel.height()));
        let resp = ui.interact(r, ui.id().with("split_x"), egui::Sense::drag());
        if resp.hovered() || resp.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        if resp.dragged() {
            if let Some(p) = resp.interact_pointer_pos() {
                let nx = (p.x - panel.min.x) / panel.width();
                let _ = app.dispatch(Command::SetSplits { x: Some(nx), y: None });
            }
        }
    }
    if uses_y {
        let y = panel.min.y + s.y * panel.height();
        let r = egui::Rect::from_center_size(egui::pos2(panel.center().x, y), egui::vec2(panel.width(), 6.0));
        let resp = ui.interact(r, ui.id().with("split_y"), egui::Sense::drag());
        if resp.hovered() || resp.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }
        if resp.dragged() {
            if let Some(p) = resp.interact_pointer_pos() {
                let ny = (p.y - panel.min.y) / panel.height();
                let _ = app.dispatch(Command::SetSplits { x: None, y: Some(ny) });
            }
        }
    }
}
