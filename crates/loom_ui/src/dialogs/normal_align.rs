//! Normal Align dialog: position offset, rotation offset and Flip Normal
//! applied live on top of the two surface picks.

use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;

#[derive(Clone, Copy, Default)]
struct Settings {
    offset: [f32; 3],
    angle: f32,
    flip: bool,
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let Some(picks) = app.state.normal_align else {
        *open = false;
        return;
    };
    let id = egui::Id::new("normal_align_dialog");
    let mut st: Settings = ctx.data(|d| d.get_temp(id)).unwrap_or_default();
    // The second surface pick already applied the alignment.
    if app.preview_dialog() != Some(loom_commands::Dialog::NormalAlign) {
        app.preview_mark(loom_commands::Dialog::NormalAlign);
    }
    let speed = app.spinner_speed(0.1);
    let mut changed = false;
    let mut done = false;
    let mut cancel = false;
    egui::Window::new("Normal Align").open(open).resizable(false).show(ctx, |ui| {
        ui.label(egui::RichText::new("Position Offset:").strong());
        ui.horizontal(|ui| {
            for (c, l) in st.offset.iter_mut().zip(["X:", "Y:", "Z:"]) {
                ui.label(l);
                let r = ui.add(crate::spinner::spin(ui, c, ..).speed(speed).fixed_decimals(3));
                changed |= r.changed();
            }
        });
        ui.label(egui::RichText::new("Rotation Offset:").strong());
        ui.horizontal(|ui| {
            ui.label("Angle:");
            changed |= ui.add(crate::spinner::spin(ui, &mut st.angle, ..).speed(speed).fixed_decimals(1).suffix("°")).changed();
            changed |= ui.checkbox(&mut st.flip, "Flip Normal").changed();
        });
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                done = true;
            }
            if ui.button("Cancel Align").clicked() {
                cancel = true;
            }
        });
    });
    if changed {
        app.preview_undo();
        app.preview_apply(loom_commands::Dialog::NormalAlign, |app| {
            let _ = app.dispatch(Command::NormalAlignFaces {
                source_point: picks.source_point.to_array(),
                source_normal: picks.source_normal.to_array(),
                target_point: picks.target_point.to_array(),
                target_normal: picks.target_normal.to_array(),
                offset: st.offset,
                angle_deg: st.angle,
                flip: st.flip,
            });
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
        app.state.normal_align = None;
        ctx.data_mut(|d| d.remove::<Settings>(id));
    } else {
        ctx.data_mut(|d| d.insert_temp(id, st));
    }
}
