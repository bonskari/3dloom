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
    /// Display as Box: the preview members show as bounding boxes.
    display_as_box: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { mv: [0.0; 3], rotate: [0.0; 3], scale: [100.0; 3], totals: [false; 3], re_orient: true, uniform: false, kind: CloneKind::Copy, dims: 1, count: [1, 1, 1], row_2d: [0.0; 3], row_3d: [0.0; 3], preview: false, display_as_box: false }
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

    /// The unavailable side of transform `k` (0 Move, 1 Rotate, 2 Scale):
    /// the value equivalent to the side being set, for 1D Count members.
    fn equivalent(&self, k: usize, v: [f32; 3]) -> [f32; 3] {
        let n = self.count[0].max(1) as f32;
        let to_totals = self.totals[k];
        v.map(|x| match (k, to_totals) {
            // Totals divide over the count (see ArrayObjects).
            (2, false) => 100.0 * (x / 100.0).max(0.0).powf(n),
            (2, true) => 100.0 * (x / 100.0).max(0.0).powf(1.0 / n),
            (_, false) => x * n,
            (_, true) => x / n,
        })
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
        changed |= ui.add(crate::spinner::spin(ui, c, ..).speed(speed).fixed_decimals(decimals)).changed();
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
            for (k, (name, dec)) in [("Move", 3usize), ("Rotate", 1), ("Scale", 1)].into_iter().enumerate() {
                let t = st.totals[k];
                // The side not being set shows the equivalent value.
                let mut other = st.equivalent(k, [st.mv, st.rotate, st.scale][k]);
                ui.label(name);
                let mut switch = None;
                if ui.selectable_label(!t, "<").on_hover_text("Incremental").clicked() && t {
                    switch = Some(false);
                }
                let v = match k {
                    0 => &mut st.mv,
                    1 => &mut st.rotate,
                    _ => &mut st.scale,
                };
                ui.add_enabled_ui(!t, |ui| {
                    changed |= row(ui, "", if t { &mut other } else { v }, speed, dec) && !t;
                });
                if ui.selectable_label(t, ">").on_hover_text("Totals").clicked() && !t {
                    switch = Some(true);
                }
                ui.add_enabled_ui(t, |ui| {
                    changed |= row(ui, "", if t { v } else { &mut other }, speed, dec) && t;
                });
                if let Some(to) = switch {
                    // Switching sides keeps the array: the value becomes its equivalent.
                    let eq = st.equivalent(k, [st.mv, st.rotate, st.scale][k]);
                    match k {
                        0 => st.mv = eq,
                        1 => st.rotate = eq,
                        _ => st.scale = eq,
                    }
                    st.totals[k] = to;
                    changed = true;
                }
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
                            changed |= ui.add(crate::spinner::spin(ui, &mut st.count[d as usize - 1], 1..=1000).speed(0.1)).changed();
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
            changed |= ui.checkbox(&mut st.display_as_box, "Display as Box").changed();
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
    // The preview: the array, with Display as Box its new members as boxes (one undo entry).
    let preview = |app: &mut LoomApp, st: &Settings| {
        if !st.display_as_box {
            apply(app, st);
            return;
        }
        let before: std::collections::BTreeSet<loom_scene::NodeId> = app.state.scene.nodes().map(|n| n.id).collect();
        app.state.undo.begin("Array");
        apply(app, st);
        let new: Vec<loom_scene::NodeId> = app.state.scene.nodes().map(|n| n.id).filter(|n| !before.contains(n)).collect();
        for n in new {
            let _ = app.dispatch(Command::SetDisplayProperty { id: n.0, name: "Display as Box".into(), value: true });
        }
        app.state.undo.end();
    };
    if previewing && (changed || !st.preview) {
        app.preview_undo();
    }
    if st.preview && app.preview.is_none() && !done && !cancel && st.total() > 1 {
        app.preview_apply(loom_commands::Dialog::Array, |app| preview(app, &st));
    }
    if done {
        if app.preview_dialog() == Some(loom_commands::Dialog::Array) && !st.display_as_box {
            app.preview_keep();
        } else {
            // Boxes are only for the preview: the real array is made afresh.
            app.preview_undo();
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
