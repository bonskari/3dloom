//! The parity30 round.

use super::*;

/// The Rendered Frame Window's Render Preset list holds the presets loaded or
/// saved; choosing one loads it (all categories), and Load Preset... opens the
/// preset file chooser.
#[test]
fn rfw_render_preset_list_loads_a_saved_preset() {
    let mut h = Harness::new();
    h.app.native_file_dialogs = false;
    let dir = std::env::temp_dir().join(format!("loom-p30-presets-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let small = dir.join("small.rps").to_string_lossy().into_owned();
    let big = dir.join("big.rps").to_string_lossy().into_owned();
    let size = |w: u32, h: u32| Command::RenderSetup { width: Some(w), height: Some(h), image_aspect: None, pixel_aspect: None, lock_image_aspect: None, lock_pixel_aspect: None, render_hidden: None, force_two_sided: None, atmospherics: None, effects: None, save_file: None, output: None };
    h.cmd(size(320, 240));
    h.cmd(Command::SaveRenderPreset { path: small.clone() });
    h.cmd(size(800, 600));
    h.cmd(Command::SaveRenderPreset { path: big.clone() });
    h.cmd(Command::RenderedFrameWindow);
    h.frames(2);
    let rfw = h.ctx.memory(|m| m.area_rect(egui::Id::new("Rendered Frame Window"))).expect("the Rendered Frame Window is shown");
    assert!(h.text_rect("Render Preset:", rfw).is_some(), "the Render Preset list is in the window");
    // The list shows the preset saved last; open it and choose the other one.
    click_text(&mut h, "big", rfw, 0.0);
    let items = h.text_rects("small");
    assert_eq!(items.len(), 1, "small is listed once: {items:?}");
    h.click(items[0].center());
    h.frames(2);
    assert_eq!((h.app.state.scene.render.width, h.app.state.scene.render.height), (320, 240));
    assert_eq!(h.app.state.render_preset.as_deref(), Some(std::path::Path::new(&small)));
    let rfw = h.ctx.memory(|m| m.area_rect(egui::Id::new("Rendered Frame Window"))).unwrap();
    click_text(&mut h, "small", rfw, 0.0);
    let load = h.text_rects("Load Preset...");
    assert_eq!(load.len(), 1);
    h.click(load[0].center());
    h.frames(2);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::LoadRenderPreset));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Render To Texture: the selected element's Use Automatic Map Size check
/// box turns the size off (Width / Height and the presets are disabled), and
/// the Automatic Mapping rollout holds the Automatic Map Size group.
#[test]
fn render_to_texture_use_automatic_map_size() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1400.0));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0; 3], size: [50.0, 50.0, 0.0] });
    h.cmd(Command::RenderToTexture);
    h.cmd(Command::AddTextureElement { element: "NormalsMap".into() });
    h.frames(2);
    let rtt = |h: &Harness| h.ctx.memory(|m| m.area_rect(egui::Id::new("Render To Texture"))).expect("Render To Texture shows");
    click_text(&mut h, "Use Automatic Map Size", rtt(&h), 0.0);
    assert!(h.app.state.scene.render.bake.elements[0].auto_size);
    click_text(&mut h, "512x512", rtt(&h), 0.0);
    assert_eq!(h.app.state.scene.render.bake.elements[0].width, 256, "the presets are off");
    click_text(&mut h, "Automatic Mapping", rtt(&h), 0.0);
    assert!(h.text_rect("Automatic Map Size", rtt(&h)).is_some());
    click_text(&mut h, "Nearest power of 2", rtt(&h), 0.0);
    assert!(h.app.state.scene.render.bake.auto_map_pow2);
}

/// Render Setup > Render Output: Create Now is off until Put Image File List
/// is on, then writes the .ifl beside the output file.
#[test]
fn render_setup_create_now_writes_the_ifl() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1400.0));
    let dir = std::env::temp_dir().join(format!("loom-ui-ifl-now-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("clip.png").to_string_lossy().into_owned();
    h.cmd(Command::RenderSetup { width: None, height: None, image_aspect: None, pixel_aspect: None, lock_image_aspect: None, lock_pixel_aspect: None, render_hidden: None, force_two_sided: None, atmospherics: None, effects: None, save_file: None, output: Some(out) });
    h.frames(2);
    let setup = |h: &Harness| h.ctx.memory(|m| m.area_rect(egui::Id::new("Render Setup"))).expect("Render Setup is open");
    click_text(&mut h, "Create Now", setup(&h), 0.0);
    assert!(!dir.join("clip.ifl").exists(), "off while Put Image File List is off");
    click_text(&mut h, "Put Image File List in Output Path", setup(&h), 0.0);
    assert!(h.app.state.scene.render.image_file_list);
    click_text(&mut h, "Create Now", setup(&h), 0.0);
    assert_eq!(std::fs::read_to_string(dir.join("clip.ifl")).unwrap(), "clip0000.png\n", "Single: the current frame");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Tools > Grab Viewport: Create Still Image File asks for a Label; Grab
/// closes the dialog, asks for the picture and shows the label in the
/// lower-right corner of the viewport while it is taken; Cancel grabs nothing.
#[test]
fn create_still_image_file_puts_the_label_in_the_corner() {
    let mut h = Harness::new();
    h.cmd(Command::CreateStillImageFile);
    h.frames(2);
    h.click_text_anywhere("Cancel");
    assert_eq!(h.app.state.dialog, None);
    assert!(h.app.grab_pending.is_none(), "Cancel grabs nothing");
    h.cmd(Command::CreateStillImageFile);
    h.frames(2);
    let win = h.ctx.memory(|m| m.area_rect(egui::Id::new("Create Still Image File"))).expect("the dialog");
    let field = h.widgets.iter().find(|w| w.0 == egui::accesskit::Role::TextInput && win.contains_rect(w.2)).expect("the Label field").2;
    h.click(field.center());
    h.run(vec![egui::Event::Text("Take 3".into())]);
    h.click_text_anywhere("Grab");
    assert_eq!(h.app.state.dialog, None);
    assert!(h.app.grab_pending.is_some(), "a screenshot is asked for");
    h.frames(1);
    let r = h.app.viewport_rects[h.app.state.active_viewport];
    let corner = egui::Rect::from_min_max(r.center(), r.max);
    assert!(h.text_rect("Take 3", corner).is_some(), "the label shows in the lower-right corner");
}
