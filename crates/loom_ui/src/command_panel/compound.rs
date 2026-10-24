//! Compound Objects rollouts: Boolean (Pick Boolean, Parameters),
//! ProBoolean (Pick Boolean, operands) and Loft (Creation Method, Path
//! Parameters, Skin Parameters).

use crate::command_panel::rollout;
use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, PickMode};
use loom_scene::ObjectKind;

/// Create > Compound Objects > Object Type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompoundKind {
    Boolean,
    ProBoolean,
    Loft,
}

impl CompoundKind {
    pub const ALL: [CompoundKind; 3] = [CompoundKind::Boolean, CompoundKind::ProBoolean, CompoundKind::Loft];
    pub fn label(self) -> &'static str {
        match self {
            CompoundKind::Boolean => "Boolean",
            CompoundKind::ProBoolean => "ProBoolean",
            CompoundKind::Loft => "Loft",
        }
    }
}

fn setting(app: &mut LoomApp, name: &str, value: f64) {
    let _ = app.dispatch(Command::CompoundSettings { name: name.into(), value });
}

/// The rollouts for the pressed Object Type (creation) or the selected compound object.
pub fn rollouts(ui: &mut egui::Ui, app: &mut LoomApp, kind: Option<CompoundKind>) {
    let selected = app.state.selection.iter().next().copied().and_then(|id| app.state.scene.node(id).map(|n| n.object.clone()));
    let kind = kind.or(match &selected {
        Some(ObjectKind::Boolean { .. }) => Some(CompoundKind::Boolean),
        Some(ObjectKind::ProBoolean { .. }) => Some(CompoundKind::ProBoolean),
        Some(ObjectKind::Loft { .. }) => Some(CompoundKind::Loft),
        _ => None,
    });
    let Some(kind) = kind else { return };
    let cs = app.state.compound_settings;
    match kind {
        CompoundKind::Boolean => {
            rollout(ui, "Pick Boolean", true, |ui| {
                let picking = app.state.pick_mode == Some(PickMode::BooleanOperandB);
                let is_boolean = matches!(selected, Some(ObjectKind::Boolean { .. }));
                if ui.add_enabled(!is_boolean, egui::SelectableLabel::new(picking, "Pick Operand B")).on_hover_text("Click the second object in a viewport").clicked() {
                    let _ = app.dispatch(Command::PickOperandB);
                }
                ui.horizontal(|ui| {
                    if ui.radio(!cs.move_operand, "Copy").clicked() {
                        setting(app, "Move Operand", 0.0);
                    }
                    if ui.radio(cs.move_operand, "Move").clicked() {
                        setting(app, "Move Operand", 1.0);
                    }
                });
                ui.label("Operation:");
                let ops = ["Union", "Intersection", "Subtraction (A-B)", "Subtraction (B-A)", "Cut Refine", "Cut Split", "Cut Remove Inside", "Cut Remove Outside"];
                for (i, l) in ops.iter().enumerate() {
                    if ui.radio(cs.boolean_op == i as u32, *l).clicked() {
                        setting(app, "Boolean Operation", i as f64);
                    }
                }
            });
            if let Some(ObjectKind::Boolean { .. }) = &selected {
                if let Some(id) = app.state.selection.iter().next().copied() {
                    if let Some(err) = app.state.scene.node(id).and_then(|n| n.object.compound_error()) {
                        ui.colored_label(egui::Color32::from_rgb(255, 180, 60), err);
                    }
                }
            }
        }
        CompoundKind::ProBoolean => {
            rollout(ui, "Pick Boolean", true, |ui| {
                let picking = app.state.pick_mode == Some(PickMode::ProBooleanOperand);
                let is_pro = matches!(selected, Some(ObjectKind::ProBoolean { .. }));
                if !is_pro && ui.button("Start ProBoolean").on_hover_text("The selected object becomes the ProBoolean base").clicked() {
                    let _ = app.dispatch(Command::CreateProBoolean);
                }
                if ui.add_enabled(is_pro, egui::SelectableLabel::new(picking, "Start Picking")).on_hover_text("Click operands in a viewport").clicked() {
                    let _ = app.dispatch(Command::ProBooleanPickOperand);
                }
                ui.horizontal(|ui| {
                    if ui.radio(!cs.move_operand, "Copy").clicked() {
                        setting(app, "Move Operand", 0.0);
                    }
                    if ui.radio(cs.move_operand, "Move").clicked() {
                        setting(app, "Move Operand", 1.0);
                    }
                });
                ui.label("Operation:");
                for (i, l) in loom_scene::node::PROBOOLEAN_OPS.iter().enumerate() {
                    if ui.radio(cs.proboolean_op == i as u32, *l).clicked() {
                        setting(app, "ProBoolean Operation", i as f64);
                    }
                }
            });
            if let Some(ObjectKind::ProBoolean { operands, .. }) = &selected {
                rollout(ui, "Parameters", true, |ui| {
                    ui.label(format!("Operands: {}", operands.len()));
                    for (i, (op, m)) in operands.iter().enumerate() {
                        ui.label(format!("{}: {} ({} faces)", i + 1, loom_scene::node::PROBOOLEAN_OPS[(*op as usize).min(5)], m.face_count()));
                    }
                    if let Some(id) = app.state.selection.iter().next().copied() {
                        if let Some(err) = app.state.scene.node(id).and_then(|n| n.object.compound_error()) {
                            ui.colored_label(egui::Color32::from_rgb(255, 180, 60), err);
                        }
                    }
                });
            }
        }
        CompoundKind::Loft => {
            rollout(ui, "Creation Method", true, |ui| {
                ui.horizontal(|ui| {
                    let path_on = app.state.pick_mode == Some(PickMode::LoftGetPath);
                    let is_loft = matches!(selected, Some(ObjectKind::Loft { .. }));
                    if ui.add_enabled(!is_loft, egui::SelectableLabel::new(path_on, "Get Path")).on_hover_text("The selected shape is the cross-section: click the path").clicked() {
                        let _ = app.dispatch(Command::LoftGetPathMode);
                    }
                    let shape_on = app.state.pick_mode == Some(PickMode::LoftGetShape);
                    if ui.selectable_label(shape_on, "Get Shape").on_hover_text("Click a shape to place at the Path percent").clicked() {
                        let _ = app.dispatch(Command::LoftGetShapeMode);
                    }
                });
            });
            rollout(ui, "Path Parameters", true, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Path:");
                    let mut p = cs.loft_path_percent;
                    if ui.add(crate::spinner::spin(ui, &mut p, 0.0..=100.0).speed(0.5).suffix(" %")).changed() {
                        setting(app, "Loft Path", p as f64);
                    }
                });
                if let Some(ObjectKind::Loft { shapes, .. }) = &selected {
                    for (pct, _) in shapes {
                        ui.label(format!("shape at {pct:.1} %"));
                    }
                }
            });
            if let Some(id) = app.state.selection.iter().next().copied() {
                if let Some(err) = app.state.scene.node(id).and_then(|n| n.object.compound_error()) {
                    ui.colored_label(egui::Color32::from_rgb(255, 180, 60), err);
                }
            }
        }
    }
}
