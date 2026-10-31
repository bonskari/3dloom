//! Rotate gizmo: X/Y/Z rings drawn as 3D circles with only their front
//! halves shown, the Free sphere's outline, a grey Screen ring around it and
//! a small grey tripod at the centre; while dragging, a translucent pie slice
//! in the ring's plane and an [x, y, z] degree readout.

use super::*;

const SCREEN_RING: f32 = 1.25;
const SEGMENTS: usize = 64;
/// Centre tripod length, as a fraction of the sphere radius.
const TRIPOD: f32 = 0.3;

const SCREEN_GREY: egui::Color32 = egui::Color32::from_gray(170);
const SPHERE_GREY: egui::Color32 = egui::Color32::from_gray(100);
const FILL_GREY: egui::Color32 = egui::Color32::from_gray(214);

/// Whether the ring point in direction `dir` from the centre is on the
/// half of the sphere facing the eye.
pub fn faces_viewer(l: &GizmoLayout, p: &Projection, dir: Vec3) -> bool {
    dir.dot(p.toward_viewer_at(l.origin)) >= -0.05
}

/// Points on the ring around `axis`, each with whether it faces the viewer.
pub fn ring_points(l: &GizmoLayout, p: &Projection, a: Axis) -> Vec<(egui::Pos2, bool)> {
    let len = l.world_len(p);
    let (u, v) = basis(l.axis(a));
    (0..=SEGMENTS)
        .filter_map(|i| {
            let t = i as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
            let dir = u * t.cos() + v * t.sin();
            p.screen(l.origin + dir * len).map(|s| (s, faces_viewer(l, p, dir)))
        })
        .collect()
}

/// A ring seen (nearly) edge-on projects to a line through the centre and
/// is not offered as a handle.
fn ring_usable(l: &GizmoLayout, p: &Projection, a: Axis) -> bool {
    l.axis(a).dot(p.toward_viewer_at(l.origin)).abs() > 0.1
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
    let best = [Axis::X, Axis::Y, Axis::Z]
        .into_iter()
        .filter(|a| ring_usable(l, p, *a))
        .map(|a| (a, ring_distance(&ring_points(l, p, a), pointer)))
        .min_by(|a, b| a.1.total_cmp(&b.1));
    if let Some((a, d)) = best {
        if d <= HIT_PX {
            return Some(Handle::Axis(a));
        }
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
    let sign = if l.axis(a).dot(p.toward_viewer_at(l.origin)) >= 0.0 { 1.0 } else { -1.0 };
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
    if free_active {
        painter.circle_filled(o, l.size_px, FILL_GREY.gamma_multiply(0.35));
    }
    if let Some(d) = drag {
        if l.pie_slice && d.angle_deg.abs() > 0.0 {
            pie_slice(painter, l, p, d);
        }
    }
    if l.free_rotation || free_active {
        painter.circle_stroke(o, l.size_px, egui::Stroke::new(if free_active { 2.0_f32 } else { 1.0 }, if free_active { hover() } else { SPHERE_GREY }));
    }
    for a in [Axis::X, Axis::Y, Axis::Z] {
        let active = hl == Some(Handle::Axis(a));
        let stroke = egui::Stroke::new(if active { 3.0_f32 } else { 2.0 }, if active { hover() } else { axis_color(a) });
        // Only the half facing the viewer is drawn; the back half is behind the sphere.
        let pts = ring_points(l, p, a);
        let mut run: Vec<egui::Pos2> = Vec::new();
        for w in pts.windows(2) {
            if w[0].1 && w[1].1 {
                if run.is_empty() {
                    run.push(w[0].0);
                }
                run.push(w[1].0);
            } else if !run.is_empty() {
                painter.add(egui::Shape::line(std::mem::take(&mut run), stroke));
            }
        }
        if !run.is_empty() {
            painter.add(egui::Shape::line(run, stroke));
        }
    }
    let screen_active = hl == Some(Handle::Screen);
    if l.screen_handle {
        painter.circle_stroke(o, l.size_px * SCREEN_RING, egui::Stroke::new(if screen_active { 2.0_f32 } else { 1.0 }, if screen_active { hover() } else { SCREEN_GREY }));
    }
    let tl = GizmoLayout { size_px: l.size_px * TRIPOD, ..*l };
    for a in [Axis::X, Axis::Y, Axis::Z] {
        if let Some((s, e)) = axis_segment(&tl, p, a, 0.0, 1.0) {
            painter.line_segment([s, e], egui::Stroke::new(1.0_f32, SCREEN_GREY));
            if l.axis_labels {
                let d = pos_sub(e, s).normalize_or_zero();
                painter.text(egui::pos2(e.x + d.x * 6.0, e.y + d.y * 6.0), egui::Align2::CENTER_CENTER, a.label().to_lowercase(), egui::FontId::proportional(10.0), SCREEN_GREY);
            }
        }
    }
    if let Some(d) = drag {
        if l.angle_data {
            let deg = match d.handle {
                Handle::Axis(_) => d.rotate_deg,
                // Screen / Free: the accumulated rotation in the reference system.
                _ => {
                    let frame = Quat::from_mat3(&l.axes);
                    let (x, y, z) = (frame.inverse() * d.quat * frame).to_euler(glam::EulerRot::XYZ);
                    Vec3::new(x, y, z) * (180.0 / std::f32::consts::PI)
                }
            };
            let top = egui::pos2(o.x, o.y - l.size_px * if l.screen_handle { SCREEN_RING } else { 1.0 } - 4.0);
            painter.text(top, egui::Align2::CENTER_BOTTOM, format!("[{:.2}, {:.2}, {:.2}]", deg.x, deg.y, deg.z), egui::FontId::proportional(11.0), hover());
        }
    }
}

/// The swept angle as a translucent slice in the plane of the dragged ring
/// (or the screen plane), starting where the drag began. Turns beyond 360°
/// overlap and the shading gets more opaque.
fn pie_slice(painter: &egui::Painter, l: &GizmoLayout, p: &Projection, d: &DragState) {
    let (n, radius, color) = match d.handle {
        Handle::Axis(a) => (l.axis(a), l.world_len(p), axis_color(a)),
        Handle::Screen => (p.toward_viewer_at(l.origin), l.world_len(p) * SCREEN_RING, SCREEN_GREY),
        _ => return,
    };
    let (u, v) = basis(n);
    let Some(start) = p.hit_plane(d.start, l.origin, n) else { return };
    let w = start - l.origin;
    let a0 = w.dot(v).atan2(w.dot(u));
    let sweep = d.angle_deg.to_radians();
    let Some(o) = p.screen(l.origin) else { return };
    let fill = color.gamma_multiply(0.25);
    let steps = ((sweep.abs() / 0.05).ceil() as usize).clamp(2, 2880);
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(o, fill);
    for i in 0..=steps {
        let t = a0 + sweep * i as f32 / steps as f32;
        let Some(q) = p.screen(l.origin + (u * t.cos() + v * t.sin()) * radius) else { return };
        mesh.colored_vertex(q, fill);
    }
    for i in 1..=steps as u32 {
        mesh.add_triangle(0, i, i + 1);
    }
    painter.add(egui::Shape::mesh(mesh));
}
