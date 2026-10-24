//! Editable Poly > Edit Elements rollout: Insert Vertex, Flip,
//! Edit Tri, Retriangulate, Turn.

use super::{action, pick_on};
use crate::command_panel::rollout;
use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, PickMode, SelectionMode};

pub fn show(ui: &mut egui::Ui, app: &mut LoomApp) {
    if app.state.selection_mode != SelectionMode::Element {
        return;
    }
    rollout(ui, "Edit Elements", true, |ui| {
        let has = !app.state.sub_selection.is_empty();
        ui.horizontal(|ui| {
            let on = pick_on(app, &[PickMode::InsertVertex]);
            if ui.selectable_label(on, "Insert Vertex").on_hover_text("Insert Vertex Mode").clicked() {
                let _ = app.dispatch(Command::InsertVertexMode);
            }
            ui.add_enabled_ui(has, |ui| action(ui, app, "Flip", "Flip Normals of the selected elements", Command::FlipNormals));
        });
        ui.horizontal(|ui| {
            let on = pick_on(app, &[PickMode::EditTriangulationA, PickMode::EditTriangulationB]);
            if ui.selectable_label(on, "Edit Tri").on_hover_text("Edit Triangulation Mode").clicked() {
                let _ = app.dispatch(Command::EditTriangulationMode);
            }
            ui.add_enabled_ui(has, |ui| {
                action(ui, app, "Retriangulate", "Retriangulate the selected elements", Command::Retriangulate);
                action(ui, app, "Turn", "Turn edges between triangles", Command::Turn);
            });
        });
    });
}
