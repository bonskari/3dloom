//! The parity28 round.

use super::*;

fn perspective_index(h: &Harness) -> usize {
    h.app.state.viewports.iter().position(|v| v.kind == loom_commands::ViewKind::Perspective).expect("a Perspective view")
}

/// Half the horizontal field of view's tangent of viewport `i` as drawn.
fn half_tan_x(h: &Harness, i: usize) -> f32 {
    let r = h.app.viewport_rects[i];
    1.0 / h.app.camera_for(i).projection_matrix(r.width() / r.height()).x_axis.x
}

/// The Perspective view's 45 degree FOV is measured across its width, in the
/// quad layout and maximized alike; Create Camera From View gives the camera
/// that same horizontal FOV and the camera view draws the same width.
#[test]
fn the_perspective_fov_spans_the_view_width() {
    let mut h = Harness::new();
    let i = perspective_index(&h);
    assert!((h.app.state.viewports[i].fov_radians.to_degrees() - 45.0).abs() < 1e-4);
    let t = 22.5f32.to_radians().tan();
    assert!((half_tan_x(&h, i) - t).abs() < 1e-4, "quad: {}", half_tan_x(&h, i));
    h.cmd(Command::ActiveViewport { index: i as u32 });
    maximized(&mut h);
    h.frames(2);
    let r = h.app.viewport_rects[i];
    assert!((half_tan_x(&h, i) - t).abs() < 1e-4, "maximized ({}x{}): {}", r.width(), r.height(), half_tan_x(&h, i));
    h.cmd(Command::CreateCameraFromView);
    let loom_commands::ViewKind::Camera(id) = h.app.state.viewports[i].kind else { panic!("not looking through the new camera") };
    let Some(loom_scene::ObjectKind::Camera { fov, .. }) = h.app.state.scene.node(id).map(|n| &n.object) else { panic!("no camera") };
    assert!((fov.get() - 45.0).abs() < 1e-3, "camera FOV {}", fov.get());
    assert!((half_tan_x(&h, i) - t).abs() < 1e-4, "the camera view draws the same width");
}

/// Object Skin Pose Mode: from the Animation menu the posed object shows its
/// skin pose; a move there (Auto Key on) changes only the skin pose, the
/// animation keeps no new key, and leaving the mode (Hierarchy > Pivot >
/// Skin Pose) puts the object back at its animated pose. Enabled > Rotation
/// off keeps the pose's rotation.
#[test]
fn object_skin_pose_mode_edits_the_skin_pose_not_the_animation() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let id = *h.app.state.selection.iter().next().unwrap();
    let pos = |h: &Harness| h.app.state.scene.node(id).unwrap().transform.position.get();
    let skin = |h: &Harness| h.app.state.scene.node(id).unwrap().skin_pose.clone().expect("a skin pose");
    assert!(h.app.dispatch(Command::ObjectSkinPoseMode).is_err(), "only after Set as Skin Pose");
    super::widgets::menu_path(&mut h, &["Animation", "Set as Skin Pose"]);
    // Animate the box away at frame 10.
    h.cmd(Command::AutoKeyModeToggle);
    h.cmd(Command::SetFrame { frame: 10 });
    h.cmd(Command::MoveSelection { delta: [30.0, 0.0, 0.0], system: None });
    assert!((pos(&h).x - 30.0).abs() < 1e-3);
    let keys = |h: &Harness| h.app.state.scene.node(id).unwrap().transform.position.keys().map_or(0, |k| k.tracks.iter().map(|t| t.keys.len()).sum::<usize>());
    let animated = keys(&h);
    assert!(animated > 0, "the move was keyed");
    super::widgets::menu_path(&mut h, &["Animation", "Skin Pose Mode"]);
    assert!(h.app.state.object_skin_pose_live.is_some());
    assert!(pos(&h).length() < 1e-3, "the skin pose shows: {}", pos(&h));
    h.cmd(Command::MoveSelection { delta: [0.0, 5.0, 0.0], system: None });
    h.cmd(Command::SetTransform { id: id.0, position: None, rotation_deg: Some([0.0, 0.0, 30.0]), scale: None });
    // Hierarchy > Pivot > Skin Pose: Enabled Rotation off, then leave the mode.
    h.app.panel_tab = crate::Tab::Hierarchy;
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 1400.0));
    h.frames(3);
    h.click(h.find(egui::accesskit::Role::CheckBox, "Rotation").center());
    h.frames(2);
    assert_eq!(h.app.state.skin_pose_enabled, [true, false, true]);
    let mode = h.find_all(egui::accesskit::Role::Button, "Skin Pose Mode");
    assert_eq!(mode.len(), 1, "the Skin Pose rollout's button");
    h.click(mode[0].center());
    h.frames(2);
    assert!(h.app.state.object_skin_pose_live.is_none(), "the mode is off");
    assert!((pos(&h).x - 30.0).abs() < 1e-3, "back at the animated pose: {}", pos(&h));
    let s = skin(&h);
    assert!((s.position.get() - Vec3::new(0.0, 5.0, 0.0)).length() < 1e-3, "the move went into the skin pose: {}", s.position.get());
    assert!(s.rotation.get().length() < 1e-3, "Rotation not recorded: {}", s.rotation.get());
    assert_eq!(keys(&h), animated, "no key from the mode's edits");
    // Assume Skin Pose keys the pose at the current frame with Auto Key on.
    h.cmd(Command::SetFrame { frame: 20 });
    super::widgets::menu_path(&mut h, &["Animation", "Assume Skin Pose"]);
    assert!((pos(&h) - Vec3::new(0.0, 5.0, 0.0)).length() < 1e-3);
    h.cmd(Command::SetFrame { frame: 10 });
    assert!((pos(&h).x - 30.0).abs() < 1e-3, "the animation is kept");
}

/// Render Message Window: information messages show only with Show
/// Information Messages (off by default), set from the list's right-click
/// menu, where Open Message Window on Error also turns off the window
/// opening by itself.
#[test]
fn render_message_window_right_click_shows_information_messages() {
    use loom_commands::render::{message, MessageLevel};
    let mut h = Harness::new();
    message(&mut h.app.state, MessageLevel::Info, "an information line");
    message(&mut h.app.state, MessageLevel::Error, "an error line");
    assert!(h.app.state.rendered.messages_open, "the error opened the window");
    h.frames(3);
    assert!(h.text_centers("INFO: an information line").is_empty(), "information hidden by default");
    let err = h.text_centers("ERROR: an error line");
    assert_eq!(err.len(), 1, "the error shows");
    h.run(vec![egui::Event::PointerMoved(err[0]), egui::Event::PointerButton { pos: err[0], button: egui::PointerButton::Secondary, pressed: true, modifiers: egui::Modifiers::NONE }]);
    h.run(vec![egui::Event::PointerButton { pos: err[0], button: egui::PointerButton::Secondary, pressed: false, modifiers: egui::Modifiers::NONE }]);
    h.frames(2);
    h.click(h.find(egui::accesskit::Role::CheckBox, "Show Information Messages").center());
    h.frames(2);
    assert!(h.app.state.prefs.render_show_info);
    assert_eq!(h.text_centers("INFO: an information line").len(), 1, "information shows");
    h.click(h.find(egui::accesskit::Role::CheckBox, "Open Message Window on Error").center());
    h.frames(2);
    assert!(!h.app.state.prefs.render_open_on_error);
    h.app.state.rendered.messages_open = false;
    message(&mut h.app.state, MessageLevel::Error, "another error");
    assert!(!h.app.state.rendered.messages_open, "the window stays closed");
}

/// Show Safe Frames: the Perspective view's FOV then spans the live area
/// (the 4:3 output fitted in the wider view), so the frame shows what renders.
#[test]
fn with_safe_frames_the_perspective_fov_spans_the_live_area() {
    let mut h = Harness::new();
    let i = perspective_index(&h);
    h.cmd(Command::ActiveViewport { index: i as u32 });
    let t = 22.5f32.to_radians().tan();
    let before = half_tan_x(&h, i);
    // Shift+F: Show Safe Frames.
    h.run(vec![egui::Event::PointerMoved(h.app.viewport_rects[i].center())]);
    h.mods = egui::Modifiers::SHIFT;
    h.key(egui::Key::F);
    h.mods = egui::Modifiers::NONE;
    h.frames(2);
    assert!(h.app.state.viewports[i].safe_frames);
    let r = h.app.viewport_rects[i];
    let live = crate::viewport_widget::safe_frame_live_area(&h.app, r);
    assert!((before - t).abs() < 1e-4);
    let across_live = half_tan_x(&h, i) * live.width() / r.width();
    assert!((across_live - t).abs() < 1e-4, "45 degrees across the live area: {across_live} vs {t}");
    assert_eq!(crate::rendered_frame::output_frame(&h.app, i, r), live, "the render frame is the live area");
}
