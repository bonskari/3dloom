//! Align Selection dialog (Alt+A after picking the target).

use crate::LoomApp;
use eframe::egui;
use loom_commands::{AlignPoint, Command, CoordSystem};

#[derive(Clone, Copy)]
struct Settings {
    axes: [bool; 3],
    current: AlignPoint,
    target: AlignPoint,
    orientation: [bool; 3],
    scale: [bool; 3],
}

impl Default for Settings {
    fn default() -> Self {
        // Every option starts off, as in the reference.
        Self { axes: [false; 3], current: AlignPoint::Center, target: AlignPoint::Center, orientation: [false; 3], scale: [false; 3] }
    }
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let Some(target) = app.state.align_target else {
        *open = false;
        return;
    };
    let id = egui::Id::new("align_dialog");
    let mut st: Settings = ctx.data(|d| d.get_temp(id)).unwrap_or_default();
    // The alignment previews live: every change is applied on top of an
    // undone previous preview.
    let previewing = app.preview_dialog() == Some(loom_commands::Dialog::Align);
    let kept_id = egui::Id::new("align_dialog_kept");
    // After Apply the result stays and nothing is previewed until a change.
    let kept: bool = ctx.data(|d| d.get_temp(kept_id)).unwrap_or(false);
    let name = app.state.scene.node(target).map(|n| n.name.clone()).unwrap_or_default();
    let sys = match app.state.coord_system {
        CoordSystem::View => "World",
        s => s.label(),
    };
    let mut changed = false;
    let mut done = false;
    let mut cancel = false;
    egui::Window::new(format!("Align Selection ({name})")).open(open).resizable(false).show(ctx, |ui| {
        ui.label(egui::RichText::new(format!("Align Position ({sys}):")).strong());
        ui.horizontal(|ui| {
            for (k, l) in ["X Position", "Y Position", "Z Position"].iter().enumerate() {
                changed |= ui.checkbox(&mut st.axes[k], *l).changed();
            }
        });
        ui.horizontal_top(|ui| {
            ui.group(|ui| {
                ui.label("Current Object:");
                for p in AlignPoint::ALL {
                    changed |= ui.radio_value(&mut st.current, p, p.label()).changed();
                }
            });
            ui.group(|ui| {
                ui.label("Target Object:");
                for p in AlignPoint::ALL {
                    changed |= ui.radio_value(&mut st.target, p, p.label()).changed();
                }
            });
        });
        ui.label(egui::RichText::new("Align Orientation (Local):").strong());
        ui.horizontal(|ui| {
            for (k, l) in ["X Axis", "Y Axis", "Z Axis"].iter().enumerate() {
                changed |= ui.checkbox(&mut st.orientation[k], *l).changed();
            }
        });
        ui.label(egui::RichText::new("Match Scale:").strong());
        ui.horizontal(|ui| {
            for (k, l) in ["X Axis", "Y Axis", "Z Axis"].iter().enumerate() {
                changed |= ui.checkbox(&mut st.scale[k], *l).changed();
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Apply").clicked() {
                // Keep the current result; the next change previews on top of it.
                app.preview_keep();
                ctx.data_mut(|d| d.insert_temp(kept_id, true));
            }
            if ui.button("OK").clicked() {
                done = true;
            }
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
        });
    });
    let apply = |app: &mut LoomApp, st: &Settings| {
        let _ = app.dispatch(Command::AlignObjects {
            target: target.0,
            x: st.axes[0],
            y: st.axes[1],
            z: st.axes[2],
            current: st.current,
            target_pt: st.target,
            orientation: st.orientation,
            match_scale: st.scale,
        });
    };
    let kept_now: bool = ctx.data(|d| d.get_temp(kept_id)).unwrap_or(kept);
    if changed {
        app.preview_undo();
        app.preview_apply(loom_commands::Dialog::Align, |app| apply(app, &st));
        ctx.data_mut(|d| d.insert_temp(kept_id, false));
    } else if !previewing && !kept_now && !done && !cancel {
        app.preview_apply(loom_commands::Dialog::Align, |app| apply(app, &st));
    }
    if done {
        app.preview_keep();
    }
    if cancel || (!*open && !done) {
        app.preview_undo();
    }
    if done || cancel || !*open {
        *open = false;
        app.state.align_target = None;
        ctx.data_mut(|d| d.remove::<bool>(kept_id));
    }
    ctx.data_mut(|d| d.insert_temp(id, st));
}
