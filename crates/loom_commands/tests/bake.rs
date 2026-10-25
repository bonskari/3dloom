//! Render To Texture: the dialog's settings, the geometry elements baked to
//! files, the lit ones handed to the renderer, Automatic Flatten UVs,
//! Projection Mapping and the baked (Shell) materials.

use loom_commands::{execute, execute_json, AppState, Command, Dialog, PrimitiveKind};
use loom_scene::bake::ElementKind;
use loom_scene::material::{MaterialKind, ShellView, Surfaces};
use loom_scene::{Modifier, NodeId};
use serde_json::json;

fn tmp_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("loom-bake-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn plane(s: &mut AppState, at: [f32; 3], size: f32) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: at, size: [size, size, 0.0] }).unwrap();
    *s.selection.iter().next().unwrap()
}

fn settings(s: &mut AppState, args: serde_json::Value) {
    let r = execute_json(s, &json!({ "cmd": "Render To Texture Settings", "args": args }));
    assert_eq!(r["ok"], true, "{r}");
}

fn element(s: &mut AppState, kind: &str, args: serde_json::Value) {
    execute(s, Command::AddTextureElement { element: kind.into() }).unwrap();
    let index = s.scene.render.bake.elements.len() - 1;
    let mut a = args.as_object().cloned().unwrap_or_default();
    a.insert("index".into(), json!(index));
    let r = execute_json(s, &json!({ "cmd": "Texture Element Settings", "args": a }));
    assert_eq!(r["ok"], true, "{r}");
}

#[test]
fn render_to_texture_opens_its_dialog_with_0() {
    let mut s = AppState::new();
    execute(&mut s, Command::Hotkey { chord: "0".into() }).unwrap();
    assert_eq!(s.dialog, Some(Dialog::RenderToTexture));
    execute(&mut s, Command::RenderToTexture).unwrap();
    assert_eq!(s.dialog, None, "the command toggles the dialog");
}

/// The NormalsMap of a flat plane is (0.5, 0.5, 1) everywhere on it, in the
/// automatic unwrap's channel 3; the Automatic Flatten UVs entry, the map
/// and the Shell material are one undo step.
#[test]
fn a_flat_planes_normals_map_is_flat() {
    let mut s = AppState::new();
    let p = plane(&mut s, [0.0; 3], 100.0);
    let dir = tmp_dir("normals");
    settings(&mut s, json!({ "output_dir": dir.to_string_lossy(), "padding": 2 }));
    element(&mut s, "NormalsMap", json!({ "width": 64, "height": 64, "format": "png", "target_slot": "Bump" }));
    let undo = s.undo.len();
    let r = execute(&mut s, Command::BakeTextures).unwrap();
    assert_eq!(r["pending"], 0, "nothing for the GPU");
    let name = s.scene.node(p).unwrap().name.clone();
    let path = dir.join(format!("{name}NormalsMap.png"));
    assert_eq!(r["written"][0], path.to_string_lossy().as_ref());
    let img = image::open(&path).expect("the map file").to_rgba8();
    assert_eq!(img.dimensions(), (64, 64));
    let inside: Vec<[u8; 4]> = img.pixels().filter(|p| p.0[3] > 0).map(|p| p.0).collect();
    assert!(inside.len() > 64 * 64 / 2, "the plane covers its map: {}", inside.len());
    for c in &inside {
        assert!(c[0].abs_diff(128) <= 1 && c[1].abs_diff(128) <= 1 && c[2] == 255, "flat normal (0.5, 0.5, 1): {c:?}");
    }
    // Use Automatic Unwrap: an Automatic Flatten UVs entry on channel 3.
    let node = s.scene.node(p).unwrap();
    let auto = node.modifiers.iter().find(|m| m.name == "Automatic Flatten UVs").expect("the unwrap entry");
    let Modifier::UnwrapUVW(u) = &auto.modifier else { panic!("an Unwrap UVW") };
    assert_eq!(u.channel.get(), 3);
    assert!(s.scene.eval_mesh(p).value.mesh.map(3).is_some());
    // Save Source (Create Shell): the baked material holds the map at its Target Map Slot.
    let shell = node.material.expect("a material");
    let MaterialKind::Shell(sh) = &s.scene.materials[&shell].kind else { panic!("a Shell material") };
    assert_eq!(sh.materials[0], None, "no source material before");
    let baked = &s.scene.materials[&sh.materials[1].unwrap()];
    assert_eq!(baked.bump_map.as_ref().map(|m| m.path.as_str()), Some(path.to_string_lossy().as_ref()));
    let Some(Surfaces::Single(surf)) = s.scene.node_surfaces_for(p, ShellView::Viewport) else { panic!("viewports show the baked material") };
    assert!(surf.bump_map.is_some());
    assert_eq!(s.undo.len(), undo + 1, "one undo step");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    let node = s.scene.node(p).unwrap();
    assert!(node.material.is_none() && node.modifiers.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn lit_elements_wait_for_the_renderer() {
    let mut s = AppState::new();
    let p = plane(&mut s, [0.0; 3], 50.0);
    let dir = tmp_dir("lit");
    settings(&mut s, json!({ "output_dir": dir.to_string_lossy() }));
    element(&mut s, "LightingMap", json!({ "width": 128, "height": 128 }));
    element(&mut s, "CompleteMap", json!({ "enabled": false }));
    let r = execute(&mut s, Command::BakeTextures).unwrap();
    assert_eq!(r["pending"], 1, "the disabled CompleteMap is left out");
    let req = s.pending_bake.clone().expect("a request for the renderer");
    assert_eq!(req.jobs.len(), 1);
    let job = &req.jobs[0];
    assert_eq!((job.node, job.channel, job.element.kind, job.element.width, job.padding), (p, 3, ElementKind::Lighting, 128, 2));
    assert!(job.path.ends_with("LightingMap.tga"), "{}", job.path);
    assert!(execute(&mut s, Command::BakeTextures).is_err(), "one bake at a time");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn output_into_source_puts_the_map_into_the_objects_material() {
    let mut s = AppState::new();
    let p = plane(&mut s, [0.0; 3], 50.0);
    execute(&mut s, Command::AssignMaterialToSelection).unwrap();
    let source = s.scene.node(p).unwrap().material.unwrap();
    let dir = tmp_dir("source");
    settings(&mut s, json!({ "output_dir": dir.to_string_lossy(), "output_into_source": true }));
    element(&mut s, "AlphaMap", json!({ "width": 32, "height": 32, "format": "png", "target_slot": "Opacity" }));
    execute(&mut s, Command::BakeTextures).unwrap();
    assert_eq!(s.scene.node(p).unwrap().material, Some(source), "no Shell");
    let path = s.scene.materials[&source].opacity_map.as_ref().expect("the map in the source").path.clone();
    let img = image::open(&path).unwrap().to_rgba8();
    assert!(img.pixels().any(|p| p.0 == [255, 255, 255, 255]), "fully opaque");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn shells_switch_views_and_clear_to_either_material() {
    let mut s = AppState::new();
    let p = plane(&mut s, [0.0; 3], 50.0);
    execute(&mut s, Command::AssignMaterialToSelection).unwrap();
    let source = s.scene.node(p).unwrap().material.unwrap();
    settings(&mut s, json!({ "create_new_baked": false }));
    element(&mut s, "DiffuseMap", json!({ "target_slot": "Diffuse Color" }));
    // Update Baked Materials: the Shell without rendering.
    execute(&mut s, Command::UpdateBakedMaterials).unwrap();
    let shell = s.scene.node(p).unwrap().material.unwrap();
    let MaterialKind::Shell(sh) = s.scene.materials[&shell].kind.clone() else { panic!("a Shell") };
    assert_eq!(sh.materials[0], Some(source));
    let baked = sh.materials[1].unwrap();
    assert_ne!(baked, source, "Duplicate Source To Baked: a copy");
    let b = &s.scene.materials[&baked];
    assert!(b.diffuse_map.as_ref().unwrap().path.ends_with("DiffuseMap.tga") && b.show_map_in_viewport);
    assert_eq!((sh.viewport, sh.render), (1, 1));
    // Original / Baked for the views.
    settings(&mut s, json!({ "views_baked": false }));
    let MaterialKind::Shell(sh) = &s.scene.materials[&shell].kind else { panic!() };
    assert_eq!((sh.viewport, sh.render), (0, 1));
    execute(&mut s, Command::ClearShellMaterials).unwrap();
    assert_eq!(s.scene.node(p).unwrap().material, Some(source), "Keep Source Materials");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    settings(&mut s, json!({ "keep_baked": true }));
    execute(&mut s, Command::ClearShellMaterials).unwrap();
    assert_eq!(s.scene.node(p).unwrap().material, Some(baked));
}

/// Pick adds a Projection modifier with the high-resolution object; the
/// HeightMap then records how far above the plane the high one is.
#[test]
fn projection_bakes_from_the_picked_object() {
    let mut s = AppState::new();
    let high = plane(&mut s, [0.0, 0.0, 2.0], 120.0);
    let low = plane(&mut s, [0.0; 3], 100.0);
    let dir = tmp_dir("projection");
    settings(&mut s, json!({ "output_dir": dir.to_string_lossy(), "render_to_files_only": true }));
    assert!(execute(&mut s, Command::PickProjectionTarget { target: low.0 }).is_err(), "not onto itself");
    execute(&mut s, Command::PickProjectionTarget { target: high.0 }).unwrap();
    let node = s.scene.node(low).unwrap();
    let Some(Modifier::Projection(d)) = node.modifiers.iter().map(|m| &m.modifier).find(|m| matches!(m, Modifier::Projection(_))) else { panic!("a Projection modifier") };
    assert_eq!(d.targets, vec![high]);
    assert!(d.push.get() > 2.0, "the cage reaches past the high object: {}", d.push.get());
    element(&mut s, "HeightMap", json!({ "width": 32, "height": 32, "format": "png" }));
    let r = execute_json(&mut s, &json!({ "cmd": "Projection Options", "args": { "min_height": 0.0, "max_height": 4.0 } }));
    assert_eq!(r["ok"], true, "{r}");
    // Without Enabled the projection is not used.
    settings(&mut s, json!({ "projection": true }));
    let r = execute(&mut s, Command::BakeTextures).unwrap();
    let o = &s.scene.render.bake.options;
    assert!((o.buffer_min - 2.0).abs() < 1e-3 && (o.buffer_max - 2.0).abs() < 1e-3, "{} {}", o.buffer_min, o.buffer_max);
    let img = image::open(r["written"][0].as_str().unwrap()).unwrap().to_rgba8();
    let c = img.get_pixel(16, 16).0;
    assert!(c[0].abs_diff(128) <= 1, "2 of 0-4 units is mid grey: {c:?}");
    assert!(s.scene.node(low).unwrap().material.is_none(), "Render to Files Only");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn unwrap_only_and_clear_unwrappers() {
    let mut s = AppState::new();
    let p = plane(&mut s, [0.0; 3], 50.0);
    settings(&mut s, json!({ "channel": 5 }));
    assert_eq!(s.scene.render.bake.auto_channel, 5);
    let r = execute(&mut s, Command::UnwrapOnly).unwrap();
    assert_eq!(r["unwrapped"], 1);
    assert!(s.scene.eval_mesh(p).value.mesh.map(5).is_some());
    assert_eq!(execute(&mut s, Command::UnwrapOnly).unwrap()["unwrapped"], 0, "once per channel");
    // All Prepared: objects with the unwrap, selected or not.
    execute(&mut s, Command::SelectNone).unwrap();
    settings(&mut s, json!({ "objects": "All Prepared" }));
    execute(&mut s, Command::ClearUnwrappers).unwrap();
    assert!(s.scene.node(p).unwrap().modifiers.is_empty());
    // Use Existing Channel: the channel spinner sets that mode's channel.
    settings(&mut s, json!({ "auto_unwrap": false, "channel": 1 }));
    assert_eq!((s.scene.render.bake.existing_channel, s.scene.render.bake.auto_channel), (1, 5));
}

#[test]
fn settings_are_checked_undoable_and_saved() {
    let mut s = AppState::new();
    for bad in [json!({ "objects": "Some" }), json!({ "channel": 0 }), json!({ "threshold_angle": 200.0 })] {
        let r = execute_json(&mut s, &json!({ "cmd": "Render To Texture Settings", "args": bad }));
        assert_eq!(r["ok"], false, "{bad}");
    }
    assert!(execute(&mut s, Command::AddTextureElement { element: "Glow".into() }).is_err());
    execute(&mut s, Command::AddTextureElement { element: "SpecularMap".into() }).unwrap();
    for bad in [json!({ "index": 0, "width": 0 }), json!({ "index": 0, "target_slot": "Nowhere" }), json!({ "index": 0, "component": "Direct Light On", "on": false }), json!({ "index": 3, "name": "x" }), json!({ "index": 0, "format": "doc" })] {
        let r = execute_json(&mut s, &json!({ "cmd": "Texture Element Settings", "args": bad }));
        assert_eq!(r["ok"], false, "{bad}");
    }
    let r = execute_json(&mut s, &json!({ "cmd": "Texture Element Settings", "args": { "index": 0, "component": "Lighting", "on": false } }));
    assert_eq!(r["ok"], true, "{r}");
    assert!(!s.scene.render.bake.elements[0].lighting);
    let undo = s.undo.len();
    settings(&mut s, json!({ "padding": 8, "skip_existing": true }));
    assert_eq!(s.undo.len(), undo + 1);
    let path = std::env::temp_dir().join(format!("loom-bake-{}.loom", std::process::id()));
    execute(&mut s, Command::SaveFileAs { path: Some(path.to_string_lossy().into()) }).unwrap();
    let mut t = AppState::new();
    execute(&mut t, Command::OpenFile { path: Some(path.to_string_lossy().into()) }).unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(t.scene.render.bake, s.scene.render.bake);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.render.bake.padding, 2);
    execute(&mut s, Command::DeleteTextureElement { index: 0 }).unwrap();
    assert!(s.scene.render.bake.elements.is_empty());
    assert!(execute(&mut s, Command::BakeTextures).is_err(), "nothing to render");
}
