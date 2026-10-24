//! Preference Settings dialog: General, Viewports and Gizmos panels (the
//! settings this build reads).

use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    General,
    Viewports,
    Gizmos,
    Files,
}

fn set(app: &mut LoomApp, name: &str, value: f64) {
    let _ = app.dispatch(Command::SetPreference { name: name.to_owned(), value });
}

fn check(ui: &mut egui::Ui, app: &mut LoomApp, name: &str, label: &str) {
    let mut on = app.state.prefs.get(name).unwrap_or(0.0) != 0.0;
    if ui.checkbox(&mut on, label).changed() {
        set(app, name, if on { 1.0 } else { 0.0 });
    }
}

fn number(ui: &mut egui::Ui, app: &mut LoomApp, name: &str, label: &str, range: std::ops::RangeInclusive<f64>, decimals: usize) {
    let mut v = app.state.prefs.get(name).unwrap_or(0.0);
    ui.label(label);
    if ui.add(egui::DragValue::new(&mut v).speed(app.spinner_speed(0.1)).range(range).fixed_decimals(decimals)).changed() {
        set(app, name, v);
    }
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let tab_id = egui::Id::new("preferences_tab");
    let mut tab: Tab = ctx.data(|d| d.get_temp(tab_id)).unwrap_or_default();
    egui::Window::new("Preference Settings").open(open).resizable(false).default_width(560.0).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut tab, Tab::General, "General");
            ui.selectable_value(&mut tab, Tab::Viewports, "Viewports");
            ui.selectable_value(&mut tab, Tab::Gizmos, "Gizmos");
            ui.selectable_value(&mut tab, Tab::Files, "Files");
        });
        ui.separator();
        match tab {
            Tab::General => {
                ui.vertical(|ui| {
                    ui.group(|ui| {
                        ui.label(egui::RichText::new("Scene Undo").strong());
                        ui.horizontal(|ui| number(ui, app, "Scene Undo Levels", "Levels:", 1.0..=500.0, 0));
                        ui.label(egui::RichText::new("Ref. Coord. System").strong());
                        check(ui, app, "Reference Coordinate System Constant", "Constant");
                        ui.label(egui::RichText::new("Scene Selection").strong());
                        check(ui, app, "Auto Window/Crossing by Direction", "Auto Window/Crossing by Direction");
                        ui.horizontal(|ui| number(ui, app, "Paint Selection Brush Size", "Paint Selection Brush Size:", 1.0..=200.0, 0));
                        check(ui, app, "Space Locks Selection", "Space locks the selection");
                    });
                    ui.group(|ui| {
                        ui.label(egui::RichText::new("Spinners").strong());
                        ui.horizontal(|ui| number(ui, app, "Spinner Precision", "Precision:", 0.0..=10.0, 0));
                        ui.horizontal(|ui| number(ui, app, "Spinner Snap", "Snap:", 0.001..=1000.0, 3));
                        check(ui, app, "Spinner Use Snap", "Use Snap");
                        check(ui, app, "Spinner Wrap Cursor", "Wrap Cursor near Spinner");
                    });
                });
            }
            Tab::Files => {
                ui.group(|ui| {
                    ui.label(egui::RichText::new("Auto Backup").strong());
                    check(ui, app, "Auto Backup Enable", "Enable");
                    ui.horizontal(|ui| number(ui, app, "Auto Backup Count", "Number of Autobak files:", 1.0..=99.0, 0));
                    ui.horizontal(|ui| number(ui, app, "Auto Backup Interval", "Backup Interval (minutes):", 0.1..=1440.0, 1));
                    ui.horizontal(|ui| {
                        ui.label("Auto Backup File Name:");
                        let id = egui::Id::new("auto_backup_name_edit");
                        let mut name: String = ui.data(|d| d.get_temp(id)).unwrap_or_else(|| app.state.prefs.auto_backup_name.clone());
                        let r = ui.text_edit_singleline(&mut name);
                        if r.lost_focus() {
                            let _ = app.dispatch(Command::SetAutoBackupName { name: name.clone() });
                            ui.data_mut(|d| d.remove::<String>(id));
                        } else {
                            ui.data_mut(|d| d.insert_temp(id, name));
                        }
                    });
                    ui.label(format!("Folder: {}", loom_commands::autosave::autoback_dir().display()));
                });
            }
            Tab::Viewports => {
                ui.group(|ui| {
                    ui.label(egui::RichText::new("Viewport Parameters").strong());
                    check(ui, app, "Viewport Tooltips", "Viewport tooltips");
                    check(ui, app, "Show Cross Hair Cursor", "Show cross hair cursor");
                });
            }
            Tab::Gizmos => {
                ui.group(|ui| {
                    ui.label(egui::RichText::new("Transform Gizmos").strong());
                    ui.horizontal(|ui| {
                        check(ui, app, "Transform Gizmos", "On");
                        check(ui, app, "Show Axis Labels", "Show Axis Labels");
                        check(ui, app, "Allow Multiple Gizmos", "Allow Multiple Gizmos");
                        number(ui, app, "Gizmo Size", "Size:", 1.0..=100.0, 0);
                    });
                });
                ui.horizontal_top(|ui| {
                    ui.group(|ui| {
                        ui.label(egui::RichText::new("Move Gizmo").strong());
                        ui.label("Plane Handles");
                        ui.horizontal(|ui| number(ui, app, "Move Plane Handle Size", "Size:", 1.0..=100.0, 0));
                        ui.horizontal(|ui| number(ui, app, "Move Plane Handle Offset", "Offset:", 0.0..=100.0, 0));
                        check(ui, app, "Center Box Handle", "Center Box Handle: Move in Screen Space");
                    });
                    ui.group(|ui| {
                        ui.label(egui::RichText::new("Rotate Gizmo").strong());
                        check(ui, app, "Rotate Free Rotation", "Free Rotation");
                        check(ui, app, "Rotate Show Tripod", "Show Tripod");
                        check(ui, app, "Rotate Screen Handle", "Screen Handle");
                        check(ui, app, "Rotate Show Pie Slice", "Show Pie Slice");
                        check(ui, app, "Rotate Angle Data", "Angle Data");
                    });
                    ui.group(|ui| {
                        ui.label(egui::RichText::new("Scale Gizmo").strong());
                        ui.horizontal(|ui| number(ui, app, "Scale Size", "Size (%):", 10.0..=400.0, 0));
                        ui.label(egui::RichText::new("Rotation Increment").strong());
                        ui.horizontal(|ui| number(ui, app, "Rotation Increment", "Degrees:", 0.0..=180.0, 1));
                        ui.horizontal(|ui| number(ui, app, "Viewport Orbit Snap Angle", "Orbit Snap Angle:", 0.0..=90.0, 1));
                    });
                });
            }
        }
        if ui.button("OK").clicked() {
            let _ = app.dispatch(Command::CloseDialog);
        }
    });
    ctx.data_mut(|d| d.insert_temp(tab_id, tab));
}
