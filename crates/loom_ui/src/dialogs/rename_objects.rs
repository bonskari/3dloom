//! Tools > Rename Objects: Selected / Pick, Base Name, Prefix, Remove First,
//! Suffix, Remove Last, Numbered (Base Number, Step), Rename.

use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;

#[derive(Clone, Default)]
struct Form {
    pick: bool,
    picked: Vec<u64>,
    base: (bool, String),
    prefix: (bool, String),
    remove_first: (bool, u32),
    suffix: (bool, String),
    remove_last: (bool, u32),
    numbered: bool,
    base_number: i64,
    step: i64,
}

fn text_option(ui: &mut egui::Ui, label: &str, v: &mut (bool, String)) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut v.0, label);
        ui.add_enabled(v.0, egui::TextEdit::singleline(&mut v.1).desired_width(160.0).id_salt(label));
    });
}

fn count_option(ui: &mut egui::Ui, label: &str, v: &mut (bool, u32)) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut v.0, label);
        ui.add_enabled_ui(v.0, |ui| {
            ui.add(crate::spinner::spin(ui, &mut v.1, 0..=999));
            ui.label("Digits");
        });
    });
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let id = egui::Id::new("rename_objects_form");
    let mut f: Form = ctx.data(|d| d.get_temp(id)).unwrap_or(Form { step: 1, ..Default::default() });
    egui::Window::new("Rename Objects").open(open).resizable(false).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.radio_value(&mut f.pick, false, "Selected");
            ui.radio_value(&mut f.pick, true, "Pick");
        });
        if f.pick {
            // Pick: tick the objects to rename.
            egui::ScrollArea::vertical().max_height(140.0).show(ui, |ui| {
                let nodes: Vec<(u64, String)> = app.state.scene.nodes().map(|n| (n.id.0, n.name.clone())).collect();
                for (nid, name) in nodes {
                    let mut on = f.picked.contains(&nid);
                    if ui.checkbox(&mut on, name).changed() {
                        f.picked.retain(|x| *x != nid);
                        if on {
                            f.picked.push(nid);
                        }
                    }
                }
            });
        }
        ui.separator();
        text_option(ui, "Base Name:", &mut f.base);
        text_option(ui, "Prefix:", &mut f.prefix);
        count_option(ui, "Remove First:", &mut f.remove_first);
        text_option(ui, "Suffix:", &mut f.suffix);
        count_option(ui, "Remove Last:", &mut f.remove_last);
        ui.horizontal(|ui| {
            ui.checkbox(&mut f.numbered, "Numbered");
            ui.add_enabled_ui(f.numbered, |ui| {
                ui.label("Base Number:");
                ui.add(crate::spinner::spin(ui, &mut f.base_number, ..));
                ui.label("Step:");
                ui.add(crate::spinner::spin(ui, &mut f.step, ..));
            });
        });
        ui.separator();
        let count = if f.pick { f.picked.len() } else { app.state.selection.len() };
        if ui.add_enabled(count > 0, egui::Button::new("Rename")).clicked() {
            let opt = |v: &(bool, String)| v.0.then(|| v.1.clone());
            let cmd = Command::RenameObjects {
                ids: f.pick.then(|| f.picked.clone()),
                base_name: opt(&f.base),
                prefix: opt(&f.prefix),
                remove_first: f.remove_first.0.then_some(f.remove_first.1 as usize),
                suffix: opt(&f.suffix),
                remove_last: f.remove_last.0.then_some(f.remove_last.1 as usize),
                base_number: f.numbered.then_some(f.base_number),
                step: f.numbered.then_some(f.step),
            };
            if let Err(e) = app.dispatch(cmd) {
                app.state.status_prompt = e;
            }
        }
    });
    ctx.data_mut(|d| d.insert_temp(id, f));
}
