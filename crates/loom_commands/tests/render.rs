//! Realistic viewport shading, Render, Render Setup and the Rendered Frame Window.

use loom_commands::render::{Channels, RenderProgress};
use loom_commands::{execute, execute_json, AppState, Command, Dialog, Shading};
use loom_scene::render::RenderArea;
use serde_json::json;
use std::sync::Arc;

#[test]
fn viewport_shading_sets_realistic_on_the_active_viewport() {
    let mut s = AppState::new();
    let r = execute(&mut s, Command::ViewportShading { shading: Shading::Realistic, edged_faces: None }).unwrap();
    assert_eq!(r["shading"], "Realistic");
    assert_eq!(s.active().shading, Shading::Realistic);
    // The JSON path (control socket, MCP) takes the variant name.
    let r = execute_json(&mut s, &json!({ "cmd": "Viewport Shading", "args": { "shading": "Wireframe" } }));
    assert_eq!(r["ok"], true, "{r}");
    let r = execute_json(&mut s, &json!({ "cmd": "Viewport Shading", "args": { "shading": "Realistic" } }));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(s.active().shading, Shading::Realistic);
}

#[test]
fn realistic_shading_is_kept_in_the_loom_file() {
    let mut s = AppState::new();
    execute(&mut s, Command::ViewportShading { shading: Shading::Realistic, edged_faces: None }).unwrap();
    let active = s.active_viewport;
    let path = std::env::temp_dir().join(format!("loom-realistic-{}.loom", std::process::id()));
    execute(&mut s, Command::SaveFileAs { path: Some(path.to_string_lossy().into()) }).unwrap();
    let mut t = AppState::new();
    execute(&mut t, Command::OpenFile { path: Some(path.to_string_lossy().into()) }).unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(t.viewports[active].shading, Shading::Realistic);
}

#[test]
fn wireframe_toggle_leaves_realistic_for_wireframe() {
    let mut s = AppState::new();
    execute(&mut s, Command::ViewportShading { shading: Shading::Realistic, edged_faces: None }).unwrap();
    execute(&mut s, Command::WireframeSmoothHighlightsToggle).unwrap();
    assert_eq!(s.active().shading, Shading::Wireframe);
}

#[test]
fn render_opens_the_rendered_frame_window_instead_of_a_file_dialog() {
    let mut s = AppState::new();
    execute(&mut s, Command::Render { path: None, width: None, height: None }).unwrap();
    assert_eq!(s.dialog, None, "no file dialog");
    assert!(s.rendered.open);
    let req = s.pending_render.clone().expect("a request for the renderer");
    assert_eq!((req.settings.width, req.settings.height), (640, 480));
    assert_eq!(req.save_path, None);
    assert!(!req.iterative);
    assert!(s.rendered.title.starts_with("Perspective, frame 0"), "{}", s.rendered.title);
}

#[test]
fn render_with_a_path_records_where_to_save() {
    let mut s = AppState::new();
    s.active_viewport = 1;
    let r = execute(&mut s, Command::Render { path: Some("/tmp/out.png".into()), width: Some(320), height: Some(200) }).unwrap();
    assert_eq!(r["pending"], true);
    let req = s.pending_render.clone().unwrap();
    assert_eq!((req.viewport, req.settings.width, req.settings.height, req.save_path.as_deref()), (1, 320, 200, Some("/tmp/out.png")));
    // The override is for this render only.
    assert_eq!((s.scene.render.width, s.scene.render.height), (640, 480));
}

#[test]
fn a_second_render_waits_for_the_first() {
    let mut s = AppState::new();
    execute(&mut s, Command::Render { path: None, width: None, height: None }).unwrap();
    assert!(execute(&mut s, Command::RenderIterative).is_err(), "pending request");
    s.pending_render = None;
    s.rendered.progress = Some(RenderProgress { done: 1, total: 4, paused: false });
    assert!(execute(&mut s, Command::Render { path: None, width: None, height: None }).is_err(), "render in progress");
}

#[test]
fn production_saves_the_render_output_file_only_with_save_file() {
    let mut s = AppState::new();
    assert!(execute(&mut s, Command::RenderSetup { width: None, height: None, image_aspect: None, pixel_aspect: None, lock_image_aspect: None, lock_pixel_aspect: None, render_hidden: None, force_two_sided: None, save_file: Some(true), output: None }).is_err(), "Save File needs a file");
    let r = execute_json(&mut s, &json!({ "cmd": "Render SetUp...", "args": { "output": "/tmp/loom-out.png" } }));
    assert_eq!(r["ok"], true, "{r}");
    assert!(s.scene.render.save_file, "choosing Files turns Save File on");
    execute(&mut s, Command::Render { path: None, width: None, height: None }).unwrap();
    assert_eq!(s.pending_render.take().unwrap().save_path.as_deref(), Some("/tmp/loom-out.png"));
    // Iterative never writes files.
    execute(&mut s, Command::RenderIterative).unwrap();
    let req = s.pending_render.take().unwrap();
    assert!(req.iterative && req.save_path.is_none());
    execute_json(&mut s, &json!({ "cmd": "Render SetUp...", "args": { "save_file": false } }));
    execute(&mut s, Command::Render { path: None, width: None, height: None }).unwrap();
    assert_eq!(s.pending_render.take().unwrap().save_path, None);
}

#[test]
fn render_last_repeats_the_last_render_without_saving() {
    let mut s = AppState::new();
    assert!(execute(&mut s, Command::RenderLast).is_err(), "nothing rendered yet");
    s.active_viewport = 2;
    execute(&mut s, Command::Render { path: Some("/tmp/x.png".into()), width: Some(100), height: Some(50) }).unwrap();
    s.pending_render = None;
    s.active_viewport = 0;
    execute(&mut s, Command::RenderLast).unwrap();
    let req = s.pending_render.take().unwrap();
    assert_eq!((req.viewport, req.settings.width, req.save_path), (2, 100, None));
    // F9 is Render Last.
    execute(&mut s, Command::Hotkey { chord: "F9".into() }).unwrap();
    assert!(s.pending_render.is_some());
}

fn setup(s: &mut AppState, args: serde_json::Value) -> serde_json::Value {
    let r = execute_json(s, &json!({ "cmd": "Render SetUp...", "args": args }));
    assert_eq!(r["ok"], true, "{r}");
    r
}

#[test]
fn render_setup_values_are_one_undo_step_each_and_saved_in_the_file() {
    let mut s = AppState::new();
    execute(&mut s, Command::RenderSetup { width: None, height: None, image_aspect: None, pixel_aspect: None, lock_image_aspect: None, lock_pixel_aspect: None, render_hidden: None, force_two_sided: None, save_file: None, output: None }).unwrap();
    assert_eq!(s.dialog, Some(Dialog::RenderSetup));
    setup(&mut s, json!({ "width": 800, "height": 600 }));
    setup(&mut s, json!({ "render_hidden": true, "force_two_sided": true }));
    let r = execute_json(&mut s, &json!({ "cmd": "Bevy Renderer Settings", "args": { "antialiasing": false, "supersampling": 2, "shadows": false, "shadow_map_size": 1024, "background": [0.2, 0.3, 0.4], "ambient": [0.5, 0.5, 0.5] } }));
    assert_eq!(r["ok"], true, "{r}");
    let r = execute_json(&mut s, &json!({ "cmd": "Area to Render", "args": { "area": "Region", "region": [0.1, 0.2, 0.5, 0.6] } }));
    assert_eq!(r["ok"], true, "{r}");
    let saved = s.scene.render.clone();
    assert_eq!((saved.width, saved.height, saved.render_hidden, saved.force_two_sided), (800, 600, true, true));
    assert_eq!((saved.bevy.supersampling, saved.bevy.shadow_map_size, saved.bevy.background), (2, 1024, [0.2, 0.3, 0.4]));
    assert_eq!((saved.area, saved.region), (RenderArea::Region, [0.1, 0.2, 0.5, 0.6]));

    let path = std::env::temp_dir().join(format!("loom-render-settings-{}.loom", std::process::id()));
    execute(&mut s, Command::SaveFileAs { path: Some(path.to_string_lossy().into()) }).unwrap();
    let mut t = AppState::new();
    execute(&mut t, Command::OpenFile { path: Some(path.to_string_lossy().into()) }).unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(t.scene.render, saved);

    for _ in 0..3 {
        execute(&mut s, Command::UndoSceneOperation).unwrap();
    }
    assert_eq!((s.scene.render.width, s.scene.render.render_hidden, s.scene.render.bevy.supersampling), (800, false, 1));
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.render, loom_scene::render::RenderSettings::default());
}

#[test]
fn image_aspect_lock_keeps_width_and_height_together() {
    let mut s = AppState::new();
    setup(&mut s, json!({ "lock_image_aspect": true }));
    setup(&mut s, json!({ "width": 1280 }));
    assert_eq!((s.scene.render.width, s.scene.render.height), (1280, 960));
    setup(&mut s, json!({ "height": 240 }));
    assert_eq!((s.scene.render.width, s.scene.render.height), (320, 240));
    // Image Aspect sets the height; a locked pixel aspect refuses changes.
    setup(&mut s, json!({ "image_aspect": 2.0 }));
    assert_eq!(s.scene.render.height, 160);
    setup(&mut s, json!({ "lock_pixel_aspect": true }));
    let r = execute_json(&mut s, &json!({ "cmd": "Render SetUp...", "args": { "pixel_aspect": 0.9 } }));
    assert_eq!(r["ok"], false, "{r}");
    setup(&mut s, json!({ "lock_pixel_aspect": false, "pixel_aspect": 0.5 }));
    // Image aspect stays 2: the height follows the pixel aspect.
    assert_eq!(s.scene.render.height, 80);
    assert!((s.scene.render.image_aspect() - 2.0).abs() < 1e-3);
}

#[test]
fn render_sizes_and_settings_out_of_range_are_refused() {
    let mut s = AppState::new();
    assert!(execute(&mut s, Command::Render { path: Some("/tmp/a.png".into()), width: Some(0), height: Some(10) }).is_err());
    assert_eq!(execute_json(&mut s, &json!({ "cmd": "Render SetUp...", "args": { "width": 9000 } }))["ok"], false);
    assert_eq!(execute_json(&mut s, &json!({ "cmd": "Bevy Renderer Settings", "args": { "supersampling": 5 } }))["ok"], false);
    assert_eq!(execute_json(&mut s, &json!({ "cmd": "Area to Render", "args": { "region": [0.6, 0.2, 0.5, 0.6] } }))["ok"], false);
    assert_eq!((s.scene.render.width, s.scene.render.height), (640, 480));
    assert!(s.pending_render.is_none());
    assert!(s.undo.is_empty());
}

#[test]
fn blowup_keeps_its_own_rectangle_at_the_output_shape() {
    let mut s = AppState::new();
    execute(&mut s, Command::AreaToRender { area: Some(RenderArea::Blowup), region: Some([0.0, 0.0, 0.5, 0.3]) }).unwrap();
    let b = s.scene.render.blowup;
    assert!(((b[2] - b[0]) - (b[3] - b[1])).abs() < 1e-6, "same fraction of width and height: {b:?}");
    assert_eq!(s.scene.render.region, [0.25, 0.25, 0.75, 0.75], "Region/Crop rectangle untouched");
}

#[test]
fn edit_region_switches_view_to_region() {
    let mut s = AppState::new();
    execute(&mut s, Command::EditRegion).unwrap();
    assert!(s.rendered.edit_region);
    assert_eq!(s.scene.render.area, RenderArea::Region);
    execute(&mut s, Command::AreaToRender { area: Some(RenderArea::View), region: None }).unwrap();
    assert!(!s.rendered.edit_region, "View has no region to edit");
}

#[test]
fn selected_area_needs_a_selection() {
    let mut s = AppState::new();
    execute(&mut s, Command::AreaToRender { area: Some(RenderArea::Selected), region: None }).unwrap();
    assert!(execute(&mut s, Command::Render { path: None, width: None, height: None }).is_err());
    execute(&mut s, Command::CreatePrimitive { kind: loom_commands::PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    execute(&mut s, Command::Render { path: None, width: None, height: None }).unwrap();
    assert_eq!(s.pending_render.unwrap().selection.len(), 1);
}

#[test]
fn render_viewport_lock_keeps_the_chosen_view() {
    let mut s = AppState::new();
    s.active_viewport = 1;
    execute(&mut s, Command::RenderViewport { viewport: None, lock: Some(true), iterative: None }).unwrap();
    s.active_viewport = 3;
    assert_eq!(loom_commands::render::render_viewport(&s), 1, "locked to the viewport active when locking");
    execute(&mut s, Command::RenderViewport { viewport: Some(0), lock: None, iterative: Some(true) }).unwrap();
    assert_eq!((loom_commands::render::render_viewport(&s), s.active_viewport, s.rendered.iterative), (0, 3, true));
    execute(&mut s, Command::RenderViewport { viewport: None, lock: Some(false), iterative: None }).unwrap();
    assert_eq!(loom_commands::render::render_viewport(&s), 3, "unlocked follows the active viewport");
    // Choosing from the list while unlocked activates that viewport.
    execute(&mut s, Command::RenderViewport { viewport: Some(2), lock: None, iterative: None }).unwrap();
    assert_eq!(s.active_viewport, 2);
    assert!(execute(&mut s, Command::RenderViewport { viewport: Some(9), lock: None, iterative: None }).is_err());
}

/// 16 x 8, orange, with one transparent pixel in the far corner (JPEG
/// blends 8 x 8 blocks, so the corner block's colour stays clean).
fn with_image(s: &mut AppState) {
    let mut img = image::RgbaImage::from_pixel(16, 8, image::Rgba([200, 100, 50, 255]));
    img.put_pixel(15, 7, image::Rgba([10, 20, 30, 0]));
    s.rendered.set_image(Some(Arc::new(img)));
}

#[test]
fn save_image_writes_every_format_by_extension() {
    let mut s = AppState::new();
    assert!(execute(&mut s, Command::SaveRenderedImage { path: Some("/tmp/x.png".into()) }).is_err(), "no image yet");
    with_image(&mut s);
    execute(&mut s, Command::SaveRenderedImage { path: None }).unwrap();
    assert_eq!(s.dialog, Some(Dialog::SaveRenderedImage));
    let dir = std::env::temp_dir().join(format!("loom-save-image-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for ext in ["png", "jpg", "bmp", "tga", "exr"] {
        let p = dir.join(format!("frame.{ext}"));
        execute(&mut s, Command::SaveRenderedImage { path: Some(p.to_string_lossy().into()) }).unwrap();
        let back = image::open(&p).unwrap_or_else(|e| panic!("{ext}: {e}"));
        assert_eq!((back.width(), back.height()), (16, 8), "{ext}");
        if ext == "exr" {
            // OpenEXR holds linear light: sRGB 200 is 0.578.
            let px = back.to_rgba32f().get_pixel(0, 0).0;
            assert!((px[0] - 0.578).abs() < 0.01 && (px[3] - 1.0).abs() < 1e-3, "{ext}: {px:?}");
        } else {
            let px = back.to_rgba8().get_pixel(0, 0).0;
            assert!(px[0] > 185 && px[1] > 85 && px[1] < 115 && px[2] < 65, "{ext}: {px:?}");
        }
    }
    let png = image::open(dir.join("frame.png")).unwrap().to_rgba8();
    assert_eq!(png.get_pixel(15, 7).0, [10, 20, 30, 0], "PNG keeps the alpha");
    assert!(execute(&mut s, Command::SaveRenderedImage { path: Some(dir.join("frame.xyz").to_string_lossy().into()) }).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn channels_clone_clear_and_copy() {
    let mut s = AppState::new();
    assert!(execute(&mut s, Command::CloneRenderedFrameWindow).is_err());
    assert!(execute(&mut s, Command::CopyRenderedImage).is_err());
    with_image(&mut s);
    let r = execute_json(&mut s, &json!({ "cmd": "Rendered Frame Window Channels", "args": { "red": false } }));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(s.rendered.channels.display([200, 100, 50, 255]), [0, 100, 50, 255]);
    execute(&mut s, Command::RenderedFrameWindowChannels { red: Some(true), green: None, blue: None, alpha: Some(true), mono: None }).unwrap();
    assert_eq!(s.rendered.channels.display([10, 20, 30, 77]), [77, 77, 77, 255]);
    let mono = Channels { mono: true, ..Default::default() };
    let g = mono.display([200, 100, 50, 255])[0];
    assert_eq!(g, loom_commands::render::mono([200, 100, 50]));
    assert!(g > 100 && g < 140, "{g}");

    execute(&mut s, Command::CopyRenderedImage).unwrap();
    assert!(s.rendered.copy_requested);
    execute(&mut s, Command::CloneRenderedFrameWindow).unwrap();
    execute(&mut s, Command::CloneRenderedFrameWindow).unwrap();
    assert_eq!(s.rendered.clones.len(), 2);
    assert_eq!(s.rendered.clones[1].channels, s.rendered.channels);
    let v = s.rendered.version;
    execute(&mut s, Command::ClearRenderedFrameWindow).unwrap();
    assert!(s.rendered.image.is_none() && s.rendered.version > v);
    assert!(s.rendered.clones[0].image.width() == 16, "clones keep their copy");
}

#[test]
fn cancel_and_pause_act_on_the_render_in_progress() {
    let mut s = AppState::new();
    assert!(execute(&mut s, Command::CancelRender).is_err());
    assert!(execute(&mut s, Command::PauseRender).is_err());
    execute(&mut s, Command::Render { path: None, width: None, height: None }).unwrap();
    execute(&mut s, Command::CancelRender).unwrap();
    assert!(s.pending_render.is_none(), "a request not started yet is dropped");
    s.rendered.progress = Some(RenderProgress { done: 2, total: 6, paused: false });
    execute(&mut s, Command::PauseRender).unwrap();
    assert!(s.rendered.progress.unwrap().paused);
    execute(&mut s, Command::CancelRender).unwrap();
    assert!(s.rendered.cancel);
    let st = execute(&mut s, Command::State).unwrap();
    assert_eq!(st["rendered_frame"]["rendering"]["done"], 2, "{}", st["rendered_frame"]);
}

#[test]
fn render_commands_are_named_actions() {
    let mut s = AppState::new();
    for name in ["Render SetUp...", "Rendered Frame Window", "Render Iterative"] {
        let r = execute_json(&mut s, &json!({ "cmd": name }));
        assert_eq!(r["ok"], true, "{name}: {r}");
    }
    assert_eq!(s.dialog, Some(Dialog::RenderSetup));
    let r = execute_json(&mut s, &json!({ "cmd": "Render", "args": { "path": "/tmp/b.png", "width": 64, "height": 48 } }));
    assert_eq!(r["ok"], false, "the Iterative render is still pending: {r}");
    s.pending_render = None;
    let r = execute_json(&mut s, &json!({ "cmd": "Render", "args": { "path": "/tmp/b.png", "width": 64, "height": 48 } }));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(s.pending_render.as_ref().map(|r| (r.settings.width, r.settings.height)), Some((64, 48)));
}
