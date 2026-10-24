//! Editable Poly rollouts of the Modify panel, one file per rollout.

pub mod edit_borders;
pub mod edit_edges;
pub mod edit_elements;
pub mod edit_geometry;
pub mod paint_deformation;
pub mod polygon_properties;
pub mod subdivision_surface;
pub mod vertex_properties;
pub mod edit_polygons;
pub mod edit_vertices;
pub mod selection;
pub mod soft_selection;

use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;

/// A rollout action button that dispatches one command.
pub fn action(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, hover: &str, cmd: Command) {
    if ui.button(label).on_hover_text(hover).clicked() {
        let _ = app.dispatch(cmd);
    }
}

/// A mode button (drawn pressed while the mode is on) and its
/// Settings box, as in the Edit rollouts.
pub fn mode_with_settings(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, hover: &str, on: bool, mode: Command, settings: Command) {
    if ui.selectable_label(on, label).on_hover_text(hover).clicked() {
        let _ = app.dispatch(mode);
    }
    if ui.small_button("□").on_hover_text(format!("{label} Settings")).clicked() {
        let _ = app.dispatch(settings);
    }
}

pub fn pick_on(app: &LoomApp, modes: &[loom_commands::PickMode]) -> bool {
    app.state.pick_mode.is_some_and(|m| modes.contains(&m))
}
