//! File menu dialogs: New Scene, the Reset confirmation, Summary Info,
//! File Properties; and Group > Group's name prompt.

use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, NewKeep, PendingFileAction};

/// New Scene: Keep Objects and Hierarchy / Keep Objects / New All (the default).
pub fn new_scene(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let id = egui::Id::new("new_scene_choice");
    let mut choice: u8 = ctx.data(|d| d.get_temp(id)).unwrap_or(2);
    let mut action = None;
    let mut done = false;
    egui::Window::new("New Scene").open(open).collapsible(false).resizable(false).show(ctx, |ui| {
        ui.radio_value(&mut choice, 0, "Keep Objects and Hierarchy");
        ui.radio_value(&mut choice, 1, "Keep Objects");
        ui.radio_value(&mut choice, 2, "New All");
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                action = Some(match choice {
                    0 => PendingFileAction::NewKeeping(NewKeep::ObjectsAndHierarchy),
                    1 => PendingFileAction::NewKeeping(NewKeep::Objects),
                    _ => PendingFileAction::New,
                });
            }
            if ui.button("Cancel").clicked() {
                done = true;
            }
        });
    });
    ctx.data_mut(|d| d.insert_temp(id, choice));
    if let Some(a) = action {
        app.state.dialog = None;
        // The save prompt (when the scene has changes) comes next.
        app.request_file_action(a);
    } else if done {
        *open = false;
    }
}

/// Reset: "Do you really want to reset?" Yes / No.
pub fn confirm_reset(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let mut done = false;
    egui::Window::new("3dLoom").id(egui::Id::new("confirm_reset")).open(open).collapsible(false).resizable(false).show(ctx, |ui| {
        ui.label("Do you really want to reset?");
        ui.horizontal(|ui| {
            if ui.button("Yes").clicked() {
                let _ = app.dispatch(Command::ResetFile);
                done = true;
            }
            if ui.button("No").clicked() {
                done = true;
            }
        });
    });
    if done {
        *open = false;
    }
}

/// Summary Info: scene totals, the description (saved with the scene) and the materials.
pub fn summary_info(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let info = loom_commands::scene_file::summary_info(&app.state);
    let desc_id = egui::Id::new("summary_info_description");
    let mut desc: String = ctx.data(|d| d.get_temp(desc_id)).unwrap_or_else(|| app.state.file_properties.description.clone());
    let mut done = false;
    egui::Window::new("Summary Info").open(open).resizable(true).default_width(380.0).show(ctx, |ui| {
        ui.label(format!("Scene Name: {}", app.state.file.as_ref().and_then(|f| f.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".into())));
        egui::Grid::new("summary_info_totals").num_columns(2).spacing([16.0, 2.0]).show(ui, |ui| {
            for (label, key) in [("Objects", "objects"), ("Shapes", "shapes"), ("Cameras", "cameras"), ("Helpers", "helpers"), ("Groups", "groups"), ("Total", "total"), ("Vertices", "vertices"), ("Faces", "faces"), ("Layers", "layers")] {
                ui.label(label);
                ui.label(info[key].to_string());
                ui.end_row();
            }
        });
        ui.label("Description:");
        ui.add(egui::TextEdit::multiline(&mut desc).desired_rows(3).desired_width(f32::INFINITY));
        ui.label("Materials:");
        let materials = info["materials"].as_array().cloned().unwrap_or_default();
        if materials.is_empty() {
            ui.label("(none)");
        }
        for m in materials {
            ui.label(m.as_str().unwrap_or_default());
        }
        ui.separator();
        if ui.button("OK").clicked() {
            done = true;
        }
    });
    if desc != app.state.file_properties.description {
        ctx.data_mut(|d| d.insert_temp(desc_id, desc.clone()));
    }
    if done || !*open {
        if desc != app.state.file_properties.description {
            let _ = app.dispatch(Command::SetFileProperty { name: "Description".into(), value: desc });
        }
        ctx.data_mut(|d| d.remove::<String>(desc_id));
        *open = false;
    }
}

#[derive(Clone, Default)]
struct PropsEdit {
    tab: u8,
    fields: Vec<String>,
    custom_name: String,
    custom_value: String,
}

/// File Properties: Summary (Title … Comments), Contents (read-only) and Custom tabs.
pub fn file_properties(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    const SUMMARY: [&str; 8] = ["Title", "Subject", "Author", "Manager", "Company", "Category", "Keywords", "Comments"];
    let id = egui::Id::new("file_properties_edit");
    let props = app.state.file_properties.clone();
    let mut st: PropsEdit = ctx.data(|d| d.get_temp(id)).unwrap_or_else(|| PropsEdit { fields: SUMMARY.iter().map(|f| props.field(f).unwrap_or_default().to_owned()).collect(), ..Default::default() });
    let mut ok = false;
    let mut cancel = false;
    egui::Window::new("File Properties").open(open).resizable(true).default_width(400.0).show(ctx, |ui| {
        ui.horizontal(|ui| {
            for (i, t) in ["Summary", "Contents", "Custom"].iter().enumerate() {
                if ui.selectable_label(st.tab == i as u8, *t).clicked() {
                    st.tab = i as u8;
                }
            }
        });
        ui.separator();
        match st.tab {
            0 => {
                egui::Grid::new("file_properties_summary").num_columns(2).show(ui, |ui| {
                    for (i, f) in SUMMARY.iter().enumerate() {
                        ui.label(format!("{f}:"));
                        ui.add(egui::TextEdit::singleline(&mut st.fields[i]).desired_width(260.0));
                        ui.end_row();
                    }
                });
            }
            1 => {
                let info = loom_commands::scene_file::summary_info(&app.state);
                for (label, key) in [("Objects", "objects"), ("Shapes", "shapes"), ("Cameras", "cameras"), ("Helpers", "helpers"), ("Groups", "groups"), ("Vertices", "vertices"), ("Faces", "faces")] {
                    ui.label(format!("{label}: {}", info[key]));
                }
                ui.label("Materials:");
                for m in info["materials"].as_array().cloned().unwrap_or_default() {
                    ui.label(format!("  {}", m.as_str().unwrap_or_default()));
                }
            }
            _ => {
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.add(egui::TextEdit::singleline(&mut st.custom_name).desired_width(120.0));
                    ui.label("Value:");
                    ui.add(egui::TextEdit::singleline(&mut st.custom_value).desired_width(120.0));
                    if ui.add_enabled(!st.custom_name.trim().is_empty(), egui::Button::new("Add")).clicked() {
                        let _ = app.dispatch(Command::SetCustomFileProperty { name: st.custom_name.clone(), value: Some(st.custom_value.clone()) });
                        st.custom_name.clear();
                        st.custom_value.clear();
                    }
                });
                egui::Grid::new("file_properties_custom").num_columns(3).striped(true).show(ui, |ui| {
                    for (k, v) in &props.custom {
                        ui.label(k);
                        ui.label(v);
                        if ui.button("Delete").clicked() {
                            let _ = app.dispatch(Command::SetCustomFileProperty { name: k.clone(), value: None });
                        }
                        ui.end_row();
                    }
                });
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            ok = ui.button("OK").clicked();
            cancel = ui.button("Cancel").clicked();
        });
    });
    if ok {
        for (i, f) in SUMMARY.iter().enumerate() {
            if props.field(f) != Some(st.fields[i].as_str()) {
                let _ = app.dispatch(Command::SetFileProperty { name: (*f).into(), value: st.fields[i].clone() });
            }
        }
    }
    if ok || cancel || !*open {
        ctx.data_mut(|d| d.remove::<PropsEdit>(id));
        *open = false;
    } else {
        ctx.data_mut(|d| d.insert_temp(id, st));
    }
}

/// Group > Group: "Group name:" with the next free GroupNN.
pub fn group_name(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let id = egui::Id::new("group_name_edit");
    let mut name: String = ctx.data(|d| d.get_temp(id)).unwrap_or_else(|| app.state.scene.next_name("Group"));
    let mut done = false;
    egui::Window::new("Group").open(open).collapsible(false).resizable(false).show(ctx, |ui| {
        ui.label("Group name:");
        let r = ui.text_edit_singleline(&mut name);
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.horizontal(|ui| {
            if ui.add_enabled(!name.trim().is_empty(), egui::Button::new("OK")).clicked() || (enter && !name.trim().is_empty()) {
                if let Err(e) = app.dispatch(Command::Group { name: Some(name.trim().to_owned()) }) {
                    app.state.status_prompt = e;
                }
                done = true;
            }
            if ui.button("Cancel").clicked() {
                done = true;
            }
        });
    });
    if done || !*open {
        ctx.data_mut(|d| d.remove::<String>(id));
        *open = false;
    } else {
        ctx.data_mut(|d| d.insert_temp(id, name));
    }
}

/// FBX Export (`selected` = Export Selected) or FBX Import (`None`): the
/// options, then OK writes or reads the chosen file.
pub fn fbx_options(ctx: &egui::Context, app: &mut LoomApp, selected: Option<bool>, open: &mut bool) {
    let Some(path) = app.state.fbx_pending.clone() else {
        *open = false;
        return;
    };
    let title = if selected.is_some() { "FBX Export" } else { "FBX Import" };
    let mut done = false;
    let mut ok = false;
    egui::Window::new(title).open(open).collapsible(false).resizable(false).show(ctx, |ui| {
        ui.label(path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
        ui.separator();
        let o = app.state.fbx_options;
        let mut flag = |ui: &mut egui::Ui, label: &str, name: &str, on: bool| {
            let mut v = on;
            if ui.checkbox(&mut v, label).changed() {
                let _ = app.dispatch(Command::SetFbxOption { name: name.into(), value: v });
            }
        };
        if selected.is_some() {
            ui.label(egui::RichText::new("Include").strong());
            flag(ui, "Smoothing Groups", "Smoothing Groups", o.smoothing_groups);
            flag(ui, "Animation", "Animation", o.animation);
            flag(ui, "Cameras", "Cameras", o.cameras);
            ui.label(egui::RichText::new("Axis Conversion").strong());
            ui.horizontal(|ui| {
                ui.label("Up Axis:");
                if ui.radio(o.y_up, "Y-up").clicked() && !o.y_up {
                    let _ = app.dispatch(Command::SetFbxOption { name: "Y-Up".into(), value: true });
                }
                if ui.radio(!o.y_up, "Z-up").clicked() && o.y_up {
                    let _ = app.dispatch(Command::SetFbxOption { name: "Y-Up".into(), value: false });
                }
            });
        } else {
            ui.label(egui::RichText::new("Include").strong());
            flag(ui, "Animation", "Import Animation", o.import_animation);
        }
        ui.separator();
        ui.horizontal(|ui| {
            ok = ui.button("OK").clicked();
            done = ui.button("Cancel").clicked();
        });
    });
    if ok {
        let p = Some(path.to_string_lossy().into_owned());
        let cmd = match selected {
            Some(true) => Command::ExportSelected { path: p },
            Some(false) => Command::ExportFile { path: p },
            None => Command::ImportFile { path: p },
        };
        if let Err(e) = app.dispatch(cmd) {
            app.state.status_prompt = e;
        }
        done = true;
    }
    if done {
        app.state.fbx_pending = None;
        *open = false;
    }
}
