//! Create > Cameras: Target / Free creation, Camera Parameters, Type switch.

use glam::Vec3;
use loom_commands::{execute, AppState, Command};
use loom_scene::{NodeId, ObjectKind, ParamValue};

fn param(s: &AppState, id: NodeId, name: &str) -> ParamValue {
    s.scene.node(id).unwrap().object.parameters().into_iter().find(|(n, _)| *n == name).unwrap_or_else(|| panic!("no {name}")).1
}

fn float(s: &AppState, id: NodeId, name: &str) -> f32 {
    match param(s, id, name) {
        ParamValue::Float(f) => f,
        other => panic!("{name}: {other:?}"),
    }
}

fn set(s: &mut AppState, id: NodeId, name: &str, value: f64) {
    execute(s, Command::SetParameter { id: id.0, name: name.into(), value }).unwrap();
}

fn forward(s: &AppState, id: NodeId) -> Vec3 {
    -s.scene.world_matrix(id).z_axis.truncate().normalize()
}

fn eye(s: &AppState, id: NodeId) -> Vec3 {
    s.scene.world_matrix(id).w_axis.truncate()
}

fn target_of(s: &AppState, id: NodeId) -> Option<NodeId> {
    match s.scene.node(id).unwrap().object {
        ObjectKind::Camera { target, .. } => target,
        _ => panic!("not a camera"),
    }
}

fn create_at(s: &mut AppState, target: bool, position: [f32; 3], look_at: [f32; 3]) -> NodeId {
    let r = execute(s, Command::CreateCameraAt { target, position, look_at }).unwrap();
    NodeId(r["id"].as_u64().unwrap())
}

#[test]
fn new_camera_has_the_reference_defaults() {
    let mut s = AppState::new();
    let cam = create_at(&mut s, false, [0.0, -100.0, 0.0], [0.0, 0.0, 0.0]);
    assert!((float(&s, cam, "Lens") - 43.456).abs() < 1e-3);
    assert!((float(&s, cam, "FOV") - 45.0).abs() < 1e-4);
    assert_eq!(param(&s, cam, "FOV Direction"), ParamValue::Int(0));
    for b in ["Orthographic Projection", "Show Cone", "Show Horizon", "Show Ranges", "Clip Manually"] {
        assert_eq!(param(&s, cam, b), ParamValue::Bool(false), "{b}");
    }
    assert_eq!(float(&s, cam, "Near Range"), 0.0);
    assert_eq!(float(&s, cam, "Far Range"), 1000.0);
    assert_eq!(float(&s, cam, "Near Clip"), 1.0);
    assert_eq!(float(&s, cam, "Far Clip"), 1000.0);
    // A free camera keeps the gap it was created with as its Target Distance.
    assert!((float(&s, cam, "Target Distance") - 100.0).abs() < 1e-3);
    let mut t = AppState::new();
    let c = create_at(&mut t, false, [0.0; 3], [0.0; 3]);
    assert_eq!(float(&t, c, "Target Distance"), 160.0);
}

#[test]
fn lens_and_fov_stay_in_sync() {
    let mut s = AppState::new();
    let cam = create_at(&mut s, false, [0.0; 3], [0.0, 10.0, 0.0]);
    set(&mut s, cam, "Lens", 50.0);
    assert!((float(&s, cam, "FOV") - 39.5978).abs() < 1e-3, "{}", float(&s, cam, "FOV"));
    set(&mut s, cam, "FOV", 90.0);
    assert!((float(&s, cam, "Lens") - 18.0).abs() < 1e-3);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!((float(&s, cam, "Lens") - 50.0).abs() < 1e-3);
    // Manual clipping never goes nearer than 0.1.
    set(&mut s, cam, "Near Clip", 0.0);
    assert!((float(&s, cam, "Near Clip") - 0.1).abs() < 1e-6);
}

#[test]
fn target_camera_aims_at_its_target_and_follows_it() {
    let mut s = AppState::new();
    let cam = create_at(&mut s, true, [0.0, -100.0, 50.0], [0.0, 0.0, 0.0]);
    let t = target_of(&s, cam).expect("target node");
    assert_eq!(s.scene.node(t).unwrap().name, "Camera001.Target");
    assert_eq!(s.selection.iter().copied().collect::<Vec<_>>(), vec![cam]);
    let want = (Vec3::ZERO - Vec3::new(0.0, -100.0, 50.0)).normalize();
    assert!(forward(&s, cam).distance(want) < 1e-4, "{}", forward(&s, cam));
    // The camera's up stays on the world-up side.
    assert!(s.scene.world_matrix(cam).y_axis.z > 0.0);
    execute(&mut s, Command::SetTransform { id: t.0, position: Some([100.0, -100.0, 50.0]), rotation_deg: None, scale: None }).unwrap();
    assert!(forward(&s, cam).distance(Vec3::X) < 1e-4, "{}", forward(&s, cam));
    assert!((loom_commands::camera::target_distance(&s.scene, cam).unwrap() - 100.0).abs() < 1e-3);
    // One undo step removes the camera and its target.
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(s.scene.node(cam).is_none() && s.scene.node(t).is_none());
}

#[test]
fn free_camera_looks_along_the_given_direction() {
    let mut s = AppState::new();
    let cam = create_at(&mut s, false, [10.0, 0.0, 0.0], [10.0, 0.0, -50.0]);
    assert!(target_of(&s, cam).is_none());
    assert!(forward(&s, cam).distance(-Vec3::Z) < 1e-4);
    assert!(eye(&s, cam).distance(Vec3::new(10.0, 0.0, 0.0)) < 1e-4);
    assert!((float(&s, cam, "Target Distance") - 50.0).abs() < 1e-3);
}

#[test]
fn type_switch_removes_or_adds_the_target() {
    let mut s = AppState::new();
    let cam = create_at(&mut s, true, [0.0, -100.0, 0.0], [30.0, 0.0, 0.0]);
    let aim = forward(&s, cam);
    let t = target_of(&s, cam).unwrap();
    execute(&mut s, Command::CameraType { id: cam.0, target: false }).unwrap();
    assert!(target_of(&s, cam).is_none());
    assert!(s.scene.node(t).is_none(), "the target goes away");
    assert!(forward(&s, cam).distance(aim) < 1e-4, "the free camera keeps its aim");
    let gap = Vec3::new(30.0, 100.0, 0.0).length();
    assert!((float(&s, cam, "Target Distance") - gap).abs() < 1e-3);
    execute(&mut s, Command::CameraType { id: cam.0, target: true }).unwrap();
    let t2 = target_of(&s, cam).unwrap();
    let at = s.scene.world_matrix(t2).w_axis.truncate();
    assert!(at.distance(Vec3::new(30.0, 0.0, 0.0)) < 1e-2, "{at}");
    assert!(forward(&s, cam).distance(aim) < 1e-4);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(target_of(&s, cam), Some(t));
    assert!(s.scene.node(t).is_some());
}

#[test]
fn camera_files_without_the_new_parameters_still_load() {
    let mut s = AppState::new();
    let cam = create_at(&mut s, false, [0.0; 3], [0.0; 3]);
    let mut v = serde_json::to_value(&s.scene.node(cam).unwrap().object).unwrap();
    let fields = v["Camera"].as_object_mut().unwrap();
    fields.retain(|k, _| k == "fov" || k == "target");
    let o: ObjectKind = serde_json::from_value(v).unwrap();
    assert_eq!(o, ObjectKind::camera_object(45.0));
}

#[test]
fn overlay_cone_reaches_the_target_in_the_render_aspect() {
    use loom_commands::camera::{overlay_lines, LineKind};
    let mut s = AppState::new();
    let cam = create_at(&mut s, true, [0.0, -100.0, 0.0], [0.0, 0.0, 0.0]);
    let lines = overlay_lines(&s.scene, cam, false);
    assert_eq!(lines.iter().filter(|l| l.2 == LineKind::TargetLine).count(), 1);
    assert!(lines.iter().all(|l| l.2 == LineKind::TargetLine), "no cone unless selected or Show Cone");
    let lines = overlay_lines(&s.scene, cam, true);
    let cone: Vec<_> = lines.iter().filter(|l| l.2 == LineKind::Cone).collect();
    assert_eq!(cone.len(), 8);
    // The far rectangle lies on the target's plane (y = 0), 4:3, 45 degrees across.
    let half_w = 100.0 * (22.5f32).to_radians().tan();
    let xs: Vec<f32> = cone.iter().flat_map(|l| [l.0, l.1]).filter(|p| p.y.abs() < 1e-3).map(|p| p.x.abs()).collect();
    assert!(!xs.is_empty() && xs.iter().all(|x| (x - half_w).abs() < 1e-2), "{xs:?}");
    let zs: Vec<f32> = cone.iter().flat_map(|l| [l.0, l.1]).filter(|p| p.y.abs() < 1e-3).map(|p| p.z.abs()).collect();
    assert!(zs.iter().all(|z| (z - half_w * 0.75).abs() < 1e-2), "{zs:?}");
    // Clip Manually adds two red rectangles with diagonals; Show Ranges two more.
    set(&mut s, cam, "Clip Manually", 1.0);
    set(&mut s, cam, "Show Ranges", 1.0);
    let lines = overlay_lines(&s.scene, cam, false);
    assert_eq!(lines.iter().filter(|l| l.2 == LineKind::Clip).count(), 12);
    assert_eq!(lines.iter().filter(|l| l.2 == LineKind::Range).count(), 8);
}
