//! Raytracer Settings, Advanced Lighting (Light Tracer), the renderer's
//! motion blur / supersampling / colour range options and the objects'
//! Motion Blur: checked, one undo step each and kept in the scene file.

use loom_commands::{execute, execute_json, AppState, Command, PrimitiveKind};
use loom_scene::render::{AdvancedPlugin, ColorRange};
use loom_scene::MotionBlur;
use serde_json::json;

fn run(s: &mut AppState, cmd: &str, args: serde_json::Value) -> serde_json::Value {
    let r = execute_json(s, &json!({ "cmd": cmd, "args": args }));
    assert_eq!(r["ok"], true, "{cmd}: {r}");
    r["result"].clone()
}

fn refused(s: &mut AppState, cmd: &str, args: serde_json::Value) {
    let r = execute_json(s, &json!({ "cmd": cmd, "args": args }));
    assert_eq!(r["ok"], false, "{cmd} {args} should be refused: {r}");
}

fn saved_and_loaded(s: &mut AppState) -> AppState {
    let path = std::env::temp_dir().join(format!("loom-raytrace-{}-{:?}.loom", std::process::id(), std::thread::current().id()));
    execute(s, Command::SaveFileAs { path: Some(path.to_string_lossy().into()) }).unwrap();
    let mut t = AppState::new();
    execute(&mut t, Command::OpenFile { path: Some(path.to_string_lossy().into()) }).unwrap();
    let _ = std::fs::remove_file(&path);
    t
}

#[test]
fn raytracer_settings_are_checked_undoable_and_saved() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    let id = s.scene.nodes().next().unwrap().id.0;
    let r = run(&mut s, "Raytracer Settings", json!({ "max_depth": 3, "cutoff": 0.1, "antialias": true, "samples": 16, "blur": 5.0, "self_reflect": false, "max_depth_color": [1.0, 0.0, 1.0], "exclude": [id] }));
    assert_eq!(r["max_depth"], 3);
    let rt = &s.scene.render.raytracer;
    assert_eq!((rt.max_depth, rt.samples, rt.antialias, rt.self_reflect, rt.max_depth_color, rt.exclude.clone()), (3, 16, true, false, Some([1.0, 0.0, 1.0]), vec![id]));
    for bad in [json!({ "max_depth": 101 }), json!({ "cutoff": 2.0 }), json!({ "samples": 1 }), json!({ "blur": 50.0 }), json!({ "exclude": [9999] }), json!({ "max_depth_color": [2.0, 0.0, 0.0] })] {
        refused(&mut s, "Raytracer Settings", bad);
    }
    run(&mut s, "Raytracer Settings", json!({ "max_depth_background": true }));
    assert_eq!(s.scene.render.raytracer.max_depth_color, None);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.render.raytracer.max_depth_color, Some([1.0, 0.0, 1.0]), "each change is an undo step");
    let t = saved_and_loaded(&mut s);
    assert_eq!(t.scene.render.raytracer, s.scene.render.raytracer);
}

#[test]
fn advanced_lighting_takes_the_light_tracer_and_its_parameters() {
    let mut s = AppState::new();
    assert!(!s.scene.render.advanced.light_tracer(), "no plug-in by default");
    run(&mut s, "Advanced Lighting", json!({ "plugin": "LightTracer", "bounces": 2, "rays_per_sample": 64, "color_bleed": 0.5, "sky_lights": 2.0, "cone_angle": 60.0, "filter_size": 2.0, "color_filter": [1.0, 0.5, 0.5] }));
    let a = s.scene.render.advanced.clone();
    assert_eq!((a.plugin, a.bounces, a.rays_per_sample), (AdvancedPlugin::LightTracer, 2, 64));
    assert!(a.light_tracer());
    // Common > Use Advanced Lighting off: renders leave it out.
    run(&mut s, "Advanced Lighting", json!({ "use_advanced": false }));
    assert!(!s.scene.render.advanced.light_tracer());
    for bad in [json!({ "bounces": 11 }), json!({ "cone_angle": 20.0 }), json!({ "rays_per_sample": 0 }), json!({ "ray_bias": -1.0 })] {
        refused(&mut s, "Advanced Lighting", bad);
    }
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(s.scene.render.advanced.light_tracer());
    let t = saved_and_loaded(&mut s);
    assert_eq!(t.scene.render.advanced, s.scene.render.advanced);
}

#[test]
fn renderer_options_set_motion_blur_supersampling_and_color_range() {
    let mut s = AppState::new();
    let b = &s.scene.render.bevy;
    assert!(b.object_motion_blur && b.image_motion_blur && b.filter_maps && b.auto_reflect && b.global_supersampler, "the reference's defaults");
    run(&mut s, "Renderer Options", json!({ "object_blur_duration": 1.0, "object_blur_samples": 4, "object_blur_subdivisions": 8, "image_blur_duration": 2.0, "color_range": "Scale", "filter_maps": false, "reflect_iterations": 3 }));
    let b = s.scene.render.bevy.clone();
    assert_eq!((b.object_blur_samples, b.object_blur_subdivisions, b.color_range, b.filter_maps, b.reflect_iterations), (4, 8, ColorRange::Scale, false, 3));
    // Four of eight slices over one frame, centred on frame 10.
    assert_eq!(b.object_blur_times(10.0), vec![9.5625, 9.8125, 10.0625, 10.3125]);
    run(&mut s, "Bevy Renderer Settings", json!({ "supersampling": 3 }));
    assert_eq!(s.scene.render.bevy.samples_per_side(), 3);
    run(&mut s, "Renderer Options", json!({ "global_supersampler": false }));
    assert_eq!(s.scene.render.bevy.samples_per_side(), 1, "the factor applies only with the global supersampler");
    run(&mut s, "Renderer Options", json!({ "global_supersampler": true, "disable_all_samplers": true }));
    assert_eq!(s.scene.render.bevy.samples_per_side(), 1);
    for bad in [json!({ "object_blur_samples": 0 }), json!({ "object_blur_subdivisions": 33 }), json!({ "reflect_iterations": 0 }), json!({ "image_blur_duration": -1.0 })] {
        refused(&mut s, "Renderer Options", bad);
    }
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(!s.scene.render.bevy.disable_all_samplers);
    let t = saved_and_loaded(&mut s);
    assert_eq!(t.scene.render.bevy, s.scene.render.bevy);
}

#[test]
fn motion_blur_is_set_per_object_in_one_undo_step() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Sphere, position: [30.0, 0.0, 0.0], size: [10.0; 3] }).unwrap();
    execute(&mut s, Command::SelectAll).unwrap();
    run(&mut s, "Set Motion Blur", json!({ "kind": "Image", "multiplier": 2.5 }));
    assert!(s.scene.nodes().all(|n| n.render.motion_blur == MotionBlur::Image && n.render.motion_blur_multiplier == 2.5 && n.render.blur() == MotionBlur::Image));
    run(&mut s, "Set Motion Blur", json!({ "enabled": false }));
    assert!(s.scene.nodes().all(|n| n.render.blur() == MotionBlur::None), "Enabled off: no blur");
    refused(&mut s, "Set Motion Blur", json!({ "multiplier": -1.0 }));
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(s.scene.nodes().all(|n| n.render.blur() == MotionBlur::Image));
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(s.scene.nodes().all(|n| n.render.motion_blur == MotionBlur::None), "both objects in one step");
    run(&mut s, "Set Motion Blur", json!({ "kind": "Object" }));
    // Visible to Reflection/Refraction is a Rendering Control checkbox.
    run(&mut s, "Set Render Property", json!({ "name": "Visible to Reflection/Refraction", "value": false }));
    let t = saved_and_loaded(&mut s);
    assert!(t.scene.nodes().all(|n| n.render.motion_blur == MotionBlur::Object && !n.render.visible_to_reflection));
}

#[test]
fn reflection_maps_and_raytrace_materials_resolve_to_traced_surfaces() {
    use loom_scene::material::{Map, MapType, MaterialKind, StandardMaterial, Surface, TraceSource};
    let mut m = StandardMaterial::new("mirror");
    m.reflection_map = Some(Map::new(MapType::FlatMirror, 80.0));
    let t = Surface::from_material(&m).trace.expect("a Flat Mirror traces");
    assert_eq!((t.reflect_source, t.reflect.x, t.reflect_adds), (TraceSource::FlatMirror, 0.8, true));
    // Flat Mirror is a reflection map only; Thin Wall a refraction map only.
    let mut m = StandardMaterial::new("window");
    m.refraction_map = Some(Map::new(MapType::ThinWall, 50.0));
    m.reflection_map = Some(Map::new(MapType::ThinWall, 50.0));
    let t = Surface::from_material(&m).trace.unwrap();
    assert_eq!((t.refract_source, t.refract.x, t.reflect.x), (TraceSource::ThinWall, 0.5, 0.0));
    // A Raytrace map in Refraction with Trace Mode Reflection reflects.
    let mut m = StandardMaterial::new("rt");
    let mut map = Map::new(MapType::Raytrace, 100.0);
    map.extra.trace_mode = 1;
    m.refraction_map = Some(map);
    let t = Surface::from_material(&m).trace.unwrap();
    assert_eq!((t.reflect.x, t.refract.x), (1.0, 0.0));
    // The Raytrace material's Reflect and Transparency with its IOR.
    let mut m = StandardMaterial::new("glass");
    let rt = loom_scene::material_types::Raytrace { reflect: glam::Vec3::splat(0.2), transparency: glam::Vec3::splat(0.9), ior: 1.33, ..Default::default() };
    m.kind = MaterialKind::Raytrace(Box::new(rt));
    let t = Surface::from_material(&m).trace.unwrap();
    assert_eq!((t.reflect.x, t.refract.x, t.ior, t.reflect_adds), (0.2, 0.9, 1.33, false));
    assert!(Surface::from_material(&StandardMaterial::new("plain")).trace.is_none());
}
