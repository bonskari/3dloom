use super::*;

fn point_scroll(delta: egui::Vec2, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta, modifiers }
}

#[test]
fn two_finger_pan_changes_only_the_hovered_viewport_without_zooming() {
    let mut h = Harness::new();
    let index = 0;
    assert_ne!(index, h.app.state.active_viewport);
    let center = h.app.viewport_rects[index].center();
    let before: Vec<_> = h.app.state.viewports.iter().map(|v| v.camera.clone()).collect();
    h.run(vec![egui::Event::PointerMoved(center)]);
    h.run(vec![point_scroll(egui::vec2(20.0, -15.0), egui::Modifiers::NONE)]);
    let after = &h.app.state.viewports[index].camera;
    assert_ne!(after.target, before[index].target, "two-finger scrolling must pan");
    assert_eq!(after.distance, before[index].distance, "a pan must not zoom");
    assert_eq!((after.yaw, after.pitch), (before[index].yaw, before[index].pitch));
    for (i, viewport) in h.app.state.viewports.iter().enumerate().filter(|(i, _)| *i != index) {
        assert_eq!(viewport.camera.target, before[i].target);
        assert_eq!(viewport.camera.distance, before[i].distance);
    }
}

#[test]
fn native_pinch_zooms_once_and_does_not_repeat_on_later_frames() {
    let mut h = Harness::new();
    let index = h.app.state.active_viewport;
    let center = h.app.viewport_rects[index].center();
    let distance = h.app.state.viewports[index].camera.distance;
    h.run(vec![egui::Event::PointerMoved(center)]);
    h.run(vec![egui::Event::Zoom(1.25)]);
    let zoomed = h.app.state.viewports[index].camera.distance;
    assert!((zoomed - distance / 1.25).abs() < 1e-5, "pinch factor must reach the 3D camera exactly once");
    h.frames(4);
    assert_eq!(h.app.state.viewports[index].camera.distance, zoomed);
}

#[test]
fn alt_two_finger_orbit_preserves_distance_and_fixed_axis_views_become_free() {
    let mut h = Harness::new();
    let index = 0;
    assert!(h.app.state.viewports[index].kind.angles().is_some());
    let center = h.app.viewport_rects[index].center();
    let before = h.app.state.viewports[index].camera.clone();
    h.mods = egui::Modifiers::ALT;
    h.run(vec![egui::Event::PointerMoved(center)]);
    h.run(vec![point_scroll(egui::vec2(15.0, 10.0), h.mods)]);
    let after = &h.app.state.viewports[index];
    assert_ne!((after.camera.yaw, after.camera.pitch), (before.yaw, before.pitch));
    assert_eq!(after.camera.distance, before.distance);
    assert_eq!(after.kind, loom_commands::ViewKind::Orthographic);
}

#[test]
fn pinch_over_a_side_panel_does_not_change_the_scene_view() {
    let mut h = Harness::new();
    let before: Vec<_> = h.app.state.viewports.iter().map(|v| v.camera.distance).collect();
    h.run(vec![egui::Event::PointerMoved(egui::pos2(SCREEN.right() - 20.0, SCREEN.center().y))]);
    h.run(vec![egui::Event::Zoom(1.25)]);
    assert_eq!(h.app.state.viewports.iter().map(|v| v.camera.distance).collect::<Vec<_>>(), before);
}

#[test]
fn modifier_zoom_uses_each_raw_event_once_without_panning() {
    for modifiers in [egui::Modifiers::CTRL, egui::Modifiers::COMMAND, egui::Modifiers::MAC_CMD, egui::Modifiers { ctrl: true, alt: true, ..egui::Modifiers::NONE }] {
        let mut h = Harness::new();
        let index = h.app.state.active_viewport;
        let center = h.app.viewport_rects[index].center();
        let before = h.app.state.viewports[index].camera.clone();
        h.run(vec![egui::Event::PointerMoved(center)]);
        // Event modifiers differ from the frame's unmodified keyboard state.
        h.run(vec![point_scroll(egui::vec2(7.0, 10.0), modifiers), egui::Event::Zoom(1.25)]);
        let after = &h.app.state.viewports[index].camera;
        let expected = before.distance / (0.02f32.exp() * 1.25);
        assert!((after.distance - expected).abs() < before.distance * 1e-6);
        assert_eq!(after.target, before.target);
        assert_eq!((after.yaw, after.pitch), (before.yaw, before.pitch));
        let distance = after.distance;
        h.frames(4);
        assert_eq!(h.app.state.viewports[index].camera.distance, distance);
    }
}

#[test]
fn both_pan_axes_work_in_orthographic_and_perspective_views() {
    for index in [0, 3] {
        for delta in [egui::vec2(20.0, 0.0), egui::vec2(0.0, 20.0)] {
            let mut h = Harness::new();
            let before = h.app.state.viewports[index].camera.clone();
            let (right, up, _) = before.basis();
            h.run(vec![egui::Event::PointerMoved(h.app.viewport_rects[index].center())]);
            h.run(vec![point_scroll(delta, egui::Modifiers::NONE)]);
            let shift = h.app.state.viewports[index].camera.target - before.target;
            if delta.x != 0.0 { assert!(shift.dot(right) < 0.0); assert!(shift.dot(up).abs() < 1e-4); }
            else { assert!(shift.dot(up) > 0.0); assert!(shift.dot(right).abs() < 1e-4); }
            h.run(vec![point_scroll(-delta, egui::Modifiers::NONE)]);
            assert!(h.app.state.viewports[index].camera.target.distance(before.target) < 1e-4);
        }
    }
}

#[test]
fn explicit_device_modes_cover_x11_and_precision_mice_and_roundtrip_locally() {
    for (mode, serialized) in [(1.0, "mouse"), (2.0, "touchpad")] {
        let mut h = Harness::new();
        h.cmd(Command::SetPreference { name: "Viewport Navigation Device".into(), value: mode });
        let index = h.app.state.active_viewport;
        let before = h.app.state.viewports[index].camera.clone();
        h.run(vec![egui::Event::PointerMoved(h.app.viewport_rects[index].center())]);
        let unit = if mode == 2.0 { egui::MouseWheelUnit::Line } else { egui::MouseWheelUnit::Point };
        h.run(vec![egui::Event::MouseWheel { unit, delta: egui::vec2(0.0, 1.0), modifiers: egui::Modifiers::NONE }]);
        let after = &h.app.state.viewports[index].camera;
        if mode == 2.0 { assert_ne!(after.target, before.target); assert_eq!(after.distance, before.distance); }
        else { assert_eq!(after.target, before.target); assert!(after.distance < before.distance); }
        let value = serde_json::to_value(&h.app.state.prefs).unwrap();
        assert_eq!(value["viewport_navigation_device"], serialized);
        let loaded: loom_commands::state::Prefs = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(loaded, h.app.state.prefs);
        let mut old = value;
        old.as_object_mut().unwrap().remove("viewport_navigation_device");
        let old: loom_commands::state::Prefs = serde_json::from_value(old).unwrap();
        assert_eq!(old.get("Viewport Navigation Device"), Some(0.0));
        for invalid in [-1.0, 0.5, 3.0, f64::NAN, f64::INFINITY] {
            let mut prefs = loaded.clone();
            assert!(prefs.set("Viewport Navigation Device", invalid).is_err());
            assert_eq!(prefs, loaded);
        }
    }
}

#[test]
fn scene_camera_pan_changes_the_node_and_undo_restores_it() {
    let mut h = Harness::new();
    let result = h.app.dispatch(Command::CreateCameraAt { target: true, position: [0.0, -100.0, 20.0], look_at: [0.0, 0.0, 20.0] }).unwrap();
    let camera = loom_scene::NodeId(result["id"].as_u64().unwrap());
    h.cmd(Command::CameraView);
    let index = h.app.state.active_viewport;
    let before = h.app.state.scene.world_matrix(camera);
    h.run(vec![egui::Event::PointerMoved(h.app.viewport_rects[index].center() + egui::vec2(0.0, 80.0))]);
    h.run(vec![point_scroll(egui::vec2(20.0, 0.0), egui::Modifiers::NONE)]);
    assert_ne!(h.app.state.scene.world_matrix(camera), before);
    assert!(!h.app.camera_nav_open);
    h.cmd(Command::UndoSceneOperation);
    assert!(h.app.state.scene.world_matrix(camera).abs_diff_eq(before, 1e-4));
}

#[test]
fn disabled_views_and_zero_orbit_events_do_not_navigate() {
    let mut h = Harness::new();
    let index = 0;
    let before = h.app.state.viewports[index].clone();
    h.run(vec![egui::Event::PointerMoved(h.app.viewport_rects[index].center())]);
    h.run(vec![point_scroll(egui::Vec2::ZERO, egui::Modifiers::ALT)]);
    assert_eq!(h.app.state.viewports[index].kind, before.kind);
    h.app.state.viewports[index].disabled = true;
    h.run(vec![point_scroll(egui::vec2(20.0, 15.0), egui::Modifiers::NONE), egui::Event::Zoom(1.25)]);
    let after = &h.app.state.viewports[index];
    assert_eq!(after.camera.target, before.camera.target);
    assert_eq!(after.camera.distance, before.camera.distance);
}

#[test]
fn native_input_protocol_delivers_pixel_scroll_and_pinch_and_rejects_bad_factors() {
    let mut h = Harness::new();
    let index = h.app.state.active_viewport;
    let before = h.app.state.viewports[index].camera.clone();
    let center = h.app.viewport_rects[index].center();
    inject(&mut h, serde_json::json!([
        {"type":"move", "pos":[center.x,center.y]},
        {"type":"scroll", "unit":"point", "delta":[12.0,0.0]},
        {"type":"zoom", "factor":1.25}
    ]));
    let after = &h.app.state.viewports[index].camera;
    assert_ne!(after.target, before.target);
    assert!((after.distance - before.distance / 1.25).abs() < 1e-4);
    for factor in [serde_json::json!(0), serde_json::json!(-1), serde_json::json!(1e100), serde_json::Value::Null] {
        assert!(crate::live::parse_input(&serde_json::json!([{ "type":"zoom", "factor":factor }])).is_err());
    }
    assert!(crate::live::parse_input(&serde_json::json!([{ "type":"scroll", "delta":[0,1], "unit":"invalid" }])).is_err());
}

#[test]
fn preferences_exposes_the_persistent_navigation_device_choice() {
    let mut h = Harness::new();
    h.cmd(Command::Preferences);
    h.click_text_anywhere("Viewports");
    h.click_text_anywhere("Automatic");
    h.click_text_anywhere("Touchpad");
    assert_eq!(h.app.state.prefs.get("Viewport Navigation Device"), Some(2.0));
}

#[test]
fn automatic_mouse_wheel_and_page_scroll_preserve_the_legacy_zoom_factor() {
    for (unit, amount) in [(egui::MouseWheelUnit::Line, 1.0), (egui::MouseWheelUnit::Page, 0.01)] {
        let mut h = Harness::new();
        let index = h.app.state.active_viewport;
        let before = h.app.state.viewports[index].camera.clone();
        let scale = if unit == egui::MouseWheelUnit::Line { h.ctx.options(|o| o.line_scroll_speed) } else { SCREEN.height() };
        h.run(vec![egui::Event::PointerMoved(h.app.viewport_rects[index].center())]);
        h.run(vec![egui::Event::MouseWheel { unit, delta: egui::vec2(0.0, amount), modifiers: egui::Modifiers::NONE }]);
        let expected = (before.distance * (1.0 - 0.1 * (amount * scale / 50.0)).max(0.1)).clamp(0.05, 10_000.0);
        let after = &h.app.state.viewports[index].camera;
        assert_eq!(after.distance, expected);
        assert_eq!(after.target, before.target);
    }
}

#[test]
fn shift_two_finger_scroll_is_horizontal_like_the_native_input_convention() {
    let mut h = Harness::new();
    let index = h.app.state.active_viewport;
    let before = h.app.state.viewports[index].camera.clone();
    let (right, up, _) = before.basis();
    h.run(vec![egui::Event::PointerMoved(h.app.viewport_rects[index].center())]);
    h.run(vec![point_scroll(egui::vec2(0.0, 15.0), egui::Modifiers::SHIFT)]);
    let shift = h.app.state.viewports[index].camera.target - before.target;
    assert!(shift.dot(right) < 0.0);
    assert!(shift.dot(up).abs() < 1e-4);
    assert_eq!(h.app.state.viewports[index].camera.distance, before.distance);
}

#[test]
fn wheel_before_pinch_keeps_event_order_at_the_zoom_limit() {
    let mut h = Harness::new();
    let index = h.app.state.active_viewport;
    h.app.state.viewports[index].camera.distance = 0.05;
    h.run(vec![egui::Event::PointerMoved(h.app.viewport_rects[index].center())]);
    h.run(vec![
        egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Line, delta: egui::vec2(0.0, -1.0), modifiers: egui::Modifiers::NONE },
        egui::Event::Zoom(2.0),
    ]);
    assert_eq!(h.app.state.viewports[index].camera.distance, 0.05, "zoom out, then pinch back to the minimum");
}

#[test]
fn scene_camera_pinch_and_orbit_have_atomic_undo_in_camera_modes() {
    use loom_input::NavMode as N;
    for (mode, event) in [
        (N::None, egui::Event::Zoom(1.25)),
        (N::None, point_scroll(egui::vec2(20.0, 10.0), egui::Modifiers::ALT)),
        (N::Fov, egui::Event::Zoom(1.25)),
        (N::PanCamera, point_scroll(egui::vec2(20.0, 10.0), egui::Modifiers::ALT)),
    ] {
        let mut h = Harness::new();
        let result = h.app.dispatch(Command::CreateCameraAt { target: true, position: [0.0, -100.0, 20.0], look_at: [0.0, 0.0, 20.0] }).unwrap();
        let camera = loom_scene::NodeId(result["id"].as_u64().unwrap());
        h.cmd(Command::CameraView);
        h.app.state.nav_mode = mode;
        let index = h.app.state.active_viewport;
        let before = serde_json::to_value(h.app.state.scene.node(camera).unwrap()).unwrap();
        let matrix = h.app.state.scene.world_matrix(camera);
        h.run(vec![egui::Event::PointerMoved(h.app.viewport_rects[index].center() + egui::vec2(0.0, 80.0))]);
        h.run(vec![event]);
        assert_ne!(serde_json::to_value(h.app.state.scene.node(camera).unwrap()).unwrap(), before);
        if matches!(mode, N::Fov | N::PanCamera) {
            assert!(h.app.state.scene.world_matrix(camera).w_axis.abs_diff_eq(matrix.w_axis, 1e-4), "optical zoom and pan-camera preserve the eye position");
        }
        assert!(!h.app.camera_nav_open);
        h.cmd(Command::UndoSceneOperation);
        assert_eq!(serde_json::to_value(h.app.state.scene.node(camera).unwrap()).unwrap(), before);
    }
}
