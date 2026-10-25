//! Move gizmo: three arrows, three plane squares between them and a
//! centre box for screen-space moves.

use super::*;

const CENTER_PX: f32 = 5.0;

/// Plane handle span along each axis, from Preferences > Gizmos > Move.
fn plane_span(l: &GizmoLayout) -> (f32, f32) {
    let size = l.plane_size / 100.0;
    let offset = l.plane_offset / 100.0;
    let from = (offset - size * 0.5).clamp(0.05, 0.95);
    (from, (from + size * 0.6).min(1.0))
}

/// Screen direction (unit) and pixels per world unit of an axis, or None
/// when the axis is seen end-on.
pub fn axis_screen_dir(l: &GizmoLayout, p: &Projection, a: Axis) -> Option<(Vec2, f32)> {
    let o = p.screen(l.origin)?;
    let e = p.screen(l.origin + l.axis(a))?;
    let d = pos_sub(e, o);
    let len = d.length();
    (len > 1e-3).then(|| (d / len, len))
}

/// Arrow cone, as fractions of the axis length: tip position, length, base radius.
const CONE_TIP: f32 = 1.12;
const CONE_LENGTH: f32 = 0.26;
const CONE_RADIUS: f32 = 0.08;

pub fn plane_quad(l: &GizmoLayout, p: &Projection, plane: Plane) -> Option<[egui::Pos2; 4]> {
    let len = l.world_len(p);
    let (ua, va) = plane.axes();
    let (u, v) = (l.axis(ua) * len, l.axis(va) * len);
    let o = l.origin;
    let (from, to) = plane_span(l);
    let pts = [
        p.screen(o + u * from + v * from)?,
        p.screen(o + u * to + v * from)?,
        p.screen(o + u * to + v * to)?,
        p.screen(o + u * from + v * to)?,
    ];
    // A plane seen edge-on collapses to a line: not a usable handle.
    let area = (0..4).map(|i| {
        let (a, b) = (pts[i], pts[(i + 1) % 4]);
        a.x * b.y - b.x * a.y
    }).sum::<f32>().abs() * 0.5;
    (area > 4.0).then_some(pts)
}

pub fn hit(l: &GizmoLayout, p: &Projection, pointer: egui::Pos2) -> Option<Handle> {
    let o = p.screen(l.origin)?;
    for plane in Plane::ALL {
        if let Some(q) = plane_quad(l, p, plane) {
            if point_in_polygon(pointer, &q) {
                return Some(Handle::Plane(plane));
            }
        }
    }
    if l.center_box && (pointer.x - o.x).abs() <= CENTER_PX && (pointer.y - o.y).abs() <= CENTER_PX {
        return Some(Handle::CenterBox);
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

/// World delta (in gizmo axes) for a pointer move on a handle.
pub fn drag_delta(l: &GizmoLayout, p: &Projection, handle: Handle, last: egui::Pos2, now: egui::Pos2) -> Option<Vec3> {
    match handle {
        Handle::Axis(a) => {
            let (dir, px_per_unit) = axis_screen_dir(l, p, a)?;
            let amount = pos_sub(now, last).dot(dir) / px_per_unit;
            Some(unit(a) * amount)
        }
        Handle::Plane(plane) => {
            let n = l.axis(plane.normal());
            let a = p.hit_plane(last, l.origin, n)?;
            let b = p.hit_plane(now, l.origin, n)?;
            Some(l.axes.transpose() * (b - a))
        }
        Handle::CenterBox => {
            let n = p.toward_viewer();
            let a = p.hit_plane(last, l.origin, n)?;
            let b = p.hit_plane(now, l.origin, n)?;
            Some(l.axes.transpose() * (b - a))
        }
        _ => None,
    }
}

pub fn unit(a: Axis) -> Vec3 {
    match a {
        Axis::X => Vec3::X,
        Axis::Y => Vec3::Y,
        Axis::Z => Vec3::Z,
    }
}

pub fn draw(painter: &egui::Painter, l: &GizmoLayout, p: &Projection, hl: Option<Handle>, drag: Option<&DragState>) {
    for plane in Plane::ALL {
        if let Some(q) = plane_quad(l, p, plane) {
            let active = hl == Some(Handle::Plane(plane));
            let (ua, va) = plane.axes();
            let fill = if active { hover().gamma_multiply(0.45) } else { egui::Color32::TRANSPARENT };
            painter.add(egui::Shape::convex_polygon(q.to_vec(), fill, egui::Stroke::NONE));
            let c = |a: Axis| if active { hover() } else { axis_color(a) };
            // The plane handle is drawn as an open corner bracket (the two outer edges).
            painter.line_segment([q[1], q[2]], egui::Stroke::new(1.5, c(va)));
            painter.line_segment([q[3], q[2]], egui::Stroke::new(1.5, c(ua)));
        }
    }
    for a in [Axis::X, Axis::Y, Axis::Z] {
        if let Some((s, e)) = axis_segment(l, p, a, 0.0, 1.0) {
            let active = hl == Some(Handle::Axis(a));
            let color = if active { hover() } else { axis_color(a) };
            painter.line_segment([s, e], egui::Stroke::new(if active { 3.0 } else { 2.0 }, color));
            arrow_cone(painter, l, p, a, color);
            if l.axis_labels {
                let d = pos_sub(e, s).normalize_or_zero();
                painter.text(egui::pos2(e.x + d.x * 10.0, e.y + d.y * 10.0), egui::Align2::CENTER_CENTER, a.label(), egui::FontId::proportional(11.0), color);
            }
        }
    }
    if let Some(o) = p.screen(l.origin) {
        let active = hl == Some(Handle::CenterBox);
        let r = egui::Rect::from_center_size(o, egui::vec2(CENTER_PX * 2.0, CENTER_PX * 2.0));
        if l.center_box {
            painter.rect_filled(r, 0.0, if active { hover() } else { egui::Color32::from_gray(200) });
            painter.rect_stroke(r, 0.0, egui::Stroke::new(1.0_f32, egui::Color32::BLACK), egui::StrokeKind::Middle);
        }
        if let Some(d) = drag {
            let off = d.offset;
            painter.text(
                egui::pos2(o.x + 14.0, o.y + 14.0),
                egui::Align2::LEFT_TOP,
                format!("[{:.2}, {:.2}, {:.2}]", off.x, off.y, off.z),
                egui::FontId::proportional(11.0),
                hover(),
            );
        }
    }
}

/// The arrowhead: a solid cone at the axis tip, seen in perspective, lit
/// from its left side so it reads as 3D.
pub fn arrow_cone(painter: &egui::Painter, l: &GizmoLayout, p: &Projection, a: Axis, color: egui::Color32) {
    let len = l.world_len(p);
    let dir = l.axis(a);
    let tip = l.origin + dir * len * CONE_TIP;
    let base = l.origin + dir * len * (CONE_TIP - CONE_LENGTH);
    let side = if dir.cross(Vec3::Z).length_squared() > 1e-4 { dir.cross(Vec3::Z) } else { dir.cross(Vec3::X) }.normalize();
    let up = dir.cross(side);
    let r = len * CONE_RADIUS;
    let Some(tip_px) = p.screen(tip) else { return };
    let rim: Vec<egui::Pos2> = (0..16)
        .filter_map(|i| {
            let t = i as f32 / 16.0 * std::f32::consts::TAU;
            p.screen(base + (side * t.cos() + up * t.sin()) * r)
        })
        .collect();
    if rim.len() < 3 {
        return;
    }
    let mut pts = rim.clone();
    pts.push(tip_px);
    let hull = convex_hull(pts);
    painter.add(egui::Shape::convex_polygon(hull, color, egui::Stroke::NONE));
    // Highlight: the half of the silhouette toward the screen's upper left.
    let lit: Vec<egui::Pos2> = rim.iter().copied().filter(|q| (q.x - tip_px.x) + (q.y - tip_px.y) < 0.0).collect();
    if lit.len() >= 2 {
        let mut pts = lit;
        pts.push(tip_px);
        painter.add(egui::Shape::convex_polygon(convex_hull(pts), egui::Color32::from_white_alpha(35), egui::Stroke::NONE));
    }
}

/// Convex hull (monotone chain), counter-clockwise.
fn convex_hull(mut pts: Vec<egui::Pos2>) -> Vec<egui::Pos2> {
    pts.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    let cross = |o: egui::Pos2, a: egui::Pos2, b: egui::Pos2| (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x);
    let mut hull: Vec<egui::Pos2> = Vec::new();
    for pass in 0..2 {
        let start = hull.len();
        let iter: Box<dyn Iterator<Item = &egui::Pos2>> = if pass == 0 { Box::new(pts.iter()) } else { Box::new(pts.iter().rev()) };
        for &q in iter {
            while hull.len() >= start + 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], q) <= 0.0 {
                hull.pop();
            }
            hull.push(q);
        }
        hull.pop();
    }
    hull
}
