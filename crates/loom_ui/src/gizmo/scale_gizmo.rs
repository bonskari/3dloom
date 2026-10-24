//! Scale gizmo: a triangle. Three axes ending in small boxes, an inner
//! triangle between them for uniform scale and, for each axis pair, the band
//! between the inner and the outer triangle edge for two-axis scale. Each
//! triangle edge is half one axis's colour, half the other's.

use super::*;

/// Inner triangle corners, as a fraction of the axis length.
const INNER: f32 = 0.5;
/// Outer triangle corners: the plane bands span INNER..OUTER.
const OUTER: f32 = 0.7;
const END_BOX_PX: f32 = 6.0;

const FILL_GREY: egui::Color32 = egui::Color32::from_gray(214);

/// The layout the gizmo is drawn and hit with: Preferences > Gizmos > Scale size applied.
fn sized(l: &GizmoLayout) -> GizmoLayout {
    GizmoLayout { size_px: l.size_px * (l.scale_size / 100.0), ..*l }
}

fn corner(l: &GizmoLayout, p: &Projection, a: Axis, at: f32) -> Option<egui::Pos2> {
    p.screen(l.origin + l.axis(a) * (l.world_len(p) * at))
}

/// The two-axis handle: the band between the inner and outer edge of `plane`.
pub fn plane_band(l: &GizmoLayout, p: &Projection, plane: Plane) -> Option<[egui::Pos2; 4]> {
    let (ua, va) = plane.axes();
    Some([corner(l, p, ua, INNER)?, corner(l, p, ua, OUTER)?, corner(l, p, va, OUTER)?, corner(l, p, va, INNER)?])
}

pub fn uniform_polygon(l: &GizmoLayout, p: &Projection) -> Option<Vec<egui::Pos2>> {
    [Axis::X, Axis::Y, Axis::Z].into_iter().map(|a| corner(l, p, a, INNER)).collect()
}

pub fn hit(l: &GizmoLayout, p: &Projection, pointer: egui::Pos2) -> Option<Handle> {
    let l = &sized(l);
    let o = p.screen(l.origin)?;
    // Uniform: inside the inner triangle, including the axis lines within it.
    let on_inner_axis = [Axis::X, Axis::Y, Axis::Z]
        .into_iter()
        .filter_map(|a| axis_segment(l, p, a, 0.0, INNER))
        .any(|(s, e)| distance_to_segment(pointer, s, e) <= HIT_PX);
    if let Some(poly) = uniform_polygon(l, p) {
        if point_in_polygon(pointer, &poly) || on_inner_axis || pointer.distance(o) <= 5.0 {
            return Some(Handle::Uniform);
        }
    }
    for plane in Plane::ALL {
        if let Some(t) = plane_band(l, p, plane) {
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

/// Squash: the handle's axes scale by `f`, the others the opposite way so
/// the volume stays (axis: 1/√f each; plane: 1/f² on the normal). The
/// centre handle has no other axis and scales uniformly.
pub fn squash_factors(h: Handle, f: f32) -> Vec3 {
    match h {
        Handle::Axis(a) => {
            let u = move_gizmo::unit(a);
            u * f + (Vec3::ONE - u) / f.sqrt()
        }
        Handle::Plane(pl) => {
            let n = move_gizmo::unit(pl.normal());
            (Vec3::ONE - n) * f + n / (f * f)
        }
        _ => factors(h, f),
    }
}

/// Whether axis `a` is part of handle `h`.
fn involves(h: Option<Handle>, a: Axis) -> bool {
    match h {
        Some(Handle::Axis(b)) => a == b,
        Some(Handle::Plane(pl)) => pl.axes().0 == a || pl.axes().1 == a,
        Some(Handle::Uniform) => true,
        _ => false,
    }
}

pub fn draw(painter: &egui::Painter, l: &GizmoLayout, p: &Projection, hl: Option<Handle>, drag: Option<&DragState>) {
    // While dragging, the gizmo stretches along the scaled axes.
    let mut sl = sized(l);
    if let Some(d) = drag {
        let f = factors(d.handle, d.factor.clamp(0.2, 3.0));
        sl.axes = Mat3::from_cols(sl.axes.x_axis * f.x, sl.axes.y_axis * f.y, sl.axes.z_axis * f.z);
    }
    let sl = &sl;
    let uniform_active = hl == Some(Handle::Uniform);
    if uniform_active {
        if let Some(poly) = uniform_polygon(sl, p) {
            painter.add(egui::Shape::convex_polygon(poly, FILL_GREY.gamma_multiply(0.6), egui::Stroke::NONE));
        }
    }
    for plane in Plane::ALL {
        let active = hl == Some(Handle::Plane(plane));
        if active {
            if let Some(t) = plane_band(sl, p, plane) {
                painter.add(egui::Shape::convex_polygon(t.to_vec(), FILL_GREY.gamma_multiply(0.6), egui::Stroke::NONE));
            }
        }
        let (ua, va) = plane.axes();
        for at in [INNER, OUTER] {
            let (Some(a), Some(b)) = (corner(sl, p, ua, at), corner(sl, p, va, at)) else { continue };
            if active || uniform_active {
                painter.line_segment([a, b], egui::Stroke::new(1.5_f32, HOVER));
            } else {
                let m = egui::pos2((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
                painter.line_segment([a, m], egui::Stroke::new(1.5_f32, axis_color(ua)));
                painter.line_segment([m, b], egui::Stroke::new(1.5_f32, axis_color(va)));
            }
        }
    }
    for a in [Axis::X, Axis::Y, Axis::Z] {
        let Some((s, e)) = axis_segment(sl, p, a, 0.0, 1.0) else { continue };
        let involved = involves(hl, a);
        let color = if involved && !uniform_active { HOVER } else { axis_color(a) };
        let width = if hl == Some(Handle::Axis(a)) { 3.0_f32 } else { 2.0 };
        painter.line_segment([s, e], egui::Stroke::new(width, color));
        if uniform_active {
            if let Some((s, m)) = axis_segment(sl, p, a, 0.0, INNER) {
                painter.line_segment([s, m], egui::Stroke::new(width, HOVER));
            }
        }
        painter.rect_filled(egui::Rect::from_center_size(e, egui::vec2(END_BOX_PX, END_BOX_PX)), 0.0, axis_color(a));
        if l.axis_labels {
            let d = pos_sub(e, s).normalize_or_zero();
            let label = if involved { HOVER } else { axis_color(a) };
            painter.text(egui::pos2(e.x + d.x * 10.0, e.y + d.y * 10.0), egui::Align2::CENTER_CENTER, a.label(), egui::FontId::proportional(11.0), label);
        }
    }
    if let (Some(d), Some(o)) = (drag, p.screen(l.origin)) {
        painter.text(egui::pos2(o.x + 14.0, o.y + 14.0), egui::Align2::LEFT_TOP, format!("{:.1}%", d.factor * 100.0), egui::FontId::proportional(11.0), HOVER);
    }
}
