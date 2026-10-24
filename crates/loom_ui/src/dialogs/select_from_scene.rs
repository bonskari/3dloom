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
    let id = egui::Id::new("select_from_scene_state");
    let mut st: State = ctx.data(|d| d.get_temp(id)).unwrap_or(State { geometry: true, cameras: true, ..Default::default() });
    let mut done = false;
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
            if ui.button("Select All").clicked() {
                st.chosen = app.state.scene.nodes().map(|n| n.id.0).collect();
            }
            if ui.button("Select None").clicked() {
                st.chosen.clear();
            }
            if ui.button("Select Invert").clicked() {
                let all: BTreeSet<u64> = app.state.scene.nodes().map(|n| n.id.0).collect();
                st.chosen = all.difference(&st.chosen).copied().collect();
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
            egui::Grid::new("sfs_rows").num_columns(3).striped(true).show(ui, |ui| {
                ui.label(egui::RichText::new("Name").strong());
                ui.label(egui::RichText::new("Type").strong());
                ui.label(egui::RichText::new("State").strong());
                ui.end_row();
                let rows: Vec<(u64, String, &'static str, bool, bool)> = app
                    .state
                    .scene
                    .nodes()
                    .map(|n| (n.id.0, n.name.clone(), n.object.base_name(), n.hidden, n.frozen))
                    .collect();
                for (nid, name, kind, hidden, frozen) in rows {
                    let is_cam = kind == "Camera";
                    if (is_cam && !st.cameras) || (!is_cam && !st.geometry) {
                        continue;
                    }
                    // Attach List: the object being attached to is not a candidate.
                    if purpose == DialogPurpose::Attach && app.state.selection.contains(&loom_scene::NodeId(nid)) {
                        continue;
                    }
                    let hidden_ok = match purpose {
                        DialogPurpose::Unhide => true,
                        _ => st.show_hidden || !hidden,
                    };
                    if !hidden_ok || (!st.search.is_empty() && !name.to_lowercase().contains(&st.search.to_lowercase())) {
                        continue;
                    }
                    let on = st.chosen.contains(&nid);
                    if ui.selectable_label(on, &name).clicked() {
                        if on {
                            st.chosen.remove(&nid);
                        } else {
                            st.chosen.insert(nid);
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
            if ui.button(verb).clicked() {
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
        st.chosen.clear();
        *open = false;
    }
    ctx.data_mut(|d| d.insert_temp(id, st));
    let _ = NodeId(0);
}
