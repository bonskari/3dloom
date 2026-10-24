//! Select From Scene dialog (H), also used for Hide/Unhide/Freeze/Unfreeze
//! by Name with the matching title and action.

use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, DialogPurpose};
use loom_scene::NodeId;
use std::collections::BTreeSet;

#[derive(Default, Clone)]
struct State {
    search: String,
    chosen: BTreeSet<u64>,
    geometry: bool,
    cameras: bool,
    show_hidden: bool,
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let purpose = app.state.dialog_purpose;
    let title = match purpose {
        DialogPurpose::Select => "Select From Scene",
        DialogPurpose::Hide => "Hide Objects",
        DialogPurpose::Unhide => "Unhide Objects",
        DialogPurpose::Freeze => "Freeze Objects",
        DialogPurpose::Unfreeze => "Unfreeze Objects",
        DialogPurpose::Attach => "Attach List",
        DialogPurpose::SkinAddBones => "Select Bones",
    };
    let id = egui::Id::new(STATE_ID);
    // Select From Scene opens with the current selection highlighted, as in Max.
    let mut st: State = ctx.data(|d| d.get_temp(id)).unwrap_or_else(|| State {
        geometry: true,
        cameras: true,
        chosen: if purpose == DialogPurpose::Select { app.state.selection.iter().map(|n| n.0).collect() } else { BTreeSet::new() },
        ..Default::default()
    });
    let mut done = false;
    let mut apply = false;
    egui::Window::new(title).open(open).default_width(380.0).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label("Find:");
            ui.text_edit_singleline(&mut st.search);
            ui.checkbox(&mut st.geometry, "Geometry");
            ui.checkbox(&mut st.cameras, "Cameras");
        });
        ui.horizontal(|ui| {
            ui.label("Display:");
            ui.checkbox(&mut st.show_hidden, "Hidden");
            // Select All / Invert act on the listed names only.
            let listed: BTreeSet<u64> = rows(app, purpose, &st).into_iter().map(|r| r.0).collect();
            if ui.button("Select All").clicked() {
                st.chosen = listed.clone();
            }
            if ui.button("Select None").clicked() {
                st.chosen.clear();
            }
            if ui.button("Select Invert").clicked() {
                st.chosen = listed.difference(&st.chosen).copied().collect();
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
            egui::Grid::new("sfs_rows").num_columns(3).striped(true).show(ui, |ui| {
                ui.label(egui::RichText::new("Name").strong());
                ui.label(egui::RichText::new("Type").strong());
                ui.label(egui::RichText::new("State").strong());
                ui.end_row();
                for (nid, name, kind, hidden, frozen) in rows(app, purpose, &st) {
                    let on = st.chosen.contains(&nid);
                    // Click picks only this name, Ctrl+click toggles it, a double-click applies.
                    let r = ui.selectable_label(on, &name);
                    if r.double_clicked() {
                        st.chosen = BTreeSet::from([nid]);
                        apply = true;
                    } else if r.clicked() {
                        if ui.input(|i| i.modifiers.command) {
                            if on {
                                st.chosen.remove(&nid);
                            } else {
                                st.chosen.insert(nid);
                            }
                        } else {
                            st.chosen = BTreeSet::from([nid]);
                        }
                    }
                    ui.label(kind);
                    ui.label(match (hidden, frozen) {
                        (true, true) => "hidden, frozen",
                        (true, false) => "hidden",
                        (false, true) => "frozen",
                        _ => "",
                    });
                    ui.end_row();
                }
            });
        });
        ui.separator();
        ui.horizontal(|ui| {
            let verb = match purpose {
                DialogPurpose::Select => "Select",
                DialogPurpose::Hide => "Hide",
                DialogPurpose::Unhide => "Unhide",
                DialogPurpose::Freeze => "Freeze",
                DialogPurpose::Unfreeze => "Unfreeze",
                DialogPurpose::Attach => "Attach",
                DialogPurpose::SkinAddBones => "Select",
            };
            if ui.button(verb).clicked() || apply {
                let ids: Vec<u64> = st.chosen.iter().copied().collect();
                if purpose == DialogPurpose::Attach {
                    for id in &ids {
                        let _ = app.dispatch(Command::AttachNode { id: *id });
                    }
                    done = true;
                }
                let _ = app.dispatch(match purpose {
                    DialogPurpose::Attach => Command::CloseDialog,
                    DialogPurpose::SkinAddBones => Command::SkinAddBones { ids },
                    DialogPurpose::Select => Command::SelectNodes { ids, op: loom_commands::SelectOp::Replace },
                    DialogPurpose::Hide => Command::HideNodes { ids },
                    DialogPurpose::Unhide => Command::UnhideNodes { ids },
                    DialogPurpose::Freeze => Command::FreezeNodes { ids },
                    DialogPurpose::Unfreeze => Command::UnfreezeNodes { ids },
                });
                done = true;
            }
            if ui.button("Cancel").clicked() {
                done = true;
            }
        });
    });
    if done {
        *open = false;
    }
    ctx.data_mut(|d| if *open { d.insert_temp(id, st) } else { d.remove::<State>(id) });
    let _ = NodeId(0);
}

const STATE_ID: &str = "select_from_scene_state";

/// Forgets the dialog's session (chosen names, filters) so it reopens fresh.
pub fn clear_state(d: &mut egui::util::IdTypeMap) {
    d.remove::<State>(egui::Id::new(STATE_ID));
}

/// The listed objects: (id, name, type, hidden, frozen) after the filters.
fn rows(app: &LoomApp, purpose: DialogPurpose, st: &State) -> Vec<(u64, String, &'static str, bool, bool)> {
    app.state
        .scene
        .nodes()
        .map(|n| (n.id.0, n.name.clone(), n.object.base_name(), n.hidden, n.frozen))
        .filter(|(nid, name, kind, hidden, _)| {
            let is_cam = *kind == "Camera";
            if (is_cam && !st.cameras) || (!is_cam && !st.geometry) {
                return false;
            }
            // Attach List: the object being attached to is not a candidate.
            if purpose == DialogPurpose::Attach && app.state.selection.contains(&loom_scene::NodeId(*nid)) {
                return false;
            }
            let hidden_ok = match purpose {
                DialogPurpose::Unhide => true,
                _ => st.show_hidden || !hidden,
            };
            hidden_ok && (st.search.is_empty() || name.to_lowercase().contains(&st.search.to_lowercase()))
        })
        .collect()
}
