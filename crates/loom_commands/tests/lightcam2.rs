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
    assert!(l.shadows_on.get(), "General Parameters: Shadows On, Default=on");
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
fn a_light_that_is_off_shows_black_and_on_shows_its_colour() {
    let mut s = AppState::new();
    let (spot, _) = create(&mut s, LightKind::TargetSpot, [0.0, 0.0, 100.0], [0.0; 3]);
    let colour = |s: &AppState| s.scene.display_color(s.scene.node(spot).unwrap());
    assert_eq!(colour(&s), loom_commands::light::LIGHT_COLOR, "yellow when on");
    set(&mut s, spot, "On", 0.0);
    assert_eq!(colour(&s), [0, 0, 0], "black when off");
}

#[test]
fn shadow_parameters_and_area_shadows_reach_the_renderers() {
    let mut s = AppState::new();
    let (spot, _) = create(&mut s, LightKind::TargetSpot, [0.0, 0.0, 100.0], [0.0; 3]);
    let sample = |s: &AppState| loom_scene::light::scene_lights(&s.scene, |_| true).into_iter().find(|l| l.id == spot).unwrap();
    assert_eq!(sample(&s).shadow_transmission, Vec3::ZERO);
    set(&mut s, spot, "Shadow Density", 0.5);
    assert!((sample(&s).shadow_transmission - Vec3::splat(0.5)).length() < 1e-6);
    set(&mut s, spot, "Shadow Type", 3.0);
    set(&mut s, spot, "Area Mode", 4.0);
    set(&mut s, spot, "Area Length", 30.0);
    assert_eq!(sample(&s).source_radius, 15.0, "a Sphere Light 30 long: a 15-unit source");
    set(&mut s, spot, "Absolute Map Bias", 1.0);
    assert!(sample(&s).absolute_bias);
    for (name, v) in [("2-Sided Shadows", 0.0), ("Max Quadtree Depth", 9.0), ("Shadow Integrity", 4.0), ("Shadow Quality", 6.0), ("Shadow Spread", 2.0), ("Jitter Amount", 0.5), ("Atmosphere Shadows", 1.0)] {
        set(&mut s, spot, name, v);
        let got = s.scene.node(spot).unwrap().object.parameters().into_iter().find(|p| p.0 == name).unwrap().1.as_f64();
        assert_eq!(got, v, "{name} is kept");
    }
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    let got = s.scene.node(spot).unwrap().object.parameters().into_iter().find(|p| p.0 == "Atmosphere Shadows").unwrap().1.as_f64();
    assert_eq!(got, 0.0, "each edit is one undo step");
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

#[test]
fn rolling_a_target_camera_writes_its_animatable_roll_track() {
    use loom_scene::constraint::RotationConstraint;
    let mut s = AppState::new();
    let id = NodeId(execute(&mut s, Command::CreateCamera { target: true }).unwrap()["id"].as_u64().unwrap());
    let roll = |s: &AppState| match &s.scene.node(id).unwrap().constraints.rotation {
        Some(RotationConstraint::LookAt { roll, .. }) => roll.clone(),
        _ => panic!("a LookAt"),
    };
    execute(&mut s, Command::SelectNone).unwrap();
    execute(&mut s, Command::CameraView).unwrap();
    assert_eq!(s.active().kind, ViewKind::Camera(id));
    let tracks: Vec<String> = loom_scene::tracks::list_tracks(&s.scene, id).into_iter().map(|t| t.name).collect();
    assert!(tracks.iter().any(|n| n == "LookAt Constraint / Roll"), "{tracks:?}");
    let up0 = s.scene.world_matrix(id).y_axis.truncate();
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::SetFrame { frame: 10 }).unwrap();
    execute(&mut s, Command::NavigateCamera { op: "roll".into(), x: 30.0, y: None }).unwrap();
    let r = roll(&s);
    assert!((r.get().abs() - 30.0).abs() < 1e-2, "roll {}", r.get());
    assert!(r.is_animated(), "Auto Key keys the Roll track");
    let up = s.scene.world_matrix(id).y_axis.truncate();
    assert!((up.angle_between(up0).to_degrees() - 30.0).abs() < 0.1, "the camera turned about its line of sight");
    execute(&mut s, Command::SetFrame { frame: 0 }).unwrap();
    assert!(s.scene.world_matrix(id).y_axis.truncate().angle_between(up0) < 1e-3, "frame 0 stays upright");
}

/// A symmetric downlight web: 1000 cd at the nadir, half at 30°, none from 60° up.
const DOWNLIGHT_IES: &str = "IESNA:LM-63-2002\n[TEST] downlight\nTILT=NONE\n1 -1 1.0 4 1 1 2 0.1 0.1 0.0\n1.0 1.0 20\n0 30 60 90\n0\n1000 500 0 0\n";

#[test]
fn photometric_web_file_lights_by_the_web_and_survives_save_and_load() {
    let mut s = AppState::new();
    // Aimed down -Z from 100 units up.
    let (f, _) = create(&mut s, LightKind::FreeLight, [0.0, 0.0, 100.0], [0.0; 3]);
    let dir = std::env::temp_dir().join(format!("loom-ies-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let ies = dir.join("downlight.ies");
    std::fs::write(&ies, DOWNLIGHT_IES).unwrap();
    let r = execute(&mut s, Command::PhotometricWebFile { id: f.0, path: ies.to_string_lossy().into_owned() }).unwrap();
    assert_eq!(r["peak_candela"], 1000.0);
    let ph = light(&s, f).photometric.unwrap();
    assert_eq!((ph.distribution.get(), ph.candela.get()), (loom_scene::light::DIST_WEB, 1000.0), "Photometric Web at the web's peak");
    // Lumens: the web's flux, not a sphere's.
    set(&mut s, f, "Intensity Unit", 0.0);
    let lm = s.scene.node(f).unwrap().object.parameters().into_iter().find(|(n, _)| *n == "Intensity").unwrap().1;
    assert!(matches!(lm, loom_scene::ParamValue::Float(v) if (v - 1087.0).abs() < 10.0), "{lm:?}");
    let sample = |s: &AppState| loom_scene::light::scene_lights(&s.scene, |_| true).into_iter().find(|l| l.id == f).unwrap();
    let l = sample(&s);
    assert!(l.web.is_some());
    let down = l.falloff_towards(10.0, Vec3::NEG_Z);
    let off = l.falloff_towards(10.0, Vec3::new(30f32.to_radians().sin(), 0.0, -30f32.to_radians().cos()));
    let up = l.falloff_towards(10.0, Vec3::new(1.0, 0.0, 0.2).normalize());
    assert!((off / down - 0.5).abs() < 0.01 && up == 0.0, "{down} {off} {up}");
    let (beam, field) = l.cone().expect("the web lights a cone");
    assert!((beam - 60.0).abs() <= 1.5 && (field - 108.0).abs() <= 1.5, "beam and field for cone renderers: {beam} {field}");
    // Y Rotation 90: the nadir now points along the light's X.
    set(&mut s, f, "Y Rotation", 90.0);
    let l = sample(&s);
    assert!(l.falloff_towards(10.0, Vec3::NEG_Z) < 0.01, "turned away from straight down");
    assert!((l.direction.length() - 1.0).abs() < 1e-4 && l.direction.z.abs() < 1e-3, "{}", l.direction);
    // Saved in the scene: renders with the file gone.
    let path = dir.join("web.loom").to_string_lossy().into_owned();
    execute(&mut s, Command::SaveFile { path: Some(path.clone()) }).unwrap();
    std::fs::remove_file(&ies).unwrap();
    let mut u = AppState::new();
    execute(&mut u, Command::OpenFile { path: Some(path) }).unwrap();
    assert_eq!(u.scene.node(f).unwrap(), s.scene.node(f).unwrap());
    assert!(sample(&u).web.is_some());
    // One undo step back to Uniform Spherical; a missing file or a standard light is an error.
    let mut s2 = AppState::new();
    let (g, _) = create(&mut s2, LightKind::FreeLight, [0.0, 0.0, 100.0], [0.0; 3]);
    assert!(execute(&mut s2, Command::PhotometricWebFile { id: g.0, path: ies.to_string_lossy().into_owned() }).is_err());
    let (omni, _) = create(&mut s2, LightKind::Omni, [0.0; 3], [0.0; 3]);
    std::fs::write(&ies, DOWNLIGHT_IES).unwrap();
    assert!(execute(&mut s2, Command::PhotometricWebFile { id: omni.0, path: ies.to_string_lossy().into_owned() }).is_err());
    execute(&mut s2, Command::PhotometricWebFile { id: g.0, path: ies.to_string_lossy().into_owned() }).unwrap();
    execute(&mut s2, Command::UndoSceneOperation).unwrap();
    assert_eq!(light(&s2, g).photometric.unwrap().distribution.get(), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_luminaire_head_dims_and_filters_the_lights_linked_below_it() {
    let mut s = AppState::new();
    let r = execute(&mut s, Command::CreateHelper { kind: "luminaire".into(), position: [0.0, 0.0, 50.0], size: 0.0 }).unwrap();
    let head = NodeId(r["node"].as_u64().unwrap());
    assert_eq!(s.scene.node(head).unwrap().name, "Luminaire001");
    assert!(s.scene.node(head).unwrap().object.is_helper());
    let (omni, _) = create(&mut s, LightKind::Omni, [0.0, 0.0, 40.0], [0.0; 3]);
    let (photo, _) = create(&mut s, LightKind::FreeLight, [10.0, 0.0, 40.0], [10.0, 0.0, 0.0]);
    let (alone, _) = create(&mut s, LightKind::Omni, [50.0, 0.0, 40.0], [0.0; 3]);
    let color = |s: &AppState, id: NodeId| loom_scene::light::scene_lights(&s.scene, |_| true).into_iter().find(|l| l.id == id).unwrap().color;
    let before = (color(&s, omni), color(&s, photo), color(&s, alone));
    for l in [omni, photo] {
        execute(&mut s, Command::SetParent { id: l.0, parent: Some(head.0) }).unwrap();
    }
    assert_eq!((color(&s, omni), color(&s, photo)), (before.0, before.1), "Dimmer 100 and a white filter change nothing");
    set(&mut s, head, "Dimmer", 50.0);
    set(&mut s, head, "Filter Color G", 0.0);
    set(&mut s, head, "Filter Color B", 0.0);
    let (o, p) = (color(&s, omni), color(&s, photo));
    assert!((o.x - before.0.x * 0.5).abs() < 1e-4 && o.y == 0.0 && o.z == 0.0, "half as bright, red only: {o}");
    assert!((p.x - before.1.x * 0.5).abs() < 1e-3 && p.y == 0.0, "photometric lights too: {p}");
    assert_eq!(color(&s, alone), before.2, "a light outside the assembly keeps its light");
    // The parameters list for the Modify panel and the scene file.
    let params = s.scene.node(head).unwrap().object.parameters();
    assert!(params.iter().any(|(n, v)| *n == "Dimmer" && matches!(v, loom_scene::ParamValue::Float(x) if *x == 50.0)));
    let dir = std::env::temp_dir().join(format!("loom-luminaire-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("lum.loom").to_string_lossy().into_owned();
    execute(&mut s, Command::SaveFile { path: Some(path.clone()) }).unwrap();
    let mut u = AppState::new();
    execute(&mut u, Command::OpenFile { path: Some(path) }).unwrap();
    assert_eq!(color(&u, omni), o);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn auto_key_keys_a_camera_fov() {
    let mut s = AppState::new();
    let id = NodeId(execute(&mut s, Command::CreateCamera { target: false }).unwrap()["id"].as_u64().unwrap());
    let fov = |s: &AppState| match &s.scene.node(id).unwrap().object {
        ObjectKind::Camera { fov, .. } => fov.clone(),
        _ => panic!("a camera"),
    };
    let start = fov(&s).get();
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::SetFrame { frame: 20 }).unwrap();
    execute(&mut s, Command::SetParameter { id: id.0, name: "FOV".into(), value: 80.0 }).unwrap();
    let f = fov(&s);
    assert!(f.is_animated(), "Auto Key keys FOV");
    assert!((f.get_at(20.0) - 80.0).abs() < 1e-3 && (f.get_at(0.0) - start).abs() < 1e-3);
}

#[test]
fn walking_through_a_camera_view_with_auto_key_keys_the_camera() {
    let mut s = AppState::new();
    let id = NodeId(execute(&mut s, Command::CreateCamera { target: false }).unwrap()["id"].as_u64().unwrap());
    execute(&mut s, Command::CameraView).unwrap();
    assert_eq!(s.active().kind, ViewKind::Camera(id));
    let start = s.scene.world_matrix(id).w_axis.truncate();
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::SetFrame { frame: 10 }).unwrap();
    execute(&mut s, Command::WalkThroughStep { forward: 10.0, right: 0.0, up: 0.0 }).unwrap();
    let p = &s.scene.node(id).unwrap().transform.position;
    assert!(p.is_animated(), "the walk keyed the camera's position");
    assert!(p.get_at(10.0).distance(p.get_at(0.0)) > 1.0, "it moved between frame 0 and 10");
    assert!(p.get_at(0.0).distance(start) < 1e-3);
}
