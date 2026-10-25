//! File > Replace (the matching objects list), Set Project Folder, Asset
//! Tracking and the Missing External Files dialog shown after File > Open.

use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;
use std::collections::BTreeSet;

#[derive(Clone, Default)]
struct ReplaceState {
    picked: BTreeSet<String>,
    materials: bool,
    init: bool,
}

/// Replace: the file's objects whose names objects in the scene have; OK
/// replaces the highlighted ones (and their materials when asked).
pub fn replace_objects(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let Some(path) = app.state.replace_source.clone() else {
        *open = false;
        return;
    };
    let names = match loom_commands::file::read(&path) {
        Ok(doc) => loom_commands::assets::replace_candidates(&app.state, &doc),
        Err(e) => {
            app.state.status_prompt = e;
            *open = false;
            return;
        }
    };
    let key = egui::Id::new("replace_objects_state");
    let mut st: ReplaceState = ctx.data(|d| d.get_temp(key)).unwrap_or_default();
    if !st.init {
        st.picked = names.iter().cloned().collect();
        st.init = true;
    }
    let mut done = None;
    let title = format!("Replace - {}", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    egui::Window::new(title).id(egui::Id::new("replace_objects")).open(open).collapsible(false).resizable(false).show(ctx, |ui| {
        if names.is_empty() {
            ui.label("No object in this file has the name of an object in the scene.");
        }
        egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
            for n in &names {
                let on = st.picked.contains(n);
                if ui.selectable_label(on, n).clicked() {
                    if on {
                        st.picked.remove(n);
                    } else {
                        st.picked.insert(n.clone());
                    }
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.button("All").clicked() {
                st.picked = names.iter().cloned().collect();
            }
            if ui.button("None").clicked() {
                st.picked.clear();
            }
        });
        ui.checkbox(&mut st.materials, "Replace materials too");
        ui.horizontal(|ui| {
            if ui.add_enabled(!st.picked.is_empty(), egui::Button::new("OK")).clicked() {
                done = Some(true);
            }
            if ui.button("Cancel").clicked() {
                done = Some(false);
            }
        });
    });
    if let Some(apply) = done {
        if apply {
            let picked: Vec<String> = names.iter().filter(|n| st.picked.contains(*n)).cloned().collect();
            let cmd = Command::ReplaceFile { path: Some(path.to_string_lossy().into_owned()), names: Some(picked), materials: Some(st.materials) };
            if let Err(e) = app.dispatch(cmd) {
                app.state.status_prompt = e;
            }
        }
        *open = false;
    }
    if *open {
        ctx.data_mut(|d| d.insert_temp(key, st));
    } else {
        app.state.replace_source = None;
        ctx.data_mut(|d| d.remove::<ReplaceState>(key));
    }
}

/// Edit > Select By > Layer: highlight one or more layers; OK selects their objects.
pub fn select_by_layer(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let key = egui::Id::new("select_by_layer_picked");
    let mut picked: BTreeSet<u32> = ctx.data(|d| d.get_temp(key)).unwrap_or_default();
    let layers: Vec<(u32, String)> = app.state.scene.layers.iter().map(|l| (l.id, l.name.clone())).collect();
    let mut done = None;
    egui::Window::new("Select By Layer").open(open).collapsible(false).resizable(false).show(ctx, |ui| {
        egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
            for (id, name) in &layers {
                let on = picked.contains(id);
                if ui.selectable_label(on, name).clicked() {
                    if !ui.input(|i| i.modifiers.ctrl || i.modifiers.shift) {
                        picked.clear();
                    }
                    if on {
                        picked.remove(id);
                    } else {
                        picked.insert(*id);
                    }
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.add_enabled(!picked.is_empty(), egui::Button::new("OK")).clicked() {
                done = Some(true);
            }
            if ui.button("Cancel").clicked() {
                done = Some(false);
            }
        });
    });
    if let Some(apply) = done {
        if apply {
            let _ = app.dispatch(Command::SelectByLayer { layers: Some(picked.iter().copied().collect()) });
        }
        *open = false;
    }
    if *open {
        ctx.data_mut(|d| d.insert_temp(key, picked));
    } else {
        ctx.data_mut(|d| d.remove::<BTreeSet<u32>>(key));
    }
}

/// A folder field with OK / Cancel (the in-app chooser; a person at the
/// desktop gets the system folder dialog).
fn folder_prompt(ctx: &egui::Context, app: &LoomApp, title: &str, key: &str, start: Option<std::path::PathBuf>, open: &mut bool) -> Option<String> {
    let id = egui::Id::new(key);
    let mut text: String = ctx.data(|d| d.get_temp(id)).unwrap_or_else(|| start.unwrap_or_else(|| app.file_dir.clone()).to_string_lossy().into_owned());
    let mut chosen = None;
    let mut cancel = false;
    egui::Window::new(title).open(open).collapsible(false).resizable(false).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label("Folder:");
            ui.add(egui::TextEdit::singleline(&mut text).desired_width(360.0));
        });
        ui.horizontal(|ui| {
            if ui.add_enabled(!text.trim().is_empty(), egui::Button::new("OK")).clicked() {
                chosen = Some(text.trim().to_owned());
            }
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
        });
    });
    if chosen.is_some() || cancel || !*open {
        *open = false;
        ctx.data_mut(|d| d.remove::<String>(id));
    } else {
        ctx.data_mut(|d| d.insert_temp(id, text));
    }
    chosen
}

/// Set Project Folder: the folder that holds (or will hold) the project.
pub fn project_folder(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    if app.native_file_dialogs {
        let start = app.state.project_folder.clone().unwrap_or_else(|| app.file_dir.clone());
        if let Some(dir) = rfd::FileDialog::new().set_title("Choose a Project Folder").set_directory(start).pick_folder() {
            if let Err(e) = app.dispatch(Command::SetProjectFolder { path: Some(dir.to_string_lossy().into_owned()) }) {
                app.state.status_prompt = e;
            }
        }
        *open = false;
        return;
    }
    let start = app.state.project_folder.clone();
    if let Some(dir) = folder_prompt(ctx, app, "Choose a Project Folder", "project_folder_text", start, open) {
        if let Err(e) = app.dispatch(Command::SetProjectFolder { path: Some(dir) }) {
            app.state.status_prompt = e;
        }
    }
}

#[derive(Clone, Default)]
struct TrackingState {
    selected: Option<String>,
    new_path: String,
}

/// Asset Tracking: every file the scene names with its status; Set Path
/// repaths the highlighted file, Strip Path keeps only its name.
pub fn asset_tracking(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let key = egui::Id::new("asset_tracking_state");
    let mut st: TrackingState = ctx.data(|d| d.get_temp(key)).unwrap_or_default();
    let assets = loom_commands::assets::scene_assets(&app.state);
    let mut repath: Option<(String, String)> = None;
    egui::Window::new("Asset Tracking").open(open).resizable(true).default_width(560.0).show(ctx, |ui| {
        if let Some(p) = &app.state.project_folder {
            ui.label(format!("Project Folder: {}", p.display()));
        }
        ui.label(format!("Scene: {}", app.state.file.as_ref().map(|f| f.display().to_string()).unwrap_or_else(|| "Untitled".into())));
        ui.separator();
        if assets.is_empty() {
            ui.label("The scene names no files.");
        }
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            egui::Grid::new("asset_tracking_rows").striped(true).show(ui, |ui| {
                ui.label(egui::RichText::new("Name").strong());
                ui.label(egui::RichText::new("Status").strong());
                ui.label(egui::RichText::new("Full Path").strong());
                ui.end_row();
                for a in &assets {
                    let p = std::path::Path::new(a);
                    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| a.clone());
                    if ui.selectable_label(st.selected.as_deref() == Some(a.as_str()), name).clicked() {
                        st.selected = Some(a.clone());
                        st.new_path = a.clone();
                    }
                    let found = p.exists();
                    ui.colored_label(if found { egui::Color32::from_rgb(120, 200, 120) } else { egui::Color32::from_rgb(230, 90, 90) }, if found { "OK" } else { "Missing" });
                    ui.label(a);
                    ui.end_row();
                }
            });
        });
        ui.separator();
        ui.add_enabled_ui(st.selected.is_some(), |ui| {
            ui.horizontal(|ui| {
                ui.label("Path:");
                ui.add(egui::TextEdit::singleline(&mut st.new_path).desired_width(360.0));
            });
            ui.horizontal(|ui| {
                if let Some(sel) = st.selected.clone() {
                    if ui.button("Set Path").clicked() && !st.new_path.trim().is_empty() && st.new_path.trim() != sel {
                        repath = Some((sel.clone(), st.new_path.trim().to_owned()));
                    }
                    if ui.button("Strip Path").clicked() {
                        let name = std::path::Path::new(&sel).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        if name != sel {
                            repath = Some((sel.clone(), name));
                        }
                    }
                } else {
                    let _ = ui.button("Set Path");
                    let _ = ui.button("Strip Path");
                }
            });
        });
    });
    if let Some((from, to)) = repath {
        match app.dispatch(Command::RepathAsset { from, to: to.clone() }) {
            Ok(_) => {
                st.selected = Some(to.clone());
                st.new_path = to;
            }
            Err(e) => app.state.status_prompt = e,
        }
    }
    if *open {
        ctx.data_mut(|d| d.insert_temp(key, st));
    } else {
        ctx.data_mut(|d| d.remove::<TrackingState>(key));
    }
}

/// Missing External Files: the files the opened scene names that are not
/// there; Browse looks for them by name in a folder, Continue opens anyway.
pub fn missing_files(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let missing = loom_commands::assets::missing_assets(&app.state);
    if missing.is_empty() {
        *open = false;
        return;
    }
    let key = egui::Id::new("missing_files_folder");
    let mut folder: Option<String> = ctx.data(|d| d.get_temp(key));
    let mut done = false;
    let mut find: Option<String> = None;
    egui::Window::new("Missing External Files").open(open).collapsible(false).resizable(true).default_width(480.0).show(ctx, |ui| {
        ui.label("The scene names files that are not there:");
        egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
            for m in &missing {
                ui.label(m);
            }
        });
        if let Some(f) = folder.as_mut() {
            ui.horizontal(|ui| {
                ui.label("Look in:");
                ui.add(egui::TextEdit::singleline(f).desired_width(320.0));
                if ui.button("Find").clicked() {
                    find = Some(f.trim().to_owned());
                }
            });
        }
        ui.horizontal(|ui| {
            if ui.button("Continue").clicked() {
                done = true;
            }
            if ui.button("Browse").clicked() {
                if app.native_file_dialogs {
                    if let Some(dir) = rfd::FileDialog::new().set_title("Find Missing Files").set_directory(app.file_dir.clone()).pick_folder() {
                        find = Some(dir.to_string_lossy().into_owned());
                    }
                } else if folder.is_none() {
                    folder = Some(app.state.file.as_ref().and_then(|f| f.parent()).map(|d| d.to_string_lossy().into_owned()).unwrap_or_default());
                }
            }
        });
    });
    if let Some(dir) = find {
        if let Err(e) = app.dispatch(Command::FindMissingFiles { folder: dir }) {
            app.state.status_prompt = e;
        }
    }
    if done {
        *open = false;
    }
    if *open {
        if let Some(f) = folder {
            ctx.data_mut(|d| d.insert_temp(key, f));
        }
    } else {
        ctx.data_mut(|d| d.remove::<String>(key));
    }
}
