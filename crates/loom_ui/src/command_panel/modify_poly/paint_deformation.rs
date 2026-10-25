//! Editable Poly > Paint Deformation rollout: Push/Pull, Relax,
//! Revert, Commit, Cancel, Push/Pull Value, Brush Size, Brush Strength,
//! Push/Pull Direction.

use crate::command_panel::rollout;
use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, PaintMode};
use loom_scene::PaintDirection;

pub fn show(ui: &mut egui::Ui, app: &mut LoomApp) {
    let Some(id) = app.state.selection.iter().next().copied() else { return };
    let Some(paint) = app.state.scene.node(id).and_then(|n| n.edit_poly()).map(|p| p.paint.clone()) else { return };
    rollout(ui, "Paint Deformation", false, |ui| {
        ui.horizontal(|ui| {
            for (mode, l, cmd) in [(PaintMode::PushPull, "Push/Pull", Command::PaintDeformPushPull), (PaintMode::Relax, "Relax", Command::PaintDeformRelax), (PaintMode::Revert, "Revert", Command::PaintDeformRevert)] {
                if ui.selectable_label(app.state.paint_deform == Some(mode), l).clicked() {
                    let _ = app.dispatch(cmd);
                }
            }
        });
        ui.horizontal(|ui| {
            let painted = paint.original.is_some();
            if ui.add_enabled(painted, egui::Button::new("Commit")).clicked() {
                let _ = app.dispatch(Command::PaintDeformCommit);
            }
            if ui.add_enabled(painted, egui::Button::new("Cancel")).clicked() {
                let _ = app.dispatch(Command::PaintDeformCancel);
            }
        });
        egui::Grid::new("paint_deform_grid").num_columns(2).show(ui, |ui| {
            ui.label("Push/Pull Value:");
            let mut v = paint.push_pull_value;
            if ui.add(crate::spinner::spin(ui, &mut v, ..).speed(0.1).fixed_decimals(2)).changed() {
                let _ = app.dispatch(Command::PaintDeformSettings { value: Some(v), size: None, strength: None, direction: None });
            }
            ui.end_row();
            ui.label("Brush Size:");
            let mut s = paint.brush_size;
            if ui.add(crate::spinner::spin(ui, &mut s, 0.01..=f32::MAX).speed(0.1).fixed_decimals(2)).changed() {
                let _ = app.dispatch(Command::PaintDeformSettings { value: None, size: Some(s), strength: None, direction: None });
            }
            ui.end_row();
            ui.label("Brush Strength:");
            let mut st = paint.brush_strength;
            if ui.add(crate::spinner::spin(ui, &mut st, 0.0..=1.0).speed(0.01).fixed_decimals(2)).changed() {
                let _ = app.dispatch(Command::PaintDeformSettings { value: None, size: None, strength: Some(st), direction: None });
            }
            ui.end_row();
        });
        ui.label(egui::RichText::new("Push/Pull Direction").strong());
        let dir = paint.direction;
        for (d, l, name) in [
            (PaintDirection::OriginalNormals, "Original Normals", "OriginalNormals"),
            (PaintDirection::DeformedNormals, "Deformed Normals", "DeformedNormals"),
        ] {
            if ui.radio(dir == d, l).clicked() {
                let _ = app.dispatch(Command::PaintDeformSettings { value: None, size: None, strength: None, direction: Some(name.into()) });
            }
        }
        ui.horizontal(|ui| {
            ui.label("Transform axis:");
            for (a, l) in [(0u8, "X"), (1, "Y"), (2, "Z")] {
                if ui.radio(dir == PaintDirection::TransformAxis(a), l).clicked() {
                    let _ = app.dispatch(Command::PaintDeformSettings { value: None, size: None, strength: None, direction: Some(l.into()) });
                }
            }
        });
    });
}
