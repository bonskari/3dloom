//! Viewport Clipping (General viewport label menu): a per-viewport near / far
//! clip the viewport's slider sets; off, the planes follow the camera.

use loom_commands::{execute, AppState, Command};

#[test]
fn clipping_toggles_and_keeps_its_planes() {
    let mut s = AppState::new();
    let i = s.active_viewport;
    assert!(!s.viewports[i].clipping);
    execute(&mut s, Command::ViewportClipping).unwrap();
    assert!(s.viewports[i].clipping);
    execute(&mut s, Command::SetViewportClipping { index: None, near: 5.0, far: 80.0 }).unwrap();
    assert_eq!(s.viewports[i].clip, (5.0, 80.0));
    execute(&mut s, Command::ViewportClipping).unwrap();
    assert!(!s.viewports[i].clipping);
    assert_eq!(s.viewports[i].clip, (5.0, 80.0), "the slider positions are kept");
    // Far never ends up in front of near.
    execute(&mut s, Command::SetViewportClipping { index: Some(0), near: 50.0, far: 10.0 }).unwrap();
    assert!(s.viewports[0].clip.1 > s.viewports[0].clip.0);
    assert!(s.viewports[0].clipping, "setting the planes turns clipping on");
}
