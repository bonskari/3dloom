use eframe::egui;
use glam::Vec3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum GizmoView {
    Front,
    Back,
    Left,
    Right,
    Top,
    Bottom,
}

pub fn rect(viewport: egui::Rect) -> egui::Rect {
    let size = egui::vec2(112.0, 112.0);
    egui::Rect::from_min_size(
        viewport.right_top() + egui::vec2(-size.x - 12.0, 12.0),
        size,
    )
}

pub fn hit_test(
    pos: egui::Pos2,
    gizmo_rect: egui::Rect,
    yaw: f32,
    pitch: f32,
    perspective: bool,
) -> Option<GizmoView> {
    let faces = projected_faces(gizmo_rect, yaw, pitch, perspective);
    for face in faces.iter().rev() {
        if point_in_polygon(pos, &face.points) {
            return Some(face.view);
        }
    }
    None
}

pub fn target_angles(view: GizmoView) -> (f32, f32) {
    use loom_commands::ViewKind;
    let kind = match view {
        GizmoView::Front => ViewKind::Front,
        GizmoView::Back => ViewKind::Back,
        GizmoView::Left => ViewKind::Left,
        GizmoView::Right => ViewKind::Right,
        GizmoView::Top => ViewKind::Top,
        GizmoView::Bottom => ViewKind::Bottom,
    };
    kind.angles().expect("axis views have fixed angles")
}

pub fn view_kind(view: GizmoView) -> loom_commands::ViewKind {
    use loom_commands::ViewKind;
    match view {
        GizmoView::Front => ViewKind::Front,
        GizmoView::Back => ViewKind::Back,
        GizmoView::Left => ViewKind::Left,
        GizmoView::Right => ViewKind::Right,
        GizmoView::Top => ViewKind::Top,
        GizmoView::Bottom => ViewKind::Bottom,
    }
}

pub fn angled_perspective_angles() -> (f32, f32) {
    loom_commands::viewport::HOME_PERSPECTIVE
}

/// The ViewCube shows only in the active viewport, and never through a camera.
pub fn shows_in(active: bool, kind: loom_commands::ViewKind) -> bool {
    active && !matches!(kind, loom_commands::ViewKind::Camera(_))
}

pub fn angles_match(
    current_yaw: f32,
    current_pitch: f32,
    target_yaw: f32,
    target_pitch: f32,
) -> bool {
    let mut dyaw = target_yaw - current_yaw;
    while dyaw > std::f32::consts::PI {
        dyaw -= std::f32::consts::TAU;
    }
    while dyaw < -std::f32::consts::PI {
        dyaw += std::f32::consts::TAU;
    }
    dyaw.abs() < 0.03 && (target_pitch - current_pitch).abs() < 0.03
}

/// The Home icon above the cube's top-left corner (shown while the cube is hovered).
pub fn home_rect(gizmo_rect: egui::Rect) -> egui::Rect {
    egui::Rect::from_min_size(gizmo_rect.left_top() + egui::vec2(4.0, 2.0), egui::vec2(16.0, 14.0))
}

/// The ViewCube: a light grey lit cube over a compass ring, no backdrop.
/// Inactive it is see-through; under the pointer it is opaque and shows Home.
pub fn draw(painter: &egui::Painter, gizmo_rect: egui::Rect, yaw: f32, pitch: f32, perspective: bool, hovered: bool) {
    let alpha = if hovered { 1.0 } else { INACTIVE_OPACITY };
    let fade = |c: egui::Color32| c.gamma_multiply(alpha);
    let (_, _, to_camera) = basis(yaw, pitch);
    let project = projector(gizmo_rect, yaw, pitch, perspective);

    // Compass ring on the ground under the cube; North is +Y.
    let ring: Vec<egui::Pos2> = (0..48)
        .map(|i| {
            let t = i as f32 / 48.0 * std::f32::consts::TAU;
            project(Vec3::new(t.cos() * COMPASS_RADIUS, t.sin() * COMPASS_RADIUS, -1.0))
        })
        .collect();
    painter.add(egui::Shape::closed_line(ring, egui::Stroke::new(5.0_f32, fade(egui::Color32::from_gray(185)))));
    if to_camera.z > 0.2 {
        for (label, dir) in [("N", Vec3::Y), ("E", Vec3::X), ("S", -Vec3::Y), ("W", -Vec3::X)] {
            let p = project(dir * (COMPASS_RADIUS + 0.45) - Vec3::Z);
            painter.text(p, egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(11.0), fade(egui::Color32::from_gray(215)));
        }
    }

    let light = Vec3::new(-0.3, -0.5, 0.8).normalize();
    for face in projected_faces(gizmo_rect, yaw, pitch, perspective) {
        let lit = 0.78 + 0.22 * face.normal.dot(light).max(0.0);
        let gray = (238.0 * lit) as u8;
        painter.add(egui::Shape::convex_polygon(face.points.to_vec(), fade(egui::Color32::from_gray(gray)), egui::Stroke::new(1.0_f32, fade(egui::Color32::from_gray(150)))));
        let c = face.points.iter().fold(egui::Pos2::ZERO, |sum, p| sum + p.to_vec2()) / face.points.len() as f32;
        painter.text(c, egui::Align2::CENTER_CENTER, face.label, egui::FontId::proportional(10.0), fade(egui::Color32::from_gray(70)));
    }

    if hovered {
        let r = home_rect(gizmo_rect);
        let c = egui::Color32::from_gray(200);
        let roof = [egui::pos2(r.center().x, r.top()), egui::pos2(r.right(), r.top() + 6.0), egui::pos2(r.left(), r.top() + 6.0)];
        painter.add(egui::Shape::convex_polygon(roof.to_vec(), c, egui::Stroke::NONE));
        painter.rect_filled(egui::Rect::from_min_max(egui::pos2(r.left() + 3.0, r.top() + 6.0), egui::pos2(r.right() - 3.0, r.bottom())), 0.0, c);
    }
}

/// Inactive opacity (Viewport Configuration > ViewCube > Inactive Opacity, 50%).
const INACTIVE_OPACITY: f32 = 0.5;
/// Pixels per cube half-size unit.
const SCALE: f32 = 24.0;
/// Compass ring radius in cube half-size units.
const COMPASS_RADIUS: f32 = 1.75;

/// Camera distance of the cube's own perspective, in cube half-sizes.
const EYE_DISTANCE: f32 = 5.0;

/// World (cube space) to screen: the viewport's own projection, so the cube
/// is in perspective in a Perspective view and orthographic in an axis view.
fn projector(rect: egui::Rect, yaw: f32, pitch: f32, perspective: bool) -> impl Fn(Vec3) -> egui::Pos2 {
    let (right, up, to_camera) = basis(yaw, pitch);
    let center = rect.center();
    move |v: Vec3| {
        // Perspective: shrink so the nearest corner lands about where the ortho one would.
        let f = if perspective { 0.8 * EYE_DISTANCE / (EYE_DISTANCE - v.dot(to_camera)) } else { 1.0 };
        egui::pos2(center.x + v.dot(right) * SCALE * f, center.y - v.dot(up) * SCALE * f)
    }
}

/// Screen right, screen up and the direction toward the camera, in world space.
fn basis(yaw: f32, pitch: f32) -> (Vec3, Vec3, Vec3) {
    let to_camera = Vec3::new(pitch.cos() * yaw.cos(), pitch.cos() * yaw.sin(), pitch.sin()).normalize_or_zero();
    let mut right = Vec3::new(-yaw.sin(), yaw.cos(), 0.0).normalize_or_zero();
    if right.length_squared() <= f32::EPSILON {
        right = Vec3::Y;
    }
    let up = to_camera.cross(right).normalize_or_zero();
    (right, up, to_camera)
}

struct Face2d {
    view: GizmoView,
    points: [egui::Pos2; 4],
    depth: f32,
    normal: Vec3,
    label: &'static str,
}

fn projected_faces(rect: egui::Rect, yaw: f32, pitch: f32, perspective: bool) -> Vec<Face2d> {
    let (_, _, to_camera) = basis(yaw, pitch);
    let project = projector(rect, yaw, pitch, perspective);
    // A face is seen when the eye is on its outer side: in perspective the
    // eye sits EYE_DISTANCE away, so grazing faces drop out a little sooner.
    let facing = if perspective { 1.0 / EYE_DISTANCE + 0.02 } else { 0.10 };

    let verts = [
        Vec3::new(-1.0, -1.0, -1.0),
        Vec3::new(1.0, -1.0, -1.0),
        Vec3::new(1.0, 1.0, -1.0),
        Vec3::new(-1.0, 1.0, -1.0),
        Vec3::new(-1.0, -1.0, 1.0),
        Vec3::new(1.0, -1.0, 1.0),
        Vec3::new(1.0, 1.0, 1.0),
        Vec3::new(-1.0, 1.0, 1.0),
    ];
    let faces = [
        (
            GizmoView::Right,
            Vec3::X,
            [1, 5, 6, 2],
            "RIGHT",
        ),
        (
            GizmoView::Left,
            -Vec3::X,
            [0, 3, 7, 4],
            "LEFT",
        ),
        (
            GizmoView::Back,
            Vec3::Y,
            [3, 2, 6, 7],
            "BACK",
        ),
        (
            GizmoView::Front,
            -Vec3::Y,
            [0, 4, 5, 1],
            "FRONT",
        ),
        (
            GizmoView::Top,
            Vec3::Z,
            [4, 7, 6, 5],
            "TOP",
        ),
        (
            GizmoView::Bottom,
            -Vec3::Z,
            [0, 1, 2, 3],
            "BOTTOM",
        ),
    ];

    let mut out = Vec::new();
    for (view, normal, ids, label) in faces {
        if normal.dot(to_camera) <= facing {
            continue;
        }
        let points = ids.map(|i| project(verts[i]));
        if polygon_area(&points) < 10.0 {
            continue;
        }
        let depth = ids.iter().map(|i| verts[*i].dot(to_camera)).sum::<f32>() / 4.0;
        out.push(Face2d {
            view,
            points,
            depth,
            normal,
            label,
        });
    }
    out.sort_by(|a, b| a.depth.total_cmp(&b.depth));
    out
}

fn polygon_area(points: &[egui::Pos2]) -> f32 {
    if points.len() < 3 {
        return 0.0;
    }
    let mut sum = 0.0;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        sum += a.x * b.y - b.x * a.y;
    }
    sum.abs() * 0.5
}

fn point_in_polygon(point: egui::Pos2, polygon: &[egui::Pos2]) -> bool {
    let mut inside = false;
    let mut j = polygon.len() - 1;
    for i in 0..polygon.len() {
        let pi = polygon[i];
        let pj = polygon[j];
        if ((pi.y > point.y) != (pj.y > point.y))
            && (point.x < (pj.x - pi.x) * (point.y - pi.y) / (pj.y - pi.y) + pi.x)
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_view_shows_top_left_front() {
        let (yaw, pitch) = loom_commands::viewport::HOME_PERSPECTIVE;
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(112.0, 112.0));
        let mut seen: Vec<GizmoView> = projected_faces(rect, yaw, pitch, false).iter().map(|f| f.view).collect();
        seen.sort_by_key(|v| *v as u8);
        assert_eq!(seen, vec![GizmoView::Front, GizmoView::Left, GizmoView::Top]);
    }

    #[test]
    fn top_face_is_drawn_above_the_cube_centre() {
        let (yaw, pitch) = loom_commands::viewport::HOME_PERSPECTIVE;
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(112.0, 112.0));
        let faces = projected_faces(rect, yaw, pitch, false);
        let centre = |v: GizmoView| {
            let f = faces.iter().find(|f| f.view == v).unwrap();
            f.points.iter().fold(egui::Vec2::ZERO, |s, p| s + p.to_vec2()) / 4.0
        };
        assert!(centre(GizmoView::Top).y < rect.center().y, "TOP must be at the top of the cube");
        assert!(centre(GizmoView::Left).x < centre(GizmoView::Front).x, "LEFT is left of FRONT");
    }

    /// In a Perspective view the cube is in perspective too: the TOP face's near
    /// edges are longer than its far ones. In an axis view all four match.
    #[test]
    fn cube_follows_the_viewport_projection() {
        let (yaw, pitch) = loom_commands::viewport::HOME_PERSPECTIVE;
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(112.0, 112.0));
        let spread = |perspective: bool| {
            let faces = projected_faces(rect, yaw, pitch, perspective);
            let top = faces.iter().find(|f| f.view == GizmoView::Top).unwrap();
            let lens: Vec<f32> = (0..4).map(|i| top.points[i].distance(top.points[(i + 1) % 4])).collect();
            lens.iter().cloned().fold(0.0, f32::max) / lens.iter().cloned().fold(f32::MAX, f32::min)
        };
        assert!(spread(true) > 1.1, "perspective: {}", spread(true));
        assert!((spread(false) - 1.0).abs() < 0.01, "ortho: {}", spread(false));
    }

    #[test]
    fn cube_shows_only_in_active_non_camera_views() {
        assert!(shows_in(true, loom_commands::ViewKind::Perspective));
        assert!(shows_in(true, loom_commands::ViewKind::Top));
        assert!(!shows_in(false, loom_commands::ViewKind::Perspective));
        assert!(!shows_in(true, loom_commands::ViewKind::Camera(loom_scene::NodeId(1))));
    }
}
