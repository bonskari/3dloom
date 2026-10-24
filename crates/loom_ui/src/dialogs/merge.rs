//! File > Merge: the objects of the chosen file, with All / None / Invert.

use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;
use std::collections::BTreeSet;

/// Forgets the checked set (called when any dialog closes).
pub fn clear_state(d: &mut egui::util::IdTypeMap) {
    d.remove::<BTreeSet<String>>(egui::Id::new("merge_dialog").with("checked"));
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let Some((path, names)) = app.state.merge_pending.clone() else {
        *open = false;
        return;
    };
    let id = egui::Id::new("merge_dialog").with("checked");
    let mut checked: BTreeSet<String> = ctx.data(|d| d.get_temp(id)).unwrap_or_else(|| names.iter().cloned().collect());
    let mut done = false;
    let title = format!("Merge - {}", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    egui::Window::new(title).open(open).resizable(true).default_width(320.0).show(ctx, |ui| {
        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
            for name in &names {
                let mut on = checked.contains(name);
                if ui.checkbox(&mut on, name).changed() {
                    if on {
                        checked.insert(name.clone());
                    } else {
                        checked.remove(name);
                    }
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.button("All").clicked() {
                checked = names.iter().cloned().collect();
            }
            if ui.button("None").clicked() {
                checked.clear();
            }
            if ui.button("Invert").clicked() {
                checked = names.iter().filter(|n| !checked.contains(*n)).cloned().collect();
            }
        });
        ui.horizontal(|ui| {
            if ui.add_enabled(!checked.is_empty(), egui::Button::new("OK")).clicked() {
                let chosen: Vec<String> = names.iter().filter(|n| checked.contains(*n)).cloned().collect();
                match app.dispatch(Command::MergeFile { path: Some(path.to_string_lossy().into_owned()), names: Some(chosen) }) {
                    Ok(_) => done = true,
                    Err(e) => app.state.status_prompt = e,
                }
            }
            if ui.button("Cancel").clicked() {
                done = true;
            }
        });
    });
    if done {
        *open = false;
        app.state.merge_pending = None;
        ctx.data_mut(|d| d.remove::<BTreeSet<String>>(id));
    } else {
        ctx.data_mut(|d| d.insert_temp(id, checked));
    }
}
