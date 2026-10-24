//! Create tab: category buttons, Standard Primitives Object Type rollout,
//! Name and Color, Keyboard Entry and the selected object's Parameters.

use super::rollout;
use crate::{CreateKind, LineDrag, LoomApp};
use eframe::egui;
use loom_commands::{Command, PrimitiveKind, ShapeKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Category {
    #[default]
    Geometry,
    Shapes,
    Lights,
    Cameras,
    Helpers,
    SpaceWarps,
    Systems,
}

impl Category {
    pub const ALL: [Category; 7] = [
        Category::Geometry, Category::Shapes, Category::Lights, Category::Cameras, Category::Helpers, Category::SpaceWarps, Category::Systems,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Category::Geometry => "Geometry",
            Category::Shapes => "Shapes",
            Category::Lights => "Lights",
            Category::Cameras => "Cameras",
            Category::Helpers => "Helpers",
            Category::SpaceWarps => "Space Warps",
            Category::Systems => "Systems",
        }
    }
}

pub fn show(ui: &mut egui::Ui, app: &mut LoomApp) {
    ui.horizontal_wrapped(|ui| {
        for c in Category::ALL {
            if ui.selectable_label(app.create_category == c, c.label()).clicked() {
                app.create_category = c;
                app.create_kind = None;
            }
        }
    });
    match app.create_category {
        Category::Geometry if app.create_compound => {
            egui::ComboBox::from_id_salt("create_geometry_class").selected_text("Compound Objects").show_ui(ui, |ui| {
                if ui.selectable_label(false, "Standard Primitives").clicked() {
                    app.create_compound = false;
                    app.compound_kind = None;
                }
                let _ = ui.selectable_label(true, "Compound Objects");
            });
            rollout(ui, "Object Type", true, |ui| {
                ui.checkbox(&mut app.autogrid, "AutoGrid");
                egui::Grid::new("compound_type").num_columns(2).show(ui, |ui| {
                    for (i, kind) in super::compound::CompoundKind::ALL.iter().enumerate() {
                        let on = app.compound_kind == Some(*kind);
                        if ui.add_sized([110.0, 22.0], egui::SelectableLabel::new(on, kind.label())).clicked() {
                            app.compound_kind = if on { None } else { Some(*kind) };
                            app.create_kind = None;
                            let _ = app.dispatch(Command::SelectObject);
                        }
                        if i % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
            });
            name_and_color(ui, app);
            super::compound::rollouts(ui, app, app.compound_kind);
            parameters(ui, app);
        }
        Category::Geometry => {
            egui::ComboBox::from_id_salt("create_geometry_class").selected_text("Standard Primitives").show_ui(ui, |ui| {
                let _ = ui.selectable_label(true, "Standard Primitives");
                if ui.selectable_label(false, "Compound Objects").clicked() {
                    app.create_compound = true;
                    app.create_kind = None;
                }
            });
            rollout(ui, "Object Type", true, |ui| {
                ui.checkbox(&mut app.autogrid, "AutoGrid");
                egui::Grid::new("object_type").num_columns(2).show(ui, |ui| {
                    for (i, kind) in PrimitiveKind::ALL.iter().enumerate() {
                        let on = app.create_kind == Some(CreateKind::Primitive(*kind));
                        if ui.add_sized([110.0, 22.0], egui::SelectableLabel::new(on, kind.label())).clicked() {
                            app.create_kind = if on { None } else { Some(CreateKind::Primitive(*kind)) };
                            if app.create_kind.is_some() {
                                let _ = app.dispatch(Command::SelectObject);
                            }
                        }
                        if i % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
            });
            name_and_color(ui, app);
            if let Some(CreateKind::Primitive(kind)) = app.create_kind {
                keyboard_entry(ui, app, kind);
            }
            parameters(ui, app);
        }
        Category::Shapes => {
            egui::ComboBox::from_id_salt("create_shape_class").selected_text("Splines").show_ui(ui, |ui| {
                let _ = ui.selectable_label(true, "Splines");
            });
            rollout(ui, "Object Type", true, |ui| {
                ui.checkbox(&mut app.autogrid, "AutoGrid");
                egui::Grid::new("shape_type").num_columns(2).show(ui, |ui| {
                    let kinds: Vec<(CreateKind, &str)> = std::iter::once((CreateKind::Line, "Line")).chain(ShapeKind::ALL.iter().map(|k| (CreateKind::Shape(*k), k.label()))).collect();
                    for (i, (kind, label)) in kinds.iter().enumerate() {
                        let on = app.create_kind == Some(*kind);
                        if ui.add_sized([110.0, 22.0], egui::SelectableLabel::new(on, *label)).clicked() {
                            app.create_kind = if on { None } else { Some(*kind) };
                            if let Some(line) = app.line.take() {
                                line.finish(app);
                            }
                            if app.create_kind.is_some() {
                                let _ = app.dispatch(Command::SelectObject);
                            }
                        }
                        if i % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
            });
            name_and_color(ui, app);
            if app.create_kind == Some(CreateKind::Line) {
                rollout(ui, "Creation Method", true, |ui| {
                    ui.label("Initial Type");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut app.line_initial_smooth, false, "Corner");
                        ui.radio_value(&mut app.line_initial_smooth, true, "Smooth");
                    });
                    ui.label("Drag Type");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut app.line_drag, LineDrag::Corner, "Corner");
                        ui.radio_value(&mut app.line_drag, LineDrag::Smooth, "Smooth");
                        ui.radio_value(&mut app.line_drag, LineDrag::Bezier, "Bezier");
                    });
                });
            }
            if let Some(CreateKind::Shape(kind)) = app.create_kind {
                rollout(ui, "Keyboard Entry", false, |ui| {
                    let speed = app.spinner_speed(0.1);
                    let ke = &mut app.keyboard_entry;
                    ui.horizontal(|ui| {
                        for (v, l) in ke.position.iter_mut().zip(["X:", "Y:", "Z:"]) {
                            ui.label(l);
                            ui.add(egui::DragValue::new(v).speed(speed));
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Size:");
                        for v in ke.size.iter_mut() {
                            ui.add(egui::DragValue::new(v).speed(speed).range(0.0..=1e6));
                        }
                    });
                    let (position, size) = (ke.position, ke.size);
                    if ui.button("Create").clicked() {
                        let _ = app.dispatch(Command::CreateShape { kind, position, size });
                    }
                });
            }
            parameters(ui, app);
        }
        Category::Cameras => {
            rollout(ui, "Object Type", true, |ui| {
                for (label, target) in [("Target", true), ("Free", false)] {
                    if ui.add_sized([110.0, 22.0], egui::Button::new(label)).clicked() {
                        let _ = app.dispatch(Command::CreateCamera { target });
                    }
                }
            });
            name_and_color(ui, app);
            parameters(ui, app);
        }
        Category::Helpers => {
            rollout(ui, "Object Type", true, |ui| {
                ui.checkbox(&mut app.autogrid, "AutoGrid");
                egui::Grid::new("helper_type").num_columns(2).show(ui, |ui| {
                    for (i, (kind, label)) in [(crate::HelperKind::Dummy, "Dummy"), (crate::HelperKind::Point, "Point")].iter().enumerate() {
                        let on = app.create_kind == Some(CreateKind::Helper(*kind));
                        if ui.add_sized([110.0, 22.0], egui::SelectableLabel::new(on, *label)).clicked() {
                            app.create_kind = if on { None } else { Some(CreateKind::Helper(*kind)) };
                            if app.create_kind.is_some() {
                                let _ = app.dispatch(Command::SelectObject);
                            }
                        }
                        if i % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
            });
            name_and_color(ui, app);
            parameters(ui, app);
        }
        Category::Systems => {
            rollout(ui, "Object Type", true, |ui| {
                ui.checkbox(&mut app.autogrid, "AutoGrid");
                let on = app.create_kind == Some(CreateKind::Bones);
                if ui.add_sized([110.0, 22.0], egui::SelectableLabel::new(on, "Bones")).clicked() {
                    app.create_kind = if on { None } else { Some(CreateKind::Bones) };
                    app.bone_points.clear();
                    if app.create_kind.is_some() {
                        let _ = app.dispatch(Command::SelectObject);
                    }
                }
            });
            name_and_color(ui, app);
            if app.create_kind == Some(CreateKind::Bones) {
                rollout(ui, "IK Chain Assignment", true, |ui| {
                    ui.label("Click joints in a viewport; right-click ends the chain.");
                    if ui.button("Bone Tools...").clicked() {
                        let _ = app.dispatch(Command::BoneTools);
                    }
                });
            }
            parameters(ui, app);
        }
        // Lights and Space Warps have no object types (out of scope); they show an empty Object Type rollout.
        _ => {
            rollout(ui, "Object Type", true, |_ui| {});
        }
    }
}

pub fn name_and_color(ui: &mut egui::Ui, app: &mut LoomApp) {
    rollout(ui, "Name and Color", true, |ui| {
        let Some(id) = app.state.selection.iter().next().copied() else {
            ui.label("");
            return;
        };
        let Some(node) = app.state.scene.node(id) else { return };
        let mut name = node.name.clone();
        let mut rgb = node.wire_color;
        ui.horizontal(|ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(170.0));
            if r.lost_focus() && name != app.state.scene.node(id).map(|n| n.name.clone()).unwrap_or_default() {
                let _ = app.dispatch(Command::Rename { id: id.0, name: name.clone() });
            }
            if ui.color_edit_button_srgb(&mut rgb).changed() {
                let _ = app.dispatch(Command::SetWireColor { id: id.0, rgb });
            }
        });
    });
}

fn keyboard_entry(ui: &mut egui::Ui, app: &mut LoomApp, kind: PrimitiveKind) {
    rollout(ui, "Keyboard Entry", false, |ui| {
        let speed = app.spinner_speed(0.1);
        let ke = &mut app.keyboard_entry;
        ui.horizontal(|ui| {
            for (v, l) in ke.position.iter_mut().zip(["X:", "Y:", "Z:"]) {
                ui.label(l);
                ui.add(egui::DragValue::new(v).speed(speed));
            }
        });
        ui.horizontal(|ui| {
            ui.label("Size:");
            for v in ke.size.iter_mut() {
                ui.add(egui::DragValue::new(v).speed(speed).range(0.0..=1e6));
            }
        });
        let (position, size) = (ke.position, ke.size);
        if ui.button("Create").clicked() {
            let _ = app.dispatch(Command::CreatePrimitive { kind, position, size });
        }
    });
}

/// Parameters rollout of the selected parametric object; spinner drags are
/// one undo entry each.
pub fn parameters(ui: &mut egui::Ui, app: &mut LoomApp) {
    let Some(id) = app.state.selection.iter().next().copied() else { return };
    let Some(node) = app.state.scene.node(id) else { return };
    let params = node.object.parameters();
    let (is_compound, is_shape) = (node.object.is_compound(), node.object.is_shape());
    if is_compound && !matches!(app.panel_tab, crate::Tab::Create) {
        super::compound::rollouts(ui, app, None);
    }
    if params.is_empty() {
        return;
    }
    if is_shape {
        // Shapes: Rendering, Interpolation and Parameters rollouts.
        let interp = ["Steps", "Optimize", "Adaptive"];
        let render: Vec<_> = loom_scene::node::SHAPE_COMMON_NAMES.iter().filter(|n| !interp.contains(n)).copied().collect();
        let own: Vec<_> = params.iter().filter(|(n, _)| !loom_scene::node::SHAPE_COMMON_NAMES.contains(n)).cloned().collect();
        let rend: Vec<_> = params.iter().filter(|(n, _)| render.contains(n)).cloned().collect();
        let intp: Vec<_> = params.iter().filter(|(n, _)| interp.contains(n)).cloned().collect();
        rollout(ui, "Rendering", false, |ui| param_grid(ui, app, id, "rendering", &rend));
        rollout(ui, "Interpolation", false, |ui| param_grid(ui, app, id, "interpolation", &intp));
        if !own.is_empty() {
            rollout(ui, "Parameters", true, |ui| param_grid(ui, app, id, "parameters", &own));
        }
        return;
    }
    rollout(ui, "Parameters", true, |ui| param_grid(ui, app, id, "parameters", &params));
}

/// One grid of spinners / checkboxes for the given parameters.
fn param_grid(ui: &mut egui::Ui, app: &mut LoomApp, id: loom_scene::NodeId, salt: &str, params: &[(&'static str, loom_scene::ParamValue)]) {
    let params = params.to_vec();
    {
        egui::Grid::new((salt, id.0)).num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            for (name, value) in params {
                ui.label(name);
                match value {
                    loom_scene::ParamValue::Float(f) => {
                        let mut v = f;
                        let length = ["Length", "Width", "Height", "Depth", "Radius", "Radius 1", "Radius 2"].contains(&name);
                        let r = if length {
                            ui.add(app.length_drag(&mut v, app.spinner_speed(0.05)))
                        } else {
                            ui.add(egui::DragValue::new(&mut v).speed(app.spinner_speed(0.05)).fixed_decimals(app.state.prefs.spinner_precision))
                        };
                        spin(app, id, name, &r, v as f64, f as f64);
                    }
                    loom_scene::ParamValue::Int(i) if app.state.scene.node(id).and_then(|n| n.object.choices(name)).is_some() => {
                        let choices = app.state.scene.node(id).and_then(|n| n.object.choices(name)).unwrap();
                        ui.vertical(|ui| {
                            for (ci, c) in choices.iter().enumerate() {
                                if ui.radio(i as usize == ci, *c).clicked() && i as usize != ci {
                                    let _ = app.dispatch(Command::SetParameter { id: id.0, name: name.into(), value: ci as f64 });
                                }
                            }
                        });
                    }
                    loom_scene::ParamValue::Int(i) => {
                        let mut v = i;
                        let r = ui.add(egui::DragValue::new(&mut v).speed(app.spinner_speed(0.1)));
                        spin(app, id, name, &r, v as f64, i as f64);
                    }
                    loom_scene::ParamValue::Bool(b) => {
                        let mut v = b;
                        if ui.checkbox(&mut v, "").changed() {
                            let _ = app.dispatch(Command::SetParameter { id: id.0, name: name.into(), value: if v { 1.0 } else { 0.0 } });
                        }
                    }
                }
                ui.end_row();
            }
        });
    }
}

fn spin(app: &mut LoomApp, id: loom_scene::NodeId, name: &str, r: &egui::Response, new: f64, old: f64) {
    if r.drag_started() {
        app.state.undo.begin(name);
    }
    if r.changed() && (new - old).abs() > 1e-9 {
        let _ = app.dispatch(Command::SetParameter { id: id.0, name: name.into(), value: new });
    }
    if r.drag_stopped() {
        app.state.undo.end();
    }
}
