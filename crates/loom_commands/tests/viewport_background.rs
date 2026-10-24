//! Viewport Background: Aspect Ratio, Lock Zoom/Pan anchored to the view at
//! lock time, Display Background, Apply to All Views / Active Only, Reset
//! Background Transform, saved with the scene.

use glam::Vec3;
use loom_commands::file::SceneSettings;
use loom_commands::viewport::BackgroundAspect;
use loom_commands::{execute, AppState, Command};

fn set(s: &mut AppState, path: Option<&str>, aspect: Option<&str>, display: Option<bool>, all_views: Option<bool>) {
    execute(s, Command::SetViewportBackground { path: path.map(str::to_owned), aspect: aspect.map(str::to_owned), lock: None, display, all_views }).unwrap();
}

#[test]
fn aspect_and_display_apply_to_the_active_view_or_all() {
    let mut s = AppState::new();
    s.active_viewport = 1; // Front
    set(&mut s, Some("/tmp/front.png"), Some("Match Bitmap"), None, None);
    assert_eq!(s.viewports[1].background.as_deref(), Some("/tmp/front.png"));
    assert_eq!(s.viewports[1].background_aspect, BackgroundAspect::Bitmap);
    assert!(s.viewports[1].background_display, "a new background is displayed");
    assert!(s.viewports[0].background.is_none(), "Active Only by default");
    set(&mut s, Some("/tmp/all.png"), Some("Match Rendering Output"), None, Some(true));
    for vp in &s.viewports {
        assert_eq!(vp.background.as_deref(), Some("/tmp/all.png"));
        assert_eq!(vp.background_aspect, BackgroundAspect::Rendering);
    }
    set(&mut s, None, None, Some(false), None);
    assert!(!s.viewports[1].background_display && s.viewports[1].background.is_some(), "Display Background off keeps the file");
    assert!(execute(&mut s, Command::SetViewportBackground { path: None, aspect: Some("Sideways".into()), lock: None, display: None, all_views: None }).is_err());
}

#[test]
fn lock_zoom_pan_remembers_the_view_it_was_locked_in() {
    let mut s = AppState::new();
    s.active_viewport = 0;
    set(&mut s, Some("/tmp/top.png"), Some("Match Bitmap"), None, None);
    execute(&mut s, Command::Orbit { yaw: None, pitch: None, distance: Some(50.0), target: Some([3.0, 4.0, 0.0]), roll: None }).unwrap();
    execute(&mut s, Command::BackgroundLockToggle).unwrap();
    let vp = s.active();
    assert!(vp.background_locked);
    assert_eq!(vp.background_anchor, Some((Vec3::new(3.0, 4.0, 0.0), 50.0)));
    // Zooming keeps the anchor; Reset Background Transform takes the current view.
    execute(&mut s, Command::Orbit { yaw: None, pitch: None, distance: Some(20.0), target: Some([0.0; 3]), roll: None }).unwrap();
    assert_eq!(s.active().background_anchor, Some((Vec3::new(3.0, 4.0, 0.0), 50.0)));
    execute(&mut s, Command::ResetBackgroundTransform).unwrap();
    assert_eq!(s.active().background_anchor, Some((Vec3::ZERO, 20.0)));
    let json = serde_json::to_string(&SceneSettings::from_state(&s)).unwrap();
    let mut t = AppState::new();
    serde_json::from_str::<SceneSettings>(&json).unwrap().apply_to(&mut t);
    assert_eq!(t.viewports[0].background_anchor, Some((Vec3::ZERO, 20.0)));
    assert_eq!(t.viewports[0].background_aspect, BackgroundAspect::Bitmap);
    execute(&mut s, Command::BackgroundLockToggle).unwrap();
    assert!(!s.active().background_locked && s.active().background_anchor.is_none());
}
