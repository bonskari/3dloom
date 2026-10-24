//! Scale gizmo: axes ending in small boxes, triangles between axis pairs
//! for non-uniform (plane) scale, and a centre triangle for uniform scale.

use super::*;

const PLANE_FROM: f32 = 0.35;
const PLANE_TO: f32 = 0.6;
const UNIFORM: f32 = 0.3;

fn triangle(l: &GizmoLayout, p: &Projection, plane: Plane, from: f32, to: f32) -> Option<[egui::Pos2; 4]> {
    let len = l.world_len(p);
    let (ua, va) = plane.axes();
    let (u, v) = (l.axis(ua) * len, l.axis(va) * len);
    let o = l.origin;
    Some([p.screen(o + u * from)?, p.screen(o + u * to)?, p.screen(o + v * to)?, p.screen(o + v * from)?])
}

pub fn uniform_polygon(l: &GizmoLayout, p: &Projection) -> Option<Vec<egui::Pos2>> {
    let len = l.world_len(p) * UNIFORM;
    let pts: Option<Vec<_>> = [Axis::X, Axis::Y, Axis::Z].into_iter().map(|a| p.screen(l.origin + l.axis(a) * len)).collect();
    pts
}

pub fn hit(l: &GizmoLayout, p: &Projection, pointer: egui::Pos2) -> Option<Handle> {
    let o = p.screen(l.origin)?;
    if let Some(poly) = uniform_polygon(l, p) {
        if point_in_polygon(pointer, &poly) || pointer.distance(o) <= 5.0 {
            return Some(Handle::Uniform);
        }
    }
    for plane in Plane::ALL {
        if let Some(t) = triangle(l, p, plane, PLANE_FROM, PLANE_TO) {
            if point_in_polygon(pointer, &t) {
                return Some(Handle::Plane(plane));
            }
        }
    }
    [Axis::X, Axis::Y, Axis::Z]
        .into_iter()
        .filter_map(|a| {
            let (s, e) = axis_segment(l, p, a, 0.0, 1.0)?;
            let d = distance_to_segment(pointer, s, e);
            (d <= HIT_PX).then_some((a, d))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(a, _)| Handle::Axis(a))
}

/// Per-axis factors for a handle and a scalar factor.
pub fn factors(h: Handle, f: f32) -> Vec3 {
    match h {
        Handle::Axis(a) => Vec3::ONE + (f - 1.0) * move_gizmo::unit(a),
        Handle::Plane(pl) => {
            let (a, b) = pl.axes();
            Vec3::ONE + (f - 1.0) * (move_gizmo::unit(a) + move_gizmo::unit(b))
        }
        _ => Vec3::splat(f),
    }
}

pub fn draw(painter: &egui::Painter, l: &GizmoLayout, p: &Projection, hl: Option<Handle>, drag: Option<&DragState>) {
    let stretch = drag.map(|d| d.factor.clamp(0.2, 3.0)).unwrap_or(1.0);
    let sl = GizmoLayout { size_px: l.size_px * stretch * (l.scale_size / 100.0), ..*l };
    if let Some(poly) = uniform_polygon(&sl, p) {
        let active = hl == Some(Handle::Uniform);
        painter.add(egui::Shape::convex_polygon(poly, if active { HOVER.gamma_multiply(0.6) } else { egui::Color32::from_gray(180).gamma_multiply(0.4) }, egui::Stroke::new(1.0_f32, if active { HOVER } else { egui::Color32::from_gray(200) })));
    }
    for plane in Plane::ALL {
        if let Some(t) = triangle(&sl, p, plane, PLANE_FROM, PLANE_TO) {
            let active = hl == Some(Handle::Plane(plane));
            let (ua, va) = plane.axes();
            let fill = if active { HOVER.gamma_multiply(0.45) } else { egui::Color32::from_gray(160).gamma_multiply(0.25) };
            painter.add(egui::Shape::convex_polygon(t.to_vec(), fill, egui::Stroke::NONE));
            painter.line_segment([t[0], t[1]], egui::Stroke::new(1.5, if active { HOVER } else { axis_color(ua) }));
            painter.line_segment([t[2], t[3]], egui::Stroke::new(1.5, if active { HOVER } else { axis_color(va) }));
            painter.line_segment([t[1], t[2]], egui::Stroke::new(1.5, if active { HOVER } else { egui::Color32::from_gray(190) }));
            painter.line_segment([t[3], t[0]], egui::Stroke::new(1.5, if active { HOVER } else { egui::Color32::from_gray(190) }));
        }
    }
    for a in [Axis::X, Axis::Y, Axis::Z] {
        if let Some((s, e)) = axis_segment(&sl, p, a, 0.0, 1.0) {
            let active = hl == Some(Handle::Axis(a));
            let color = if active { HOVER } else { axis_color(a) };
            painter.line_segment([s, e], egui::Stroke::new(if active { 3.0 } else { 2.0 }, color));
            painter.rect_filled(egui::Rect::from_center_size(e, egui::vec2(7.0, 7.0)), 0.0, color);
            if l.axis_labels {
                let d = pos_sub(e, s).normalize_or_zero();
                painter.text(egui::pos2(e.x + d.x * 10.0, e.y + d.y * 10.0), egui::Align2::CENTER_CENTER, a.label(), egui::FontId::proportional(11.0), color);
            }
        }
    }
    if let (Some(d), Some(o)) = (drag, p.screen(l.origin)) {
        painter.text(egui::pos2(o.x + 14.0, o.y + 14.0), egui::Align2::LEFT_TOP, format!("{:.1}%", d.factor * 100.0), egui::FontId::proportional(11.0), HOVER);
    }
}
