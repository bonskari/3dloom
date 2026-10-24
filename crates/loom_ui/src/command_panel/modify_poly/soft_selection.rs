//! Editable Poly > Soft Selection rollout: Use Soft Selection, Edge
//! Distance, Affect Backfacing, Falloff / Pinch / Bubble with the curve
//! graph, Shaded Face Toggle, Lock Soft Selection, Paint Soft Selection.
//! Editable Spline shows the same rollout without the face and paint rows
//! (the spline Soft Selection has Falloff / Pinch / Bubble and Edge
//! Distance only).

use crate::command_panel::rollout;
use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, SoftPaint};

pub fn show(ui: &mut egui::Ui, app: &mut LoomApp) {
    let Some(id) = app.state.selection.iter().next().copied() else { return };
    let node = app.state.scene.node(id);
    let spline = node.is_some_and(|n| n.edit_poly().is_none() && n.edit_spline().is_some());
    let Some(soft) = node.and_then(|n| n.edit_poly().map(|p| p.soft.clone()).or_else(|| n.edit_spline().map(|s| s.soft.clone()))) else { return };
    rollout(ui, "Soft Selection", false, |ui| {
        let mut on = soft.on;
        if ui.checkbox(&mut on, "Use Soft Selection").changed() {
            let _ = app.dispatch(Command::UseSoftSelection);
        }
        ui.add_enabled_ui(on, |ui| {
            ui.horizontal(|ui| {
                let mut ed = soft.edge_distance.is_some();
                if ui.checkbox(&mut ed, "Edge Distance:").changed() {
                    let _ = app.dispatch(Command::SoftSelectionSettings { falloff: None, pinch: None, bubble: None, edge_distance: Some(if ed { 1 } else { 0 }), affect_backfacing: None });
                }
                let mut n = soft.edge_distance.unwrap_or(1);
                if ui.add_enabled(ed, egui::DragValue::new(&mut n).speed(0.1).range(1..=999)).changed() {
                    let _ = app.dispatch(Command::SoftSelectionSettings { falloff: None, pinch: None, bubble: None, edge_distance: Some(n), affect_backfacing: None });
                }
            });
            if !spline {
                let mut ab = soft.affect_backfacing;
                if ui.checkbox(&mut ab, "Affect Backfacing").changed() {
                    let _ = app.dispatch(Command::AffectBackfacing);
                }
            }
            let speed = app.spinner_speed(0.1);
            let mut falloff = soft.falloff.get();
            let mut pinch = soft.pinch.get();
            let mut bubble = soft.bubble.get();
            egui::Grid::new("soft_params").num_columns(2).spacing([8.0, 2.0]).show(ui, |ui| {
                ui.label("Falloff:");
                if ui.add(app.length_drag(&mut falloff, speed)).changed() {
                    let _ = app.dispatch(Command::SoftSelectionSettings { falloff: Some(falloff), pinch: None, bubble: None, edge_distance: None, affect_backfacing: None });
                }
                ui.end_row();
                ui.label("Pinch:");
                if ui.add(egui::DragValue::new(&mut pinch).speed(0.01).range(-1.0..=1.0).fixed_decimals(2)).changed() {
                    let _ = app.dispatch(Command::SoftSelectionSettings { falloff: None, pinch: Some(pinch), bubble: None, edge_distance: None, affect_backfacing: None });
                }
                ui.end_row();
                ui.label("Bubble:");
                if ui.add(egui::DragValue::new(&mut bubble).speed(0.01).range(-1.0..=1.0).fixed_decimals(2)).changed() {
                    let _ = app.dispatch(Command::SoftSelectionSettings { falloff: None, pinch: None, bubble: Some(bubble), edge_distance: None, affect_backfacing: None });
                }
                ui.end_row();
            });
            // The falloff curve: weight over distance.
            let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width().min(220.0), 70.0), egui::Sense::hover());
            let p = ui.painter_at(rect);
            p.rect_filled(rect, 2.0, egui::Color32::from_gray(30));
            let pts: Vec<egui::Pos2> = (0..=60)
                .map(|i| {
                    let t = i as f32 / 60.0;
                    let w = loom_mesh::soft_select::curve(t * falloff.max(1e-3), falloff.max(1e-3), pinch, bubble);
                    egui::pos2(rect.left() + 8.0 + t * (rect.width() - 16.0), rect.bottom() - 6.0 - w * (rect.height() - 12.0))
                })
                .collect();
            p.add(egui::Shape::line(pts, egui::Stroke::new(1.5, egui::Color32::from_rgb(255, 200, 80))));
            if ui.selectable_label(app.state.soft_edit, "Edit Soft Selection").on_hover_text(crate::shortcuts::tip(&app.state, "Edit Soft Selection: drag in the viewport: Falloff (Ctrl: Pinch, Shift: Bubble)", "Edit Soft Selection")).clicked() {
                let _ = app.dispatch(Command::EditSoftSelection);
            }
            if spline {
                return;
            }
            ui.horizontal(|ui| {
                let mut shaded = app.state.soft_shaded_faces;
                if ui.checkbox(&mut shaded, "Shaded Face Toggle").changed() {
                    let _ = app.dispatch(Command::ShadedFaceToggle);
                }
                let mut lock = soft.lock;
                if ui.checkbox(&mut lock, "Lock Soft Selection").changed() {
                    let _ = app.dispatch(Command::LockSoftSelection);
                }
            });
            ui.label(egui::RichText::new("Paint Soft Selection").strong());
            ui.horizontal(|ui| {
                if ui.selectable_label(app.state.soft_paint == Some(SoftPaint::Paint), "Paint").clicked() {
                    let _ = app.dispatch(Command::PaintSoftSelection);
                }
                if ui.selectable_label(app.state.soft_paint == Some(SoftPaint::Blur), "Blur").clicked() {
                    let _ = app.dispatch(Command::PaintSoftSelectionBlur);
                }
                if ui.button("Revert").clicked() {
                    let _ = app.dispatch(Command::SoftSelectionRevert);
                }
            });
            egui::Grid::new("soft_brush").num_columns(2).spacing([8.0, 2.0]).show(ui, |ui| {
                ui.label("Selection Value:");
                ui.add(egui::DragValue::new(&mut app.soft_brush.value).speed(0.01).range(0.0..=1.0).fixed_decimals(2));
                ui.end_row();
                ui.label("Brush Size:");
                ui.add(egui::DragValue::new(&mut app.soft_brush.size).speed(speed).range(0.001..=1e6).fixed_decimals(1));
                ui.end_row();
                ui.label("Brush Strength:");
                ui.add(egui::DragValue::new(&mut app.soft_brush.strength).speed(0.01).range(0.0..=1.0).fixed_decimals(2));
                ui.end_row();
            });
        });
    });
}
