//! Render SetUp: the output size, and Render.

use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let (mut w, mut h) = app.state.render_size;
    egui::Window::new("Render Setup").open(open).resizable(false).show(ctx, |ui| {
        ui.label(egui::RichText::new("Output Size").strong());
        ui.horizontal(|ui| {
            ui.label("Width:");
            let a = ui.add(egui::DragValue::new(&mut w).range(1..=8192));
            ui.label("Height:");
            let b = ui.add(egui::DragValue::new(&mut h).range(1..=8192));
            if a.changed() || b.changed() {
                let _ = app.dispatch(Command::RenderSetup { width: Some(w), height: Some(h) });
            }
        });
        ui.horizontal(|ui| {
            for (pw, ph) in [(320, 240), (640, 480), (1280, 720), (1920, 1080)] {
                if ui.button(format!("{pw}x{ph}")).clicked() {
                    let _ = app.dispatch(Command::RenderSetup { width: Some(pw), height: Some(ph) });
                }
            }
        });
        ui.separator();
        if ui.button("Render").clicked() {
            let _ = app.dispatch(Command::Render { path: None, width: None, height: None });
        }
    });
}
