//! Editable Spline rollouts of the Modify panel: Selection and Geometry
//! (Rendering and Interpolation come from the shape parameters).

pub mod geometry;
pub mod selection;

use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;

/// A Spline Settings checkbox bound to one named setting.
pub fn setting_check(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, name: &str, value: bool) {
    let mut v = value;
    if ui.checkbox(&mut v, label).changed() {
        let _ = app.dispatch(Command::SplineSettings { name: name.into(), value: if v { 1.0 } else { 0.0 } });
    }
}

/// A Spline Settings spinner bound to one named setting.
pub fn setting_spin(ui: &mut egui::Ui, app: &mut LoomApp, name: &str, value: f32, speed: f32, min: f32) {
    let mut v = value;
    if ui.add(egui::DragValue::new(&mut v).speed(speed).range(min..=f32::MAX).fixed_decimals(2)).changed() {
        let _ = app.dispatch(Command::SplineSettings { name: name.into(), value: v as f64 });
    }
}
