//! Render output options and gamma, Output Size formats, the renderer's
//! log file, View Image File, Object Channel, Batch Render, Panorama
//! Exporter projection, Make Preview and the AmbientOcclusionMap settings.

use loom_commands::render::{save_image_as, save_output};
use loom_commands::render_output::{cube_to_equirect, panorama_view, sequence_files, GBuffer, GChannel};
use loom_commands::{execute, execute_json, AppState, Command, PrimitiveKind};
use loom_scene::render::{OutputFormat, PngType};
use serde_json::json;
use std::sync::Arc;

fn tmp(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("loom-render3-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn ok(s: &mut AppState, cmd: &str, args: serde_json::Value) -> serde_json::Value {
    let r = execute_json(s, &json!({ "cmd": cmd, "args": args }));
    assert_eq!(r["ok"], true, "{cmd}: {r}");
    if r.get("result").is_some() { r["result"].clone() } else { r }
}

/// A gradient with a half-transparent band: detail for JPEG, alpha for the rest.
fn picture() -> image::RgbaImage {
    image::RgbaImage::from_fn(64, 48, |x, y| image::Rgba([(x * 4) as u8, (y * 5) as u8, ((x * y) % 256) as u8, if y < 16 { 128 } else { 255 }]))
}

#[test]
fn jpeg_quality_trades_file_size_for_fidelity() {
    let d = tmp("jpeg");
    let img = picture();
    let size = |q: u8| {
        let p = d.join(format!("q{q}.jpg"));
        save_image_as(&img, p.to_str().unwrap(), &OutputFormat { jpeg_quality: q, ..Default::default() }, None).unwrap();
        let back = image::open(&p).unwrap().to_rgb8();
        let err: u64 = back.pixels().zip(img.pixels()).map(|(a, b)| (0..3).map(|k| (a.0[k] as i64 - b.0[k] as i64).unsigned_abs()).sum::<u64>()).sum();
        (std::fs::metadata(&p).unwrap().len(), err)
    };
    let (small, rough) = size(10);
    let (big, fine) = size(100);
    assert!(small < big, "quality 10 writes a smaller file: {small} vs {big}");
    assert!(fine < rough, "quality 100 is closer to the image: {fine} vs {rough}");
}

#[test]
fn png_setup_writes_its_colour_type_and_alpha() {
    let d = tmp("png");
    let img = picture();
    let write = |t: PngType, alpha: bool| {
        let p = d.join(format!("{t:?}{alpha}.png"));
        save_image_as(&img, p.to_str().unwrap(), &OutputFormat { png_type: t, png_alpha: alpha, ..Default::default() }, None).unwrap();
        image::open(&p).unwrap()
    };
    assert_eq!(write(PngType::Rgb24, true).color(), image::ColorType::Rgba8);
    assert_eq!(write(PngType::Rgb24, false).color(), image::ColorType::Rgb8);
    let deep = write(PngType::Rgb48, false);
    assert_eq!(deep.color(), image::ColorType::Rgb16);
    // 8-bit values widened to 16 bits keep their value.
    assert_eq!(deep.to_rgb16().get_pixel(10, 20).0[0], 40 * 257);
    assert_eq!(write(PngType::Gray8, false).color(), image::ColorType::L8);
    let g = write(PngType::Gray16, true);
    assert_eq!(g.color(), image::ColorType::La16);
    assert_eq!(g.to_luma_alpha16().get_pixel(0, 0).0[1], 128 * 257, "the alpha survives");
}

#[test]
fn targa_setup_bits_compression_alpha_split_and_premultiplied() {
    let d = tmp("tga");
    // Flat colour: run-length encoding shrinks it.
    let flat = image::RgbaImage::from_pixel(64, 64, image::Rgba([200, 100, 50, 128]));
    let write = |name: &str, f: OutputFormat| {
        let p = d.join(name);
        save_image_as(&flat, p.to_str().unwrap(), &f, None).unwrap();
        p
    };
    let raw = write("raw.tga", OutputFormat { tga_compress: false, ..Default::default() });
    let rle = write("rle.tga", OutputFormat { tga_compress: true, ..Default::default() });
    assert!(std::fs::metadata(&rle).unwrap().len() * 4 < std::fs::metadata(&raw).unwrap().len());
    let rgb = write("rgb.tga", OutputFormat { tga_bits: 24, ..Default::default() });
    assert_eq!(image::open(&rgb).unwrap().color(), image::ColorType::Rgb8);
    assert_eq!(image::open(&raw).unwrap().color(), image::ColorType::Rgba8);
    let split = write("split.tga", OutputFormat { tga_alpha_split: true, ..Default::default() });
    let alpha = image::open(split.with_file_name("a_split.tga")).expect("Alpha Split writes a_split.tga").to_luma8();
    assert_eq!(alpha.get_pixel(5, 5).0[0], 128);
    let pre = write("pre.tga", OutputFormat { tga_premultiplied: true, ..Default::default() });
    let p = image::open(&pre).unwrap().to_rgba8().get_pixel(1, 1).0;
    assert_eq!(&p[..3], &[100, 50, 25], "colour times alpha 128/255");
}

#[test]
fn openexr_setup_half_or_full_float_rgba_or_rgb() {
    use exr::meta::attribute::SampleType;
    let d = tmp("exr");
    let img = picture();
    let channels = |full: bool, alpha: bool| {
        let p = d.join(format!("{full}{alpha}.exr"));
        save_image_as(&img, p.to_str().unwrap(), &OutputFormat { exr_full_float: full, exr_alpha: alpha, ..Default::default() }, None).unwrap();
        let meta = exr::meta::MetaData::read_from_file(&p, false).unwrap();
        meta.headers[0].channels.list.iter().map(|c| (c.name.to_string(), c.sample_type)).collect::<Vec<_>>()
    };
    let half = channels(false, true);
    assert_eq!(half.len(), 4);
    assert!(half.iter().all(|c| c.1 == SampleType::F16), "{half:?}");
    let full = channels(true, false);
    assert_eq!(full.len(), 3, "RGB: no alpha {full:?}");
    assert!(full.iter().all(|c| c.1 == SampleType::F32));
    // The image reads back as linear light: sRGB 40 is about 0.021.
    let back = image::open(d.join("falsetrue.exr")).unwrap().to_rgba32f();
    assert!((back.get_pixel(10, 20).0[0] - 0.0212).abs() < 0.001, "{:?}", back.get_pixel(10, 20));
}

#[test]
fn output_gamma_reencodes_saved_renders() {
    let d = tmp("gamma");
    let mut s = AppState::new();
    let grey = image::RgbaImage::from_pixel(4, 4, image::Rgba([128, 128, 128, 255]));
    let saved = |s: &AppState, name: &str| {
        let p = d.join(name);
        save_output(&grey, p.to_str().unwrap(), &s.scene.render).unwrap();
        image::open(&p).unwrap().to_rgba8().get_pixel(0, 0).0[0]
    };
    assert_eq!(saved(&s, "off.png"), 128, "correction off: as rendered");
    ok(&mut s, "Gamma and LUT Settings", json!({ "enabled": true }));
    assert_eq!(saved(&s, "system.png"), 128, "output gamma 2.2: as rendered");
    let r = ok(&mut s, "Gamma and LUT Settings", json!({ "override_gamma": 1.0 }));
    assert_eq!(r["applied"], 1.0);
    // 128 at display gamma 2.2 is linear 0.2195; written for gamma 1.0.
    assert_eq!(saved(&s, "override.png"), 56);
    ok(&mut s, "Gamma and LUT Settings", json!({ "use_system": true, "output_gamma": 1.8 }));
    let v = saved(&s, "1.8.png");
    assert!(v > 100 && v < 128, "a gamma of 1.8 writes the midtones darker: {v}");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.render.gamma.override_output, Some(1.0), "one undo step");
}

#[test]
fn output_size_formats_set_aspect_size_and_aperture() {
    let mut s = AppState::new();
    let r = ok(&mut s, "Output Size Format", json!({ "format": "HDTV (video)" }));
    assert_eq!((r["width"].as_u64(), r["height"].as_u64()), (Some(1920), Some(1080)));
    let r = ok(&mut s, "Output Size Format", json!({ "format": "NTSC D-1 (video)" }));
    assert_eq!((r["width"].as_u64(), r["height"].as_u64()), (Some(720), Some(486)));
    assert!((r["image_aspect"].as_f64().unwrap() - 4.0 / 3.0).abs() < 1e-3, "{r}");
    let r = ok(&mut s, "Output Size Format", json!({ "format": "PAL (video)" }));
    assert_eq!((r["width"].as_u64(), r["height"].as_u64()), (Some(720), Some(576)));
    assert!((r["image_aspect"].as_f64().unwrap() - 4.0 / 3.0).abs() < 1e-3, "{r}");
    // A film format keeps the width; the height follows its aspect, the aperture is its gate.
    ok(&mut s, "Render SetUp...", json!({ "width": 1000, "lock_image_aspect": false }));
    let r = ok(&mut s, "Output Size Format", json!({ "format": "35mm 1.85:1 (cine)" }));
    assert_eq!((r["width"].as_u64(), r["height"].as_u64()), (Some(1000), Some(541)));
    assert!((r["aperture_width"].as_f64().unwrap() - 21.946).abs() < 1e-3);
    let e = execute_json(&mut s, &json!({ "cmd": "Output Size Format", "args": { "aperture_width": 50.0 } }));
    assert_eq!(e["ok"], false, "a format's aperture is fixed");
    ok(&mut s, "Output Size Format", json!({ "format": "Custom", "aperture_width": 50.0 }));
    assert_eq!(s.scene.render.aperture_width, 50.0);
    assert_eq!(s.scene.render.film_format, None);
    assert!(execute_json(&mut s, &json!({ "cmd": "Output Size Format", "args": { "format": "Super 8" } }))["ok"] == false);
}

#[test]
fn aperture_width_changes_the_lens_reading_not_the_view() {
    use loom_commands::render_output::{fov_for, lens_for};
    // The reference's example: a 43 mm lens reads 59.722 mm at a 50 mm aperture.
    let fov = loom_scene::camera::lens_to_fov(43.0);
    assert!((lens_for(fov, 36.0) - 43.0).abs() < 1e-3);
    assert!((lens_for(fov, 50.0) - 59.722).abs() < 1e-2, "{}", lens_for(fov, 50.0));
    assert!((fov_for(59.722, 50.0) - fov).abs() < 1e-2);
}

#[test]
fn renderer_messages_go_to_the_log_file() {
    let d = tmp("log");
    let log = d.join("render.log");
    std::fs::write(&log, "old session\n").unwrap();
    let mut s = AppState::new();
    assert!(execute_json(&mut s, &json!({ "cmd": "Render Message Log", "args": { "write": true } }))["ok"] == false, "needs a file");
    ok(&mut s, "Render Message Log", json!({ "path": log.to_string_lossy(), "write": true }));
    loom_commands::render::message(&mut s, loom_commands::render::MessageLevel::Info, "first");
    loom_commands::render::message(&mut s, loom_commands::render::MessageLevel::Error, "second");
    let text = std::fs::read_to_string(&log).unwrap();
    assert_eq!(text, "INFO: first\nERROR: second\n", "the session's first message starts the file anew");
    // Append to File keeps what is there.
    ok(&mut s, "Render Message Log", json!({ "path": log.to_string_lossy(), "append": true }));
    loom_commands::render::message(&mut s, loom_commands::render::MessageLevel::Warning, "third");
    assert!(std::fs::read_to_string(&log).unwrap().ends_with("ERROR: second\nWARNING: third\n"));
    ok(&mut s, "Render Message Log", json!({ "write": false }));
    loom_commands::render::message(&mut s, loom_commands::render::MessageLevel::Info, "not logged");
    assert!(!std::fs::read_to_string(&log).unwrap().contains("not logged"));
}

#[test]
fn view_image_file_steps_through_a_numbered_sequence() {
    let d = tmp("view");
    for n in [3, 1, 2] {
        image::RgbaImage::from_pixel(8, 6, image::Rgba([n * 50, 0, 0, 255])).save(d.join(format!("shot{n:04}.png"))).unwrap();
    }
    image::RgbaImage::from_pixel(8, 6, image::Rgba([9, 9, 9, 255])).save(d.join("other0001.png")).unwrap();
    let path = d.join("shot0002.png");
    let (files, at) = sequence_files(path.to_str().unwrap(), true).unwrap();
    assert_eq!(files.len(), 3, "{files:?}");
    assert_eq!(at, 1);
    let mut s = AppState::new();
    let r = ok(&mut s, "View Image File", json!({ "path": path.to_string_lossy(), "sequence": true }));
    assert_eq!((r["index"].as_u64(), r["count"].as_u64()), (Some(1), Some(3)));
    assert!(s.rendered.open);
    let red = |s: &AppState| s.rendered.image.as_ref().unwrap().get_pixel(0, 0).0[0];
    assert_eq!(red(&s), 100);
    ok(&mut s, "View Image File Step", json!({ "step": 1 }));
    assert_eq!(red(&s), 150);
    ok(&mut s, "View Image File Step", json!({ "step": 5 }));
    assert_eq!(red(&s), 150, "the last frame stays");
    ok(&mut s, "View Image File Step", json!({ "index": 0 }));
    assert_eq!(red(&s), 50);
    // Without Sequence: the file alone; a .ifl lists its own.
    ok(&mut s, "View Image File", json!({ "path": path.to_string_lossy() }));
    assert_eq!(s.rendered.viewer.as_ref().unwrap().files.len(), 1);
    std::fs::write(d.join("list.ifl"), "shot0003.png\nshot0001.png\n").unwrap();
    ok(&mut s, "View Image File", json!({ "path": d.join("list.ifl").to_string_lossy() }));
    assert_eq!(red(&s), 150);
    // Without a path: the file dialog.
    ok(&mut s, "View Image File", json!({}));
    assert_eq!(s.dialog, Some(loom_commands::Dialog::ViewImageFile));
}

#[test]
fn g_buffer_channels_show_depth_ids_and_normals() {
    let g = GBuffer { size: [3, 1], depth: vec![Some(10.0), Some(20.0), None], object: vec![5, 7, 0], node: vec![1, 2, 0], normal: vec![[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0; 3]] };
    let z = g.image(GChannel::ZDepth, None);
    assert_eq!(z.get_pixel(0, 0).0[0], 255, "nearest white");
    assert_eq!(z.get_pixel(1, 0).0[0], 0, "farthest black");
    let o = g.image(GChannel::ObjectId, None);
    assert_ne!(o.get_pixel(0, 0).0, o.get_pixel(1, 0).0, "IDs 5 and 7 differ in colour");
    assert_eq!(&o.get_pixel(2, 0).0[..3], &[0, 0, 0]);
    assert_eq!(loom_commands::render_output::id_color(5), loom_commands::render_output::id_color(5), "an ID always has its colour");
    let n = g.image(GChannel::Normal, None);
    assert_eq!(&n.get_pixel(0, 0).0[..3], &[128, 128, 255]);
    let mut s = AppState::new();
    assert!(execute_json(&mut s, &json!({ "cmd": "Rendered Frame Window G-Buffer Channel", "args": { "channel": "ZDepth" } }))["ok"] == false, "nothing rendered");
    s.rendered.gbuffer = Some(Arc::new(g));
    ok(&mut s, "Rendered Frame Window G-Buffer Channel", json!({ "channel": "ObjectId" }));
    assert_eq!(s.rendered.gchannel, Some(GChannel::ObjectId));
    ok(&mut s, "Rendered Frame Window Toggle UI", json!({}));
    assert!(s.rendered.ui_hidden);
}

#[test]
fn object_channel_is_set_on_the_selection_with_undo() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    let id = *s.selection.iter().next().unwrap();
    let r = ok(&mut s, "Set Object Channel", json!({ "channel": 7 }));
    assert_eq!(r["objects"], 1);
    assert_eq!(s.scene.node(id).unwrap().render.object_channel, 7);
    assert!(execute_json(&mut s, &json!({ "cmd": "Set Object Channel", "args": { "channel": 70000 } }))["ok"] == false);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.node(id).unwrap().render.object_channel, 0);
}

#[test]
fn render_element_options_set_each_elements_rollout() {
    let mut s = AppState::new();
    for k in ["Diffuse", "Lighting", "Matte", "Blend", "IlluminanceHdr"] {
        ok(&mut s, "Add Render Element", json!({ "kind": k }));
    }
    ok(&mut s, "Render Element Options", json!({ "index": 0, "lighting": false }));
    ok(&mut s, "Render Element Options", json!({ "index": 1, "direct": false, "shadows": false }));
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    let id = s.selection.iter().next().unwrap().0;
    ok(&mut s, "Render Element Options", json!({ "index": 2, "matte_gbuffer": 3, "matte_objects": [id] }));
    ok(&mut s, "Render Element Options", json!({ "index": 3, "blend_parts": ["Diffuse", "Shadows"] }));
    ok(&mut s, "Render Element Options", json!({ "index": 4, "scale": 0.25 }));
    let e = &s.scene.render.elements;
    assert!(!e[0].lighting);
    assert!(!e[1].direct && e[1].indirect && !e[1].shadows);
    assert_eq!((e[2].matte_gbuffer, e[2].matte_objects.clone()), (Some(3), vec![id]));
    assert!(e[3].blend.diffuse && e[3].blend.shadows && !e[3].blend.specular && !e[3].blend.reflection);
    assert_eq!(e[4].scale, 0.25);
    assert!(execute_json(&mut s, &json!({ "cmd": "Render Element Options", "args": { "index": 3, "blend_parts": ["Glow"] } }))["ok"] == false);
    ok(&mut s, "Render Element Options", json!({ "index": 2, "matte_gbuffer": -1 }));
    assert_eq!(s.scene.render.elements[2].matte_gbuffer, None);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.render.elements[2].matte_gbuffer, Some(3), "one undo step each");
}

fn camera(s: &mut AppState, at: [f32; 3]) -> u64 {
    let r = execute(s, Command::CreateCameraAt { target: false, position: at, look_at: [0.0; 3] }).unwrap();
    s.scene.nodes().filter(|n| matches!(n.object, loom_scene::ObjectKind::Camera { .. })).map(|n| n.id.0).max().unwrap_or_else(|| panic!("no camera: {r}"))
}

#[test]
fn batch_render_queues_tasks_warns_then_renders_them_in_turn() {
    let d = tmp("batch");
    let mut s = AppState::new();
    let cam = camera(&mut s, [100.0, -100.0, 60.0]);
    assert!(execute_json(&mut s, &json!({ "cmd": "Batch Render Run", "args": {} }))["ok"] == false, "an empty queue");
    ok(&mut s, "Batch Render Add", json!({}));
    ok(&mut s, "Batch Render Add", json!({}));
    let out = d.join("cam.png");
    ok(&mut s, "Batch Render Task", json!({ "index": 0, "camera": cam, "output": out.to_string_lossy() }));
    ok(&mut s, "Batch Render Task", json!({ "index": 1, "override_preset": true, "frames": [2, 4], "width": 80, "height": 60 }));
    assert!(execute_json(&mut s, &json!({ "cmd": "Batch Render Task", "args": { "index": 1, "camera": 999 } }))["ok"] == false);
    ok(&mut s, "Batch Render Add", json!({ "duplicate": 1 }));
    assert_eq!(s.scene.render.batch[2].frames, [2, 4]);
    ok(&mut s, "Batch Render Delete", json!({ "index": 2 }));
    // Task 1 has no output: the warning lists it and nothing starts.
    let r = ok(&mut s, "Batch Render Run", json!({}));
    assert_eq!(r["started"], false);
    assert!(r["warning"][0].as_str().unwrap().contains("no output path"), "{r}");
    assert!(s.pending_render.is_none());
    // An output folder that does not exist: an error until Update Path gives one.
    ok(&mut s, "Batch Render Task", json!({ "index": 1, "output": "/nonexistent-loom-dir/seq.png" }));
    assert!(execute_json(&mut s, &json!({ "cmd": "Batch Render Run", "args": { "confirm": true } }))["ok"] == false);
    let r = ok(&mut s, "Batch Render Run", json!({ "confirm": true, "update_path": d.to_string_lossy() }));
    assert_eq!(r["started"], true, "{r}");
    assert_eq!(s.scene.render.batch[1].output.as_deref(), Some(d.join("seq.png").to_str().unwrap()));
    // The first task renders through its camera at Render Setup's size.
    let req = s.pending_render.take().expect("the first task's render");
    assert_eq!(req.camera.map(|c| c.0), Some(cam));
    assert_eq!(req.save_path.as_deref(), Some(out.to_str().unwrap()));
    // Its render done: the second task's frames 2 to 4 follow, numbered, at 80 x 60.
    assert!(loom_commands::render::sequence_next(&mut s).is_some());
    let req = s.pending_render.take().expect("task 2, frame 2");
    assert_eq!((req.settings.width, req.settings.height, req.camera), (80, 60, None));
    assert_eq!(s.current_frame, 2);
    assert!(req.save_path.unwrap().ends_with("seq0002.png"));
    for frame in [3, 4] {
        assert!(loom_commands::render::sequence_next(&mut s).is_some_and(|r| r.is_ok()));
        assert_eq!(s.current_frame, frame);
        s.pending_render = None;
    }
    assert!(loom_commands::render::sequence_next(&mut s).is_none(), "the queue is done");
    assert!(s.rendered.batch.run.is_none());
    assert_eq!(s.current_frame, 0, "the time slider is back");
}

/// Six faces, one colour each: the spherical panorama shows each where it looks.
#[test]
fn panorama_faces_join_into_a_spherical_image() {
    let colours = [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0], [255, 255, 255], [0, 0, 0]];
    let faces: Vec<Arc<image::RgbaImage>> = colours.iter().map(|c| Arc::new(image::RgbaImage::from_pixel(16, 16, image::Rgba([c[0], c[1], c[2], 255])))).collect();
    let pano = cube_to_equirect(&faces, 64);
    assert_eq!(pano.dimensions(), (128, 64));
    let at = |u: f32, v: f32| pano.get_pixel((u * 128.0) as u32, (v * 64.0) as u32).0;
    assert_eq!(&at(0.5, 0.5)[..3], &[255, 0, 0], "the centre looks along +X");
    assert_eq!(&at(0.0, 0.5)[..3], &[0, 255, 0], "the edges look along -X");
    assert_eq!(&at(0.25, 0.5)[..3], &[0, 0, 255], "left of centre is +Y");
    assert_eq!(&at(0.75, 0.5)[..3], &[255, 255, 0], "right of centre is -Y");
    assert_eq!(&at(0.5, 0.02)[..3], &[255, 255, 255], "the top row looks up");
    assert_eq!(&at(0.5, 0.98)[..3], &[0, 0, 0], "the bottom row looks down");
    // The viewer turned 90 degrees left looks at +Y.
    let v = panorama_view(&pano, 90.0, 0.0, 60.0, 40, 30);
    assert_eq!(&v.get_pixel(20, 15).0[..3], &[0, 0, 255]);
    let v = panorama_view(&pano, 0.0, 80.0, 30.0, 40, 30);
    assert_eq!(&v.get_pixel(20, 15).0[..3], &[255, 255, 255], "pitched up");
    let mut s = AppState::new();
    assert!(execute_json(&mut s, &json!({ "cmd": "Panorama Viewer", "args": {} }))["ok"] == false, "no panorama yet");
    let d = tmp("pano");
    pano.save(d.join("p.png")).unwrap();
    let r = ok(&mut s, "Panorama Viewer", json!({ "path": d.join("p.png").to_string_lossy(), "yaw": -90.0 }));
    assert_eq!(r["yaw"], 270.0);
    assert!(s.panorama.open);
    assert!(execute_json(&mut s, &json!({ "cmd": "Panorama Viewer", "args": { "fov": 200.0 } }))["ok"] == false, "10 to 120 degrees");
    assert!(execute_json(&mut s, &json!({ "cmd": "Panorama Exporter Render", "args": { "camera": 12345 } }))["ok"] == false, "not a camera");
    let cam = camera(&mut s, [10.0, 0.0, 0.0]);
    ok(&mut s, "Panorama Exporter Render", json!({ "camera": cam, "size": 64 }));
    let req = s.pending_render.as_ref().expect("the first face");
    assert_eq!((req.cube_face, req.settings.width, req.settings.height), (Some(0), 64, 64));
}

#[test]
fn ambient_occlusion_settings_spread_distance_and_colours() {
    let mut s = AppState::new();
    let _roof = execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0, 0.0, 30.0], size: [40.0, 40.0, 0.0] }).unwrap();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0; 3], size: [40.0, 40.0, 0.0] }).unwrap();
    let d = tmp("ao");
    ok(&mut s, "Render To Texture Settings", json!({ "output_dir": d.to_string_lossy(), "render_to_files_only": true }));
    execute(&mut s, Command::AddTextureElement { element: "AmbientOcclusionMap".into() }).unwrap();
    let index = s.scene.render.bake.elements.len() - 1;
    ok(&mut s, "Texture Element Settings", json!({ "index": index, "width": 16, "height": 16, "format": "png" }));
    assert!(execute_json(&mut s, &json!({ "cmd": "Ambient Occlusion Map Settings", "args": { "index": 99 } }))["ok"] == false);
    let mean = |s: &mut AppState| {
        let r = execute(s, Command::BakeTextures).unwrap();
        let img = image::open(r["written"][0].as_str().unwrap()).unwrap().to_rgba8();
        let px: Vec<[u8; 4]> = img.pixels().filter(|p| p.0[3] > 0).map(|p| p.0).collect();
        let m = |k: usize| px.iter().map(|p| p[k] as f32).sum::<f32>() / px.len() as f32;
        [m(0), m(1), m(2)]
    };
    // Spread 0: every ray goes straight up into the roof.
    ok(&mut s, "Ambient Occlusion Map Settings", json!({ "index": index, "spread": 0.0, "samples": 8 }));
    assert!(mean(&mut s)[0] < 5.0, "fully occluded");
    // Max Distance shorter than the gap: the roof no longer counts.
    ok(&mut s, "Ambient Occlusion Map Settings", json!({ "index": index, "max_distance": 10.0 }));
    assert!(mean(&mut s)[0] > 250.0, "open");
    // Bright and Dark colours.
    ok(&mut s, "Ambient Occlusion Map Settings", json!({ "index": index, "max_distance": 0.0, "dark": [0.0, 0.0, 1.0], "bright": [1.0, 0.0, 0.0] }));
    let m = mean(&mut s);
    assert!(m[2] > 250.0 && m[0] < 5.0, "occluded texels take the Dark colour: {m:?}");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.render.bake.elements[index].ao_dark, [0.0; 3]);
}

#[test]
fn camera_cone_has_the_render_aspect() {
    let mut s = AppState::new();
    let cam = camera(&mut s, [0.0, -100.0, 0.0]);
    let width_over_height = |s: &AppState| {
        let lines = loom_commands::camera::overlay_lines(&s.scene, loom_scene::NodeId(cam), true);
        let cone: Vec<_> = lines.iter().filter(|l| l.2 == loom_commands::camera::LineKind::Cone).collect();
        let pts: Vec<glam::Vec3> = cone.iter().flat_map(|l| [l.0, l.1]).filter(|p| (p.y - -100.0).abs() > 1.0).collect();
        let (lo, hi) = pts.iter().fold((glam::Vec3::splat(f32::MAX), glam::Vec3::splat(f32::MIN)), |(a, b), p| (a.min(*p), b.max(*p)));
        (hi.x - lo.x) / (hi.z - lo.z)
    };
    assert!((width_over_height(&s) - 4.0 / 3.0).abs() < 0.02);
    ok(&mut s, "Render SetUp...", json!({ "width": 1920, "height": 1080 }));
    assert!((width_over_height(&s) - 16.0 / 9.0).abs() < 0.02, "{}", width_over_height(&s));
}

#[test]
fn make_preview_renders_a_numbered_sequence_with_its_list() {
    let d = tmp("preview");
    let mut s = AppState::new();
    let r = ok(&mut s, "Make Preview", json!({ "path": d.to_string_lossy(), "start": 0, "end": 4, "every_nth": 2, "percent": 50 }));
    assert_eq!(r["frames"], json!([0, 2, 4]));
    assert_eq!((r["width"].as_u64(), r["height"].as_u64()), (Some(320), Some(240)));
    assert!(r["path"].as_str().unwrap().ends_with("_scene.ifl"));
    let list = std::fs::read_to_string(d.join("_scene.ifl")).unwrap();
    assert_eq!(list, "_scene0000.png\n_scene0002.png\n_scene0004.png\n");
    let req = s.pending_render.as_ref().expect("the first preview frame renders");
    assert_eq!((req.settings.width, req.settings.bevy.passes), (320, 1));
    assert!(req.save_path.as_ref().unwrap().ends_with("_scene0000.png"));
}

#[test]
fn configure_preset_sets_a_size_button() {
    let mut s = AppState::new();
    let r = ok(&mut s, "Configure Preset", json!({ "button": 2, "width": 720, "height": 486, "pixel_aspect": 0.9 }));
    assert_eq!((r["width"].as_u64(), r["height"].as_u64()), (Some(720), Some(486)));
    assert_eq!(s.prefs.size_presets[2], (720, 486, 0.9));
    ok(&mut s, "Render SetUp...", json!({ "width": 400, "height": 300 }));
    ok(&mut s, "Configure Preset", json!({ "button": 0, "get_current": true }));
    assert_eq!(s.prefs.size_presets[0], (400, 300, 1.0), "Get Current Settings");
    assert!(execute_json(&mut s, &json!({ "cmd": "Configure Preset", "args": { "button": 4 } }))["ok"] == false);
}

#[test]
fn ram_player_configuration_resizes_and_splits_the_alpha() {
    let d = tmp("ram");
    let p = d.join("a.png");
    image::RgbaImage::from_fn(40, 20, |x, _| image::Rgba([200, 10, 10, (x * 6) as u8])).save(&p).unwrap();
    let mut s = AppState::new();
    ok(&mut s, "RAM Player Configuration", json!({ "width": 20, "height": 99, "lock_aspect": true, "filter": true, "alpha_to_other": true }));
    let r = ok(&mut s, "RAM Player Open", json!({ "channel": 0, "path": p.to_string_lossy() }));
    assert_eq!((r["width"].as_u64(), r["height"].as_u64()), (Some(20), Some(10)), "the width sets the size, the aspect kept");
    let other = s.rendered.ram_player.channels[1].clone().expect("the alpha loads into channel B");
    let a = other.1.get_pixel(10, 5).0;
    assert_eq!(a[0], a[1]);
    assert!((a[0] as i32 - 123).abs() < 8, "the alpha as grey: {a:?}");
    ok(&mut s, "RAM Player Configuration", json!({ "original": true, "alpha_to_other": false, "lock_aspect": false }));
    let r = ok(&mut s, "RAM Player Open", json!({ "channel": 0, "path": p.to_string_lossy() }));
    assert_eq!((r["width"].as_u64(), r["height"].as_u64()), (Some(40), Some(20)));
}
