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
}
