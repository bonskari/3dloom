//! "Delete isolated vertices?" when deleting polygons would leave vertices
//! with no polygon.

use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let mut answer = None;
    egui::Window::new("Delete Polygons").open(open).resizable(false).collapsible(false).show(ctx, |ui| {
        ui.label("Delete isolated vertices?");
        ui.horizontal(|ui| {
            if ui.button("Yes").clicked() {
                answer = Some(true);
            }
            if ui.button("No").clicked() {
                answer = Some(false);
            }
        });
    });
    if let Some(delete_isolated) = answer {
        let _ = app.dispatch(Command::DeletePolygons { delete_isolated });
        *open = false;
    }
}
