use loom_commands::{execute,AppState,Command};
#[test]
fn preview_limits_are_local_preferences_and_legacy_config_gets_safe_defaults(){
 let mut s=AppState::new();
 assert_eq!(s.prefs.get("Viewport Texture Max Size"),Some(2048.));
 assert_eq!(s.prefs.get("Viewport Texture Budget MB"),Some(256.));
 execute(&mut s,Command::SetPreference{name:"Viewport Texture Max Size".into(),value:1024.}).unwrap();
 execute(&mut s,Command::SetPreference{name:"Viewport Texture Budget MB".into(),value:128.}).unwrap();
 assert_eq!(s.prefs.get("Viewport Texture Max Size"),Some(1024.));
 let old:loom_commands::state::Prefs=serde_json::from_value(serde_json::json!({})).unwrap();assert_eq!(old.get("Viewport Texture Budget MB"),Some(256.));
 assert!(serde_json::to_value(&s.scene).unwrap().get("viewport_texture_max_size").is_none());
 let before=s.prefs.clone();assert!(execute(&mut s,Command::SetPreference{name:"Viewport Texture Budget MB".into(),value:f64::NAN}).is_err());assert_eq!(s.prefs,before);
}
