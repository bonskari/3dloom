//! The format dialogs that open after File > Import / Export picks an .fbx
//! or .obj file: FBX Importer / Exporter and OBJ Importer / Exporter, laid
//! out like the reference's (Presets, Include / Geometry / Animation /
//! Cameras / Embed Media, Advanced Options: Units / Axis Conversion / FBX
//! File Format; the OBJ groups). Every control sets one option through
//! `Set Exchange Option`; OK runs the import or export.

use crate::LoomApp;
use eframe::egui;
use loom_commands::exchange::{FileContent, ObjFaces, ObjNormals, UpAxis, FBX_VERSIONS};
use loom_commands::{Command, SystemUnit};
use serde_json::{json, Value};

/// Forgets the cached file statistics and list state (called when any dialog closes).
pub fn clear_state(d: &mut egui::util::IdTypeMap) {
    d.remove::<Value>(egui::Id::new("exchange_info"));
    d.remove::<String>(egui::Id::new("exchange_preset_name"));
    d.remove::<String>(egui::Id::new("exchange_obj_filter"));
}

fn set(app: &mut LoomApp, dialog: &str, name: &str, value: Value) {
    if let Err(e) = app.dispatch(Command::SetExchangeOption { dialog: dialog.into(), name: name.into(), value }) {
        app.state.status_prompt = e;
    }
}

fn check(ui: &mut egui::Ui, app: &mut LoomApp, dialog: &str, name: &str, label: &str, on: bool) {
    let mut v = on;
    if ui.checkbox(&mut v, label).changed() {
        set(app, dialog, name, json!(v));
    }
}

/// A labelled spinner: (value, range, drag speed).
fn number(ui: &mut egui::Ui, app: &mut LoomApp, dialog: &str, name: &str, label: &str, spin: (f32, std::ops::RangeInclusive<f32>, f64)) {
    let (value, range, speed) = spin;
    ui.horizontal(|ui| {
        ui.label(label);
        let mut v = value;
        if ui.add(egui::DragValue::new(&mut v).range(range).speed(speed)).changed() {
            set(app, dialog, name, json!(v));
        }
    });
}

fn units_combo(ui: &mut egui::Ui, app: &mut LoomApp, dialog: &str, name: &str, id: &str, current: SystemUnit, enabled: bool) {
    ui.add_enabled_ui(enabled, |ui| {
        egui::ComboBox::from_id_salt(id).selected_text(current.label()).show_ui(ui, |ui| {
            for u in SystemUnit::ALL {
                if ui.selectable_label(u == current, u.label()).clicked() {
                    set(app, dialog, name, json!(u));
                }
            }
        });
    });
}

/// The Presets group: current preset, Save as… and Delete.
fn presets(ui: &mut egui::Ui, ctx: &egui::Context, app: &mut LoomApp, dialog: &str, current: &str) {
    let names: Vec<String> = app.state.exchange.describe(dialog).ok().and_then(|v| serde_json::from_value(v["presets"].clone()).ok()).unwrap_or_default();
    ui.group(|ui| {
        ui.label(egui::RichText::new("Presets").strong());
        ui.horizontal(|ui| {
            ui.label("Current Preset:");
            egui::ComboBox::from_id_salt(format!("{dialog}_preset")).selected_text(current).show_ui(ui, |ui| {
                for n in &names {
                    if ui.selectable_label(n == current, n).clicked() {
                        set(app, dialog, "preset", json!(n));
                    }
                }
            });
        });
        let id = egui::Id::new("exchange_preset_name");
        let mut name: String = ctx.data(|d| d.get_temp(id)).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut name).hint_text("Preset name").desired_width(140.0));
            if ui.add_enabled(!name.trim().is_empty(), egui::Button::new("Save Preset")).clicked() {
                match app.dispatch(Command::SaveExchangePreset { dialog: dialog.into(), name: name.trim().into() }) {
                    Ok(_) => name.clear(),
                    Err(e) => app.state.status_prompt = e,
                }
            }
            let user = app.state.exchange.user_presets.get(dialog).is_some_and(|m| m.contains_key(current));
            if ui.add_enabled(user, egui::Button::new("Delete Preset")).clicked() {
                let _ = app.dispatch(Command::DeleteExchangePreset { dialog: dialog.into(), name: current.into() });
                set(app, dialog, "preset", json!(names.first().cloned().unwrap_or_default()));
            }
        });
        ctx.data_mut(|d| d.insert_temp(id, name));
    });
}

/// The options' scroll height: the window (title, presets, buttons) fits
/// the space the panels leave, so OK / Cancel stay reachable.
fn scroll_height(ctx: &egui::Context) -> f32 {
    (ctx.available_rect().height() - 120.0).clamp(150.0, 620.0)
}

/// Where the options windows open: the top left of the space the panels leave.
fn window_pos(ctx: &egui::Context) -> egui::Pos2 {
    ctx.available_rect().min + egui::vec2(40.0, 8.0)
}

fn section(ui: &mut egui::Ui, title: &str, open: bool, body: impl FnOnce(&mut egui::Ui)) {
    egui::CollapsingHeader::new(egui::RichText::new(title).strong()).default_open(open).show(ui, body);
}

/// OK / Cancel: OK runs the pending import or export; both close the dialog.
fn buttons(ui: &mut egui::Ui, app: &mut LoomApp, ok: &str, done: &mut bool, export: bool) {
    ui.separator();
    ui.horizontal(|ui| {
        if ui.button(ok).clicked() {
            if let Some((path, selected)) = app.state.exchange_pending.clone() {
                let p = Some(path.to_string_lossy().into_owned());
                let cmd = match (export, selected) {
                    (false, _) => Command::ImportFile { path: p },
                    (true, false) => Command::ExportFile { path: p },
                    (true, true) => Command::ExportSelected { path: p },
                };
                if let Err(e) = app.dispatch(cmd) {
                    app.state.status_prompt = e;
                }
            }
            *done = true;
        }
        if ui.button("Cancel").clicked() {
            *done = true;
        }
    });
}

fn file_title(app: &LoomApp, title: &str) -> String {
    let name = app.state.exchange_pending.as_ref().and_then(|(p, _)| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    format!("{title} - {name}")
}

fn finish(app: &mut LoomApp, done: bool, open: &mut bool) {
    if done || !*open {
        *open = false;
        app.state.exchange_pending = None;
    }
}

pub fn fbx_export(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    const D: &str = "fbx_export";
    let o = app.state.exchange.fbx_export.clone();
    let mut done = false;
    let range = app.state.frame_range;
    egui::Window::new(file_title(app, "FBX Export")).id(egui::Id::new("fbx_export_options")).open(open).default_pos(window_pos(ctx)).resizable(false).default_width(360.0).show(ctx, |ui| {
        egui::ScrollArea::vertical().max_height((ui.available_height() - 40.0).min(scroll_height(ctx))).show(ui, |ui| {
            presets(ui, ctx, app, D, &o.preset);
            section(ui, "Include", true, |ui| {
                section(ui, "Geometry", true, |ui| {
                    check(ui, app, D, "smoothing_groups", "Smoothing Groups", o.smoothing_groups);
                    check(ui, app, D, "split_normals", "Split per-vertex Normals", o.split_normals);
                    check(ui, app, D, "tangents_and_binormals", "Tangents and Binormals", o.tangents_and_binormals);
                    check(ui, app, D, "turbosmooth", "TurboSmooth", o.turbosmooth);
                    check(ui, app, D, "preserve_instances", "Preserve Instances", o.preserve_instances);
                    check(ui, app, D, "selection_sets", "Selection Sets", o.selection_sets);
                    check(ui, app, D, "convert_deforming_dummies_to_bones", "Convert Deforming Dummies to Bones", o.convert_deforming_dummies_to_bones);
                    check(ui, app, D, "triangulate", "Triangulate", o.triangulate);
                });
                section(ui, "Animation", true, |ui| {
                    check(ui, app, D, "animation", "Animation", o.animation);
                    ui.add_enabled_ui(o.animation, |ui| {
                        section(ui, "Extra Options", false, |ui| {
                            check(ui, app, D, "use_scene_name", "Use Scene Name", o.use_scene_name);
                            check(ui, app, D, "remove_single_key", "Remove Single Key", o.remove_single_key);
                        });
                        section(ui, "Bake Animation", true, |ui| {
                            check(ui, app, D, "bake_animation", "Bake Animation", o.bake_animation);
                            ui.add_enabled_ui(o.bake_animation, |ui| {
                                ui.horizontal(|ui| {
                                    let (mut a, mut b, mut s) = (o.bake_start.unwrap_or(range.0), o.bake_end.unwrap_or(range.1), o.bake_step.unwrap_or(1));
                                    ui.label("Start:");
                                    if ui.add(egui::DragValue::new(&mut a)).changed() {
                                        set(app, D, "bake_start", json!(a));
                                    }
                                    ui.label("End:");
                                    if ui.add(egui::DragValue::new(&mut b)).changed() {
                                        set(app, D, "bake_end", json!(b));
                                    }
                                    ui.label("Step:");
                                    if ui.add(egui::DragValue::new(&mut s).range(1..=1000)).changed() {
                                        set(app, D, "bake_step", json!(s));
                                    }
                                });
                            });
                        });
                        section(ui, "Deformations", true, |ui| {
                            check(ui, app, D, "deformations", "Deformations", o.deformations);
                            ui.add_enabled_ui(o.deformations, |ui| {
                                check(ui, app, D, "skins", "Skins", o.skins);
                                check(ui, app, D, "morphs", "Morphs", o.morphs);
                            });
                        });
                        section(ui, "Curve Filters", false, |ui| {
                            check(ui, app, D, "constant_key_reducer", "Constant Key Reducer", o.constant_key_reducer);
                            ui.add_enabled_ui(o.constant_key_reducer, |ui| {
                                number(ui, app, D, "translation_precision", "Translation Precision", (o.translation_precision, 0.0..=100.0, 0.0001));
                                number(ui, app, D, "rotation_precision", "Rotation Precision", (o.rotation_precision, 0.0..=100.0, 0.001));
                                number(ui, app, D, "scaling_precision", "Scaling Precision", (o.scaling_precision, 0.0..=100.0, 0.001));
                                number(ui, app, D, "other_precision", "Other Precision", (o.other_precision, 0.0..=100.0, 0.001));
                            });
                            check(ui, app, D, "key_reducer", "Key Reducer", o.key_reducer);
                            ui.add_enabled_ui(o.key_reducer, |ui| {
                                number(ui, app, D, "key_reducer_precision", "Filter Intensity", (o.key_reducer_precision, 0.0..=100.0, 0.01));
                                check(ui, app, D, "xyz_curve_sync", "XYZ Curve Synchronization", o.xyz_curve_sync);
                            });
                        });
                    });
                });
                check(ui, app, D, "cameras", "Cameras", o.cameras);
                check(ui, app, D, "lights", "Lights", o.lights);
                check(ui, app, D, "embed_media", "Embed Media", o.embed_media);
            });
            section(ui, "Advanced Options", true, |ui| {
                section(ui, "Units", true, |ui| {
                    check(ui, app, D, "units_automatic", "Automatic", o.units_automatic);
                    let scene_m = app.state.units.system_unit.metres() * app.state.units.system_scale as f64;
                    let file_m = if o.units_automatic { scene_m } else { o.units.metres() };
                    ui.label(format!("Scale Factor: {}", fmt(scene_m / file_m)));
                    ui.horizontal(|ui| {
                        ui.label("Scene units converted to:");
                        let shown = if o.units_automatic { app.state.units.system_unit } else { o.units };
                        units_combo(ui, app, D, "units", "fbx_export_units", shown, !o.units_automatic);
                    });
                });
                section(ui, "Axis Conversion", true, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Up Axis:");
                        for (axis, label) in [(UpAxis::Y, "Y-up"), (UpAxis::Z, "Z-up")] {
                            if ui.radio(o.up_axis == axis, label).clicked() && o.up_axis != axis {
                                set(app, D, "up_axis", json!(axis));
                            }
                        }
                    });
                });
                section(ui, "FBX File Format", true, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Type:");
                        if ui.radio(!o.ascii, "Binary").clicked() && o.ascii {
                            set(app, D, "ascii", json!(false));
                        }
                        if ui.radio(o.ascii, "ASCII").clicked() && !o.ascii {
                            set(app, D, "ascii", json!(true));
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Version:");
                        let current = FBX_VERSIONS.iter().find(|v| v.0 == o.version).map_or("", |v| v.1);
                        egui::ComboBox::from_id_salt("fbx_export_version").selected_text(current).show_ui(ui, |ui| {
                            for (v, label) in FBX_VERSIONS {
                                if ui.selectable_label(v == o.version, label).clicked() {
                                    set(app, D, "version", json!(v));
                                }
                            }
                        });
                    });
                });
            });
        });
        buttons(ui, app, "OK", &mut done, true);
    });
    finish(app, done, open);
}

pub fn fbx_import(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    const D: &str = "fbx_import";
    let o = app.state.exchange.fbx_import.clone();
    let mut done = false;
    // The file's statistics, read once per dialog.
    let info_id = egui::Id::new("exchange_info");
    let info: Value = match ctx.data(|d| d.get_temp::<Value>(info_id)) {
        Some(v) => v,
        None => {
            let v = app.state.exchange_pending.clone().map(|(p, _)| app.dispatch(Command::FbxFileInfo { path: p.to_string_lossy().into_owned() }).unwrap_or_else(|e| json!({ "error": e }))).unwrap_or(Value::Null);
            ctx.data_mut(|d| d.insert_temp(info_id, v.clone()));
            v
        }
    };
    egui::Window::new(file_title(app, "FBX Import")).id(egui::Id::new("fbx_import_options")).open(open).default_pos(window_pos(ctx)).resizable(false).default_width(360.0).show(ctx, |ui| {
        egui::ScrollArea::vertical().max_height((ui.available_height() - 40.0).min(scroll_height(ctx))).show(ui, |ui| {
            presets(ui, ctx, app, D, &o.preset);
            section(ui, "Statistics", false, |ui| {
                if let Some(e) = info["error"].as_str() {
                    ui.label(e);
                }
                for (label, key) in [
                    ("File name", "file_name"),
                    ("File directory", "file_directory"),
                    ("File version", "file_version"),
                    ("File creator", "file_creator"),
                    ("File creation time", "file_creation_time"),
                    ("File axis direction", "file_axis_direction"),
                    ("File units", "file_units"),
                    ("System axis direction", "system_axis_direction"),
                    ("System units", "system_units"),
                ] {
                    let v = &info[key];
                    ui.label(format!("{label}: {}", v.as_str().map(str::to_owned).unwrap_or_else(|| v.to_string())));
                }
                ui.label(format!("File content: {} elements, {} materials, {} textures, {} caches", info["elements"], info["materials"], info["textures"], info["caches"]));
                ui.label(format!("FPS: file {}, system {}", info["file_fps"], info["system_fps"]));
            });
            section(ui, "Include", true, |ui| {
                ui.horizontal(|ui| {
                    ui.label("File content:");
                    egui::ComboBox::from_id_salt("fbx_import_content").selected_text(o.file_content.label()).show_ui(ui, |ui| {
                        for c in FileContent::ALL {
                            if ui.selectable_label(c == o.file_content, c.label()).clicked() {
                                set(app, D, "file_content", json!(c));
                            }
                        }
                    });
                });
                section(ui, "Geometry", true, |ui| {
                    check(ui, app, D, "smoothing_groups", "Smoothing Groups", o.smoothing_groups);
                });
                section(ui, "Animation", true, |ui| {
                    check(ui, app, D, "animation", "Animation", o.animation);
                    ui.add_enabled_ui(o.animation, |ui| {
                        section(ui, "Extra Options", true, |ui| {
                            let takes: Vec<String> = serde_json::from_value(info["takes"].clone()).unwrap_or_default();
                            ui.horizontal(|ui| {
                                ui.label("Take:");
                                let current = o.take.clone().or_else(|| takes.first().cloned()).unwrap_or_else(|| "No animation".into());
                                egui::ComboBox::from_id_salt("fbx_import_take").selected_text(&current).show_ui(ui, |ui| {
                                    for t in &takes {
                                        if ui.selectable_label(*t == current, t).clicked() {
                                            set(app, D, "take", json!(t));
                                        }
                                    }
                                });
                            });
                            check(ui, app, D, "fill_timeline", "Fill Timeline", o.fill_timeline);
                            check(ui, app, D, "bake_animation_layers", "Bake Animation Layers", o.bake_animation_layers);
                        });
                        section(ui, "Deformations", true, |ui| {
                            check(ui, app, D, "deformations", "Deformations", o.deformations);
                            ui.add_enabled_ui(o.deformations, |ui| {
                                check(ui, app, D, "skins", "Skins", o.skins);
                                check(ui, app, D, "morphs", "Morphs", o.morphs);
                            });
                        });
                        section(ui, "Curve Filters", false, |ui| {
                            check(ui, app, D, "constant_key_reducer", "Constant Key Reducer", o.constant_key_reducer);
                            ui.add_enabled_ui(o.constant_key_reducer, |ui| {
                                number(ui, app, D, "translation_precision", "Translation Filter Intensity", (o.translation_precision, 0.0..=100.0, 0.0001));
                                number(ui, app, D, "rotation_precision", "Rotation Filter Intensity", (o.rotation_precision, 0.0..=100.0, 0.001));
                                number(ui, app, D, "scaling_precision", "Scaling Filter Intensity", (o.scaling_precision, 0.0..=100.0, 0.001));
                                number(ui, app, D, "other_precision", "Other Filter Intensity", (o.other_precision, 0.0..=100.0, 0.001));
                            });
                            check(ui, app, D, "key_reducer", "Key Reducer", o.key_reducer);
                            ui.add_enabled_ui(o.key_reducer, |ui| {
                                number(ui, app, D, "key_reducer_precision", "Filter Intensity", (o.key_reducer_precision, 0.0..=100.0, 0.01));
                                check(ui, app, D, "xyz_curve_sync", "XYZ Curve Synchronization", o.xyz_curve_sync);
                            });
                        });
                        section(ui, "Bone Creation", false, |ui| {
                            ui.horizontal(|ui| {
                                if ui.radio(o.bones_as_dummies, "Convert as Dummy").clicked() && !o.bones_as_dummies {
                                    set(app, D, "bones_as_dummies", json!(true));
                                }
                                if ui.radio(!o.bones_as_dummies, "Leave as Bones").clicked() && o.bones_as_dummies {
                                    set(app, D, "bones_as_dummies", json!(false));
                                }
                            });
                            ui.add_enabled_ui(!o.bones_as_dummies, |ui| {
                                check(ui, app, D, "lock_width_height", "Lock Width and Height", o.lock_width_height);
                                number(ui, app, D, "bone_width", "Width", (o.bone_width, 0.001..=1e6, 0.1));
                                ui.add_enabled_ui(!o.lock_width_height, |ui| number(ui, app, D, "bone_height", "Height", (o.bone_height, 0.001..=1e6, 0.1)));
                                number(ui, app, D, "bone_taper", "Taper (%)", (o.bone_taper, 0.0..=100.0, 1.0));
                            });
                        });
                    });
                });
                check(ui, app, D, "cameras", "Cameras", o.cameras);
                check(ui, app, D, "lights", "Lights", o.lights);
            });
            section(ui, "Advanced Options", true, |ui| {
                section(ui, "Units", true, |ui| {
                    check(ui, app, D, "units_automatic", "Automatic", o.units_automatic);
                    if let Some(f) = info["scale_factor"].as_f64() {
                        ui.label(format!("Scale Factor: {}", fmt(f)));
                    }
                    ui.horizontal(|ui| {
                        ui.label("File units converted to:");
                        let shown = if o.units_automatic { app.state.units.system_unit } else { o.units };
                        units_combo(ui, app, D, "units", "fbx_import_units", shown, !o.units_automatic);
                    });
                    if !o.units_automatic {
                        ui.label(egui::RichText::new("The file's numbers are read in this unit.").small().weak());
                    }
                });
            });
        });
        buttons(ui, app, "OK", &mut done, false);
    });
    // The scale factor depends on the unit options: read it again after a change.
    if app.state.exchange.fbx_import.units != o.units || app.state.exchange.fbx_import.units_automatic != o.units_automatic {
        ctx.data_mut(|d| d.remove::<Value>(info_id));
    }
    finish(app, done, open);
}

pub fn obj_export(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    const D: &str = "obj_export";
    let o = app.state.exchange.obj_export.clone();
    let mut done = false;
    egui::Window::new(file_title(app, "OBJ Export Options")).id(egui::Id::new("obj_export_options")).open(open).default_pos(window_pos(ctx)).resizable(false).show(ctx, |ui| {
        ui.horizontal_top(|ui| {
            ui.group(|ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("Geometry").strong());
                    check(ui, app, D, "flip_yz", "Flip YZ-axis (Poser-like)", o.flip_yz);
                    check(ui, app, D, "hidden_objects", "Hidden Objects", o.hidden_objects);
                    ui.horizontal(|ui| {
                        ui.label("Faces:");
                        egui::ComboBox::from_id_salt("obj_export_faces").selected_text(format!("{:?}", o.faces)).show_ui(ui, |ui| {
                            for f in [ObjFaces::Triangles, ObjFaces::Quads, ObjFaces::Polygons] {
                                if ui.selectable_label(f == o.faces, format!("{f:?}")).clicked() {
                                    set(app, D, "faces", json!(f));
                                }
                            }
                        });
                    });
                    check(ui, app, D, "texture_coordinates", "Texture coordinates", o.texture_coordinates);
                    check(ui, app, D, "normals", "Normals", o.normals);
                    check(ui, app, D, "smoothing_groups", "Smoothing groups", o.smoothing_groups);
                    number(ui, app, D, "scale", "Scale:", (o.scale, 0.0001..=1e6, 0.01));
                });
            });
            ui.vertical(|ui| {
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("Material").strong());
                        check(ui, app, D, "use_material", "Use material", o.use_material);
                        ui.add_enabled_ui(o.use_material, |ui| {
                            check(ui, app, D, "create_mat_library", "Create mat-library", o.create_mat_library);
                            check(ui, app, D, "force_black_ambient", "Force black ambient", o.force_black_ambient);
                        });
                    });
                });
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("Output").strong());
                        ui.horizontal(|ui| {
                            ui.label("Target:");
                            if ui.radio(!o.target_windows, "Unix (LF)").clicked() && o.target_windows {
                                set(app, D, "target_windows", json!(false));
                            }
                            if ui.radio(o.target_windows, "Windows (CR LF)").clicked() && !o.target_windows {
                                set(app, D, "target_windows", json!(true));
                            }
                        });
                        check(ui, app, D, "relative_numbers", "Relative numbers", o.relative_numbers);
                        ui.horizontal(|ui| {
                            ui.label("Precision:");
                            let mut p = o.precision;
                            if ui.add(egui::DragValue::new(&mut p).range(1..=12)).changed() {
                                set(app, D, "precision", json!(p));
                            }
                        });
                    });
                });
            });
        });
        presets(ui, ctx, app, D, &o.preset);
        buttons(ui, app, "Export", &mut done, true);
    });
    finish(app, done, open);
}

/// Import STL File: vertex weld and threshold, auto-smooth and its angle,
/// remove double faces, unify normals.
pub fn stl_import(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    const D: &str = "stl_import";
    let o = app.state.exchange.stl_import.clone();
    let mut done = false;
    egui::Window::new(file_title(app, "Import STL File")).id(egui::Id::new("stl_import_options")).open(open).default_pos(window_pos(ctx)).resizable(false).show(ctx, |ui| {
        ui.label(egui::RichText::new("Vertex Welding").strong());
        check(ui, app, D, "weld", "Weld Vertices", o.weld);
        ui.add_enabled_ui(o.weld, |ui| number(ui, app, D, "weld_threshold", "Weld Threshold:", (o.weld_threshold, 0.0..=1e6, 0.001)));
        ui.separator();
        ui.label(egui::RichText::new("Normals").strong());
        check(ui, app, D, "auto_smooth", "Auto-Smooth", o.auto_smooth);
        ui.add_enabled_ui(o.auto_smooth, |ui| number(ui, app, D, "smooth_angle", "Smooth Angle:", (o.smooth_angle, 0.0..=180.0, 0.5)));
        check(ui, app, D, "remove_double_faces", "Remove Double Faces", o.remove_double_faces);
        check(ui, app, D, "unify_normals", "Unify Normals", o.unify_normals);
        buttons(ui, app, "OK", &mut done, false);
    });
    finish(app, done, open);
}

/// Export STL File: Binary or ASCII.
pub fn stl_export(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    const D: &str = "stl_export";
    let o = app.state.exchange.stl_export.clone();
    let mut done = false;
    egui::Window::new(file_title(app, "Export STL File")).id(egui::Id::new("stl_export_options")).open(open).default_pos(window_pos(ctx)).resizable(false).show(ctx, |ui| {
        ui.horizontal(|ui| {
            if ui.radio(o.binary, "Binary").clicked() && !o.binary {
                set(app, D, "binary", json!(true));
            }
            if ui.radio(!o.binary, "ASCII").clicked() && o.binary {
                set(app, D, "binary", json!(false));
            }
        });
        buttons(ui, app, "OK", &mut done, true);
    });
    finish(app, done, open);
}

/// 3DS Import: merge the objects into the scene or replace it.
pub fn tds_import(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    const D: &str = "3ds_import";
    let o = app.state.exchange.tds_import.clone();
    let mut done = false;
    egui::Window::new(file_title(app, "3DS Import")).id(egui::Id::new("tds_import_options")).open(open).default_pos(window_pos(ctx)).resizable(false).show(ctx, |ui| {
        if ui.radio(!o.replace_scene, "Merge objects with current scene").clicked() && o.replace_scene {
            set(app, D, "replace_scene", json!(false));
        }
        if ui.radio(o.replace_scene, "Completely replace current scene").clicked() && !o.replace_scene {
            set(app, D, "replace_scene", json!(true));
        }
        buttons(ui, app, "OK", &mut done, false);
    });
    finish(app, done, open);
}

/// `*` any run, `?` one character, case-insensitive.
fn wildcard(pattern: &str, name: &str) -> bool {
    fn go(p: &[char], n: &[char]) -> bool {
        match (p.first(), n.first()) {
            (None, None) => true,
            (Some('*'), _) => go(&p[1..], n) || (!n.is_empty() && go(p, &n[1..])),
            (Some('?'), Some(_)) => go(&p[1..], &n[1..]),
            (Some(a), Some(b)) => a == b && go(&p[1..], &n[1..]),
            _ => false,
        }
    }
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let n: Vec<char> = name.to_lowercase().chars().collect();
    go(&p, &n)
}

pub fn obj_import(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    const D: &str = "obj_import";
    let o = app.state.exchange.obj_import.clone();
    let mut done = false;
    let info_id = egui::Id::new("exchange_info");
    let info: Value = match ctx.data(|d| d.get_temp::<Value>(info_id)) {
        Some(v) => v,
        None => {
            let v = app.state.exchange_pending.clone().map(|(p, _)| app.dispatch(Command::ObjFileInfo { path: p.to_string_lossy().into_owned() }).unwrap_or_else(|e| json!({ "error": e }))).unwrap_or(Value::Null);
            // A new file: every object checked.
            set(app, D, "objects", Value::Null);
            ctx.data_mut(|d| d.insert_temp(info_id, v.clone()));
            v
        }
    };
    let names: Vec<(String, u64)> = info["objects"].as_array().map(|a| a.iter().map(|x| (x["name"].as_str().unwrap_or("").to_owned(), x["faces"].as_u64().unwrap_or(0))).collect()).unwrap_or_default();
    let checked: Vec<String> = o.objects.clone().unwrap_or_else(|| names.iter().map(|n| n.0.clone()).collect());
    let filter_id = egui::Id::new("exchange_obj_filter");
    let mut filter: String = ctx.data(|d| d.get_temp(filter_id)).unwrap_or_default();
    egui::Window::new(file_title(app, "OBJ Import Options")).id(egui::Id::new("obj_import_options")).open(open).default_pos(window_pos(ctx)).resizable(true).default_width(520.0).show(ctx, |ui| {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("Objects").strong());
                        check(ui, app, D, "reset_scene", "Reset scene", o.reset_scene);
                        ui.horizontal(|ui| {
                            ui.label("Prefix:");
                            let mut p = o.prefix.clone();
                            if ui.add(egui::TextEdit::singleline(&mut p).desired_width(80.0)).changed() {
                                set(app, D, "prefix", json!(p));
                            }
                        });
                        check(ui, app, D, "single_mesh", "Import as single mesh", o.single_mesh);
                        check(ui, app, D, "retriangulate", "Retriangulate Polygons", o.retriangulate);
                        ui.label(format!("{}: {} faces", info["file_name"].as_str().unwrap_or(""), info["faces"]));
                        // The "LED" icons: green when the file has the feature, red when not.
                        let led = |ui: &mut egui::Ui, on: bool, label: &str| {
                            let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                            ui.painter().circle_filled(r.center(), 4.0, if on { egui::Color32::from_rgb(60, 200, 60) } else { egui::Color32::from_rgb(200, 60, 60) });
                            ui.label(label);
                        };
                        ui.horizontal(|ui| {
                            led(ui, info["normals"] == true, "Normals");
                            led(ui, info["texture_coordinates"] == true, "UVs");
                            led(ui, info["smoothing_groups"] == true, "Smoothing");
                            led(ui, info["materials"] == true, "Materials");
                        });
                        egui::ScrollArea::vertical().max_height(180.0).id_salt("obj_import_objects").show(ui, |ui| {
                            for (name, faces) in &names {
                                let mut on = checked.contains(name);
                                if ui.checkbox(&mut on, format!("{name} ({faces})")).changed() {
                                    let mut next: Vec<String> = checked.iter().filter(|n| *n != name).cloned().collect();
                                    if on {
                                        next.push(name.clone());
                                    }
                                    set(app, D, "objects", json!(next));
                                }
                            }
                        });
                        ui.horizontal(|ui| {
                            if ui.button("All").clicked() {
                                set(app, D, "objects", Value::Null);
                            }
                            if ui.button("None").clicked() {
                                set(app, D, "objects", json!(Vec::<String>::new()));
                            }
                            if ui.button("Invert").clicked() {
                                let inv: Vec<&String> = names.iter().map(|n| &n.0).filter(|n| !checked.contains(n)).collect();
                                set(app, D, "objects", json!(inv));
                            }
                            let r = ui.add(egui::TextEdit::singleline(&mut filter).hint_text("sph*").desired_width(80.0));
                            if r.changed() && !filter.is_empty() {
                                let hit: Vec<&String> = names.iter().map(|n| &n.0).filter(|n| wildcard(&filter, n)).collect();
                                set(app, D, "objects", json!(hit));
                            }
                        });
                    });
                });
            });
            ui.vertical(|ui| {
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("Geometry").strong());
                        check(ui, app, D, "flip_zy", "Flip ZY-axis", o.flip_zy);
                        check(ui, app, D, "center_pivots", "Center Pivots", o.center_pivots);
                        check(ui, app, D, "texture_coordinates", "Texture coordinates", o.texture_coordinates);
                        check(ui, app, D, "smoothing_groups", "Smoothing groups", o.smoothing_groups);
                    });
                });
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("Normals").strong());
                        for n in ObjNormals::ALL {
                            ui.horizontal(|ui| {
                                if ui.radio(o.normals == n, n.label()).clicked() && o.normals != n {
                                    set(app, D, "normals", json!(n));
                                }
                                if n == ObjNormals::AutoSmooth {
                                    let mut a = o.auto_smooth_angle;
                                    if ui.add_enabled(o.normals == n, egui::DragValue::new(&mut a).range(0.0..=180.0)).changed() {
                                        set(app, D, "auto_smooth_angle", json!(a));
                                    }
                                }
                            });
                        }
                        check(ui, app, D, "flip_normals", "Flip Normals", o.flip_normals);
                    });
                });
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("Units/Scale").strong());
                        ui.horizontal(|ui| {
                            check(ui, app, D, "convert_units", "Convert", o.convert_units);
                            ui.label("Model Units:");
                            units_combo(ui, app, D, "model_units", "obj_import_units", o.model_units, o.convert_units);
                        });
                        ui.add_enabled_ui(!o.convert_units, |ui| number(ui, app, D, "object_scale", "Object Scale:", (o.object_scale, 0.0001..=1e6, 0.01)));
                    });
                });
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("Material").strong());
                        check(ui, app, D, "unique_wire_color", "Unique Wire Color", o.unique_wire_color);
                        check(ui, app, D, "import_materials", "Import Materials", o.import_materials);
                        ui.add_enabled_ui(o.import_materials, |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Prefix:");
                                let mut p = o.material_prefix.clone();
                                if ui.add(egui::TextEdit::singleline(&mut p).desired_width(80.0)).changed() {
                                    set(app, D, "material_prefix", json!(p));
                                }
                            });
                            number(ui, app, D, "default_bump", "Default bump:", (o.default_bump, -999.0..=999.0, 1.0));
                            check(ui, app, D, "force_black_ambient", "Force black ambient", o.force_black_ambient);
                            check(ui, app, D, "import_into_mat_editor", "Import into Mat-Editor", o.import_into_mat_editor);
                            check(ui, app, D, "show_maps_in_viewport", "Show maps in viewport", o.show_maps_in_viewport);
                        });
                    });
                });
            });
        });
        presets(ui, ctx, app, D, &o.preset);
        buttons(ui, app, "Import", &mut done, false);
    });
    ctx.data_mut(|d| d.insert_temp(filter_id, filter));
    finish(app, done, open);
}

fn fmt(x: f64) -> String {
    let s = format!("{x:.7}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

#[cfg(test)]
mod tests {
    use super::wildcard;

    #[test]
    fn wildcards_match_like_the_object_filter() {
        assert!(wildcard("sph*", "Sphere001"));
        assert!(!wildcard("sph*", "Box001"));
        assert!(wildcard("box?", "Box1"));
        assert!(!wildcard("box?", "Box12"));
    }
}
