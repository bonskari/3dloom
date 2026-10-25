//! Layer Manager (Manage Layers), titled with the current layer: one row
//! per layer with the current radio, an editable name, Hide, Freeze, Render
//! and Color, expandable to the layer's objects (a click selects one,
//! Ctrl+click adds); the toolbar has Create New Layer, Delete Highlighted
//! Empty Layers, Add Selected Objects to Highlighted Layer, Select
//! Highlighted Objects and Layers, Highlight Selected Objects' Layers,
//! Hide/Unhide All Layers and Freeze/Unfreeze All Layers.

use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, SelectOp};
use std::collections::BTreeSet;

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let id = egui::Id::new("layer_manager");
    let mut highlighted: BTreeSet<u32> = ctx.data(|d| d.get_temp(id.with("highlight"))).unwrap_or_default();
    let mut expanded: BTreeSet<u32> = ctx.data(|d| d.get_temp(id.with("expanded"))).unwrap_or_default();
    let layers = app.state.scene.layers.clone();
    let current = app.state.scene.current_layer;
    let current_name = layers.iter().find(|l| l.id == current).map(|l| l.name.clone()).unwrap_or_default();
    let members = |layer: u32| -> Vec<(u64, String)> { app.state.scene.nodes().filter(|n| n.layer == layer).map(|n| (n.id.0, n.name.clone())).collect() };
    let objects: Vec<Vec<(u64, String)>> = layers.iter().map(|l| members(l.id)).collect();
    // The window id stays put while the title follows the current layer.
    egui::Window::new(format!("Layer: {current_name}")).id(id.with("window")).open(open).default_width(460.0).show(ctx, |ui| {
        ui.horizontal(|ui| {
            if ui.button("Create New Layer").on_hover_text("Create a layer (becomes current; the selection moves onto it)").clicked() {
                let _ = app.dispatch(Command::CreateLayer { name: None });
            }
            let empty: Vec<u32> = layers.iter().zip(&objects).filter(|(l, o)| l.id != 0 && o.is_empty() && highlighted.contains(&l.id)).map(|(l, _)| l.id).collect();
            if ui.add_enabled(!empty.is_empty(), egui::Button::new("Delete")).on_hover_text("Delete Highlighted Empty Layers").clicked() {
                for l in &empty {
                    let _ = app.dispatch(Command::DeleteLayer { layer: *l });
                    highlighted.remove(l);
                }
            }
            let target = (highlighted.len() == 1).then(|| *highlighted.iter().next().expect("one"));
            if ui.add_enabled(target.is_some() && !app.state.selection.is_empty(), egui::Button::new("Add Selection")).on_hover_text("Add Selected Objects to Highlighted Layer").clicked() {
                if let Some(t) = target {
                    let _ = app.dispatch(Command::MoveToLayer { layer: t });
                }
            }
            if ui.add_enabled(!highlighted.is_empty(), egui::Button::new("Select Highlighted")).on_hover_text("Select Highlighted Objects and Layers").clicked() {
                let ids: Vec<u64> = layers.iter().zip(&objects).filter(|(l, _)| highlighted.contains(&l.id)).flat_map(|(_, o)| o.iter().map(|(i, _)| *i)).collect();
                let _ = app.dispatch(Command::SelectNodes { ids, op: SelectOp::Replace });
            }
            if ui.add_enabled(!app.state.selection.is_empty(), egui::Button::new("Highlight Selected")).on_hover_text("Highlight Selected Objects' Layers").clicked() {
                highlighted = app.state.selection.iter().filter_map(|n| app.state.scene.node(*n)).map(|n| n.layer).collect();
            }
        });
        ui.horizontal(|ui| {
            let all_hidden = layers.iter().all(|l| l.hidden);
            if ui.button(if all_hidden { "Unhide All Layers" } else { "Hide All Layers" }).clicked() {
                let _ = app.dispatch(Command::SetAllLayersHidden { on: !all_hidden });
            }
            let all_frozen = layers.iter().all(|l| l.frozen);
            if ui.button(if all_frozen { "Unfreeze All Layers" } else { "Freeze All Layers" }).clicked() {
                let _ = app.dispatch(Command::SetAllLayersFrozen { on: !all_frozen });
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
            egui::Grid::new(id.with("grid")).num_columns(7).spacing([8.0, 2.0]).striped(true).show(ui, |ui| {
                ui.label("");
                ui.label(egui::RichText::new("Layers").strong());
                ui.label("Hide");
                ui.label("Freeze");
                ui.label("Render");
                ui.label("Color");
                ui.label("Objects");
                ui.end_row();
                for (i, l) in layers.iter().enumerate() {
                    if ui.radio(current == l.id, "").on_hover_text("Current layer: new objects go here").clicked() && current != l.id {
                        let _ = app.dispatch(Command::SetCurrentLayer { layer: l.id });
                    }
                    ui.horizontal(|ui| {
                        let open_layer = expanded.contains(&l.id);
                        if ui.add_enabled(!objects[i].is_empty(), egui::Button::new(if open_layer { "−" } else { "+" }).small()).on_hover_text("Show the layer's objects").clicked() {
                            if open_layer {
                                expanded.remove(&l.id);
                            } else {
                                expanded.insert(l.id);
                            }
                        }
                        let name_id = id.with(("name", l.id));
                        let mut name: String = ui.ctx().data(|d| d.get_temp(name_id)).unwrap_or_else(|| l.name.clone());
                        let edit = egui::TextEdit::singleline(&mut name).desired_width(150.0);
                        let edit = if highlighted.contains(&l.id) { edit.background_color(ui.visuals().selection.bg_fill) } else { edit };
                        let r = ui.add_sized([150.0, 18.0], edit);
                        if r.clicked() || r.gained_focus() {
                            if ui.input(|inp| inp.modifiers.ctrl) {
                                highlighted.insert(l.id);
                            } else {
                                highlighted = BTreeSet::from([l.id]);
                            }
                        }
                        if r.lost_focus() {
                            if name.trim() != l.name && !name.trim().is_empty() {
                                let _ = app.dispatch(Command::RenameLayer { layer: l.id, name: name.trim().to_owned() });
                            }
                            ui.ctx().data_mut(|d| d.remove::<String>(name_id));
                        } else if r.changed() {
                            ui.ctx().data_mut(|d| d.insert_temp(name_id, name.clone()));
                        }
                    });
                    let mut hidden = l.hidden;
                    if ui.checkbox(&mut hidden, "").on_hover_text("Hide").changed() {
                        let _ = app.dispatch(Command::SetLayerHidden { layer: l.id, on: hidden });
                    }
                    let mut frozen = l.frozen;
                    if ui.checkbox(&mut frozen, "").on_hover_text("Freeze").changed() {
                        let _ = app.dispatch(Command::SetLayerFrozen { layer: l.id, on: frozen });
                    }
                    let mut render = l.renderable;
                    if ui.checkbox(&mut render, "").on_hover_text("Render").changed() {
                        let _ = app.dispatch(Command::SetLayerRenderable { layer: l.id, on: render });
                    }
                    let mut rgb = l.color;
                    if ui.color_edit_button_srgb(&mut rgb).on_hover_text("Layer colour").changed() {
                        let _ = app.dispatch(Command::SetLayerColor { layer: l.id, rgb });
                    }
                    ui.label(objects[i].len().to_string());
                    ui.end_row();
                    if expanded.contains(&l.id) {
                        for (nid, name) in &objects[i] {
                            ui.label("");
                            ui.horizontal(|ui| {
                                ui.add_space(24.0);
                                let on = app.state.selection.contains(&loom_scene::NodeId(*nid));
                                if ui.selectable_label(on, name).clicked() {
                                    let op = if ui.input(|inp| inp.modifiers.ctrl) { SelectOp::Add } else { SelectOp::Replace };
                                    let _ = app.dispatch(Command::SelectNodes { ids: vec![*nid], op });
                                }
                            });
                            ui.end_row();
                        }
                    }
                }
            });
        });
    });
    ctx.data_mut(|d| {
        d.insert_temp(id.with("highlight"), highlighted);
        d.insert_temp(id.with("expanded"), expanded);
    });
}
