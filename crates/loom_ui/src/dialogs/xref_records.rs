//! File > References > XRef Scene / XRef Objects: the XRef Scenes and XRef
//! Objects dialogs. The list holds the referenced files (an XRef Objects
//! record also lists its objects); Add / Create XRef Record from File picks
//! another file, Remove drops the chosen references and their objects,
//! All / None / Invert change the choice, Automatic re-reads a file when it
//! changes on disk and Update Now re-reads the chosen ones. The path field
//! (and Browse) points the chosen reference at another file; Enabled unloads
//! or reloads it, Overlay keeps it out of scenes that reference this one,
//! Merge makes its objects this scene's own; the XRef Scenes dialog's
//! Convert Selected turns the selection into a new referenced file.

use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;

pub fn show(ctx: &egui::Context, app: &mut LoomApp, scene: bool, open: &mut bool) {
    let chosen_id = egui::Id::new("xref_records_chosen").with(scene);
    let records: Vec<loom_scene::XrefRecord> = app.state.scene.xrefs.iter().filter(|r| r.scene == scene).cloned().collect();
    let mut chosen: Vec<String> = ctx.data(|d| d.get_temp(chosen_id)).unwrap_or_default();
    chosen.retain(|p| records.iter().any(|r| &r.path == p));
    let mut run: Vec<Command> = Vec::new();
    let title = if scene { "XRef Scenes" } else { "XRef Objects" };
    egui::Window::new(title).open(open).resizable(false).collapsible(false).show(ctx, |ui| {
        ui.horizontal(|ui| {
            let add = if scene { "Add..." } else { "Create XRef Record from File" };
            if ui.button(add).clicked() {
                run.push(if scene { Command::XrefScene { path: None, automatic: None } } else { Command::XrefObjects { path: None, names: None, automatic: None } });
            }
            if scene && ui.add_enabled(!app.state.selection.is_empty(), egui::Button::new("Convert Selected")).clicked() {
                run.push(Command::ConvertSelectedToXref { path: None });
            }
            if ui.add_enabled(!chosen.is_empty(), egui::Button::new("Remove")).clicked() {
                run.extend(chosen.iter().map(|p| Command::RemoveXref { path: p.clone() }));
            }
        });
        // The path of the one chosen reference: type another file (Enter) or Browse.
        let one = match chosen.as_slice() {
            [p] => Some(p.clone()),
            _ => None,
        };
        let field_id = egui::Id::new("xref_records_path").with(scene);
        ui.horizontal(|ui| {
            let mut text: String = ctx.data(|d| d.get_temp::<(Option<String>, String)>(field_id)).filter(|(of, _)| *of == one).map(|(_, t)| t).unwrap_or_else(|| one.clone().unwrap_or_default());
            let r = ui.add_enabled(one.is_some(), egui::TextEdit::singleline(&mut text).desired_width(300.0));
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                if let Some(p) = &one {
                    if &text != p {
                        run.push(Command::SetXrefPath { path: Some(p.clone()), scene, new_path: text.clone() });
                    }
                }
            }
            ctx.data_mut(|d| d.insert_temp(field_id, (one.clone(), text)));
            if ui.add_enabled(one.is_some(), egui::Button::new("Browse...")).clicked() {
                if let Some(p) = &one {
                    run.push(Command::XrefBrowse { path: p.clone(), scene });
                }
            }
        });
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_min_size(egui::vec2(360.0, 120.0));
            for r in &records {
                let on = chosen.contains(&r.path);
                // Disabled references are listed grey.
                let text = if r.enabled { egui::RichText::new(r.path.as_str()) } else { egui::RichText::new(r.path.as_str()).weak() };
                if ui.selectable_label(on, text).clicked() {
                    if ui.input(|i| i.modifiers.command) {
                        if on {
                            chosen.retain(|p| p != &r.path);
                        } else {
                            chosen.push(r.path.clone());
                        }
                    } else {
                        chosen = vec![r.path.clone()];
                    }
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.button("All").clicked() {
                chosen = records.iter().map(|r| r.path.clone()).collect();
            }
            if ui.button("None").clicked() {
                chosen.clear();
            }
            if ui.button("Invert").clicked() {
                chosen = records.iter().map(|r| r.path.clone()).filter(|p| !chosen.contains(p)).collect();
            }
        });
        if !scene {
            // The chosen record's objects.
            if let [one] = chosen.as_slice() {
                if let Some(r) = records.iter().find(|r| &r.path == one) {
                    ui.label("Objects:");
                    for (name, _) in &r.nodes {
                        ui.label(name.as_str());
                    }
                }
            }
        }
        ui.add_enabled_ui(!chosen.is_empty(), |ui| {
            let picked: Vec<&loom_scene::XrefRecord> = records.iter().filter(|r| chosen.contains(&r.path)).collect();
            ui.horizontal(|ui| {
                let mut enabled = !picked.is_empty() && picked.iter().all(|r| r.enabled);
                if ui.checkbox(&mut enabled, "Enabled").changed() {
                    run.extend(chosen.iter().map(|p| Command::SetXrefEnabled { path: p.clone(), scene, enabled }));
                }
                let mut overlay = !picked.is_empty() && picked.iter().all(|r| r.overlay);
                if ui.checkbox(&mut overlay, "Overlay").changed() {
                    run.extend(chosen.iter().map(|p| Command::SetXrefOverlay { path: p.clone(), scene, overlay }));
                }
                if ui.button("Merge").clicked() {
                    ctx.data_mut(|d| d.insert_temp(egui::Id::new("xref_merge_confirm").with(scene), chosen.clone()));
                }
            });
            ui.horizontal(|ui| {
                let all_auto = !chosen.is_empty() && records.iter().filter(|r| chosen.contains(&r.path)).all(|r| r.automatic);
                let mut auto = all_auto;
                if ui.checkbox(&mut auto, "Automatic").changed() {
                    run.extend(chosen.iter().map(|p| Command::SetXrefAutomatic { path: p.clone(), automatic: auto }));
                }
                if ui.button("Update Now").clicked() {
                    run.extend(chosen.iter().map(|p| Command::UpdateXrefs { path: Some(p.clone()) }));
                }
            });
        });
    });
    // Merge asks first.
    let confirm_id = egui::Id::new("xref_merge_confirm").with(scene);
    let pending: Vec<String> = ctx.data(|d| d.get_temp(confirm_id)).unwrap_or_default();
    if !pending.is_empty() {
        let mut answer = None;
        egui::Window::new("Merge XRef").collapsible(false).resizable(false).show(ctx, |ui| {
            ui.label(format!("Merge {} into the scene? It will no longer be a reference.", pending.join(", ")));
            ui.horizontal(|ui| {
                if ui.button("Yes").clicked() {
                    answer = Some(true);
                }
                if ui.button("No").clicked() {
                    answer = Some(false);
                }
            });
        });
        if let Some(yes) = answer {
            if yes {
                run.extend(pending.iter().map(|p| Command::MergeXref { path: p.clone(), scene }));
                chosen.retain(|p| !pending.contains(p));
            }
            ctx.data_mut(|d| d.remove::<Vec<String>>(confirm_id));
        }
    }
    ctx.data_mut(|d| d.insert_temp(chosen_id, chosen));
    for c in run {
        if let Err(e) = app.dispatch(c) {
            app.state.status_prompt = e;
        }
    }
}
