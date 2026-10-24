//! Object Properties, the Object Color dialog, the Display panel and the
//! viewport statistics, clicked as a person does.

use super::*;
use egui::accesskit::Role;

fn boxed(h: &mut Harness) -> loom_scene::NodeId {
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    *h.app.state.selection.iter().next().expect("the box is selected")
}

#[test]
fn object_properties_rendering_control_and_user_defined_panel() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1000.0));
    let id = boxed(&mut h);
    h.cmd(Command::ObjectProperties);
    h.click(h.find(Role::CheckBox, "Cast Shadows").center());
    assert!(!h.app.state.scene.node(id).unwrap().render.cast_shadows, "the Cast Shadows box turns it off");
    h.click(h.find(Role::CheckBox, "Visible to Camera").center());
    assert!(!h.app.state.scene.node(id).unwrap().render.visible_to_camera);
    h.click(h.find(Role::CheckBox, "Edges Only").center());
    assert!(!h.app.state.scene.node(id).unwrap().display.edges_only);
    assert!(h.app.world_styles()[0].diagonals, "Edges Only off draws the diagonals");
    // User Defined: typed text is kept on OK.
    h.click(h.find(Role::Button, "User Defined").center());
    h.click(h.find(Role::MultilineTextInput, "").center());
    h.run(vec![egui::Event::Text("lod=1".into())]);
    h.frames(1);
    h.click(h.find(Role::Button, "OK").center());
    assert_eq!(h.app.state.scene.node(id).unwrap().user_props, "lod=1");
}

#[test]
fn object_properties_freeze_acts_on_the_selection_only() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1000.0));
    let a = boxed(&mut h);
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [30.0, 0.0, 0.0], size: [10.0; 3] });
    let b = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::FreezeNodes { ids: vec![a.0] });
    h.cmd(Command::SelectNodes { ids: vec![b.0], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::ObjectProperties);
    h.click(h.find(Role::CheckBox, "Hide").center());
    assert!(h.app.state.scene.node(b).unwrap().hidden, "Hide hides the selection");
    assert!(!h.app.state.scene.node(a).unwrap().hidden);
    assert!(h.app.state.scene.node(a).unwrap().frozen, "the other object keeps its own state");
}

#[test]
fn the_colour_swatch_opens_object_color_and_ok_colours_the_object() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1000.0));
    let id = boxed(&mut h);
    h.cmd(Command::ObjectProperties);
    h.click(h.find(Role::Button, "Object Color").center());
    assert!(h.app.state.display.object_color_open);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::ObjectProperties), "Object Properties stays open under it");
    h.click(h.find(Role::Button, "Basic Color 3").center());
    // Object Color's OK sits beside its Cancel (Object Properties has none).
    let cancel = h.find(Role::Button, "Cancel");
    let ok = h.find_all(Role::Button, "OK").into_iter().min_by(|p, q| p.center().distance(cancel.center()).total_cmp(&q.center().distance(cancel.center()))).unwrap();
    h.click(ok.center());
    assert!(!h.app.state.display.object_color_open, "OK closes the dialog");
    assert_eq!(h.app.state.scene.node(id).unwrap().wire_color, loom_scene::OBJECT_COLORS[2]);
}

#[test]
fn display_panel_display_color_hide_frozen_and_link_display() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1400.0));
    let id = boxed(&mut h);
    h.app.panel_tab = crate::Tab::Display;
    h.frames(2);
    h.click_widget("Display Color");
    let material = h.find_all(Role::RadioButton, "Material Color");
    assert_eq!(material.len(), 2, "Wireframe and Shaded each offer Material Color");
    h.click(material[0].center());
    assert_eq!(h.app.state.display.wireframe_color, loom_commands::display::ColorSource::Material);
    h.click(h.find(Role::CheckBox, "Hide Frozen Objects").center());
    assert!(h.app.state.display.hide_frozen);
    h.click_widget("Link Display");
    h.click(h.find(Role::CheckBox, "Display Links").center());
    assert!(h.app.state.scene.node(id).unwrap().display.display_links);
    h.click(h.find(Role::CheckBox, "Bone Objects").center());
    assert!(h.app.state.display.hide_bones);
}

#[test]
fn seven_shows_the_statistics_of_the_scene() {
    let mut h = Harness::new();
    boxed(&mut h);
    h.key(egui::Key::Num7);
    assert!(h.app.state.show_statistics);
    assert_eq!(crate::viewport_widget::statistics_counts(&h.app, false), [6, 12, 12, 8], "a box: 6 polygons, 12 triangles, 12 edges, 8 vertices");
    assert!(!h.texts("Polys:").is_empty() && !h.texts("Total").is_empty(), "the statistics show in the active viewport");
    h.cmd(Command::SetStatistics { polygons: Some(false), triangles: None, edges: None, vertices: None, fps: None, scope: None, default_settings: None });
    assert!(h.texts("Polys:").is_empty(), "Polygon Count off hides its line");
}
