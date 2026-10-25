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

#[test]
fn measure_utility_shows_the_name_lock_selected_and_a_floater() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 1400.0));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    h.app.panel_tab = crate::Tab::Utilities;
    h.frames(2);
    h.click_widget("Measure");
    assert_eq!(h.widgets_labelled("Box001").len(), 1, "the object's name");
    assert_eq!(h.widgets_labelled("Volume: 1000.000").len(), 1);
    h.click(h.find(egui::accesskit::Role::CheckBox, "Lock Selected").center());
    h.cmd(Command::SelectNone);
    assert_eq!(h.widgets_labelled("Box001").len(), 1, "Lock Selected keeps measuring the box");
    h.click_widget("New Floater");
    assert!(h.app.measure_floater);
    assert_eq!(h.text_rects("Volume: 1000.000").len(), 2, "the floater shows the same read-outs");
    h.click(h.find(egui::accesskit::Role::CheckBox, "Lock Selected").center());
    assert_eq!(h.widgets_labelled("Box001").len(), 0, "unlocked: nothing selected");
}

#[test]
fn snapshot_dialog_from_the_tools_menu_makes_range_copies() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    h.cmd(Command::AutoKeyModeToggle);
    h.cmd(Command::SetFrame { frame: 100 });
    h.cmd(Command::MoveSelection { delta: [100.0, 0.0, 0.0], system: None });
    h.cmd(Command::AutoKeyModeToggle);
    h.cmd(Command::SetFrame { frame: 0 });
    h.click(h.find(egui::accesskit::Role::Button, "Tools").center());
    h.click(h.find(egui::accesskit::Role::Button, "Snapshot...").center());
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::Snapshot));
    h.click_text_anywhere("Range");
    h.click_text_anywhere("OK");
    assert_eq!(h.app.state.dialog, None);
    assert_eq!(h.app.state.scene.len(), 11, "the box and 10 copies from frame 0 to 100");
}

#[test]
fn clone_and_align_dialog_picks_destinations_previews_and_applies() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [60.0, 0.0, 0.0], size: [10.0; 3] });
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0, 60.0, 0.0], size: [10.0; 3] });
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Sphere, position: [0.0; 3], size: [8.0; 3] });
    wide_view(&mut h);
    h.cmd(Command::CloneAndAlignDialog);
    h.click_text_anywhere("Pick");
    assert_eq!(h.app.state.pick_mode, Some(loom_commands::PickMode::CloneAlignDestination));
    let (a, b) = (h.screen_of(Vec3::new(60.0, 0.0, 5.0)), h.screen_of(Vec3::new(0.0, 60.0, 5.0)));
    h.click(a);
    h.click(b);
    assert_eq!(h.app.state.clone_align_destinations.len(), 2);
    assert_eq!(h.app.state.scene.len(), 3, "no preview while picking");
    h.click_text_anywhere("Pick");
    h.frames(2);
    assert_eq!(h.app.state.scene.len(), 5, "a preview clone on each destination");
    h.click_text_anywhere("Apply");
    h.frames(2);
    h.click_text_anywhere("Cancel");
    h.frames(2);
    assert_eq!(h.app.state.dialog, None);
    assert_eq!(h.app.state.scene.len(), 5, "the applied clones stay");
    let spheres: Vec<Vec3> = h.app.state.scene.nodes().filter(|n| matches!(n.object, ObjectKind::Sphere { .. })).map(|n| h.app.state.scene.world_matrix(n.id).w_axis.truncate()).collect();
    assert!(spheres.iter().any(|p| p.distance(Vec3::new(60.0, 0.0, 0.0)) < 1e-3) && spheres.iter().any(|p| p.distance(Vec3::new(0.0, 60.0, 0.0)) < 1e-3), "{spheres:?}");
}

#[test]
fn place_highlight_drag_moves_the_selection_onto_the_reflected_ray() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0; 3], size: [300.0, 300.0, 0.0] });
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0, 0.0, 100.0], size: [2.0; 3] });
    let lamp = *h.app.state.selection.iter().next().unwrap();
    wide_view(&mut h);
    h.cmd(Command::PlaceHighlight);
    let (a, b) = (h.screen_of(Vec3::new(20.0, 20.0, 0.0)), h.screen_of(Vec3::ZERO));
    h.drag(a, b);
    assert_eq!(h.app.state.pick_mode, None, "the release ends Place Highlight");
    let at = h.app.state.scene.world_matrix(lamp).w_axis.truncate();
    assert!((at.length() - 100.0).abs() < 1.0, "it keeps its distance from the picked point: {at}");
    assert!(at.z > 10.0 && at.distance(Vec3::new(0.0, 0.0, 100.0)) > 5.0, "on the reflected ray: {at}");
    h.cmd(Command::UndoSceneOperation);
    let back = h.app.state.scene.world_matrix(lamp).w_axis.truncate();
    assert!(back.distance(Vec3::new(0.0, 0.0, 100.0)) < 1e-3, "one undo step: {back}");
}

/// Types `text` into the spinner at `at` (click, select all, type, Enter).
fn type_into(h: &mut Harness, at: egui::Pos2, text: &str) {
    h.click(at);
    h.mods = egui::Modifiers::COMMAND;
    h.key(egui::Key::A);
    h.mods = egui::Modifiers::NONE;
    h.run(vec![egui::Event::Text(text.into())]);
    h.key(egui::Key::Enter);
}

/// The spinners on the row of the widget `row` (left to right).
fn row_spinners(h: &Harness, row: egui::Rect) -> Vec<(String, egui::Rect)> {
    let mut v: Vec<(String, egui::Rect)> = h.widgets.iter().filter(|(r, _, w)| *r == egui::accesskit::Role::SpinButton && (w.center().y - row.center().y).abs() < row.height().max(8.0) && w.min.x > row.min.x).map(|(_, t, w)| (t.clone(), *w)).collect();
    v.sort_by(|a, b| a.1.min.x.total_cmp(&b.1.min.x));
    v
}

#[test]
fn array_dialog_shows_equivalent_totals_previews_as_boxes_and_keeps_its_settings() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    h.cmd(Command::Array);
    let move_label = h.find(egui::accesskit::Role::Label, "Move");
    let inc_x = row_spinners(&h, move_label)[0].1;
    type_into(&mut h, inc_x.center(), "25");
    let one_d = h.find(egui::accesskit::Role::RadioButton, "1D");
    let count = row_spinners(&h, one_d)[0].1;
    type_into(&mut h, count.center(), "3");
    let move_label = h.find(egui::accesskit::Role::Label, "Move");
    let row = row_spinners(&h, move_label);
    assert_eq!(row.len(), 6, "{row:?}");
    assert!(row[0].0.starts_with("25"), "{row:?}");
    assert!(row[3].0.starts_with("75"), "the Totals side shows the equivalent 3 × 25: {row:?}");
    // Switch Move to Totals: the array stays the same.
    let arrows = h.text_rects(">");
    h.click(arrows[0].center());
    let row = row_spinners(&h, h.find(egui::accesskit::Role::Label, "Move"));
    assert!(row[3].0.starts_with("75") && row[0].0.starts_with("25"), "{row:?}");
    // Display as Box, then Preview: the new members show as boxes.
    h.click(h.find(egui::accesskit::Role::CheckBox, "Display as Box").center());
    h.click_text_anywhere("Preview");
    h.frames(2);
    assert_eq!(h.app.state.scene.len(), 3);
    assert_eq!(h.app.state.scene.nodes().filter(|n| n.display.display_as_box).count(), 2, "preview members as boxes");
    h.click_text_anywhere("OK");
    h.frames(2);
    assert_eq!(h.app.state.scene.len(), 3, "OK keeps one array");
    assert_eq!(h.app.state.scene.nodes().filter(|n| n.display.display_as_box).count(), 0, "the real members are not boxes");
    let xs: std::collections::BTreeSet<i32> = h.app.state.scene.nodes().map(|n| h.app.state.scene.world_matrix(n.id).w_axis.x.round() as i32).collect();
    assert_eq!(xs, [0, 25, 50].into_iter().collect());
    // The dialog keeps its settings for the session.
    h.cmd(Command::Array);
    let row = row_spinners(&h, h.find(egui::accesskit::Role::Label, "Move"));
    assert!(row[3].0.starts_with("75"), "{row:?}");
}

#[test]
fn spacing_tool_picks_a_path_and_uses_the_distribution_list() {
    let mut h = Harness::new();
    h.cmd(Command::CreateShape { kind: loom_commands::ShapeKind::Arc, position: [0.0; 3], size: [60.0, 60.0, 0.0] });
    let arc = *h.app.state.selection.iter().next().unwrap();
    for (name, v) in [("Radius", 30.0), ("From", 0.0), ("To", 180.0)] {
        h.cmd(Command::SetParameter { id: arc.0, name: name.into(), value: v });
    }
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0, 0.0, -60.0], size: [4.0; 3] });
    let b = *h.app.state.selection.iter().next().unwrap();
    wide_view(&mut h);
    h.cmd(Command::SpacingTool);
    h.click_text_anywhere("Pick Path");
    assert_eq!(h.app.state.pick_mode, Some(loom_commands::PickMode::SpacingPath));
    let on_arc = h.screen_of(Vec3::new(0.0, 30.0, 0.0));
    h.click(on_arc);
    assert_eq!(h.app.state.spacing_path, Some(arc), "the click on the arc picked it");
    assert_eq!(h.app.state.selection.iter().copied().collect::<Vec<_>>(), vec![b]);
    assert_eq!(h.text_rects("Arc001").len(), 1, "the path's name shows");
    h.click_text_anywhere("Divide Evenly, Objects at Ends");
    h.click_text_anywhere("Centered, Specify Spacing");
    h.click_text_anywhere("Apply");
    // Spacing 10 along the half circle (30π ≈ 94.2 long): ten objects, centred on the path.
    let mut at: Vec<Vec3> = h.app.state.selection.iter().map(|id| h.app.state.scene.world_matrix(*id).w_axis.truncate()).collect();
    at.sort_by(|a, b| b.x.total_cmp(&a.x));
    assert_eq!(at.len(), 10, "{at:?}");
    assert!((at[0].x + at[9].x).abs() < 0.2 && (at[0].y - at[9].y).abs() < 0.2, "centred: {at:?}");
    assert!(at.iter().all(|p| (p.length() - 30.0).abs() < 0.2), "on the arc: {at:?}");
}

#[test]
fn grid_and_snap_settings_shows_the_override_marker_colour_and_user_grids() {
    let mut h = Harness::new();
    h.cmd(Command::GridAndSnapSettings);
    assert_eq!(h.text_rects("Override OFF").len(), 1);
    h.cmd(Command::SnapOverride { name: Some("Vertex".into()) });
    assert_eq!(h.text_rects("Override Vertex").len(), 1, "the label names the override in use");
    h.click_text_anywhere("User Grids");
    h.click(h.find(egui::accesskit::Role::CheckBox, "Activate grids when creating").center());
    assert!(h.app.state.user_grids.activate_when_created);
    h.click_text_anywhere("Object space");
    assert!(h.app.state.user_grids.autogrid_object_space);
    h.click_text_anywhere("Options");
    assert_eq!(h.text_rects("Color:").len(), 1, "Marker > Color swatch");
}
