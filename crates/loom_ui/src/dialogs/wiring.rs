//! Animation > Wire Parameters (Parameter Wiring dialog), Add Custom
//! Attributes and the Parameter Collector. Every change is a command.

use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;
use loom_scene::NodeId;

/// The transform tracks every node offers, then its custom attributes.
fn parameters(app: &LoomApp, id: NodeId) -> Vec<String> {
    let mut v: Vec<String> = ["Position", "Rotation", "Scale"]
        .iter()
        .flat_map(|t| ["X", "Y", "Z"].iter().map(move |a| format!("{t} {a}")))
        .collect();
    if let Some(n) = app.state.scene.node(id) {
        v.extend(n.custom_attributes.iter().map(|a| a.name.clone()));
    }
    v
}

fn node_name(app: &LoomApp, id: NodeId) -> String {
    app.state.scene.node(id).map(|n| n.name.clone()).unwrap_or_else(|| format!("#{}", id.0))
}

#[derive(Clone)]
struct Wiring {
    source: Option<(NodeId, String)>,
    target: Option<(NodeId, String)>,
    two_way: bool,
    scale: f32,
    offset: f32,
    expression: String,
}

impl Default for Wiring {
    fn default() -> Self {
        Self { source: None, target: None, two_way: false, scale: 1.0, offset: 0.0, expression: String::new() }
    }
}

/// One side of the dialog: a tree of nodes with their parameters.
fn side(ui: &mut egui::Ui, app: &LoomApp, salt: &str, pick: &mut Option<(NodeId, String)>) {
    let ids: Vec<NodeId> = app.state.scene.nodes().map(|n| n.id).collect();
    egui::ScrollArea::vertical().id_salt(salt).max_height(260.0).show(ui, |ui| {
        for id in ids {
            egui::CollapsingHeader::new(node_name(app, id)).id_salt((salt, id.0)).show(ui, |ui| {
                for p in parameters(app, id) {
                    let on = pick.as_ref() == Some(&(id, p.clone()));
                    if ui.selectable_label(on, &p).clicked() {
                        *pick = Some((id, p));
                    }
                }
            });
        }
    });
}

pub fn parameter_wiring(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let fid = egui::Id::new("parameter_wiring_form");
    let mut f: Wiring = ctx.data(|d| d.get_temp(fid)).unwrap_or_default();
    egui::Window::new("Parameter Wiring").open(open).resizable(true).default_width(560.0).show(ctx, |ui| {
        ui.columns(2, |c| {
            c[0].label(egui::RichText::new("Source").strong());
            side(&mut c[0], app, "wire_source", &mut f.source);
            c[1].label(egui::RichText::new("Target").strong());
            side(&mut c[1], app, "wire_target", &mut f.target);
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.radio_value(&mut f.two_way, false, "One-way (Source -> Target)");
            ui.radio_value(&mut f.two_way, true, "Two-way");
        });
        ui.horizontal(|ui| {
            ui.label("Scale:");
            ui.add(crate::spinner::spin(ui, &mut f.scale, -1000.0..=1000.0));
            ui.label("Offset:");
            ui.add(crate::spinner::spin(ui, &mut f.offset, -100000.0..=100000.0));
        });
        ui.horizontal(|ui| {
            ui.label("Expression:");
            ui.add(egui::TextEdit::singleline(&mut f.expression).desired_width(320.0).hint_text("e.g. Source * 2"));
        });
        let ready = f.source.is_some() && f.target.is_some() && f.source != f.target;
        if ui.add_enabled(ready, egui::Button::new("Connect")).clicked() {
            if let (Some((sn, sp)), Some((tn, tp))) = (f.source.clone(), f.target.clone()) {
                let expression = (!f.expression.trim().is_empty()).then(|| f.expression.clone());
                let _ = app.dispatch(Command::WireParameter {
                    source_node: sn.0,
                    source_param: sp,
                    target_node: tn.0,
                    target_param: tp,
                    two_way: Some(f.two_way),
                    scale: Some(f.scale),
                    offset: Some(f.offset),
                    expression,
                });
            }
        }
        ui.separator();
        ui.label(egui::RichText::new("Wires").strong());
        let wires = app.state.scene.parameter_wires.clone();
        if wires.is_empty() {
            ui.weak("No wires.");
        }
        for (i, w) in wires.iter().enumerate() {
            ui.horizontal(|ui| {
                let arrow = if w.two_way { "↔" } else { "->" };
                ui.label(format!("{}.{} {arrow} {}.{}", node_name(app, w.source_node), w.source_param, node_name(app, w.target_node), w.target_param));
                if ui.small_button("Disconnect").clicked() {
                    let _ = app.dispatch(Command::DisconnectWireParameter { index: i as u32 });
                }
            });
        }
    });
    ctx.data_mut(|d| d.insert_temp(fid, f));
}

#[derive(Clone)]
struct NewAttribute {
    name: String,
    kind: String,
    value: f32,
    min: f32,
    max: f32,
}

impl Default for NewAttribute {
    fn default() -> Self {
        Self { name: "Param1".into(), kind: "float".into(), value: 0.0, min: -100.0, max: 100.0 }
    }
}

pub fn add_custom_attributes(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let fid = egui::Id::new("add_custom_attributes_form");
    let mut f: NewAttribute = ctx.data(|d| d.get_temp(fid)).unwrap_or_default();
    egui::Window::new("Add Parameter").open(open).resizable(false).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label("Name:");
            ui.text_edit_singleline(&mut f.name);
        });
        ui.horizontal(|ui| {
            ui.label("Parameter Type:");
            egui::ComboBox::from_id_salt("custom_attr_type").selected_text(match f.kind.as_str() {
                "integer" => "Integer",
                "boolean" => "Boolean",
                _ => "Float",
            }).show_ui(ui, |ui| {
                ui.selectable_value(&mut f.kind, "float".to_string(), "Float");
                ui.selectable_value(&mut f.kind, "integer".to_string(), "Integer");
                ui.selectable_value(&mut f.kind, "boolean".to_string(), "Boolean");
            });
        });
        if f.kind != "boolean" {
            ui.horizontal(|ui| {
                ui.label("Range From:");
                ui.add(crate::spinner::spin(ui, &mut f.min, -1.0e6..=1.0e6));
                ui.label("To:");
                ui.add(crate::spinner::spin(ui, &mut f.max, -1.0e6..=1.0e6));
            });
        }
        ui.horizontal(|ui| {
            ui.label("Default:");
            ui.add(crate::spinner::spin(ui, &mut f.value, -1.0e6..=1.0e6));
        });
        let can = !f.name.trim().is_empty() && !app.state.selection.is_empty();
        if ui.add_enabled(can, egui::Button::new("Add")).on_disabled_hover_text("Select an object and name the parameter").clicked() {
            let _ = app.dispatch(Command::AddCustomAttribute {
                id: None,
                name: f.name.trim().to_string(),
                attr_type: Some(f.kind.clone()),
                value: Some(f.value),
                min: Some(f.min),
                max: Some(f.max),
            });
        }
    });
    ctx.data_mut(|d| d.insert_temp(fid, f));
}

pub fn parameter_collector(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    egui::Window::new("Parameter Collector").open(open).resizable(true).default_width(360.0).show(ctx, |ui| {
        let ids: Vec<NodeId> = app.state.selection.iter().copied().collect();
        if ids.is_empty() {
            ui.weak("Select objects to collect their custom attributes.");
        }
        for id in ids {
            let Some(n) = app.state.scene.node(id) else { continue };
            let name = n.name.clone();
            let attrs = n.custom_attributes.clone();
            ui.label(egui::RichText::new(name).strong());
            if attrs.is_empty() {
                ui.weak("  No custom attributes.");
            }
            for a in attrs {
                ui.horizontal(|ui| {
                    ui.label(&a.name);
                    let mut v = a.value;
                    let changed = match a.attr_type.as_str() {
                        "boolean" => {
                            let mut b = v != 0.0;
                            let r = ui.checkbox(&mut b, "").changed();
                            v = if b { 1.0 } else { 0.0 };
                            r
                        }
                        "integer" => {
                            let mut i = v.round() as i32;
                            let r = ui.add(crate::spinner::spin(ui, &mut i, a.min.round() as i32..=a.max.round() as i32)).changed();
                            v = i as f32;
                            r
                        }
                        _ => ui.add(crate::spinner::spin(ui, &mut v, a.min..=a.max)).changed(),
                    };
                    if changed {
                        let _ = app.dispatch(Command::SetCustomAttribute { id: id.0, name: a.name.clone(), value: v });
                    }
                    if ui.small_button("Delete").clicked() {
                        let _ = app.dispatch(Command::DeleteCustomAttribute { id: id.0, name: a.name.clone() });
                    }
                });
            }
        }
    });
}
