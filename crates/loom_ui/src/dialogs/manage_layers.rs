//! Layer Manager (Manage Layers): one row per layer with the
//! current radio, hide and freeze toggles and an editable name, plus New
//! Layer, Delete, Add Selection to Current Layer, Select Highlighted
//! Objects and Highlight Selected Objects' Layers.

use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let id = egui::Id::new("layer_manager");
    let mut highlighted: Option<u32> = ctx.data(|d| d.get_temp(id.with("highlight")));
    let layers = app.state.scene.layers.clone();
    let current = app.state.scene.current_layer;
    let counts: Vec<usize> = layers.iter().map(|l| app.state.scene.nodes().filter(|n| n.layer == l.id).count()).collect();
    egui::Window::new("Layer Manager").open(open).default_width(420.0).show(ctx, |ui| {
        ui.horizontal(|ui| {
            if ui.button("New Layer").on_hover_text("Create a layer (becomes current; the selection moves onto it)").clicked() {
                let _ = app.dispatch(Command::CreateLayer { name: None });
            }
            let can_delete = highlighted.is_some_and(|h| h != 0 && counts.get(layers.iter().position(|l| l.id == h).unwrap_or(0)).copied().unwrap_or(1) == 0);
            if ui.add_enabled(can_delete, egui::Button::new("Delete")).on_hover_text("Delete the highlighted empty layer").clicked() {
                if let Some(h) = highlighted {
                    let _ = app.dispatch(Command::DeleteLayer { layer: h });
                    highlighted = None;
                }
            }
            if ui.add_enabled(!app.state.selection.is_empty(), egui::Button::new("Add Selection to Current Layer")).clicked() {
                let _ = app.dispatch(Command::AddSelectionToCurrentLayer);
            }
        });
        ui.horizontal(|ui| {
            if ui.add_enabled(highlighted.is_some(), egui::Button::new("Select Highlighted Objects")).clicked() {
                if let Some(h) = highlighted {
                    let _ = app.dispatch(Command::SelectLayerObjects { layer: h });
                }
            }
            if ui.add_enabled(!app.state.selection.is_empty(), egui::Button::new("Highlight Selected Objects' Layers")).clicked() {
                highlighted = app.state.selection.iter().next().and_then(|n| app.state.scene.node(*n)).map(|n| n.layer);
            }
        });
        ui.separator();
        egui::Grid::new(id.with("grid")).num_columns(6).spacing([8.0, 2.0]).striped(true).show(ui, |ui| {
            ui.label("");
            ui.label(egui::RichText::new("Layers").strong());
            ui.label("Hide");
            ui.label("Freeze");
            ui.label("Objects");
            ui.label("");
            ui.end_row();
            for (i, l) in layers.iter().enumerate() {
                if ui.radio(current == l.id, "").on_hover_text("Current layer: new objects go here").clicked() && current != l.id {
                    let _ = app.dispatch(Command::SetCurrentLayer { layer: l.id });
                }
                let name_id = id.with(("name", l.id));
                let mut name: String = ui.ctx().data(|d| d.get_temp(name_id)).unwrap_or_else(|| l.name.clone());
                let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(160.0));
                if r.clicked() || r.gained_focus() {
                    highlighted = Some(l.id);
                }
                if r.lost_focus() {
                    if name.trim() != l.name && !name.trim().is_empty() {
                        let _ = app.dispatch(Command::RenameLayer { layer: l.id, name: name.trim().to_owned() });
                    }
                    ui.ctx().data_mut(|d| d.remove::<String>(name_id));
                } else if r.changed() {
                    ui.ctx().data_mut(|d| d.insert_temp(name_id, name.clone()));
                }
                let mut hidden = l.hidden;
                if ui.checkbox(&mut hidden, "").changed() {
                    let _ = app.dispatch(Command::SetLayerHidden { layer: l.id, on: hidden });
                }
                let mut frozen = l.frozen;
                if ui.checkbox(&mut frozen, "").changed() {
                    let _ = app.dispatch(Command::SetLayerFrozen { layer: l.id, on: frozen });
                }
                ui.label(counts[i].to_string());
                let r = ui.selectable_label(highlighted == Some(l.id), crate::icons::HIGHLIGHT);
                crate::icons::named(&r, egui::WidgetType::SelectableLabel, "Highlight this layer");
                if r.on_hover_text("Highlight this layer").clicked() {
                    highlighted = Some(l.id);
                }
                ui.end_row();
            }
        });
    });
    ctx.data_mut(|d| match highlighted {
        Some(h) => d.insert_temp(id.with("highlight"), h),
        None => d.remove::<u32>(id.with("highlight")),
    });
}
