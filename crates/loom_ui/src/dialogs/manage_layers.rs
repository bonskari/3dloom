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
    // Column sort: a click on a column header sorts the list by it, a second
    // click reverses it (the reference's Layer Manager columns).
    let sort_id = id.with("sort");
    let (sort_col, sort_up): (usize, bool) = ctx.data(|d| d.get_temp(sort_id)).unwrap_or((1, true));
    let mut order: Vec<usize> = (0..layers.len()).collect();
    order.sort_by(|a, b| {
        let (la, lb) = (&layers[*a], &layers[*b]);
        let c = match sort_col {
            2 => la.hidden.cmp(&lb.hidden),
            3 => la.frozen.cmp(&lb.frozen),
            4 => la.render.renderable.cmp(&lb.render.renderable),
            5 => la.color.cmp(&lb.color),
            6 => objects[*a].len().cmp(&objects[*b].len()),
            _ => la.name.to_lowercase().cmp(&lb.name.to_lowercase()),
        };
        let c = if sort_up { c } else { c.reverse() };
        c.then(a.cmp(b))
    });
    // The window id stays put while the title follows the current layer.
    let window = egui::Window::new(format!("Layer: {current_name}")).id(id.with("window")).default_width(460.0);
    crate::os_window::OsWindow::new("Layer Manager", format!("Layer: {current_name}"), [480.0, 420.0]).show(ctx, open, window, |ui| {
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
            if ui.add_enabled(target.is_some(), egui::Button::new("Layer Properties")).on_hover_text("Display properties and rendering control of the highlighted layer").clicked() {
                if let Some(t) = target {
                    let _ = app.dispatch(Command::LayerProperties { layer: t });
                }
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
                let mut clicked: Option<usize> = None;
                ui.label("");
                for (col, label) in [(1usize, "Layers"), (2, "Hide"), (3, "Freeze"), (4, "Render"), (5, "Color"), (6, "Objects")] {
                    let text = if col == sort_col { format!("{label} {}", if sort_up { "▲" } else { "▼" }) } else { label.to_owned() };
                    let text = if col == 1 { egui::RichText::new(text).strong() } else { egui::RichText::new(text) };
                    if ui.add(egui::Button::new(text).frame(false)).on_hover_text("Sort by this column").clicked() {
                        clicked = Some(col);
                    }
                }
                ui.end_row();
                if let Some(col) = clicked {
                    let next = if col == sort_col { (col, !sort_up) } else { (col, true) };
                    ui.ctx().data_mut(|d| d.insert_temp(sort_id, next));
                }
                for (i, l) in order.iter().map(|i| (*i, &layers[*i])) {
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
                    let mut render = l.render.renderable;
                    if ui.checkbox(&mut render, "").on_hover_text("Render").changed() {
                        let _ = app.dispatch(Command::SetLayerRenderable { layer: l.id, on: render });
                    }
                    let mut rgb = l.color;
                    if crate::color_selector::srgb(ui, &mut rgb).on_hover_text("Layer colour").changed() {
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
