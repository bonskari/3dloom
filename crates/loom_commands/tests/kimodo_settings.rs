use loom_commands::{execute, AppState, Command};
use loom_commands::settings::Settings;

#[test]
fn kimodo_address_is_local_preferences_and_not_project_data() {
    let mut state = AppState::new();
    assert!(state.prefs.kimodo_api_url.is_empty());
    state.prefs.kimodo_api_url = "https://example.org/motion-api".into();
    let local = Settings::capture(&state).to_json();
    let mut restored = AppState::new();
    Settings::from_json(&local).unwrap().apply(&mut restored);
    assert_eq!(restored.prefs.kimodo_api_url, state.prefs.kimodo_api_url);
    let path = std::env::temp_dir().join(format!("loom-kimodo-settings-{}.loom", std::process::id()));
    execute(&mut state, Command::SaveFileAsCopy { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    let scene = std::fs::read_to_string(&path).unwrap();
    assert!(!scene.contains("example.org"));
    assert!(!scene.contains("kimodo_api_url"));
    std::fs::remove_file(path).unwrap();
}
