//! Editable Poly > Edit Edges rollout: Insert Vertex, Remove,
//! Split, Extrude, Weld, Chamfer, Target Weld, Bridge, Connect, Create Shape
//! From Selection (phase 3), Edit Tri / Turn.

use super::{action, action_keyed, mode_with_settings, pick_on};
use crate::command_panel::rollout;
use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, PickMode, PolyDrag, SelectionMode};

pub fn show(ui: &mut egui::Ui, app: &mut LoomApp) {
    if app.state.selection_mode != SelectionMode::Edge {
        return;
    }
    rollout(ui, "Edit Edges", true, |ui| {
        let has = !app.state.sub_selection.is_empty();
        let ps = app.state.poly_settings;
        ui.horizontal(|ui| {
            let on = pick_on(app, &[PickMode::InsertVertex]);
            if ui.selectable_label(on, "Insert Vertex").on_hover_text("Insert Vertex Mode: click an edge").clicked() {
                let _ = app.dispatch(Command::InsertVertexMode);
            }
        });
        ui.horizontal(|ui| {
            ui.add_enabled_ui(has, |ui| {
                action(ui, app, "Remove", "Remove the selected edges (Backspace)", Command::Remove);
                action(ui, app, "Split", "Split the mesh along the selected edges", Command::SplitEdges);
            });
        });
        ui.horizontal(|ui| {
            let on = app.state.poly_drag == Some(PolyDrag::Extrude);
            mode_with_settings(ui, app, "Extrude", "Extrude Mode: drag a selected edge", on, Command::ExtrudeMode, Command::ExtrudeSettings);
            ui.add_enabled_ui(has, |ui| action(ui, app, "Weld", "Weld the selected edges' vertices within the threshold", Command::Weld { threshold: ps.weld_threshold }));
            if ui.small_button("□").on_hover_text("Weld Settings").clicked() {
                let _ = app.dispatch(Command::WeldSettings);
            }
        });
        ui.horizontal(|ui| {
            let on = app.state.poly_drag == Some(PolyDrag::Chamfer);
            mode_with_settings(ui, app, "Chamfer", "Chamfer Mode: drag a selected edge", on, Command::ChamferMode, Command::ChamferSettings);
            let on = pick_on(app, &[PickMode::TargetWeldSource, PickMode::TargetWeldTarget]);
            if ui.selectable_label(on, "Target Weld").on_hover_text(crate::shortcuts::name_tip(&app.state, "Target Weld Mode")).clicked() {
                let _ = app.dispatch(Command::TargetWeldMode);
            }
        });
        ui.horizontal(|ui| {
            ui.add_enabled_ui(has, |ui| action(ui, app, "Bridge", "Bridge two selected border loops", Command::Bridge { segments: ps.bridge_segments, smooth: ps.bridge_smooth, bridge_adjacent: ps.bridge_adjacent, twist1: ps.bridge_twist1, twist2: ps.bridge_twist2 }));
            if ui.small_button("□").on_hover_text("Bridge Settings").clicked() {
                let _ = app.dispatch(Command::BridgeSettings);
            }
            ui.add_enabled_ui(has, |ui| action_keyed(ui, app, "Connect", "Connect: new edges across the selected edges", "Connect", Command::ConnectEdges { segments: ps.connect_segments, pinch: ps.connect_pinch, slide: ps.connect_slide }));
            if ui.small_button("□").on_hover_text("Connect Edge Settings").clicked() {
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
            if ui.selectable_label(on, "Edit Tri").on_hover_text("Edit Triangulation Mode: pick two vertices of a polygon").clicked() {
                let _ = app.dispatch(Command::EditTriangulationMode);
            }
            ui.add_enabled_ui(has, |ui| action(ui, app, "Turn", "Turn the selected edges between triangles", Command::Turn));
        });
    });
}
