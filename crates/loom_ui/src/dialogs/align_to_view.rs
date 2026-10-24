//! Align to View dialog: which local axis points at the viewer, with Flip.

use crate::LoomApp;
use eframe::egui;
use loom_commands::{Axis, Command};

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let axis_id = egui::Id::new("align_to_view_axis");
    let flip_id = egui::Id::new("align_to_view_flip");
    let mut axis: Axis = ctx.data(|d| d.get_temp(axis_id)).unwrap_or(Axis::Z);
    let mut flip: bool = ctx.data(|d| d.get_temp(flip_id)).unwrap_or(false);
    let applied = app.preview_dialog() == Some(loom_commands::Dialog::AlignToView);
    let mut changed = false;
    let mut done = false;
    let mut cancel = false;
    egui::Window::new("Align to View").open(open).resizable(false).show(ctx, |ui| {
        for (a, l) in [(Axis::X, "Align X"), (Axis::Y, "Align Y"), (Axis::Z, "Align Z")] {
            changed |= ui.radio_value(&mut axis, a, l).changed();
        }
        changed |= ui.checkbox(&mut flip, "Flip").changed();
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                done = true;
            }
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
        });
    });
    if changed {
        app.preview_undo();
        app.preview_apply(loom_commands::Dialog::AlignToView, |app| {
            let _ = app.dispatch(Command::AlignToViewAxis { axis, flip });
        });
    } else if !applied && !done && !cancel {
        app.preview_apply(loom_commands::Dialog::AlignToView, |app| {
            let _ = app.dispatch(Command::AlignToViewAxis { axis, flip });
        });
    }
    if done {
        app.preview_keep();
    }
    if cancel || (!*open && !done) {
        app.preview_undo();
    }
    if done || cancel || !*open {
        *open = false;
    }
    ctx.data_mut(|d| {
        d.insert_temp(axis_id, axis);
        d.insert_temp(flip_id, flip);
    });
}
