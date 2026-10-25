//! Editable Poly > Edit Geometry rollout, in order: Repeat Last,
//! Constraints, Preserve UVs, Create / Collapse, Attach / Detach, Slice
//! Plane / Split / Slice / Reset Plane, QuickSlice / Cut, MSmooth /
//! Tessellate, Make Planar X Y Z, View Align / Grid Align, Relax, Hide /
//! Unhide, Named Selections, Delete Isolated Vertices, Full Interactivity.

use super::{action, pick_on};
use crate::command_panel::rollout;
use crate::LoomApp;
use eframe::egui;
use loom_commands::{Axis, Command, PickMode, PolyConstraint, SelectionMode};

pub fn show(ui: &mut egui::Ui, app: &mut LoomApp) {
    rollout(ui, "Edit Geometry", true, |ui| {
        let level = app.state.selection_mode;
        let sub = level != SelectionMode::Object;
        let has = sub && !app.state.sub_selection.is_empty();
        let ps = app.state.poly_settings;
        action(ui, app, "Repeat Last", "Repeat Last Operation", Command::RepeatLastOperation);
        ui.label("Constraints:");
        ui.horizontal(|ui| {
            for (c, l, cmd) in [
                (PolyConstraint::None, "None", Command::ConstrainToNone),
                (PolyConstraint::Edges, "Edge", Command::ConstrainToEdges),
                (PolyConstraint::Faces, "Face", Command::ConstrainToFaces),
                (PolyConstraint::Normals, "Normal", Command::ConstrainToNormals),
            ] {
                if ui.radio(app.state.poly_constraint == c, l).clicked() && app.state.poly_constraint != c {
                    let _ = app.dispatch(cmd);
                }
            }
        });
        let mut uv = app.state.preserve_uvs;
        if ui.checkbox(&mut uv, "Preserve UVs").changed() {
            let _ = app.dispatch(Command::PreserveUVs { on: uv });
        }
        ui.horizontal(|ui| {
            let creating = pick_on(app, &[PickMode::CreatePolygon, PickMode::CreateVertex]);
            if ui.add_enabled(matches!(level, SelectionMode::Vertex | SelectionMode::Face | SelectionMode::Element), egui::SelectableLabel::new(creating, "Create")).on_hover_text("Create: vertices (vertex level) or a polygon from picked vertices").clicked() {
                let _ = app.dispatch(Command::Create);
            }
            ui.add_enabled_ui(has, |ui| action(ui, app, "Collapse", "Collapse the selection to a point", Command::Collapse));
        });
        ui.horizontal(|ui| {
            let on = pick_on(app, &[PickMode::AttachObject]);
            if ui.selectable_label(on, "Attach").on_hover_text("Attach: click an object").clicked() {
                let _ = app.dispatch(Command::Attach);
            }
            if ui.small_button(crate::icons::SETTINGS).on_hover_text("Attach List").clicked() {
                let _ = app.dispatch(Command::AttachList);
            }
            ui.add_enabled_ui(has, |ui| action(ui, app, "Detach", "Detach the selection", Command::Detach));
        });
        ui.horizontal(|ui| {
            if ui.add_enabled(sub, egui::SelectableLabel::new(app.state.slice_plane_mode, "Slice Plane")).on_hover_text("Show the Slice Plane; move it with the transform tools").clicked() {
                let _ = app.dispatch(Command::SlicePlaneMode);
            }
            let mut split = app.state.slice_split;
            if ui.checkbox(&mut split, "Split").changed() {
                let _ = app.dispatch(Command::SliceSplit { on: split });
            }
        });
        ui.horizontal(|ui| {
            ui.add_enabled_ui(app.state.slice_plane_mode, |ui| {
                action(ui, app, "Slice", "Slice with the plane", Command::Slice);
                action(ui, app, "Reset Plane", "Reset the Slice Plane", Command::ResetSlicePlane);
            });
        });
        ui.horizontal(|ui| {
            let on = pick_on(app, &[PickMode::QuickSliceA, PickMode::QuickSliceB]);
            if ui.selectable_label(on, "QuickSlice").on_hover_text(crate::shortcuts::tip(&app.state, "QuickSlice: click two points", "Quickslice Mode")).clicked() {
                let _ = app.dispatch(Command::QuickSliceMode);
            }
            let on = pick_on(app, &[PickMode::CutA, PickMode::CutB]);
            if ui.selectable_label(on, "Cut").on_hover_text(crate::shortcuts::tip(&app.state, "Cut: click the start and end points on the surface", "Cut")).clicked() {
                let _ = app.dispatch(Command::Cut);
            }
        });
        ui.horizontal(|ui| {
            action(ui, app, "MSmooth", "MeshSmooth with the current settings", Command::MeshSmooth);
            if ui.small_button(crate::icons::SETTINGS).on_hover_text("MeshSmooth Settings").clicked() {
                let _ = app.dispatch(Command::MeshSmoothSettings);
            }
            ui.add_enabled_ui(has, |ui| action(ui, app, "Tessellate", "Tessellate the selection", Command::Tessellate));
            if ui.small_button(crate::icons::SETTINGS).on_hover_text("Tessellate Settings").clicked() {
                let _ = app.dispatch(Command::TessellateSettings);
            }
        });
        ui.horizontal(|ui| {
            ui.add_enabled_ui(has, |ui| {
                action(ui, app, "Make Planar", "Make Planar: onto the average plane", Command::MakePlanar);
                for (a, l) in [(Axis::X, "X"), (Axis::Y, "Y"), (Axis::Z, "Z")] {
                    action(ui, app, l, &format!("Make Planar along {l}"), Command::MakePlanarAxis { axis: a });
                }
            });
        });
        ui.horizontal(|ui| {
            ui.add_enabled_ui(has, |ui| {
                action(ui, app, "View Align", "Flatten onto the view plane", Command::ViewAlign);
                action(ui, app, "Grid Align", "Flatten onto the home grid", Command::AlignToGrid);
            });
        });
        ui.horizontal(|ui| {
            action(ui, app, "Relax", "Relax the selection with the current settings", Command::Relax { amount: ps.relax_amount, iterations: ps.relax_iterations, hold_boundary: ps.relax_hold_boundary, hold_outer: ps.relax_hold_outer });
            if ui.small_button(crate::icons::SETTINGS).on_hover_text("Relax Settings").clicked() {
                let _ = app.dispatch(Command::RelaxSettings);
            }
        });
        ui.horizontal(|ui| {
            ui.add_enabled_ui(has, |ui| action(ui, app, "Hide Selected", "Hide the selected sub-objects", Command::Hide));
            ui.add_enabled_ui(sub, |ui| action(ui, app, "Unhide All", "Unhide all sub-objects", Command::UnhideAll));
        });
        ui.add_enabled_ui(has, |ui| action(ui, app, "Hide Unselected", "Hide the unselected sub-objects", Command::HideUnselected));
        ui.label("Named Selections:");
        ui.horizontal(|ui| {
            let name = app.named_sub_selection_name.clone();
            let mut n = name.clone();
            if ui.add(egui::TextEdit::singleline(&mut n).desired_width(70.0).hint_text("name")).changed() {
                app.named_sub_selection_name = n.clone();
            }
            ui.add_enabled_ui(has && !n.trim().is_empty(), |ui| action(ui, app, "Copy", "Copy the sub-object selection under this name", Command::NamedSubSelectionCopy { name: n.clone() }));
            ui.add_enabled_ui(sub && !n.trim().is_empty(), |ui| action(ui, app, "Paste", "Paste the named sub-object selection", Command::NamedSubSelectionPaste { name: n.clone() }));
        });
        action(ui, app, "Delete Isolated Vertices", "Delete vertices no polygon uses", Command::DeleteIsolatedVertices);
        let mut fi = app.state.full_interactivity;
        if ui.checkbox(&mut fi, "Full Interactivity").changed() {
            let _ = app.dispatch(Command::FullInteractivity { on: fi });
        }
    });
}
