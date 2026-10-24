//! Editable Poly > Edit Vertices rollout: Remove, Break, Extrude,
//! Weld, Chamfer, Target Weld, Connect, Remove Isolated / Unused Map Vertices.

use super::{action, mode_with_settings, pick_on};
use crate::command_panel::rollout;
use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, PickMode, PolyDrag, SelectionMode};

pub fn show(ui: &mut egui::Ui, app: &mut LoomApp) {
    if app.state.selection_mode != SelectionMode::Vertex {
        return;
    }
    rollout(ui, "Edit Vertices", true, |ui| {
        let has = !app.state.sub_selection.is_empty();
        ui.horizontal(|ui| {
            ui.add_enabled_ui(has, |ui| {
                action(ui, app, "Remove", "Remove the selected vertices", Command::Remove);
                action(ui, app, "Break", "Break the selected vertices", Command::Break);
            });
        });
        ui.horizontal(|ui| {
            let on = app.state.poly_drag == Some(PolyDrag::Extrude);
            mode_with_settings(ui, app, "Extrude", "Extrude Mode: drag a selected vertex", on, Command::ExtrudeMode, Command::ExtrudeSettings);
            let th = app.state.poly_settings.weld_threshold;
            ui.add_enabled_ui(has, |ui| action(ui, app, "Weld", "Weld the selected vertices within the threshold", Command::Weld { threshold: th }));
            if ui.small_button("□").on_hover_text("Weld Settings").clicked() {
                let _ = app.dispatch(Command::WeldSettings);
            }
        });
        ui.horizontal(|ui| {
            let on = app.state.poly_drag == Some(PolyDrag::Chamfer);
            mode_with_settings(ui, app, "Chamfer", "Chamfer Mode: drag a selected vertex", on, Command::ChamferMode, Command::ChamferSettings);
            let on = pick_on(app, &[PickMode::TargetWeldSource, PickMode::TargetWeldTarget]);
            if ui.selectable_label(on, "Target Weld").on_hover_text(crate::shortcuts::tip(&app.state, "Target Weld Mode: pick a vertex, then its target", "Target Weld Mode")).clicked() {
                let _ = app.dispatch(Command::TargetWeldMode);
            }
        });
        ui.horizontal(|ui| {
            ui.add_enabled_ui(has, |ui| action(ui, app, "Connect", "Connect the selected vertex pairs across their polygons", Command::ConnectVertices));
        });
        ui.horizontal(|ui| {
            action(ui, app, "Remove Isolated Vertices", "Delete vertices no polygon uses", Command::RemoveIsolatedVertices);
        });
        ui.horizontal(|ui| {
            action(ui, app, "Remove Unused Map Verts", "Delete map vertices no map face uses", Command::RemoveUnusedMapVertices);
        });
    });
}
