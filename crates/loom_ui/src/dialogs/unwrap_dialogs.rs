//! Unwrap UVW dialogs: Unwrap Options, Stitch / Relax / Pack / Flatten /
//! Normal / Unfold / Pelt tools and Render UVW Template. Every control is
//! a `Set Unwrap Option` and every button a command.

use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, Dialog};

fn set(app: &mut LoomApp, name: &str, value: f64) {
    let _ = app.dispatch(Command::SetUnwrapOption { name: name.to_owned(), value });
}

fn check(ui: &mut egui::Ui, app: &mut LoomApp, name: &str, label: &str, on: bool) {
    let mut v = on;
    if ui.checkbox(&mut v, label).changed() {
        set(app, name, if v { 1.0 } else { 0.0 });
    }
}

fn number(ui: &mut egui::Ui, app: &mut LoomApp, name: &str, label: &str, value: f64, range: std::ops::RangeInclusive<f64>, decimals: usize) {
    let mut v = value;
    ui.label(label);
    if ui.add(crate::spinner::spin(ui, &mut v, range).speed(app.spinner_speed(0.05)).fixed_decimals(decimals)).changed() {
        set(app, name, v);
    }
}

fn radios(ui: &mut egui::Ui, app: &mut LoomApp, name: &str, value: u32, labels: &[&str]) {
    ui.horizontal_wrapped(|ui| {
        for (i, l) in labels.iter().enumerate() {
            if ui.radio(value == i as u32, *l).clicked() && value != i as u32 {
                set(app, name, i as f64);
            }
        }
    });
}

/// OK runs `apply`, Apply runs it and keeps the dialog, Cancel closes.
fn buttons(ui: &mut egui::Ui, app: &mut LoomApp, apply: Option<Command>, done: &mut bool) {
    ui.horizontal(|ui| {
        if let Some(cmd) = apply.clone() {
            if ui.button("OK").clicked() {
                let _ = app.dispatch(cmd);
                *done = true;
            }
            if ui.button("Apply").clicked() {
                let _ = app.dispatch(apply.clone().expect("checked"));
            }
        } else if ui.button("OK").clicked() {
            *done = true;
        }
        if ui.button("Cancel").clicked() {
            *done = true;
        }
    });
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp, dialog: Dialog, open: &mut bool) {
    let e = app.state.uv_editor.clone();
    let mut done = false;
    match dialog {
        Dialog::UnwrapOptions => {
            egui::Window::new("Unwrap Options").open(open).resizable(false).show(ctx, |ui| {
                ui.label(egui::RichText::new("Display Preferences").strong());
                egui::Grid::new("unwrap_opts_display").num_columns(2).show(ui, |ui| {
                    number(ui, app, "Checker Tiling", "Checker Tiling:", e.checker_tiling as f64, 0.01..=100.0, 2);
                    ui.end_row();
                    number(ui, app, "Tiles", "Tiles:", e.tiles as f64, 1.0..=10.0, 0);
                    ui.end_row();
                    number(ui, app, "Tile Brightness", "Tile Brightness:", e.tile_brightness as f64, 0.0..=1.0, 2);
                    ui.end_row();
                });
                check(ui, app, "Tile Bitmap", "Tile Bitmap", e.tile_bitmap);
                ui.label(egui::RichText::new("Bitmap Options").strong());
                egui::Grid::new("unwrap_opts_bitmap").num_columns(2).show(ui, |ui| {
                    number(ui, app, "Bitmap Width", "Width:", e.bitmap_size[0] as f64, 16.0..=8192.0, 0);
                    ui.end_row();
                    number(ui, app, "Bitmap Height", "Height:", e.bitmap_size[1] as f64, 16.0..=8192.0, 0);
                    ui.end_row();
                });
                ui.label(egui::RichText::new("Advanced Options").strong());
                check(ui, app, "Constant Update", "Constant Update", e.constant_update);
                egui::Grid::new("unwrap_opts_adv").num_columns(2).show(ui, |ui| {
                    number(ui, app, "Weld Threshold", "Weld Threshold:", e.weld_threshold as f64, 0.0..=10.0, 3);
                    ui.end_row();
                    number(ui, app, "Grid Size", "Grid Size:", e.grid_size as f64, 0.001..=10.0, 3);
                    ui.end_row();
                });
                ui.horizontal(|ui| {
                    if ui.button("Load Defaults").clicked() {
                        let _ = app.dispatch(Command::LoadDefaults);
                    }
                    if ui.button("Save Current Settings As Default").clicked() {
                        let _ = app.dispatch(Command::SaveCurrentSettingsAsDefault);
                    }
                });
                buttons(ui, app, None, &mut done);
            });
        }
        Dialog::StitchTool => {
            egui::Window::new("Stitch Tool").open(open).resizable(false).show(ctx, |ui| {
                check(ui, app, "Stitch Average", "Align Clusters (average)", e.stitch_average);
                ui.horizontal(|ui| number(ui, app, "Stitch Bias", "Bias:", e.stitch_bias as f64, 0.0..=1.0, 2));
                buttons(ui, app, Some(Command::Stitch), &mut done);
            });
        }
        Dialog::RelaxTool => {
            egui::Window::new("Relax Tool").open(open).resizable(false).show(ctx, |ui| {
                radios(ui, app, "Relax By", e.relax_by, &["Relax By Face Angles", "Relax By Edge Angles", "Relax By Centers"]);
                egui::Grid::new("relax_grid").num_columns(2).show(ui, |ui| {
                    number(ui, app, "Relax Iterations", "Iterations:", e.relax_iterations as f64, 1.0..=10000.0, 0);
                    ui.end_row();
                    number(ui, app, "Relax Amount", "Amount:", e.relax_amount as f64, 0.0..=1.0, 2);
                    ui.end_row();
                    number(ui, app, "Relax Stretch", "Stretch:", e.relax_stretch as f64, 0.0..=1.0, 2);
                    ui.end_row();
                });
                check(ui, app, "Relax Keep Boundary Points Fixed", "Keep Boundary Points Fixed", e.relax_keep_boundary);
                buttons(ui, app, Some(Command::RelaxUnwrap), &mut done);
            });
        }
        Dialog::PackUvs => {
            egui::Window::new("Pack UVs").open(open).resizable(false).show(ctx, |ui| {
                radios(ui, app, "Pack Method", e.pack_method, &["Linear Packing", "Recursive Packing"]);
                ui.horizontal(|ui| number(ui, app, "Pack Spacing", "Spacing:", e.pack_spacing as f64, 0.0..=1.0, 3));
                check(ui, app, "Pack Normalize", "Normalize Clusters", e.pack_normalize);
                check(ui, app, "Pack Rotate Clusters", "Rotate Clusters", e.pack_rotate);
                check(ui, app, "Pack Fill Holes", "Fill Holes", e.pack_fill_holes);
                buttons(ui, app, Some(Command::Pack), &mut done);
            });
        }
        Dialog::FlattenMapping => {
            egui::Window::new("Flatten Mapping").open(open).resizable(false).show(ctx, |ui| {
                egui::Grid::new("flatten_grid").num_columns(2).show(ui, |ui| {
                    number(ui, app, "Flatten Angle", "Face Angle Threshold:", e.flatten_angle as f64, 0.0..=180.0, 1);
                    ui.end_row();
                    number(ui, app, "Flatten Spacing", "Spacing:", e.flatten_spacing as f64, 0.0..=1.0, 3);
                    ui.end_row();
                });
                check(ui, app, "Flatten Normalize", "Normalize Clusters", e.flatten_normalize);
                check(ui, app, "Flatten Rotate Clusters", "Rotate Clusters", e.flatten_rotate);
                check(ui, app, "Flatten Fill Holes", "Fill Holes", e.flatten_fill_holes);
                radios(ui, app, "Flatten By", e.flatten_by, &["By Face Angle", "By Smoothing Group", "By Material ID"]);
                buttons(ui, app, Some(Command::FlattenMap), &mut done);
            });
        }
        Dialog::NormalMapping => {
            egui::Window::new("Normal Mapping").open(open).resizable(false).show(ctx, |ui| {
                radios(ui, app, "Normal Mapping", e.normal_kind, &["Back/Front Mapping", "Left/Right Mapping", "Top/Bottom Mapping", "Box Mapping", "Diamond Mapping"]);
                ui.horizontal(|ui| number(ui, app, "Normal Spacing", "Spacing:", e.normal_spacing as f64, 0.0..=1.0, 3));
                check(ui, app, "Normal Normalize", "Normalize Clusters", e.normal_normalize);
                check(ui, app, "Normal Rotate Clusters", "Rotate Clusters", e.normal_rotate);
                check(ui, app, "Normal Align By Width", "Align By Width", e.normal_align_width);
                buttons(ui, app, Some(Command::NormalMap), &mut done);
            });
        }
        Dialog::UnfoldMapping => {
            egui::Window::new("Unfold Mapping").open(open).resizable(false).show(ctx, |ui| {
                radios(ui, app, "Unfold Walk", e.unfold_walk, &["Walk to closest face", "Walk to farthest face"]);
                check(ui, app, "Unfold Normalize", "Normalize Clusters", e.unfold_normalize);
                buttons(ui, app, Some(Command::UnfoldMap), &mut done);
            });
        }
        Dialog::PeltMap => {
            egui::Window::new("Pelt Map").open(open).resizable(false).show(ctx, |ui| {
                ui.label(egui::RichText::new("Pelt Map Parameters").strong());
                ui.horizontal(|ui| number(ui, app, "Pelt Iterations", "Iterations:", e.pelt_iterations as f64, 1.0..=10000.0, 0));
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Simulate Pelt Pulling").clicked() {
                        let _ = app.dispatch(Command::PeltDialogRunSimulation);
                    }
                    if ui.button("Reset").clicked() {
                        let _ = app.dispatch(Command::PeltDialogResetStretcher);
                    }
                    if ui.button("Relax (Light)").clicked() {
                        let _ = app.dispatch(Command::PeltDialogRelaxSimulationLight);
                    }
                    if ui.button("Relax (Heavy)").clicked() {
                        let _ = app.dispatch(Command::PeltDialogRelaxSimulationHeavy);
                    }
                    if ui.button("Select Pelt UVs").clicked() {
                        let _ = app.dispatch(Command::PeltDialogSelectPeltUvs);
                    }
                });
                buttons(ui, app, Some(Command::PeltMap), &mut done);
            });
        }
        Dialog::RenderUvwTemplate => {
            let id = egui::Id::new("render_template_path");
            let mut path: String = ctx.data(|d| d.get_temp(id)).unwrap_or_else(|| {
                let base = app.state.file.as_ref().and_then(|f| f.parent()).map(|p| p.to_path_buf()).unwrap_or_else(|| app.file_dir.clone());
                base.join("uvw_template.png").to_string_lossy().into_owned()
            });
            egui::Window::new("Render UVs").open(open).resizable(false).show(ctx, |ui| {
                egui::Grid::new("template_grid").num_columns(2).show(ui, |ui| {
                    number(ui, app, "Template Width", "Width:", e.template_width as f64, 16.0..=8192.0, 0);
                    ui.end_row();
                    number(ui, app, "Template Height", "Height:", e.template_height as f64, 16.0..=8192.0, 0);
                    ui.end_row();
                });
                radios(ui, app, "Template Mode", e.template_mode, &["Mode: Wire", "Mode: Solid"]);
                ui.horizontal(|ui| {
                    ui.label("File:");
                    ui.text_edit_singleline(&mut path);
                });
                ui.horizontal(|ui| {
                    if ui.button("Render UV Template").clicked() {
                        let mode = if e.template_mode == 1 { "solid" } else { "wire" };
                        let _ = app.dispatch(Command::RenderUvwTemplate { path: path.clone(), width: e.template_width, height: e.template_height, mode: mode.into() });
                        done = true;
                    }
                    if ui.button("Cancel").clicked() {
                        done = true;
                    }
                });
            });
            ctx.data_mut(|d| d.insert_temp(id, path));
        }
        _ => {}
    }
    if done {
        *open = false;
    }
}
