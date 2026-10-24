//! The axis tripod shown at the transform centre when Select Object is
//! the active tool: three labelled lines in the reference system's
//! orientation; the constrained axes are red.

use super::*;
use loom_commands::AxisConstraint;

pub fn constrained(c: AxisConstraint, a: Axis) -> bool {
    matches!(
        (c, a),
        (AxisConstraint::X, Axis::X)
            | (AxisConstraint::Y, Axis::Y)
            | (AxisConstraint::Z, Axis::Z)
            | (AxisConstraint::XY, Axis::X | Axis::Y)
            | (AxisConstraint::YZ, Axis::Y | Axis::Z)
            | (AxisConstraint::ZX, Axis::Z | Axis::X)
            | (AxisConstraint::XYZ, _)
    )
}

pub fn draw(painter: &egui::Painter, app: &LoomApp, l: &GizmoLayout, p: &Projection) {
    if app.state.selection.is_empty() {
        return;
    }
    let l = GizmoLayout { size_px: l.size_px * 0.6, ..*l };
    for a in [Axis::X, Axis::Y, Axis::Z] {
        if let Some((s, e)) = axis_segment(&l, p, a, 0.0, 1.0) {
            let color = if constrained(app.state.axis_constraint, a) { egui::Color32::from_rgb(230, 60, 60) } else { egui::Color32::from_gray(210) };
            painter.line_segment([s, e], egui::Stroke::new(1.5, color));
            let d = pos_sub(e, s).normalize_or_zero();
            painter.text(egui::pos2(e.x + d.x * 8.0, e.y + d.y * 8.0), egui::Align2::CENTER_CENTER, a.label(), egui::FontId::proportional(11.0), color);
        }
    }
}
