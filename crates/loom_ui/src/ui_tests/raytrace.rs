//! Ray tracing, motion blur, the Light Tracer and the renderer options in
//! renders: each measured in the rendered pixels, and their Render Setup
//! and Object Properties controls clicked as a person would.

use super::*;
use egui::accesskit::Role;

fn ok(h: &mut Harness, cmd: &str, args: serde_json::Value) -> serde_json::Value {
    let r = h.app.dispatch_json(&serde_json::json!({ "cmd": cmd, "args": args }));
    assert_eq!(r["ok"], true, "{cmd}: {r}");
    r["result"].clone()
}

fn render(h: &mut Harness) -> image::RgbaImage {
    h.cmd(Command::Render { path: None, width: None, height: None });
    finish_render(h, 200);
    assert_eq!(h.app.last_error, None, "{:?}", h.app.state.rendered.messages);
    (*h.app.state.rendered.image.clone().expect("a rendered image")).clone()
}

fn selected(h: &Harness) -> loom_scene::NodeId {
    *h.app.state.selection.iter().next().expect("the new object is selected")
}

fn red(p: &image::Rgba<u8>) -> bool {
    p.0[0] > 90 && p.0[0] as i32 > p.0[1] as i32 + 50 && p.0[0] as i32 > p.0[2] as i32 + 50
}

fn setup_window(h: &mut Harness) -> egui::Rect {
    h.cmd(Command::RenderSetup { width: None, height: None, image_aspect: None, pixel_aspect: None, lock_image_aspect: None, lock_pixel_aspect: None, render_hidden: None, force_two_sided: None, atmospherics: None, effects: None, save_file: None, output: None });
    h.frames(2);
    h.ctx.memory(|m| m.area_rect(egui::Id::new("Render Setup"))).expect("Render Setup is open")
}

/// A Flat Mirror floor under a red box: ray traced, the floor shows the
/// box's reflection (red pixels the box itself does not cover); with Enable
/// Raytracing off they are gone. The Reflection element holds them.
#[test]
fn a_flat_mirror_floor_reflects_the_box_on_it() {
    let Some(mut h) = Harness::with_gpu() else {
        eprintln!("a_flat_mirror_floor_reflects_the_box_on_it: no GPU adapter, skipped");
        return;
    };
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0; 3], size: [200.0, 200.0, 0.0] });
    h.cmd(Command::MaterialEditorSlot { slot: 0 });
    ok(&mut h, "Set Material Color", serde_json::json!({ "which": "Diffuse", "rgb": [0, 0, 0] }));
    ok(&mut h, "New Map", serde_json::json!({ "map": "Reflection", "kind": "Flat Mirror" }));
    h.cmd(Command::AssignMaterialToSelection);
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [40.0, 40.0, 40.0] });
    let b = selected(&h);
    h.cmd(Command::SetWireColor { id: b.0, rgb: [230, 20, 20] });
    framed(&mut h);
    let dir = std::env::temp_dir().join(format!("loom-ui-mirror-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("mirror.png");
    ok(&mut h, "Render SetUp...", serde_json::json!({ "width": 160, "height": 120, "output": out.to_string_lossy() }));
    ok(&mut h, "Bevy Renderer Settings", serde_json::json!({ "passes": 1 }));
    ok(&mut h, "Add Render Element", serde_json::json!({ "kind": "Reflection" }));
    let traced = render(&mut h);
    ok(&mut h, "Raytracer Settings", serde_json::json!({ "enabled": false }));
    let plain = render(&mut h);
    let (on, off) = (traced.pixels().filter(|p| red(p)).count(), plain.pixels().filter(|p| red(p)).count());
    assert!(off > 200, "the box is in the frame: {off}");
    assert!(on > off + 150, "the mirror adds the box's reflection: {on} red pixels against {off}");
    let refl = image::open(dir.join("mirror_Reflection.png")).unwrap().to_rgba8();
    let shown = refl.pixels().filter(|p| red(p) && p.0[3] > 0).count();
    assert!(shown > 150, "the Reflection element has the reflected box: {shown}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A glass sphere (Raytrace material, Transparency white) in front of a red
/// stripe: at IOR 1 the stripe shows through where it is, at IOR 1.5 the
/// sphere bends it elsewhere.
#[test]
fn a_raytrace_glass_sphere_refracts_the_stripe_behind_it() {
    let Some(mut h) = Harness::with_gpu() else {
        eprintln!("a_raytrace_glass_sphere_refracts_the_stripe_behind_it: no GPU adapter, skipped");
        return;
    };
    // A white backdrop and a red stripe behind the sphere, seen from the front.
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0, 80.0, -60.0], size: [200.0, 2.0, 120.0] });
    let back = selected(&h);
    h.cmd(Command::SetWireColor { id: back.0, rgb: [240, 240, 240] });
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [14.0, 60.0, -60.0], size: [5.0, 2.0, 120.0] });
    let stripe = selected(&h);
    h.cmd(Command::SetWireColor { id: stripe.0, rgb: [230, 20, 20] });
    // Radius 20, centred on the origin (a sphere rests on its position).
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Sphere, position: [0.0, 0.0, -20.0], size: [40.0, 40.0, 40.0] });
    h.cmd(Command::MaterialEditorSlot { slot: 0 });
    ok(&mut h, "Set Material Type", serde_json::json!({ "kind": "Raytrace" }));
    ok(&mut h, "Set Material Color", serde_json::json!({ "which": "Diffuse", "rgb": [0, 0, 0] }));
    ok(&mut h, "Set Material Color", serde_json::json!({ "which": "Transparency", "rgb": [255, 255, 255] }));
    h.cmd(Command::AssignMaterialToSelection);
    h.cmd(Command::FrontView);
    framed(&mut h);
    ok(&mut h, "Render SetUp...", serde_json::json!({ "width": 160, "height": 120 }));
    ok(&mut h, "Bevy Renderer Settings", serde_json::json!({ "passes": 1 }));
    // The stripe's red pixels on the middle rows (through the sphere's centre).
    let stripe_x = |img: &image::RgbaImage| {
        let xs: Vec<u32> = (55..65).flat_map(|y| (0..160).filter(move |x| red(img.get_pixel(*x, y)))).collect();
        assert!(!xs.is_empty(), "the stripe shows on the middle rows");
        xs.iter().sum::<u32>() as f32 / xs.len() as f32
    };
    ok(&mut h, "Set Material Parameter", serde_json::json!({ "name": "Index of Refr.", "value": 1.0 }));
    let straight = stripe_x(&render(&mut h));
    ok(&mut h, "Raytracer Settings", serde_json::json!({ "enabled": false }));
    let behind = render(&mut h);
    // The stripe above the sphere is where IOR 1 shows it through the sphere.
    let top: Vec<u32> = (28..33).flat_map(|y| (0..160).filter(move |x| red(behind.get_pixel(*x, y)))).collect();
    assert!(!top.is_empty(), "the stripe above the sphere");
    let top_x = top.iter().sum::<u32>() as f32 / top.len() as f32;
    assert!((straight - top_x).abs() <= 1.5, "IOR 1 shows the stripe where it is: {straight} against {top_x}");
    ok(&mut h, "Raytracer Settings", serde_json::json!({ "enabled": true }));
    ok(&mut h, "Set Material Parameter", serde_json::json!({ "name": "Index of Refr.", "value": 1.5 }));
    let bent = stripe_x(&render(&mut h));
    assert!((bent - straight).abs() >= 3.0, "IOR 1.5 bends the stripe: {bent} against {straight}");
}

/// Columns of the image with a pixel more than half covered.
fn covered_columns(img: &image::RgbaImage) -> usize {
    (0..img.width()).filter(|x| (0..img.height()).any(|y| img.get_pixel(*x, y).0[3] > 20)).count()
}

/// A box moving across the frame: Object motion blur widens it into a
/// streak of partly covered pixels, Image motion blur too, and the
/// Velocity element shows its motion to the right.
#[test]
fn motion_blur_smears_a_moving_box() {
    let Some(mut h) = Harness::with_gpu() else {
        eprintln!("motion_blur_smears_a_moving_box: no GPU adapter, skipped");
        return;
    };
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [20.0; 3] });
    let b = selected(&h);
    // Far markers so the framing covers the motion.
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [140.0, 0.0, 0.0], size: [0.5; 3] });
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [-140.0, 0.0, 0.0], size: [0.5; 3] });
    h.cmd(Command::SelectNodes { ids: vec![b.0], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::AutoKeyModeToggle);
    h.cmd(Command::SetFrame { frame: 2 });
    h.cmd(Command::MoveSelection { delta: [80.0, 0.0, 0.0], system: None });
    h.cmd(Command::AutoKeyModeToggle);
    h.cmd(Command::SetFrame { frame: 1 });
    h.cmd(Command::FrontView);
    framed(&mut h);
    let dir = std::env::temp_dir().join(format!("loom-ui-blur-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("blur.png");
    ok(&mut h, "Render SetUp...", serde_json::json!({ "width": 200, "height": 100, "output": out.to_string_lossy() }));
    ok(&mut h, "Bevy Renderer Settings", serde_json::json!({ "passes": 4 }));
    ok(&mut h, "Renderer Options", serde_json::json!({ "object_blur_duration": 1.0, "object_blur_samples": 8, "object_blur_subdivisions": 8, "image_blur_duration": 1.0 }));
    let sharp = covered_columns(&render(&mut h));
    ok(&mut h, "Set Motion Blur", serde_json::json!({ "kind": "Object" }));
    let object = covered_columns(&render(&mut h));
    assert!(object > sharp + 8, "Object motion blur streaks the box: {object} columns against {sharp}");
    ok(&mut h, "Set Motion Blur", serde_json::json!({ "kind": "Image" }));
    ok(&mut h, "Add Render Element", serde_json::json!({ "kind": "Velocity" }));
    let image_blur = covered_columns(&render(&mut h));
    assert!(image_blur > sharp + 8, "Image motion blur smears it: {image_blur} columns against {sharp}");
    let v = image::open(dir.join("blur_Velocity.png")).unwrap().to_rgba8();
    let right = v.pixels().filter(|p| p.0[3] > 0 && p.0[0] > 200).count();
    assert!(right > 50, "the Velocity element shows the motion to the right: {right}");
    // Apply off: no blur.
    ok(&mut h, "Renderer Options", serde_json::json!({ "image_motion_blur": false }));
    let off = covered_columns(&render(&mut h));
    assert!(off <= sharp + 2, "Apply off leaves it sharp: {off} against {sharp}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The Light Tracer with a bounce brightens what lies in shadow (light
/// reflected from the lit floor and box); without it those pixels stay dark.
#[test]
fn light_tracer_brightens_the_shadow_by_bounce() {
    let Some(mut h) = Harness::with_gpu() else {
        eprintln!("light_tracer_brightens_the_shadow_by_bounce: no GPU adapter, skipped");
        return;
    };
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0; 3], size: [160.0, 160.0, 0.0] });
    let floor = selected(&h);
    h.cmd(Command::SetWireColor { id: floor.0, rgb: [230, 230, 230] });
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [30.0, 30.0, 50.0] });
    let b = selected(&h);
    h.cmd(Command::SetWireColor { id: b.0, rgb: [230, 230, 230] });
    framed(&mut h);
    ok(&mut h, "Render SetUp...", serde_json::json!({ "width": 160, "height": 120 }));
    ok(&mut h, "Bevy Renderer Settings", serde_json::json!({ "passes": 1 }));
    let plain = render(&mut h);
    let dark: Vec<(u32, u32)> = plain.enumerate_pixels().filter(|(_, _, p)| p.0[3] == 255 && p.0[0].max(p.0[1]).max(p.0[2]) < 25).map(|(x, y, _)| (x, y)).collect();
    assert!(dark.len() > 100, "the box's shadow and dark side: {}", dark.len());
    let mean = |img: &image::RgbaImage| dark.iter().map(|&(x, y)| img.get_pixel(x, y).0[0] as f32).sum::<f32>() / dark.len() as f32;
    ok(&mut h, "Advanced Lighting", serde_json::json!({ "plugin": "LightTracer", "bounces": 0, "rays_per_sample": 48, "filter_size": 2.0 }));
    let no_bounce = mean(&render(&mut h));
    ok(&mut h, "Advanced Lighting", serde_json::json!({ "bounces": 1 }));
    let bounced = mean(&render(&mut h));
    let before = mean(&plain);
    assert!((no_bounce - before).abs() < 4.0, "no skylight, no bounce: nothing to add ({no_bounce} against {before})");
    assert!(bounced > before + 10.0, "the bounce lights the shadow: {bounced} against {before}");
    // Common > Use Advanced Lighting off leaves it out again.
    ok(&mut h, "Advanced Lighting", serde_json::json!({ "use_advanced": false }));
    let off = mean(&render(&mut h));
    assert!((off - before).abs() < 4.0, "{off} against {before}");
}

/// Color Range Limiting: an over-lit orange teapot clips to yellow-white
/// with Clamp; Scale keeps its hue (green stays well under red).
#[test]
fn color_range_scale_keeps_the_hue_of_overbright_colours() {
    let Some(mut h) = Harness::with_gpu() else {
        eprintln!("color_range_scale_keeps_the_hue_of_overbright_colours: no GPU adapter, skipped");
        return;
    };
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Teapot, position: [0.0; 3], size: [25.0; 3] });
    let t = selected(&h);
    h.cmd(Command::SetWireColor { id: t.0, rgb: [255, 140, 40] });
    framed(&mut h);
    ok(&mut h, "Environment Settings", serde_json::json!({ "level": 5.0 }));
    ok(&mut h, "Render SetUp...", serde_json::json!({ "width": 120, "height": 90 }));
    ok(&mut h, "Bevy Renderer Settings", serde_json::json!({ "passes": 1 }));
    let clipped = |img: &image::RgbaImage| img.pixels().filter(|p| p.0[3] == 255 && p.0[0] >= 250 && p.0[1] >= 245).count();
    let clamp = clipped(&render(&mut h));
    ok(&mut h, "Renderer Options", serde_json::json!({ "color_range": "Scale" }));
    let scaled = render(&mut h);
    assert!(clamp > 100, "Clamp washes the lit side out: {clamp}");
    assert!(clipped(&scaled) * 5 < clamp, "Scale keeps the hue: {} against {clamp}", clipped(&scaled));
    let bright = scaled.pixels().filter(|p| p.0[3] == 255 && p.0[0] >= 250).count();
    assert!(bright > 100, "the lit side still reaches full red: {bright}");
}

/// Filter Maps off reads a fine checker bitmap at its nearest texel:
/// neighbouring pixels differ far more than filtered.
#[test]
fn filter_maps_off_reads_the_nearest_texel() {
    let Some(mut h) = Harness::with_gpu() else {
        eprintln!("filter_maps_off_reads_the_nearest_texel: no GPU adapter, skipped");
        return;
    };
    let dir = std::env::temp_dir().join(format!("loom-ui-filtermaps-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let checker = dir.join("checker.png");
    image::RgbaImage::from_fn(256, 256, |x, y| if (x + y) % 2 == 0 { image::Rgba([255, 255, 255, 255]) } else { image::Rgba([0, 0, 0, 255]) }).save(&checker).unwrap();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0; 3], size: [100.0, 100.0, 0.0] });
    h.cmd(Command::MaterialEditorSlot { slot: 0 });
    ok(&mut h, "Set Material Map", serde_json::json!({ "map": "Diffuse Color", "path": checker.to_string_lossy() }));
    h.cmd(Command::AssignMaterialToSelection);
    h.cmd(Command::TopView);
    framed(&mut h);
    ok(&mut h, "Render SetUp...", serde_json::json!({ "width": 160, "height": 120 }));
    ok(&mut h, "Bevy Renderer Settings", serde_json::json!({ "passes": 1, "antialiasing": false }));
    // Mean difference between horizontal neighbours on the plane.
    let contrast = |img: &image::RgbaImage| {
        let (mut sum, mut n) = (0.0f32, 0.0f32);
        for y in 0..img.height() {
            for x in 0..img.width() - 1 {
                let (a, b) = (img.get_pixel(x, y).0, img.get_pixel(x + 1, y).0);
                if a[3] == 255 && b[3] == 255 {
                    sum += (a[1] as f32 - b[1] as f32).abs();
                    n += 1.0;
                }
            }
        }
        assert!(n > 1000.0, "the plane fills the frame: {n}");
        sum / n
    };
    let filtered = contrast(&render(&mut h));
    ok(&mut h, "Renderer Options", serde_json::json!({ "filter_maps": false }));
    let nearest = contrast(&render(&mut h));
    assert!(nearest > filtered * 1.3 + 3.0, "unfiltered texels stay apart: {nearest} against {filtered}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Render Setup: the Raytracer and Advanced Lighting panels, the Renderer
/// panel's new options and Common > Use Advanced Lighting, each a click and
/// an undo step.
#[test]
fn raytracer_and_advanced_lighting_panels_are_clicked() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1400.0));
    let win = setup_window(&mut h);
    click_text(&mut h, "Raytracer", win, 0.0);
    let win = h.ctx.memory(|m| m.area_rect(egui::Id::new("Render Setup"))).unwrap();
    click_text(&mut h, "Enable Self Reflect/Refract", win, 0.0);
    assert!(!h.app.state.scene.render.raytracer.self_reflect);
    click_text(&mut h, "On", win, 0.0);
    assert!(h.app.state.scene.render.raytracer.antialias);
    h.cmd(Command::UndoSceneOperation);
    assert!(!h.app.state.scene.render.raytracer.antialias, "one undo step");
    click_text(&mut h, "Advanced Lighting", win, 0.0);
    let win = h.ctx.memory(|m| m.area_rect(egui::Id::new("Render Setup"))).unwrap();
    click_text(&mut h, "Active", win, 0.0);
    assert!(!h.app.state.scene.render.advanced.active);
    ok(&mut h, "Advanced Lighting", serde_json::json!({ "plugin": "LightTracer" }));
    h.frames(2);
    let win = h.ctx.memory(|m| m.area_rect(egui::Id::new("Render Setup"))).unwrap();
    assert!(h.text_rect("Rays/Sample:", win).is_some(), "the Light Tracer's parameters show");
    click_text(&mut h, "Sky Lights:", win, 0.0);
    assert!(!h.app.state.scene.render.advanced.sky_lights_on);
    click_text(&mut h, "Common", win, 0.0);
    let win = h.ctx.memory(|m| m.area_rect(egui::Id::new("Render Setup"))).unwrap();
    click_text(&mut h, "Use Advanced Lighting", win, 0.0);
    assert!(!h.app.state.scene.render.advanced.use_advanced);
    click_text(&mut h, "Renderer", win, 0.0);
    let win = h.ctx.memory(|m| m.area_rect(egui::Id::new("Render Setup"))).unwrap();
    click_text(&mut h, "Filter Maps", win, 0.0);
    assert!(!h.app.state.scene.render.bevy.filter_maps);
    click_text(&mut h, "Scale", win, 0.0);
    assert_eq!(h.app.state.scene.render.bevy.color_range, loom_scene::render::ColorRange::Scale);
    click_text(&mut h, "Disable all Samplers", win, 0.0);
    assert!(h.app.state.scene.render.bevy.disable_all_samplers);
    click_text(&mut h, "Apply to Environment Map", win, 0.0);
    assert!(h.app.state.scene.render.bevy.image_blur_environment);
}

/// Object Properties > Motion Blur: the Image and Object buttons set the
/// object's blur; Visible to Reflection/Refraction is a live checkbox.
#[test]
fn object_properties_motion_blur_is_clicked() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1000.0));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let id = selected(&h);
    h.cmd(Command::ObjectProperties);
    h.frames(2);
    h.click(h.find(Role::RadioButton, "Image").center());
    assert_eq!(h.app.state.scene.node(id).unwrap().render.motion_blur, loom_scene::MotionBlur::Image);
    h.click(h.find(Role::RadioButton, "Object").center());
    assert_eq!(h.app.state.scene.node(id).unwrap().render.motion_blur, loom_scene::MotionBlur::Object);
    h.click(h.find(Role::CheckBox, "Visible to Reflection/Refraction").center());
    assert!(!h.app.state.scene.node(id).unwrap().render.visible_to_reflection);
    h.cmd(Command::UndoSceneOperation);
    assert!(h.app.state.scene.node(id).unwrap().render.visible_to_reflection);
}
