//! Tools > Display Floater: a modeless window with the Display panel's
//! Hide / Freeze controls (Hide/Freeze tab) and its Hide by Category and
//! Display Properties (Object Level tab), open beside any panel.

use crate::command_panel::display;
use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;

pub fn show(ctx: &egui::Context, app: &mut LoomApp) {
    if !app.state.display_floater {
        return;
    }
    let mut open = true;
    let tab_id = egui::Id::new("display_floater_tab");
    let mut object_level: bool = ctx.data(|d| d.get_temp(tab_id)).unwrap_or(false);
    egui::Window::new("Display Floater").open(&mut open).default_width(220.0).resizable(false).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut object_level, false, "Hide/Freeze");
            ui.selectable_value(&mut object_level, true, "Object Level");
        });
        ui.separator();
        if object_level {
            ui.label(egui::RichText::new("Hide by Category").strong());
            display::hide_by_category(ui, app);
            ui.separator();
            ui.label(egui::RichText::new("Display Properties").strong());
            display::display_properties(ui, app);
        } else {
            ui.columns(2, |cols| {
                cols[0].label(egui::RichText::new("Hide").strong());
                display::hide(&mut cols[0], app);
                cols[1].label(egui::RichText::new("Freeze").strong());
                display::freeze(&mut cols[1], app);
            });
        }
    });
    ctx.data_mut(|d| d.insert_temp(tab_id, object_level));
    if !open {
        let _ = app.dispatch(Command::DisplayFloater);
    }
}
