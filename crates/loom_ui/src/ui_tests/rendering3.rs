//! Render elements from the G-buffer and the shading parts, Batch Render,
//! command-line rendering, ActiveShade, the Panorama Exporter, Make
//! Preview and the Rendered Frame Window's channel list, frame arrows and
//! Toggle UI: each measured in the rendered pixels or files.

use super::*;

fn ok(h: &mut Harness, cmd: &str, args: serde_json::Value) -> serde_json::Value {
    let r = h.app.dispatch_json(&serde_json::json!({ "cmd": cmd, "args": args }));
    assert_eq!(r["ok"], true, "{cmd}: {r}");
    if r.get("result").is_some() { r["result"].clone() } else { r }
}

fn selected(h: &Harness) -> loom_scene::NodeId {
    *h.app.state.selection.iter().next().expect("the new object is selected")
}

fn tmp(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("loom-ui-r3-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn render(h: &mut Harness) -> image::RgbaImage {
    h.cmd(Command::Render { path: None, width: None, height: None });
    finish_render(h, 300);
    assert_eq!(h.app.last_error, None, "{:?}", h.app.state.rendered.messages);
    (*h.app.state.rendered.image.clone().expect("a rendered image")).clone()
}

fn element(h: &mut Harness, kind: &str, name: &str, dir: &std::path::Path) -> usize {
    let i = ok(h, "Add Render Element", serde_json::json!({ "kind": kind, "name": name }))["index"].as_u64().unwrap() as usize;
    ok(h, "Render Element Parameters", serde_json::json!({ "index": i, "path": dir.join(format!("{name}.png")).to_string_lossy() }));
    i
}

fn open(dir: &std::path::Path, name: &str) -> image::RgbaImage {
    let p = dir.join(format!("{name}.png"));
    image::open(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display())).to_rgba8()
}

/// Mean of channel `k` over the pixels `keep` picks.
fn mean(img: &image::RgbaImage, k: usize, keep: impl Fn(u32, u32) -> bool) -> f32 {
    let v: Vec<f32> = img.enumerate_pixels().filter(|(x, y, _)| keep(*x, *y)).map(|(_, _, p)| p.0[k] as f32).collect();
    assert!(v.len() > 50, "pixels to measure: {}", v.len());
    v.iter().sum::<f32>() / v.len() as f32
}

/// Two boxes with Object Channels 3 and 5: the G-buffer tags their pixels,
/// the Object ID element paints each in its ID's colour, the Matte element
/// is white on the G-buffer ID 5 box only; the window's channel list shows
/// the Object channel.
#[test]
fn object_ids_tag_the_g_buffer_object_id_and_matte_elements() {
    let Some(mut h) = Harness::with_gpu() else {
        eprintln!("object_ids_tag_the_g_buffer_object_id_and_matte_elements: no GPU adapter, skipped");
        return;
    };
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [-30.0, 0.0, 0.0], size: [25.0; 3] });
    ok(&mut h, "Set Object Channel", serde_json::json!({ "channel": 3 }));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [30.0, 0.0, 0.0], size: [25.0; 3] });
    ok(&mut h, "Set Object Channel", serde_json::json!({ "channel": 5 }));
    framed(&mut h);
    let d = tmp("ids");
    ok(&mut h, "Render SetUp...", serde_json::json!({ "width": 160, "height": 120 }));
    ok(&mut h, "Bevy Renderer Settings", serde_json::json!({ "passes": 1 }));
    element(&mut h, "ObjectId", "ids", &d);
    let m = element(&mut h, "Matte", "matte", &d);
    ok(&mut h, "Render Element Options", serde_json::json!({ "index": m, "matte_gbuffer": 5 }));
    render(&mut h);
    let g = h.app.state.rendered.gbuffer.clone().expect("the G-buffer of the render");
    let ids: std::collections::BTreeSet<u32> = g.object.iter().zip(&g.depth).filter(|(_, d)| d.is_some()).map(|(o, _)| *o).collect();
    assert_eq!(ids, [3, 5].into_iter().collect(), "both boxes' channels are in the frame");
    // Each box's pixels: where the G-buffer has its channel.
    let gb: &loom_commands::render_output::GBuffer = &g;
    let at = |id: u32| move |x: u32, y: u32| gb.object[(y * 160 + x) as usize] == id && gb.depth[(y * 160 + x) as usize].is_some();
    let ids = open(&d, "ids");
    let c3 = loom_commands::render_output::id_color(3);
    let c5 = loom_commands::render_output::id_color(5);
    for k in 0..3 {
        assert!((mean(&ids, k, at(3)) - c3[k] as f32).abs() < 1.0, "box 3 in its colour");
        assert!((mean(&ids, k, at(5)) - c5[k] as f32).abs() < 1.0, "box 5 in its colour");
    }
    let matte = open(&d, "matte");
    assert!(mean(&matte, 0, at(5)) > 254.0, "the G-buffer ID 5 box is white");
    assert!(mean(&matte, 0, at(3)) < 1.0, "the other box is black");
    // The channel list, clicked as a person would.
    let win = h.ctx.memory(|m| m.area_rect(egui::Id::new("Rendered Frame Window"))).expect("the window is open");
    click_text(&mut h, "RGB Alpha", win, 0.0);
    h.click_text_anywhere("Object");
    h.frames(2);
    assert_eq!(h.app.state.rendered.gchannel, Some(loom_commands::render_output::GChannel::ObjectId));
}

/// Diffuse with Lighting off is the flat colour; Blend of every part comes
/// close to the render (Blend of the highlights alone does not); Scale
/// Factor scales Illuminance; Lighting without direct light is darker.
#[test]
fn shading_part_elements_measure_their_part() {
    let Some(mut h) = Harness::with_gpu() else {
        eprintln!("shading_part_elements_measure_their_part: no GPU adapter, skipped");
        return;
    };
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Sphere, position: [0.0; 3], size: [30.0; 3] });
    let id = selected(&h);
    h.cmd(Command::SetWireColor { id: id.0, rgb: [200, 60, 40] });
    h.cmd(Command::CreateLight { kind: loom_commands::light::LightKind::Omni, position: [-80.0, -100.0, 120.0], target: [0.0; 3] });
    framed(&mut h);
    let d = tmp("parts");
    ok(&mut h, "Render SetUp...", serde_json::json!({ "width": 160, "height": 120 }));
    ok(&mut h, "Bevy Renderer Settings", serde_json::json!({ "passes": 1, "ambient": [0.2, 0.2, 0.2] }));
    let flat = element(&mut h, "Diffuse", "flat", &d);
    ok(&mut h, "Render Element Options", serde_json::json!({ "index": flat, "lighting": false }));
    element(&mut h, "Diffuse", "lit", &d);
    element(&mut h, "Blend", "blend", &d);
    let spec = element(&mut h, "Blend", "spec", &d);
    ok(&mut h, "Render Element Options", serde_json::json!({ "index": spec, "blend_parts": ["Specular"] }));
    element(&mut h, "IlluminanceHdr", "illum", &d);
    let quarter = element(&mut h, "IlluminanceHdr", "illum4", &d);
    ok(&mut h, "Render Element Options", serde_json::json!({ "index": quarter, "scale": 0.25 }));
    let amb = element(&mut h, "Lighting", "ambient", &d);
    ok(&mut h, "Render Element Options", serde_json::json!({ "index": amb, "direct": false }));
    let main = render(&mut h);
    let on = |x: u32, y: u32| main.get_pixel(x, y).0[3] == 255;
    let spread = |img: &image::RgbaImage| {
        let v: Vec<u8> = img.enumerate_pixels().filter(|(x, y, _)| on(*x, *y)).map(|(_, _, p)| p.0[0]).collect();
        *v.iter().max().unwrap() as i32 - *v.iter().min().unwrap() as i32
    };
    let (flat, lit) = (open(&d, "flat"), open(&d, "lit"));
    assert!(spread(&flat) <= 6, "unlit: one colour over the sphere ({})", spread(&flat));
    assert!(spread(&lit) > 60, "lit: bright to dark ({})", spread(&lit));
    assert!((mean(&flat, 0, on) - 200.0).abs() < 4.0 && (mean(&flat, 1, on) - 60.0).abs() < 4.0, "the diffuse colour itself");
    let diff = |a: &image::RgbaImage| mean(&image::RgbaImage::from_fn(160, 120, |x, y| image::Rgba([(a.get_pixel(x, y).0[0] as i32 - main.get_pixel(x, y).0[0] as i32).unsigned_abs() as u8, 0, 0, 255])), 0, on);
    let (all, highlights) = (diff(&open(&d, "blend")), diff(&open(&d, "spec")));
    assert!(all < 12.0, "every part adds up to the render: {all}");
    assert!(highlights > 4.0 * all, "the highlights alone do not: {highlights} vs {all}");
    let (i1, i4) = (mean(&open(&d, "illum"), 0, on), mean(&open(&d, "illum4"), 0, on));
    assert!(i4 < i1 * 0.75 && i4 > 5.0, "Scale Factor 0.25 is darker: {i4} vs {i1}");
    assert!(mean(&open(&d, "ambient"), 0, on) < i1 * 0.6, "without the omni only the ambient light is left");
}

/// Batch Render: a camera task and a viewport task render in turn into
/// their files; the camera's view differs from the viewport's.
#[test]
fn batch_render_writes_each_task() {
    let Some(mut h) = Harness::with_gpu() else {
        eprintln!("batch_render_writes_each_task: no GPU adapter, skipped");
        return;
    };
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [20.0; 3] });
    framed(&mut h);
    h.cmd(Command::CreateCameraAt { target: false, position: [300.0, 0.0, 40.0], look_at: [0.0; 3] });
    let cam = selected(&h).0;
    let d = tmp("batch");
    ok(&mut h, "Bevy Renderer Settings", serde_json::json!({ "passes": 1 }));
    ok(&mut h, "Render SetUp...", serde_json::json!({ "width": 120, "height": 90 }));
    h.cmd(Command::BatchRender);
    let win = h.ctx.memory(|m| m.area_rect(egui::Id::new("Batch Render"))).expect("the dialog is open");
    click_text(&mut h, "Add...", win, 0.0);
    click_text(&mut h, "Add...", win, 0.0);
    assert_eq!(h.app.state.scene.render.batch.len(), 2, "two clicks, two tasks");
    ok(&mut h, "Batch Render Task", serde_json::json!({ "index": 0, "camera": cam, "output": d.join("cam.png").to_string_lossy() }));
    ok(&mut h, "Batch Render Task", serde_json::json!({ "index": 1, "output": d.join("view.png").to_string_lossy() }));
    h.frames(2);
    // The dialog grew with its two tasks.
    let win = h.ctx.memory(|m| m.area_rect(egui::Id::new("Batch Render"))).unwrap();
    click_text(&mut h, "Render", win, 0.0);
    finish_render(&mut h, 600);
    for _ in 0..600 {
        if h.app.state.rendered.batch.run.is_none() && h.app.state.rendered.progress.is_none() && h.app.state.pending_render.is_none() {
            break;
        }
        h.frames(1);
    }
    assert_eq!(h.app.last_error, None, "{:?}", h.app.state.rendered.messages);
    let (a, b) = (open(&d, "cam"), open(&d, "view"));
    let cover = |img: &image::RgbaImage| img.pixels().filter(|p| p.0[3] > 128).count();
    assert!(cover(&a) > 20 && cover(&b) > 20, "the box is in both: {} {}", cover(&a), cover(&b));
    assert!(cover(&b) > 3 * cover(&a), "the far camera sees it smaller: {} vs {}", cover(&a), cover(&b));
}

/// `loom --render scene.loom --camera Camera001 --output out.png --size 80x60`.
#[test]
fn command_line_render_writes_the_scene_through_a_camera() {
    let args: Vec<String> = ["--render", "s.loom", "--camera", "Cam", "--output", "o.png", "--size", "80x60", "--frames", "2-3"].iter().map(|s| s.to_string()).collect();
    let r = crate::cli_render::parse(&args).unwrap().unwrap();
    assert_eq!((r.size, r.frames, r.camera.as_deref()), (Some([80, 60]), Some([2, 3]), Some("Cam")));
    assert!(crate::cli_render::parse(&["scene.loom".to_owned()]).is_none(), "no --render: the app opens");
    assert!(crate::cli_render::parse(&["--render".to_owned(), "s.loom".to_owned(), "--size".to_owned(), "big".to_owned()]).unwrap().is_err());
    let Some(mut h) = Harness::with_gpu() else {
        eprintln!("command_line_render_writes_the_scene_through_a_camera: no GPU adapter, skipped");
        return;
    };
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [20.0; 3] });
    h.cmd(Command::CreateCameraAt { target: false, position: [80.0, -80.0, 50.0], look_at: [0.0; 3] });
    let name = h.app.state.scene.node(selected(&h)).unwrap().name.clone();
    ok(&mut h, "Bevy Renderer Settings", serde_json::json!({ "passes": 1 }));
    let d = tmp("cli");
    let scene = d.join("scene.loom");
    h.cmd(Command::SaveFileAs { path: Some(scene.to_string_lossy().into()) });
    drop(h);
    let out = d.join("out.png");
    let args = crate::cli_render::RenderArgs { scene: scene.to_string_lossy().into(), camera: Some(name), output: Some(out.to_string_lossy().into()), size: Some([80, 60]), frames: None, preset: None };
    let files = crate::cli_render::run(&args).expect("the render");
    assert_eq!(files, vec![out.to_string_lossy().into_owned()]);
    let img = image::open(&out).unwrap().to_rgba8();
    assert_eq!(img.dimensions(), (80, 60));
    assert!(img.pixels().filter(|p| p.0[3] > 128).count() > 100, "the box through the camera");
    let bad = crate::cli_render::RenderArgs { camera: Some("Nobody".into()), ..args };
    assert!(crate::cli_render::run(&bad).is_err());
}

/// ActiveShade: opening the floater renders the view; changing the
/// object's colour renders it again (Auto Update) in the new colour.
#[test]
fn active_shade_renders_again_when_the_scene_changes() {
    let Some(mut h) = Harness::with_gpu() else {
        eprintln!("active_shade_renders_again_when_the_scene_changes: no GPU adapter, skipped");
        return;
    };
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [30.0; 3] });
    let id = selected(&h);
    h.cmd(Command::SetWireColor { id: id.0, rgb: [220, 30, 30] });
    framed(&mut h);
    ok(&mut h, "Render SetUp...", serde_json::json!({ "width": 120, "height": 90 }));
    h.cmd(Command::ActiveShadeFloater);
    let wait = |h: &mut Harness, n: u32| {
        for _ in 0..400 {
            if h.app.state.rendered.active_shade.renders >= n && h.app.render_job.is_none() && h.app.state.pending_render.is_none() {
                return;
            }
            h.frames(1);
        }
        panic!("ActiveShade did not render ({} renders)", h.app.state.rendered.active_shade.renders);
    };
    wait(&mut h, 1);
    let mean_rgb = |h: &Harness| {
        let img = h.app.state.rendered.active_shade.image.clone().unwrap();
        let px: Vec<[u8; 4]> = img.pixels().filter(|p| p.0[3] == 255).map(|p| p.0).collect();
        assert!(px.len() > 100);
        [0, 1, 2].map(|k| px.iter().map(|p| p[k] as f32).sum::<f32>() / px.len() as f32)
    };
    let first = mean_rgb(&h);
    assert!(first[0] > 2.0 * first[2], "red: {first:?}");
    assert!(h.app.state.rendered.image.is_none(), "the Rendered Frame Window is not used");
    h.cmd(Command::SetWireColor { id: id.0, rgb: [30, 30, 220] });
    wait(&mut h, 2);
    let second = mean_rgb(&h);
    assert!(second[2] > 2.0 * second[0], "blue after the change: {second:?}");
    // Auto Update off: a change waits for Update.
    ok(&mut h, "ActiveShade Command", serde_json::json!({ "action": "Auto Update", "on": false }));
    h.cmd(Command::SetWireColor { id: id.0, rgb: [30, 220, 30] });
    h.frames(20);
    assert_eq!(h.app.state.rendered.active_shade.renders, 2);
    let win = h.ctx.memory(|m| m.area_rect(egui::Id::new("ActiveShade"))).expect("the floater is open");
    click_text(&mut h, "Update", win, 0.0);
    wait(&mut h, 3);
    let third = mean_rgb(&h);
    assert!(third[1] > 2.0 * third[0], "green after Update: {third:?}");
}

/// A camera between a red box (+X) and a blue box (-X): the panorama's
/// centre is red and its left and right edges blue; the file is 2:1.
#[test]
fn panorama_exporter_renders_the_surroundings() {
    let Some(mut h) = Harness::with_gpu() else {
        eprintln!("panorama_exporter_renders_the_surroundings: no GPU adapter, skipped");
        return;
    };
    for (x, rgb) in [(80.0, [230, 20, 20]), (-80.0, [20, 20, 230])] {
        h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [x, 0.0, -30.0], size: [40.0, 120.0, 60.0] });
        let id = selected(&h);
        h.cmd(Command::SetWireColor { id: id.0, rgb });
    }
    h.cmd(Command::CreateCameraAt { target: false, position: [0.0, 0.0, 0.0], look_at: [0.0, 50.0, 0.0] });
    let cam = selected(&h).0;
    ok(&mut h, "Bevy Renderer Settings", serde_json::json!({ "passes": 1, "ambient": [0.3, 0.3, 0.3] }));
    let d = tmp("pano");
    let out = d.join("pano.png");
    ok(&mut h, "Panorama Exporter Render", serde_json::json!({ "camera": cam, "size": 64, "output": out.to_string_lossy() }));
    for _ in 0..600 {
        h.frames(1);
        if h.app.state.rendered.batch.panorama.is_none() && h.app.state.rendered.progress.is_none() && h.app.state.pending_render.is_none() {
            break;
        }
    }
    assert_eq!(h.app.last_error, None, "{:?}", h.app.state.rendered.messages);
    let img = image::open(&out).unwrap().to_rgba8();
    assert_eq!(img.dimensions(), (128, 64));
    let px = |u: f32| img.get_pixel((u * 127.0) as u32, 32).0;
    let (c, l, r) = (px(0.5), px(0.02), px(0.98));
    assert!(c[0] > 60 && c[0] as u32 > 2 * c[2] as u32, "the centre looks at the red box: {c:?}");
    assert!(l[2] > 60 && l[2] as u32 > 2 * l[0] as u32, "the left edge looks at the blue box: {l:?}");
    assert!(r[2] > 60 && r[2] as u32 > 2 * r[0] as u32, "the right edge too: {r:?}");
    assert_eq!(h.app.state.rendered.image.as_ref().map(|i| i.dimensions()), Some((128, 64)), "the window shows it");
    ok(&mut h, "Panorama Viewer", serde_json::json!({ "yaw": 180.0 }));
    h.frames(2);
    assert!(h.ctx.memory(|m| m.area_rect(egui::Id::new("Panorama Viewer"))).is_some(), "the viewer window opens");
}

/// Make Preview renders the frames to _sceneNNNN.png with their list; View
/// Animated Sequence File steps through them with the window's arrows;
/// Rename gives them a name of their own; Toggle UI hides the controls.
#[test]
fn make_preview_view_and_rename_the_sequence() {
    let Some(mut h) = Harness::with_gpu() else {
        eprintln!("make_preview_view_and_rename_the_sequence: no GPU adapter, skipped");
        return;
    };
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [20.0; 3] });
    framed(&mut h);
    let d = tmp("preview");
    ok(&mut h, "Make Preview", serde_json::json!({ "path": d.to_string_lossy(), "start": 0, "end": 2, "percent": 20 }));
    finish_render(&mut h, 400);
    assert_eq!(h.app.last_error, None, "{:?}", h.app.state.rendered.messages);
    for n in 0..3 {
        let p = d.join(format!("_scene{n:04}.png"));
        assert_eq!(image::open(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display())).to_rgba8().dimensions(), (128, 96));
    }
    h.cmd(Command::ViewAnimatedSequenceFile);
    assert_eq!(h.app.state.rendered.viewer.as_ref().map(|v| (v.index, v.files.len())), Some((0, 3)));
    let win = h.ctx.memory(|m| m.area_rect(egui::Id::new("Rendered Frame Window"))).expect("the window is open");
    click_text(&mut h, ">", win, 0.0);
    assert_eq!(h.app.state.rendered.viewer.as_ref().unwrap().index, 1, "the next frame arrow");
    // Toggle UI: the rendering controls go, the image stays.
    assert!(h.text_rect("Area to Render:", win).is_some());
    click_text(&mut h, "Toggle UI", win, 0.0);
    let win = h.ctx.memory(|m| m.area_rect(egui::Id::new("Rendered Frame Window"))).unwrap();
    assert!(h.text_rect("Area to Render:", win).is_none(), "hidden");
    assert!(h.app.state.rendered.ui_hidden);
    ok(&mut h, "Rename Animated Sequence File", serde_json::json!({ "name": "walk" }));
    assert!(d.join("walk0002.png").is_file() && d.join("walk.ifl").is_file());
    assert!(!d.join("_scene0002.png").exists());
    assert_eq!(std::fs::read_to_string(d.join("walk.ifl")).unwrap(), "walk0000.png\nwalk0001.png\nwalk0002.png\n");
}
