//! The Layers toolbar (Customize > Show UI > Show Layers Toolbar): Layer
//! Manager, the layer list (current layer; each row with Hide, Freeze,
//! Render, Color and a click on the name makes it current), Create New
//! Layer, Add Selection to Current Layer, Select Objects in Current Layer and
//! Set Current Layer to Selection's Layer.

use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;

pub fn show(ctx: &egui::Context, app: &mut LoomApp) {
    if app.state.expert_mode || !app.state.show_layers_toolbar {
        return;
    }
    egui::TopBottomPanel::top("layers_toolbar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label("Layers:");
            let on = app.state.dialog == Some(loom_commands::Dialog::LayerManager);
            if ui.selectable_label(on, "Layer Manager").on_hover_text("Manage Layers").clicked() {
                let _ = app.dispatch(Command::LayerManager);
            }
            let layers = app.state.scene.layers.clone();
            let current = app.state.scene.current_layer;
            let current_name = layers.iter().find(|l| l.id == current).map(|l| l.name.clone()).unwrap_or_default();
            egui::ComboBox::from_id_salt("layer_list")
                .selected_text(current_name)
                .width(160.0)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .show_ui(ui, |ui| {
                    for l in &layers {
                        ui.horizontal(|ui| {
                            if ui.selectable_label(l.hidden, "Hide").on_hover_text("Hide / unhide the layer").clicked() {
                                let _ = app.dispatch(Command::SetLayerHidden { layer: l.id, on: !l.hidden });
                            }
                            if ui.selectable_label(l.frozen, "Freeze").on_hover_text("Freeze / unfreeze the layer").clicked() {
                                let _ = app.dispatch(Command::SetLayerFrozen { layer: l.id, on: !l.frozen });
                            }
                            if ui.selectable_label(!l.renderable, "No Render").on_hover_text("Render on / off").clicked() {
                                let _ = app.dispatch(Command::SetLayerRenderable { layer: l.id, on: !l.renderable });
                            }
                            let mut rgb = l.color;
                            if ui.color_edit_button_srgb(&mut rgb).changed() {
                                let _ = app.dispatch(Command::SetLayerColor { layer: l.id, rgb });
                            }
                            if ui.selectable_label(l.id == current, &l.name).on_hover_text("Make current").clicked() {
                                if l.id != current {
                                    let _ = app.dispatch(Command::SetCurrentLayer { layer: l.id });
                                }
                                ui.memory_mut(|m| m.close_popup());
                            }
                        });
                    }
                });
            if ui.button("Create New Layer").clicked() {
                let _ = app.dispatch(Command::CreateLayer { name: None });
            }
            let has = !app.state.selection.is_empty();
            if ui.add_enabled(has, egui::Button::new("Add Selection to Current Layer")).clicked() {
                let _ = app.dispatch(Command::AddSelectionToCurrentLayer);
            }
            if ui.button("Select Objects in Current Layer").clicked() {
                let _ = app.dispatch(Command::SelectLayerObjects { layer: current });
            }
            if ui.add_enabled(has, egui::Button::new("Set Current Layer to Selection's Layer")).clicked() {
                if let Err(e) = app.dispatch(Command::SetCurrentLayerToSelection) {
                    app.state.status_prompt = e;
                }
            }
        });
    });
}
