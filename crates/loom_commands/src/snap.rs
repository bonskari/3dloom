//! Standard snaps: settings (Grid and Snap Settings dialog), the
//! home grid, and snap resolution for a pointer position.

use crate::state::AppState;
use glam::{Vec2, Vec3};
use loom_scene::NodeId;
use serde::{Deserialize, Serialize};

/// Snaps Toggle flyout: 2D (construction plane only), 2.5D (geometry
/// projected onto the plane), 3D (anywhere in space).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SnapMode {
    Snap2D,
    Snap25D,
    #[default]
    Snap3D,
}

impl SnapMode {
    pub fn label(self) -> &'static str {
        match self {
            SnapMode::Snap2D => "2D",
            SnapMode::Snap25D => "2.5D",
            SnapMode::Snap3D => "3D",
        }
    }

    /// Alt+S cycles 2D, 2.5D, 3D.
    pub fn next(self) -> Self {
        match self {
            SnapMode::Snap2D => SnapMode::Snap25D,
            SnapMode::Snap25D => SnapMode::Snap3D,
            SnapMode::Snap3D => SnapMode::Snap2D,
        }
    }
}

/// Snaps tab > Standard check boxes, in dialog order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapTargets {
    pub grid_points: bool,
    pub pivot: bool,
    pub perpendicular: bool,
    pub vertex: bool,
    pub edge_segment: bool,
    pub face: bool,
    pub grid_lines: bool,
    pub bounding_box: bool,
    pub tangent: bool,
    pub endpoint: bool,
    pub midpoint: bool,
    pub center_face: bool,
}

impl Default for SnapTargets {
    /// Only Grid Points is on by default.
    fn default() -> Self {
        Self { grid_points: true, ..Self::none() }
    }
}

impl SnapTargets {
    pub const NAMES: [&'static str; 12] = [
        "Grid Points", "Pivot", "Perpendicular", "Vertex", "Edge/Segment", "Face", "Grid Lines", "Bounding Box", "Tangent", "Endpoint", "Midpoint", "Center Face",
    ];

    pub fn none() -> Self {
        Self {
            grid_points: false,
            pivot: false,
            perpendicular: false,
            vertex: false,
            edge_segment: false,
            face: false,
            grid_lines: false,
            bounding_box: false,
            tangent: false,
            endpoint: false,
            midpoint: false,
            center_face: false,
        }
    }

    pub fn get(&self, name: &str) -> Option<bool> {
        Some(*self.field(name)?)
    }

    pub fn set(&mut self, name: &str, on: bool) -> Result<(), String> {
        *self.field_mut(name).ok_or_else(|| format!("unknown snap target {name:?}"))? = on;
        Ok(())
    }

    fn field(&self, name: &str) -> Option<&bool> {
        Some(match name {
            "Grid Points" => &self.grid_points,
            "Pivot" => &self.pivot,
            "Perpendicular" => &self.perpendicular,
            "Vertex" => &self.vertex,
            "Edge/Segment" => &self.edge_segment,
            "Face" => &self.face,
            "Grid Lines" => &self.grid_lines,
            "Bounding Box" => &self.bounding_box,
            "Tangent" => &self.tangent,
            "Endpoint" => &self.endpoint,
            "Midpoint" => &self.midpoint,
            "Center Face" => &self.center_face,
            _ => return None,
        })
    }

    fn field_mut(&mut self, name: &str) -> Option<&mut bool> {
        Some(match name {
            "Grid Points" => &mut self.grid_points,
            "Pivot" => &mut self.pivot,
            "Perpendicular" => &mut self.perpendicular,
            "Vertex" => &mut self.vertex,
            "Edge/Segment" => &mut self.edge_segment,
            "Face" => &mut self.face,
            "Grid Lines" => &mut self.grid_lines,
            "Bounding Box" => &mut self.bounding_box,
            "Tangent" => &mut self.tangent,
            "Endpoint" => &mut self.endpoint,
            "Midpoint" => &mut self.midpoint,
            "Center Face" => &mut self.center_face,
            _ => return None,
        })
    }
}

/// Snaps and Options tabs of Grid and Snap Settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapSettings {
    pub mode: SnapMode,
    pub targets: SnapTargets,
    /// Options > Marker > Display / Size (px).
    pub marker_display: bool,
    pub marker_size: f32,
    /// Options > General.
    pub preview_radius_px: f32,
    pub radius_px: f32,
    pub angle_deg: f32,
    pub percent: f32,
    pub snap_to_frozen: bool,
    /// Options > Translation.
    pub use_axis_constraints: bool,
    pub show_rubber_band: bool,
    /// Snaps shortcut menu > Snap Override: one target for the next snap.
    pub override_target: Option<String>,
    /// "Last" entry of the Snap Override quadrant.
    pub last_override: Option<String>,
    /// Alt+Shift+S: which of the candidates inside the radius is used.
    pub hit_index: usize,
}

impl Default for SnapSettings {
    fn default() -> Self {
        Self {
            mode: SnapMode::Snap3D,
            targets: SnapTargets::default(),
            marker_display: true,
            marker_size: 20.0,
            preview_radius_px: 30.0,
            radius_px: 20.0,
            angle_deg: 5.0,
            percent: 10.0,
            snap_to_frozen: false,
            use_axis_constraints: false,
            show_rubber_band: true,
            override_target: None,
            last_override: None,
            hit_index: 0,
        }
    }
}

/// Home Grid tab.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HomeGrid {
    pub spacing: f32,
    pub major_every: u32,
    pub perspective_extent: u32,
    pub inhibit_subdivision: bool,
    pub inhibit_resize: bool,
    /// Dynamic Update: All Viewports (true) or Active Viewport (false).
    pub dynamic_update_all: bool,
}

impl Default for HomeGrid {
    fn default() -> Self {
        Self { spacing: 10.0, major_every: 10, perspective_extent: 7, inhibit_subdivision: false, inhibit_resize: false, dynamic_update_all: false }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnapKind {
    GridPoint,
    GridLine,
    Pivot,
    Perpendicular,
    Vertex,
    EdgeSegment,
    Face,
    BoundingBox,
    Endpoint,
    Midpoint,
    CenterFace,
}

impl SnapKind {
    pub fn label(self) -> &'static str {
        match self {
            SnapKind::GridPoint => "Grid Point",
            SnapKind::GridLine => "Grid Line",
            SnapKind::Pivot => "Pivot",
            SnapKind::Perpendicular => "Perpendicular",
            SnapKind::Vertex => "Vertex",
            SnapKind::EdgeSegment => "Edge/Segment",
            SnapKind::Face => "Face",
            SnapKind::BoundingBox => "Bounding Box",
            SnapKind::Endpoint => "Endpoint",
            SnapKind::Midpoint => "Midpoint",
            SnapKind::CenterFace => "Center Face",
        }
    }

    /// Grid points and lines are secondary to object snaps.
    fn secondary(self) -> bool {
        matches!(self, SnapKind::GridPoint | SnapKind::GridLine)
    }
}

/// Construction plane as (origin, u, v, normal).
pub type Plane = (Vec3, Vec3, Vec3, Vec3);

/// What the pointer is over, for snap resolution.
pub struct SnapQuery<'a> {
    /// Pointer in viewport pixels.
    pub pointer: Vec2,
    /// World ray through the pointer.
    pub ray: (Vec3, Vec3),
    /// World point to viewport pixels (None when behind the camera).
    pub project: &'a dyn Fn(Vec3) -> Option<Vec2>,
    pub plane: Plane,
    /// Start point of the current operation (perpendicular / rubber band).
    pub start: Option<Vec3>,
    /// Nodes not snapped to (the ones being moved).
    pub exclude: &'a [NodeId],
    /// Vertices not snapped to (a sub-object selection being moved).
    pub exclude_vertices: &'a [(NodeId, u32)],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapHit {
    pub point: Vec3,
    pub kind: SnapKind,
    pub screen: Vec2,
    pub distance_px: f32,
}

/// Nearest grid intersection on the plane.
pub fn grid_point(p: Vec3, plane: Plane, spacing: f32) -> Vec3 {
    let (o, u, v, _) = plane;
    let d = p - o;
    let s = spacing.max(1e-6);
    o + u * ((d.dot(u) / s).round() * s) + v * ((d.dot(v) / s).round() * s)
}

/// Nearest point on a grid line of the plane.
pub fn grid_line(p: Vec3, plane: Plane, spacing: f32) -> Vec3 {
    let (o, u, v, _) = plane;
    let d = p - o;
    let s = spacing.max(1e-6);
    let (a, b) = (d.dot(u), d.dot(v));
    let (ra, rb) = ((a / s).round() * s, (b / s).round() * s);
    if (a - ra).abs() <= (b - rb).abs() { o + u * ra + v * b } else { o + u * a + v * rb }
}

/// Rounds `deg` to the nearest multiple of `increment`.
pub fn angle(deg: f32, increment: f32) -> f32 {
    if increment <= 0.0 { deg } else { (deg / increment).round() * increment }
}

/// Rounds a scale factor to the nearest `percent` step.
pub fn percent(factor: f32, percent: f32) -> f32 {
    let step = percent / 100.0;
    if step <= 0.0 { factor } else { (factor / step).round() * step }
}

fn plane_hit(ray: (Vec3, Vec3), plane: Plane) -> Option<Vec3> {
    let (o, d) = ray;
    let (po, _, _, n) = plane;
    let denom = d.dot(n);
    if denom.abs() <= 1e-6 {
        return None;
    }
    Some(o + d * ((po - o).dot(n) / denom))
}

fn project_to_plane(p: Vec3, plane: Plane) -> Vec3 {
    let (o, _, _, n) = plane;
    p - n * (p - o).dot(n)
}

fn closest_on_segment(a: Vec3, b: Vec3, ray: (Vec3, Vec3)) -> Vec3 {
    // Closest point on segment AB to the ray (line-line closest points).
    let (o, d) = ray;
    let ab = b - a;
    let l2 = ab.length_squared();
    if l2 <= 1e-12 {
        return a;
    }
    let w0 = a - o;
    let (aa, bb, cc, dd, ee) = (ab.dot(ab), ab.dot(d), d.dot(d), ab.dot(w0), d.dot(w0));
    let denom = aa * cc - bb * bb;
    let t = if denom.abs() < 1e-9 { 0.0 } else { ((bb * ee - cc * dd) / denom).clamp(0.0, 1.0) };
    a + ab * t
}

fn ray_triangle(ray: (Vec3, Vec3), a: Vec3, b: Vec3, c: Vec3) -> Option<Vec3> {
    let (o, d) = ray;
    let (e1, e2) = (b - a, c - a);
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let t = o - a;
    let u = t.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = t.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let dist = e2.dot(q) * inv;
    (dist >= 0.0).then(|| o + d * dist)
}

/// The snap point for the pointer, or None when snaps are off or nothing
/// is within the snap radius.
pub fn resolve(state: &AppState, q: &SnapQuery) -> Option<SnapHit> {
    resolve_within(state, q, state.snaps.radius_px)
}

/// `resolve` with an explicit pixel radius (the preview radius shows a
/// marker before the pointer is close enough to snap).
pub fn resolve_within(state: &AppState, q: &SnapQuery, radius_px: f32) -> Option<SnapHit> {
    if !state.snaps_on {
        return None;
    }
    let s = &state.snaps;
    let mut targets = s.targets;
    if let Some(name) = &s.override_target {
        targets = SnapTargets::none();
        if name != "None" {
            let _ = targets.set(name, true);
        }
    }
    let mut cands: Vec<(Vec3, SnapKind)> = Vec::new();
    // Face: only the nearest surface along the ray, across every node.
    let mut face_hit: Option<(f32, Vec3)> = None;
    let spacing = state.grid_spacing();
    if let Some(on_plane) = plane_hit(q.ray, q.plane) {
        if targets.grid_points {
            cands.push((grid_point(on_plane, q.plane, spacing), SnapKind::GridPoint));
        }
        if targets.grid_lines {
            cands.push((grid_line(on_plane, q.plane, spacing), SnapKind::GridLine));
        }
    }
    let wants_geometry = targets.pivot || targets.perpendicular || targets.vertex || targets.edge_segment || targets.face || targets.bounding_box || targets.endpoint || targets.midpoint || targets.center_face;
    if wants_geometry {
        for node in state.scene.nodes() {
            if q.exclude.contains(&node.id) || node.hidden || !state.is_visible(node) || (node.frozen && !s.snap_to_frozen) {
                continue;
            }
            let world = state.scene.object_matrix(node.id);
            if targets.pivot {
                cands.push((state.scene.world_matrix(node.id).w_axis.truncate(), SnapKind::Pivot));
            }
            if targets.bounding_box {
                if let Some((min, max)) = state.object_bounds(node.id) {
                    for i in 0..8 {
                        let c = Vec3::new(if i & 1 == 0 { min.x } else { max.x }, if i & 2 == 0 { min.y } else { max.y }, if i & 4 == 0 { min.z } else { max.z });
                        cands.push((c, SnapKind::BoundingBox));
                    }
                }
            }
            let mesh = node.cage_mesh();
            if targets.vertex || targets.endpoint {
                let kind = if targets.vertex { SnapKind::Vertex } else { SnapKind::Endpoint };
                for (i, v) in mesh.vertices().iter().enumerate() {
                    if q.exclude_vertices.contains(&(node.id, i as u32)) {
                        continue;
                    }
                    cands.push((world.transform_point3(v.position), kind));
                }
            }
            if targets.midpoint || targets.edge_segment || targets.perpendicular {
                for (a, b) in mesh.unique_edges() {
                    let (a, b) = (world.transform_point3(a), world.transform_point3(b));
                    if targets.midpoint {
                        cands.push(((a + b) * 0.5, SnapKind::Midpoint));
                    }
                    if targets.edge_segment {
                        cands.push((closest_on_segment(a, b, q.ray), SnapKind::EdgeSegment));
                    }
                    if let (true, Some(start)) = (targets.perpendicular, q.start) {
                        let ab = b - a;
                        let t = ((start - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
                        cands.push((a + ab * t, SnapKind::Perpendicular));
                    }
                }
            }
            if targets.face || targets.center_face {
                for f in 0..mesh.face_count() {
                    let Some(pts) = mesh.face_positions(loom_mesh::FaceId(f as u32)) else { continue };
                    let pts: Vec<Vec3> = pts.into_iter().map(|p| world.transform_point3(p)).collect();
                    if targets.center_face && !pts.is_empty() {
                        cands.push((pts.iter().copied().sum::<Vec3>() / pts.len() as f32, SnapKind::CenterFace));
                    }
                    if targets.face {
                        for i in 1..pts.len().saturating_sub(1) {
                            if let Some(h) = ray_triangle(q.ray, pts[0], pts[i], pts[i + 1]) {
                                let t = (h - q.ray.0).dot(q.ray.1);
                                if face_hit.is_none_or(|(bt, _)| t < bt) {
                                    face_hit = Some((t, h));
                                }
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
    if let Some((_, h)) = face_hit {
        cands.push((h, SnapKind::Face));
    }
    // 2D: only points on the construction plane; 2.5D: project onto it.
    let (_, _, _, n) = q.plane;
    let on_plane = |p: Vec3| (p - q.plane.0).dot(n).abs() < 1e-4;
    let mut hits: Vec<SnapHit> = cands
        .into_iter()
        .filter_map(|(p, kind)| {
            let p = match s.mode {
                SnapMode::Snap2D if !kind.secondary() && !on_plane(p) => return None,
                SnapMode::Snap25D if !kind.secondary() => project_to_plane(p, q.plane),
                _ => p,
            };
            let screen = (q.project)(p)?;
            let distance_px = screen.distance(q.pointer);
            (distance_px <= radius_px).then_some(SnapHit { point: p, kind, screen, distance_px })
        })
        .collect();
    if hits.is_empty() {
        return None;
    }
    hits.sort_by(|a, b| a.kind.secondary().cmp(&b.kind.secondary()).then(a.distance_px.total_cmp(&b.distance_px)));
    Some(hits[s.hit_index % hits.len()])
}

impl AppState {
    /// Angle Snap: degrees rounded to the Options tab increment when on.
    pub fn snap_angle_deg(&self, deg: f32) -> f32 {
        if self.angle_snap_on { angle(deg, self.snaps.angle_deg) } else { deg }
    }

    /// Percent Snap: scale factor rounded to the Options tab percent when on.
    pub fn snap_scale_factor(&self, factor: f32) -> f32 {
        if self.percent_snap_on { percent(factor, self.snaps.percent) } else { factor }
    }
}
