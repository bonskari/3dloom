//! Rendering > Render To Texture, clicked as a person does.

use super::*;
use egui::accesskit::Role;

fn plane(h: &mut Harness) -> loom_scene::NodeId {
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0; 3], size: [50.0, 50.0, 0.0] });
    *h.app.state.selection.iter().next().expect("the plane is selected")
}

fn window(h: &Harness) -> egui::Rect {
    h.ctx.memory(|m| m.area_rect(egui::Id::new("Render To Texture"))).expect("Render To Texture shows")
}

/// The button `label` inside the Render To Texture window.
fn button(h: &Harness, label: &str) -> egui::Pos2 {
    let win = window(h);
    h.find_all(Role::Button, label).into_iter().find(|r| win.contains_rect(*r)).unwrap_or_else(|| panic!("no {label:?} button in the dialog")).center()
}

fn output_dir(h: &mut Harness, tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("loom-ui-bake-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let r = h.app.dispatch_json(&serde_json::json!({ "cmd": "Render To Texture Settings", "args": { "output_dir": dir.to_string_lossy() } }));
    assert_eq!(r["ok"], true, "{r}");
    h.frames(1);
    dir
}

/// 0 opens the dialog; Add... > NormalsMap, a size preset and Render write
/// the map of the selected plane and give it a Shell material.
#[test]
fn render_to_texture_dialog_adds_an_element_and_renders_it() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1400.0));
    let p = plane(&mut h);
    h.key(egui::Key::Num0);
    assert!(h.app.state.render_to_texture_open, "0 opens Render To Texture");
    let win = window(&h);
    let name = h.app.state.scene.node(p).unwrap().name.clone();
    for label in ["General Settings", "Objects to Bake", "Output", "Baked Material", "Skip Existing Files", "Use Automatic Unwrap", "Use Existing Channel", "All Selected", "Save Source (Create Shell)", "Output Into Source", name.as_str()] {
        assert!(h.text_rect(label, win).is_some(), "{label:?} in the dialog");
    }
    let dir = output_dir(&mut h, "normals");
    h.click(button(&h, "Add..."));
    h.click(h.find(Role::Button, "NormalsMap").center());
    let elements = &h.app.state.scene.render.bake.elements;
    assert_eq!(elements.len(), 1, "Add... > NormalsMap adds the element");
    assert_eq!(elements[0].kind, loom_scene::bake::ElementKind::Normals);
    let win = window(&h);
    for label in ["Selected Element Common Settings", "Target Map Slot:", "Element Type:", "Output into Normal Bump", "Render Height Map into Alpha Channel"] {
        assert!(h.text_rect(label, win).is_some(), "{label:?} for the selected element");
    }
    let preset = h.text_rect("128x128", win).expect("the 128x128 preset");
    h.click(preset.center());
    assert_eq!((h.app.state.scene.render.bake.elements[0].width, h.app.state.scene.render.bake.elements[0].height), (128, 128));
    // Target Map Slot: the list's second entry.
    let combo = h.text_rect("(none)", win).expect("the Target Map Slot list");
    h.click(combo.center());
    h.click_text_anywhere("Diffuse Color");
    assert_eq!(h.app.state.scene.render.bake.elements[0].target_slot, "Diffuse Color");
    h.click(button(&h, "Render"));
    let path = dir.join(format!("{name}NormalsMap.tga"));
    let img = image::open(&path).expect("Render writes the map").to_rgba8();
    assert_eq!(img.dimensions(), (128, 128));
    let shell = h.app.state.scene.node(p).unwrap().material.expect("a Shell material");
    assert!(matches!(h.app.state.scene.materials[&shell].kind, loom_scene::material::MaterialKind::Shell(_)));
    // Close and 0 again.
    h.click(button(&h, "Close"));
    assert!(!h.app.state.render_to_texture_open);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A LightingMap needs the GPU: the UI renders it right after Render.
#[test]
fn a_lighting_map_renders_on_the_gpu_from_the_dialog() {
    let Some(mut h) = Harness::with_gpu() else {
        eprintln!("a_lighting_map_renders_on_the_gpu_from_the_dialog: no GPU adapter, skipped");
        return;
    };
    let p = plane(&mut h);
    h.cmd(Command::RenderToTexture);
    let dir = output_dir(&mut h, "lighting");
    h.click(button(&h, "Add..."));
    h.click(h.find(Role::Button, "LightingMap").center());
    h.click(button(&h, "Render"));
    for _ in 0..10 {
        if h.app.state.pending_bake.is_none() {
            break;
        }
        h.frames(1);
    }
    assert!(h.app.state.pending_bake.is_none(), "the renderer took the request");
    let name = h.app.state.scene.node(p).unwrap().name.clone();
    let img = image::open(dir.join(format!("{name}LightingMap.tga"))).expect("the lighting map").to_rgba8();
    assert_eq!(img.dimensions(), (256, 256));
    // The default key light lights the plane: every texel on it is lit.
    let lit = img.pixels().filter(|p| p.0[3] == 255 && p.0[0] > 40).count();
    assert!(lit > 256 * 256 / 2, "lit texels: {lit}");
    assert!(h.app.state.rendered.image.is_some(), "the Rendered Frame Window shows the map");
    let _ = std::fs::remove_dir_all(&dir);
}
