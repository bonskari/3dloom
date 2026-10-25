//! Photometric lights, Sunlight / Daylight systems, Add Default Lights to
//! Scene, light viewport controls, Select Target, target renaming and the
//! Target Distance spinner of target cameras and lights.

use glam::Vec3;
use loom_commands::light::LightKind;
use loom_commands::{execute, AppState, Command, ViewKind};
use loom_scene::light::{Emitter, LightType};
use loom_scene::{NodeId, ObjectKind};

fn create(s: &mut AppState, kind: LightKind, position: [f32; 3], target: [f32; 3]) -> (NodeId, Option<NodeId>) {
    let r = execute(s, Command::CreateLight { kind, position, target }).unwrap();
    (NodeId(r["id"].as_u64().unwrap()), r["target"].as_u64().map(NodeId))
}

fn light(s: &AppState, id: NodeId) -> loom_scene::light::LightObject {
    s.scene.node(id).unwrap().object.light().expect("a light").clone()
}

fn set(s: &mut AppState, id: NodeId, name: &str, value: f64) {
    execute(s, Command::SetParameter { id: id.0, name: name.into(), value }).unwrap();
}

fn pos(s: &AppState, id: NodeId) -> Vec3 {
    s.scene.world_matrix(id).w_axis.truncate()
}

#[test]
fn photometric_target_and_free_lights_have_the_reference_names_and_defaults() {
    let mut s = AppState::new();
    let (t, target) = create(&mut s, LightKind::TargetLight, [0.0, 0.0, 100.0], [0.0; 3]);
    let (f, none) = create(&mut s, LightKind::FreeLight, [50.0, 0.0, 100.0], [50.0, 0.0, 0.0]);
    assert_eq!(s.scene.node(t).unwrap().name, "TPhotometricLight001");
    assert_eq!(s.scene.node(target.unwrap()).unwrap().name, "TPhotometricLight001.Target");
    assert_eq!(s.scene.node(f).unwrap().name, "PhotometricLight001");
    assert!(none.is_none());
    assert_eq!(s.scene.node(t).unwrap().object.stack_name(), "Target Light");
    let l = light(&s, f);
    let ph = l.photometric.as_ref().expect("photometric");
    assert_eq!((ph.candela.get(), ph.distribution.get(), ph.lamp.get()), (1500.0, 0, loom_scene::light::LAMP_D65));
    assert!(!l.shadows_on.get());
    assert!((l.target_distance.get() - 100.0).abs() < 1e-3, "a free light keeps the click's aim distance");
    // Units: the default system unit is the inch.
    assert!((ph.unit_metres - 0.0254).abs() < 1e-6);
}

#[test]
fn photometric_light_renders_with_inverse_square_from_one_metre_and_its_distribution() {
    let mut s = AppState::new();
    let (f, _) = create(&mut s, LightKind::FreeLight, [0.0, 0.0, 100.0], [0.0; 3]);
    let sample = |s: &AppState| loom_scene::light::scene_lights(&s.scene, |_| true).into_iter().find(|l| l.id == f).unwrap();
    let l = sample(&s);
    assert_eq!(l.emitter, Emitter::Point);
    assert_eq!(l.decay, 2);
    assert!((l.decay_start - 1.0 / 0.0254).abs() < 1e-3, "one metre in inches");
    // 1500 cd at 2 m is the brightness of a standard light of multiplier 1.
    let two_m = 2.0 / 0.0254;
    let at_two_metres = l.color.max_element() * l.falloff(two_m, 1.0);
    assert!((at_two_metres - 1.0).abs() < 0.02, "{at_two_metres}");
    set(&mut s, f, "Dimming On", 1.0);
    set(&mut s, f, "Dimming", 50.0);
    assert!((sample(&s).color.max_element() * sample(&s).falloff(two_m, 1.0) - 0.5).abs() < 0.02);
    set(&mut s, f, "Distribution", 2.0);
    assert_eq!(light(&s, f).light_type, LightType::Spot);
    assert_eq!(sample(&s).emitter, Emitter::Spot { hotspot: 30.0, falloff: 60.0 });
    set(&mut s, f, "Distribution", 1.0);
    assert!(matches!(sample(&s).emitter, Emitter::Spot { falloff, .. } if falloff > 170.0), "Uniform Diffuse lights a hemisphere");
    // Kelvin: a warm lamp is redder than D65.
    set(&mut s, f, "Use Kelvin", 1.0);
    set(&mut s, f, "Kelvin", 2700.0);
    let c = sample(&s).color;
    assert!(c.x > c.z * 2.0, "{c}");
    // Shape/Area Shadows: a sphere emitter is the renderers' source radius.
    set(&mut s, f, "Light Shape", 4.0);
    set(&mut s, f, "Shape Radius", 6.0);
    assert_eq!(sample(&s).source_radius, 6.0);
}

#[test]
fn templates_set_distribution_intensity_lamp_and_beam() {
    let mut s = AppState::new();
    let (f, _) = create(&mut s, LightKind::FreeLight, [0.0, 0.0, 100.0], [0.0; 3]);
    execute(&mut s, Command::PhotometricTemplate { id: f.0, index: 4 }).unwrap();
    let l = light(&s, f);
    let ph = l.photometric.as_ref().unwrap();
    assert_eq!((ph.distribution.get(), ph.candela.get()), (2, 3000.0));
    assert_eq!((l.hotspot.get(), l.falloff.get()), (25.0, 50.0));
    assert_eq!(l.light_type, LightType::Spot);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(light(&s, f).photometric.unwrap().candela.get(), 1500.0, "one undo step");
    let (omni, _) = create(&mut s, LightKind::Omni, [0.0; 3], [0.0; 3]);
    assert!(execute(&mut s, Command::PhotometricTemplate { id: omni.0, index: 0 }).is_err(), "standard lights take no template");
}

#[test]
fn photometric_lights_survive_save_and_load() {
    let mut s = AppState::new();
    let (t, _) = create(&mut s, LightKind::TargetLight, [0.0, -50.0, 100.0], [0.0; 3]);
    set(&mut s, t, "Intensity Unit", 0.0);
    set(&mut s, t, "Filter Color G", 0.5);
    let dir = std::env::temp_dir().join("loom-lightcam2-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("photometric.loom").to_string_lossy().into_owned();
    execute(&mut s, Command::SaveFile { path: Some(path.clone()) }).unwrap();
    let mut u = AppState::new();
    execute(&mut u, Command::OpenFile { path: Some(path) }).unwrap();
    assert_eq!(u.scene.node(t).unwrap(), s.scene.node(t).unwrap());
}

#[test]
fn sunlight_system_places_the_sun_by_date_time_and_location() {
    let mut s = AppState::new();
    let r = execute(&mut s, Command::CreateSunlight { daylight: false, position: [0.0; 3], radius: 50.0, orbital_scale: Some(500.0) }).unwrap();
    let (compass, sun) = (NodeId(r["compass"].as_u64().unwrap()), NodeId(r["sun"].as_u64().unwrap()));
    assert!(r["sky"].is_null());
    assert!(matches!(s.scene.node(compass).unwrap().object, ObjectKind::Compass { .. }));
    assert_eq!(s.scene.node(sun).unwrap().parent, Some(compass));
    assert_eq!(light(&s, sun).light_type, LightType::Directional);
    assert!(light(&s, sun).shadows_on.get());
    // Noon in June in San Francisco: high in the south.
    let r = execute(&mut s, Command::SunlightParameters { id: compass.0, location: Some("San Francisco, CA".into()), latitude: None, longitude: None, time_zone: None, year: Some(2010), month: Some(6), day: Some(21), hour: Some(12), minute: Some(11), dst: Some(false), north: None, orbital_scale: None }).unwrap();
    assert!(r["altitude"].as_f64().unwrap() > 70.0);
    let p = pos(&s, sun);
    assert!((p.length() - 500.0).abs() < 0.5, "orbital scale from the compass: {p}");
    assert!(p.y < 0.0 && p.z > 450.0, "south and high: {p}");
    let dir = -s.scene.world_matrix(sun).z_axis.truncate().normalize();
    assert!(dir.distance(-p.normalize()) < 1e-3, "the sun shines at the compass");
    // Morning: from the east (+X).
    execute(&mut s, Command::SunlightParameters { id: sun.0, location: None, latitude: None, longitude: None, time_zone: None, year: None, month: None, day: None, hour: Some(8), minute: Some(0), dst: None, north: None, orbital_scale: None }).unwrap();
    assert!(pos(&s, sun).x > 100.0, "{}", pos(&s, sun));
    // North Direction turns the compass and the sun with it.
    let before = pos(&s, sun);
    execute(&mut s, Command::SunlightParameters { id: sun.0, location: None, latitude: None, longitude: None, time_zone: None, year: None, month: None, day: None, hour: None, minute: None, dst: None, north: Some(90.0), orbital_scale: None }).unwrap();
    let after = pos(&s, sun);
    let want = glam::Quat::from_rotation_z(90f32.to_radians()) * before;
    assert!(after.distance(want) < 0.5, "{after} vs {want}");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(pos(&s, sun).distance(before) < 1e-3, "one undo step");
    assert!(execute(&mut s, Command::SunlightParameters { id: sun.0, location: Some("Atlantis".into()), latitude: None, longitude: None, time_zone: None, year: None, month: None, day: None, hour: None, minute: None, dst: None, north: None, orbital_scale: None }).is_err());
}

#[test]
fn daylight_system_adds_a_skylight_and_is_one_undo_step() {
    let mut s = AppState::new();
    let r = execute(&mut s, Command::CreateSunlight { daylight: true, position: [10.0, 0.0, 0.0], radius: 20.0, orbital_scale: None }).unwrap();
    let sky = NodeId(r["sky"].as_u64().unwrap());
    assert_eq!(light(&s, sky).light_type, LightType::Skylight);
    assert_eq!(s.scene.len(), 3);
    let lights = loom_scene::light::scene_lights(&s.scene, |_| true);
    assert!(lights.iter().any(|l| l.emitter == Emitter::Directional) && lights.iter().any(|l| l.emitter == Emitter::Ambient));
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.len(), 0);
}

#[test]
fn add_default_lights_to_scene_makes_the_key_and_fill_omnis() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: loom_commands::PrimitiveKind::Box, position: [0.0; 3], size: [20.0; 3] }).unwrap();
    let r = execute(&mut s, Command::AddDefaultLightsToScene { key: None, fill: None, distance_scale: None }).unwrap();
    assert_eq!(r["dialog"], "AddDefaultLights");
    execute(&mut s, Command::AddDefaultLightsToScene { key: Some(true), fill: Some(true), distance_scale: Some(2.0) }).unwrap();
    let key = s.scene.nodes().find(|n| n.name == "DefaultKeyLight").expect("key light").id;
    let fill = s.scene.nodes().find(|n| n.name == "DefaultFillLight").expect("fill light").id;
    assert_eq!(light(&s, key).light_type, LightType::Omni);
    assert!(light(&s, key).multiplier.get() > light(&s, fill).multiplier.get(), "the key is brighter than the fill");
    let centre = Vec3::new(0.0, 0.0, 10.0);
    let d = (pos(&s, key) - centre).length();
    assert!(d > 60.0, "out from the scene by its size × 2: {d}");
    assert!(s.active().scene_lights);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(s.scene.nodes().all(|n| n.name != "DefaultKeyLight"));
}

#[test]
fn light_view_controls_move_the_light_and_change_its_cone() {
    let mut s = AppState::new();
    let (spot, target) = create(&mut s, LightKind::TargetSpot, [0.0, -100.0, 0.0], [0.0; 3]);
    let t = target.unwrap();
    execute(&mut s, Command::SelectNone).unwrap();
    execute(&mut s, Command::SpotDirectionalLightView).unwrap();
    assert_eq!(s.active().kind, ViewKind::Camera(spot));
    execute(&mut s, Command::NavigateCamera { op: "dolly".into(), x: 0.5, y: None }).unwrap();
    assert!(pos(&s, spot).distance(Vec3::new(0.0, -50.0, 0.0)) < 1e-2, "Dolly Light: {}", pos(&s, spot));
    assert!(pos(&s, t).length() < 1e-3, "the target stays");
    execute(&mut s, Command::NavigateCamera { op: "truck".into(), x: 10.0, y: Some(0.0) }).unwrap();
    assert!((pos(&s, t).x - 10.0).abs() < 1e-2 && (pos(&s, spot).x - 10.0).abs() < 1e-2, "Truck moves light and target");
    execute(&mut s, Command::NavigateCamera { op: "hotspot".into(), x: 20.0, y: None }).unwrap();
    execute(&mut s, Command::NavigateCamera { op: "falloff".into(), x: 70.0, y: None }).unwrap();
    assert_eq!((light(&s, spot).hotspot.get(), light(&s, spot).falloff.get()), (20.0, 70.0));
    execute(&mut s, Command::NavigateCamera { op: "roll".into(), x: 30.0, y: None }).unwrap();
    let up = s.scene.world_matrix(spot).y_axis.truncate().normalize();
    assert!(up.z < 0.95, "Roll Light tilts the light about its axis: {up}");
    assert!(execute(&mut s, Command::NavigateCamera { op: "fov".into(), x: 30.0, y: None }).is_err());
    execute(&mut s, Command::LightHotspotMode).unwrap();
    assert_eq!(s.nav_mode, loom_input::NavMode::LightHotspot);
}

#[test]
fn renaming_a_target_camera_or_light_renames_its_target() {
    let mut s = AppState::new();
    let r = execute(&mut s, Command::CreateCameraAt { target: true, position: [0.0, -100.0, 50.0], look_at: [0.0; 3] }).unwrap();
    let (cam, ct) = (r["id"].as_u64().unwrap(), NodeId(r["target"].as_u64().unwrap()));
    execute(&mut s, Command::Rename { id: cam, name: "Hero".into() }).unwrap();
    assert_eq!(s.scene.node(ct).unwrap().name, "Hero.Target");
    let (spot, lt) = create(&mut s, LightKind::TargetLight, [0.0, 0.0, 80.0], [0.0; 3]);
    execute(&mut s, Command::Rename { id: spot.0, name: "Klieg".into() }).unwrap();
    assert_eq!(s.scene.node(lt.unwrap()).unwrap().name, "Klieg.Target");
    // Renaming the target does not rename the light.
    execute(&mut s, Command::Rename { id: lt.unwrap().0, name: "Other.Target".into() }).unwrap();
    assert_eq!(s.scene.node(spot).unwrap().name, "Klieg");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.node(lt.unwrap()).unwrap().name, "TPhotometricLight001.Target", "one undo step renames both back");
}

#[test]
fn target_distance_moves_the_target_of_target_cameras_and_lights() {
    let mut s = AppState::new();
    let r = execute(&mut s, Command::CreateCameraAt { target: true, position: [0.0, -100.0, 0.0], look_at: [0.0; 3] }).unwrap();
    let (cam, ct) = (NodeId(r["id"].as_u64().unwrap()), NodeId(r["target"].as_u64().unwrap()));
    set(&mut s, cam, "Target Distance", 40.0);
    assert!(pos(&s, ct).distance(Vec3::new(0.0, -60.0, 0.0)) < 1e-3, "{}", pos(&s, ct));
    assert!((loom_commands::camera::target_distance(&s.scene, cam).unwrap() - 40.0).abs() < 1e-3);
    let (spot, lt) = create(&mut s, LightKind::TargetSpot, [0.0, 0.0, 100.0], [0.0; 3]);
    set(&mut s, spot, "Target Distance", 25.0);
    assert!(pos(&s, lt.unwrap()).distance(Vec3::new(0.0, 0.0, 75.0)) < 1e-3);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(pos(&s, lt.unwrap()).length() < 1e-3);
}

#[test]
fn select_target_selects_the_targets_of_the_selection() {
    let mut s = AppState::new();
    let (spot, t) = create(&mut s, LightKind::TargetSpot, [0.0, 0.0, 100.0], [0.0; 3]);
    execute(&mut s, Command::SelectNodes { ids: vec![spot.0], op: loom_commands::command::SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SelectTarget).unwrap();
    assert_eq!(s.selection.iter().copied().collect::<Vec<_>>(), vec![t.unwrap()]);
    let (omni, _) = create(&mut s, LightKind::Omni, [0.0; 3], [0.0; 3]);
    let _ = omni;
    assert!(execute(&mut s, Command::SelectTarget).is_err());
}
