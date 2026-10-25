//! Utilities tab: Measure, Rescale World Units, Collapse, Reset XForm.

use super::rollout;
use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Utility {
    #[default]
    None,
    Measure,
    RescaleWorldUnits,
    Collapse,
    ResetXForm,
    AssignVertexColors,
}

pub fn show(ui: &mut egui::Ui, app: &mut LoomApp) {
    rollout(ui, "Utilities", true, |ui| {
        let w = ui.available_width();
        for (label, u) in [
            ("Measure", Utility::Measure),
            ("Rescale World Units", Utility::RescaleWorldUnits),
            ("Collapse", Utility::Collapse),
            ("Reset XForm", Utility::ResetXForm),
            ("Assign Vertex Colors", Utility::AssignVertexColors),
        ] {
            if ui.add_sized([w, 22.0], egui::SelectableLabel::new(app.utility == u, label)).clicked() {
                app.utility = if app.utility == u { Utility::None } else { u };
            }
        }
    });
    let has = !app.state.selection.is_empty();
    match app.utility {
        Utility::None => {}
        Utility::Measure => rollout(ui, "Measure", true, |ui| {
            let r = measured(app);
            ui.label(r["name"].as_str().unwrap_or("").to_owned());
            let mut lock = app.measure_lock.is_some();
            if ui.checkbox(&mut lock, "Lock Selected").changed() {
                app.measure_lock = lock.then(|| app.state.selection.iter().map(|n| n.0).collect());
            }
            measure_groups(ui, &r);
            ui.add_enabled_ui(!r["center_of_mass"].is_null() && app.measure_lock.is_none(), |ui| {
                if ui.button("Create Center Point").clicked() {
                    let _ = app.dispatch(Command::MeasureCreateCenterPoint);
                }
            });
            ui.horizontal(|ui| {
                if ui.button("New Floater").clicked() {
                    app.measure_floater = true;
                }
                if ui.button("Close").clicked() {
                    app.utility = Utility::None;
                }
            });
        }),
        Utility::RescaleWorldUnits => rollout(ui, "Rescale World Units", true, |ui| {
            ui.horizontal(|ui| {
                ui.label("Scale Factor:");
                ui.add(crate::spinner::spin(ui, &mut app.rescale_factor, 0.0001..=10_000.0).speed(0.01));
            });
            if ui.button("Rescale").clicked() {
                let _ = app.dispatch(Command::RescaleWorldUnits { factor: app.rescale_factor });
            }
        }),
        Utility::Collapse => rollout(ui, "Collapse", true, |ui| {
            ui.add_enabled_ui(has, |ui| {
                if ui.button("Collapse Selected").clicked() {
                    let _ = app.dispatch(Command::ConvertToEditablePoly);
                }
            });
        }),
        Utility::AssignVertexColors => rollout(ui, "Assign Vertex Colors", true, |ui| assign_vertex_colors(ui, app, false)),
        Utility::ResetXForm => rollout(ui, "Reset Transform", true, |ui| {
            ui.add_enabled_ui(has, |ui| {
                if ui.button("Reset Selected").clicked() {
                    let _ = app.dispatch(Command::ResetXForm);
                }
            });
        }),
    }
}

/// What Measure shows: the locked nodes, or the selection.
fn measured(app: &mut LoomApp) -> serde_json::Value {
    let r = match app.measure_lock.clone() {
        Some(ids) => app.dispatch(Command::MeasureNodes { ids }),
        None => app.dispatch(Command::Measure),
    };
    r.unwrap_or_default()
}

/// The Objects, Shapes and Dimensions groups of Measure (panel and floater).
fn measure_groups(ui: &mut egui::Ui, r: &serde_json::Value) {
    let f = |v: &serde_json::Value| v.as_f64().unwrap_or(0.0);
    let xyz = |v: &serde_json::Value| format!("X: {:.3}  Y: {:.3}  Z: {:.3}", f(&v[0]), f(&v[1]), f(&v[2]));
    ui.group(|ui| {
        ui.label("Objects");
        ui.label(format!("Surface Area: {:.3}", f(&r["surface_area"])));
        // An asterisk: the mesh has holes, so the volume is not exact.
        let star = if r["volume_open"].as_bool().unwrap_or(false) { "*" } else { "" };
        ui.label(format!("Volume: {:.3}{star}", f(&r["volume"])));
        ui.label("Center of Mass:");
        ui.label(if r["center_of_mass"].is_null() { "-".to_owned() } else { xyz(&r["center_of_mass"]) });
    });
    ui.group(|ui| {
        ui.label("Shapes");
        ui.label(format!("Length: {:.3}", f(&r["shape_length"])));
    });
    ui.group(|ui| {
        ui.label("Dimensions");
        ui.label(if r["dimensions"].is_null() { "-".to_owned() } else { xyz(&r["dimensions"]) });
    });
}

/// Measure > New Floater: a modeless Measure window with the same groups.
pub fn measure_floater(ctx: &egui::Context, app: &mut LoomApp) {
    if !app.measure_floater {
        return;
    }
    let r = measured(app);
    let mut open = true;
    egui::Window::new("Measure").open(&mut open).resizable(true).default_width(240.0).show(ctx, |ui| {
        ui.label(r["name"].as_str().unwrap_or("").to_owned());
        measure_groups(ui, &r);
    });
    app.measure_floater = open;
}

/// Assign Vertex Colors: Light Model, Color Assignment and Shadows, then
/// Assign To Selected (the utility) or Assign (a VertexPaint entry's rollout).
pub fn assign_vertex_colors(ui: &mut egui::Ui, app: &mut LoomApp, update: bool) {
    let id = egui::Id::new("assign_vertex_colors");
    let (mut model, mut by_vertex, mut shadows): (u32, bool, bool) = ui.ctx().data(|d| d.get_temp(id)).unwrap_or((0, false, false));
    ui.label("Light Model:");
    for (i, l) in loom_commands::assign_vertex_colors::LIGHT_MODELS.iter().enumerate() {
        ui.radio_value(&mut model, i as u32, *l);
    }
    ui.label("Color Assignment:");
    ui.radio_value(&mut by_vertex, false, "Color by Face");
    ui.radio_value(&mut by_vertex, true, "Color by Vertex");
    // Lighting Only turns Shadows off, as in the reference.
    ui.add_enabled(model != 1, egui::Checkbox::new(&mut shadows, "Shadows"));
    ui.ctx().data_mut(|d| d.insert_temp(id, (model, by_vertex, shadows)));
    let label = if update { "Assign" } else { "Assign To Selected" };
    if ui.add_enabled(!app.state.selection.is_empty(), egui::Button::new(label)).clicked() {
        if let Err(e) = app.dispatch(Command::AssignVertexColors { light_model: model, by_vertex, shadows, update }) {
            app.state.status_prompt = e;
        }
    }
}
