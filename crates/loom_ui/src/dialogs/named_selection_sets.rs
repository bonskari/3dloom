//! Edit Named Selection Sets dialog.

use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let id = egui::Id::new("named_sets_new_name");
    let mut new_name: String = ctx.data(|d| d.get_temp(id)).unwrap_or_default();
    egui::Window::new("Named Selection Sets").open(open).default_width(300.0).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut new_name);
            if ui.button("Create New Set").clicked() && !new_name.trim().is_empty() {
                let _ = app.dispatch(Command::NamedSelectionSet { name: new_name.trim().to_owned() });
                new_name.clear();
            }
        });
        ui.separator();
        let sets: Vec<(String, usize)> = app.state.named_sets.iter().map(|(k, v)| (k.clone(), v.len())).collect();
        for (name, n) in sets {
            ui.horizontal(|ui| {
                ui.label(format!("{name} ({n})"));
                if ui.button("Select").clicked() {
                    let _ = app.dispatch(Command::SelectNamedSet { name: name.clone() });
                }
                if ui.button("Update").clicked() {
                    let _ = app.dispatch(Command::NamedSelectionSet { name: name.clone() });
                }
                if ui.button("Remove").clicked() {
                    let _ = app.dispatch(Command::DeleteNamedSet { name: name.clone() });
                }
            });
        }
    });
    ctx.data_mut(|d| d.insert_temp(id, new_name));
}
