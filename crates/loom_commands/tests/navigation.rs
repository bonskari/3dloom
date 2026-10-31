//! Viewport navigation: ViewCube Home and options, Save / Restore Active
//! View, Shade Selected, Create Camera From View, Select Camera, and the
//! camera viewport controls, which move the camera node with scene undo.

use glam::Vec3;
use loom_commands::file::SceneSettings;
use loom_commands::{execute, AppState, Command, Dialog, ViewKind};
use loom_scene::{NodeId, ObjectKind};

fn camera_at(s: &mut AppState, target: bool, position: [f32; 3], look_at: [f32; 3]) -> NodeId {
    let r = execute(s, Command::CreateCameraAt { target, position, look_at }).unwrap();
    NodeId(r["id"].as_u64().unwrap())
}

fn eye(s: &AppState, id: NodeId) -> Vec3 {
    s.scene.world_matrix(id).w_axis.truncate()
}

fn forward(s: &AppState, id: NodeId) -> Vec3 {
    -s.scene.world_matrix(id).z_axis.truncate().normalize()
}

fn up(s: &AppState, id: NodeId) -> Vec3 {
    s.scene.world_matrix(id).y_axis.truncate().normalize()
}

fn target_of(s: &AppState, id: NodeId) -> Option<NodeId> {
    match s.scene.node(id).unwrap().object {
        ObjectKind::Camera { target, .. } => target,
        _ => panic!("not a camera"),
    }
}

fn fov(s: &AppState, id: NodeId) -> f32 {
    match &s.scene.node(id).unwrap().object {
        ObjectKind::Camera { fov, .. } => fov.get(),
        _ => panic!("not a camera"),
    }
}

fn nav(s: &mut AppState, op: &str, x: f32, y: Option<f32>) {
    execute(s, Command::NavigateCamera { op: op.into(), x, y }).unwrap();
}

#[test]
fn viewcube_toggle_and_options() {
    let mut s = AppState::new();
    assert!(s.viewcube.show);
    execute(&mut s, Command::ViewCubeToggle).unwrap();
    assert!(!s.viewcube.show);
    execute(&mut s, Command::Hotkey { chord: "Alt+Ctrl+V".into() }).unwrap();
    assert!(s.viewcube.show, "Alt+Ctrl+V shows it again");
    execute(&mut s, Command::SetViewCubeOptions { show: None, all_views: Some(true), size: Some(3), inactive_opacity: Some(25.0), compass: Some(false), snap_to_closest: None, fit_to_view: None, animated: None, keep_upright: None, north: None }).unwrap();
    assert!(s.viewcube.all_views && !s.viewcube.compass);
    assert_eq!(s.viewcube.size, 3);
    assert!((s.viewcube.inactive_opacity - 0.25).abs() < 1e-6);
}

/// Home goes to the Perspective Home view; Set Current View as Home redefines it.
#[test]
fn home_view_is_the_default_perspective_until_set() {
    let mut s = AppState::new();
    s.active_viewport = 0; // Top
    execute(&mut s, Command::HomeView).unwrap();
    assert_eq!(s.active().kind, ViewKind::Perspective, "Home is a Perspective view");
    assert_eq!((s.active().camera.yaw, s.active().camera.pitch), loom_commands::viewport::HOME_PERSPECTIVE);
    execute(&mut s, Command::Orbit { yaw: Some(30.0), pitch: Some(20.0), distance: Some(77.0), target: Some([1.0, 2.0, 3.0]), roll: None }).unwrap();
    execute(&mut s, Command::SetCurrentViewAsHome).unwrap();
    execute(&mut s, Command::FrontView).unwrap();
    execute(&mut s, Command::HomeView).unwrap();
    let c = &s.active().camera;
    assert_eq!(s.active().kind, ViewKind::Perspective);
    assert!((c.yaw.to_degrees() - 30.0).abs() < 1e-3 && (c.distance - 77.0).abs() < 1e-4);
    assert_eq!(c.target, Vec3::new(1.0, 2.0, 3.0));
    // Saved with the scene.
    let json = serde_json::to_string(&SceneSettings::from_state(&s)).unwrap();
    let mut t = AppState::new();
    serde_json::from_str::<SceneSettings>(&json).unwrap().apply_to(&mut t);
    assert_eq!(t.home_view, s.home_view);
}

#[test]
fn save_and_restore_active_view_per_view_kind() {
    let mut s = AppState::new();
    s.active_viewport = 0; // Top
    execute(&mut s, Command::Orbit { yaw: None, pitch: None, distance: Some(42.0), target: Some([5.0, 6.0, 0.0]), roll: None }).unwrap();
    execute(&mut s, Command::SaveActiveView).unwrap();
    execute(&mut s, Command::ZoomExtents).unwrap();
    execute(&mut s, Command::Orbit { yaw: None, pitch: None, distance: Some(10.0), target: Some([0.0; 3]), roll: None }).unwrap();
    // Another Top viewport restores the Top slot; a Front one has none.
    s.active_viewport = 1;
    let front = s.active().camera.clone();
    let r = execute(&mut s, Command::RestoreActiveView);
    assert!(r.is_err(), "nothing saved for Front");
    assert_eq!(s.active().camera.distance, front.distance);
    s.active_viewport = 0;
    execute(&mut s, Command::RestoreActiveView).unwrap();
    assert_eq!(s.active().camera.distance, 42.0);
    assert_eq!(s.active().camera.target, Vec3::new(5.0, 6.0, 0.0));
    execute(&mut s, Command::UndoViewportOperation).unwrap();
    assert_eq!(s.active().camera.distance, 10.0, "Restore is a view-undo step");
    let json = serde_json::to_string(&SceneSettings::from_state(&s)).unwrap();
    let mut t = AppState::new();
    serde_json::from_str::<SceneSettings>(&json).unwrap().apply_to(&mut t);
    assert!(t.saved_views.contains_key("Top"), "saved with the scene");
}

#[test]
fn shade_selected_toggles() {
    let mut s = AppState::new();
    execute(&mut s, Command::ShadeSelectedToggle).unwrap();
    assert!(s.shade_selected);
}

/// Create Camera From View: a Target camera on the view's eye and target,
/// with the view's horizontal FOV, and the viewport now looks through it.
#[test]
fn create_camera_from_view_matches_and_switches_the_view() {
    let mut s = AppState::new();
    execute(&mut s, Command::Orbit { yaw: Some(-60.0), pitch: Some(25.0), distance: Some(200.0), target: Some([10.0, 0.0, 5.0]), roll: None }).unwrap();
    let view_eye = s.active().camera.eye_position();
    let r = execute(&mut s, Command::Hotkey { chord: "Ctrl+C".into() }).unwrap();
    let _ = r;
    let ViewKind::Camera(cam) = s.active().kind else { panic!("viewport not switched: {:?}", s.active().kind) };
    let t = target_of(&s, cam).expect("a Target camera");
    assert!((eye(&s, cam) - view_eye).length() < 1e-3);
    assert!((eye(&s, t) - Vec3::new(10.0, 0.0, 5.0)).length() < 1e-3);
    let fy = s.active().fov_y_radians;
    let fx = 2.0 * ((fy * 0.5).tan() * loom_scene::camera::RENDER_ASPECT).atan();
    assert!((fov(&s, cam) - fx.to_degrees()).abs() < 1e-3, "FOV {} vs {}", fov(&s, cam), fx.to_degrees());
    // Only from a Perspective view; with a camera selected there, that camera
    // is moved to the view (Match Camera to View) and the view stays.
    assert!(execute(&mut s, Command::CreateCameraFromView).is_err(), "not from a camera view");
    let n = s.scene.len();
    execute(&mut s, Command::PerspectiveUserView).unwrap();
    execute(&mut s, Command::Orbit { yaw: Some(10.0), pitch: Some(5.0), distance: Some(50.0), target: Some([0.0; 3]), roll: None }).unwrap();
    s.selection = [cam].into();
    let view_eye = s.active().camera.eye_position();
    execute(&mut s, Command::CreateCameraFromView).unwrap();
    assert_eq!(s.scene.len(), n, "no new camera: the selected one matched the view");
    assert!((eye(&s, cam) - view_eye).length() < 1e-3);
    assert!(eye(&s, t).length() < 1e-3);
    assert_eq!(s.active().kind, ViewKind::Perspective);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!((eye(&s, t) - Vec3::new(10.0, 0.0, 5.0)).length() < 1e-3, "one scene undo step");
}

#[test]
fn c_with_several_cameras_and_none_selected_opens_the_select_camera_list() {
    let mut s = AppState::new();
    camera_at(&mut s, false, [0.0, -100.0, 0.0], [0.0; 3]);
    camera_at(&mut s, false, [100.0, 0.0, 0.0], [0.0; 3]);
    s.selection.clear();
    execute(&mut s, Command::CameraView).unwrap();
    assert_eq!(s.dialog, Some(Dialog::SelectCamera));
    assert_eq!(s.active().kind, ViewKind::Perspective);
}

fn camera_view(s: &mut AppState, target: bool) -> NodeId {
    let id = camera_at(s, target, [0.0, -100.0, 0.0], [0.0, 0.0, 0.0]);
    execute(s, Command::ViewportView { index: s.active_viewport as u32, kind: ViewKind::Camera(id) }).unwrap();
    id
}

#[test]
fn dolly_moves_the_camera_or_target_along_the_line_of_sight() {
    let mut s = AppState::new();
    let cam = camera_view(&mut s, true);
    let t = target_of(&s, cam).unwrap();
    nav(&mut s, "dolly", 0.5, None);
    assert!((eye(&s, cam) - Vec3::new(0.0, -50.0, 0.0)).length() < 1e-3, "{}", eye(&s, cam));
    assert!(eye(&s, t).length() < 1e-3, "Dolly Camera keeps the target");
    nav(&mut s, "dolly_target", 2.0, None);
    assert!((eye(&s, t) - Vec3::new(0.0, 50.0, 0.0)).length() < 1e-3, "Dolly Target keeps the camera: {}", eye(&s, t));
    assert!((eye(&s, cam) - Vec3::new(0.0, -50.0, 0.0)).length() < 1e-3);
    nav(&mut s, "dolly_both", 0.5, None);
    assert!((eye(&s, cam) - Vec3::new(0.0, 0.0, 0.0)).length() < 1e-3, "both move by half the distance: {}", eye(&s, cam));
    assert!((eye(&s, t) - Vec3::new(0.0, 100.0, 0.0)).length() < 1e-3);
    for _ in 0..3 {
        execute(&mut s, Command::UndoSceneOperation).unwrap();
    }
    assert!((eye(&s, cam) - Vec3::new(0.0, -100.0, 0.0)).length() < 1e-3, "each is one scene undo step");
}

#[test]
fn free_camera_dolly_keeps_its_target_distance() {
    let mut s = AppState::new();
    let cam = camera_view(&mut s, false);
    nav(&mut s, "dolly", 0.5, None);
    assert!((eye(&s, cam) - Vec3::new(0.0, -50.0, 0.0)).length() < 1e-3);
    match &s.scene.node(cam).unwrap().object {
        ObjectKind::Camera { target_distance, .. } => assert!((target_distance.get() - 100.0).abs() < 1e-3, "{}", target_distance.get()),
        _ => unreachable!(),
    }
}

#[test]
fn truck_orbit_and_pan_camera() {
    let mut s = AppState::new();
    let cam = camera_view(&mut s, true);
    let t = target_of(&s, cam).unwrap();
    nav(&mut s, "truck", 10.0, Some(5.0));
    assert!((eye(&s, cam) - Vec3::new(10.0, -100.0, 5.0)).length() < 1e-3, "{}", eye(&s, cam));
    assert!((eye(&s, t) - Vec3::new(10.0, 0.0, 5.0)).length() < 1e-3, "trucking moves the target too");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    // Orbit Camera: 90 degrees about world Z around the target.
    nav(&mut s, "orbit", 90.0, None);
    let e = eye(&s, cam);
    assert!(eye(&s, t).length() < 1e-3);
    assert!((e.length() - 100.0).abs() < 1e-2 && e.y.abs() < 1e-2, "camera went round the target: {e}");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    // Pan Camera: the target swings about the camera.
    nav(&mut s, "pan", 90.0, None);
    assert!((eye(&s, cam) - Vec3::new(0.0, -100.0, 0.0)).length() < 1e-3, "the camera stays");
    let tp = eye(&s, t);
    assert!((tp.y + 100.0).abs() < 1e-2 && (tp.x.abs() - 100.0).abs() < 1e-2, "the target swung 90 degrees: {tp}");
}

#[test]
fn roll_camera_turns_free_and_target_cameras_about_the_line_of_sight() {
    for target in [false, true] {
        let mut s = AppState::new();
        let cam = camera_view(&mut s, target);
        let f = forward(&s, cam);
        nav(&mut s, "roll", 90.0, None);
        assert!((forward(&s, cam) - f).length() < 1e-3, "the line of sight stays");
        assert!(up(&s, cam).z.abs() < 1e-3, "rolled 90 degrees: up {}", up(&s, cam));
        execute(&mut s, Command::UndoSceneOperation).unwrap();
        assert!((up(&s, cam) - Vec3::Z).length() < 1e-3);
    }
}

#[test]
fn perspective_widens_fov_and_dollies_in_keeping_the_framing() {
    let mut s = AppState::new();
    let cam = camera_view(&mut s, true);
    let width = |s: &AppState| eye(s, cam).length() * (fov(s, cam).to_radians() * 0.5).tan();
    let w = width(&s);
    let f = fov(&s, cam);
    nav(&mut s, "perspective", 0.5, None);
    assert!(fov(&s, cam) > f, "FOV widened");
    assert!((eye(&s, cam).length() - 50.0).abs() < 1e-3);
    assert!((width(&s) - w).abs() < 1e-2, "the frame width at the target is kept");
    nav(&mut s, "fov", 30.0, None);
    assert!((fov(&s, cam) - 30.0).abs() < 1e-4);
}

#[test]
fn navigate_camera_needs_a_camera_view() {
    let mut s = AppState::new();
    assert!(execute(&mut s, Command::NavigateCamera { op: "dolly".into(), x: 0.5, y: None }).is_err());
}

#[test]
fn camera_modes_toggle_like_the_other_nav_modes() {
    let mut s = AppState::new();
    execute(&mut s, Command::DollyCameraMode).unwrap();
    assert_eq!(s.nav_mode, loom_commands::NavMode::DollyCamera);
    execute(&mut s, Command::DollyCameraMode).unwrap();
    assert_eq!(s.nav_mode, loom_commands::NavMode::None);
}

/// I pans the view so the point under the cursor is at the centre, keeping the zoom.
#[test]
fn pan_viewport_recentres_on_a_point() {
    let mut s = AppState::new();
    s.active_viewport = 0; // Top
    let d = s.active().camera.distance;
    execute(&mut s, Command::PanViewport { point: Some([30.0, -20.0, 5.0]) }).unwrap();
    let c = s.active().camera.clone();
    assert!((c.target.x - 30.0).abs() < 0.05 && (c.target.y + 20.0).abs() < 0.05, "{}", c.target);
    assert!(c.target.z.abs() < 0.05, "it pans, it does not dolly");
    assert_eq!(c.distance, d);
    execute(&mut s, Command::UndoViewportOperation).unwrap();
    assert!(s.active().camera.target.length() < 1e-4);
    execute(&mut s, Command::PanViewport { point: None }).unwrap();
    assert_eq!(s.nav_mode, loom_commands::NavMode::Pan);
}
