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

/// A rollout action button that dispatches one command; the hover text carries its chord.
pub fn action(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, hover: &str, cmd: Command) {
    let key = crate::shortcuts::action_of(&cmd);
    action_keyed(ui, app, label, hover, key, cmd);
}

/// [`action`] for a command whose chord belongs to another action name (the
/// Connect button dispatches Connect Edges with its settings; Shift+Ctrl+E runs Connect).
pub fn action_keyed(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, hover: &str, key: &str, cmd: Command) {
    if ui.button(label).on_hover_text(crate::shortcuts::tip(&app.state, hover, key)).clicked() {
        let _ = app.dispatch(cmd);
    }
}

/// A mode button (drawn pressed while the mode is on) and its
/// Settings box, as in the Edit rollouts.
pub fn mode_with_settings(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, hover: &str, on: bool, mode: Command, settings: Command) {
    let hover = crate::shortcuts::tip(&app.state, hover, crate::shortcuts::action_of(&mode));
    if ui.selectable_label(on, label).on_hover_text(hover).clicked() {
        let _ = app.dispatch(mode);
    }
    if crate::icons::small(ui, crate::icons::SETTINGS, &format!("{label} Settings")).clicked() {
        let _ = app.dispatch(settings);
    }
}

pub fn pick_on(app: &LoomApp, modes: &[loom_commands::PickMode]) -> bool {
    app.state.pick_mode.is_some_and(|m| modes.contains(&m))
}
