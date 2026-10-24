//! Object Properties dialog, General panel: Object Information,
//! Interactivity, Display Properties.

use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;
use loom_scene::DisplayProps;

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let ids: Vec<loom_scene::NodeId> = app.state.selection.iter().copied().collect();
    if ids.is_empty() {
        *open = false;
        return;
    }
    let first = ids[0];
    let Some(node) = app.state.scene.node(first).cloned() else {
        *open = false;
        return;
    };
    let multiple = ids.len() > 1;
    let mesh = node.object.mesh();
    let (min, max) = app.state.object_bounds(first).unwrap_or((glam::Vec3::ZERO, glam::Vec3::ZERO));
    let dims = max - min;
    let parent = node.parent.and_then(|p| app.state.scene.node(p)).map(|n| n.name.clone()).unwrap_or_else(|| "Scene Root".to_owned());
    let children = app.state.scene.nodes().filter(|n| n.parent == Some(first)).count();
    let name_id = egui::Id::new("object_properties_name");
    let mut name: String = ctx.data(|d| d.get_temp(name_id)).unwrap_or_else(|| node.name.clone());
    let mut rgb = node.wire_color;
    egui::Window::new("Object Properties").open(open).resizable(false).default_width(520.0).show(ctx, |ui| {
        ui.horizontal(|ui| {
            let _ = ui.selectable_label(true, "General");
            ui.add_enabled(false, egui::SelectableLabel::new(false, "Adv. Lighting"));
            ui.add_enabled(false, egui::SelectableLabel::new(false, "mental ray"));
            ui.add_enabled(false, egui::SelectableLabel::new(false, "User Defined"));
        });
        ui.separator();
        ui.group(|ui| {
            ui.label(egui::RichText::new("Object Information").strong());
            ui.horizontal(|ui| {
                ui.label("Name:");
                if multiple {
                    ui.label("Multiple Selected");
                } else {
                    let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(200.0));
                    if r.lost_focus() && name != node.name {
                        let _ = app.dispatch(Command::Rename { id: first.0, name: name.clone() });
                    }
                }
                if ui.color_edit_button_srgb(&mut rgb).changed() {
                    for id in &ids {
                        let _ = app.dispatch(Command::SetWireColor { id: id.0, rgb });
                    }
                }
                // Object colour By Layer / By Object.
                let by_layer = node.display.color_by_layer;
                if ui.selectable_label(by_layer, if by_layer { "By Layer" } else { "By Object" }).on_hover_text("Object colour from the layer or from the object").clicked() {
                    let _ = app.dispatch(Command::SetColorByLayer { on: !by_layer });
                }
            });
            egui::Grid::new("object_info").num_columns(4).spacing([12.0, 2.0]).show(ui, |ui| {
                ui.label("Dimensions:");
                ui.label(format!("X: {}", app.state.format_units(dims.x)));
                ui.label("Vertices:");
                ui.label(mesh.vertex_count().to_string());
                ui.end_row();
                ui.label("");
                ui.label(format!("Y: {}", app.state.format_units(dims.y)));
                ui.label("Faces:");
                ui.label(mesh.face_count().to_string());
                ui.end_row();
                ui.label("");
                ui.label(format!("Z: {}", app.state.format_units(dims.z)));
                ui.label("");
                ui.label("");
                ui.end_row();
                ui.label("Parent:");
                ui.label(parent);
                ui.label("Material Name:");
                ui.label("None");
                ui.end_row();
                ui.label("Num. Children:");
                ui.label(children.to_string());
                ui.label("In Group/Assembly:");
                ui.label(node.parent.and_then(|p| app.state.scene.node(p)).filter(|p| matches!(p.object, loom_scene::ObjectKind::Group { .. })).map(|p| p.name.clone()).unwrap_or_else(|| "None".to_owned()));
                ui.end_row();
                ui.label("Layer:");
                ui.label(app.state.scene.layer(node.layer).map(|l| l.name.clone()).unwrap_or_else(|| "0 (default)".to_owned()));
                ui.end_row();
            });
        });
        ui.horizontal_top(|ui| {
            ui.group(|ui| {
                ui.label(egui::RichText::new("Interactivity").strong());
                let mut hide = node.hidden;
                if ui.checkbox(&mut hide, "Hide").changed() {
                    let _ = app.dispatch(if hide { Command::HideSelection } else { Command::UnhideAll });
                }
                let mut frozen = node.frozen;
                if ui.checkbox(&mut frozen, "Freeze").changed() {
                    let _ = app.dispatch(if frozen { Command::FreezeSelection } else { Command::UnfreezeAll });
                }
            });
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Display Properties").strong());
                    ui.add_enabled(false, egui::Button::new("By Object"));
                });
                for prop in DisplayProps::NAMES {
                    let mut on = node.display.get(prop).unwrap_or(false);
                    if ui.checkbox(&mut on, prop).changed() {
                        for id in &ids {
                            let _ = app.dispatch(Command::SetDisplayProperty { id: id.0, name: prop.to_owned(), value: on });
                        }
                    }
                }
                ui.add_enabled(false, egui::Checkbox::new(&mut false, "Trajectory"));
                ui.add_enabled(false, egui::Checkbox::new(&mut false, "Never Degrade"));
            });
        });
        ui.label(egui::RichText::new("Rendering Control and Motion Blur: rendering is out of scope.").small().weak());
        if ui.button("OK").clicked() {
            let _ = app.dispatch(Command::CloseDialog);
        }
    });
    ctx.data_mut(|d| d.insert_temp(name_id, name));
}
