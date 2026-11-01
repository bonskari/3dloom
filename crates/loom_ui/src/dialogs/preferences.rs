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
    Rendering,
    Animation,
    InverseKinematics,
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
    if ui.add(crate::spinner::spin(ui, &mut v, range).speed(app.spinner_speed(0.1)).fixed_decimals(decimals)).changed() {
        set(app, name, v);
    }
}

/// The next time the dialog shows, it is on the General tab (Spinner Snap right-click).
pub fn open_on_general(ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("preferences_tab"), Tab::General));
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let tab_id = egui::Id::new("preferences_tab");
    let mut tab: Tab = ctx.data(|d| d.get_temp(tab_id)).unwrap_or_default();
    // The values when the dialog opened: Cancel puts them back.
    let snapshot_id = egui::Id::new("preferences_snapshot");
    let snapshot: loom_commands::Prefs = ctx.data_mut(|d| d.get_temp_mut_or_insert_with(snapshot_id, || app.state.prefs.clone()).clone());
    let mut done = false;
    egui::Window::new("Preference Settings").open(open).resizable(false).default_width(560.0).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut tab, Tab::General, "General");
            ui.selectable_value(&mut tab, Tab::Viewports, "Viewports");
            ui.selectable_value(&mut tab, Tab::Gizmos, "Gizmos");
            ui.selectable_value(&mut tab, Tab::Files, "Files");
            ui.selectable_value(&mut tab, Tab::Rendering, "Rendering");
            ui.selectable_value(&mut tab, Tab::Animation, "Animation");
            ui.selectable_value(&mut tab, Tab::InverseKinematics, "Inverse Kinematics");
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
                        ui.label(egui::RichText::new("Create Objects").strong());
                        check(ui, app, "Keep Creating", "Keep Creating");
                        ui.label(egui::RichText::new("Sub-Materials").strong());
                        check(ui, app, "Sub-Materials Assign Automatically", "Assign Automatically");
                        ui.label(egui::RichText::new("Command Panel").strong());
                        ui.horizontal(|ui| number(ui, app, "Rollout Threshold", "Rollout Threshold:", 0.0..=2000.0, 0));
                        ui.label(egui::RichText::new("Default Ambient Light Color").strong());
                        {
                            let a = app.state.prefs.default_ambient;
                            let mut c = egui::Rgba::from_rgb(a[0], a[1], a[2]);
                            if egui::color_picker::color_edit_button_rgba(ui, &mut c, egui::color_picker::Alpha::Opaque).changed() {
                                for (n, v) in [("R", c.r()), ("G", c.g()), ("B", c.b())] {
                                    let _ = app.dispatch(Command::SetPreference { name: format!("Default Ambient Light Color {n}"), value: v as f64 });
                                }
                            }
                        }
                        ui.label(egui::RichText::new("Layer Defaults").strong());
                        check(ui, app, "Default to By Layer for New Nodes", "Default to By Layer for New Nodes");
                        check(ui, app, "New Lights Renderable By Layer", "New Lights Renderable By Layer");
                        ui.label("Propagate Unhide/Unfreeze Commands to Layers?");
                        ui.horizontal(|ui| {
                            let cur = app.state.prefs.propagate_layers;
                            for (v, label) in [(0u8, "Propagate"), (1, "Do Not Propagate"), (2, "Ask")] {
                                if ui.radio(cur == v, label).clicked() {
                                    let _ = app.dispatch(Command::SetPreference { name: "Propagate Unhide/Unfreeze to Layers".into(), value: v as f64 });
                                }
                            }
                        });
                    });
                    ui.group(|ui| {
                        ui.label(egui::RichText::new("UI Display").strong());
                        check(ui, app, "Topology Dependence Warning", "Topology Dependence Warning");
                        check(ui, app, "Large Toolbar Buttons", "Large Toolbar Buttons");
                        ui.horizontal(|ui| number(ui, app, "Flyout Time", "Flyout Time (ms):", 0.0..=2000.0, 0));
                    });
                    ui.group(|ui| {
                        ui.label(egui::RichText::new("Spinners").strong());
                        ui.horizontal(|ui| number(ui, app, "Spinner Precision", "Precision:", 0.0..=10.0, 0));
                        ui.horizontal(|ui| number(ui, app, "Spinner Snap", "Snap:", 0.001..=1000.0, 3));
                        check(ui, app, "Spinner Use Snap", "Use Snap");
                        check(ui, app, "Spinner Wrap Cursor", "Wrap Cursor near Spinner");
                    });
                    ui.group(|ui| {
                        ui.label(egui::RichText::new("Windows").strong());
                        check(ui, app, "Floating Windows as OS Windows", "Floating windows as separate OS windows");
                    });
                });
            }
            Tab::Files => {
                ui.group(|ui| {
                    ui.label(egui::RichText::new("File Handling").strong());
                    check(ui, app, "Backup on Save", "Backup on Save");
                    check(ui, app, "Increment on Save", "Increment on Save");
                    ui.horizontal(|ui| number(ui, app, "Recent Files in File Menu", "Recent Files in File Menu:", 0.0..=50.0, 0));
                });
                ui.group(|ui| {
                    ui.label(egui::RichText::new("Import Options").strong());
                    check(ui, app, "Zoom Extents on Import", "Zoom Extents on Import");
                });
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
            Tab::Rendering => {
                ui.group(|ui| {
                    ui.label(egui::RichText::new("Output File Sequencing").strong());
                    check(ui, app, "Nth Serial Numbering", "Nth Serial Numbering");
                });
            }
            Tab::Animation => {
                ui.group(|ui| {
                    ui.label(egui::RichText::new("Animate").strong());
                    check(ui, app, "Local Center During Animate", "Local Center During Animate");
                });
            }
            Tab::InverseKinematics => {
                for (group, prefix) in [("Applied IK", "Applied IK"), ("Interactive IK", "Interactive IK")] {
                    ui.group(|ui| {
                        ui.label(egui::RichText::new(group).strong());
                        ui.horizontal(|ui| number(ui, app, &format!("{prefix} Threshold"), "Position:", 0.0001..=1000.0, 3));
                        ui.horizontal(|ui| number(ui, app, &format!("{prefix} Iterations"), "Iterations:", 1.0..=10_000.0, 0));
                    });
                }
                check(ui, app, "Always Transform Children of the World", "Always Transform Children of the World");
            }
            Tab::Viewports => {
                ui.group(|ui| {
                    ui.label(egui::RichText::new("Viewport Parameters").strong());
                    check(ui, app, "Viewport Tooltips", "Viewport tooltips");
                    check(ui, app, "Show Cross Hair Cursor", "Show cross hair cursor");
                    check(ui, app, "Display World Axis", "Display World Axis");
                    check(ui, app, "Mask Viewport to Safe Region", "Mask Viewport to Safe Region");
                    ui.horizontal(|ui| number(ui, app, "Grid Nudge Distance", "Grid Nudge Distance:", 0.0..=10_000.0, 3));
                });
                ui.group(|ui| {
                    ui.label(egui::RichText::new("Ghosting").strong());
                    ui.horizontal(|ui| number(ui, app, "Ghosting Frames", "Ghosting Frames:", 1.0..=50.0, 0));
                    ui.horizontal(|ui| number(ui, app, "Ghosting Nth Frame", "Display Nth Frame:", 1.0..=1000.0, 0));
                    ui.horizontal(|ui| {
                        let mode = app.state.prefs.ghost_mode;
                        for (k, label) in ["Ghost Before Current Frame", "Ghost After Current Frame", "Ghost Before and After"].iter().enumerate() {
                            if ui.radio(mode == k as u32, *label).clicked() {
                                set(app, "Ghosting Mode", k as f64);
                            }
                        }
                    });
                    check(ui, app, "Ghosting Frame Numbers", "Show Frame Numbers");
                });
                ui.group(|ui| {
                    ui.label(egui::RichText::new("Mouse Control").strong());
                    ui.label("Zoom About Mouse Point:");
                    ui.horizontal(|ui| {
                        check(ui, app, "Zoom About Mouse Point Orthographic", "Orthographic");
                        check(ui, app, "Zoom About Mouse Point Perspective", "Perspective");
                    });
                    check(ui, app, "Right Click Menu over Selected Only", "Right Click Menu over Selected Only");
                    ui.horizontal(|ui| number(ui, app, "Wheel Zoom Increment", "Wheel Zoom Increment:", 0.1..=10.0, 2));
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
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new("Move Gizmo").strong());
                            ui.horizontal(|ui| number(ui, app, "Move Gizmo Relative Size", "Relative Size (%):", 0.0..=500.0, 0));
                            ui.label("Plane Handles");
                            check(ui, app, "Move Plane Handles", "On");
                            ui.horizontal(|ui| number(ui, app, "Move Plane Handle Size", "Size:", 1.0..=100.0, 0));
                            ui.horizontal(|ui| number(ui, app, "Move Plane Handle Offset", "Offset:", 0.0..=100.0, 0));
                            check(ui, app, "Center Box Handle", "Center Box Handle: Move in Screen Space");
                        });
                    });
                    ui.group(|ui| {
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new("Rotate Gizmo").strong());
                            ui.horizontal(|ui| number(ui, app, "Rotate Gizmo Relative Size", "Relative Size (%):", 0.0..=500.0, 0));
                            check(ui, app, "Rotate Free Rotation", "Free Rotation");
                            check(ui, app, "Rotate Show Tripod", "Show Tripod");
                            check(ui, app, "Rotate Screen Handle", "Screen Handle");
                            check(ui, app, "Rotate Show Pie Slice", "Show Pie Slice");
                            check(ui, app, "Rotate Angle Data", "Angle Data");
                            ui.label("Rotation Method:");
                            let m = app.state.prefs.gizmos.rotation_method;
                            for (v, label) in [(0u8, "Linear Roll"), (1, "Circular Crank"), (2, "Legacy R4")] {
                                if ui.radio(m == v, label).clicked() {
                                    set(app, "Rotation Method", v as f64);
                                }
                            }
                            ui.horizontal(|ui| number(ui, app, "Planar Angle Threshold", "Planar Angle Threshold:", 0.0..=90.0, 1));
                        });
                    });
                    ui.group(|ui| {
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new("Scale Gizmo").strong());
                            ui.horizontal(|ui| number(ui, app, "Scale Size", "Relative Size (%):", 10.0..=400.0, 0));
                            ui.horizontal(|ui| number(ui, app, "Scale Uniform Handle Size", "Uniform Handle Size (%):", 1.0..=95.0, 0));
                            ui.horizontal(|ui| number(ui, app, "Scale 2-Axis Handle Size", "2-Axis Handle Size (%):", 0.0..=99.0, 0));
                            check(ui, app, "Uniform 2-Axis Scaling", "Uniform 2-Axis Scaling");
                            ui.label(egui::RichText::new("Move/Rotate Transforms").strong());
                            let proj = app.state.prefs.gizmos.move_projection;
                            ui.horizontal(|ui| {
                                for (v, label) in [(false, "Intersection"), (true, "Projection")] {
                                    if ui.radio(proj == v, label).clicked() {
                                        set(app, "Move/Rotate Projection", if v { 1.0 } else { 0.0 });
                                    }
                                }
                            });
                            ui.horizontal(|ui| number(ui, app, "Persp Sens", "Persp Sens:", 0.01..=100.0, 2));
                            ui.horizontal(|ui| number(ui, app, "Rotation Increment", "Rotation Increment:", 0.0..=180.0, 2));
                            ui.horizontal(|ui| number(ui, app, "Viewport Orbit Snap Angle", "Orbit Snap Angle:", 0.0..=90.0, 1));
                        });
                    });
                });
            }
        }
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                done = true;
            }
            if ui.button("Cancel").clicked() {
                // Every preference back to its value when the dialog opened.
                for name in loom_commands::Prefs::NAMES.iter().chain(["Viewport Orbit Snap Angle"].iter()) {
                    if let (Some(was), Some(now)) = (snapshot.get(name), app.state.prefs.get(name)) {
                        if was != now {
                            set(app, name, was);
                        }
                    }
                }
                if app.state.prefs.auto_backup_name != snapshot.auto_backup_name {
                    let _ = app.dispatch(Command::SetAutoBackupName { name: snapshot.auto_backup_name.clone() });
                }
                done = true;
            }
        });
    });
    if done || !*open {
        ctx.data_mut(|d| d.remove::<loom_commands::Prefs>(snapshot_id));
    }
    if done {
        let _ = app.dispatch(Command::CloseDialog);
    }
    ctx.data_mut(|d| d.insert_temp(tab_id, tab));
}
