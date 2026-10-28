//! Helper line art in the viewports: Point tripods / crosses / boxes, the
//! Tape line, the Protractor's lines, grid objects, and the compass rose's
//! N / E / S / W. Lines are depth-tested on the GPU like edges; a Point with
//! Draw On Top (and every helper in a Realistic view) draws over everything.

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

/// The sRGB colour of a helper line.
fn color(app: &LoomApp, n: &loom_scene::Node, kind: LineKind) -> [u8; 3] {
    let selected = app.state.selection.contains(&n.id);
    let wire = if selected { [255, 255, 255] } else { n.wire_color };
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

/// The evaluated bone axis used by both its on-top display and selection.
/// Source width and skin bindings are not changed by this display proxy.
pub(crate) fn bone_axis(app: &LoomApp, node: &loom_scene::Node) -> Option<(Vec3, Vec3)> {
    if !bone_axis_visible(app, node) { return None; }
    app.bone_axes.iter().find_map(|(id, a, b)| (*id == node.id).then_some((*a, *b)))
}

fn bone_axis_visible(app: &LoomApp, node: &loom_scene::Node) -> bool {
    app.state.is_visible(node)
        && !(node.parent.is_some() && node.display.display_links && node.display.link_replaces_object)
}

/// Every visible helper's lines for one viewport: (depth-tested lines in
/// linear colour for the GPU, on-top lines in sRGB for the painter).
pub fn lines(app: &LoomApp, view_proj: Mat4, rect: egui::Rect, realistic: bool) -> (DepthLines, TopLines) {
    let mut depth = Vec::new();
    let mut top = Vec::new();
    let up = |p: Vec3| {
        // A world direction parallel to the screen: the inverse view's up.
        let inv = view_proj.inverse();
        let c = view_proj * p.extend(1.0);
        let q = inv * glam::Vec4::new(c.x, c.y + 0.01 * c.w, c.z, c.w);
        let q = q.truncate() / q.w;
        (q - p).normalize_or_zero()
    };
    let upp = |p: Vec3| units_per_pixel(view_proj, rect.height(), p, up(p));
    for (id, a, b) in &app.bone_axes {
        if let Some(n) = app.state.scene.node(*id).filter(|n| bone_axis_visible(app, n)) {
            top.push((*a, *b, color(app, n, LineKind::Wire)));
        }
    }
    for n in app.state.scene.nodes() {
        if !n.object.is_helper() || !app.state.is_visible(n) {
            continue;
        }
        let on_top = realistic || matches!(&n.object, ObjectKind::Point { draw_on_top, .. } if draw_on_top.get());
        for (a, b, k) in helper_lines(&app.state, n.id, &upp) {
            let c = color(app, n, k);
            if on_top {
                top.push((a, b, c));
            } else {
                depth.push((a, b, linear(c)));
            }
        }
    }
    for proxy in &app.rig_controls {
        let Some(node) = crate::loomrig_overlay::visible_node(app, proxy) else { continue };
        let c = color(app, node, LineKind::Wire);
        top.extend(proxy.segments().map(|(a, b)| (a, b, c)));
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
        let c = if selected { egui::Color32::WHITE } else { egui::Color32::from_rgb(n.wire_color[0], n.wire_color[1], n.wire_color[2]) };
        for (p, text) in helper_labels(&app.state, n.id) {
            if let Some(s) = crate::viewport_widget::project_world_to_screen(p, view_proj, rect) {
                painter.text(s, egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(12.0), c);
            }
        }
    }
}

#[cfg(test)]
mod bone_tests {
    use super::*;
    use loom_scene::Node;

    #[test]
    fn prepared_bone_axes_follow_pose_changes_and_scene_replacement() {
        let mut app = LoomApp::headless();
        let mut bone = Node::new("animated bone", ObjectKind::bone(0.5));
        bone.object_offset.position.set(Vec3::new(10., 0., 0.));
        loom_scene::anim::with(10., true, || bone.transform.position.set(Vec3::new(0.5, 0., 0.)));
        let id = app.state.scene.add_node(bone);
        let mut axes = Vec::new();
        for frame in [0, 5, 10, 3] {
            app.state.mesh_dirty = true;
            app.dispatch(loom_commands::Command::SetFrame { frame }).unwrap();
            let node = app.state.scene.node(id).unwrap();
            let actual = bone_axis(&app, node).unwrap();
            let world = app.state.scene.world_matrix(id);
            assert_eq!(actual, (world.transform_point3(Vec3::ZERO), world.transform_point3(node.object.bone_tip_local().unwrap())));
            axes.push(actual);
        }
        assert_ne!(axes[0], axes[1]);
        app.state.scene = loom_scene::Scene::new();
        app.state.mesh_dirty = true;
        app.sync_after_command();
        assert!(app.bone_axes.is_empty(), "a new scene cannot retain the previous skeleton axes");
    }

    #[test]
    fn viewport_bone_axes_do_not_reevaluate_the_animated_pose() {
        let mut app = LoomApp::headless();
        let mut bone = Node::new("animated bone", ObjectKind::bone(0.5));
        loom_scene::anim::with(10., true, || bone.transform.position.set(Vec3::new(0.5, 0., 0.)));
        app.state.scene.add_node(bone);
        app.state.scene.set_time(5.);
        app.state.mesh_dirty = true;
        app.sync_after_command();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400., 400.));
        let reads = loom_scene::anim::animated_reads();
        for _ in 0..4 { lines(&app, Mat4::IDENTITY, rect, true); }
        assert_eq!(loom_scene::anim::animated_reads(), reads, "viewport axes must reuse the already prepared pose");
    }

    #[test]
    fn link_replacing_bone_has_no_unpickable_axis_proxy() {
        let mut app = LoomApp::headless();
        let parent = app.state.scene.add_node(Node::new("parent", ObjectKind::bone(0.5)));
        let mut bone = Node::new("link only", ObjectKind::bone(0.5));
        bone.parent = Some(parent);
        bone.display.display_links = true;
        bone.display.link_replaces_object = true;
        let id = app.state.scene.add_node(bone);
        assert!(bone_axis(&app, app.state.scene.node(id).unwrap()).is_none());
    }

    #[test]
    fn visible_bones_have_on_top_axes_even_with_geometry_hidden() {
        let mut app = LoomApp::headless();
        let mut bone = ObjectKind::bone(0.5);
        if let ObjectKind::Bone { width, height, .. } = &mut bone { width.set(0.01); height.set(0.01); }
        let id = app.state.scene.add_node(Node::new("thin imported bone", bone));
        app.state.hide_category.geometry = true;
        app.state.mesh_dirty = true;
        app.sync_after_command();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400., 400.));
        let (depth, top) = lines(&app, Mat4::IDENTITY, rect, true);
        assert!(depth.is_empty());
        let world = app.state.scene.world_matrix(id);
        let tip = app.state.scene.node(id).unwrap().object.bone_tip_local().unwrap();
        assert!(top.iter().any(|(a, b, _)| *a == world.transform_point3(Vec3::ZERO) && *b == world.transform_point3(tip)), "visible bones need an on-top axis independent of source width");
        app.state.display.hide_bones = true;
        assert!(lines(&app, Mat4::IDENTITY, rect, true).1.is_empty());
    }
}
