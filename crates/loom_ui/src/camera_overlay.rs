//! Camera display in the viewports: the field-of-view cone, Environment
//! Ranges and clipping rectangles, the line to the target, and the horizon
//! line in the camera's own view.

use crate::LoomApp;
use eframe::egui;
use glam::{Mat4, Vec3, Vec4};
use loom_commands::camera::{overlay_lines, LineKind};
use loom_commands::ViewKind;
use loom_scene::ObjectKind;

/// The cone's light blue.
const CONE: egui::Color32 = egui::Color32::from_rgb(120, 200, 255);
/// Environment Ranges rectangles.
const RANGE: egui::Color32 = egui::Color32::from_rgb(230, 220, 60);
/// Clipping plane rectangles.
const CLIP: egui::Color32 = egui::Color32::from_rgb(230, 40, 40);
/// The horizon line.
const HORIZON: egui::Color32 = egui::Color32::from_gray(45);

/// A world segment clipped to the part in front of the eye, in screen space.
fn segment(view_proj: Mat4, rect: egui::Rect, a: Vec3, b: Vec3) -> Option<[egui::Pos2; 2]> {
    let mut ca = view_proj * a.extend(1.0);
    let mut cb = view_proj * b.extend(1.0);
    const W: f32 = 1e-4;
    if ca.w < W && cb.w < W {
        return None;
    }
    let cut = |p: Vec4, q: Vec4| p + (q - p) * ((W - p.w) / (q.w - p.w));
    if ca.w < W {
        ca = cut(ca, cb);
    } else if cb.w < W {
        cb = cut(cb, ca);
    }
    let screen = |c: Vec4| egui::pos2(rect.min.x + (c.x / c.w + 1.0) * 0.5 * rect.width(), rect.min.y + (1.0 - c.y / c.w) * 0.5 * rect.height());
    Some([screen(ca), screen(cb)])
}

pub fn draw(painter: &egui::Painter, app: &LoomApp, index: usize, view_proj: Mat4, rect: egui::Rect) {
    let kind = app.state.viewports[index].kind;
    for n in app.state.scene.nodes() {
        if !matches!(n.object, ObjectKind::Camera { .. }) || !app.state.is_visible(n) {
            continue;
        }
        if kind == ViewKind::Camera(n.id) {
            horizon(painter, app, n, view_proj, rect);
            continue;
        }
        let selected = app.state.selection.contains(&n.id);
        let wire = if selected { egui::Color32::WHITE } else { egui::Color32::from_rgb(n.wire_color[0], n.wire_color[1], n.wire_color[2]) };
        for (a, b, k) in overlay_lines(&app.state.scene, n.id, selected) {
            let color = match k {
                LineKind::Cone => CONE,
                LineKind::Range => RANGE,
                LineKind::Clip => CLIP,
                LineKind::TargetLine => wire,
            };
            if let Some(s) = segment(view_proj, rect, a, b) {
                painter.line_segment(s, egui::Stroke::new(1.0_f32, color));
            }
        }
    }
}

/// Show Horizon: a dark gray line across the camera view where the ground
/// plane's horizon is (points at infinity in horizontal directions).
fn horizon(painter: &egui::Painter, app: &LoomApp, n: &loom_scene::Node, view_proj: Mat4, rect: egui::Rect) {
    let ObjectKind::Camera { show_horizon, ortho, .. } = &n.object else { return };
    if !show_horizon.get() || ortho.get() {
        return;
    }
    let world = app.state.scene.world_matrix(n.id);
    let forward = -world.z_axis.truncate();
    let flat = Vec3::new(forward.x, forward.y, 0.0).normalize_or_zero();
    if flat == Vec3::ZERO {
        return;
    }
    let side = flat.cross(Vec3::Z);
    let project = |d: Vec3| {
        let c = view_proj * d.extend(0.0);
        (c.w > 1e-6).then(|| egui::pos2(rect.min.x + (c.x / c.w + 1.0) * 0.5 * rect.width(), rect.min.y + (1.0 - c.y / c.w) * 0.5 * rect.height()))
    };
    let (Some(p), Some(q)) = (project(flat + side * 0.1), project(flat - side * 0.1)) else { return };
    let d = q - p;
    if d.x.abs() < 1e-6 {
        return;
    }
    let y_at = |x: f32| p.y + d.y * (x - p.x) / d.x;
    let (a, b) = (egui::pos2(rect.left(), y_at(rect.left())), egui::pos2(rect.right(), y_at(rect.right())));
    painter.with_clip_rect(rect).line_segment([a, b], egui::Stroke::new(1.0_f32, HORIZON));
}
