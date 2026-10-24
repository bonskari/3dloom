//! Editable Spline > Selection rollout: Vertex / Segment / Spline, Lock
//! Handles, Area Selection, Segment End, Select By, Display options and the
//! selection readout.

use super::{setting_check, setting_spin};
use crate::command_panel::rollout;
use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, SelectionMode};

pub fn show(ui: &mut egui::Ui, app: &mut LoomApp) {
    rollout(ui, "Selection", true, |ui| {
        ui.horizontal(|ui| {
            for (mode, label) in [(SelectionMode::Vertex, "Vertex"), (SelectionMode::Edge, "Segment"), (SelectionMode::Face, "Spline")] {
                let selected = app.state.selection_mode == mode;
                if ui.selectable_label(selected, label).on_hover_text(crate::shortcuts::tip(&app.state, &format!("{label} level"), crate::shortcuts::level_action(mode))).clicked() {
                    let mode = if selected { SelectionMode::Object } else { mode };
                    let _ = app.dispatch(Command::SelectionMode { mode });
                }
            }
        });
        let ss = app.state.spline_settings;
        ui.label("Named Selections:");
        ui.horizontal(|ui| {
            if ui.button("Copy").on_hover_text("Named Sub Selection Copy").clicked() {
                let _ = app.dispatch(Command::NamedSubSelectionCopy { name: "spline".into() });
            }
            if ui.button("Paste").on_hover_text("Named Sub Selection Paste").clicked() {
                let _ = app.dispatch(Command::NamedSubSelectionPaste { name: "spline".into() });
            }
        });
        setting_check(ui, app, "Lock Handles", "Lock Handles", ss.lock_handles);
        ui.horizontal(|ui| {
            setting_check(ui, app, "Area Selection", "Area Selection", ss.area_selection);
            setting_spin(ui, app, "Area Threshold", ss.area_threshold, 0.1, 0.0);
        });
        setting_check(ui, app, "Segment End", "Segment End", ss.segment_end);
        ui.label("Select By: (Vertex / Segment / Spline levels select by click)");
        ui.label("Display:");
        setting_check(ui, app, "Show Vertex Numbers", "Show Vertex Numbers", ss.show_vertex_numbers);
        setting_check(ui, app, "Show Selected Segs", "Show Selected Segs", ss.show_selected_segs);
        let n = app.state.sub_selection.len();
        let what = match app.state.selection_mode {
            SelectionMode::Object => String::new(),
            SelectionMode::Vertex => format!("{n} Vertices Selected"),
            SelectionMode::Edge | SelectionMode::Border => format!("{n} Segments Selected"),
            _ => format!("{n} Splines Selected"),
        };
        if !what.is_empty() {
            ui.label(what);
        }
    });
}
