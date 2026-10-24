//! Transform gizmos: Move (arrows, plane squares, centre box),
//! Rotate (axis rings, Screen ring, Free sphere) and Scale (axes with box
//! ends, plane triangles, centre triangle), plus the axis tripod shown when
//! no transform tool is active. Geometry is a constant pixel size
//! (Preferences > Gizmos > Size) at the transform centre, oriented by the
//! reference coordinate system.

pub mod move_gizmo;
pub mod rotate_gizmo;
pub mod scale_gizmo;
pub mod tripod;
#[cfg(test)]
mod tests;

use eframe::egui;
use glam::{Mat3, Mat4, Quat, Vec2, Vec3};
pub use loom_commands::Axis;
use loom_commands::{Command, CoordSystem, Tool};

use crate::LoomApp;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plane {
    XY,
    YZ,
    ZX,
}

impl Plane {
    pub const ALL: [Plane; 3] = [Plane::XY, Plane::YZ, Plane::ZX];

    pub fn axes(self) -> (Axis, Axis) {
        match self {
            Plane::XY => (Axis::X, Axis::Y),
            Plane::YZ => (Axis::Y, Axis::Z),
            Plane::ZX => (Axis::Z, Axis::X),
        }
    }

    pub fn normal(self) -> Axis {
        match self {
            Plane::XY => Axis::Z,
            Plane::YZ => Axis::X,
            Plane::ZX => Axis::Y,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
    Axis(Axis),
    Plane(Plane),
    /// Move: screen-plane drag from the centre box.
    CenterBox,
    /// Scale: all three axes.
    Uniform,
    /// Rotate: around the view direction.
    Screen,
    /// Rotate: trackball.
    Free,
}

/// Where and how big the gizmo is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GizmoLayout {
    pub origin: Vec3,
    /// Columns = reference coordinate system axes (unit vectors).
    pub axes: Mat3,
    pub size_px: f32,
    /// Preferences > Gizmos values the drawing reads.
    pub axis_labels: bool,
    pub plane_size: f32,
    pub plane_offset: f32,
    pub center_box: bool,
    pub screen_handle: bool,
    pub free_rotation: bool,
    pub pie_slice: bool,
    pub angle_data: bool,
    pub scale_size: f32,
}

impl Default for GizmoLayout {
    fn default() -> Self {
        Self {
            origin: Vec3::ZERO,
            axes: Mat3::IDENTITY,
            size_px: 40.0,
            axis_labels: true,
            plane_size: 50.0,
            plane_offset: 50.0,
            center_box: true,
            screen_handle: true,
            free_rotation: true,
            pie_slice: true,
            angle_data: true,
            scale_size: 100.0,
        }
    }
}

impl GizmoLayout {
    pub fn axis(&self, a: Axis) -> Vec3 {
        match a {
            Axis::X => self.axes.x_axis,
            Axis::Y => self.axes.y_axis,
            Axis::Z => self.axes.z_axis,
        }
    }

    /// Axis length in world units so that it spans `size_px` pixels.
    pub fn world_len(&self, p: &Projection) -> f32 {
        self.size_px * p.units_per_pixel(self.origin)
    }
}

/// Camera matrices and the viewport rectangle, for screen/world conversion.
#[derive(Debug, Clone, Copy)]
pub struct Projection {
    pub view: Mat4,
    pub proj: Mat4,
    pub rect: egui::Rect,
    view_proj: Mat4,
    inv_view: Mat4,
}

impl Projection {
    pub fn new(view: Mat4, proj: Mat4, rect: egui::Rect) -> Self {
        Self { view, proj, rect, view_proj: proj * view, inv_view: view.inverse() }
    }

    pub fn screen(&self, world: Vec3) -> Option<egui::Pos2> {
        let clip = self.view_proj * world.extend(1.0);
        if clip.w <= 1e-6 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        Some(egui::pos2(
            self.rect.min.x + (ndc.x + 1.0) * 0.5 * self.rect.width(),
            self.rect.min.y + (1.0 - ndc.y) * 0.5 * self.rect.height(),
        ))
    }

    /// World-space ray through a screen point.
    pub fn ray(&self, p: egui::Pos2) -> (Vec3, Vec3) {
        let local = Vec2::new(p.x - self.rect.left(), p.y - self.rect.top());
        let size = Vec2::new(self.rect.width(), self.rect.height());
        loom_render::pick::screen_to_world_ray(local, size, self.view, self.proj)
    }

    /// Unit vector from the scene toward the viewer.
    pub fn toward_viewer(&self) -> Vec3 {
        self.inv_view.z_axis.truncate().normalize_or_zero()
    }

    pub fn camera_right(&self) -> Vec3 {
        self.inv_view.x_axis.truncate().normalize_or_zero()
    }

    /// World units per screen pixel at the depth of `at`.
    pub fn units_per_pixel(&self, at: Vec3) -> f32 {
        match (self.screen(at), self.screen(at + self.camera_right())) {
            (Some(a), Some(b)) => {
                let d = a.distance(b);
                if d > 1e-6 { 1.0 / d } else { 1.0 }
            }
            _ => 1.0,
        }
    }

    /// Intersection of the pointer ray with a plane through `origin`.
    pub fn hit_plane(&self, p: egui::Pos2, origin: Vec3, normal: Vec3) -> Option<Vec3> {
        let (o, d) = self.ray(p);
        let denom = d.dot(normal);
        if denom.abs() <= 1e-6 {
            return None;
        }
        let t = (origin - o).dot(normal) / denom;
        Some(o + d * t)
    }
}

/// An in-progress gizmo drag.
#[derive(Debug, Clone, Copy)]
pub struct DragState {
    pub handle: Handle,
    pub start: egui::Pos2,
    pub last: egui::Pos2,
    /// Rotate: accumulated (snapped) angle for the pie slice and readout.
    pub angle_deg: f32,
    /// Rotate: unsnapped accumulated angle, so Angle Snap works on slow drags.
    pub raw_angle_deg: f32,
    /// Rotate: accumulated world rotation of Screen / Free drags.
    pub quat: Quat,
    /// Scale: accumulated (snapped) factor for the readout.
    pub factor: f32,
    /// Scale: unsnapped accumulated factor.
    pub raw_factor: f32,
    /// Move: accumulated offset (reference system) for the readout.
    pub offset: Vec3,
    /// Rotate: accumulated Euler degrees in the reference system (axis handles).
    pub rotate_deg: Vec3,
    /// Scale: accumulated per-axis factors.
    pub factors: Vec3,
    /// Shift was held when the drag started: the drag moves a clone.
    pub shift: bool,
    /// Transform centre when the drag began (snap rubber band / perpendicular).
    pub snap_start: Option<Vec3>,
}

impl DragState {
    pub fn new(handle: Handle, start: egui::Pos2) -> Self {
        Self { handle, start, last: start, angle_deg: 0.0, raw_angle_deg: 0.0, quat: Quat::IDENTITY, factor: 1.0, raw_factor: 1.0, offset: Vec3::ZERO, rotate_deg: Vec3::ZERO, factors: Vec3::ONE, shift: false, snap_start: None }
    }
}

pub const HIT_PX: f32 = 8.0;
pub const HOVER: egui::Color32 = egui::Color32::from_rgb(255, 255, 0);

pub fn axis_color(a: Axis) -> egui::Color32 {
    match a {
        Axis::X => egui::Color32::from_rgb(230, 60, 60),
        Axis::Y => egui::Color32::from_rgb(70, 200, 70),
        Axis::Z => egui::Color32::from_rgb(70, 110, 255),
    }
}

pub fn pos_sub(a: egui::Pos2, b: egui::Pos2) -> Vec2 {
    Vec2::new(a.x - b.x, a.y - b.y)
}

pub fn distance_to_segment(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let ab = pos_sub(b, a);
    let denom = ab.length_squared();
    if denom <= f32::EPSILON {
        return p.distance(a);
    }
    let t = (pos_sub(p, a).dot(ab) / denom).clamp(0.0, 1.0);
    let c = egui::pos2(a.x + ab.x * t, a.y + ab.y * t);
    p.distance(c)
}

pub fn point_in_polygon(p: egui::Pos2, poly: &[egui::Pos2]) -> bool {
    let mut inside = false;
    let n = poly.len();
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Projects `origin + axis * t` for the three axes; None when the axis
/// projects to (nearly) a point, i.e. is seen end-on.
pub fn axis_segment(l: &GizmoLayout, p: &Projection, a: Axis, from: f32, to: f32) -> Option<(egui::Pos2, egui::Pos2)> {
    let len = l.world_len(p);
    let s = p.screen(l.origin + l.axis(a) * (len * from))?;
    let e = p.screen(l.origin + l.axis(a) * (len * to))?;
    (s.distance(e) > 2.0 * (to - from).abs()).then_some((s, e))
}

// ---- layout from app state --------------------------------------------------

/// `viewport_height` is the viewport's height in pixels: the gizmo is sized as a
/// percent of the viewport, so it grows with a maximized view.
pub fn layout_for(app: &LoomApp, viewport: usize, viewport_height: f32) -> GizmoLayout {
    let frame = loom_commands::frame_for(&app.state, viewport, app.state.selection.iter().next().copied().filter(|_| app.state.selection.len() == 1 || app.state.coord_system == CoordSystem::Local));
    let g = app.state.prefs.gizmos;
    let mut layout = GizmoLayout {
        origin: frame.origin,
        axes: frame.axes,
        size_px: g.size / 100.0 * viewport_height,
        axis_labels: g.show_axis_labels,
        plane_size: g.move_plane_size,
        plane_offset: g.move_plane_offset,
        center_box: g.center_box_handle,
        screen_handle: g.rotate_screen_handle,
        free_rotation: g.rotate_free_rotation,
        pie_slice: g.rotate_show_pie_slice,
        angle_data: g.rotate_angle_data,
        scale_size: g.scale_size,
    };
    if app.state.selection_mode != loom_commands::SelectionMode::Object {
        layout.origin = app.selection_pivot();
    }
    layout
}

pub fn hit(app: &LoomApp, l: &GizmoLayout, p: &Projection, pointer: egui::Pos2) -> Option<Handle> {
    if !app.state.prefs.gizmos.enabled {
        return None;
    }
    match app.state.active_tool {
        Tool::Move => move_gizmo::hit(l, p, pointer),
        Tool::Rotate => rotate_gizmo::hit(l, p, pointer),
        Tool::Scale => scale_gizmo::hit(l, p, pointer),
        Tool::Select => None,
    }
}

pub fn draw(painter: &egui::Painter, app: &LoomApp, l: &GizmoLayout, p: &Projection, hovered: Option<Handle>, drag: Option<&DragState>) {
    if app.state.active_tool == Tool::Select || !app.state.prefs.gizmos.enabled {
        tripod::draw(painter, app, l, p);
        return;
    }
    let hl = drag.map(|d| d.handle).or(hovered);
    match app.state.active_tool {
        Tool::Move => move_gizmo::draw(painter, l, p, hl, drag),
        Tool::Rotate => rotate_gizmo::draw(painter, l, p, hl, drag),
        Tool::Scale => scale_gizmo::draw(painter, l, p, hl, drag),
        Tool::Select => {}
    }
}

/// Applies one pointer movement of a gizmo drag and updates the drag state.
pub fn drag(app: &mut LoomApp, drag: &mut DragState, l: &GizmoLayout, p: &Projection, pointer: egui::Pos2, viewport: usize) {
    if pointer == drag.last {
        return;
    }
    let system = Some(app.state.coord_system);
    match app.state.active_tool {
        Tool::Move => {
            // Snaps: the transform centre jumps to the snap point under the
            // pointer (optionally restricted to the handle's axes).
            let snapped = crate::viewport_widget::snap_at(app, viewport, p, pointer, drag.snap_start, true).map(|hit| {
                let mut world = hit.point - l.origin;
                if app.state.snaps.use_axis_constraints {
                    world = match drag.handle {
                        Handle::Axis(a) => l.axis(a) * world.dot(l.axis(a)),
                        Handle::Plane(pl) => {
                            let n = l.axis(pl.normal());
                            world - n * world.dot(n)
                        }
                        _ => world,
                    };
                }
                l.axes.transpose() * world
            });
            let free = move_gizmo::drag_delta(l, p, drag.handle, drag.last, pointer);
            if let Some(delta) = snapped.or(free) {
                if delta.length_squared() > 0.0 {
                    drag.offset += delta;
                    let _ = app.dispatch(Command::MoveSelection { delta: delta.to_array(), system });
                }
            }
        }
        Tool::Rotate => match drag.handle {
            Handle::Free => {
                let q = rotate_gizmo::trackball(l, p, drag.last, pointer);
                drag.quat = q * drag.quat;
                let _ = app.dispatch(Command::RotateSelectionFree { quat: q.to_array() });
            }
            Handle::Axis(_) | Handle::Screen => {
                let axis = match drag.handle {
                    Handle::Axis(a) => Some(a),
                    _ => None,
                };
                let raw = rotate_gizmo::drag_angle_deg_handle(l, p, drag.handle, drag.last, pointer);
                drag.raw_angle_deg += raw;
                let target = app.state.snap_angle_deg(drag.raw_angle_deg);
                let step = target - drag.angle_deg;
                if step.abs() > 0.0 {
                    drag.angle_deg = target;
                    match axis {
                        Some(a) => {
                            drag.rotate_deg += move_gizmo::unit(a) * step;
                            let _ = app.dispatch(Command::RotateSelection { axis: a, degrees: step, system });
                        }
                        None => {
                            let q = Quat::from_axis_angle(p.toward_viewer(), step.to_radians());
                            drag.quat = q * drag.quat;
                            let _ = app.dispatch(Command::RotateSelectionFree { quat: q.to_array() });
                        }
                    }
                }
            }
            _ => {}
        },
        Tool::Scale => {
            let d = pos_sub(pointer, drag.last);
            drag.raw_factor = (drag.raw_factor * (1.0 + (d.x - d.y) * 0.01)).clamp(0.01, 100.0);
            let target = app.state.snap_scale_factor(drag.raw_factor).clamp(0.01, 100.0);
            let f = target / drag.factor;
            if f.is_finite() && (f - 1.0).abs() > f32::EPSILON {
                drag.factor = target;
                let factors = scale_gizmo::factors(drag.handle, f);
                drag.factors *= factors;
                let _ = app.dispatch(Command::ScaleSelection { factors: factors.to_array(), system });
            }
        }
        Tool::Select => {}
    }
    drag.last = pointer;
}
