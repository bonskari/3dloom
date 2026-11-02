//! File > References > XRef Scene / XRef Objects: the XRef Scenes and XRef
//! Objects dialogs. The list holds the referenced files (an XRef Objects
//! record also lists its objects); Add / Create XRef Record from File picks
//! another file, Remove drops the chosen references and their objects,
//! All / None / Invert change the choice, Automatic re-reads a file when it
//! changes on disk and Update Now re-reads the chosen ones.

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
            if ui.add_enabled(!chosen.is_empty(), egui::Button::new("Remove")).clicked() {
                run.extend(chosen.iter().map(|p| Command::RemoveXref { path: p.clone() }));
            }
        });
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_min_size(egui::vec2(360.0, 120.0));
            for r in &records {
                let on = chosen.contains(&r.path);
                if ui.selectable_label(on, r.path.as_str()).clicked() {
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
    ctx.data_mut(|d| d.insert_temp(chosen_id, chosen));
    for c in run {
        if let Err(e) = app.dispatch(c) {
            app.state.status_prompt = e;
        }
    }
}
