//! Mirror dialog: axis, offset and clone selection, previewed live.

use crate::LoomApp;
use eframe::egui;
use loom_commands::{CloneKind, Command, CoordSystem, MirrorAxis};

#[derive(Clone, Copy)]
struct Settings {
    axis: MirrorAxis,
    offset: f32,
    clone: Option<CloneKind>,
}

impl Default for Settings {
    fn default() -> Self {
        Self { axis: MirrorAxis::X, offset: 0.0, clone: None }
    }
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let id = egui::Id::new("mirror_dialog");
    let sources_id = egui::Id::new("mirror_dialog_sources");
    let mut st: Settings = ctx.data(|d| d.get_temp(id)).unwrap_or_default();
    let applied = app.preview_dialog() == Some(loom_commands::Dialog::Mirror);
    let sources: Vec<u64> = ctx.data(|d| d.get_temp(sources_id)).unwrap_or_else(|| app.state.selection.iter().map(|i| i.0).collect());
    let sys = match app.state.coord_system {
        CoordSystem::View => "Screen".to_owned(),
        s => s.label().to_owned(),
    };
    let speed = app.spinner_speed(0.1);
    let mut changed = false;
    let mut done = false;
    let mut cancel = false;
    egui::Window::new(format!("Mirror: {sys} Coordinates")).open(open).resizable(false).show(ctx, |ui| {
        ui.horizontal_top(|ui| {
            ui.group(|ui| {
                ui.label(egui::RichText::new("Mirror Axis:").strong());
                for a in MirrorAxis::ALL {
                    changed |= ui.radio_value(&mut st.axis, a, a.label()).changed();
                }
                ui.horizontal(|ui| {
                    ui.label("Offset:");
                    changed |= ui.add(egui::DragValue::new(&mut st.offset).speed(speed).fixed_decimals(3)).changed();
                });
            });
            ui.group(|ui| {
                ui.label(egui::RichText::new("Clone Selection:").strong());
                for (c, l) in [(None, "No Clone"), (Some(CloneKind::Copy), "Copy"), (Some(CloneKind::Instance), "Instance"), (Some(CloneKind::Reference), "Reference")] {
                    changed |= ui.radio_value(&mut st.clone, c, l).changed();
                }
            });
        });
        ui.add_enabled(false, egui::Checkbox::new(&mut false, "Mirror IK Limits"));
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                done = true;
            }
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
        });
    });
    let apply = |app: &mut LoomApp, st: &Settings| {
        let _ = app.dispatch(Command::SelectNodes { ids: sources.clone(), op: loom_commands::SelectOp::Replace });
        let _ = app.dispatch(Command::MirrorObjects { axis: st.axis, offset: st.offset, clone: st.clone });
    };
    if changed {
        app.preview_undo();
        app.preview_apply(loom_commands::Dialog::Mirror, |app| apply(app, &st));
    } else if !applied && !done && !cancel {
        app.preview_apply(loom_commands::Dialog::Mirror, |app| apply(app, &st));
        ctx.data_mut(|d| d.insert_temp(sources_id, sources.clone()));
    }
    if done {
        app.preview_keep();
    }
    if cancel || (!*open && !done) {
        app.preview_undo();
        let _ = app.dispatch(Command::SelectNodes { ids: sources.clone(), op: loom_commands::SelectOp::Replace });
    }
    if done || cancel || !*open {
        *open = false;
        ctx.data_mut(|d| d.remove::<Vec<u64>>(sources_id));
    }
    ctx.data_mut(|d| d.insert_temp(id, st));
}
