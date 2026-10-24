use crate::event::*;
use glam::{Vec2, Vec3};

/// Orbit camera state.
///
/// Coordinate convention: Z up, camera orbits around `target`.
#[derive(Debug, Clone)]
pub struct OrbitCamera {
    pub target: Vec3,
    pub distance: f32,
    /// Horizontal angle around Z, radians.
    pub yaw: f32,
    /// Vertical angle from the XY plane, radians, in (-pi, pi]: past ±pi/2 the
    /// view has orbited over a pole and is upside down.
    pub pitch: f32,
    /// Rotation of the view about its line of sight, radians (ViewCube roll
    /// arrows, orbiting outside the trackball).
    pub roll: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            distance: 5.0,
            yaw: 0.75,
            pitch: 0.5,
            roll: 0.0,
        }
    }
}

impl OrbitCamera {
    pub fn eye_position(&self) -> Vec3 {
        let cp = self.pitch.cos();
        let dir = Vec3::new(cp * self.yaw.cos(), cp * self.yaw.sin(), self.pitch.sin());
        self.target + dir * self.distance
    }

    /// Screen right, screen up and the direction toward the eye, in world space.
    /// With no roll, right stays horizontal and up follows the pitch over the poles.
    pub fn basis(&self) -> (Vec3, Vec3, Vec3) {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        let to_eye = Vec3::new(cp * cy, cp * sy, sp);
        let right = Vec3::new(-sy, cy, 0.0);
        let up = Vec3::new(-sp * cy, -sp * sy, cp);
        let (sr, cr) = self.roll.sin_cos();
        (right * cr + up * sr, up * cr - right * sr, to_eye)
    }

    /// The orbit that looks from `eye` at `target` with `up` as screen up
    /// (as near as the line of sight allows).
    pub fn looking(eye: Vec3, target: Vec3, up: Vec3) -> Self {
        let offset = eye - target;
        let distance = offset.length().max(1e-4);
        let dir = offset / distance;
        let mut cam = OrbitCamera { target, distance, yaw: dir.y.atan2(dir.x), pitch: dir.z.clamp(-1.0, 1.0).asin(), roll: 0.0 };
        let (r0, u0, _) = cam.basis();
        cam.roll = (-up.dot(r0)).atan2(up.dot(u0));
        cam
    }

    /// The view's up vector (for a look-at matrix).
    pub fn up(&self) -> Vec3 {
        self.basis().1
    }

    /// Orbits by `dx` / `dy` radians of horizontal / vertical drag: yaw turns
    /// about world Z, pitch over the target without a limit (wrapped to (-pi, pi]).
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        // A rolled view turns the drag with it, so the scene follows the cursor.
        let (sr, cr) = self.roll.sin_cos();
        let (dx, dy) = (dx * cr + dy * sr, dy * cr - dx * sr);
        // Upside down (over a pole) horizontal drags turn the other way round.
        let flip = if self.pitch.cos() < 0.0 { -1.0 } else { 1.0 };
        self.yaw = wrap_angle(self.yaw - dx * flip);
        self.pitch = wrap_angle(self.pitch + dy);
    }
}

/// Wraps an angle into (-pi, pi].
pub fn wrap_angle(a: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    let mut a = a % TAU;
    if a > PI {
        a -= TAU;
    } else if a <= -PI {
        a += TAU;
    }
    a
}

/// Modal navigation tool (status bar buttons). `None` = default mouse map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NavMode {
    #[default]
    None,
    Zoom,
    Pan,
    Orbit,
    OrbitSelected,
    OrbitSubObject,
    ZoomRegion,
    Fov,
    /// Zoom All: wheel-style zoom applied to every viewport.
    ZoomAll,
    // Camera viewport controls: they move the camera object.
    DollyCamera,
    DollyTarget,
    DollyBoth,
    /// Perspective: FOV and dolly together.
    Perspective,
    RollCamera,
    TruckCamera,
    OrbitCamera,
    PanCamera,
}

impl NavMode {
    /// The camera viewport controls (they edit the camera node).
    pub fn is_camera(self) -> bool {
        matches!(self, NavMode::DollyCamera | NavMode::DollyTarget | NavMode::DollyBoth | NavMode::Perspective | NavMode::RollCamera | NavMode::TruckCamera | NavMode::OrbitCamera | NavMode::PanCamera)
    }
}

/// Translates InputEvents into OrbitCamera mutations with the viewport navigation map:
///
/// - Middle drag → Pan
/// - Alt + Middle drag → Orbit
/// - Ctrl + Alt + Middle drag → Zoom
/// - Alt (+ Ctrl) + Left drag → same as with Middle (Loom addition)
/// - Wheel → Zoom
/// - Left button navigates only in a modal `NavMode`.
///
/// Modifiers while dragging: Shift keeps a pan or orbit on the first axis
/// moved; Ctrl pans (and zooms) faster, Alt zooms slower in the Zoom modes.
/// In the Orbit modes a drag that starts outside the trackball rolls the
/// view, one that starts on a trackball handle orbits about that handle's
/// axis only; Angle Snap snaps the orbit to `orbit_snap` steps.
#[derive(Debug, Default)]
pub struct ViewportNavigator {
    dragging: Option<DragMode>,
    last_pos: Vec2,
    orbit_speed: f32,
    pan_speed: f32,
    zoom_speed: f32,
    mode: NavMode,
    /// The Orbit trackball (centre, radius) in the coordinates of the events.
    trackball: Option<(Vec2, f32)>,
    /// Orbit snap step in radians (Angle Snap on).
    orbit_snap: Option<f32>,
    /// Axis a Shift or handle drag is held to: 0 horizontal, 1 vertical.
    axis_lock: Option<u8>,
    /// (yaw, pitch) at the press and the drag since, for snapping the whole orbit.
    orbit_from: (f32, f32),
    orbit_acc: Vec2,
    /// A roll drag that started outside the trackball (orbits while the cursor is back inside).
    roll_outside: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragMode {
    Orbit,
    Pan,
    Zoom,
    Roll,
}

/// Pixels around a trackball handle that grab it.
const HANDLE_GRAB: f32 = 8.0;

impl ViewportNavigator {
    pub fn new() -> Self {
        Self { orbit_speed: 0.006, pan_speed: 0.0015, zoom_speed: 0.1, ..Self::default() }
    }

    pub fn set_mode(&mut self, mode: NavMode) {
        self.mode = mode;
        self.dragging = None;
    }

    pub fn mode(&self) -> NavMode {
        self.mode
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging.is_some()
    }

    /// The Orbit trackball for the next press (the viewport's centre and radius).
    pub fn set_trackball(&mut self, trackball: Option<(Vec2, f32)>) {
        self.trackball = trackball;
    }

    /// Angle Snap for orbiting: the step in radians, or None.
    pub fn set_orbit_snap(&mut self, step: Option<f32>) {
        self.orbit_snap = step.filter(|s| *s > 0.0);
    }

    fn drag_for(&self, button: MouseButton, mods: Modifiers) -> Option<DragMode> {
        let zoom_mode = matches!(self.mode, NavMode::Zoom | NavMode::Fov | NavMode::ZoomAll);
        match button {
            // In the Zoom modes Alt slows the zoom down instead of orbiting.
            MouseButton::Left if zoom_mode => Some(DragMode::Zoom),
            MouseButton::Middle | MouseButton::Left if mods.alt && mods.ctrl => Some(DragMode::Zoom),
            MouseButton::Middle | MouseButton::Left if mods.alt => Some(DragMode::Orbit),
            MouseButton::Middle => Some(DragMode::Pan),
            MouseButton::Left => match self.mode {
                NavMode::Orbit | NavMode::OrbitSelected | NavMode::OrbitSubObject | NavMode::OrbitCamera | NavMode::PanCamera => Some(DragMode::Orbit),
                NavMode::Pan | NavMode::TruckCamera => Some(DragMode::Pan),
                NavMode::Zoom | NavMode::Fov | NavMode::ZoomAll | NavMode::DollyCamera | NavMode::DollyTarget | NavMode::DollyBoth | NavMode::Perspective => Some(DragMode::Zoom),
                NavMode::RollCamera => Some(DragMode::Roll),
                NavMode::None | NavMode::ZoomRegion => None,
            },
            MouseButton::Right => None,
        }
    }

    /// Process one input event. Returns true if camera state changed.
    pub fn handle(&mut self, ev: InputEvent, cam: &mut OrbitCamera) -> bool {
        match ev {
            InputEvent::MouseDown { button, pos, mods } => {
                if let Some(mut mode) = self.drag_for(button, mods) {
                    self.axis_lock = None;
                    self.roll_outside = false;
                    let trackball = matches!(self.mode, NavMode::Orbit | NavMode::OrbitSelected | NavMode::OrbitSubObject) && button == MouseButton::Left && !mods.alt;
                    if let (true, DragMode::Orbit, Some((c, r))) = (trackball, mode, self.trackball) {
                        let handles = [(c + Vec2::new(-r, 0.0), 0), (c + Vec2::new(r, 0.0), 0), (c + Vec2::new(0.0, -r), 1), (c + Vec2::new(0.0, r), 1)];
                        if let Some((_, axis)) = handles.iter().find(|(h, _)| h.distance(pos) <= HANDLE_GRAB) {
                            self.axis_lock = Some(*axis);
                        } else if pos.distance(c) > r {
                            mode = DragMode::Roll;
                            self.roll_outside = true;
                        }
                    }
                    self.dragging = Some(mode);
                    self.last_pos = pos;
                    self.orbit_from = (cam.yaw, cam.pitch);
                    self.orbit_acc = Vec2::ZERO;
                }
                false
            }
            InputEvent::MouseUp { .. } => {
                self.dragging = None;
                self.axis_lock = None;
                self.roll_outside = false;
                false
            }
            InputEvent::MouseMove { pos, mods } => {
                let mut delta = pos - self.last_pos;
                let prev = self.last_pos;
                self.last_pos = pos;
                let Some(mut mode) = self.dragging else { return false };
                if self.roll_outside {
                    // Outside the trackball the drag spins the view; back inside it orbits.
                    let inside = self.trackball.is_some_and(|(c, r)| pos.distance(c) <= r);
                    mode = if inside { DragMode::Orbit } else { DragMode::Roll };
                    if mode == DragMode::Orbit {
                        self.orbit_from = (cam.yaw, cam.pitch);
                        self.orbit_acc = Vec2::ZERO;
                    }
                }
                if matches!(mode, DragMode::Orbit | DragMode::Pan) {
                    if mods.shift && self.axis_lock.is_none() && delta != Vec2::ZERO {
                        self.axis_lock = Some(if delta.x.abs() >= delta.y.abs() { 0 } else { 1 });
                    }
                    match self.axis_lock {
                        Some(0) => delta.y = 0.0,
                        Some(_) => delta.x = 0.0,
                        None => {}
                    }
                }
                match mode {
                    DragMode::Orbit => {
                        match self.orbit_snap {
                            Some(step) => {
                                // Snap the whole drag since the press, not each step.
                                self.orbit_acc += delta * self.orbit_speed;
                                let snap = |a: f32| (a / step).round() * step;
                                (cam.yaw, cam.pitch) = self.orbit_from;
                                cam.orbit(snap(self.orbit_acc.x), snap(self.orbit_acc.y));
                            }
                            None => cam.orbit(delta.x * self.orbit_speed, delta.y * self.orbit_speed),
                        }
                        true
                    }
                    DragMode::Roll => {
                        let turn = match self.trackball.filter(|_| self.roll_outside) {
                            // Around the trackball: the angle the cursor swept about its centre.
                            Some((c, _)) => {
                                let (a, b) = (prev - c, pos - c);
                                if a.length_squared() < 1e-6 || b.length_squared() < 1e-6 { 0.0 } else { a.perp_dot(b).atan2(a.dot(b)) }
                            }
                            // Roll Camera: a horizontal drag.
                            None => delta.x * self.orbit_speed,
                        };
                        cam.roll = wrap_angle(cam.roll - turn);
                        true
                    }
                    DragMode::Pan => {
                        let (right, up, _) = cam.basis();
                        let fast = if mods.ctrl { 4.0 } else { 1.0 };
                        let scale = cam.distance * self.pan_speed * fast;
                        cam.target += (-right * delta.x + up * delta.y) * scale;
                        true
                    }
                    DragMode::Zoom => {
                        // Drag up zooms in, drag down zooms out. In a Zoom mode Ctrl is
                        // faster and Alt slower (Ctrl+Alt is the zoom chord itself).
                        let modal = self.mode != NavMode::None;
                        let rate = match (mods.ctrl, mods.alt) {
                            (true, false) if modal => 0.04,
                            (false, true) if modal => 0.0025,
                            _ => 0.01,
                        };
                        let factor = (1.0 + delta.y * rate).max(0.1);
                        cam.distance = (cam.distance * factor).clamp(0.05, 10_000.0);
                        true
                    }
                }
            }
            InputEvent::Scroll { delta_y, .. } => {
                let factor = (1.0 - delta_y * self.zoom_speed).max(0.1);
                cam.distance = (cam.distance * factor).clamp(0.05, 10_000.0);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mods(alt: bool, ctrl: bool) -> Modifiers {
        Modifiers { alt, ctrl, shift: false }
    }

    fn drag(ctrl: &mut ViewportNavigator, cam: &mut OrbitCamera, button: MouseButton, m: Modifiers) {
        ctrl.handle(InputEvent::MouseDown { button, pos: Vec2::new(100.0, 100.0), mods: m }, cam);
        ctrl.handle(InputEvent::MouseMove { pos: Vec2::new(140.0, 120.0), mods: m }, cam);
        ctrl.handle(InputEvent::MouseUp { button, pos: Vec2::new(140.0, 120.0), mods: m }, cam);
    }

    #[test]
    fn middle_drag_pans() {
        let (mut cam, mut c) = (OrbitCamera::default(), ViewportNavigator::new());
        let (yaw, target) = (cam.yaw, cam.target);
        drag(&mut c, &mut cam, MouseButton::Middle, mods(false, false));
        assert_ne!(cam.target, target);
        assert_eq!(cam.yaw, yaw);
    }

    #[test]
    fn alt_middle_drag_orbits() {
        let (mut cam, mut c) = (OrbitCamera::default(), ViewportNavigator::new());
        let (yaw, target) = (cam.yaw, cam.target);
        drag(&mut c, &mut cam, MouseButton::Middle, mods(true, false));
        assert_ne!(cam.yaw, yaw);
        assert_eq!(cam.target, target);
    }

    #[test]
    fn ctrl_alt_middle_drag_zooms() {
        let (mut cam, mut c) = (OrbitCamera::default(), ViewportNavigator::new());
        let d = cam.distance;
        drag(&mut c, &mut cam, MouseButton::Middle, mods(true, true));
        assert_ne!(cam.distance, d);
    }

    #[test]
    fn plain_left_drag_never_navigates() {
        let (mut cam, mut c) = (OrbitCamera::default(), ViewportNavigator::new());
        let before = cam.clone();
        drag(&mut c, &mut cam, MouseButton::Left, mods(false, false));
        assert_eq!((cam.yaw, cam.pitch, cam.distance, cam.target), (before.yaw, before.pitch, before.distance, before.target));
    }

    // Alt+LMB navigates exactly like Alt+MMB.
    #[test]
    fn alt_left_drag_orbits() {
        let (mut cam, mut c) = (OrbitCamera::default(), ViewportNavigator::new());
        let (yaw, target) = (cam.yaw, cam.target);
        drag(&mut c, &mut cam, MouseButton::Left, mods(true, false));
        assert_ne!(cam.yaw, yaw);
        assert_eq!(cam.target, target);
    }

    #[test]
    fn ctrl_alt_left_drag_zooms() {
        let (mut cam, mut c) = (OrbitCamera::default(), ViewportNavigator::new());
        let (d, yaw) = (cam.distance, cam.yaw);
        drag(&mut c, &mut cam, MouseButton::Left, mods(true, true));
        assert_ne!(cam.distance, d);
        assert_eq!(cam.yaw, yaw);
    }

    #[test]
    fn left_drag_navigates_in_modal_mode() {
        let (mut cam, mut c) = (OrbitCamera::default(), ViewportNavigator::new());
        c.set_mode(NavMode::Orbit);
        let yaw = cam.yaw;
        drag(&mut c, &mut cam, MouseButton::Left, mods(false, false));
        assert_ne!(cam.yaw, yaw);
        c.set_mode(NavMode::None);
    }

    #[test]
    fn wheel_zooms_toward_target() {
        let (mut cam, mut c) = (OrbitCamera::default(), ViewportNavigator::new());
        let d = cam.distance;
        c.handle(InputEvent::Scroll { delta_y: 1.0, pos: Vec2::ZERO, mods: mods(false, false) }, &mut cam);
        assert!(cam.distance < d);
    }

    fn drag_path(c: &mut ViewportNavigator, cam: &mut OrbitCamera, button: MouseButton, m: Modifiers, points: &[(f32, f32)]) {
        let p = |i: usize| Vec2::new(points[i].0, points[i].1);
        c.handle(InputEvent::MouseDown { button, pos: p(0), mods: m }, cam);
        for i in 1..points.len() {
            c.handle(InputEvent::MouseMove { pos: p(i), mods: m }, cam);
        }
        c.handle(InputEvent::MouseUp { button, pos: p(points.len() - 1), mods: m }, cam);
    }

    /// Orbit is not stopped at the poles: dragging on past the top turns the view upside down.
    #[test]
    fn orbit_goes_over_the_pole() {
        let (mut cam, mut c) = (OrbitCamera::default(), ViewportNavigator::new());
        cam.pitch = 1.3;
        let path: Vec<(f32, f32)> = (0..=20).map(|i| (100.0, 100.0 + i as f32 * 10.0)).collect();
        drag_path(&mut c, &mut cam, MouseButton::Middle, mods(true, false), &path);
        assert!(cam.pitch > std::f32::consts::FRAC_PI_2, "pitch {}", cam.pitch);
        assert!(cam.up().z < 0.0, "over the pole the view is upside down: up {}", cam.up());
    }

    #[test]
    fn basis_is_orthonormal_and_roll_turns_it_about_the_line_of_sight() {
        let mut cam = OrbitCamera { pitch: 2.0, ..OrbitCamera::default() };
        let (r, u, e) = cam.basis();
        assert!(r.dot(u).abs() < 1e-5 && r.dot(e).abs() < 1e-5 && u.dot(e).abs() < 1e-5);
        assert!((r.cross(u) - e).length() < 1e-5, "right x up points at the eye");
        cam.roll = std::f32::consts::FRAC_PI_2;
        let (r2, u2, e2) = cam.basis();
        assert!((e2 - e).length() < 1e-5);
        assert!((r2 - u).length() < 1e-5 && (u2 + r).length() < 1e-5);
    }

    #[test]
    fn looking_rebuilds_the_orbit_from_eye_target_and_up() {
        let cam = OrbitCamera { target: Vec3::new(1.0, 2.0, 3.0), distance: 7.0, yaw: -2.1, pitch: 0.4, roll: 0.9 };
        let back = OrbitCamera::looking(cam.eye_position(), cam.target, cam.up());
        let (a, b) = (cam.basis(), back.basis());
        assert!((a.0 - b.0).length() < 1e-4 && (a.1 - b.1).length() < 1e-4 && (a.2 - b.2).length() < 1e-4);
        assert!((back.distance - 7.0).abs() < 1e-4);
    }

    #[test]
    fn shift_holds_an_orbit_to_the_first_axis_moved() {
        let (mut cam, mut c) = (OrbitCamera::default(), ViewportNavigator::new());
        let pitch = cam.pitch;
        let m = Modifiers { alt: true, ctrl: false, shift: true };
        drag_path(&mut c, &mut cam, MouseButton::Middle, m, &[(100.0, 100.0), (110.0, 102.0), (120.0, 140.0)]);
        assert_eq!(cam.pitch, pitch, "the first move was horizontal: no pitch");
        let yaw = cam.yaw;
        drag_path(&mut c, &mut cam, MouseButton::Middle, m, &[(100.0, 100.0), (101.0, 110.0), (160.0, 120.0)]);
        assert_eq!(cam.yaw, yaw, "the first move was vertical: no yaw");
    }

    #[test]
    fn shift_holds_a_pan_to_one_axis_and_ctrl_pans_faster() {
        let mut c = ViewportNavigator::new();
        let mut cam = OrbitCamera { pitch: 0.0, yaw: 0.0, ..OrbitCamera::default() };
        let m = Modifiers { alt: false, ctrl: false, shift: true };
        drag_path(&mut c, &mut cam, MouseButton::Middle, m, &[(100.0, 100.0), (110.0, 101.0), (120.0, 130.0)]);
        assert!(cam.target.z.abs() < 1e-6, "vertical part dropped: {}", cam.target);
        let slow = cam.target.length();
        let mut fast_cam = OrbitCamera { pitch: 0.0, yaw: 0.0, ..OrbitCamera::default() };
        drag_path(&mut c, &mut fast_cam, MouseButton::Middle, Modifiers { alt: false, ctrl: true, shift: true }, &[(100.0, 100.0), (110.0, 101.0), (120.0, 130.0)]);
        assert!(fast_cam.target.length() > slow * 2.0, "Ctrl: {} vs {}", fast_cam.target.length(), slow);
    }

    #[test]
    fn zoom_mode_ctrl_is_faster_and_alt_slower() {
        let zoomed = |m: Modifiers| {
            let (mut cam, mut c) = (OrbitCamera::default(), ViewportNavigator::new());
            c.set_mode(NavMode::Zoom);
            drag_path(&mut c, &mut cam, MouseButton::Left, m, &[(100.0, 100.0), (100.0, 90.0)]);
            5.0 - cam.distance
        };
        let normal = zoomed(mods(false, false));
        assert!(normal > 0.0);
        assert!(zoomed(mods(false, true)) > normal * 2.0, "Ctrl");
        let alt = zoomed(mods(true, false));
        assert!(alt > 0.0 && alt < normal * 0.5, "Alt zooms slower, it does not orbit: {alt}");
    }

    #[test]
    fn orbit_outside_the_trackball_rolls_and_a_handle_holds_the_axis() {
        let (mut cam, mut c) = (OrbitCamera::default(), ViewportNavigator::new());
        c.set_mode(NavMode::Orbit);
        c.set_trackball(Some((Vec2::new(200.0, 200.0), 100.0)));
        let (yaw, pitch) = (cam.yaw, cam.pitch);
        // A quarter turn around the centre, outside the circle.
        let path: Vec<(f32, f32)> = (0..=10).map(|i| {
            let a = i as f32 / 10.0 * std::f32::consts::FRAC_PI_2;
            (200.0 + 150.0 * a.cos(), 200.0 + 150.0 * a.sin())
        }).collect();
        drag_path(&mut c, &mut cam, MouseButton::Left, mods(false, false), &path);
        assert!((cam.roll.abs() - std::f32::consts::FRAC_PI_2).abs() < 0.05, "roll {}", cam.roll);
        assert_eq!((cam.yaw, cam.pitch), (yaw, pitch), "rolling does not orbit");
        // The right-hand handle: a drag with a vertical part only turns horizontally.
        let mut cam = OrbitCamera::default();
        let pitch = cam.pitch;
        drag_path(&mut c, &mut cam, MouseButton::Left, mods(false, false), &[(300.0, 200.0), (320.0, 240.0)]);
        assert_eq!(cam.pitch, pitch);
        assert_ne!(cam.yaw, OrbitCamera::default().yaw);
    }

    #[test]
    fn angle_snap_orbits_in_whole_steps() {
        let (mut cam, mut c) = (OrbitCamera::default(), ViewportNavigator::new());
        c.set_orbit_snap(Some(10f32.to_radians()));
        let yaw = cam.yaw;
        drag_path(&mut c, &mut cam, MouseButton::Middle, mods(true, false), &[(100.0, 100.0), (110.0, 100.0), (160.0, 100.0)]);
        let turned = (yaw - cam.yaw).to_degrees();
        assert!((turned - 20.0).abs() < 1e-3, "60 px at 0.006 rad/px = 20.6 degrees, snapped to 20: {turned}");
    }
}
