//! Helpers and precision tools, clicked as a person does: the Helpers
//! Object Type buttons, dragging out Tape / Grid / Compass, Protractor picks,
//! activating a grid from the quad menu, AutoGrid with Alt and Tools >
//! Measure Distance.

use super::*;
use loom_scene::ObjectKind;

fn helper_nodes(h: &Harness, f: impl Fn(&ObjectKind) -> bool) -> Vec<loom_scene::NodeId> {
    h.app.state.scene.nodes().filter(|n| f(&n.object)).map(|n| n.id).collect()
}

fn helpers_panel(h: &mut Harness) {
    h.click(h.find(egui::accesskit::Role::Button, "Helpers").center());
}

#[test]
fn helpers_object_type_lists_the_standard_helpers() {
    let mut h = Harness::new();
    helpers_panel(&mut h);
    for label in ["Dummy", "Point", "Grid", "Tape", "Protractor", "Compass"] {
        assert_eq!(h.widgets_labelled(label).len(), 1, "{label} button");
    }
    h.click_widget("Protractor");
    assert_eq!(h.app.create_kind, Some(crate::CreateKind::Helper(crate::HelperKind::Protractor)));
}

#[test]
fn tape_is_dragged_from_the_tape_to_its_target() {
    let mut h = Harness::new();
    keep_creating(&mut h, false);
    wide_view(&mut h);
    helpers_panel(&mut h);
    h.click_widget("Tape");
    let (a, b) = (h.screen_of(Vec3::new(-40.0, 0.0, 0.0)), h.screen_of(Vec3::new(30.0, 0.0, 0.0)));
    h.drag(a, b);
    let tapes = helper_nodes(&h, |o| matches!(o, ObjectKind::Tape { .. }));
    assert_eq!(tapes.len(), 1);
    let ObjectKind::Tape { target: Some(t), .. } = h.app.state.scene.node(tapes[0]).unwrap().object else { panic!() };
    let start = h.app.state.scene.world_matrix(tapes[0]).w_axis.truncate();
    let end = h.app.state.scene.world_matrix(t).w_axis.truncate();
    assert!(start.distance(Vec3::new(-40.0, 0.0, 0.0)) < 2.0 && end.distance(Vec3::new(30.0, 0.0, 0.0)) < 2.0, "{start} {end}");
    assert!(h.app.create_kind.is_none(), "the drag ends the mode");
    // The Modify panel shows the measured length.
    h.frames(2);
    assert_eq!(h.app.panel_tab, crate::Tab::Modify);
    assert!(h.has(egui::accesskit::Role::CheckBox, "Specify Length"));
    assert!(h.has(egui::accesskit::Role::Label, "To X Axis:"));
    // One undo removes both ends.
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(h.app.state.scene.len(), 0);
}

#[test]
fn grid_is_dragged_corner_to_corner_and_activated_from_the_quad_menu() {
    let mut h = Harness::new();
    keep_creating(&mut h, false);
    wide_view(&mut h);
    helpers_panel(&mut h);
    h.click_widget("Grid");
    let (a, b) = (h.screen_of(Vec3::new(-40.0, -30.0, 0.0)), h.screen_of(Vec3::new(40.0, 30.0, 0.0)));
    h.drag(a, b);
    let grids = helper_nodes(&h, |o| matches!(o, ObjectKind::Grid { .. }));
    assert_eq!(grids.len(), 1);
    let g = grids[0];
    let ObjectKind::Grid { length, width, .. } = &h.app.state.scene.node(g).unwrap().object else { panic!() };
    assert!((width.get() - 80.0).abs() < 3.0 && (length.get() - 60.0).abs() < 3.0, "{} x {}", width.get(), length.get());
    // Right-click the selected grid: tools 1 activates it.
    let at = h.app.viewport_rects[h.app.state.active_viewport].center() + egui::vec2(-150.0, 80.0);
    right_click(&mut h, at);
    h.click_text_anywhere("Activate Grid");
    assert_eq!(h.app.state.active_grid(), Some(g));
    right_click(&mut h, at);
    h.click_text_anywhere("Activate Home Grid");
    assert_eq!(h.app.state.active_grid(), None);
}

#[test]
fn objects_are_created_on_the_active_grid() {
    let mut h = Harness::new();
    keep_creating(&mut h, false);
    h.cmd(Command::CreateHelper { kind: "grid".into(), position: [0.0, 0.0, 25.0], size: 200.0 });
    h.cmd(Command::ActivateGridObject);
    wide_view(&mut h);
    h.app.start_create(Some(crate::CreateKind::Helper(crate::HelperKind::Dummy)));
    let p = h.screen_of(Vec3::new(10.0, 10.0, 25.0));
    h.click(p);
    let dummy = helper_nodes(&h, |o| matches!(o, ObjectKind::Dummy { .. }))[0];
    let at = h.app.state.scene.world_matrix(dummy).w_axis.truncate();
    assert!(at.distance(Vec3::new(10.0, 10.0, 25.0)) < 1.0, "built on the grid's plane: {at}");
}

#[test]
fn compass_drag_sets_its_radius() {
    let mut h = Harness::new();
    keep_creating(&mut h, false);
    wide_view(&mut h);
    helpers_panel(&mut h);
    h.click_widget("Compass");
    let (a, b) = (h.screen_of(Vec3::ZERO), h.screen_of(Vec3::new(30.0, 0.0, 0.0)));
    h.drag(a, b);
    let c = helper_nodes(&h, |o| matches!(o, ObjectKind::Compass { .. }));
    assert_eq!(c.len(), 1);
    let ObjectKind::Compass { radius, .. } = &h.app.state.scene.node(c[0]).unwrap().object else { panic!() };
    assert!((radius.get() - 30.0).abs() < 2.0, "{}", radius.get());
}

#[test]
fn protractor_pick_object_buttons_pick_in_the_viewport() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [40.0, 0.0, 0.0], size: [10.0; 3] });
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0, 40.0, 0.0], size: [10.0; 3] });
    h.cmd(Command::CreateHelper { kind: "protractor".into(), position: [0.0; 3], size: 0.0 });
    let p = *h.app.state.selection.iter().next().unwrap();
    wide_view(&mut h);
    h.app.panel_tab = crate::Tab::Modify;
    h.frames(2);
    h.click(panel_text(&h, "Pick Object 1"));
    assert_eq!(h.app.state.pick_mode, Some(loom_commands::PickMode::ProtractorObject(1)));
    let pa = h.screen_of(Vec3::new(40.0, 0.0, 5.0));
    h.click(pa);
    h.click(panel_text(&h, "Pick Object 2"));
    let pb = h.screen_of(Vec3::new(0.0, 40.0, 5.0));
    h.click(pb);
    let r = h.app.dispatch(Command::HelperReadout { id: p.0 }).unwrap();
    assert!((r["angle"].as_f64().unwrap() - 90.0).abs() < 1e-3, "{r}");
    h.frames(2);
    assert_eq!(h.app.state.selection.iter().copied().collect::<Vec<_>>(), vec![p]);
    assert!(h.text_centers("Box001").len() + h.text_centers("Box002").len() >= 2, "the picked names show above the buttons");
}

#[test]
fn autogrid_with_alt_keeps_an_active_grid_on_the_surface() {
    let mut h = Harness::new();
    keep_creating(&mut h, false);
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [60.0; 3] });
    wide_view(&mut h);
    h.app.start_create(Some(crate::CreateKind::Helper(crate::HelperKind::Point)));
    h.app.autogrid = true;
    let top = h.screen_of(Vec3::new(5.0, 5.0, 60.0));
    h.mods = egui::Modifiers::ALT;
    h.click(top);
    h.mods = egui::Modifiers::NONE;
    let grid = h.app.state.active_grid().expect("Alt kept the AutoGrid as an active grid object");
    let at = h.app.state.scene.world_matrix(grid).w_axis.truncate();
    assert!((at.z - 60.0).abs() < 0.5, "on the box top: {at}");
    assert!(!h.app.autogrid, "AutoGrid turns off");
    let point = helper_nodes(&h, |o| matches!(o, ObjectKind::Point { .. }));
    assert_eq!(point.len(), 1, "the point is created too");
    assert!((h.app.state.scene.world_matrix(point[0]).w_axis.z - 60.0).abs() < 0.5);
}

#[test]
fn measure_distance_from_the_tools_menu() {
    let mut h = Harness::new();
    wide_view(&mut h);
    h.click(h.find(egui::accesskit::Role::Button, "Tools").center());
    h.click(h.find(egui::accesskit::Role::Button, "Measure Distance").center());
    assert_eq!(h.app.state.pick_mode, Some(loom_commands::PickMode::MeasureDistanceA));
    // Grid Points snapping keeps the picks on whole grid points.
    h.cmd(Command::SnapsToggle);
    h.click(h.screen_of(Vec3::new(0.0, 0.0, 0.0)));
    h.click(h.screen_of(Vec3::new(30.0, 40.0, 0.0)));
    assert_eq!(h.app.state.pick_mode, None);
    assert!(h.app.state.status_prompt.starts_with("Distance: 50"), "{}", h.app.state.status_prompt);
}
