//! Rendering > Environment and Effects: background, global lighting,
//! exposure control, atmospheres and rendering effects, each edit one undo
//! step, saved in the .loom file; Render Setup's Atmospherics / Effects.

use glam::Vec3;
use loom_commands::environment::EnvironmentTab;
use loom_commands::{execute, execute_json, AppState, Command};
use loom_scene::environment::{AtmosphereKind, EffectKind, EnvMapping, ExposureKind, LayerFalloff};
use serde_json::json;

fn ok(s: &mut AppState, cmd: &str, args: serde_json::Value) -> serde_json::Value {
    let r = execute_json(s, &json!({ "cmd": cmd, "args": args }));
    assert_eq!(r["ok"], true, "{cmd}: {r}");
    r["result"].clone()
}

fn err(s: &mut AppState, cmd: &str, args: serde_json::Value) -> String {
    let r = execute_json(s, &json!({ "cmd": cmd, "args": args }));
    assert_eq!(r["ok"], false, "{cmd} should fail: {r}");
    r["error"].as_str().unwrap_or_default().to_owned()
}

fn save_and_open(s: &mut AppState, tag: &str) -> AppState {
    let path = std::env::temp_dir().join(format!("loom-env-{tag}-{}.loom", std::process::id()));
    execute(s, Command::SaveFileAs { path: Some(path.to_string_lossy().into()) }).unwrap();
    let mut t = AppState::new();
    execute(&mut t, Command::OpenFile { path: Some(path.to_string_lossy().into()) }).unwrap();
    let _ = std::fs::remove_file(&path);
    t
}

#[test]
fn eight_opens_the_environment_panel_and_effects_the_effects_panel() {
    let mut s = AppState::new();
    // "Environment Dialog Toggle" is the shortcut table's row for 8.
    let r = ok(&mut s, "Environment Dialog Toggle", json!({}));
    assert_eq!(r["open"], true);
    assert_eq!(s.environment_ui.open, Some(EnvironmentTab::Environment));
    ok(&mut s, "Environment Dialog Toggle", json!({}));
    assert_eq!(s.environment_ui.open, None);
    ok(&mut s, "Rendering Effects", json!({}));
    assert_eq!(s.environment_ui.open, Some(EnvironmentTab::Effects));
    // 8 while the Effects panel shows switches to Environment.
    ok(&mut s, "Environment Dialog Toggle", json!({}));
    assert_eq!(s.environment_ui.open, Some(EnvironmentTab::Environment));
    assert!(!loom_commands::command::OUT_OF_SCOPE.iter().any(|(n, _)| *n == "Environment Dialog Toggle"));
    let key = loom_commands::keymap::Keymap::main_ui().lookup(loom_commands::keymap::parse_chord("8").unwrap());
    assert_eq!(key, Some("Environment Dialog Toggle"));
}

#[test]
fn background_and_global_lighting_are_undoable_and_saved() {
    let mut s = AppState::new();
    let before = s.scene.environment.clone();
    ok(&mut s, "Environment Settings", json!({ "background": [0.2, 0.4, 0.6] }));
    ok(&mut s, "Environment Settings", json!({ "tint": [1.0, 0.5, 0.5], "level": 2.0, "ambient": [0.1, 0.1, 0.1] }));
    ok(&mut s, "Environment Settings", json!({ "map": "/tmp/sky.png", "mapping": "Spherical" }));
    let e = &s.scene.environment;
    assert_eq!(e.background.get(), Vec3::new(0.2, 0.4, 0.6));
    assert_eq!((e.tint.get(), e.level.get(), e.ambient.get()), (Vec3::new(1.0, 0.5, 0.5), 2.0, Vec3::splat(0.1)));
    // Choosing a map turns Use Map on, as the reference does.
    assert_eq!((e.background_map.as_deref(), e.use_map, e.map_mapping), (Some("/tmp/sky.png"), true, EnvMapping::Spherical));
    assert_eq!(e.map(), Some("/tmp/sky.png"));
    ok(&mut s, "Environment Settings", json!({ "use_map": false }));
    assert_eq!(s.scene.environment.map(), None, "Use Map off renders the colour");
    assert!(err(&mut s, "Environment Settings", json!({ "level": -1.0 })).contains("level"));
    assert!(err(&mut s, "Environment Settings", json!({ "background": [2.0, 0.0, 0.0] })).contains("0-1"));

    let saved = s.scene.environment.clone();
    let t = save_and_open(&mut s, "bg");
    assert_eq!(t.scene.environment, saved);

    for _ in 0..4 {
        execute(&mut s, Command::UndoSceneOperation).unwrap();
    }
    assert_eq!(s.scene.environment, before);
    execute(&mut s, Command::RedoSceneOperation).unwrap();
    assert_eq!(s.scene.environment.background.get(), Vec3::new(0.2, 0.4, 0.6));
}

#[test]
fn background_colour_animates_under_auto_key() {
    let mut s = AppState::new();
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    s.current_frame = 20;
    ok(&mut s, "Environment Settings", json!({ "background": [1.0, 0.0, 0.0] }));
    let bg = &s.scene.environment.background;
    assert!(bg.is_animated(), "a change at a nonzero frame with Auto Key keys the colour");
    assert_eq!(bg.get_at(0.0), Vec3::ZERO);
    assert_eq!(bg.get_at(20.0), Vec3::new(1.0, 0.0, 0.0));
}

#[test]
fn old_files_keep_their_render_setup_background_and_ambient() {
    // Before the Environment dialog, Render Setup's Renderer panel held them.
    let mut s = AppState::new();
    let path = std::env::temp_dir().join(format!("loom-env-legacy-{}.loom", std::process::id()));
    execute(&mut s, Command::SaveFileAs { path: Some(path.to_string_lossy().into()) }).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    v["scene"].as_object_mut().unwrap().remove("environment");
    v["scene"]["render"]["bevy"]["background"] = json!([0.0, 0.0, 1.0]);
    v["scene"]["render"]["bevy"]["ambient"] = json!([0.5, 0.5, 0.5]);
    std::fs::write(&path, serde_json::to_string(&v).unwrap()).unwrap();
    let mut t = AppState::new();
    execute(&mut t, Command::OpenFile { path: Some(path.to_string_lossy().into()) }).unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(t.scene.environment.background.get(), Vec3::new(0.0, 0.0, 1.0));
    assert_eq!(t.scene.environment.ambient.get(), Vec3::splat(0.5));
}

#[test]
fn exposure_control_type_and_parameters() {
    let mut s = AppState::new();
    let r = ok(&mut s, "Exposure Control", json!({ "kind": "Linear", "brightness": 70.0, "contrast": 40.0, "exposure_value": -1.5, "process_background": true }));
    assert_eq!(r["kind"], "Linear");
    let x = &s.scene.environment.exposure;
    assert_eq!((x.kind, x.active, x.process_background), (ExposureKind::Linear, true, true));
    assert_eq!((x.brightness.get(), x.contrast.get(), x.exposure_value.get()), (70.0, 40.0, -1.5));
    ok(&mut s, "Exposure Control", json!({ "kind": "Logarithmic", "brightness": 150.0, "mid_tones": 2.0 }));
    assert_eq!(s.scene.environment.exposure.mid_tones.get(), 2.0);
    // Logarithmic brightness goes to 200; the others stop at 100.
    ok(&mut s, "Exposure Control", json!({ "kind": "Automatic" }));
    assert_eq!(s.scene.environment.exposure.brightness.get(), 100.0, "clamped into Automatic's range");
    assert!(err(&mut s, "Exposure Control", json!({ "exposure_value": 6.0 })).contains("-5"));
    assert!(err(&mut s, "Exposure Control", json!({ "mid_tones": 0.0 })).contains("0.01"));
    ok(&mut s, "Exposure Control", json!({ "active": false }));
    assert!(!s.scene.environment.exposure.active);
    let t = save_and_open(&mut s, "exposure");
    assert_eq!(t.scene.environment.exposure, s.scene.environment.exposure);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(s.scene.environment.exposure.active);
}

#[test]
fn render_preview_asks_the_renderer_for_a_thumbnail() {
    let mut s = AppState::new();
    ok(&mut s, "Exposure Render Preview", json!({}));
    let req = s.pending_render.clone().expect("a preview render request");
    assert!(req.preview, "into the Exposure Control thumbnail, not the Rendered Frame Window");
    assert!(req.settings.width <= 200 && req.settings.height <= 200);
    assert!(!s.rendered.open, "the Rendered Frame Window stays as it is");
    assert_eq!(s.rendered.last, None, "Render Last does not repeat a preview");
}

#[test]
fn atmosphere_list_add_rename_activate_move_delete() {
    let mut s = AppState::new();
    let r = ok(&mut s, "Add Atmosphere", json!({ "kind": "Fog" }));
    assert_eq!(r["index"], 0);
    ok(&mut s, "Add Atmosphere", json!({ "kind": "Volume Fog" }));
    ok(&mut s, "Add Atmosphere", json!({ "kind": "Fog" }));
    let names: Vec<String> = s.scene.environment.atmospheres.iter().map(|a| a.name.clone()).collect();
    assert_eq!(names, ["Fog", "Volume Fog", "Fog 1"]);
    assert!(err(&mut s, "Add Atmosphere", json!({ "kind": "Fire Effect" })).contains("Fog"));

    ok(&mut s, "Atmosphere Entry", json!({ "index": 2, "name": "Ground Fog", "active": false }));
    ok(&mut s, "Atmosphere Entry", json!({ "index": 2, "move_by": -2 }));
    let a = &s.scene.environment.atmospheres;
    assert_eq!((a[0].name.as_str(), a[0].active), ("Ground Fog", false));
    assert_eq!(a[1].name, "Fog");
    assert!(err(&mut s, "Atmosphere Entry", json!({ "index": 9, "active": true })).contains("no atmosphere 9"));

    ok(&mut s, "Delete Atmosphere", json!({ "index": 1 }));
    assert_eq!(s.scene.environment.atmospheres.len(), 2);
    let t = save_and_open(&mut s, "atmos");
    assert_eq!(t.scene.environment.atmospheres, s.scene.environment.atmospheres);
    for _ in 0..5 {
        execute(&mut s, Command::UndoSceneOperation).unwrap();
    }
    assert_eq!(s.scene.environment.atmospheres.len(), 1, "one undo step per edit");
}

#[test]
fn fog_volume_fog_and_volume_light_parameters() {
    let mut s = AppState::new();
    ok(&mut s, "Add Atmosphere", json!({ "kind": "Fog" }));
    ok(&mut s, "Fog Parameters", json!({ "index": 0, "color": [0.5, 0.6, 0.7], "near": 10.0, "far": 80.0, "exponential": true, "fog_background": false }));
    ok(&mut s, "Fog Parameters", json!({ "index": 0, "layered": true, "top": 30.0, "bottom": 5.0, "density": 70.0, "falloff": "Top" }));
    let AtmosphereKind::Fog(f) = &s.scene.environment.atmospheres[0].kind else { panic!("fog") };
    assert_eq!((f.color.get(), f.near.get(), f.far.get(), f.exponential, f.fog_background), (Vec3::new(0.5, 0.6, 0.7), 10.0, 80.0, true, false));
    assert_eq!((f.layered, f.top.get(), f.bottom.get(), f.density.get(), f.falloff), (true, 30.0, 5.0, 70.0, LayerFalloff::Top));
    assert!(err(&mut s, "Fog Parameters", json!({ "index": 0, "far": 120.0 })).contains("0-100"));
    assert!(err(&mut s, "Fog Parameters", json!({ "index": 0, "top": 1.0, "bottom": 2.0 })).contains("Top"));
    ok(&mut s, "Add Atmosphere", json!({ "kind": "Volume Fog" }));
    assert!(err(&mut s, "Fog Parameters", json!({ "index": 1, "near": 1.0 })).contains("not a Fog"));
    ok(&mut s, "Volume Fog Parameters", json!({ "index": 1, "density": 12.0, "color": [0.9, 0.9, 1.0], "fog_background": false }));
    let AtmosphereKind::VolumeFog(v) = &s.scene.environment.atmospheres[1].kind else { panic!("volume fog") };
    assert_eq!((v.density.get(), v.fog_background), (12.0, false));

    // Volume Light picks lights only.
    let light = ok(&mut s, "Create Light", json!({ "kind": "omni", "position": [0.0, 0.0, 50.0], "target": [0.0, 0.0, 0.0] }))["id"].as_u64().unwrap();
    let box_id = ok(&mut s, "Create Primitive", json!({ "kind": "box", "position": [0.0, 0.0, 0.0], "size": [10.0, 10.0, 10.0] }))["id"].as_u64().unwrap();
    ok(&mut s, "Add Atmosphere", json!({ "kind": "Volume Light" }));
    ok(&mut s, "Volume Light Parameters", json!({ "index": 2, "add_light": light, "density": 3.0 }));
    assert!(err(&mut s, "Volume Light Parameters", json!({ "index": 2, "add_light": box_id })).contains("not a light"));
    let AtmosphereKind::VolumeLight(l) = &s.scene.environment.atmospheres[2].kind else { panic!("volume light") };
    assert_eq!((l.lights.as_slice(), l.density.get()), (&[loom_scene::NodeId(light)][..], 3.0));
    ok(&mut s, "Volume Light Parameters", json!({ "index": 2, "remove_light": light }));
    let AtmosphereKind::VolumeLight(l) = &s.scene.environment.atmospheres[2].kind else { panic!("volume light") };
    assert!(l.lights.is_empty());
}

#[test]
fn effects_list_and_parameters() {
    let mut s = AppState::new();
    for k in ["Lens Effects", "Blur", "Brightness and Contrast", "Color Balance", "Film Grain", "Depth of Field"] {
        ok(&mut s, "Add Effect", json!({ "kind": k }));
    }
    assert_eq!(s.scene.environment.effects.len(), 6);
    assert!(err(&mut s, "Add Effect", json!({ "kind": "Motion Blur" })).contains("Blur"));
    ok(&mut s, "Glow Parameters", json!({ "index": 0, "size": 8.0, "intensity": 150.0, "use_source_color": 0.0, "inner_color": [1.0, 0.9, 0.2], "bright": 180.0 }));
    ok(&mut s, "Blur Parameters", json!({ "index": 1, "blur_type": "Directional", "u_radius": 6.0, "rotation": 45.0, "non_background": true, "blend": 30.0 }));
    ok(&mut s, "Brightness and Contrast Parameters", json!({ "index": 2, "brightness": 0.7, "contrast": 0.6, "ignore_background": true }));
    ok(&mut s, "Color Balance Parameters", json!({ "index": 3, "cyan_red": 40.0, "yellow_blue": -20.0, "preserve_luminosity": true }));
    ok(&mut s, "Film Grain Parameters", json!({ "index": 4, "grain": 1.5, "ignore_background": true }));
    let cam = ok(&mut s, "Create Camera", json!({ "target": true }))["id"].as_u64().unwrap();
    ok(&mut s, "Depth of Field Parameters", json!({ "index": 5, "focal_node": cam, "horiz_loss": 4.0, "vert_loss": 4.0, "focal_range": 30.0, "focal_limit": 90.0 }));
    assert!(err(&mut s, "Depth of Field Parameters", json!({ "index": 5, "focal_range": 100.0, "focal_limit": 50.0 })).contains("Focal Limit"));
    assert!(err(&mut s, "Brightness and Contrast Parameters", json!({ "index": 2, "brightness": 1.5 })).contains("0-1"));
    assert!(err(&mut s, "Film Grain Parameters", json!({ "index": 2, "grain": 1.0 })).contains("not a Film Grain"));
    let e = &s.scene.environment.effects;
    let EffectKind::Glow(g) = &e[0].kind else { panic!() };
    assert_eq!((g.size.get(), g.intensity.get(), g.use_source_color.get(), g.bright.get()), (8.0, 150.0, 0.0, 180.0));
    let EffectKind::Blur(b) = &e[1].kind else { panic!() };
    assert_eq!((b.blur_type, b.u_radius.get(), b.rotation.get(), b.non_background, b.blend.get()), (loom_scene::environment::BlurType::Directional, 6.0, 45.0, true, 30.0));
    let EffectKind::BrightnessContrast(bc) = &e[2].kind else { panic!() };
    assert_eq!((bc.brightness.get(), bc.contrast.get(), bc.ignore_background), (0.7, 0.6, true));
    let EffectKind::ColorBalance(cb) = &e[3].kind else { panic!() };
    assert_eq!((cb.cyan_red.get(), cb.yellow_blue.get(), cb.preserve_luminosity), (40.0, -20.0, true));
    let EffectKind::FilmGrain(fg) = &e[4].kind else { panic!() };
    assert_eq!((fg.grain.get(), fg.ignore_background), (1.5, true));
    let EffectKind::DepthOfField(d) = &e[5].kind else { panic!() };
    assert_eq!((d.focal_node, d.focal_range.get(), d.focal_limit.get()), (Some(loom_scene::NodeId(cam)), 30.0, 90.0));
    ok(&mut s, "Depth of Field Parameters", json!({ "index": 5, "focal_node": 0 }));
    let EffectKind::DepthOfField(d) = &s.scene.environment.effects[5].kind else { panic!() };
    assert_eq!(d.focal_node, None, "0 removes the focal node (Use Camera)");

    ok(&mut s, "Effect Entry", json!({ "index": 4, "active": false, "name": "Grain", "move_by": 1 }));
    assert_eq!((s.scene.environment.effects[5].name.as_str(), s.scene.environment.effects[5].active), ("Grain", false));
    ok(&mut s, "Delete Effect", json!({ "index": 0 }));
    assert_eq!(s.scene.environment.effects.len(), 5);
    let t = save_and_open(&mut s, "effects");
    assert_eq!(t.scene.environment.effects, s.scene.environment.effects);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.environment.effects.len(), 6);
}

#[test]
fn render_setup_atmospherics_and_effects_options() {
    let mut s = AppState::new();
    assert!(s.scene.render.atmospherics && s.scene.render.effects, "on by default");
    ok(&mut s, "Render SetUp...", json!({ "atmospherics": false, "effects": false }));
    assert!(!s.scene.render.atmospherics && !s.scene.render.effects);
    execute(&mut s, Command::Render { path: None, width: None, height: None }).unwrap();
    let req = s.pending_render.clone().unwrap();
    assert!(!req.settings.atmospherics && !req.settings.effects, "the render request carries them");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(s.scene.render.atmospherics && s.scene.render.effects);
}

#[test]
fn viewport_background_can_use_the_environment_background() {
    let mut s = AppState::new();
    ok(&mut s, "Set Viewport Background", json!({ "use_environment": true, "display": true }));
    let vp = s.active();
    assert!(vp.background_use_environment && vp.background_display);
    ok(&mut s, "Set Viewport Background", json!({ "use_environment": false }));
    assert!(!s.active().background_use_environment);
}
