//! Modifier List: the most-used section on top and the search box.

use loom_commands::{execute, modifier_list, AppState, Command, PrimitiveKind};

#[test]
fn most_used_starts_from_the_common_ones_and_follows_use() {
    let mut s = AppState::new();
    let top = modifier_list::most_used(&s.modifier_usage, 8);
    assert_eq!(top.len(), 8);
    assert_eq!(top[0], "Edit Poly", "a fresh install starts with the common modelling modifiers");
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    for _ in 0..3 {
        execute(&mut s, Command::AddModifier { name: "Twist".into() }).unwrap();
    }
    execute(&mut s, Command::AddModifier { name: "Bend".into() }).unwrap();
    let top = modifier_list::most_used(&s.modifier_usage, 8);
    assert_eq!(&top[..2], ["Twist", "Bend"], "used modifiers come first, most used first");
    assert!(top.contains(&"Edit Poly".to_owned()), "the rest is filled with the common ones");
}

#[test]
fn search_matches_anywhere_in_the_name_ignoring_case() {
    let hits = modifier_list::search("smooth");
    assert!(hits.iter().any(|n| n == "TurboSmooth"));
    assert!(hits.iter().any(|n| n == "MeshSmooth"));
    assert!(hits.iter().all(|n| n.to_lowercase().contains("smooth")));
    assert!(modifier_list::search("").len() > 50, "an empty search lists everything");
}

#[test]
fn usage_round_trips_through_its_file() {
    let mut s = AppState::new();
    s.modifier_usage.insert("Shell".into(), 5);
    let text = modifier_list::usage_to_json(&s.modifier_usage);
    assert_eq!(modifier_list::usage_from_json(&text), s.modifier_usage);
    assert!(modifier_list::usage_from_json("not json").is_empty());
}
