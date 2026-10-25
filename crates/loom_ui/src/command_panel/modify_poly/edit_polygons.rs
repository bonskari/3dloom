//! Editable Poly > Edit Polygons rollout: Insert Vertex, Extrude,
//! Outline, Bevel, Inset, Bridge, Flip, Hinge From Edge, Extrude Along
//! Spline, Edit Tri, Retriangulate, Turn.

use super::{action, mode_with_settings, pick_on};
use crate::command_panel::rollout;
use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, PickMode, PolyDrag, SelectionMode};

pub fn show(ui: &mut egui::Ui, app: &mut LoomApp) {
    if app.state.selection_mode != SelectionMode::Face {
        return;
    }
    rollout(ui, "Edit Polygons", true, |ui| {
        let has = !app.state.sub_selection.is_empty();
        let ps = app.state.poly_settings;
        ui.horizontal(|ui| {
            let on = pick_on(app, &[PickMode::InsertVertex]);
            if ui.selectable_label(on, "Insert Vertex").on_hover_text("Insert Vertex Mode: click inside a polygon").clicked() {
                let _ = app.dispatch(Command::InsertVertexMode);
            }
        });
        ui.horizontal(|ui| {
            let on = app.state.poly_drag == Some(PolyDrag::Extrude);
            mode_with_settings(ui, app, "Extrude", "Extrude Mode: drag the selected polygons", on, Command::ExtrudeMode, Command::ExtrudeSettings);
            let on = app.state.poly_drag == Some(PolyDrag::Outline);
            mode_with_settings(ui, app, "Outline", "Outline Mode: drag to grow or shrink the outline", on, Command::OutlineMode, Command::OutlineSettings);
        });
        ui.horizontal(|ui| {
            let on = app.state.poly_drag == Some(PolyDrag::Bevel);
            mode_with_settings(ui, app, "Bevel", "Bevel Mode: drag up for height, sideways for outline", on, Command::BevelMode, Command::BevelSettings);
            let on = app.state.poly_drag == Some(PolyDrag::Inset);
            mode_with_settings(ui, app, "Inset", "Inset Mode: drag to inset", on, Command::InsetMode, Command::InsetSettings);
        });
        ui.horizontal(|ui| {
            ui.add_enabled_ui(has, |ui| action(ui, app, "Bridge", "Bridge two selected polygons", Command::Bridge { segments: ps.bridge_segments, smooth: ps.bridge_smooth, bridge_adjacent: ps.bridge_adjacent, twist1: ps.bridge_twist1, twist2: ps.bridge_twist2 }));
            if ui.small_button(crate::icons::SETTINGS).on_hover_text("Bridge Settings").clicked() {
                let _ = app.dispatch(Command::BridgeSettings);
            }
            ui.add_enabled_ui(has, |ui| action(ui, app, "Flip", "Flip Normals of the selected polygons (whole elements)", Command::FlipNormals));
        });
        ui.horizontal(|ui| {
            let on = app.state.poly_drag == Some(PolyDrag::Hinge);
            mode_with_settings(ui, app, "Hinge From Edge", "Hinge From Edge Mode: click the hinge edge, then drag", on, Command::HingeFromEdgeMode, Command::HingeFromEdgeSettings);
        });
        ui.horizontal(|ui| {
            let on = pick_on(app, &[PickMode::ExtrudeAlongSpline]);
            mode_with_settings(ui, app, "Extrude Along Spline", "Extrude Along Spline: click a shape to sweep the selected polygons along it", on, Command::ExtrudeAlongSplineMode, Command::ExtrudeAlongSplineSettings);
        });
        ui.horizontal(|ui| {
            let on = pick_on(app, &[PickMode::EditTriangulationA, PickMode::EditTriangulationB]);
            if ui.selectable_label(on, "Edit Tri").on_hover_text("Edit Triangulation Mode").clicked() {
                let _ = app.dispatch(Command::EditTriangulationMode);
            }
            ui.add_enabled_ui(has, |ui| {
                action(ui, app, "Retriangulate", "Retriangulate the selected polygons", Command::Retriangulate);
                action(ui, app, "Turn", "Turn edges between triangles", Command::Turn);
            });
        });
    });
}
