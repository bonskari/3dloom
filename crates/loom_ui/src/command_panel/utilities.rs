//! Utilities tab: Measure, Rescale World Units, Collapse, Reset XForm.

use super::rollout;
use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Utility {
    #[default]
    None,
    Measure,
    RescaleWorldUnits,
    Collapse,
    ResetXForm,
}

pub fn show(ui: &mut egui::Ui, app: &mut LoomApp) {
    rollout(ui, "Utilities", true, |ui| {
        let w = ui.available_width();
        for (label, u) in [
            ("Measure", Utility::Measure),
            ("Rescale World Units", Utility::RescaleWorldUnits),
            ("Collapse", Utility::Collapse),
            ("Reset XForm", Utility::ResetXForm),
        ] {
            if ui.add_sized([w, 22.0], egui::SelectableLabel::new(app.utility == u, label)).clicked() {
                app.utility = if app.utility == u { Utility::None } else { u };
            }
        }
    });
    let has = !app.state.selection.is_empty();
    match app.utility {
        Utility::None => {}
        Utility::Measure => rollout(ui, "Measure", true, |ui| {
            match app.dispatch(Command::Measure) {
                Ok(r) if !r["dimensions"].is_null() => {
                    let d = &r["dimensions"];
                    ui.label(format!("Dimensions:  X {:.3}  Y {:.3}  Z {:.3}", d[0].as_f64().unwrap_or(0.0), d[1].as_f64().unwrap_or(0.0), d[2].as_f64().unwrap_or(0.0)));
                    ui.label(format!("Surface Area: {:.3}", r["surface_area"].as_f64().unwrap_or(0.0)));
                    ui.label(format!("Volume: {:.3}", r["volume"].as_f64().unwrap_or(0.0)));
                    let c = &r["center_of_mass"];
                    ui.label(format!("Center of Mass:  X {:.3}  Y {:.3}  Z {:.3}", c[0].as_f64().unwrap_or(0.0), c[1].as_f64().unwrap_or(0.0), c[2].as_f64().unwrap_or(0.0)));
                }
                _ => {
                    ui.label("Select an object to measure.");
                }
            }
        }),
        Utility::RescaleWorldUnits => rollout(ui, "Rescale World Units", true, |ui| {
            ui.horizontal(|ui| {
                ui.label("Scale Factor:");
                ui.add(crate::spinner::spin(ui, &mut app.rescale_factor, 0.0001..=10_000.0).speed(0.01));
            });
            if ui.button("Rescale").clicked() {
                let _ = app.dispatch(Command::RescaleWorldUnits { factor: app.rescale_factor });
            }
        }),
        Utility::Collapse => rollout(ui, "Collapse", true, |ui| {
            ui.add_enabled_ui(has, |ui| {
                if ui.button("Collapse Selected").clicked() {
                    let _ = app.dispatch(Command::ConvertToEditablePoly);
                }
            });
        }),
        Utility::ResetXForm => rollout(ui, "Reset Transform", true, |ui| {
            ui.add_enabled_ui(has, |ui| {
                if ui.button("Reset Selected").clicked() {
                    let _ = app.dispatch(Command::ResetXForm);
                }
            });
        }),
    }
}
