//! The ViewCube: 26 hot spots (6 faces, 12 edges, 8 corners), the Home
//! icon, and in a face view the two roll arrows and the four adjacent-face
//! triangles. Drawn in the viewport's own projection and orientation.

use eframe::egui;
use glam::{IVec3, Vec3};
use loom_input::OrbitCamera;

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

/// What a point on the ViewCube is over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    /// A face, edge or corner: the direction toward the eye, each component -1, 0 or 1.
    Spot(IVec3),
    Home,
    /// Roll arrows: counterclockwise / clockwise by 90 degrees.
    RollLeft,
    RollRight,
    /// Adjacent-face triangle on the cube's top, right, bottom or left side.
    Adjacent(Side),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Top,
    Right,
    Bottom,
    Left,
}

/// ViewCube Size (Viewport Configuration > ViewCube): Large, Normal, Small, Tiny.
pub fn size_factor(size: u8) -> f32 {
    match size {
        0 => 1.3,
        2 => 0.75,
        3 => 0.5,
        _ => 1.0,
    }
}

pub fn rect(viewport: egui::Rect, size: u8) -> egui::Rect {
    let s = 112.0 * size_factor(size);
    egui::Rect::from_min_size(viewport.right_top() + egui::vec2(-s - 12.0, 12.0), egui::vec2(s, s))
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

/// The face whose outward normal is `dir`, for a face spot.
pub fn face_of(dir: IVec3) -> Option<GizmoView> {
    Some(match dir.to_array() {
        [0, -1, 0] => GizmoView::Front,
        [0, 1, 0] => GizmoView::Back,
        [-1, 0, 0] => GizmoView::Left,
        [1, 0, 0] => GizmoView::Right,
        [0, 0, 1] => GizmoView::Top,
        [0, 0, -1] => GizmoView::Bottom,
        _ => return None,
    })
}

/// Yaw and pitch that look along `-dir` (from `dir` toward the centre).
pub fn spot_angles(dir: IVec3) -> (f32, f32) {
    if let Some(face) = face_of(dir) {
        return view_kind(face).angles().expect("axis views have fixed angles");
    }
    let d = dir.as_vec3().normalize();
    (d.y.atan2(d.x), d.z.asin())
}

/// The roll that keeps `up` as screen up for a view at (yaw, pitch).
pub fn roll_for(yaw: f32, pitch: f32, up: Vec3) -> f32 {
    let (r0, u0, _) = OrbitCamera { yaw, pitch, roll: 0.0, ..OrbitCamera::default() }.basis();
    (-up.dot(r0)).atan2(up.dot(u0))
}

/// The face the view looks straight at, if it is a face view.
pub fn face_view(cam: &OrbitCamera) -> Option<IVec3> {
    let (_, _, to_eye) = cam.basis();
    let axis = [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z].into_iter().find(|a| a.dot(to_eye) > 0.9995)?;
    Some(axis.round().as_ivec3())
}

/// An adjacent-face triangle's target: the new direction toward the eye and
/// the new screen up, turning the view 90 degrees toward that side.
pub fn adjacent(cam: &OrbitCamera, side: Side) -> (Vec3, Vec3) {
    let (right, up, to_eye) = cam.basis();
    let snap = |v: Vec3| {
        let a = v.abs();
        if a.x >= a.y && a.x >= a.z { Vec3::X * v.x.signum() } else if a.y >= a.z { Vec3::Y * v.y.signum() } else { Vec3::Z * v.z.signum() }
    };
    let (eye, up) = match side {
        Side::Top => (up, -to_eye),
        Side::Bottom => (-up, to_eye),
        Side::Right => (right, up),
        Side::Left => (-right, up),
    };
    (snap(eye), snap(up))
}

/// The Home icon above the cube's top-left corner (shown while the cube is hovered).
pub fn home_rect(gizmo_rect: egui::Rect) -> egui::Rect {
    egui::Rect::from_min_size(gizmo_rect.left_top() + egui::vec2(4.0, 2.0), egui::vec2(16.0, 14.0))
}

fn scale(gizmo_rect: egui::Rect) -> f32 {
    24.0 * gizmo_rect.width() / 112.0
}

/// Centres of the roll arrows (left = counterclockwise, right = clockwise), above right of the cube.
fn roll_centres(gizmo_rect: egui::Rect) -> [egui::Pos2; 2] {
    let c = gizmo_rect.center();
    let s = scale(gizmo_rect);
    [c + egui::vec2(1.05, -1.75) * s, c + egui::vec2(1.75, -1.05) * s]
}

/// The adjacent-face triangles: (side, its three corners).
fn triangles(gizmo_rect: egui::Rect) -> [(Side, [egui::Pos2; 3]); 4] {
    let c = gizmo_rect.center();
    let s = scale(gizmo_rect);
    let tri = |dir: egui::Vec2| {
        let n = egui::vec2(-dir.y, dir.x);
        [c + dir * (1.75 * s), c + dir * (1.3 * s) + n * (0.35 * s), c + dir * (1.3 * s) - n * (0.35 * s)]
    };
    [(Side::Top, tri(egui::vec2(0.0, -1.0))), (Side::Right, tri(egui::vec2(1.0, 0.0))), (Side::Bottom, tri(egui::vec2(0.0, 1.0))), (Side::Left, tri(egui::vec2(-1.0, 0.0)))]
}

/// What `pos` is over: the Home icon, a face view's arrows and triangles, or a hot spot.
pub fn hit_test(pos: egui::Pos2, gizmo_rect: egui::Rect, cam: &OrbitCamera, perspective: bool) -> Option<Hit> {
    if home_rect(gizmo_rect).contains(pos) {
        return Some(Hit::Home);
    }
    if face_view(cam).is_some() {
        let r = 0.35 * scale(gizmo_rect);
        let [left, right] = roll_centres(gizmo_rect);
        if pos.distance(left) <= r {
            return Some(Hit::RollLeft);
        }
        if pos.distance(right) <= r {
            return Some(Hit::RollRight);
        }
        for (side, t) in triangles(gizmo_rect) {
            if point_in_polygon(pos, &t) {
                return Some(Hit::Adjacent(side));
            }
        }
    }
    for spot in projected_spots(gizmo_rect, cam, perspective).iter().rev() {
        if point_in_polygon(pos, &spot.points) {
            return Some(Hit::Spot(spot.dir));
        }
    }
    None
}

/// The ViewCube shows in the active viewport (or every one with Show in All
/// Views), and never through a camera.
pub fn shows_in(active: bool, kind: loom_commands::ViewKind, settings: &loom_commands::ViewCubeSettings) -> bool {
    settings.show && (active || settings.all_views) && !matches!(kind, loom_commands::ViewKind::Camera(_))
}

/// The ViewCube: a light grey lit cube over a compass ring, no backdrop.
/// Inactive it is see-through (`inactive_opacity`); under the pointer it is
/// opaque, shows Home, and highlights the hot spot under it.
pub fn draw(painter: &egui::Painter, gizmo_rect: egui::Rect, cam: &OrbitCamera, perspective: bool, hovered: Option<Option<Hit>>, settings: &loom_commands::ViewCubeSettings) {
    let alpha = if hovered.is_some() { 1.0 } else { settings.inactive_opacity.clamp(0.0, 1.0) };
    if alpha <= 0.0 {
        return;
    }
    let fade = |c: egui::Color32| c.gamma_multiply(alpha);
    let (_, _, to_camera) = cam.basis();
    let project = projector(gizmo_rect, cam, perspective);
    let font = |px: f32| egui::FontId::proportional(px * gizmo_rect.width() / 112.0);

    if settings.compass {
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
                painter.text(p, egui::Align2::CENTER_CENTER, label, font(11.0), fade(egui::Color32::from_gray(215)));
            }
        }
    }

    let light = Vec3::new(-0.3, -0.5, 0.8).normalize();
    let hot = match hovered {
        Some(Some(Hit::Spot(d))) => Some(d),
        _ => None,
    };
    for spot in projected_spots(gizmo_rect, cam, perspective) {
        let normal = spot.face.as_vec3();
        let lit = 0.78 + 0.22 * normal.dot(light).max(0.0);
        let gray = (238.0 * lit) as u8;
        let fill = if hot == Some(spot.dir) { egui::Color32::from_rgb(140, 190, 240) } else { egui::Color32::from_gray(gray) };
        painter.add(egui::Shape::convex_polygon(spot.points.to_vec(), fade(fill), egui::Stroke::NONE));
    }
    for face in projected_faces(gizmo_rect, cam, perspective) {
        painter.add(egui::Shape::closed_line(face.points.to_vec(), egui::Stroke::new(1.0_f32, fade(egui::Color32::from_gray(150)))));
        let c = face.points.iter().fold(egui::Pos2::ZERO, |sum, p| sum + p.to_vec2()) / face.points.len() as f32;
        painter.text(c, egui::Align2::CENTER_CENTER, face.label, font(10.0), fade(egui::Color32::from_gray(70)));
    }

    if hovered.is_some() {
        let r = home_rect(gizmo_rect);
        let c = if hovered == Some(Some(Hit::Home)) { egui::Color32::from_rgb(140, 190, 240) } else { egui::Color32::from_gray(200) };
        let roof = [egui::pos2(r.center().x, r.top()), egui::pos2(r.right(), r.top() + 6.0), egui::pos2(r.left(), r.top() + 6.0)];
        painter.add(egui::Shape::convex_polygon(roof.to_vec(), c, egui::Stroke::NONE));
        painter.rect_filled(egui::Rect::from_min_max(egui::pos2(r.left() + 3.0, r.top() + 6.0), egui::pos2(r.right() - 3.0, r.bottom())), 0.0, c);
        if face_view(cam).is_some() {
            let tint = |h: Hit| if hovered == Some(Some(h)) { egui::Color32::from_rgb(140, 190, 240) } else { egui::Color32::from_gray(200) };
            for (side, t) in triangles(gizmo_rect) {
                painter.add(egui::Shape::convex_polygon(t.to_vec(), tint(Hit::Adjacent(side)), egui::Stroke::NONE));
            }
            let s = scale(gizmo_rect);
            for (centre, hit, ccw) in [(roll_centres(gizmo_rect)[0], Hit::RollLeft, true), (roll_centres(gizmo_rect)[1], Hit::RollRight, false)] {
                // A curled arrow: an arc with a head at its end.
                let rad = 0.28 * s;
                let (a0, a1) = if ccw { (0.3_f32, 2.6_f32) } else { (2.6_f32, 0.3_f32) };
                let pts: Vec<egui::Pos2> = (0..=10).map(|i| {
                    let a = a0 + (a1 - a0) * i as f32 / 10.0;
                    centre + egui::vec2(a.cos(), -a.sin()) * rad
                }).collect();
                let end = *pts.last().unwrap();
                let tangent = (end - pts[pts.len() - 2]).normalized();
                let n = egui::vec2(-tangent.y, tangent.x);
                let color = tint(hit);
                painter.add(egui::Shape::line(pts, egui::Stroke::new(2.0_f32, color)));
                painter.add(egui::Shape::convex_polygon(vec![end + tangent * 4.0, end + n * 3.5, end - n * 3.5], color, egui::Stroke::NONE));
            }
        }
    }
}

/// Compass ring radius in cube half-size units.
const COMPASS_RADIUS: f32 = 1.75;

/// Camera distance of the cube's own perspective, in cube half-sizes.
const EYE_DISTANCE: f32 = 5.0;

/// Where the band between a face's centre and its edge and corner spots
/// starts, in face half-size units.
const SPOT_EDGE: f32 = 0.6;

/// World (cube space) to screen: the viewport's own projection, so the cube
/// is in perspective in a Perspective view and orthographic in an axis view.
fn projector(rect: egui::Rect, cam: &OrbitCamera, perspective: bool) -> impl Fn(Vec3) -> egui::Pos2 {
    let (right, up, to_camera) = cam.basis();
    let center = rect.center();
    let s = scale(rect);
    move |v: Vec3| {
        // Perspective: shrink so the nearest corner lands about where the ortho one would.
        let f = if perspective { 0.8 * EYE_DISTANCE / (EYE_DISTANCE - v.dot(to_camera)) } else { 1.0 };
        egui::pos2(center.x + v.dot(right) * s * f, center.y - v.dot(up) * s * f)
    }
}

struct Face2d {
    points: [egui::Pos2; 4],
    depth: f32,
    label: &'static str,
    normal: Vec3,
    /// In-face axes (u, v) of the 3 × 3 spot grid.
    u: Vec3,
    v: Vec3,
}

/// The six faces: outward normal, the two in-face axes, label.
const FACES: [(Vec3, Vec3, Vec3, &str); 6] = [
    (Vec3::X, Vec3::Y, Vec3::Z, "RIGHT"),
    (Vec3::NEG_X, Vec3::NEG_Y, Vec3::Z, "LEFT"),
    (Vec3::Y, Vec3::NEG_X, Vec3::Z, "BACK"),
    (Vec3::NEG_Y, Vec3::X, Vec3::Z, "FRONT"),
    (Vec3::Z, Vec3::X, Vec3::Y, "TOP"),
    (Vec3::NEG_Z, Vec3::X, Vec3::NEG_Y, "BOTTOM"),
];

fn projected_faces(rect: egui::Rect, cam: &OrbitCamera, perspective: bool) -> Vec<Face2d> {
    let (_, _, to_camera) = cam.basis();
    let project = projector(rect, cam, perspective);
    // A face is seen when the eye is on its outer side: in perspective the
    // eye sits EYE_DISTANCE away, so grazing faces drop out a little sooner.
    let facing = if perspective { 1.0 / EYE_DISTANCE + 0.02 } else { 0.10 };
    let mut out = Vec::new();
    for (normal, u, v, label) in FACES {
        if normal.dot(to_camera) <= facing {
            continue;
        }
        let corners = [normal - u - v, normal + u - v, normal + u + v, normal - u + v];
        let points = corners.map(&project);
        if polygon_area(&points) < 10.0 {
            continue;
        }
        let depth = normal.dot(to_camera);
        out.push(Face2d { points, depth, label, normal, u, v });
    }
    out.sort_by(|a, b| a.depth.total_cmp(&b.depth));
    out
}

struct Spot2d {
    /// The spot's direction (face + edge/corner neighbours).
    dir: IVec3,
    /// The face it is drawn on.
    face: IVec3,
    points: [egui::Pos2; 4],
}

/// Every visible face cut into its 3 × 3 spots, back to front.
fn projected_spots(rect: egui::Rect, cam: &OrbitCamera, perspective: bool) -> Vec<Spot2d> {
    let project = projector(rect, cam, perspective);
    let bands = [(-1.0, -SPOT_EDGE, -1), (-SPOT_EDGE, SPOT_EDGE, 0), (SPOT_EDGE, 1.0, 1)];
    let mut out = Vec::new();
    for f in projected_faces(rect, cam, perspective) {
        for (u0, u1, du) in bands {
            for (v0, v1, dv) in bands {
                let at = |a: f32, b: f32| project(f.normal + f.u * a + f.v * b);
                let points = [at(u0, v0), at(u1, v0), at(u1, v1), at(u0, v1)];
                let dir = f.normal + f.u * du as f32 + f.v * dv as f32;
                out.push(Spot2d { dir: dir.round().as_ivec3(), face: f.normal.round().as_ivec3(), points });
            }
        }
    }
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

    fn home() -> OrbitCamera {
        let (yaw, pitch) = loom_commands::viewport::HOME_PERSPECTIVE;
        OrbitCamera { yaw, pitch, ..OrbitCamera::default() }
    }

    fn unit() -> egui::Rect {
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(112.0, 112.0))
    }

    #[test]
    fn home_view_shows_top_left_front() {
        let mut seen: Vec<&str> = projected_faces(unit(), &home(), false).iter().map(|f| f.label).collect();
        seen.sort();
        assert_eq!(seen, vec!["FRONT", "LEFT", "TOP"]);
    }

    #[test]
    fn top_face_is_drawn_above_the_cube_centre() {
        let faces = projected_faces(unit(), &home(), false);
        let centre = |l: &str| {
            let f = faces.iter().find(|f| f.label == l).unwrap();
            f.points.iter().fold(egui::Vec2::ZERO, |s, p| s + p.to_vec2()) / 4.0
        };
        assert!(centre("TOP").y < unit().center().y, "TOP must be at the top of the cube");
        assert!(centre("LEFT").x < centre("FRONT").x, "LEFT is left of FRONT");
    }

    /// In a Perspective view the cube is in perspective too: the TOP face's near
    /// edges are longer than its far ones. In an axis view all four match.
    #[test]
    fn cube_follows_the_viewport_projection() {
        let spread = |perspective: bool| {
            let faces = projected_faces(unit(), &home(), perspective);
            let top = faces.iter().find(|f| f.label == "TOP").unwrap();
            let lens: Vec<f32> = (0..4).map(|i| top.points[i].distance(top.points[(i + 1) % 4])).collect();
            lens.iter().cloned().fold(0.0, f32::max) / lens.iter().cloned().fold(f32::MAX, f32::min)
        };
        assert!(spread(true) > 1.1, "perspective: {}", spread(true));
        assert!((spread(false) - 1.0).abs() < 0.01, "ortho: {}", spread(false));
    }

    #[test]
    fn cube_shows_only_in_active_non_camera_views() {
        let s = loom_commands::ViewCubeSettings::default();
        assert!(shows_in(true, loom_commands::ViewKind::Perspective, &s));
        assert!(shows_in(true, loom_commands::ViewKind::Top, &s));
        assert!(!shows_in(false, loom_commands::ViewKind::Perspective, &s));
        assert!(!shows_in(true, loom_commands::ViewKind::Camera(loom_scene::NodeId(1)), &s));
        let all = loom_commands::ViewCubeSettings { all_views: true, ..s };
        assert!(shows_in(false, loom_commands::ViewKind::Perspective, &all));
        let hidden = loom_commands::ViewCubeSettings { show: false, ..s };
        assert!(!shows_in(true, loom_commands::ViewKind::Perspective, &hidden));
    }

    /// The 26 hot spots: from the Home view the corner nearest the eye, the
    /// edges around it and the three faces' centres can all be hit.
    #[test]
    fn home_view_hits_faces_edges_and_the_near_corner() {
        let cam = home();
        let project = projector(unit(), &cam, false);
        let hit = |p: Vec3| hit_test(project(p), unit(), &cam, false);
        assert_eq!(hit(Vec3::new(0.0, -1.0, 0.0)), Some(Hit::Spot(IVec3::new(0, -1, 0))), "FRONT centre");
        assert_eq!(hit(Vec3::new(-0.9, -0.9, 0.9)), Some(Hit::Spot(IVec3::new(-1, -1, 1))), "the front-left-top corner");
        assert_eq!(hit(Vec3::new(0.0, -0.9, 0.9)), Some(Hit::Spot(IVec3::new(0, -1, 1))), "the front-top edge");
        assert_eq!(hit(Vec3::new(-0.9, 0.0, 0.9)), Some(Hit::Spot(IVec3::new(-1, 0, 1))), "the left-top edge");
    }

    #[test]
    fn corner_spot_looks_down_its_diagonal() {
        let (yaw, pitch) = spot_angles(IVec3::new(-1, -1, 1));
        let cam = OrbitCamera { yaw, pitch, ..OrbitCamera::default() };
        let d = (cam.eye_position() - cam.target).normalize();
        assert!((d - Vec3::new(-1.0, -1.0, 1.0).normalize()).length() < 1e-4, "{d}");
    }

    /// In a face view the roll arrows and adjacent-face triangles answer.
    #[test]
    fn face_view_has_roll_arrows_and_adjacent_triangles() {
        let (yaw, pitch) = loom_commands::ViewKind::Front.angles().unwrap();
        let front = OrbitCamera { yaw, pitch, ..OrbitCamera::default() };
        assert_eq!(face_view(&front), Some(IVec3::new(0, -1, 0)));
        assert_eq!(hit_test(roll_centres(unit())[0], unit(), &front, false), Some(Hit::RollLeft));
        assert_eq!(hit_test(roll_centres(unit())[1], unit(), &front, false), Some(Hit::RollRight));
        let tri = triangles(unit())[1].1;
        let c = egui::pos2((tri[0].x + tri[1].x + tri[2].x) / 3.0, (tri[0].y + tri[1].y + tri[2].y) / 3.0);
        assert_eq!(hit_test(c, unit(), &front, false), Some(Hit::Adjacent(Side::Right)));
        // Not in a three-quarter view.
        assert_eq!(hit_test(roll_centres(unit())[0], unit(), &home(), false).filter(|h| *h == Hit::RollLeft), None);
        // Front's right-hand neighbour is Right; its top neighbour is Top, seen with +Y up.
        assert_eq!(adjacent(&front, Side::Right), (Vec3::X, Vec3::Z));
        assert_eq!(adjacent(&front, Side::Top), (Vec3::Z, Vec3::Y));
    }

    #[test]
    fn roll_for_keeps_the_given_up() {
        let (yaw, pitch) = loom_commands::ViewKind::Front.angles().unwrap();
        let roll = roll_for(yaw, pitch, Vec3::X);
        let cam = OrbitCamera { yaw, pitch, roll, ..OrbitCamera::default() };
        assert!((cam.up() - Vec3::X).length() < 1e-4, "{}", cam.up());
    }
}
