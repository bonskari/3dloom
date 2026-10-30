//! Time tags: add, jump to, lock, follow another tag, rename, delete, and
//! they are saved with the scene.

use loom_commands::time_tags::tag_time;
use loom_commands::{execute, AppState, Command};

fn add(s: &mut AppState, name: &str, time: i32, relative_to: Option<&str>) {
    execute(s, Command::AddTimeTag { name: name.into(), time: Some(time), lock: None, relative_to: relative_to.map(str::to_owned) }).unwrap();
}

#[test]
fn a_tag_jumps_the_time_slider_to_its_frame() {
    let mut s = AppState::new();
    add(&mut s, "Jump", 30, None);
    execute(&mut s, Command::AddTimeTag { name: "Here".into(), time: None, lock: None, relative_to: None }).unwrap();
    assert_eq!(tag_time(&s.time_tags, "Here"), Some(0), "no time: the current frame");
    execute(&mut s, Command::GoToTimeTag { name: "Jump".into() }).unwrap();
    assert_eq!(s.current_frame, 30);
    assert!(execute(&mut s, Command::AddTimeTag { name: "Jump".into(), time: Some(1), lock: None, relative_to: None }).is_err(), "names are unique");
}

#[test]
fn a_relative_tag_follows_its_tag_and_a_locked_tag_keeps_its_frame() {
    let mut s = AppState::new();
    add(&mut s, "Start", 10, None);
    add(&mut s, "Hit", 25, Some("Start"));
    assert_eq!(tag_time(&s.time_tags, "Hit"), Some(25));
    execute(&mut s, Command::EditTimeTag { name: "Start".into(), new_name: None, time: Some(20), lock: None, relative_to: None }).unwrap();
    assert_eq!(tag_time(&s.time_tags, "Hit"), Some(35), "15 frames after Start, wherever Start goes");
    execute(&mut s, Command::EditTimeTag { name: "Start".into(), new_name: None, time: None, lock: Some(true), relative_to: None }).unwrap();
    assert!(execute(&mut s, Command::EditTimeTag { name: "Start".into(), new_name: None, time: Some(0), lock: None, relative_to: None }).is_err(), "a locked tag keeps its frame");
    assert!(execute(&mut s, Command::EditTimeTag { name: "Start".into(), new_name: None, time: None, lock: None, relative_to: Some("Hit".into()) }).is_err(), "no loops");
    // Renaming keeps the link; deleting the base leaves the follower at its frame.
    execute(&mut s, Command::EditTimeTag { name: "Start".into(), new_name: Some("Begin".into()), time: None, lock: None, relative_to: None }).unwrap();
    assert_eq!(s.time_tags[1].relative_to.as_deref(), Some("Begin"));
    execute(&mut s, Command::DeleteTimeTag { name: "Begin".into() }).unwrap();
    assert_eq!(tag_time(&s.time_tags, "Hit"), Some(35));
    assert!(s.time_tags[0].relative_to.is_none());
}

#[test]
fn time_tags_are_saved_with_the_scene() {
    let dir = std::env::temp_dir().join("loom-time-tags-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("tags.loom").to_string_lossy().into_owned();
    let mut s = AppState::new();
    add(&mut s, "A", 12, None);
    add(&mut s, "B", 3, Some("A"));
    execute(&mut s, Command::SaveFileAs { path: Some(path.clone()) }).unwrap();
    let mut t = AppState::new();
    execute(&mut t, Command::OpenFile { path: Some(path) }).unwrap();
    assert_eq!(t.time_tags, s.time_tags);
    execute(&mut t, Command::ResetFile).unwrap();
    assert!(t.time_tags.is_empty(), "Reset clears them");
}
