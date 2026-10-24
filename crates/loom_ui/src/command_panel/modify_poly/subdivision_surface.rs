//! Editable Poly > Subdivision Surface rollout: Smooth Result,
//! Use NURMS Subdivision, Isoline Display, Show Cage, Display / Render
//! iterations and smoothness, Separate By, Update Options.

use crate::command_panel::rollout;
use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;

pub fn show(ui: &mut egui::Ui, app: &mut LoomApp) {
    let Some(id) = app.state.selection.iter().next().copied() else { return };
    let Some(sub) = app.state.scene.node(id).and_then(|n| n.edit_poly()).map(|p| p.subdiv.clone()) else { return };
    rollout(ui, "Subdivision Surface", false, |ui| {
        let mut smooth = sub.smooth_result;
        if ui.checkbox(&mut smooth, "Smooth Result").changed() {
            let _ = app.dispatch(Command::SubdivisionSurface { use_nurms: None, smooth_result: Some(smooth), isoline_display: None, show_cage: None, iterations: None, smoothness: None, separate_by_smoothing: None, separate_by_materials: None });
        }
        let mut nurms = sub.use_nurms.get();
        if ui.checkbox(&mut nurms, "Use NURMS Subdivision").changed() {
            let _ = app.dispatch(Command::SubdivisionSurface { use_nurms: Some(nurms), smooth_result: None, isoline_display: None, show_cage: None, iterations: None, smoothness: None, separate_by_smoothing: None, separate_by_materials: None });
        }
        ui.horizontal(|ui| {
            let mut iso = sub.isoline_display;
            if ui.checkbox(&mut iso, "Isoline Display").changed() {
                let _ = app.dispatch(Command::SubdivisionSurface { use_nurms: None, smooth_result: None, isoline_display: Some(iso), show_cage: None, iterations: None, smoothness: None, separate_by_smoothing: None, separate_by_materials: None });
            }
            let mut cage = sub.show_cage;
            if ui.checkbox(&mut cage, "Show Cage").changed() {
                let _ = app.dispatch(Command::SubdivisionSurface { use_nurms: None, smooth_result: None, isoline_display: None, show_cage: Some(cage), iterations: None, smoothness: None, separate_by_smoothing: None, separate_by_materials: None });
            }
        });
        ui.label(egui::RichText::new("Display").strong());
        egui::Grid::new("subdiv_display").num_columns(2).show(ui, |ui| {
            ui.label("Iterations:");
            let mut it = sub.iterations.get();
            if ui.add(crate::spinner::spin(ui, &mut it, 0..=4).speed(0.1)).changed() {
                let _ = app.dispatch(Command::SubdivisionSurface { use_nurms: None, smooth_result: None, isoline_display: None, show_cage: None, iterations: Some(it), smoothness: None, separate_by_smoothing: None, separate_by_materials: None });
            }
            ui.end_row();
            ui.label("Smoothness:");
            let mut sm = sub.smoothness.get();
            if ui.add(crate::spinner::spin(ui, &mut sm, 0.0..=1.0).speed(0.01).fixed_decimals(2)).changed() {
                let _ = app.dispatch(Command::SubdivisionSurface { use_nurms: None, smooth_result: None, isoline_display: None, show_cage: None, iterations: None, smoothness: Some(sm), separate_by_smoothing: None, separate_by_materials: None });
            }
            ui.end_row();
        });
        ui.add_enabled_ui(false, |ui| {
            ui.label(egui::RichText::new("Render").strong()).on_disabled_hover_text("Loom does not render; display values are used everywhere");
            ui.checkbox(&mut false, "Iterations / Smoothness");
        });
        ui.label(egui::RichText::new("Separate By").strong());
        ui.horizontal(|ui| {
            let mut sg = sub.separate_by_smoothing;
            if ui.checkbox(&mut sg, "Smoothing Groups").changed() {
                let _ = app.dispatch(Command::SubdivisionSurface { use_nurms: None, smooth_result: None, isoline_display: None, show_cage: None, iterations: None, smoothness: None, separate_by_smoothing: Some(sg), separate_by_materials: None });
            }
            let mut mat = sub.separate_by_materials;
            if ui.checkbox(&mut mat, "Materials").changed() {
                let _ = app.dispatch(Command::SubdivisionSurface { use_nurms: None, smooth_result: None, isoline_display: None, show_cage: None, iterations: None, smoothness: None, separate_by_smoothing: None, separate_by_materials: Some(mat) });
            }
        });
        ui.label(egui::RichText::new("Update Options").strong());
        ui.horizontal(|ui| {
            let _ = ui.radio(true, "Always");
            ui.add_enabled(false, egui::RadioButton::new(false, "When Rendering"));
            ui.add_enabled(false, egui::RadioButton::new(false, "Manually"));
        });
    });
}
