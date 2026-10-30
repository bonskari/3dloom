//! Rendering > Environment and Effects (8): the dialog driven like a
//! person would, Render Setup's Atmospherics / Effects, Use Environment
//! Background, and the environment reaching a render.

use super::{click_text, finish_render, Harness};
use crate::dialogs::environment::TITLE;
use eframe::egui;
use loom_commands::environment::EnvironmentTab;
use loom_commands::Command;
use loom_scene::environment::{AtmosphereKind, EffectKind, ExposureKind};

fn window(h: &Harness, title: &str) -> egui::Rect {
    h.ctx.memory(|m| m.area_rect(egui::Id::new(title))).unwrap_or_else(|| panic!("the {title} window is shown"))
}

#[test]
fn eight_opens_the_dialog_and_its_controls_edit_the_environment() {
    let mut h = Harness::new();
    h.key(egui::Key::Num8);
    assert_eq!(h.app.state.environment_ui.open, Some(EnvironmentTab::Environment));
    h.frames(2);
    let win = window(&h, TITLE);

    // Exposure Control drop-down.
    click_text(&mut h, ExposureKind::None.label(), win, 0.0);
    let full = h.screen;
    click_text(&mut h, ExposureKind::Linear.label(), full, 0.0);
    assert_eq!(h.app.state.scene.environment.exposure.kind, ExposureKind::Linear);
    // Room for the atmosphere rollouts: fold the two rollouts above them.
    h.frames(12);
    click_text(&mut h, "Common Parameters", win, 0.0);
    h.frames(12);
    click_text(&mut h, "Exposure Control", win, 0.0);
    let win = window(&h, TITLE);

    // Atmosphere > Add... > Fog > OK.
    click_text(&mut h, "Add...", win, 0.0);
    // Apart in time, so the clicks are not a double click (which adds at once).
    h.frames(12);
    let add = window(&h, "Add Atmospheric Effect");
    click_text(&mut h, "Fog", add, 0.0);
    h.frames(12);
    let add = window(&h, "Add Atmospheric Effect");
    click_text(&mut h, "OK", add, 0.0);
    let e = &h.app.state.scene.environment;
    assert_eq!(e.atmospheres.len(), 1);
    assert!(matches!(e.atmospheres[0].kind, AtmosphereKind::Fog(_)), "{:?}", e.atmospheres[0].kind);
    // The new entry is selected: its Fog Parameters rollout shows.
    let win = window(&h, TITLE);
    click_text(&mut h, "Layered", win, 0.0);
    let AtmosphereKind::Fog(f) = &h.app.state.scene.environment.atmospheres[0].kind else { panic!() };
    assert!(f.layered, "Type: Layered");

    // The Effects panel: Add... > Film Grain > OK, then Active off.
    let win = window(&h, TITLE);
    click_text(&mut h, "Effects", win, 0.0);
    assert_eq!(h.app.state.environment_ui.open, Some(EnvironmentTab::Effects));
    let win = window(&h, TITLE);
    click_text(&mut h, "Add...", win, 0.0);
    h.frames(12);
    let add = window(&h, "Add Effect");
    click_text(&mut h, "Film Grain", add, 0.0);
    h.frames(12);
    let add = window(&h, "Add Effect");
    click_text(&mut h, "OK", add, 0.0);
    let fx = &h.app.state.scene.environment.effects;
    assert!(fx.len() == 1 && matches!(fx[0].kind, EffectKind::FilmGrain(_)) && fx[0].active);
    let win = window(&h, TITLE);
    click_text(&mut h, "Active", win, 0.0);
    assert!(!h.app.state.scene.environment.effects[0].active, "Active off");
    // Each edit is an undo step.
    h.cmd(Command::UndoSceneOperation);
    assert!(h.app.state.scene.environment.effects[0].active);

    // 8 goes back to the Environment panel, then closes the dialog.
    h.key(egui::Key::Num8);
    assert_eq!(h.app.state.environment_ui.open, Some(EnvironmentTab::Environment));
    h.key(egui::Key::Num8);
    assert_eq!(h.app.state.environment_ui.open, None);
}

#[test]
fn render_setup_atmospherics_and_effects_checkboxes() {
    let mut h = Harness::new();
    h.key(egui::Key::F10);
    let win = window(&h, "Render Setup");
    click_text(&mut h, "Atmospherics", win, 0.0);
    assert!(!h.app.state.scene.render.atmospherics);
    let win = window(&h, "Render Setup");
    click_text(&mut h, "Effects", win, 0.0);
    assert!(!h.app.state.scene.render.effects);
}

#[test]
fn viewport_background_can_show_the_environment_background() {
    let mut h = Harness::new();
    h.cmd(Command::ViewportBackground);
    let win = window(&h, "Viewport Background");
    click_text(&mut h, "Use Environment Background", win, 0.0);
    let win = window(&h, "Viewport Background");
    click_text(&mut h, "OK", win, 0.0);
    assert!(h.app.state.active().background_use_environment);
}

/// A render takes the Environment's background and applies its effects;
/// Render Setup > Effects off leaves them out; Render Preview fills the
/// Exposure Control thumbnail without touching the Rendered Frame Window.
#[test]
fn the_environment_reaches_a_render() {
    let Some(mut h) = Harness::with_gpu() else {
        eprintln!("the_environment_reaches_a_render: no GPU adapter, skipped");
        return;
    };
    let ok = |h: &mut Harness, cmd: &str, args: serde_json::Value| {
        let r = h.app.dispatch_json(&serde_json::json!({ "cmd": cmd, "args": args }));
        assert_eq!(r["ok"], true, "{cmd}: {r}");
    };
    ok(&mut h, "Render SetUp...", serde_json::json!({ "width": 64, "height": 48 }));
    ok(&mut h, "Environment Settings", serde_json::json!({ "background": [0.0, 0.0, 1.0] }));
    ok(&mut h, "Add Effect", serde_json::json!({ "kind": "Brightness and Contrast" }));
    ok(&mut h, "Brightness and Contrast Parameters", serde_json::json!({ "index": 0, "brightness": 0.7 }));
    h.cmd(Command::Render { path: None, width: None, height: None });
    finish_render(&mut h, 60);
    let img = h.app.state.rendered.image.clone().unwrap();
    assert_eq!(img.get_pixel(2, 2).0, [51, 51, 255, 0], "the blue background, brightened by 0.2");
    ok(&mut h, "Render SetUp...", serde_json::json!({ "effects": false }));
    h.cmd(Command::Render { path: None, width: None, height: None });
    finish_render(&mut h, 60);
    let img = h.app.state.rendered.image.clone().unwrap();
    assert_eq!(img.get_pixel(2, 2).0, [0, 0, 255, 0], "Effects off");

    let version = h.app.state.rendered.version;
    h.cmd(Command::ExposureRenderPreview);
    for _ in 0..60 {
        if h.app.state.environment_ui.preview.is_some() {
            break;
        }
        h.frames(1);
    }
    let p = h.app.state.environment_ui.preview.clone().expect("the preview rendered");
    assert!(p.width() <= 160 && p.height() <= 120, "{:?}", p.dimensions());
    assert_eq!(p.get_pixel(1, 1).0[..3], [0, 0, 255], "with the environment's background");
    assert_eq!(h.app.state.rendered.version, version, "the Rendered Frame Window keeps its image");
}
