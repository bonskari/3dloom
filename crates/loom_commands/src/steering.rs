//! Views > SteeringWheels: the wheels (View Object, Tour Building, Full
//! Navigation; big and mini) and their navigation tools on the active view.

use crate::state::AppState;
use crate::Command as C;
use glam::{Quat, Vec3};
use loom_input::camera_control::OrbitCamera;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum WheelKind {
    ViewObject,
    TourBuilding,
    #[default]
    FullNavigation,
}

impl WheelKind {
    /// The wedges, clockwise from the top.
    pub fn tools(self, mini: bool) -> &'static [&'static str] {
        match (self, mini) {
            (WheelKind::ViewObject, false) => &["Center", "Zoom", "Rewind", "Orbit"],
            (WheelKind::ViewObject, true) => &["Zoom", "Rewind", "Pan", "Orbit"],
            (WheelKind::TourBuilding, false) => &["Forward", "Look", "Rewind", "Up/Down"],
            (WheelKind::TourBuilding, true) => &["Walk", "Rewind", "Up/Down", "Look"],
            (WheelKind::FullNavigation, false) => &["Zoom", "Rewind", "Pan", "Orbit", "Center", "Walk", "Look", "Up/Down"],
            (WheelKind::FullNavigation, true) => &["Zoom", "Walk", "Rewind", "Up/Down", "Pan", "Look", "Orbit", "Center"],
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            WheelKind::ViewObject => "View Object",
            WheelKind::TourBuilding => "Tour Building",
            WheelKind::FullNavigation => "Full Navigation",
        }
    }
}

/// Viewport Configuration > SteeringWheels (kept with the preferences).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WheelPrefs {
    /// Big wheels: size 0..=2 (small, normal, large) and opacity 0..=1.
    pub big_size: u8,
    pub big_opacity: f32,
    pub mini_size: u8,
    pub mini_opacity: f32,
    pub tool_messages: bool,
    pub tool_tips: bool,
    pub pinned_on_start: bool,
    pub look_invert: bool,
    pub walk_constrain: bool,
    /// Walk Speed Factor, 0.1..=10.
    pub walk_speed: f32,
    pub incremental_zoom: bool,
    pub orbit_upright: bool,
    pub orbit_selection: bool,
}

impl Default for WheelPrefs {
    fn default() -> Self {
        // The reference gives the defaults of Pinned on Start, Look Invert,
        // Walk Constrain and Speed; size, opacity and the rest are this build's.
        Self { big_size: 1, big_opacity: 0.5, mini_size: 1, mini_opacity: 0.9, tool_messages: true, tool_tips: true, pinned_on_start: false, look_invert: false, walk_constrain: true, walk_speed: 10.0, incremental_zoom: false, orbit_upright: true, orbit_selection: false }
    }
}

/// The open wheel and its view history (Rewind).
#[derive(Debug, Clone, Default)]
pub struct WheelState {
    pub open: bool,
    pub kind: WheelKind,
    pub mini: bool,
    /// The Center tool's pivot (None: the view's own target).
    pub pivot: Option<Vec3>,
    pub history: Vec<OrbitCamera>,
    /// A drag on a camera / light view: one undo step until the release.
    pub transaction: bool,
}

pub fn handles(c: &C) -> bool {
    matches!(
        c,
        C::ToggleSteeringWheels | C::SteeringWheel { .. } | C::SteeringWheelTool { .. } | C::SteeringWheelCenter { .. } | C::SteeringWheelRewind { .. } | C::SteeringWheelRestoreCenter | C::SteeringWheelWalkSpeed { .. } | C::SetSteeringWheelPrefs { .. } | C::SteeringWheelRelease
    )
}

fn status(state: &AppState) -> Value {
    let w = &state.wheel;
    json!({ "open": w.open, "wheel": w.kind.name(), "mini": w.mini, "tools": w.kind.tools(w.mini), "history": w.history.len() })
}

const REWIND_LIMIT: usize = 64;

/// The camera or light a camera / light view looks through.
fn view_object(state: &AppState) -> Option<loom_scene::NodeId> {
    match state.active().kind {
        crate::state::ViewKind::Camera(id) if state.scene.node(id).is_some() => Some(id),
        _ => None,
    }
}

/// The active view as an orbit: a camera / light view's object, else the view's own.
fn current(state: &AppState) -> OrbitCamera {
    view_object(state).and_then(|id| crate::camera::orbit_of(&state.scene, id)).unwrap_or_else(|| state.active().camera.clone())
}

/// Puts the active view at `cam`: a camera / light view moves its object
/// (scene undo), any other view its own camera (view undo).
fn set_view(state: &mut AppState, cam: OrbitCamera, what: &str) -> Result<(), String> {
    match view_object(state) {
        Some(id) => crate::camera::place(state, id, &cam, None, true, what).map(|_| ()),
        None => {
            state.record_view_as(what);
            state.active_mut().camera = cam;
            Ok(())
        }
    }
}

pub fn execute(state: &mut AppState, cmd: C) -> Result<Value, String> {
    match cmd {
        C::ToggleSteeringWheels => {
            state.wheel.open = !state.wheel.open;
            Ok(status(state))
        }
        C::SteeringWheel { wheel } => {
            let w = wheel.trim().to_ascii_lowercase();
            let mini = w.starts_with("mini");
            let kind = match w.trim_start_matches("mini").trim().trim_end_matches("wheel").trim() {
                "view object" => WheelKind::ViewObject,
                "tour building" => WheelKind::TourBuilding,
                "full navigation" | "navigation" => WheelKind::FullNavigation,
                _ => return Err(format!("no wheel {wheel:?} (View Object, Tour Building, Full Navigation, each also Mini)")),
            };
            state.wheel.kind = kind;
            state.wheel.mini = mini;
            state.wheel.open = true;
            Ok(status(state))
        }
        C::SteeringWheelTool { tool, dx, dy, begin, click } => tool_step(state, &tool, dx, dy, begin.unwrap_or(false), click.unwrap_or(false)),
        C::SteeringWheelCenter { point } => {
            push_history(state);
            let p = Vec3::from(point);
            let cam = current(state);
            set_view(state, OrbitCamera::looking(cam.eye_position(), p, cam.up()), "Center")?;
            state.wheel.pivot = Some(p);
            Ok(json!({ "pivot": point }))
        }
        C::SteeringWheelRelease => {
            if std::mem::take(&mut state.wheel.transaction) {
                state.undo.end();
            }
            Ok(json!({}))
        }
        C::SteeringWheelRewind { steps } => {
            let n = state.wheel.history.len();
            let back = steps.max(1) as usize;
            if back > n {
                return Err("no earlier view to rewind to".into());
            }
            let cam = state.wheel.history[n - back].clone();
            state.wheel.history.truncate(n - back);
            set_view(state, cam, "Rewind")?;
            Ok(status(state))
        }
        C::SteeringWheelRestoreCenter => {
            push_history(state);
            let mut cam = current(state);
            cam.target = Vec3::ZERO;
            set_view(state, cam, "Restore Original Center")?;
            state.wheel.pivot = None;
            Ok(status(state))
        }
        C::SteeringWheelWalkSpeed { increase } => {
            let s = &mut state.prefs.wheels.walk_speed;
            *s = if increase { *s * 2.0 } else { *s * 0.5 }.clamp(0.1, 10.0);
            Ok(json!({ "walk_speed": *s }))
        }
        C::SetSteeringWheelPrefs { big_size, big_opacity, mini_size, mini_opacity, tool_messages, tool_tips, pinned_on_start, look_invert, walk_constrain, walk_speed, incremental_zoom, orbit_upright, orbit_selection } => {
            let p = &mut state.prefs.wheels;
            if let Some(v) = big_size {
                p.big_size = v.min(2) as u8;
            }
            if let Some(v) = big_opacity {
                p.big_opacity = (v / 100.0).clamp(0.0, 1.0);
            }
            if let Some(v) = mini_size {
                p.mini_size = v.min(2) as u8;
            }
            if let Some(v) = mini_opacity {
                p.mini_opacity = (v / 100.0).clamp(0.0, 1.0);
            }
            for (slot, v) in [(&mut p.tool_messages, tool_messages), (&mut p.tool_tips, tool_tips), (&mut p.pinned_on_start, pinned_on_start), (&mut p.look_invert, look_invert), (&mut p.walk_constrain, walk_constrain), (&mut p.incremental_zoom, incremental_zoom), (&mut p.orbit_upright, orbit_upright), (&mut p.orbit_selection, orbit_selection)] {
                if let Some(v) = v {
                    *slot = v;
                }
            }
            if let Some(v) = walk_speed {
                p.walk_speed = v.clamp(0.1, 10.0);
            }
            Ok(json!(state.prefs.wheels))
        }
        _ => Err("not a SteeringWheels command".into()),
    }
}

fn push_history(state: &mut AppState) {
    let cam = current(state);
    let h = &mut state.wheel.history;
    h.push(cam);
    if h.len() > REWIND_LIMIT {
        h.remove(0);
    }
}

/// Rotates `p` about `pivot`: `yaw` about world Z, then `pitch` about `axis`.
fn turn(p: Vec3, pivot: Vec3, yaw: f32, pitch: f32, axis: Vec3) -> Vec3 {
    let q = Quat::from_axis_angle(Vec3::Z, yaw) * Quat::from_axis_angle(axis.normalize_or_zero(), pitch);
    pivot + q * (p - pivot)
}

/// One step of a wheel tool on the active view; `dx` / `dy` are the drag (or,
/// for Walk, the cursor's offset from the wheel centre) in pixels, `begin`
/// marks the press (a Rewind history entry), `click` a click without drag.
fn tool_step(state: &mut AppState, tool: &str, dx: f32, dy: f32, begin: bool, click: bool) -> Result<Value, String> {
    let tool = tool.to_ascii_lowercase();
    if tool == "rewind" {
        return execute(state, C::SteeringWheelRewind { steps: 1 });
    }
    if begin || click {
        push_history(state);
        // A camera / light view: the whole drag is one scene undo step.
        if begin && view_object(state).is_some() && !state.wheel.transaction {
            state.undo.begin("SteeringWheels");
            state.wheel.transaction = true;
        }
    }
    let prefs = state.prefs.wheels;
    let sel_centre = state.selection_bounds().map(|(a, b)| (a + b) * 0.5);
    let pivot_set = state.wheel.pivot;
    let mut cam_value = current(state);
    let cam = &mut cam_value;
    let (right, up, to_eye) = cam.basis();
    let eye = cam.eye_position();
    let unit = cam.distance.max(1e-3) * 0.004;
    match tool.as_str() {
        "zoom" | "forward" => {
            let f = if click {
                if tool == "zoom" && !prefs.incremental_zoom {
                    return Ok(json!({ "distance": cam.distance }));
                }
                0.75
            } else {
                (dy * 0.01).exp()
            };
            // Forward goes toward the pivot (the Center point), Zoom toward the target.
            let toward = if tool == "forward" { pivot_set.unwrap_or(cam.target) } else { cam.target };
            let new_eye = toward + (eye - toward) * f;
            let shift = new_eye - eye;
            let target = cam.target + if tool == "forward" { shift } else { Vec3::ZERO };
            *cam = OrbitCamera::looking(new_eye, target, up);
        }
        "pan" => cam.target += (-right * dx + up * dy) * unit,
        "up/down" | "updown" => cam.target += Vec3::Z * (-dy) * unit,
        "orbit" => {
            let pivot = if prefs.orbit_selection { sel_centre.or(pivot_set) } else { pivot_set }.unwrap_or(cam.target);
            let (yaw, mut pitch) = (-dx * 0.01, dy * 0.01);
            if prefs.orbit_upright {
                let cur = to_eye.z.clamp(-1.0, 1.0).asin();
                let lim = 89f32.to_radians();
                pitch = (cur + pitch).clamp(-lim, lim) - cur;
            }
            let new_eye = turn(eye, pivot, yaw, -pitch, right);
            let new_target = turn(cam.target, pivot, yaw, -pitch, right);
            let new_up = if prefs.orbit_upright { Vec3::Z } else { Quat::from_axis_angle(Vec3::Z, yaw) * Quat::from_axis_angle(right, -pitch) * up };
            *cam = OrbitCamera::looking(new_eye, new_target, new_up);
        }
        "look" => {
            let dy = if prefs.look_invert { -dy } else { dy };
            let dir = -to_eye;
            let q = Quat::from_axis_angle(Vec3::Z, -dx * 0.005) * Quat::from_axis_angle(right, -dy * 0.005);
            let d = (q * dir).normalize_or_zero();
            if d.z.abs() < 0.999 {
                *cam = OrbitCamera::looking(eye, eye + d * cam.distance, Vec3::Z);
            }
        }
        "walk" => {
            // The cursor's offset from the centre steers: up = forward, sideways = turn.
            let fwd = if prefs.walk_constrain { Vec3::new(-to_eye.x, -to_eye.y, 0.0).normalize_or_zero() } else { -to_eye };
            let step = prefs.walk_speed * 0.0005 * cam.distance.max(1.0) * (-dy / 50.0);
            let turn_by = -dx * 0.0005;
            let d = Quat::from_axis_angle(Vec3::Z, turn_by) * -to_eye;
            let new_eye = eye + fwd * step;
            *cam = OrbitCamera::looking(new_eye, new_eye + d * cam.distance, Vec3::Z);
        }
        "center" => return Err("Center picks a point on an object (SteeringWheelCenter)".into()),
        _ => return Err(format!("no wheel tool {tool:?} (Zoom, Pan, Orbit, Center, Rewind, Forward, Look, Up/Down, Walk)")),
    }
    let (e, t) = (cam_value.eye_position(), cam_value.target);
    set_view(state, cam_value, "SteeringWheels")?;
    Ok(json!({ "eye": [e.x, e.y, e.z], "target": [t.x, t.y, t.z] }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eye(s: &AppState) -> Vec3 {
        s.active().camera.eye_position()
    }

    #[test]
    fn wheel_tools_move_the_active_view_and_rewind_restores_it() {
        let mut s = AppState::default();
        execute(&mut s, C::SteeringWheel { wheel: "Mini View Object".into() }).unwrap();
        assert!(s.wheel.open && s.wheel.mini && s.wheel.kind == WheelKind::ViewObject);
        let e0 = eye(&s);
        let d0 = s.active().camera.distance;
        execute(&mut s, C::SteeringWheelTool { tool: "Zoom".into(), dx: 0.0, dy: -50.0, begin: Some(true), click: None }).unwrap();
        assert!(s.active().camera.distance < d0, "drag up zooms in");
        let t0 = s.active().camera.target;
        execute(&mut s, C::SteeringWheelTool { tool: "Pan".into(), dx: 30.0, dy: 0.0, begin: Some(true), click: None }).unwrap();
        assert_ne!(s.active().camera.target, t0);
        execute(&mut s, C::SteeringWheelTool { tool: "Orbit".into(), dx: 40.0, dy: 0.0, begin: Some(true), click: None }).unwrap();
        let (_, up, _) = s.active().camera.basis();
        assert!(up.z > 0.0, "upright");
        execute(&mut s, C::SteeringWheelTool { tool: "Up/Down".into(), dx: 0.0, dy: -20.0, begin: Some(true), click: None }).unwrap();
        let before_look = eye(&s);
        execute(&mut s, C::SteeringWheelTool { tool: "Look".into(), dx: 30.0, dy: 0.0, begin: Some(true), click: None }).unwrap();
        assert!((eye(&s) - before_look).length() < 1e-3, "Look turns about the eye");
        execute(&mut s, C::SteeringWheelTool { tool: "Walk".into(), dx: 0.0, dy: -50.0, begin: Some(true), click: None }).unwrap();
        assert!((eye(&s) - before_look).length() > 1e-4, "Walk moves the eye");
        for _ in 0..6 {
            execute(&mut s, C::SteeringWheelRewind { steps: 1 }).unwrap();
        }
        assert!((eye(&s) - e0).length() < 1e-3, "rewound to the start");
        assert!(execute(&mut s, C::SteeringWheelRewind { steps: 1 }).is_err());
        execute(&mut s, C::SteeringWheelCenter { point: [5.0, 0.0, 0.0] }).unwrap();
        assert!((s.active().camera.target - Vec3::new(5.0, 0.0, 0.0)).length() < 1e-3);
        execute(&mut s, C::SteeringWheelWalkSpeed { increase: false }).unwrap();
        assert_eq!(s.prefs.wheels.walk_speed, 5.0);
        execute(&mut s, C::ToggleSteeringWheels).unwrap();
        assert!(!s.wheel.open);
    }

    #[test]
    fn in_a_camera_view_the_wheel_moves_the_camera_one_undo_step_per_drag() {
        let mut s = AppState::default();
        crate::execute::execute(&mut s, C::CreateCamera { target: false }).unwrap();
        let cam = *s.selection.iter().next().unwrap();
        crate::execute::execute(&mut s, C::CameraView).unwrap();
        assert_eq!(s.active().kind, crate::state::ViewKind::Camera(cam));
        let p0 = s.scene.node(cam).unwrap().transform.position.get();
        let undo0 = s.undo.labels();
        execute(&mut s, C::SteeringWheelTool { tool: "Pan".into(), dx: 30.0, dy: 0.0, begin: Some(true), click: None }).unwrap();
        execute(&mut s, C::SteeringWheelTool { tool: "Pan".into(), dx: 30.0, dy: 0.0, begin: Some(false), click: None }).unwrap();
        execute(&mut s, C::SteeringWheelRelease).unwrap();
        let p1 = s.scene.node(cam).unwrap().transform.position.get();
        assert!((p1 - p0).length() > 1e-3, "the camera object moved");
        assert_eq!(s.undo.labels().last().map(String::as_str), Some("SteeringWheels"));
        assert_eq!(s.undo.labels().len(), undo0.len() + 1, "one step");
        crate::execute::execute(&mut s, C::UndoSceneOperation).unwrap();
        assert!((s.scene.node(cam).unwrap().transform.position.get() - p0).length() < 1e-3, "one undo step for the drag");
        assert_eq!(s.undo.labels(), undo0);
    }

    #[test]
    fn in_a_light_view_the_wheel_moves_the_light() {
        let mut s = AppState::default();
        crate::execute::execute(&mut s, C::CreateLight { kind: crate::light::LightKind::FreeSpot, position: [0.0, -100.0, 80.0], target: [0.0, 0.0, 0.0] }).unwrap();
        let light = *s.selection.iter().next().unwrap();
        crate::execute::execute(&mut s, C::SpotDirectionalLightView).unwrap();
        assert_eq!(s.active().kind, crate::state::ViewKind::Camera(light));
        let p0 = s.scene.node(light).unwrap().transform.position.get();
        execute(&mut s, C::SteeringWheelTool { tool: "Up/Down".into(), dx: 0.0, dy: -40.0, begin: Some(true), click: None }).unwrap();
        execute(&mut s, C::SteeringWheelRelease).unwrap();
        let p1 = s.scene.node(light).unwrap().transform.position.get();
        assert!(p1.z > p0.z + 1e-3, "the light rose: {p0} -> {p1}");
        execute(&mut s, C::SteeringWheelRewind { steps: 1 }).unwrap();
        assert!((s.scene.node(light).unwrap().transform.position.get() - p0).length() < 1e-2, "Rewind puts it back");
    }
}
