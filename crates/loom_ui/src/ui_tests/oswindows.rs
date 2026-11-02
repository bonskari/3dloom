//! Floating windows as separate OS windows: the preference, the in-app
//! fallback where the backend can't open more windows (as here), and where
//! the windows were left kept in the settings file.

use super::*;

#[test]
fn the_os_windows_preference_is_on_by_default_and_preferences_switch_it() {
    let mut h = Harness::new();
    assert!(h.app.state.prefs.floating_os_windows, "on by default");
    h.cmd(Command::Preferences);
    h.click(h.find(egui::accesskit::Role::CheckBox, "Floating windows as separate OS windows").center());
    assert!(!h.app.state.prefs.floating_os_windows);
    h.cmd(Command::SetPreference { name: "Floating Windows as OS Windows".into(), value: 1.0 });
    assert!(h.app.state.prefs.floating_os_windows);
}

#[test]
fn without_multiple_viewports_the_floaters_stay_in_app_windows() {
    for on in [true, false] {
        let mut h = Harness::new();
        h.cmd(Command::SetPreference { name: "Floating Windows as OS Windows".into(), value: if on { 1.0 } else { 0.0 } });
        material_editor(&mut h);
        h.frames(2);
        let win = material_editor_rect(&h);
        assert!(SCREEN.contains_rect(win), "pref {on}: the Material Editor is an in-app window");
        assert!(h.text_rect("Material", win).is_some(), "pref {on}: its menu bar is drawn");
    }
}

#[test]
fn where_the_os_windows_were_left_is_saved_and_read_at_the_next_start() {
    let dir = std::env::temp_dir().join("loom-ui-oswindows-test");
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("settings.json");
    let mut h = Harness::new();
    h.app.use_settings_file(path.clone());
    h.frames(1);
    // Where the Material Editor's OS window was left: its viewport records it
    // inside a frame and the frame keeps it (a record made between frames is
    // replaced by the next frame's os_window::begin).
    h.app.os_window_rects.insert("Material Editor".to_owned(), [1500.0, 120.0, 420.0, 760.0]);
    h.frames(1);
    assert!(path.exists(), "a moved window writes the settings file");
    let mut next = Harness::new();
    next.app.use_settings_file(path);
    next.frames(1);
    assert_eq!(next.app.os_window_rects.get("Material Editor"), Some(&[1500.0, 120.0, 420.0, 760.0]));
    assert_eq!(crate::os_window::rects(&next.ctx).get("Material Editor"), Some(&[1500.0, 120.0, 420.0, 760.0]), "the windows open where they were");
}

/// A shortcut pressed while an OS window has the keyboard works as over an in-app window.
#[test]
fn keys_an_os_window_forwards_reach_the_main_shortcuts() {
    let mut h = Harness::new();
    material_editor(&mut h);
    let m = egui::Event::Key { key: egui::Key::M, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE };
    crate::os_window::forward_keys(&h.ctx, vec![m]);
    h.frames(1);
    assert!(!h.app.state.material_editor_open, "M from the editor's own window closes it");
}

thread_local! {
    /// The most passes one frame of an OS window took (see `with_os_windows`).
    static OS_WINDOW_PASSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The harness with a backend that opens OS windows the way the native one
/// does: each runs its own frame (`Context::run`), with every pass egui asks
/// for, inside the main window's frame.
fn with_os_windows() -> Harness {
    let h = Harness::new();
    h.ctx.set_embed_viewports(false);
    egui::Context::set_immediate_viewport_renderer(|ctx, viewport| {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(640.0, 760.0));
        let info = egui::ViewportInfo { outer_rect: Some(rect.translate(egui::vec2(1400.0, 100.0))), inner_rect: Some(rect.translate(egui::vec2(1400.0, 130.0))), ..Default::default() };
        let input = egui::RawInput { viewport_id: viewport.ids.this, viewports: std::iter::once((viewport.ids.this, info)).collect(), screen_rect: Some(rect), ..Default::default() };
        let mut ui = viewport.viewport_ui_cb;
        let out = ctx.run(input, |ctx| ui(ctx));
        OS_WINDOW_PASSES.with(|p| p.set(p.get().max(out.platform_output.num_completed_passes)));
    });
    h
}

/// Every floater that opens as an OS window draws there, also on its first
/// frame, which egui draws twice (the first pass discarded while a new Grid
/// sizes itself): the contents ran only once and the app panicked.
#[test]
fn every_os_window_floater_opens_in_its_own_window() {
    let render_setup = Command::RenderSetup { width: None, height: None, image_aspect: None, pixel_aspect: None, lock_image_aspect: None, lock_pixel_aspect: None, render_hidden: None, force_two_sided: None, atmospherics: None, effects: None, save_file: None, output: None };
    let floaters = [
        ("Material Editor", Command::MaterialEditorToggle),
        ("Material/Map Navigator", Command::MaterialMapNavigator),
        ("Track View", Command::OpenCurveEditor),
        ("Track View", Command::OpenDopeSheet),
        ("Render Setup", render_setup),
        ("Rendered Frame Window", Command::RenderedFrameWindow),
        ("RAM Player", Command::RamPlayer),
        ("Batch Render", Command::BatchRender),
        ("ActiveShade", Command::ActiveShadeFloater),
        ("Render Message Window", Command::RenderMessageWindow),
        ("Light Lister", Command::LightLister),
        ("Layer Manager", Command::LayerManager),
        ("Transform Type-In", Command::TransformTypeInDialogToggle),
    ];
    for (key, open) in floaters {
        let mut h = with_os_windows();
        poly_box(&mut h);
        h.app.native_file_dialogs = false;
        OS_WINDOW_PASSES.with(|p| p.set(0));
        h.cmd(open);
        h.frames(2);
        assert!(crate::os_window::rects(&h.ctx).contains_key(key), "{key} is drawn in an OS window");
        if key == "Material Editor" {
            assert_eq!(OS_WINDOW_PASSES.with(|p| p.get()), 2, "its first frame takes a second pass");
        }
    }
}
