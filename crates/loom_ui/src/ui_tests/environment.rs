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

/// Effects > Lens Effects: the Star Element rollout turns the star on and
/// shows its Qty spinner.
#[test]
fn lens_effects_star_element_turns_on_from_its_rollout() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1400.0));
    h.key(egui::Key::Num8);
    h.frames(2);
    let win = window(&h, TITLE);
    click_text(&mut h, "Effects", win, 0.0);
    let win = window(&h, TITLE);
    click_text(&mut h, "Add...", win, 0.0);
    h.frames(12);
    let add = window(&h, "Add Effect");
    click_text(&mut h, "Lens Effects", add, 0.0);
    h.frames(12);
    let add = window(&h, "Add Effect");
    click_text(&mut h, "OK", add, 0.0);
    h.frames(2);
    // Fold the Glow so the Star's rollout fits the window.
    let win = window(&h, TITLE);
    click_text(&mut h, "Glow Element", win, 0.0);
    h.frames(12);
    let win = window(&h, TITLE);
    click_text(&mut h, "Star Element", win, 0.0);
    h.frames(12);
    let win = window(&h, TITLE);
    assert!(h.text_rect("Qty:", win).is_some(), "the Star's Qty spinner");
    click_text(&mut h, "On", win, 0.0);
    // Fold the Star, open the Ring: its On is then the only one shown.
    click_text(&mut h, "Star Element", win, 0.0);
    h.frames(12);
    let win = window(&h, TITLE);
    click_text(&mut h, "Ring Element", win, 0.0);
    h.frames(12);
    let win = window(&h, TITLE);
    assert!(h.text_rect("Thickness:", win).is_some(), "the Ring's Thickness spinner");
    click_text(&mut h, "On", win, 0.0);
    let EffectKind::Glow(g) = &h.app.state.scene.environment.effects[0].kind else { panic!("Lens Effects") };
    assert!(g.star.on && g.ring.on && !g.streak.on);
}

/// Effects > Merge: the file dialog takes a scene file, the Merge Effects
/// list shows its effects (all ticked), an unticked one stays out, a taken
/// name is renamed, and OK merges as one undo step.
#[test]
fn effects_merge_brings_in_effects_from_a_scene_file() {
    let dir = std::env::temp_dir().join("loom-ui-exchange").join("merge_effects");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("fx.loom");
    let mut src = Harness::new();
    src.cmd(Command::AddEffect { kind: "Film Grain".into(), name: None });
    src.cmd(Command::AddEffect { kind: "Blur".into(), name: Some("Soft".into()) });
    src.cmd(Command::SaveFileAs { path: Some(path.to_string_lossy().into_owned()) });

    let mut h = Harness::new();
    h.app.native_file_dialogs = false;
    h.cmd(Command::AddEffect { kind: "Blur".into(), name: Some("Soft".into()) });
    h.key(egui::Key::Num8);
    h.frames(2);
    let win = window(&h, TITLE);
    click_text(&mut h, "Effects", win, 0.0);
    let win = window(&h, TITLE);
    click_text(&mut h, "Merge", win, 0.0);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::MergeEffectsFile));
    super::choose_file(&mut h, &path, "Open");
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::MergeEffects), "{}", h.app.state.status_prompt);
    h.frames(2);
    let list = window(&h, "Merge Effects");
    let grain = h.app.state.scene.environment.effects.len();
    assert_eq!(grain, 1);
    // Untick Film Grain, keep Soft.
    let name = src.app.state.scene.environment.effects[0].name.clone();
    click_text(&mut h, &name, list, 0.0);
    let list = window(&h, "Merge Effects");
    click_text(&mut h, "OK", list, 0.0);
    let fx: Vec<String> = h.app.state.scene.environment.effects.iter().map(|e| e.name.clone()).collect();
    assert_eq!(fx, vec!["Soft".to_owned(), "Soft 1".to_owned()], "only the ticked one, renamed");
    assert!(matches!(h.app.state.scene.environment.effects[1].kind, EffectKind::Blur(_)));
    assert_eq!(h.app.state.dialog, None);
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(h.app.state.scene.environment.effects.len(), 1, "one undo step");
}

/// Effects > File Output: Files... takes the image path through the file
/// dialog, the Channel drop-down picks Luminance and Affect Source Bitmap
/// ticks on.
#[test]
fn file_output_effect_takes_its_file_channel_and_affect_source() {
    let dir = std::env::temp_dir().join("loom-ui-exchange").join("file_output");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("mid.png");
    let mut h = Harness::new();
    h.app.native_file_dialogs = false;
    h.cmd(Command::AddEffect { kind: "File Output".into(), name: None });
    h.key(egui::Key::Num8);
    h.frames(2);
    let win = window(&h, TITLE);
    click_text(&mut h, "Effects", win, 0.0);
    let win = window(&h, TITLE);
    click_text(&mut h, "File Output", win, 0.0);
    let win = window(&h, TITLE);
    click_text(&mut h, "Files...", win, 0.0);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::FileOutputFile { index: 0 }));
    super::choose_file(&mut h, &path, "Save");
    h.frames(2);
    let win = window(&h, TITLE);
    click_text(&mut h, "Whole Image", win, 0.0);
    let full = h.screen;
    click_text(&mut h, "Luminance", full, 0.0);
    h.frames(2);
    let win = window(&h, TITLE);
    click_text(&mut h, "Affect Source Bitmap", win, 0.0);
    let EffectKind::FileOutput(f) = &h.app.state.scene.environment.effects[0].kind else { panic!("File Output") };
    assert_eq!(f.path.as_deref(), Some(path.to_string_lossy().as_ref()));
    assert_eq!((f.channel, f.affect_source), (1, true));
}

/// Effects > Add > Motion Blur shows Motion Blur Parameters; its Duration
/// spinner (default 1.0) sets the effect's Duration.
#[test]
fn motion_blur_effect_duration_spinner() {
    let mut h = Harness::new();
    h.cmd(Command::AddEffect { kind: "Motion Blur".into(), name: None });
    h.key(egui::Key::Num8);
    h.frames(2);
    let win = window(&h, TITLE);
    click_text(&mut h, "Effects", win, 0.0);
    let win = window(&h, TITLE);
    click_text(&mut h, "Motion Blur", win, 0.0);
    h.frames(2);
    let EffectKind::MotionBlur(m) = &h.app.state.scene.environment.effects[0].kind else { panic!("Motion Blur") };
    assert_eq!(m.duration.get(), 1.0, "the reference's default");
    let spin = h.spinner_after("Duration:");
    h.drag(spin.center(), spin.center() + egui::vec2(0.0, 400.0));
    h.frames(1);
    let EffectKind::MotionBlur(m) = &h.app.state.scene.environment.effects[0].kind else { panic!("Motion Blur") };
    assert!(m.duration.get() < 1.0, "dragged down: {}", m.duration.get());
}

/// Lens Effects: the Globals' Size spinner (first "Size:" of the entry)
/// scales every element; the Auto Secondary element turns on and takes
/// a hexagon from its Sides list.
#[test]
fn lens_effects_globals_and_auto_secondary() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1400.0));
    h.cmd(Command::AddEffect { kind: "Lens Effects".into(), name: None });
    h.key(egui::Key::Num8);
    h.frames(2);
    let win = window(&h, TITLE);
    click_text(&mut h, "Effects", win, 0.0);
    let win = window(&h, TITLE);
    click_text(&mut h, "Lens Effects", win, 0.0);
    let win = window(&h, TITLE);
    click_text(&mut h, "Lens Effects Globals", win, 0.0);
    h.frames(12);
    let spin = h.spinner_after("Seed:");
    h.drag(spin.center(), spin.center() + egui::vec2(0.0, -30.0));
    h.frames(1);
    let EffectKind::Glow(g) = &h.app.state.scene.environment.effects[0].kind else { panic!("Lens Effects") };
    assert_ne!(g.globals.seed, 1357, "the Seed spinner");
    let win = window(&h, TITLE);
    click_text(&mut h, "Lens Effects Globals", win, 0.0);
    h.frames(12);
    let win = window(&h, TITLE);
    click_text(&mut h, "Glow Element", win, 0.0);
    h.frames(12);
    let win = window(&h, TITLE);
    click_text(&mut h, "Auto Secondary Element", win, 0.0);
    h.frames(12);
    let win = window(&h, TITLE);
    assert!(h.text_rect("Axis:", win).is_some(), "the Auto Secondary's Axis spinner");
    click_text(&mut h, "On", win, 0.0);
    let win = window(&h, TITLE);
    let combo = h.text_rect("Circular", win).expect("the Sides list");
    click_text(&mut h, "Circular", win, 0.0);
    // The list opens below its button.
    let list = egui::Rect::from_min_size(egui::pos2(combo.min.x - 40.0, combo.max.y), egui::vec2(240.0, 300.0));
    click_text(&mut h, "6", list, 0.0);
    h.frames(2);
    let EffectKind::Glow(g) = &h.app.state.scene.environment.effects[0].kind else { panic!("Lens Effects") };
    assert!(g.auto_secondary.on && !g.manual_secondary.on);
    assert_eq!(g.auto_secondary.sides, 6);
}

/// Atmosphere > Merge: the file dialog takes a scene file, the Merge
/// Atmospheres list shows its atmospheres (all ticked), an unticked one
/// stays out, a taken name is renamed, the gizmo the merged atmosphere
/// points at comes in with it, and the whole merge is one undo step.
#[test]
fn atmosphere_merge_brings_in_atmospheres_and_their_gizmos() {
    let dir = std::env::temp_dir().join("loom-ui-exchange").join("merge_atmospheres");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("atm.loom");
    let mut src = Harness::new();
    // A Fire Effect on a sphere apparatus, and a plain Fog entry.
    src.cmd(Command::CreateHelper { kind: "spheregizmo".into(), position: [0.0; 3], size: 20.0 });
    let giz = src.app.state.scene.nodes().find(|n| n.name == "SphereGizmo001").expect("the apparatus").id.0;
    src.cmd(Command::ApparatusAddAtmosphere { id: giz, kind: Some("Fire Effect".into()), existing: None });
    src.cmd(Command::AddAtmosphere { kind: "Fog".into(), name: Some("Haze".into()) });
    src.cmd(Command::SaveFileAs { path: Some(path.to_string_lossy().into_owned()) });

    let mut h = Harness::new();
    h.app.native_file_dialogs = false;
    h.cmd(Command::AddAtmosphere { kind: "Fog".into(), name: Some("Haze".into()) });
    h.key(egui::Key::Num8);
    h.frames(2);
    let win = window(&h, TITLE);
    click_text(&mut h, "Merge", win, 0.0);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::MergeAtmospheresFile));
    super::choose_file(&mut h, &path, "Open");
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::MergeAtmospheres), "{}", h.app.state.status_prompt);
    h.frames(2);
    // Untick the Fog entry; keep the Fire Effect.
    let list = window(&h, "Merge Atmospheres");
    click_text(&mut h, "Haze", list, 0.0);
    let list = window(&h, "Merge Atmospheres");
    click_text(&mut h, "OK", list, 0.0);
    let atm = &h.app.state.scene.environment.atmospheres;
    let names: Vec<String> = atm.iter().map(|a| a.name.clone()).collect();
    assert_eq!(names, vec!["Haze".to_owned(), "Fire Effect".to_owned()], "only the ticked one");
    // The apparatus came in with it and the merged entry points at the copy.
    let gizmos: Vec<loom_scene::NodeId> = match &atm[1].kind {
        AtmosphereKind::Fire(f) => f.gizmos.clone(),
        k => panic!("{k:?}"),
    };
    assert_eq!(gizmos.len(), 1, "the Fire Effect keeps its apparatus");
    let node = h.app.state.scene.node(gizmos[0]).expect("the apparatus is in this scene");
    assert_eq!(node.name, "SphereGizmo001");
    assert_eq!(h.app.state.dialog, None);
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(h.app.state.scene.environment.atmospheres.len(), 1, "one undo step");
    assert!(h.app.state.scene.nodes().all(|n| n.name != "SphereGizmo001"), "the apparatus goes with it");
}

/// Exposure Control: the Color Correction and Desaturate Low Levels
/// checkboxes of the chosen control tick the environment's, and Physical
/// Scale shows with them.
#[test]
fn exposure_control_checkboxes_tick_color_correction_and_desaturation() {
    let mut h = Harness::new();
    h.cmd(Command::ExposureControl { kind: Some(ExposureKind::Automatic), active: None, process_background: None, brightness: None, contrast: None, exposure_value: None, mid_tones: None, physical_scale: None, color_correction: None, correction_color: None, desaturate_low: None });
    h.key(egui::Key::Num8);
    h.frames(2);
    let win = window(&h, TITLE);
    click_text(&mut h, "Color Correction:", win, 0.0);
    assert!(h.app.state.scene.environment.exposure.color_correction, "{}", h.app.state.status_prompt);
    let win = window(&h, TITLE);
    click_text(&mut h, "Desaturate Low Levels", win, 0.0);
    let x = &h.app.state.scene.environment.exposure;
    assert!(x.desaturate_low);
    assert_eq!(x.physical_scale.get(), 1500.0, "Physical Scale keeps its default");
    // <no exposure control>: the parameters are not shown at all.
    h.cmd(Command::ExposureControl { kind: Some(ExposureKind::None), active: None, process_background: None, brightness: None, contrast: None, exposure_value: None, mid_tones: None, physical_scale: None, color_correction: None, correction_color: None, desaturate_low: None });
    h.frames(2);
    assert!(h.text_rects("Desaturate Low Levels").is_empty(), "no control, no parameters");
}
