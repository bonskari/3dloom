//! Editable Poly > Edit Borders rollout: Extrude, Insert Vertex,
//! Chamfer, Cap, Bridge, Connect, Create Shape, Edit Tri / Turn.

use super::{action, action_keyed, mode_with_settings, pick_on};
use crate::command_panel::rollout;
use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, PickMode, PolyDrag, SelectionMode};

pub fn show(ui: &mut egui::Ui, app: &mut LoomApp) {
    if app.state.selection_mode != SelectionMode::Border {
        return;
    }
    rollout(ui, "Edit Borders", true, |ui| {
        let has = !app.state.sub_selection.is_empty();
        let ps = app.state.poly_settings;
        ui.horizontal(|ui| {
            let on = app.state.poly_drag == Some(PolyDrag::Extrude);
            mode_with_settings(ui, app, "Extrude", "Extrude Mode: drag a selected border", on, Command::ExtrudeMode, Command::ExtrudeSettings);
            let on = pick_on(app, &[PickMode::InsertVertex]);
            if ui.selectable_label(on, "Insert Vertex").on_hover_text("Insert Vertex Mode: click a border edge").clicked() {
                let _ = app.dispatch(Command::InsertVertexMode);
            }
        });
        ui.horizontal(|ui| {
            let on = app.state.poly_drag == Some(PolyDrag::Chamfer);
            mode_with_settings(ui, app, "Chamfer", "Chamfer Mode: drag a selected border", on, Command::ChamferMode, Command::ChamferSettings);
            ui.add_enabled_ui(has, |ui| action(ui, app, "Cap", "Cap the selected borders", Command::Cap));
        });
        ui.horizontal(|ui| {
            ui.add_enabled_ui(has, |ui| action(ui, app, "Bridge", "Bridge the two selected borders", Command::Bridge { segments: ps.bridge_segments, smooth: ps.bridge_smooth, bridge_adjacent: ps.bridge_adjacent, twist1: ps.bridge_twist1, twist2: ps.bridge_twist2 }));
            if crate::icons::small(ui, crate::icons::SETTINGS, "Bridge Settings").clicked() {
                let _ = app.dispatch(Command::BridgeSettings);
            }
            ui.add_enabled_ui(has, |ui| action_keyed(ui, app, "Connect", "Connect", "Connect", Command::ConnectEdges { segments: ps.connect_segments, pinch: ps.connect_pinch, slide: ps.connect_slide }));
            if crate::icons::small(ui, crate::icons::SETTINGS, "Connect Edge Settings").clicked() {
                let _ = app.dispatch(Command::ConnectEdgeSettings);
            }
        });
        ui.horizontal(|ui| {
            let smooth = app.state.spline_settings.create_shape_smooth;
            let has = !app.state.sub_selection.is_empty();
            if ui.add_enabled(has, egui::Button::new("Create Shape From Selection")).on_hover_text("The selected edges become a new Editable Spline").clicked() {
                let _ = app.dispatch(Command::CreateShapeFromEdges { name: None, smooth });
            }
            if ui.radio(!smooth, "Linear").clicked() {
                let _ = app.dispatch(Command::SplineSettings { name: "Create Shape Smooth".into(), value: 0.0 });
            }
            if ui.radio(smooth, "Smooth").clicked() {
                let _ = app.dispatch(Command::SplineSettings { name: "Create Shape Smooth".into(), value: 1.0 });
            }
        });
        ui.horizontal(|ui| {
            let on = pick_on(app, &[PickMode::EditTriangulationA, PickMode::EditTriangulationB]);
            if ui.selectable_label(on, "Edit Tri").on_hover_text("Edit Triangulation Mode").clicked() {
                let _ = app.dispatch(Command::EditTriangulationMode);
            }
            ui.add_enabled_ui(has, |ui| action(ui, app, "Turn", "Turn the selected edges between triangles", Command::Turn));
        });
    });
}
