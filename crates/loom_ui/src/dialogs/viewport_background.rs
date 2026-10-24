//! Viewport Background dialog (Alt+B): a PNG for the active viewport,
//! with a Lock Zoom/Pan option.

use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let id = egui::Id::new("viewport_background_path");
    let current = app.state.active().background.clone().unwrap_or_default();
    let mut path: String = ctx.data(|d| d.get_temp(id)).unwrap_or(current.clone());
    egui::Window::new("Viewport Background").open(open).default_width(420.0).show(ctx, |ui| {
        ui.label(egui::RichText::new("Background Source").strong());
        ui.horizontal(|ui| {
            ui.label("Files:");
            ui.add(egui::TextEdit::singleline(&mut path).desired_width(300.0).hint_text("/path/to/image.png"));
        });
        if !current.is_empty() {
            ui.label(format!("Current: {current}"));
        }
        let locked = app.state.active().background_locked;
        let mut l = locked;
        if ui.checkbox(&mut l, "Lock Zoom/Pan").on_hover_text("Background Lock Toggle (Alt+Ctrl+B)").changed() {
            let _ = app.dispatch(Command::BackgroundLockToggle);
        }
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                let p = path.trim().to_owned();
                let _ = app.dispatch(Command::SetViewportBackground { path: (!p.is_empty()).then_some(p) });
            }
            if ui.button("Remove").clicked() {
                path.clear();
                let _ = app.dispatch(Command::SetViewportBackground { path: None });
            }
            if ui.button("Update").clicked() {
                let _ = app.dispatch(Command::UpdateBackgroundImage);
            }
        });
    });
    ctx.data_mut(|d| d.insert_temp(id, path));
}
