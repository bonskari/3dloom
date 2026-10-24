//! Array dialog: Array Transformation (incremental / totals move, rotate,
//! scale), Type of Object, Array Dimensions with row offsets, Preview.

use crate::LoomApp;
use eframe::egui;
use loom_commands::{CloneKind, Command};

#[derive(Clone, Copy)]
struct Settings {
    mv: [f32; 3],
    rotate: [f32; 3],
    scale: [f32; 3],
    totals: [bool; 3],
    re_orient: bool,
    uniform: bool,
    kind: CloneKind,
    dims: u8,
    count: [u32; 3],
    row_2d: [f32; 3],
    row_3d: [f32; 3],
    preview: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { mv: [0.0; 3], rotate: [0.0; 3], scale: [100.0; 3], totals: [false; 3], re_orient: true, uniform: false, kind: CloneKind::Copy, dims: 1, count: [1, 1, 1], row_2d: [0.0; 3], row_3d: [0.0; 3], preview: false }
    }
}

impl Settings {
    fn command(&self) -> Command {
        let mut count = self.count;
        if self.dims < 2 {
            count[1] = 1;
        }
        if self.dims < 3 {
            count[2] = 1;
        }
        Command::ArrayObjects { mv: self.mv, rotate: self.rotate, scale: self.scale, totals: self.totals, re_orient: self.re_orient, uniform: self.uniform, kind: self.kind, count, row_2d: self.row_2d, row_3d: self.row_3d }
    }

    fn total(&self) -> u32 {
        let c = self.count;
        c[0] * if self.dims >= 2 { c[1] } else { 1 } * if self.dims >= 3 { c[2] } else { 1 }
    }
}

fn row(ui: &mut egui::Ui, label: &str, v: &mut [f32; 3], speed: f64, decimals: usize) -> bool {
    let mut changed = false;
    ui.label(label);
    for c in v.iter_mut() {
        changed |= ui.add(egui::DragValue::new(c).speed(speed).fixed_decimals(decimals)).changed();
    }
    changed
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let id = egui::Id::new("array_dialog");
    let mut st: Settings = ctx.data(|d| d.get_temp(id)).unwrap_or_default();
    let sources: Vec<u64> = ctx.data(|d| d.get_temp(id.with("sources"))).unwrap_or_else(|| app.state.selection.iter().map(|i| i.0).collect());
    let previewing = app.preview_dialog() == Some(loom_commands::Dialog::Array);
    let speed = app.spinner_speed(0.1);
    let sys = app.state.coord_system.label();
    let centre = app.state.center_mode.label();
    let mut changed = false;
    let mut done = false;
    let mut cancel = false;
    egui::Window::new("Array").open(open).resizable(false).show(ctx, |ui| {
        ui.label(egui::RichText::new(format!("Array Transformation: {sys} Coordinates ({centre})")).strong());
        egui::Grid::new("array_transform").num_columns(9).spacing([6.0, 4.0]).show(ui, |ui| {
            ui.label("");
            ui.label("Incremental");
            ui.label("X");
            ui.label("Y");
            ui.label("Z");
            ui.label("");
            ui.label("Totals");
            ui.end_row();
            let Settings { mv, rotate, scale, totals, .. } = &mut st;
            for (k, (name, v, dec)) in [("Move", mv, 3usize), ("Rotate", rotate, 1), ("Scale", scale, 1)].into_iter().enumerate() {
                let t = &mut totals[k];
                ui.label(name);
                if ui.selectable_label(!*t, "<").on_hover_text("Incremental").clicked() {
                    *t = false;
                    changed = true;
                }
                ui.add_enabled_ui(!*t, |ui| {
                    changed |= row(ui, "", v, speed, dec);
                });
                if ui.selectable_label(*t, ">").on_hover_text("Totals").clicked() {
                    *t = true;
                    changed = true;
                }
                ui.add_enabled_ui(*t, |ui| {
                    changed |= row(ui, "", v, speed, dec);
                });
                ui.end_row();
            }
        });
        ui.horizontal(|ui| {
            changed |= ui.checkbox(&mut st.re_orient, "Re-Orient").changed();
            changed |= ui.checkbox(&mut st.uniform, "Uniform").changed();
        });
        ui.horizontal_top(|ui| {
            ui.group(|ui| {
                ui.label(egui::RichText::new("Type of Object").strong());
                for (k, l) in [(CloneKind::Copy, "Copy"), (CloneKind::Instance, "Instance"), (CloneKind::Reference, "Reference")] {
                    changed |= ui.radio_value(&mut st.kind, k, l).changed();
                }
            });
            ui.group(|ui| {
                ui.label(egui::RichText::new("Array Dimensions").strong());
                egui::Grid::new("array_dims").num_columns(6).spacing([6.0, 4.0]).show(ui, |ui| {
                    ui.label("");
                    ui.label("Count");
                    ui.label(egui::RichText::new("Incremental Row Offsets").small());
                    ui.end_row();
                    for (d, label) in [(1u8, "1D"), (2, "2D"), (3, "3D")] {
                        changed |= ui.radio_value(&mut st.dims, d, label).changed();
                        ui.add_enabled_ui(st.dims >= d, |ui| {
                            changed |= ui.add(egui::DragValue::new(&mut st.count[d as usize - 1]).speed(0.1).range(1..=1000)).changed();
                        });
                        if d >= 2 {
                            ui.add_enabled_ui(st.dims >= d, |ui| {
                                let v = if d == 2 { &mut st.row_2d } else { &mut st.row_3d };
                                changed |= row(ui, "", v, speed, 3);
                            });
                        }
                        ui.end_row();
                    }
                });
                ui.label(format!("Total in Array: {}", st.total()));
            });
        });
        ui.horizontal(|ui| {
            if ui.button("Reset All Parameters").clicked() {
                st = Settings::default();
                changed = true;
            }
            if ui.selectable_label(st.preview, "Preview").clicked() {
                st.preview = !st.preview;
                changed = true;
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
        let _ = app.dispatch(Command::SelectNodes { ids: sources.clone(), op: loom_commands::SelectOp::Replace });
        let _ = app.dispatch(st.command());
    };
    if previewing && (changed || !st.preview) {
        app.preview_undo();
    }
    if st.preview && app.preview.is_none() && !done && !cancel && st.total() > 1 {
        app.preview_apply(loom_commands::Dialog::Array, |app| apply(app, &st));
    }
    if done {
        if app.preview_dialog() == Some(loom_commands::Dialog::Array) {
            app.preview_keep();
        } else {
            apply(app, &st);
        }
    }
    if cancel || (!*open && !done) {
        app.preview_undo();
        let _ = app.dispatch(Command::SelectNodes { ids: sources.clone(), op: loom_commands::SelectOp::Replace });
    }
    if done || cancel || !*open {
        *open = false;
        st.preview = false;
        ctx.data_mut(|d| d.remove::<Vec<u64>>(id.with("sources")));
    } else {
        ctx.data_mut(|d| d.insert_temp(id.with("sources"), sources));
    }
    ctx.data_mut(|d| d.insert_temp(id, st));
}
