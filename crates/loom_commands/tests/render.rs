//! Realistic viewport shading, Render and Render SetUp.

use loom_commands::{execute, execute_json, AppState, Command, Dialog, RenderRequest, Shading};
use serde_json::json;

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
fn render_without_a_path_opens_the_file_dialog() {
    let mut s = AppState::new();
    execute(&mut s, Command::Render { path: None, width: None, height: None }).unwrap();
    assert_eq!(s.dialog, Some(Dialog::RenderFile));
    assert!(s.pending_render.is_none());
}

#[test]
fn render_records_a_request_for_the_active_view() {
    let mut s = AppState::new();
    s.active_viewport = 1;
    let r = execute(&mut s, Command::Render { path: Some("/tmp/out.png".into()), width: Some(320), height: Some(200) }).unwrap();
    assert_eq!(r["pending"], true);
    assert_eq!(s.pending_render, Some(RenderRequest { path: "/tmp/out.png".into(), width: 320, height: 200, viewport: 1 }));
}

#[test]
fn render_setup_sets_the_default_output_size() {
    let mut s = AppState::new();
    assert_eq!(s.render_size, (640, 480));
    execute(&mut s, Command::RenderSetup { width: None, height: None }).unwrap();
    assert_eq!(s.dialog, Some(Dialog::RenderSetup));
    execute(&mut s, Command::RenderSetup { width: Some(800), height: Some(600) }).unwrap();
    assert_eq!(s.render_size, (800, 600));
    execute(&mut s, Command::Render { path: Some("/tmp/a.png".into()), width: None, height: None }).unwrap();
    let req = s.pending_render.clone().unwrap();
    assert_eq!((req.width, req.height), (800, 600));
}

#[test]
fn render_sizes_out_of_range_are_refused() {
    let mut s = AppState::new();
    assert!(execute(&mut s, Command::Render { path: Some("/tmp/a.png".into()), width: Some(0), height: Some(10) }).is_err());
    assert!(execute(&mut s, Command::RenderSetup { width: Some(9000), height: None }).is_err());
    assert_eq!(s.render_size, (640, 480));
    assert!(s.pending_render.is_none());
}

#[test]
fn render_and_render_setup_are_named_actions() {
    let mut s = AppState::new();
    let r = execute_json(&mut s, &json!({ "cmd": "Render SetUp..." }));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(s.dialog, Some(Dialog::RenderSetup));
    let r = execute_json(&mut s, &json!({ "cmd": "Render", "args": { "path": "/tmp/b.png", "width": 64, "height": 48 } }));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(s.pending_render.as_ref().map(|r| (r.width, r.height)), Some((64, 48)));
}
