use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp, ViewKind, ViewportLayout};

fn state() -> AppState {
    AppState::new()
}

fn box_at(s: &mut AppState, x: f32) -> loom_scene::NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [x, 0.0, 0.0], size: [1.0, 1.0, 1.0] }).unwrap();
    *s.selection.iter().next().unwrap()
}

#[test]
fn default_state_is_top_front_left_perspective_active_3() {
    let s = state();
    let kinds: Vec<ViewKind> = s.viewports.iter().map(|v| v.kind).collect();
    assert_eq!(kinds, vec![ViewKind::Top, ViewKind::Front, ViewKind::Left, ViewKind::Perspective]);
    assert_eq!(s.active_viewport, 3);
    assert_eq!(s.layout, ViewportLayout::FourGrid);
    assert!(s.viewports[0].is_orthographic() && !s.viewports[3].is_orthographic());
}

#[test]
fn view_commands_change_only_active_viewport() {
    let mut s = state();
    execute(&mut s, Command::ActiveViewport { index: 0 }).unwrap();
    execute(&mut s, Command::FrontView).unwrap();
    assert_eq!(s.viewports[0].kind, ViewKind::Front);
    assert_eq!(s.viewports[1].kind, ViewKind::Front);
    assert_eq!(s.viewports[3].kind, ViewKind::Perspective);
    execute(&mut s, Command::UndoViewportOperation).unwrap();
    assert_eq!(s.viewports[0].kind, ViewKind::Top, "viewport undo is per viewport");
}

#[test]
fn maximize_toggle_round_trips() {
    let mut s = state();
    execute(&mut s, Command::MaximizeViewportToggle).unwrap();
    assert!(s.maximized);
    execute(&mut s, Command::MaximizeViewportToggle).unwrap();
    assert!(!s.maximized);
}

#[test]
fn zoom_extents_on_empty_scene_keeps_camera() {
    let mut s = state();
    let before = s.active().camera.distance;
    execute(&mut s, Command::ZoomExtents).unwrap();
    let after = s.active().camera.distance;
    assert!(after.is_finite() && (after - before).abs() < 1e-6);
}

#[test]
fn viewport_layout_command_resizes_viewport_list() {
    let mut s = state();
    execute(&mut s, Command::ViewportLayout { layout: ViewportLayout::Single }).unwrap();
    assert_eq!(s.viewports.len(), 1);
    assert_eq!(s.viewports[0].kind, ViewKind::Perspective);
    assert_eq!(s.active_viewport, 0);
    execute(&mut s, Command::ViewportLayout { layout: ViewportLayout::ThreeLeftBig }).unwrap();
    assert_eq!(s.viewports.len(), 3);
}

#[test]
fn isolate_selection_restores_previously_hidden() {
    let mut s = state();
    let a = box_at(&mut s, 0.0);
    let b = box_at(&mut s, 2.0);
    let c = box_at(&mut s, 4.0);
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::HideSelection).unwrap();
    execute(&mut s, Command::IsolateSelection).unwrap();
    assert!(s.isolation.is_none(), "nothing selected: no-op");
    execute(&mut s, Command::SelectNodes { ids: vec![b.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::IsolateSelection).unwrap();
    assert!(s.isolation.is_some());
    assert!(s.scene.node(c).unwrap().hidden && s.scene.node(a).unwrap().hidden && !s.scene.node(b).unwrap().hidden);
    execute(&mut s, Command::ExitIsolation).unwrap();
    assert!(!s.scene.node(c).unwrap().hidden);
    assert!(s.scene.node(a).unwrap().hidden, "hidden before isolation stays hidden");
}

#[test]
fn hide_geometry_toggle_hides_boxes_but_not_shapes() {
    let mut s = state();
    let a = box_at(&mut s, 0.0);
    execute(&mut s, Command::HideGeometryToggle).unwrap();
    assert!(s.hide_category.geometry);
    assert!(!s.scene.node(a).unwrap().hidden, "node flag untouched");
    assert!(!s.is_visible(s.scene.node(a).unwrap()));
    assert!(s.scene_bounds().is_none(), "hidden category is excluded from bounds");
}

#[test]
fn nav_mode_toggles_off_when_run_twice() {
    let mut s = state();
    execute(&mut s, Command::PanView).unwrap();
    assert_eq!(format!("{:?}", s.nav_mode), "Pan");
    execute(&mut s, Command::PanView).unwrap();
    assert_eq!(format!("{:?}", s.nav_mode), "None");
}

#[test]
fn virtual_viewport_rows_are_out_of_scope() {
    assert!(loom_commands::OUT_OF_SCOPE.iter().any(|(n, _)| *n == "Virtual Viewport Toggle"));
    assert!(!loom_commands::DEFERRED.iter().any(|(n, _)| n.starts_with("Virtual Viewport")));
}
