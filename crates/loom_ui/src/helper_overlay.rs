//! Helper line art in the viewports: Point tripods / crosses / boxes, Dummy
//! markers of every Display Type but Box, the
//! Tape line, the Protractor's lines, grid objects, and the compass rose's
//! N / E / S / W. Lines are depth-tested on the GPU like edges; a Point or Dummy
//! with Draw On Top (and every helper in a Realistic view) draws over everything.

use crate::LoomApp;
use eframe::egui;
use glam::{Mat4, Vec3};
use loom_commands::helpers::{helper_labels, helper_lines, units_per_pixel, LineKind};
use loom_scene::ObjectKind;

/// The Target Line colour (Protractor's line to object 1).
const TARGET_LINE: [u8; 3] = [80, 140, 255];

fn linear(c: [u8; 3]) -> [f32; 3] {
    let f = |x: u8| {
        let x = x as f32 / 255.0;
        if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
    };
    [f(c[0]), f(c[1]), f(c[2])]
}

/// A helper's own line colour: a Dummy's Use Color, else the wire colour.
fn own_color(n: &loom_scene::Node) -> [u8; 3] {
    match &n.object {
        ObjectKind::Dummy { use_color, color, .. } if use_color.get() => {
            let c = color.get();
            [(c >> 16) as u8, (c >> 8) as u8, c as u8]
        }
        _ => n.wire_color,
    }
}

/// World units per screen pixel at a point in a view (Constant Screen Size).
pub fn units_per_px(view_proj: Mat4, rect: egui::Rect) -> impl Fn(Vec3) -> f32 {
    move |p: Vec3| {
        // A world direction parallel to the screen: the inverse view's up.
        let inv = view_proj.inverse();
        let c = view_proj * p.extend(1.0);
        let q = inv * glam::Vec4::new(c.x, c.y + 0.01 * c.w, c.z, c.w);
        let q = q.truncate() / q.w;
        units_per_pixel(view_proj, rect.height(), p, (q - p).normalize_or_zero())
    }
}

/// The sRGB colour of a helper line.
fn color(app: &LoomApp, n: &loom_scene::Node, kind: LineKind) -> [u8; 3] {
    let selected = app.state.selection.contains(&n.id);
    let wire = if selected { [255, 255, 255] } else { own_color(n) };
    match kind {
        LineKind::Wire => wire,
        LineKind::Target => TARGET_LINE,
        LineKind::Rgb(c) => c,
        LineKind::Axis(0) => [220, 40, 40],
        LineKind::Axis(1) => [40, 200, 40],
        LineKind::Axis(_) => [40, 80, 230],
        LineKind::GridMajor | LineKind::GridMinor => {
            let major = kind == LineKind::GridMajor;
            let mode = match &n.object {
                ObjectKind::Grid { color, .. } => color.get(),
                _ => 0,
            };
            match mode {
                // Object Color: main lines in the object colour, the others lighter.
                1 if major => n.wire_color,
                1 => n.wire_color.map(|c| c + ((255 - c) / 2)),
                // Home Color / Home Intensity: the home grid's lines.
                2 | 3 if major => [0, 0, 0],
                2 | 3 => [166, 166, 166],
                // Gray: two shades of gray (light lines, dark main axes, like the home grid).
                _ if major => [30, 30, 30],
                _ => [175, 175, 175],
            }
        }
    }
}

/// World segments with a linear (GPU) colour.
pub type DepthLines = Vec<(Vec3, Vec3, [f32; 3])>;
/// World segments with an sRGB (painter) colour.
pub type TopLines = Vec<(Vec3, Vec3, [u8; 3])>;

/// Every visible helper's lines for one viewport: (depth-tested lines in
/// linear colour for the GPU, on-top lines in sRGB for the painter).
pub fn lines(app: &LoomApp, view_proj: Mat4, rect: egui::Rect, realistic: bool) -> (DepthLines, TopLines) {
    let mut depth = Vec::new();
    let mut top = Vec::new();
    let upp = units_per_px(view_proj, rect);
    for n in app.state.scene.nodes() {
        if !n.object.is_helper() || !app.state.is_visible(n) {
            continue;
        }
        let on_top = realistic || matches!(&n.object, ObjectKind::Point { draw_on_top, .. } | ObjectKind::Dummy { draw_on_top, .. } if draw_on_top.get());
        for (a, b, k) in helper_lines(&app.state, n.id, &upp) {
            let c = color(app, n, k);
            if on_top {
                top.push((a, b, c));
            } else {
                depth.push((a, b, linear(c)));
            }
        }
    }
    (depth, top)
}

/// Draws the on-top lines and the helpers' text labels.
pub fn draw(painter: &egui::Painter, app: &LoomApp, view_proj: Mat4, rect: egui::Rect, top: &[(Vec3, Vec3, [u8; 3])]) {
    for (a, b, c) in top {
        if let Some(s) = crate::camera_overlay::segment(view_proj, rect, *a, *b) {
            painter.line_segment(s, egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(c[0], c[1], c[2])));
        }
    }
    for n in app.state.scene.nodes() {
        if !n.object.is_helper() || !app.state.is_visible(n) {
            continue;
        }
        let selected = app.state.selection.contains(&n.id);
        let c = if selected { [255, 255, 255] } else { own_color(n) };
        let c = egui::Color32::from_rgb(c[0], c[1], c[2]);
        for (p, text) in helper_labels(&app.state, n.id) {
            if let Some(s) = crate::viewport_widget::project_world_to_screen(p, view_proj, rect) {
                painter.text(s, egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(12.0), c);
            }
        }
    }
}
