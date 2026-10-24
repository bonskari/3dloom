//! Rotate gizmo: X/Y/Z rings (back halves dimmed), a grey Screen ring
//! and the Free trackball inside, with a pie slice and degree readout while
//! dragging.

use super::*;

const SCREEN_RING: f32 = 1.25;
const SEGMENTS: usize = 64;

/// Points on the ring around `axis`, each with whether it faces the viewer.
pub fn ring_points(l: &GizmoLayout, p: &Projection, a: Axis) -> Vec<(egui::Pos2, bool)> {
    let len = l.world_len(p);
    let n = l.axis(a);
    let (u, v) = basis(n);
    let tv = p.toward_viewer();
    (0..=SEGMENTS)
        .filter_map(|i| {
            let t = i as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
            let dir = u * t.cos() + v * t.sin();
            let w = l.origin + dir * len;
            p.screen(w).map(|s| (s, dir.dot(tv) >= -0.05))
        })
        .collect()
}

fn basis(n: Vec3) -> (Vec3, Vec3) {
    let helper = if n.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
    let u = n.cross(helper).normalize_or_zero();
    let v = n.cross(u).normalize_or_zero();
    (u, v)
}

fn ring_distance(pts: &[(egui::Pos2, bool)], pointer: egui::Pos2) -> f32 {
    pts.windows(2)
        .filter(|w| w[0].1 && w[1].1)
        .map(|w| distance_to_segment(pointer, w[0].0, w[1].0))
        .fold(f32::INFINITY, f32::min)
}

pub fn hit(l: &GizmoLayout, p: &Projection, pointer: egui::Pos2) -> Option<Handle> {
    let o = p.screen(l.origin)?;
    // A ring seen edge-on projects to a line through the centre; it is
    // not offered as a handle, so skip it here.
    let tv = p.toward_viewer();
    let best = [Axis::X, Axis::Y, Axis::Z]
        .into_iter()
        .filter(|a| l.axis(*a).dot(tv).abs() > 0.1)
        .map(|a| (a, ring_distance(&ring_points(l, p, a), pointer)))
        .min_by(|a, b| a.1.total_cmp(&b.1))?;
    if best.1 <= HIT_PX {
        return Some(Handle::Axis(best.0));
    }
    let d = pointer.distance(o);
    if l.screen_handle && (d - l.size_px * SCREEN_RING).abs() <= HIT_PX {
        return Some(Handle::Screen);
    }
    (l.free_rotation && d < l.size_px).then_some(Handle::Free)
}

/// Signed angle (degrees, right-hand rule about the axis) the pointer swept
/// around the gizmo centre.
pub fn drag_angle_deg(l: &GizmoLayout, p: &Projection, a: Axis, last: egui::Pos2, now: egui::Pos2) -> f32 {
    let sign = if l.axis(a).dot(p.toward_viewer()) >= 0.0 { 1.0 } else { -1.0 };
    screen_sweep_deg(l, p, last, now) * sign
}

pub fn drag_angle_deg_handle(l: &GizmoLayout, p: &Projection, h: Handle, last: egui::Pos2, now: egui::Pos2) -> f32 {
    match h {
        Handle::Axis(a) => drag_angle_deg(l, p, a, last, now),
        _ => screen_sweep_deg(l, p, last, now),
    }
}

/// Counter-clockwise sweep on screen (positive = counter-clockwise as seen).
fn screen_sweep_deg(l: &GizmoLayout, p: &Projection, last: egui::Pos2, now: egui::Pos2) -> f32 {
    let Some(o) = p.screen(l.origin) else { return 0.0 };
    // Screen y grows downward, so flip it to get a right-handed angle.
    let a = pos_sub(last, o);
    let b = pos_sub(now, o);
    let (a, b) = (Vec2::new(a.x, -a.y), Vec2::new(b.x, -b.y));
    if a.length_squared() < 1.0 || b.length_squared() < 1.0 {
        return 0.0;
    }
    (a.perp_dot(b)).atan2(a.dot(b)).to_degrees()
}

/// Trackball rotation between two pointer positions (world quaternion).
pub fn trackball(l: &GizmoLayout, p: &Projection, last: egui::Pos2, now: egui::Pos2) -> Quat {
    let Some(o) = p.screen(l.origin) else { return Quat::IDENTITY };
    let sphere = |q: egui::Pos2| {
        let v = pos_sub(q, o) / l.size_px;
        let v = Vec2::new(v.x, -v.y);
        let r2 = v.length_squared();
        let z = if r2 < 0.5 { (1.0 - r2).sqrt() } else { 0.5 / r2.sqrt() };
        let right = p.camera_right();
        let tv = p.toward_viewer();
        let up = tv.cross(right);
        (right * v.x + up * v.y + tv * z).normalize_or_zero()
    };
    let (a, b) = (sphere(last), sphere(now));
    Quat::from_rotation_arc(a, b)
}

pub fn draw(painter: &egui::Painter, l: &GizmoLayout, p: &Projection, hl: Option<Handle>, drag: Option<&DragState>) {
    let Some(o) = p.screen(l.origin) else { return };
    let free_active = hl == Some(Handle::Free);
    painter.circle_filled(o, l.size_px, if free_active { HOVER.gamma_multiply(0.25) } else { egui::Color32::from_white_alpha(14) });
    for a in [Axis::X, Axis::Y, Axis::Z] {
        let active = hl == Some(Handle::Axis(a));
        let color = if active { HOVER } else { axis_color(a) };
        let pts = ring_points(l, p, a);
        for w in pts.windows(2) {
            let front = w[0].1 && w[1].1;
            let stroke = if front { egui::Stroke::new(if active { 3.0 } else { 2.0 }, color) } else { egui::Stroke::new(1.0_f32, color.gamma_multiply(0.3)) };
            painter.line_segment([w[0].0, w[1].0], stroke);
        }
    }
    let screen_active = hl == Some(Handle::Screen);
    if l.screen_handle {
        painter.circle_stroke(o, l.size_px * SCREEN_RING, egui::Stroke::new(if screen_active { 3.0 } else { 1.5 }, if screen_active { HOVER } else { egui::Color32::from_gray(170) }));
    }
    if let Some(d) = drag {
        if l.pie_slice && d.angle_deg.abs() > 0.0 && d.handle != Handle::Free {
            let start = pos_sub(d.start, o);
            let r = start.length().max(1.0);
            let a0 = (-start.y).atan2(start.x);
            let sweep = d.angle_deg.to_radians() * if let Handle::Axis(a) = d.handle { if l.axis(a).dot(p.toward_viewer()) >= 0.0 { 1.0 } else { -1.0 } } else { 1.0 };
            let n = ((sweep.abs() / 0.05).ceil() as usize).clamp(2, 720);
            let mut pts = vec![o];
            for i in 0..=n {
                let t = a0 + sweep * i as f32 / n as f32;
                pts.push(egui::pos2(o.x + r * t.cos(), o.y - r * t.sin()));
            }
            painter.add(egui::Shape::convex_polygon(pts.clone(), HOVER.gamma_multiply(0.35), egui::Stroke::NONE));
            painter.add(egui::Shape::line(pts, egui::Stroke::new(1.0_f32, HOVER)));
        }
        if l.angle_data {
            painter.text(egui::pos2(o.x + 14.0, o.y + 14.0), egui::Align2::LEFT_TOP, format!("{:.1}°", d.angle_deg), egui::FontId::proportional(11.0), HOVER);
        }
    }
}
