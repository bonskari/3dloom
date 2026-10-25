//! Editable Mesh / Edit Mesh rollouts: Selection (Vertex, Edge, Face,
//! Polygon, Element, Ignore Visible Edges, Planar Thresh), Edit Geometry
//! (Attach, Detach, Delete, Divide, Turn, Extrude, Bevel, Explode,
//! Tessellate, Collapse, Weld) and Surface Properties (edge visibility and
//! Auto Edge; Flip / Unify and the polygon properties at the face levels).

use crate::command_panel::modify_poly::action;
use crate::command_panel::rollout;
use crate::LoomApp;
use eframe::egui;
use loom_commands::mesh_edit::current_level;
use loom_commands::{Command, PickMode, SelectionMode};

/// The five Editable Mesh levels in order.
pub const LEVELS: [&str; 5] = ["Vertex", "Edge", "Face", "Polygon", "Element"];

pub fn show(ui: &mut egui::Ui, app: &mut LoomApp) {
    selection(ui, app);
    super::modify_poly::soft_selection::show(ui, app);
    edit_geometry(ui, app);
    surface_properties(ui, app);
}

fn selection(ui: &mut egui::Ui, app: &mut LoomApp) {
    rollout(ui, "Selection", true, |ui| {
        let level = current_level(&app.state);
        ui.horizontal_wrapped(|ui| {
            for (i, l) in LEVELS.iter().enumerate() {
                let lvl = i as u32 + 1;
                if ui.selectable_label(level == lvl, *l).on_hover_text(format!("{l} ({lvl})")).clicked() {
                    let _ = app.dispatch(Command::SubObjectLevel { level: if level == lvl { 0 } else { lvl } });
                }
            }
        });
        let mut ib = app.state.ignore_backfacing;
        if ui.checkbox(&mut ib, "Ignore Backfacing").changed() {
            let _ = app.dispatch(Command::IgnoreBackfacingInSelections);
        }
        let polygon = level == 4;
        ui.add_enabled_ui(polygon, |ui| {
            let mut ive = app.state.mesh_patch.ignore_visible_edges;
            if ui.checkbox(&mut ive, "Ignore Visible Edges").on_hover_text("Polygon level: a click selects the faces within Planar Thresh, whatever the edge visibility").changed() {
                let _ = app.dispatch(Command::MeshPatchSetting { name: "Ignore Visible Edges".into(), value: ive as u8 as f64 });
            }
            ui.horizontal(|ui| {
                ui.label("Planar Thresh:");
                let mut t = app.state.mesh_patch.planar_thresh;
                if ui.add_enabled(ive, crate::spinner::spin(ui, &mut t, 0.0..=180.0).speed(app.spinner_speed(0.5)).fixed_decimals(1)).changed() {
                    let _ = app.dispatch(Command::MeshPatchSetting { name: "Planar Thresh".into(), value: t as f64 });
                }
            });
        });
        let n = app.state.sub_selection.len();
        let what = match level {
            1 => "Vertices",
            2 => "Edges",
            3..=5 => "Faces",
            _ => "",
        };
        if level > 0 {
            ui.label(format!("{n} {what} Selected"));
        }
    });
}

fn edit_geometry(ui: &mut egui::Ui, app: &mut LoomApp) {
    rollout(ui, "Edit Geometry", true, |ui| {
        let level = current_level(&app.state);
        let has = level > 0 && !app.state.sub_selection.is_empty();
        let faces = matches!(level, 3..=5);
        ui.horizontal(|ui| {
            let on = app.state.pick_mode.is_some_and(|m| m == PickMode::AttachObject);
            if ui.selectable_label(on, "Attach").on_hover_text("Attach: click an object; it joins as triangles").clicked() {
                let _ = app.dispatch(Command::Attach);
            }
            ui.add_enabled_ui(has && level != 2, |ui| action(ui, app, "Detach", "Detach the selected faces", Command::Detach));
        });
        ui.horizontal(|ui| {
            ui.add_enabled_ui(has, |ui| {
                action(ui, app, "Delete", "Delete the selection", Command::DeleteObjects);
                action(ui, app, "Collapse", "Collapse the selection to a point", Command::Collapse);
            });
        });
        ui.horizontal(|ui| {
            ui.add_enabled_ui(has && (level == 2 || faces), |ui| action(ui, app, "Divide", "Divide: edges split at their middle, faces at their centre (new edges invisible)", Command::MeshDivide));
            ui.add_enabled_ui(has && level == 2, |ui| action(ui, app, "Turn", "Turn: each selected edge becomes the other diagonal of its two faces", Command::MeshTurnEdge { edges: Vec::new() }));
        });
        ui.add_enabled_ui(faces, |ui| {
            let mp = app.state.mesh_patch.clone();
            ui.horizontal(|ui| {
                let mut amount = mp.extrude;
                if ui.add_enabled(has, egui::Button::new("Extrude")).on_hover_text("Extrude the selected faces by the amount").clicked() {
                    let _ = app.dispatch(Command::MeshExtrude { amount });
                }
                if ui.add(crate::spinner::spin(ui, &mut amount, f32::MIN..=f32::MAX).speed(app.spinner_speed(0.1))).changed() {
                    let _ = app.dispatch(Command::MeshPatchSetting { name: "Extrude".into(), value: amount as f64 });
                }
            });
            ui.horizontal(|ui| {
                let mut outline = mp.outline;
                if ui.add_enabled(has, egui::Button::new("Bevel")).on_hover_text("Bevel: Extrude by the Extrude amount, then Outline").clicked() {
                    let _ = app.dispatch(Command::MeshBevel { height: mp.extrude, outline });
                }
                ui.label("Outline:");
                if ui.add(crate::spinner::spin(ui, &mut outline, f32::MIN..=f32::MAX).speed(app.spinner_speed(0.1))).changed() {
                    let _ = app.dispatch(Command::MeshPatchSetting { name: "Outline".into(), value: outline as f64 });
                }
            });
            ui.horizontal(|ui| {
                if ui.add_enabled(has, egui::Button::new("Tessellate")).on_hover_text("Tessellate the selected faces").clicked() {
                    let _ = app.dispatch(Command::MeshTessellate);
                }
                ui.label("Tension:");
                let mut t = mp.tension;
                if ui.add(crate::spinner::spin(ui, &mut t, -100.0..=100.0).speed(app.spinner_speed(0.5))).changed() {
                    let _ = app.dispatch(Command::MeshPatchSetting { name: "Tension".into(), value: t as f64 });
                }
            });
            ui.horizontal(|ui| {
                if ui.radio(!mp.face_center, "By Edge").clicked() {
                    let _ = app.dispatch(Command::MeshPatchSetting { name: "Face-Center".into(), value: 0.0 });
                }
                if ui.radio(mp.face_center, "Face-Center").clicked() {
                    let _ = app.dispatch(Command::MeshPatchSetting { name: "Face-Center".into(), value: 1.0 });
                }
            });
        });
        let mp = app.state.mesh_patch.clone();
        ui.horizontal(|ui| {
            if ui.button("Explode").on_hover_text("Explode the selected faces (all faces at the top level) by the angle").clicked() {
                let _ = app.dispatch(Command::MeshExplode { name: None });
            }
            let mut a = mp.explode_angle;
            if ui.add(crate::spinner::spin(ui, &mut a, 0.0..=180.0).speed(app.spinner_speed(0.5))).changed() {
                let _ = app.dispatch(Command::MeshPatchSetting { name: "Explode Angle".into(), value: a as f64 });
            }
        });
        ui.horizontal(|ui| {
            if ui.radio(mp.explode_to_objects, "Objects").clicked() {
                let _ = app.dispatch(Command::MeshPatchSetting { name: "Explode To Objects".into(), value: 1.0 });
            }
            if ui.radio(!mp.explode_to_objects, "Elements").clicked() {
                let _ = app.dispatch(Command::MeshPatchSetting { name: "Explode To Objects".into(), value: 0.0 });
            }
        });
        ui.add_enabled_ui(has && level == 1, |ui| {
            let t = app.state.poly_settings.weld_threshold;
            action(ui, app, "Weld Selected", "Weld the selected vertices within the Weld threshold", Command::Weld { threshold: t });
        });
    });
}

fn surface_properties(ui: &mut egui::Ui, app: &mut LoomApp) {
    let level = current_level(&app.state);
    if level == 2 {
        rollout(ui, "Surface Properties", true, |ui| {
            let has = !app.state.sub_selection.is_empty();
            ui.horizontal(|ui| {
                ui.add_enabled_ui(has, |ui| {
                    action(ui, app, "Visible", "Make the selected edges visible", Command::MeshEdgeVisibility { visible: true });
                    action(ui, app, "Invisible", "Make the selected edges invisible", Command::MeshEdgeVisibility { visible: false });
                });
            });
            ui.label("Auto Edge");
            let mp = app.state.mesh_patch.clone();
            ui.horizontal(|ui| {
                ui.label("Threshold:");
                let mut t = mp.auto_edge_threshold;
                if ui.add(crate::spinner::spin(ui, &mut t, 0.0..=180.0).speed(app.spinner_speed(0.5))).changed() {
                    let _ = app.dispatch(Command::MeshPatchSetting { name: "Auto Edge Threshold".into(), value: t as f64 });
                }
            });
            for (i, l) in ["Set and Clear Edge Vis", "Set", "Clear"].iter().enumerate() {
                let on = mp.auto_edge_mode as usize == i;
                if ui.radio(on, *l).clicked() {
                    let _ = app.dispatch(Command::MeshPatchSetting { name: "Auto Edge Mode".into(), value: i as f64 });
                }
            }
            ui.add_enabled_ui(has, |ui| action(ui, app, "Auto Edge", "Auto Edge on the selected edges", Command::MeshAutoEdge));
        });
    }
    if matches!(level, 3..=5) {
        rollout(ui, "Surface Properties", true, |ui| {
            let has = !app.state.sub_selection.is_empty();
            ui.label("Normals:");
            ui.horizontal(|ui| {
                ui.add_enabled_ui(has, |ui| {
                    action(ui, app, "Flip", "Flip the normals of the selected faces (whole elements)", Command::MeshFlipNormals);
                    action(ui, app, "Unify", "Unify: the elements of the selected faces face outward", Command::MeshUnifyNormals);
                });
            });
        });
        // Material IDs and smoothing groups: the polygon properties work on the triangles.
        let mode = app.state.selection_mode;
        if mode == SelectionMode::Face || mode == SelectionMode::Element {
            super::modify_poly::polygon_properties::show(ui, app);
        }
    }
}
