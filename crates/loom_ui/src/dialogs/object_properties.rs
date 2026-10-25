//! Object Properties dialog: the General panel (Object Information,
//! Interactivity, Display Properties, Rendering Control, G-Buffer, Motion
//! Blur), Adv. Lighting, mental ray and User Defined panels.

use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;
use loom_scene::{DisplayProps, Node, RenderProps};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Panel {
    #[default]
    General,
    AdvLighting,
    UserDefined,
}

/// A checkbox over the selected objects: ticked when all are on, mixed when
/// they differ. Returns the new value when clicked.
fn tri_checkbox(ui: &mut egui::Ui, label: &str, nodes: &[&Node], get: impl Fn(&Node) -> bool) -> Option<bool> {
    let on = nodes.iter().filter(|n| get(n)).count();
    let mut v = on == nodes.len();
    ui.add(egui::Checkbox::new(&mut v, label).indeterminate(on > 0 && on < nodes.len())).changed().then_some(v)
}

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
    let nodes: Vec<Node> = ids.iter().filter_map(|id| app.state.scene.node(*id).cloned()).collect();
    let refs: Vec<&Node> = nodes.iter().collect();
    let multiple = ids.len() > 1;
    let panel_id = egui::Id::new("object_properties_panel");
    let mut panel: Panel = ctx.data(|d| d.get_temp(panel_id)).unwrap_or_default();
    let name_id = egui::Id::new("object_properties_name");
    let mut name: String = ctx.data(|d| d.get_temp(name_id)).unwrap_or_else(|| node.name.clone());
    let user_id = egui::Id::new(("object_properties_user", first));
    let mut user: String = ctx.data(|d| d.get_temp(user_id)).unwrap_or_else(|| node.user_props.clone());
    egui::Window::new("Object Properties").open(open).resizable(false).default_width(540.0).show(ctx, |ui| {
        ui.horizontal(|ui| {
            for (p, label) in [(Panel::General, "General"), (Panel::AdvLighting, "Adv. Lighting")] {
                if ui.selectable_label(panel == p, label).clicked() {
                    panel = p;
                }
            }
            // The reference's mental ray panel sets mental ray options the Bevy renderer does not have.
            ui.add_enabled(false, egui::SelectableLabel::new(false, "mental ray")).on_disabled_hover_text("No mental ray renderer");
            if ui.selectable_label(panel == Panel::UserDefined, "User Defined").clicked() {
                panel = Panel::UserDefined;
            }
        });
        ui.separator();
        match panel {
            Panel::General => general(ui, app, &node, &refs, &ids, multiple, &mut name),
            Panel::AdvLighting => adv_lighting(ui, &refs),
            Panel::UserDefined => {
                ui.label(egui::RichText::new("User Defined Properties").strong());
                let r = ui.add(egui::TextEdit::multiline(&mut user).desired_rows(10).desired_width(f32::INFINITY).interactive(!multiple));
                if r.lost_focus() && user != node.user_props {
                    let _ = app.dispatch(Command::SetUserProperties { id: first.0, text: user.clone() });
                }
                if multiple {
                    ui.label(egui::RichText::new("Select one object to edit its user properties.").small().weak());
                }
            }
        }
        if ui.button("OK").clicked() {
            if panel == Panel::UserDefined && !multiple && user != node.user_props {
                let _ = app.dispatch(Command::SetUserProperties { id: first.0, text: user.clone() });
            }
            let _ = app.dispatch(Command::CloseDialog);
        }
    });
    ctx.data_mut(|d| {
        d.insert_temp(name_id, name);
        d.insert_temp(user_id, user);
        d.insert_temp(panel_id, panel);
    });
}

#[allow(clippy::too_many_arguments)]
fn general(ui: &mut egui::Ui, app: &mut LoomApp, node: &Node, nodes: &[&Node], ids: &[loom_scene::NodeId], multiple: bool, name: &mut String) {
    let first = node.id;
    let mesh = node.object.mesh();
    let (min, max) = app.state.object_bounds(first).unwrap_or((glam::Vec3::ZERO, glam::Vec3::ZERO));
    let dims = max - min;
    let parent = node.parent.and_then(|p| app.state.scene.node(p)).map(|n| n.name.clone()).unwrap_or_else(|| "Scene Root".to_owned());
    let children = app.state.scene.nodes().filter(|n| n.parent == Some(first)).count();
    let material = app.state.scene.node_material(first).map(|m| m.name.clone()).unwrap_or_else(|| "None".to_owned());
    let shape = node.object.shape();
    ui.group(|ui| {
        ui.label(egui::RichText::new("Object Information").strong());
        ui.horizontal(|ui| {
            ui.label("Name:");
            if multiple {
                ui.label("Multiple Selected");
            } else {
                let r = ui.add(egui::TextEdit::singleline(name).desired_width(200.0));
                if r.lost_focus() && *name != node.name {
                    let _ = app.dispatch(Command::Rename { id: first.0, name: name.clone() });
                }
            }
            if crate::dialogs::object_color::swatch(ui, node.wire_color).clicked() {
                let _ = app.dispatch(Command::ObjectColorDialog);
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
            if let Some(s) = &shape {
                ui.label("Shape Vertices:");
                ui.label(s.splines.iter().map(|p| p.knots.len()).sum::<usize>().to_string());
                ui.end_row();
                ui.label("");
                ui.label("");
                ui.label("Shape Curves:");
                ui.label(s.splines.len().to_string());
            } else {
                ui.label("");
                ui.label("");
            }
            ui.end_row();
            ui.label("Parent:");
            ui.label(parent);
            ui.label("Material Name:");
            ui.label(material);
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
    let raw: Vec<u64> = ids.iter().map(|i| i.0).collect();
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.group(|ui| {
                ui.label(egui::RichText::new("Interactivity").strong());
                if let Some(v) = tri_checkbox(ui, "Hide", nodes, |n| n.hidden) {
                    let _ = app.dispatch(if v { Command::HideNodes { ids: raw.clone() } } else { Command::UnhideNodes { ids: raw.clone() } });
                }
                if let Some(v) = tri_checkbox(ui, "Freeze", nodes, |n| n.frozen) {
                    let _ = app.dispatch(if v { Command::FreezeNodes { ids: raw.clone() } } else { Command::UnfreezeNodes { ids: raw.clone() } });
                }
            });
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Display Properties").strong());
                    ui.add_enabled(false, egui::Button::new("By Object")).on_disabled_hover_text("By Layer display properties come with the layer properties");
                });
                for prop in DisplayProps::NAMES {
                    if prop == "Vertex Color Shaded" {
                        continue;
                    }
                    let label = if prop == "Vertex Color" { "Vertex Channel Display" } else { prop };
                    if let Some(v) = tri_checkbox(ui, label, nodes, |n| n.display.get(prop).unwrap_or(false)) {
                        for id in ids {
                            let _ = app.dispatch(Command::SetDisplayProperty { id: id.0, name: prop.to_owned(), value: v });
                        }
                    }
                    if prop == "Show Frozen in Gray" {
                        // Adaptive degradation does not exist here (the viewports never degrade).
                        ui.add_enabled(false, egui::Checkbox::new(&mut false, "Never Degrade")).on_disabled_hover_text("The viewports never degrade");
                    }
                }
                ui.horizontal(|ui| {
                    ui.add_space(18.0);
                    let all = nodes.iter().all(|n| n.display.vertex_colors_shaded);
                    if ui.selectable_label(all, "Shaded").on_hover_text("Shaded viewports light the vertex colours").clicked() {
                        for id in ids {
                            let _ = app.dispatch(Command::SetDisplayProperty { id: id.0, name: "Vertex Color Shaded".into(), value: !all });
                        }
                    }
                });
            });
        });
        ui.vertical(|ui| {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Rendering Control").strong());
                    ui.add_enabled(false, egui::Button::new("By Object")).on_disabled_hover_text("By Layer rendering control comes with the layer properties");
                });
                // For lights Renderable is the only control (off turns the light off in renders).
                let lights_only = nodes.iter().all(|n| matches!(n.object, loom_scene::ObjectKind::Light(_)));
                if lights_only {
                    if let Some(v) = tri_checkbox(ui, "Renderable", nodes, |n| n.render.renderable) {
                        let _ = app.dispatch(Command::SetRenderProperty { name: "Renderable".into(), value: v });
                    }
                    return;
                }
                ui.horizontal(|ui| {
                    ui.label("Visibility:");
                    let mut v = node.visibility.as_ref().map_or(1.0, |p| p.get());
                    let r = ui.add(egui::DragValue::new(&mut v).range(0.0..=1.0).speed(0.01).fixed_decimals(3));
                    if r.drag_started() {
                        app.state.undo.begin("Visibility");
                    }
                    if r.changed() {
                        let _ = app.dispatch(Command::SetObjectVisibility { value: v });
                    }
                    if r.drag_stopped() {
                        app.state.undo.end();
                    }
                });
                for prop in RenderProps::NAMES {
                    if let Some(v) = tri_checkbox(ui, prop, nodes, |n| n.render.get(prop).unwrap_or(false)) {
                        let _ = app.dispatch(Command::SetRenderProperty { name: prop.to_owned(), value: v });
                    }
                    if prop == "Visible to Camera" {
                        ui.add_enabled(false, egui::Checkbox::new(&mut true, "Visible to Reflection/Refraction")).on_disabled_hover_text("The renderer has no reflections or refractions");
                    }
                }
                ui.add_enabled(false, egui::Checkbox::new(&mut true, "Apply Atmospherics")).on_disabled_hover_text("The renderer has no atmospherics");
                ui.add_enabled(false, egui::Checkbox::new(&mut false, "Render Occluded Objects")).on_disabled_hover_text("The renderer has no G-buffer effects");
            });
            ui.group(|ui| {
                ui.label(egui::RichText::new("G-Buffer").strong());
                ui.horizontal(|ui| {
                    ui.label("Object ID:");
                    ui.add_enabled(false, egui::DragValue::new(&mut 0u32)).on_disabled_hover_text("The renderer has no G-buffer channels");
                });
            });
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Motion Blur").strong());
                    ui.add_enabled(false, egui::Button::new("By Object"));
                });
                ui.add_enabled_ui(false, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Multiplier:");
                        ui.add(egui::DragValue::new(&mut 1.0f32));
                        ui.checkbox(&mut true, "Enabled");
                    });
                });
                ui.horizontal(|ui| {
                    let _ = ui.radio(true, "None");
                    ui.add_enabled(false, egui::RadioButton::new(false, "Object")).on_disabled_hover_text("The renderer has no motion blur");
                    ui.add_enabled(false, egui::RadioButton::new(false, "Image")).on_disabled_hover_text("The renderer has no motion blur");
                });
            });
        });
    });
}

/// Adv. Lighting: the Selection Information the reference shows. The
/// radiosity settings do not apply: the Bevy renderer has no radiosity or
/// light tracer.
fn adv_lighting(ui: &mut egui::Ui, nodes: &[&Node]) {
    let is_light = |n: &&&Node| matches!(n.object, loom_scene::ObjectKind::Light(_));
    let lights = nodes.iter().filter(is_light).count();
    let geometry = nodes.iter().filter(|n| !is_light(n) && !n.object.is_helper() && !n.object.is_shape() && !n.object.is_bone()).count();
    ui.group(|ui| {
        ui.label(egui::RichText::new("Selection Information").strong());
        egui::Grid::new("adv_lighting_info").num_columns(2).show(ui, |ui| {
            ui.label("Num. Geometric Objects:");
            ui.label(geometry.to_string());
            ui.end_row();
            ui.label("Num. Light Objects:");
            ui.label(lights.to_string());
            ui.end_row();
        });
    });
    ui.label(egui::RichText::new("Radiosity and light tracer settings do not apply: the renderer has no advanced lighting.").small().weak());
}
