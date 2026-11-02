//! Preferences > Files > Increment on Save (default off): Save writes the
//! next numbered file beside the scene.

use loom_commands::{execute, AppState, Command, PrimitiveKind};

#[test]
fn increment_on_save_writes_the_next_numbered_file() {
    let dir = std::env::temp_dir().join(format!("loom-inc-save-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut s = AppState::new();
    assert!(!s.prefs.increment_on_save, "Default=off");
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    execute(&mut s, Command::SaveFileAs { path: Some(dir.join("shot.loom").to_string_lossy().into_owned()) }).unwrap();
    execute(&mut s, Command::SaveFile { path: None }).unwrap();
    assert!(!dir.join("shot01.loom").exists(), "off: Save writes the scene's own file");
    execute(&mut s, Command::SetPreference { name: "Increment on Save".into(), value: 1.0 }).unwrap();
    execute(&mut s, Command::SaveFile { path: None }).unwrap();
    assert!(dir.join("shot01.loom").is_file(), "on: the next numbered file");
    let _ = std::fs::remove_dir_all(&dir);
}
