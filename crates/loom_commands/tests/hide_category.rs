//! Display > Hide by Category: helpers have their own category, a camera's
//! target goes with the camera, and creating an object in a hidden category
//! turns off hiding for that category.

use loom_commands::{execute, execute_json, AppState, Command, PrimitiveKind};
use serde_json::json;

fn visible(s: &AppState, name: &str) -> bool {
    let n = s.scene.nodes().find(|n| n.name == name).unwrap_or_else(|| panic!("no {name}"));
    s.is_visible(n)
}

#[test]
fn helpers_hide_by_their_own_category_and_targets_by_their_owners() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    let r = execute_json(&mut s, &json!({ "cmd": "Create Helper", "args": { "kind": "dummy", "position": [30.0, 0.0, 0.0], "size": 10.0 } }));
    assert_eq!(r["ok"], true, "{r}");
    execute(&mut s, Command::CreateCamera { target: true }).unwrap();
    let names: Vec<String> = s.scene.nodes().map(|n| n.name.clone()).collect();
    let dummy = names.iter().find(|n| n.starts_with("Dummy")).unwrap().clone();
    let cam = names.iter().find(|n| n.starts_with("Camera") && !n.ends_with(".Target")).unwrap().clone();
    let target = format!("{cam}.Target");
    execute(&mut s, Command::HideHelpersToggle).unwrap();
    assert!(!visible(&s, &dummy), "Hide Helpers hides the dummy");
    assert!(visible(&s, "Box001"), "and not the geometry");
    assert!(visible(&s, &target), "a camera target is not a helper here");
    execute(&mut s, Command::HideHelpersToggle).unwrap();
    execute(&mut s, Command::HideGeometryToggle).unwrap();
    assert!(visible(&s, &dummy), "Hide Geometry leaves helpers");
    assert!(!visible(&s, "Box001"));
    execute(&mut s, Command::HideGeometryToggle).unwrap();
    execute(&mut s, Command::HideCamerasToggle).unwrap();
    assert!(!visible(&s, &cam) && !visible(&s, &target), "the target goes with its camera");
}

#[test]
fn creating_in_a_hidden_category_unhides_it() {
    let mut s = AppState::new();
    execute(&mut s, Command::HideGeometryToggle).unwrap();
    execute(&mut s, Command::HideHelpersToggle).unwrap();
    execute(&mut s, Command::HideCamerasToggle).unwrap();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    assert!(!s.hide_category.geometry, "a new box turns off Hide Geometry");
    assert!(s.hide_category.helpers && s.hide_category.cameras, "the other categories stay hidden");
    execute(&mut s, Command::CreateCamera { target: true }).unwrap();
    assert!(!s.hide_category.cameras);
    assert!(s.hide_category.helpers, "the camera's target does not unhide Helpers");
    let r = execute_json(&mut s, &json!({ "cmd": "Create Helper", "args": { "kind": "point", "position": [0.0, 0.0, 0.0], "size": 10.0 } }));
    assert_eq!(r["ok"], true, "{r}");
    assert!(!s.hide_category.helpers);
}
