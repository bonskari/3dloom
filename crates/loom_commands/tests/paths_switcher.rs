//! Configure User Paths (File I/O folders, External Files bitmap folders)
//! and the Custom UI and Defaults Switcher.

use loom_commands::settings::Settings;
use loom_commands::{execute, AppState, Command, Dialog};

fn dir(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join("loom-paths-tests").join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn file_io_folders_and_bitmap_folders_are_set_saved_and_searched() {
    let d = dir("paths");
    let scenes = d.join("scenes");
    let maps = d.join("maps");
    std::fs::create_dir_all(&scenes).unwrap();
    std::fs::create_dir_all(&maps).unwrap();
    let mut s = AppState::new();
    execute(&mut s, Command::ConfigureUserPaths).unwrap();
    assert_eq!(s.dialog, Some(Dialog::ConfigurePaths));
    execute(&mut s, Command::SetUserPath { name: "Scenes".into(), path: scenes.to_string_lossy().into() }).unwrap();
    assert_eq!(s.paths.dir("Scenes"), Some(scenes.as_path()));
    assert!(execute(&mut s, Command::SetUserPath { name: "Sounds".into(), path: scenes.to_string_lossy().into() }).is_err(), "no such entry");
    assert!(execute(&mut s, Command::SetUserPath { name: "Import".into(), path: d.join("missing").to_string_lossy().into() }).is_err(), "not a folder");
    execute(&mut s, Command::SetBitmapPaths { paths: vec![maps.to_string_lossy().into()] }).unwrap();
    // A map moved from where the scene says: found by name in an External Files folder.
    std::fs::write(maps.join("brick.png"), b"not really a png").unwrap();
    let found = s.paths.resolve_bitmap("/somewhere/else/brick.png").expect("found in the bitmap folder");
    assert_eq!(found, maps.join("brick.png"));
    assert!(s.paths.resolve_bitmap("/somewhere/else/none.png").is_none());
    // Saved with the settings.
    let loaded = Settings::from_json(&Settings::capture(&s).to_json()).unwrap();
    let mut t = AppState::new();
    loaded.apply(&mut t);
    assert_eq!(t.paths, s.paths);
    execute(&mut t, Command::SetUserPath { name: "Scenes".into(), path: String::new() }).unwrap();
    assert!(t.paths.dir("Scenes").is_none(), "cleared: the program's folder");
}

#[test]
fn auto_backup_writes_into_its_file_io_folder() {
    let d = dir("autoback");
    let mut s = AppState::new();
    execute(&mut s, Command::SetUserPath { name: "Auto Backup".into(), path: d.to_string_lossy().into() }).unwrap();
    let r = execute(&mut s, Command::AutoBackupNow).unwrap();
    let path = std::path::PathBuf::from(r["path"].as_str().unwrap());
    assert_eq!(path.parent(), Some(d.as_path()), "{r}");
    assert!(path.exists());
}

#[test]
fn the_switcher_sets_ui_schemes_and_defaults_sets() {
    let d = dir("switcher");
    std::env::set_var("LOOM_CONFIG_DIR", &d);
    let mut s = AppState::new();
    let r = execute(&mut s, Command::CustomUiDefaultsSwitcher).unwrap();
    assert_eq!(s.dialog, Some(Dialog::Switcher));
    assert_eq!(r["schemes"], serde_json::json!(["Default UI", "Modular Toolbars UI"]));
    // Modular Toolbars UI: the hidden toolbars docked under the main toolbar.
    execute(&mut s, Command::SwitchUiScheme { name: "Modular Toolbars UI".into() }).unwrap();
    for t in ["Axis Constraints", "Layers", "Extras", "Snaps"] {
        assert!(loom_commands::ui_custom::is_shown(&s, t), "{t} shown");
        assert_eq!(s.ui_custom.dock(t), loom_commands::ui_custom::Dock::Top);
    }
    // A scheme saved in the config folder's ui folder is listed and applies.
    let scheme = d.join("ui").join("Mine.ui");
    execute(&mut s, Command::SaveCustomUiScheme { path: Some(scheme.to_string_lossy().into()), parts: None }).unwrap();
    assert!(loom_commands::switcher::schemes().contains(&"Mine".to_owned()));
    execute(&mut s, Command::SwitchUiScheme { name: "Default UI".into() }).unwrap();
    assert!(!loom_commands::ui_custom::is_shown(&s, "Snaps"), "Default UI hides them again");
    execute(&mut s, Command::SwitchUiScheme { name: "Mine".into() }).unwrap();
    assert!(loom_commands::ui_custom::is_shown(&s, "Snaps"));
    assert!(execute(&mut s, Command::SwitchUiScheme { name: "Nope".into() }).is_err());
    // Defaults sets: save the current preferences, change them, switch back.
    execute(&mut s, Command::SetPreference { name: "Gizmo Size".into(), value: 44.0 }).unwrap();
    execute(&mut s, Command::SaveDefaultsSet { name: "Big Gizmo".into() }).unwrap();
    assert!(loom_commands::switcher::defaults_sets().contains(&"Big Gizmo".to_owned()));
    execute(&mut s, Command::SwitchDefaults { name: "Default".into() }).unwrap();
    assert_eq!(s.prefs, loom_commands::Prefs::default());
    execute(&mut s, Command::SwitchDefaults { name: "Big Gizmo".into() }).unwrap();
    assert_eq!(s.prefs.gizmos.size, 44.0);
    assert!(execute(&mut s, Command::SaveDefaultsSet { name: "Default".into() }).is_err());
}
