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
    // What the Material Editor's OS window notes each frame where it is.
    crate::os_window::record(&h.ctx, "Material Editor", [1500.0, 120.0, 420.0, 760.0]);
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
