//! Render Setup > Common completeness: Time Output sequences, the Options
//! group, and what the renders write.

use super::{click_text, finish_render, framed, Harness};
use eframe::egui;
use loom_commands::{Command, PrimitiveKind};
use loom_scene::render::TimeOutput;

fn ok(h: &mut Harness, cmd: &str, args: serde_json::Value) -> serde_json::Value {
    let r = h.app.dispatch_json(&serde_json::json!({ "cmd": cmd, "args": args }));
    assert_eq!(r["ok"], true, "{cmd}: {r}");
    r["result"].clone()
}

fn setup_window(h: &mut Harness) -> egui::Rect {
    h.cmd(Command::RenderSetup { width: None, height: None, image_aspect: None, pixel_aspect: None, lock_image_aspect: None, lock_pixel_aspect: None, render_hidden: None, force_two_sided: None, atmospherics: None, effects: None, save_file: None, output: None });
    h.frames(2);
    h.ctx.memory(|m| m.area_rect(egui::Id::new("Render Setup"))).expect("Render Setup is open")
}

/// Time Output and the Render Output toggles are clicked like a person would.
#[test]
fn time_output_is_chosen_in_render_setup() {
    let mut h = Harness::new();
    let win = setup_window(&mut h);
    click_text(&mut h, "Range", win, 0.0);
    assert_eq!(h.app.state.scene.render.time_output, TimeOutput::Range);
    click_text(&mut h, "Active Time Segment", win, 0.0);
    assert_eq!(h.app.state.scene.render.time_output, TimeOutput::ActiveSegment);
    click_text(&mut h, "Skip Existing Images", win, 0.0);
    assert!(h.app.state.scene.render.skip_existing);
    click_text(&mut h, "Put Image File List in Output Path", win, 0.0);
    assert!(h.app.state.scene.render.image_file_list);
    h.cmd(Command::UndoSceneOperation);
    assert!(!h.app.state.scene.render.image_file_list, "each click is an undo step");
}

/// Opaque pixels' mean x of an image.
fn centroid_x(img: &image::RgbaImage) -> f32 {
    let (mut sum, mut n) = (0.0, 0.0);
    for (x, _, p) in img.enumerate_pixels() {
        if p.0[3] > 128 {
            sum += x as f32;
            n += 1.0;
        }
    }
    assert!(n > 50.0, "the object is in the frame ({n} pixels)");
    sum / n
}

/// A Range renders each frame of an animation to its numbered file: the
/// box moves across the frame from file to file, and the current frame comes back.
#[test]
fn a_range_renders_an_animation_to_numbered_files() {
    let Some(mut h) = Harness::with_gpu() else {
        eprintln!("a_range_renders_an_animation_to_numbered_files: no GPU adapter, skipped");
        return;
    };
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [20.0; 3] });
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [120.0, 0.0, 0.0], size: [1.0; 3] });
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [-120.0, 0.0, 0.0], size: [1.0; 3] });
    framed(&mut h);
    let first = h.app.state.scene.nodes().map(|n| n.id).min().unwrap();
    h.cmd(Command::SelectNodes { ids: vec![first.0], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::AutoKeyModeToggle);
    h.cmd(Command::SetFrame { frame: 2 });
    h.cmd(Command::MoveSelection { delta: [80.0, 0.0, 0.0], system: None });
    h.cmd(Command::AutoKeyModeToggle);
    h.cmd(Command::SetFrame { frame: 50 });
    let dir = std::env::temp_dir().join(format!("loom-ui-seq-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("anim.png");
    ok(&mut h, "Render SetUp...", serde_json::json!({ "width": 160, "height": 120, "output": out.to_string_lossy() }));
    ok(&mut h, "Bevy Renderer Settings", serde_json::json!({ "passes": 1 }));
    ok(&mut h, "Render Time Output", serde_json::json!({ "time_output": "Range", "range": [0, 2] }));
    h.cmd(Command::Render { path: None, width: None, height: None });
    finish_render(&mut h, 200);
    assert_eq!(h.app.last_error, None);
    let xs: Vec<f32> = (0..=2)
        .map(|f| {
            let p = dir.join(format!("anim{f:04}.png"));
            centroid_x(&image::open(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display())).to_rgba8())
        })
        .collect();
    let (d1, d2) = (xs[1] - xs[0], xs[2] - xs[1]);
    assert!(d1.abs() > 5.0 && d2.abs() > 5.0 && d1.signum() == d2.signum(), "the box moves across the frame, frame by frame: {xs:?}");
    assert_eq!(h.app.state.current_frame, 50, "the time slider is back");
    assert!(h.app.state.rendered.sequence.is_none());
    let _ = std::fs::remove_dir_all(&dir);
    // Every frame is in the Render Message Window's log.
    let saved = h.app.state.rendered.messages.iter().filter(|m| m.text.contains("saved to")).count();
    assert_eq!(saved, 3, "{:?}", h.app.state.rendered.messages);
}

/// Super Black lifts a black object's pixels to the threshold; the
/// background stays black. Both checkboxes are clicked in Render Setup.
#[test]
fn super_black_lifts_rendered_geometry() {
    let Some(mut h) = Harness::with_gpu() else {
        eprintln!("super_black_lifts_rendered_geometry: no GPU adapter, skipped");
        return;
    };
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Teapot, position: [0.0; 3], size: [25.0; 3] });
    let id = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::SetWireColor { id: id.0, rgb: [0, 0, 0] });
    framed(&mut h);
    ok(&mut h, "Render SetUp...", serde_json::json!({ "width": 120, "height": 90 }));
    ok(&mut h, "Bevy Renderer Settings", serde_json::json!({ "passes": 1 }));
    let darkest = |h: &mut Harness| {
        h.cmd(Command::Render { path: None, width: None, height: None });
        finish_render(h, 100);
        let img = h.app.state.rendered.image.clone().unwrap();
        let geo: Vec<u8> = img.pixels().filter(|p| p.0[3] == 255).map(|p| p.0[0].min(p.0[1]).min(p.0[2])).collect();
        assert!(geo.len() > 200, "the teapot is in the frame");
        (*geo.iter().min().unwrap(), img.get_pixel(0, 0).0)
    };
    let (off, _) = darkest(&mut h);
    assert!(off < 15, "a black teapot has pixels darker than the threshold: {off}");
    let win = setup_window(&mut h);
    click_text(&mut h, "Super Black", win, 0.0);
    assert!(h.app.state.scene.render.super_black);
    let (on, bg) = darkest(&mut h);
    assert_eq!(on, 15, "lifted to the threshold");
    assert_eq!(bg, [0, 0, 0, 0], "the background stays black");
    let win = setup_window(&mut h);
    click_text(&mut h, "Video Color Check", win, 0.0);
    assert!(h.app.state.scene.render.video_color_check);
}

fn opaque(img: &image::RgbaImage) -> usize {
    img.pixels().filter(|p| p.0[3] > 128).count()
}

/// Force Wireframe draws the polygon edges only: far fewer opaque pixels
/// than the solid render, more with a thicker wire, transparent between.
#[test]
fn force_wireframe_renders_the_polygon_edges() {
    let Some(mut h) = Harness::with_gpu() else {
        eprintln!("force_wireframe_renders_the_polygon_edges: no GPU adapter, skipped");
        return;
    };
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [40.0; 3] });
    framed(&mut h);
    ok(&mut h, "Render SetUp...", serde_json::json!({ "width": 200, "height": 150 }));
    ok(&mut h, "Bevy Renderer Settings", serde_json::json!({ "passes": 1 }));
    let render = |h: &mut Harness| {
        h.cmd(Command::Render { path: None, width: None, height: None });
        finish_render(h, 100);
        assert_eq!(h.app.last_error, None);
        h.app.state.rendered.image.clone().unwrap()
    };
    let solid = opaque(&render(&mut h));
    ok(&mut h, "Render Options", serde_json::json!({ "force_wireframe": true, "wire_thickness": 1.0 }));
    let thin = opaque(&render(&mut h));
    ok(&mut h, "Render Options", serde_json::json!({ "wire_thickness": 4.0 }));
    let thick = opaque(&render(&mut h));
    assert!(solid > 3000, "the box fills the frame: {solid}");
    assert!(thin > 100 && thin * 4 < solid, "edges only: {thin} of {solid}");
    assert!(thick > thin * 2, "a thicker wire: {thick} against {thin}");
}

/// Render Elements: each enabled element is written next to the render
/// output and measured: Alpha is the coverage, the lit elements are black
/// off the objects, Lighting is uncoloured, Shadow darkens the floor under
/// the teapot, Z Depth grades with distance and Background is opaque.
#[test]
fn render_elements_write_their_images() {
    let Some(mut h) = Harness::with_gpu() else {
        eprintln!("render_elements_write_their_images: no GPU adapter, skipped");
        return;
    };
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0; 3], size: [120.0, 120.0, 0.0] });
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Teapot, position: [0.0; 3], size: [25.0; 3] });
    framed(&mut h);
    let dir = std::env::temp_dir().join(format!("loom-ui-elements-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("frame.png");
    ok(&mut h, "Render SetUp...", serde_json::json!({ "width": 160, "height": 120, "output": out.to_string_lossy() }));
    ok(&mut h, "Bevy Renderer Settings", serde_json::json!({ "passes": 1 }));
    for kind in ["Alpha", "Diffuse", "Specular", "Lighting", "Shadow", "ZDepth", "Background"] {
        ok(&mut h, "Add Render Element", serde_json::json!({ "kind": kind }));
    }
    ok(&mut h, "Render Element Parameters", serde_json::json!({ "index": 5, "z_min": 0.0, "z_max": 1000.0 }));
    h.cmd(Command::Render { path: None, width: None, height: None });
    finish_render(&mut h, 100);
    assert_eq!(h.app.last_error, None, "{:?}", h.app.state.rendered.messages);
    let main = image::open(&out).unwrap().to_rgba8();
    let load = |name: &str| image::open(dir.join(format!("frame_{name}.png"))).unwrap_or_else(|e| panic!("{name}: {e}")).to_rgba8();
    let geo: Vec<(u32, u32)> = main.enumerate_pixels().filter(|p| p.2 .0[3] == 255).map(|p| (p.0, p.1)).collect();
    let empty: Vec<(u32, u32)> = main.enumerate_pixels().filter(|p| p.2 .0[3] == 0).map(|p| (p.0, p.1)).collect();
    assert!(geo.len() > 1000 && !empty.is_empty(), "the floor and teapot fill part of the frame");
    let alpha = load("Alpha");
    assert!(geo.iter().all(|&(x, y)| alpha.get_pixel(x, y).0 == [255, 255, 255, 255]));
    assert!(empty.iter().all(|&(x, y)| alpha.get_pixel(x, y).0 == [0, 0, 0, 255]));
    let mean = |img: &image::RgbaImage| geo.iter().map(|&(x, y)| img.get_pixel(x, y).0[..3].iter().map(|c| *c as f32).sum::<f32>()).sum::<f32>() / geo.len() as f32;
    let (diffuse, specular, lighting) = (load("Diffuse"), load("Specular"), load("Lighting"));
    for img in [&diffuse, &specular, &lighting] {
        assert!(empty.iter().all(|&(x, y)| img.get_pixel(x, y).0[..3] == [0, 0, 0]), "black off the objects");
    }
    assert!(mean(&specular) < mean(&diffuse), "highlights only: {} against {}", mean(&specular), mean(&diffuse));
    let grey = geo.iter().filter(|&&(x, y)| {
        let p = lighting.get_pixel(x, y).0;
        p[0].abs_diff(p[1]) <= 3 && p[1].abs_diff(p[2]) <= 3
    });
    assert!(grey.count() * 10 >= geo.len() * 9, "white light on a white material is grey");
    let shadow = load("Shadow");
    let dark = geo.iter().filter(|&&(x, y)| shadow.get_pixel(x, y).0[0] < 128).count();
    let lit = geo.iter().filter(|&&(x, y)| shadow.get_pixel(x, y).0[0] > 240).count();
    assert!(dark > 20 && lit > dark, "the teapot's shadow on the floor: {dark} dark, {lit} lit");
    let z = load("ZDepth");
    let greys: std::collections::BTreeSet<u8> = geo.iter().map(|&(x, y)| z.get_pixel(x, y).0[0]).collect();
    assert!(greys.len() > 10, "depth grades across the scene: {greys:?}");
    assert!(empty.iter().all(|&(x, y)| z.get_pixel(x, y).0[3] == 0));
    let bg = load("Background");
    assert!(bg.pixels().all(|p| p.0 == [0, 0, 0, 255]), "the black background, opaque");
    // Display Elements: each in its own window.
    assert_eq!(h.app.state.rendered.clones.len(), 7);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The Render Shortcuts toolbar: Shift+click on B stores the settings, a click brings them back.
#[test]
fn render_shortcuts_toolbar_stores_and_applies_a_preset() {
    let mut h = Harness::new();
    h.app.state.ui_custom.shown.insert("Render Shortcuts".into(), true);
    h.frames(2);
    ok(&mut h, "Render SetUp...", serde_json::json!({ "width": 320, "height": 240 }));
    h.mods = egui::Modifiers::SHIFT;
    h.click_text_anywhere("B");
    h.mods = egui::Modifiers::NONE;
    assert!(h.app.state.rendered.shortcuts[1].is_some());
    ok(&mut h, "Render SetUp...", serde_json::json!({ "width": 800, "height": 600 }));
    h.click_text_anywhere("B");
    assert_eq!(h.app.state.scene.render.width, 320);
}

/// RAM Player: the last render goes into channel A with a click, and shows.
#[test]
fn ram_player_opens_the_last_render() {
    let mut h = Harness::new();
    h.app.state.rendered.set_image(Some(std::sync::Arc::new(image::RgbaImage::from_pixel(32, 16, image::Rgba([0, 200, 0, 255])))));
    h.cmd(Command::RamPlayer);
    h.frames(2);
    h.click_text_anywhere("Open Last Rendered Image in Channel A");
    h.frames(2);
    let a = h.app.state.rendered.ram_player.channels[0].as_ref().expect("channel A");
    assert_eq!(a.1.dimensions(), (32, 16));
    assert!(h.app.rfw.ram.texture_id().is_some(), "the window shows it");
}

/// The Render Elements panel: Add... lists the kinds; a click adds one.
#[test]
fn render_elements_panel_adds_an_element() {
    let mut h = Harness::new();
    let win = setup_window(&mut h);
    click_text(&mut h, "Render Elements", win, 0.0);
    let win = h.ctx.memory(|m| m.area_rect(egui::Id::new("Render Setup"))).unwrap();
    click_text(&mut h, "Add...", win, 0.0);
    h.frames(1);
    click_text(&mut h, "Z Depth", egui::Rect::EVERYTHING, 0.0);
    assert_eq!(h.app.state.scene.render.elements.len(), 1);
    assert_eq!(h.app.state.scene.render.elements[0].kind, loom_scene::render::ElementKind::ZDepth);
    let win = h.ctx.memory(|m| m.area_rect(egui::Id::new("Render Setup"))).unwrap();
    click_text(&mut h, "Enable", win, 0.0);
    assert!(!h.app.state.scene.render.elements[0].enabled);
}

/// Render Output > Rendered Frame Window off: a render does not open the
/// window; with gamma correction on, the title shows the display gamma.
#[test]
fn rendered_frame_window_toggle_and_gamma_in_the_title() {
    let mut h = Harness::new();
    let win = setup_window(&mut h);
    assert!(h.app.state.scene.render.rendered_frame_window, "on by default");
    click_text(&mut h, "Rendered Frame Window", win, 0.0);
    assert!(!h.app.state.scene.render.rendered_frame_window);
    h.app.state.rendered.open = false;
    h.cmd(Command::Render { path: None, width: Some(8), height: Some(8) });
    assert!(!h.app.state.rendered.open, "the render runs without the window");
    assert!(!h.app.state.rendered.title.contains("Gamma"), "{}", h.app.state.rendered.title);
    // The first render's job is not driven here (no GPU): let it go.
    h.app.state.pending_render = None;
    h.app.state.rendered.progress = None;
    click_text(&mut h, "Rendered Frame Window", win, 0.0);
    h.cmd(Command::GammaSettings { enabled: Some(true), output_gamma: None, override_gamma: None, use_system: None });
    h.cmd(Command::Render { path: None, width: Some(8), height: Some(8) });
    assert!(h.app.state.rendered.open);
    assert!(h.app.state.rendered.title.contains("Display Gamma: 2.2"), "{}", h.app.state.rendered.title);
}
