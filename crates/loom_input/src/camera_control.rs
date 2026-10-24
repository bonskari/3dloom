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
    /// Vertical angle from XY plane, radians. Clamped to (-pi/2 + eps, pi/2 - eps).
    pub pitch: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            distance: 5.0,
            yaw: 0.75,
            pitch: 0.5,
        }
    }
}

impl OrbitCamera {
    pub fn eye_position(&self) -> Vec3 {
        let cp = self.pitch.cos();
        let dir = Vec3::new(cp * self.yaw.cos(), cp * self.yaw.sin(), self.pitch.sin());
        self.target + dir * self.distance
    }
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
}

/// Translates InputEvents into OrbitCamera mutations with the viewport navigation map:
///
/// - Middle drag → Pan
/// - Alt + Middle drag → Orbit
/// - Ctrl + Alt + Middle drag → Zoom
/// - Alt (+ Ctrl) + Left drag → same as with Middle (Loom addition)
/// - Wheel → Zoom
/// - Left button navigates only in a modal `NavMode`.
#[derive(Debug, Default)]
pub struct ViewportNavigator {
    dragging: Option<DragMode>,
    last_pos: Vec2,
    orbit_speed: f32,
    pan_speed: f32,
    zoom_speed: f32,
    mode: NavMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragMode {
    Orbit,
    Pan,
    Zoom,
}

impl ViewportNavigator {
    pub fn new() -> Self {
        Self {
            dragging: None,
            last_pos: Vec2::ZERO,
            orbit_speed: 0.006,
            pan_speed: 0.0015,
            zoom_speed: 0.1,
            mode: NavMode::None,
        }
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

    fn drag_for(&self, button: MouseButton, mods: Modifiers) -> Option<DragMode> {
        match button {
            MouseButton::Middle | MouseButton::Left if mods.alt && mods.ctrl => Some(DragMode::Zoom),
            MouseButton::Middle | MouseButton::Left if mods.alt => Some(DragMode::Orbit),
            MouseButton::Middle => Some(DragMode::Pan),
            MouseButton::Left => match self.mode {
                NavMode::Orbit | NavMode::OrbitSelected | NavMode::OrbitSubObject => Some(DragMode::Orbit),
                NavMode::Pan => Some(DragMode::Pan),
                NavMode::Zoom | NavMode::Fov | NavMode::ZoomAll => Some(DragMode::Zoom),
                NavMode::None | NavMode::ZoomRegion => None,
            },
            MouseButton::Right => None,
        }
    }

    /// Process one input event. Returns true if camera state changed.
    pub fn handle(&mut self, ev: InputEvent, cam: &mut OrbitCamera) -> bool {
        match ev {
            InputEvent::MouseDown { button, pos, mods } => {
                if let Some(mode) = self.drag_for(button, mods) {
                    self.dragging = Some(mode);
                    self.last_pos = pos;
                }
                false
            }
            InputEvent::MouseUp { .. } => {
                self.dragging = None;
                false
            }
            InputEvent::MouseMove { pos, .. } => {
                let delta = pos - self.last_pos;
                self.last_pos = pos;
                match self.dragging {
                    Some(DragMode::Orbit) => {
                        cam.yaw -= delta.x * self.orbit_speed;
                        cam.pitch += delta.y * self.orbit_speed;
                        let limit = std::f32::consts::FRAC_PI_2 - 0.01;
                        cam.pitch = cam.pitch.clamp(-limit, limit);
                        true
                    }
                    Some(DragMode::Pan) => {
                        let cp = cam.pitch.cos();
                        let forward = Vec3::new(cp * cam.yaw.cos(), cp * cam.yaw.sin(), cam.pitch.sin());
                        // `forward` points target → eye; see the pan derivation in git history.
                        let right = forward.cross(Vec3::Z).normalize_or_zero();
                        let up = right.cross(forward).normalize_or_zero();
                        let scale = cam.distance * self.pan_speed;
                        cam.target += (right * delta.x + up * delta.y) * scale;
                        true
                    }
                    Some(DragMode::Zoom) => {
                        // Drag up zooms in, drag down zooms out.
                        let factor = (1.0 + delta.y * 0.01).max(0.1);
                        cam.distance = (cam.distance * factor).clamp(0.05, 10_000.0);
                        true
                    }
                    None => false,
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
}
