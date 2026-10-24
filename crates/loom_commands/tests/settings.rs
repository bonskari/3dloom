//! The user's settings file (the reference's ini + .kbd + .cui): Preferences,
//! Units Setup, Show UI toggles, keyboard shortcut edits, recent files.

use loom_commands::settings::{Settings, UiLayout};
use loom_commands::{execute, parse_chord, AppState, Command, Dialog, PrimitiveKind};
use std::path::PathBuf;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join("loom-settings-tests").join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn p(path: &std::path::Path) -> Option<String> {
    Some(path.to_string_lossy().into_owned())
}

fn pref(s: &mut AppState, name: &str, value: f64) {
    execute(s, Command::SetPreference { name: name.into(), value }).unwrap();
}

fn units(s: &mut AppState, system: &str, display: &str) {
    execute(s, Command::SetUnits { system_unit: Some(system.into()), system_scale: None, display: Some(display.into()), custom_name: None, custom_value: None, custom_unit: None }).unwrap();
}

fn assign(s: &mut AppState, group: &str, action: &str, chord: &str) -> serde_json::Value {
    execute(s, Command::AssignShortcut { group: group.into(), action: action.into(), chord: chord.into() }).unwrap()
}

#[test]
fn settings_round_trip_through_json() {
    let d = dir("round_trip");
    let mut s = AppState::new();
    pref(&mut s, "Gizmo Size", 35.0);
    pref(&mut s, "Space Locks Selection", 1.0);
    execute(&mut s, Command::KeepCreatingToggle).unwrap();
    pref(&mut s, "Spinner Precision", 1.0);
    pref(&mut s, "Recent Files in File Menu", 4.0);
    execute(&mut s, Command::SetAutoBackupName { name: "MyBak".into() }).unwrap();
    units(&mut s, "Centimeters", "Meters");
    execute(&mut s, Command::ShowMainToolbarToggle).unwrap();
    execute(&mut s, Command::ShowRibbonToggle).unwrap();
    execute(&mut s, Command::ShowCommandPanelToggle).unwrap();
    execute(&mut s, Command::ShowTrackBarToggle).unwrap();
    assign(&mut s, "Main UI", "Select and Move", "Alt+Shift+M");
    execute(&mut s, Command::RemoveShortcut { group: "Editable Poly".into(), action: "Face Level".into() }).unwrap();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    let scene = d.join("a.loom");
    execute(&mut s, Command::SaveFile { path: p(&scene) }).unwrap();

    let text = Settings::capture(&s).to_json();
    let loaded = Settings::from_json(&text).unwrap();
    let mut t = AppState::new();
    loaded.apply(&mut t);
    assert_eq!(t.prefs, s.prefs);
    assert_eq!(t.prefs.gizmos.size, 35.0);
    assert!(t.prefs.space_selection_lock);
    assert!(t.prefs.keep_creating);
    assert_eq!(t.prefs.auto_backup_name, "MyBak");
    assert_eq!(t.units, s.units);
    assert_eq!(t.format_units(100.0), "1.0m");
    assert!(!t.show_main_toolbar && !t.show_command_panel);
    assert_eq!(t.show_ribbon, s.show_ribbon);
    assert_eq!(t.show_track_bar, s.show_track_bar);
    assert_eq!(t.hotkey(parse_chord("Alt+Shift+M").unwrap()), Some("Select and Move"));
    assert_eq!(t.recent_files, vec![scene]);
    assert_eq!(Settings::capture(&t), loaded, "applying and capturing again changes nothing");
}

#[test]
fn a_missing_or_partial_file_gives_the_defaults_and_garbage_is_an_error() {
    let defaults = Settings::capture(&AppState::new());
    assert_eq!(Settings::from_json("{}").unwrap(), defaults);
    let partial = Settings::from_json(r#"{"prefs":{"undo_levels":5}}"#).unwrap();
    assert_eq!(partial.prefs.undo_levels, 5);
    assert_eq!(partial.prefs.gizmos, defaults.prefs.gizmos);
    assert!(Settings::from_json("not json").is_err());
    let mut s = AppState::new();
    Settings::from_json(r#"{"shortcuts":{"Main UI":[["Q","No Such Action"]]}}"#).unwrap().apply(&mut s);
    assert_eq!(s.hotkey(parse_chord("Q").unwrap()), Some("Select Object"), "an unknown action leaves the defaults");
}

#[test]
fn the_config_dir_follows_loom_config_dir() {
    let d = dir("config_dir");
    // Only this test sets the variable; others read paths they are given.
    std::env::set_var("LOOM_CONFIG_DIR", &d);
    assert_eq!(loom_commands::settings::config_dir(), Some(d.clone()));
    assert_eq!(loom_commands::settings::settings_path(), Some(d.join("settings.json")));
    std::env::remove_var("LOOM_CONFIG_DIR");
}

#[test]
fn assign_moves_a_taken_hotkey_and_remove_clears_all_of_an_actions_keys() {
    let mut s = AppState::new();
    let w = parse_chord("W").unwrap();
    let e = parse_chord("E").unwrap();
    assert_eq!(s.hotkey(e), Some("Select and Rotate"));
    let r = assign(&mut s, "Main UI", "Select and Move", "E");
    assert_eq!(r["previous"], "Select and Rotate", "the result names the action the key was taken from");
    assert_eq!(s.hotkey(e), Some("Select and Move"));
    assert_eq!(s.hotkey(w), Some("Select and Move"), "assigning adds a key; the old one stays");
    assert_eq!(s.shortcut_label("Select and Rotate"), None);
    execute(&mut s, Command::RemoveShortcut { group: "Main UI".into(), action: "Select and Move".into() }).unwrap();
    assert_eq!(s.hotkey(w), None);
    assert_eq!(s.hotkey(e), None);
    assert!(execute(&mut s, Command::AssignShortcut { group: "Main UI".into(), action: "No Such Action".into(), chord: "K".into() }).is_err());
    assert!(execute(&mut s, Command::AssignShortcut { group: "Nowhere".into(), action: "Select and Move".into(), chord: "K".into() }).is_err());
    assert!(execute(&mut s, Command::AssignShortcut { group: "Main UI".into(), action: "Select and Move".into(), chord: "Ctrl+Nope".into() }).is_err());
    execute(&mut s, Command::ResetShortcuts).unwrap();
    assert_eq!(s.hotkey(w), Some("Select and Move"));
    assert_eq!(s.hotkey(e), Some("Select and Rotate"));
}

#[test]
fn group_shortcuts_apply_in_their_context_only() {
    let mut s = AppState::new();
    assign(&mut s, "Editable Poly", "Create Shape From Edges", "Alt+Shift+K");
    let k = parse_chord("Alt+Shift+K").unwrap();
    assert_eq!(s.hotkey(k), None, "no Editable Poly selected");
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    execute(&mut s, Command::ConvertToEditablePoly).unwrap();
    assert_eq!(s.hotkey(k), Some("Create Shape From Edges"));
}

#[test]
fn shortcut_files_save_and_load() {
    let d = dir("kbd");
    let path = d.join("mine.kbd");
    let mut s = AppState::new();
    assign(&mut s, "Main UI", "Select and Move", "Alt+Shift+M");
    execute(&mut s, Command::SaveShortcuts { path: p(&path) }).unwrap();
    execute(&mut s, Command::ResetShortcuts).unwrap();
    let m = parse_chord("Alt+Shift+M").unwrap();
    assert_eq!(s.hotkey(m), None);
    execute(&mut s, Command::LoadShortcuts { path: p(&path) }).unwrap();
    assert_eq!(s.hotkey(m), Some("Select and Move"));
    let bad = d.join("bad.kbd");
    std::fs::write(&bad, "[Shortcuts]\nfoo=bar\n").unwrap();
    assert!(execute(&mut s, Command::LoadShortcuts { path: p(&bad) }).is_err(), "only our own format loads");
    execute(&mut s, Command::LoadShortcuts { path: None }).unwrap();
    assert_eq!(s.dialog, Some(Dialog::LoadShortcuts));
    execute(&mut s, Command::SaveShortcuts { path: None }).unwrap();
    assert_eq!(s.dialog, Some(Dialog::SaveShortcuts));
}

#[test]
fn every_key_survives_the_chart_text_round_trip() {
    for k in loom_commands::keymap::ALL_KEYS {
        for (ctrl, alt, shift) in [(false, false, false), (true, true, true), (true, false, false), (false, true, true)] {
            let c = loom_commands::Chord { key: k, ctrl, alt, shift };
            assert_eq!(parse_chord(&c.chart_text()), Ok(c), "{}", c.chart_text());
        }
    }
}

#[test]
fn recent_files_are_newest_first_unique_and_capped() {
    let d = dir("recent");
    let mut s = AppState::new();
    pref(&mut s, "Recent Files in File Menu", 2.0);
    let a = d.join("a.loom");
    let b = d.join("b.loom");
    let c = d.join("c.loom");
    execute(&mut s, Command::SaveFileAs { path: p(&a) }).unwrap();
    execute(&mut s, Command::SaveFileAs { path: p(&b) }).unwrap();
    execute(&mut s, Command::OpenFile { path: p(&a) }).unwrap();
    assert_eq!(s.recent_files, vec![a.clone(), b.clone()]);
    execute(&mut s, Command::SaveFileAs { path: p(&c) }).unwrap();
    assert_eq!(s.recent_files, vec![c.clone(), a.clone()], "two kept");
    // File > Recent opens the listed file (behind the Save Changes prompt in the UI).
    loom_commands::file_action(&mut s, loom_commands::PendingFileAction::OpenRecent(1)).unwrap();
    assert_eq!(s.file.as_deref(), Some(a.as_path()));
    assert_eq!(s.recent_files[0], a);
}

#[test]
fn reset_keeps_the_users_units_and_an_opened_file_does_not_change_them() {
    let d = dir("units");
    let mut s = AppState::new();
    units(&mut s, "Centimeters", "Meters");
    let path = d.join("inches.loom");
    let mut other = AppState::new();
    execute(&mut other, Command::SaveFile { path: p(&path) }).unwrap();
    execute(&mut s, Command::OpenFile { path: p(&path) }).unwrap();
    assert_eq!(s.units.system_unit, loom_commands::SystemUnit::Inches, "the scene's units load with it");
    assert_eq!(Settings::capture(&s).units.system_unit, loom_commands::SystemUnit::Centimeters, "the saved defaults stay the user's");
    execute(&mut s, Command::ResetFile).unwrap();
    assert_eq!(s.units.system_unit, loom_commands::SystemUnit::Centimeters);
    assert_eq!(s.units.display, loom_commands::DisplayUnits::Meters);
}

#[test]
fn ui_schemes_save_load_and_revert_to_the_startup_layout() {
    let d = dir("scheme");
    let path = d.join("mine.ui");
    let mut s = AppState::new();
    let startup = UiLayout::of(&s);
    execute(&mut s, Command::ShowMainToolbarToggle).unwrap();
    execute(&mut s, Command::ShowCommandPanelToggle).unwrap();
    assign(&mut s, "Main UI", "Select and Move", "Alt+Shift+M");
    execute(&mut s, Command::SaveCustomUiScheme { path: p(&path) }).unwrap();
    execute(&mut s, Command::RevertToStartupUi).unwrap();
    assert_eq!(UiLayout::of(&s), startup);
    assert!(s.show_main_toolbar && s.show_command_panel);
    execute(&mut s, Command::ResetShortcuts).unwrap();
    execute(&mut s, Command::LoadCustomUiScheme { path: p(&path) }).unwrap();
    assert!(!s.show_main_toolbar && !s.show_command_panel);
    assert_eq!(s.hotkey(parse_chord("Alt+Shift+M").unwrap()), Some("Select and Move"), "a scheme carries its shortcuts");
    assert!(execute(&mut s, Command::LoadCustomUiScheme { path: p(&d.join("missing.ui")) }).is_err());
    execute(&mut s, Command::LoadCustomUiScheme { path: None }).unwrap();
    assert_eq!(s.dialog, Some(Dialog::LoadCustomUiScheme));
}

/// Keep Creating: off by default (one object, then back to Select); the
/// toggle and Set Preference "Keep Creating" switch it.
#[test]
fn keep_creating_is_off_by_default_and_toggles() {
    let mut s = AppState::new();
    assert!(!s.prefs.keep_creating);
    assert_eq!(execute(&mut s, Command::KeepCreatingToggle).unwrap()["keep_creating"], true);
    assert!(s.prefs.keep_creating);
    pref(&mut s, "Keep Creating", 0.0);
    assert!(!s.prefs.keep_creating);
    assert_eq!(s.prefs.get("Keep Creating"), Some(0.0));
}
