//! Create > Lights > Standard: creation, parameters, Light Type / Targeted,
//! undo, .loom save/load, FBX round trip, light view and the Light Lister.

use glam::Vec3;
use loom_commands::light::{LightKind, LightTypeArg};
use loom_commands::{execute, AppState, Command, Dialog, ViewKind};
use loom_scene::light::{Emitter, LightType};
use loom_scene::{NodeId, ObjectKind, ParamValue};

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

fn forward(s: &AppState, id: NodeId) -> Vec3 {
    -s.scene.world_matrix(id).z_axis.truncate().normalize()
}

fn param(s: &AppState, id: NodeId, name: &str) -> ParamValue {
    s.scene.node(id).unwrap().object.parameters().into_iter().find(|(n, _)| *n == name).unwrap_or_else(|| panic!("no {name}")).1
}

#[test]
fn every_standard_light_is_created_with_the_reference_names_and_defaults() {
    let mut s = AppState::new();
    let names: Vec<String> = LightKind::ALL
        .iter()
        .map(|k| {
            let (id, _) = create(&mut s, *k, [0.0, 0.0, 50.0], [0.0, 0.0, 0.0]);
            s.scene.node(id).unwrap().name.clone()
        })
        .collect();
    assert_eq!(names, ["Spot001", "FSpot001", "Direct001", "FDirect001", "Omni001", "Sky001"]);
    let (spot, _) = create(&mut s, LightKind::FreeSpot, [0.0; 3], [0.0, 0.0, -10.0]);
    let l = light(&s, spot);
    assert!(l.on.get() && l.shadows_on.get());
    assert_eq!((l.multiplier.get(), l.color.get()), (1.0, Vec3::ONE));
    assert_eq!((l.hotspot.get(), l.falloff.get(), l.aspect.get()), (43.0, 45.0, 1.0));
    assert_eq!((l.map_bias.get(), l.map_size.get(), l.sample_range.get()), (1.0, 512, 4.0));
    assert_eq!(param(&s, spot, "Decay Type"), ParamValue::Int(0));
    let sky = s.scene.nodes().find(|n| n.name == "Sky001").unwrap().id;
    assert!(!light(&s, sky).shadows_on.get(), "a skylight casts no shadows by default");
    // The new light is selected.
    assert!(s.selection.contains(&spot));
}

#[test]
fn a_target_spot_aims_at_its_target_node_and_follows_it() {
    let mut s = AppState::new();
    let (spot, target) = create(&mut s, LightKind::TargetSpot, [-50.0, -50.0, 80.0], [10.0, 20.0, 0.0]);
    let t = target.expect("a target node");
    assert_eq!(s.scene.node(t).unwrap().name, "Spot001.Target");
    let want = (Vec3::new(10.0, 20.0, 0.0) - Vec3::new(-50.0, -50.0, 80.0)).normalize();
    assert!(forward(&s, spot).distance(want) < 1e-3, "{}", forward(&s, spot));
    execute(&mut s, Command::SetTransform { id: t.0, position: Some([60.0, 0.0, 0.0]), rotation_deg: None, scale: None }).unwrap();
    let want = (Vec3::new(60.0, 0.0, 0.0) - Vec3::new(-50.0, -50.0, 80.0)).normalize();
    assert!(forward(&s, spot).distance(want) < 1e-3, "the spot turns with its target");
    // A free spot made by a click aims down the given direction and keeps the gap.
    let (free, none) = create(&mut s, LightKind::FreeSpot, [0.0, 0.0, 100.0], [0.0, 0.0, 0.0]);
    assert!(none.is_none());
    assert!(forward(&s, free).distance(Vec3::NEG_Z) < 1e-4);
    assert!((light(&s, free).target_distance.get() - 100.0).abs() < 1e-3);
}

#[test]
fn creating_a_target_light_is_one_undo_step() {
    let mut s = AppState::new();
    create(&mut s, LightKind::TargetDirect, [0.0, 0.0, 50.0], [0.0; 3]);
    assert_eq!(s.scene.len(), 2);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.len(), 0);
    execute(&mut s, Command::RedoSceneOperation).unwrap();
    assert_eq!(s.scene.len(), 2);
}

#[test]
fn parameters_set_clamp_and_undo() {
    let mut s = AppState::new();
    let (spot, _) = create(&mut s, LightKind::FreeSpot, [0.0; 3], [0.0, 0.0, -1.0]);
    set(&mut s, spot, "Multiplier", 2.5);
    set(&mut s, spot, "Color G", 0.25);
    set(&mut s, spot, "Hotspot", 70.0);
    let l = light(&s, spot);
    assert_eq!(l.multiplier.get(), 2.5);
    assert_eq!(l.color.get(), Vec3::new(1.0, 0.25, 1.0));
    assert_eq!((l.hotspot.get(), l.falloff.get()), (70.0, 72.0), "falloff keeps 2 degrees over the hotspot");
    for (name, v) in [("Decay Type", 2.0), ("Decay Start", 30.0), ("Far Use", 1.0), ("Far Start", 50.0), ("Far End", 300.0), ("Overshoot", 1.0), ("Shadow Type", 1.0), ("Map Size", 1024.0)] {
        set(&mut s, spot, name, v);
        assert_eq!(param(&s, spot, name).as_f64(), v, "{name}");
    }
    assert!(execute(&mut s, Command::SetParameter { id: spot.0, name: "Lens".into(), value: 1.0 }).is_err());
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(light(&s, spot).shadow_type.get(), 0, "each parameter edit is one undo step");
}

#[test]
fn light_type_and_targeted_switch_keep_the_aim() {
    let mut s = AppState::new();
    let (spot, t) = create(&mut s, LightKind::TargetSpot, [0.0, -100.0, 50.0], [0.0, 0.0, 0.0]);
    let aim = forward(&s, spot);
    execute(&mut s, Command::LightType { id: spot.0, light_type: LightTypeArg::Directional, targeted: false }).unwrap();
    let l = light(&s, spot);
    assert_eq!(l.light_type, LightType::Directional);
    assert!(l.target.is_none() && s.scene.node(t.unwrap()).is_none(), "the target node goes");
    assert!(forward(&s, spot).distance(aim) < 1e-4, "the aim stays");
    assert!((l.target_distance.get() - Vec3::new(0.0, 100.0, -50.0).length()).abs() < 1e-2);
    execute(&mut s, Command::LightType { id: spot.0, light_type: LightTypeArg::Spot, targeted: true }).unwrap();
    let l = light(&s, spot);
    let nt = l.target.expect("a new target");
    assert!(s.scene.world_matrix(nt).w_axis.truncate().distance(Vec3::ZERO) < 1e-2, "the new target sits at Target Distance");
    execute(&mut s, Command::LightType { id: spot.0, light_type: LightTypeArg::Omni, targeted: true }).unwrap();
    assert!(light(&s, spot).target.is_none(), "an omni is never targeted");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(light(&s, spot).light_type, LightType::Spot);
}

#[test]
fn scene_lights_evaluate_what_the_renderers_draw() {
    let mut s = AppState::new();
    assert!(loom_scene::light::scene_lights(&s.scene, |_| true).is_empty(), "no lights: default lighting");
    let (spot, _) = create(&mut s, LightKind::FreeSpot, [0.0, 0.0, 100.0], [0.0; 3]);
    let (omni, _) = create(&mut s, LightKind::Omni, [50.0, 0.0, 10.0], [0.0; 3]);
    create(&mut s, LightKind::Skylight, [0.0; 3], [0.0; 3]);
    set(&mut s, omni, "On", 0.0);
    set(&mut s, spot, "Multiplier", 2.0);
    let lights = loom_scene::light::scene_lights(&s.scene, |_| true);
    assert_eq!(lights.len(), 2, "an omni that is off does not light");
    let sp = lights.iter().find(|l| l.id == spot).unwrap();
    assert_eq!(sp.emitter, Emitter::Spot { hotspot: 43.0, falloff: 45.0 });
    assert!(sp.direction.distance(Vec3::NEG_Z) < 1e-4);
    assert_eq!(sp.color, Vec3::splat(2.0));
    assert!(lights.iter().any(|l| l.emitter == Emitter::Ambient), "the skylight is ambient light");
    set(&mut s, spot, "Overshoot", 1.0);
    assert_eq!(loom_scene::light::scene_lights(&s.scene, |_| true).iter().find(|l| l.id == spot).unwrap().emitter, Emitter::Point);
}

#[test]
fn exclusion_and_projector_map_are_undoable_and_survive_save_and_load() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: loom_commands::PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    let boxed = *s.selection.iter().next().unwrap();
    let (spot, target) = create(&mut s, LightKind::TargetSpot, [0.0, -80.0, 60.0], [0.0; 3]);
    execute(&mut s, Command::SetLightExclusion { id: spot.0, include: false, scope: 1, nodes: vec![boxed.0] }).unwrap();
    execute(&mut s, Command::SetLightProjectorMap { id: spot.0, path: Some("/tmp/gobo.png".into()) }).unwrap();
    set(&mut s, spot, "Far Use", 1.0);
    let l = light(&s, spot);
    assert!(l.exclusion.excludes(boxed, false) && !l.exclusion.excludes(boxed, true));
    assert!(l.projector.get());
    let dir = std::env::temp_dir().join("loom-light-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("lights.loom").to_string_lossy().into_owned();
    execute(&mut s, Command::SaveFile { path: Some(path.clone()) }).unwrap();
    let mut t = AppState::new();
    execute(&mut t, Command::OpenFile { path: Some(path) }).unwrap();
    assert_eq!(t.scene.node(spot).unwrap(), s.scene.node(spot).unwrap(), "the light loads unchanged");
    assert_eq!(light(&t, spot).target, target);
    assert!(forward(&t, spot).distance(forward(&s, spot)) < 1e-4, "the loaded target light still aims at its target");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(light(&s, spot).exclusion.nodes.is_empty());
}

#[test]
fn fbx_round_trip_keeps_light_type_colour_cone_decay_and_aim() {
    let mut s = AppState::new();
    let (spot, _) = create(&mut s, LightKind::TargetSpot, [0.0, -100.0, 80.0], [20.0, 0.0, 0.0]);
    set(&mut s, spot, "Color R", 0.5);
    set(&mut s, spot, "Multiplier", 1.5);
    set(&mut s, spot, "Falloff", 60.0);
    set(&mut s, spot, "Hotspot", 30.0);
    set(&mut s, spot, "Decay Type", 2.0);
    set(&mut s, spot, "Decay Start", 25.0);
    set(&mut s, spot, "Far Use", 1.0);
    set(&mut s, spot, "Far End", 400.0);
    let (omni, _) = create(&mut s, LightKind::Omni, [30.0, 30.0, 30.0], [0.0; 3]);
    let (direct, _) = create(&mut s, LightKind::FreeDirect, [0.0, 0.0, 100.0], [50.0, 0.0, 0.0]);
    let aims = [(spot, forward(&s, spot)), (direct, forward(&s, direct))];
    let dir = std::env::temp_dir().join("loom-light-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("lights.fbx").to_string_lossy().into_owned();
    execute(&mut s, Command::ExportFile { path: Some(path.clone()) }).unwrap();
    let mut t = AppState::new();
    execute(&mut t, Command::ImportFile { path: Some(path) }).unwrap();
    let by_name = |name: &str| t.scene.nodes().find(|n| n.name == name).unwrap_or_else(|| panic!("{name} imported")).id;
    let spot2 = by_name("Spot001");
    let l = light(&t, spot2);
    assert_eq!(l.light_type, LightType::Spot);
    assert!((l.color.get() - Vec3::new(0.5, 1.0, 1.0)).length() < 1e-4, "{}", l.color.get());
    assert!((l.multiplier.get() - 1.5).abs() < 1e-4);
    assert!((l.hotspot.get() - 30.0).abs() < 1e-3 && (l.falloff.get() - 60.0).abs() < 1e-3);
    assert_eq!(l.decay.get(), 2);
    assert!((l.decay_start.get() - 25.0).abs() < 1e-2, "{}", l.decay_start.get());
    assert!(l.far_use.get() && (l.far_end.get() - 400.0).abs() < 1e-2);
    assert_eq!(light(&t, by_name("Omni001")).light_type, LightType::Omni);
    assert_eq!(light(&t, by_name("FDirect001")).light_type, LightType::Directional);
    let _ = omni;
    for (id, aim) in aims {
        let name = s.scene.node(id).unwrap().name.clone();
        let got = forward(&t, by_name(&name));
        assert!(got.distance(aim) < 1e-3, "{name} aims {got} not {aim}");
    }
}

#[test]
fn light_view_and_light_lister() {
    let mut s = AppState::new();
    let (omni, _) = create(&mut s, LightKind::Omni, [0.0; 3], [0.0; 3]);
    let r = execute(&mut s, Command::SpotDirectionalLightView).unwrap();
    assert!(r["light"].is_null(), "an omni has no light view");
    let _ = omni;
    let (spot, _) = create(&mut s, LightKind::TargetSpot, [0.0, -50.0, 50.0], [0.0; 3]);
    execute(&mut s, Command::SelectNone).unwrap();
    execute(&mut s, Command::SpotDirectionalLightView).unwrap();
    assert_eq!(s.active().kind, ViewKind::Camera(spot), "the only spot light's view");
    execute(&mut s, Command::LightLister).unwrap();
    assert_eq!(s.dialog, Some(Dialog::LightLister));
}

#[test]
fn default_lights_count_is_a_viewport_setting() {
    let mut s = AppState::new();
    assert!(s.active().scene_lights, "viewports illuminate with scene lights");
    assert_eq!(s.active().default_lights, 1);
    execute(&mut s, Command::ViewportDefaultLights { count: 2 }).unwrap();
    assert_eq!(s.active().default_lights, 2);
    execute(&mut s, Command::ViewportLightingToggle).unwrap();
    assert!(!s.active().scene_lights);
}

#[test]
fn hide_lights_and_the_lights_selection_filter_apply_to_lights() {
    let mut s = AppState::new();
    let (omni, _) = create(&mut s, LightKind::Omni, [0.0; 3], [0.0; 3]);
    let node = s.scene.node(omni).unwrap().clone();
    assert!(loom_commands::SelectionFilter::Lights.accepts(&node));
    assert!(!loom_commands::SelectionFilter::Geometry.accepts(&node));
    execute(&mut s, Command::HideLightsToggle).unwrap();
    assert!(!s.is_visible(s.scene.node(omni).unwrap()));
    assert!(matches!(s.scene.node(omni).unwrap().object, ObjectKind::Light(_)));
}
