//! Lights in the viewports: the shaded viewport's lighting (scene lights,
//! or default lights when the scene has none that is on), and the light
//! display lines: hotspot / falloff cones, attenuation ranges, the line to
//! the target, the icon's edges over a Realistic view, and the hotspot and
//! falloff circles in a light's own view.

use crate::camera_overlay::segment;
use crate::LoomApp;
use eframe::egui;
use glam::{Mat4, Vec3};
use loom_commands::light::{overlay_lines, LineKind};
use loom_commands::ViewKind;
use loom_scene::light::{Emitter, LightSample, LightType};
use loom_scene::ObjectKind;

/// Hotspot outline: light blue.
const HOTSPOT: egui::Color32 = egui::Color32::from_rgb(120, 200, 255);
/// Falloff outline: dark gray.
const FALLOFF: egui::Color32 = egui::Color32::from_gray(70);
const NEAR_START: egui::Color32 = egui::Color32::from_rgb(20, 40, 140);
const NEAR_END: egui::Color32 = egui::Color32::from_rgb(120, 160, 255);
const FAR_START: egui::Color32 = egui::Color32::from_rgb(200, 160, 100);
const FAR_END: egui::Color32 = egui::Color32::from_rgb(110, 70, 30);
/// Ambient light of a viewport lit by the two default lights.
const DEFAULT_AMBIENT: f32 = 0.2;

/// The shaded viewport's mapping of a light sample.
fn view_light(s: &LightSample) -> Option<loom_render::ViewLight> {
    let kind = match s.emitter {
        Emitter::Point => loom_render::ViewLightKind::Point,
        Emitter::Spot { hotspot, falloff } => loom_render::ViewLightKind::Spot { hotspot, falloff },
        Emitter::Directional => loom_render::ViewLightKind::Directional,
        Emitter::Ambient => return None,
    };
    Some(loom_render::ViewLight { kind, position: s.position, direction: s.direction, color: s.color, decay: s.decay, decay_start: s.decay_start, near: s.near, far: s.far })
}

/// Lighting of a shaded viewport: its scene lights while it illuminates
/// with scene lights and some light is on, else the default lights (one
/// from the viewer keeps the headlight shading).
pub fn view_lighting(app: &LoomApp, vp: &loom_commands::Viewport, view: Mat4, shading: loom_render::ViewShading) -> loom_render::ViewShading {
    let samples = if vp.scene_lights { loom_scene::light::scene_lights(&app.state.scene, |_| true) } else { Vec::new() };
    if !samples.is_empty() {
        let lights: Vec<loom_render::ViewLight> = samples.iter().filter_map(view_light).collect();
        let ambient: Vec3 = samples.iter().filter(|s| s.emitter == Emitter::Ambient).map(|s| s.color).sum();
        return shading.with_lights(&lights, ambient.to_array());
    }
    if vp.default_lights >= 2 {
        let lights: Vec<loom_render::ViewLight> = loom_scene::light::default_lights(2, view.inverse()).iter().filter_map(view_light).collect();
        return shading.with_lights(&lights, [DEFAULT_AMBIENT; 3]);
    }
    shading
}

pub fn draw(painter: &egui::Painter, app: &LoomApp, index: usize, view_proj: Mat4, rect: egui::Rect, realistic: bool) {
    let kind = app.state.viewports[index].kind;
    let scene = &app.state.scene;
    for n in scene.nodes() {
        let ObjectKind::Light(l) = &n.object else { continue };
        if !app.state.is_visible(n) {
            continue;
        }
        if kind == ViewKind::Camera(n.id) {
            light_view_circles(painter, l, rect);
            continue;
        }
        let selected = app.state.selection.contains(&n.id);
        let wire = if selected { egui::Color32::WHITE } else { let c = app.state.scene.display_color(n); egui::Color32::from_rgb(c[0], c[1], c[2]) };
        let stroke = |c| egui::Stroke::new(1.0_f32, c);
        for (a, b, k) in overlay_lines(scene, n.id, selected) {
            let color = match k {
                LineKind::Hotspot => HOTSPOT,
                LineKind::Falloff => FALLOFF,
                LineKind::NearStart => NEAR_START,
                LineKind::NearEnd => NEAR_END,
                LineKind::FarStart => FAR_START,
                LineKind::FarEnd => FAR_END,
                LineKind::TargetLine => wire,
            };
            if let Some(s) = segment(view_proj, rect, a, b) {
                painter.line_segment(s, stroke(color));
            }
        }
        // Over Bevy's image the icon is not drawn by the shaded pass: draw its edges.
        if realistic {
            let m = l.icon_mesh();
            let world = scene.object_matrix(n.id);
            for (i, _) in m.half_edges().iter().enumerate() {
                let e = loom_mesh::HalfEdgeId(i as u32);
                if m.canonical_edge_id(e) != Some(e) {
                    continue;
                }
                let Some((a, b)) = m.edge_vertices(e).and_then(|(a, b)| Some((m.vertex_position(a)?, m.vertex_position(b)?))) else { continue };
                if let Some(s) = segment(view_proj, rect, world.transform_point3(a), world.transform_point3(b)) {
                    painter.line_segment(s, stroke(wire));
                }
            }
        }
    }
}

/// A light's own view: the hotspot (light blue) and falloff (dark gray)
/// circles or rectangles; the falloff spans the view's width.
fn light_view_circles(painter: &egui::Painter, l: &loom_scene::light::LightObject, rect: egui::Rect) {
    if !l.light_type.aims() {
        return;
    }
    let (hot, fall) = (l.hotspot.get(), l.falloff.get().max(1e-3));
    let ratio = if l.light_type == LightType::Spot { (hot * 0.5).to_radians().tan() / (fall.min(179.0) * 0.5).to_radians().tan() } else { hot / fall };
    let r = rect.width() * 0.5;
    let painter = painter.with_clip_rect(rect);
    for (radius, color) in [(r, FALLOFF), (r * ratio, HOTSPOT)] {
        let stroke = egui::Stroke::new(1.5_f32, color);
        if l.rectangle.get() {
            let h = radius / l.aspect.get().max(1e-3);
            painter.rect_stroke(egui::Rect::from_center_size(rect.center(), egui::vec2(radius * 2.0, h * 2.0)), 0.0, stroke, egui::StrokeKind::Middle);
        } else {
            painter.circle_stroke(rect.center(), radius, stroke);
        }
    }
}
