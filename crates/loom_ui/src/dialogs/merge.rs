//! File > Merge: the objects of the chosen file, highlighted like Select
//! From Scene (click, Ctrl+click, Shift+click, All / None / Invert); nothing
//! starts highlighted. A highlighted name the scene already has asks the
//! Duplicate Name question (Merge, Skip, Delete Old, Rename, Auto-Rename,
//! Apply to all duplicates) before the merge runs as one undo step.

use super::select_from_scene::{list_click, name_matches};
use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, MergeDuplicate};
use std::collections::BTreeSet;

#[derive(Default, Clone)]
struct St {
    chosen: BTreeSet<usize>,
    anchor: Option<usize>,
    find: String,
    /// After OK: the chosen names, the duplicates still to answer and the answers so far.
    asking: Option<Asking>,
}

#[derive(Clone)]
struct Asking {
    names: Vec<String>,
    pending: Vec<String>,
    answers: Vec<MergeDuplicate>,
    rename: String,
    apply_to_all: bool,
}

fn state_id() -> egui::Id {
    egui::Id::new("merge_dialog").with("state")
}

/// Forgets the highlight (called when any dialog closes).
pub fn clear_state(d: &mut egui::util::IdTypeMap) {
    d.remove::<St>(state_id());
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let Some((path, names)) = app.state.merge_pending.clone() else {
        *open = false;
        return;
    };
    let mut st: St = ctx.data(|d| d.get_temp(state_id())).unwrap_or_default();
    let mut done = false;
    let mut run: Option<(Vec<String>, Vec<MergeDuplicate>)> = None;
    let order: Vec<usize> = (0..names.len()).collect();
    let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if let Some(ask) = st.asking.as_mut() {
        // Duplicate Name: one question per name the scene already has.
        let name = ask.pending[0].clone();
        let mut answer: Option<MergeDuplicate> = None;
        let mut cancel = false;
        egui::Window::new("Duplicate Name").collapsible(false).resizable(false).show(ctx, |ui| {
            ui.label(format!("An object named {name:?} already exists in the scene."));
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut ask.rename);
                let free = !ask.rename.trim().is_empty() && app.state.scene.nodes().all(|n| n.name != ask.rename.trim());
                if ui.add_enabled(free, egui::Button::new("Rename")).clicked() {
                    answer = Some(MergeDuplicate { name: name.clone(), action: "rename".into(), new_name: Some(ask.rename.trim().to_owned()) });
                }
            });
            ui.horizontal(|ui| {
                for (label, action) in [("Merge", "merge"), ("Skip", "skip"), ("Delete Old", "delete_old"), ("Auto-Rename", "auto_rename")] {
                    if ui.button(label).clicked() {
                        answer = Some(MergeDuplicate { name: name.clone(), action: action.into(), new_name: None });
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut ask.apply_to_all, "Apply to all duplicates");
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
        if cancel {
            st.asking = None;
        } else if let Some(a) = answer {
            ask.pending.remove(0);
            if ask.apply_to_all && a.action != "rename" {
                for rest in ask.pending.drain(..) {
                    ask.answers.push(MergeDuplicate { name: rest, action: a.action.clone(), new_name: None });
                }
            }
            ask.answers.push(a);
            match ask.pending.first() {
                Some(next) => ask.rename = next.clone(),
                None => run = Some((ask.names.clone(), ask.answers.clone())),
            }
        }
    } else {
        let title = if app.state.merge_as_xref { "XRef Merge" } else { "Merge" };
        egui::Window::new(format!("{title} - {file_name}")).open(open).resizable(true).default_width(320.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Find:");
                if ui.text_edit_singleline(&mut st.find).changed() && !st.find.is_empty() {
                    st.chosen = order.iter().copied().filter(|i| name_matches(&st.find, &names[*i], false)).collect();
                }
            });
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                for (i, name) in names.iter().enumerate() {
                    if ui.selectable_label(st.chosen.contains(&i), name).clicked() {
                        let mods = ui.input(|inp| inp.modifiers);
                        list_click(&mut st.chosen, &mut st.anchor, &order, i, mods);
                    }
                }
            });
            ui.horizontal(|ui| {
                if ui.button("All").clicked() {
                    st.chosen = order.iter().copied().collect();
                }
                if ui.button("None").clicked() {
                    st.chosen.clear();
                }
                if ui.button("Invert").clicked() {
                    st.chosen = order.iter().copied().filter(|i| !st.chosen.contains(i)).collect();
                }
            });
            ui.horizontal(|ui| {
                if ui.add_enabled(!st.chosen.is_empty(), egui::Button::new("OK")).clicked() {
                    let chosen: Vec<String> = st.chosen.iter().map(|i| names[*i].clone()).collect();
                    let mut pending: Vec<String> = chosen.iter().filter(|n| app.state.scene.nodes().any(|x| &x.name == *n)).cloned().collect();
                    pending.dedup();
                    // XRef objects keep their file names (renamed here when taken), no question.
                    if pending.is_empty() || app.state.merge_as_xref {
                        run = Some((chosen, Vec::new()));
                    } else {
                        st.asking = Some(Asking { names: chosen, rename: pending[0].clone(), pending, answers: Vec::new(), apply_to_all: false });
                    }
                }
                if ui.button("Cancel").clicked() {
                    done = true;
                }
            });
        });
    }
    if let Some((chosen, answers)) = run {
        let p = path.to_string_lossy().into_owned();
        let cmd = if app.state.merge_as_xref { Command::XrefObjects { path: Some(p), names: Some(chosen), automatic: None } } else { Command::MergeFile { path: Some(p), names: Some(chosen), duplicates: Some(answers) } };
        match app.dispatch(cmd) {
            Ok(_) => done = true,
            Err(e) => {
                app.state.status_prompt = e;
                st.asking = None;
            }
        }
    }
    if done {
        *open = false;
        app.state.merge_pending = None;
        app.state.merge_as_xref = false;
        ctx.data_mut(|d| d.remove::<St>(state_id()));
    } else {
        ctx.data_mut(|d| d.insert_temp(state_id(), st));
    }
}
