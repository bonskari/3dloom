//! Transform Type-In dialog (F12, Edit menu, right-click on a transform
//! button): absolute and offset X/Y/Z for the active transform.

use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, CoordSystem, Tool};

fn system_label(app: &LoomApp, absolute: bool) -> &'static str {
    match app.state.coord_system {
        CoordSystem::View => "World",
        CoordSystem::Screen if absolute => "World",
        s => s.label(),
    }
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let tool = app.state.active_tool;
    let title = match tool {
        Tool::Rotate => "Rotate Transform Type-In",
        Tool::Scale => "Scale Transform Type-In",
        _ => "Move Transform Type-In",
    };
    let has = !app.state.selection.is_empty();
    let abs_label = match tool {
        Tool::Scale => "Absolute:Local".to_owned(),
        _ => format!("Absolute:{}", system_label(app, true)),
    };
    let off_label = format!("Offset:{}", system_label(app, false));
    let id = egui::Id::new("transform_type_in_offset");
    let abs_id = egui::Id::new("transform_type_in_absolute");
    let rest = if tool == Tool::Scale { [100.0; 3] } else { [0.0; 3] };
    let mut offset: [f32; 3] = ctx.data(|d| d.get_temp(id)).unwrap_or(rest);
    // While an absolute field is being typed into, keep the typed buffer
    // instead of re-reading the scene every frame.
    let mut absolute: [f32; 3] = ctx.data(|d| d.get_temp(abs_id)).unwrap_or_else(|| loom_commands::type_in_values(&app.state).to_array());
    let mut abs_focus = false;
    let window = egui::Window::new(title.clone()).id(egui::Id::new("transform_type_in")).resizable(false);
    crate::os_window::OsWindow::new("Transform Type-In", title, [380.0, 160.0]).fixed().show(ctx, open, window, |ui| {
        ui.add_enabled_ui(has, |ui| {
            ui.horizontal_top(|ui| {
                let abs = group(ui, &abs_label, &mut absolute, app, tool == Tool::Scale || tool == Tool::Rotate);
                let off = group(ui, &off_label, &mut offset, app, tool == Tool::Scale || tool == Tool::Rotate);
                abs_focus = abs.focused;
                if abs.committed {
                    let _ = app.dispatch(Command::TransformTypeIn { absolute: true, values: absolute });
                }
                if off.committed {
                    let _ = app.dispatch(Command::TransformTypeIn { absolute: false, values: offset });
                    offset = rest;
                }
            });
        });
    });
    ctx.data_mut(|d| {
        d.insert_temp(id, offset);
        if abs_focus {
            d.insert_temp(abs_id, absolute);
        } else {
            d.remove::<[f32; 3]>(abs_id);
        }
    });
}

pub struct FieldState {
    /// A field was committed (Enter, focus lost, spinner drag ended or right-click reset).
    pub committed: bool,
    /// A field is being edited.
    pub focused: bool,
}

/// One labelled X/Y/Z group.
fn group(ui: &mut egui::Ui, label: &str, v: &mut [f32; 3], app: &LoomApp, one_decimal: bool) -> FieldState {
    let mut st = FieldState { committed: false, focused: false };
    ui.group(|ui| {
        ui.label(egui::RichText::new(label).strong());
        for (c, l) in v.iter_mut().zip(["X:", "Y:", "Z:"]) {
            ui.horizontal(|ui| {
                ui.label(l);
                let r = ui.add(crate::spinner::spin(ui, c, ..).speed(app.spinner_speed(0.1)).fixed_decimals(if one_decimal { 1 } else { 3 }).min_decimals(1));
                st.committed |= r.lost_focus() || r.drag_stopped() || r.secondary_clicked();
                st.focused |= r.has_focus() || r.dragged();
            });
        }
    });
    st
}
