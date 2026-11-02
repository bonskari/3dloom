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
    assert!(!h.text_rects("Polys:").is_empty() && !h.text_rects("Total").is_empty(), "the statistics show in the active viewport");
    h.cmd(Command::SetStatistics { polygons: Some(false), triangles: None, edges: None, vertices: None, fps: None, scope: None, default_settings: None });
    assert!(h.text_rects("Polys:").is_empty(), "Polygon Count off hides its line");
}

/// Display quad: Hide / Freeze Selection's Layer act on the selected
/// object's layer (one undo step each); Manage Scene States opens its dialog.
#[test]
fn display_quad_hides_and_freezes_the_selections_layer_and_opens_scene_states() {
    let mut h = Harness::new();
    let a = boxed(&mut h);
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [40.0, 0.0, 0.0], size: [10.0; 3] });
    let b = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::SelectNodes { ids: vec![a.0], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::CreateLayer { name: Some("Walls".into()) });
    let walls = h.app.state.scene.node(a).unwrap().layer;
    assert_ne!(walls, 0, "the new layer took the selection");
    assert_eq!(h.app.state.scene.node(b).unwrap().layer, 0);
    maximized(&mut h);
    let r = h.app.viewport_rects[h.app.state.active_viewport];
    let at = r.center() + egui::vec2(-150.0, 50.0);
    let quad_item = |h: &mut Harness, label: &str| {
        right_click(h, at);
        let display = menus(h).into_iter().find(|m| m.min.x > at.x && m.max.y < at.y).expect("the display quadrant");
        let p = h.text_rect(label, display).unwrap_or_else(|| panic!("no {label}")).center();
        h.click(p);
    };
    quad_item(&mut h, "Freeze Selection's Layer");
    assert!(h.app.state.scene.layer(walls).unwrap().frozen);
    assert!(!h.app.state.scene.layer(0).unwrap().frozen, "the other layer stays");
    h.cmd(Command::UndoSceneOperation);
    assert!(!h.app.state.scene.layer(walls).unwrap().frozen, "one undo step");
    quad_item(&mut h, "Hide Selection's Layer");
    assert!(h.app.state.scene.layer(walls).unwrap().hidden);
    quad_item(&mut h, "Manage Scene States...");
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::ManageSceneStates));    // Transform quadrant (down and right of the click), with the visible box selected: Wire Parameters.
    h.cmd(Command::SelectNodes { ids: vec![b.0], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::CloseDialog);
    right_click(&mut h, at);
    let p = h.text_rects("Wire Parameters...").into_iter().find(|r| r.min.x > at.x && r.min.y > at.y).unwrap_or_else(|| panic!("Wire Parameters in the transform quadrant: {:?} {:?}", h.text_rects("Wire Parameters..."), h.app.state.selection)).center();
    h.click(p);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::ParameterWiring));
}

/// Alt+right-click > pose quadrant (lower left): Set as Skin Pose stores the
/// box's transform and Assume Skin Pose brings it back; Set / Assume Pref
/// Angles keep and restore its rotation as the IK preferred angles.
#[test]
fn animation_quad_pose_quadrant_skin_pose_and_pref_angles() {
    let mut h = Harness::new();
    let id = boxed(&mut h);
    h.cmd(Command::SetTransform { id: id.0, position: None, rotation_deg: Some([10.0, 20.0, 30.0]), scale: None });
    maximized(&mut h);
    let r = h.app.viewport_rects[h.app.state.active_viewport];
    let at = r.center() + egui::vec2(150.0, -50.0);
    let pick = |h: &mut Harness, label: &str| {
        h.mods = egui::Modifiers::ALT;
        right_click(h, at);
        h.mods = egui::Modifiers::NONE;
        let p = h.text_rects(label).into_iter().find(|t| t.max.x < at.x && t.min.y > at.y).unwrap_or_else(|| panic!("{label} in the lower-left quadrant")).center();
        h.click(p);
    };
    pick(&mut h, "Set as Skin Pose");
    pick(&mut h, "Set Pref Angles");
    assert_eq!(h.app.state.scene.node(id).unwrap().joints.rotational.map(|a| a.preferred), [10.0, 20.0, 30.0]);
    h.cmd(Command::SetTransform { id: id.0, position: Some([50.0, 0.0, 0.0]), rotation_deg: Some([0.0; 3]), scale: None });
    pick(&mut h, "Assume Pref Angles");
    let rot = h.app.state.scene.node(id).unwrap().transform.rotation.get();
    assert!((rot - glam::Vec3::new(10.0, 20.0, 30.0)).length() < 1e-4, "{rot}");
    pick(&mut h, "Assume Skin Pose");
    assert_eq!(h.app.state.scene.node(id).unwrap().transform.position.get(), glam::Vec3::ZERO, "back to the skin pose");
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(h.app.state.scene.node(id).unwrap().transform.position.get(), glam::Vec3::new(50.0, 0.0, 0.0), "one undo step");
}

/// Animation > Constraints > Surface Constraint: pick a sphere, then U / V
/// Position move the box over it and Align to U turns it to the surface.
#[test]
fn surface_constraint_puts_the_selection_on_a_sphere() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1000.0));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Sphere, position: [0.0; 3], size: [40.0, 40.0, 0.0] });
    let sphere = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [60.0, 0.0, 0.0], size: [4.0; 3] });
    let b = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::SurfaceConstraint { target: sphere.0 });
    let centre = h.app.state.scene.world_matrix(sphere).w_axis.truncate();
    let pos = |h: &Harness| h.app.state.scene.world_matrix(b).w_axis.truncate();
    assert!(((pos(&h) - centre).length() - 20.0).abs() < 1e-2, "on the sphere: {}", pos(&h));
    h.cmd(Command::SetConstraintParameter { id: b.0, name: "V Position".into(), value: 100.0 });
    assert!((pos(&h) - (centre + glam::Vec3::Z * 20.0)).length() < 1e-2, "V 100: the top pole {}", pos(&h));
    h.cmd(Command::SetConstraintParameter { id: b.0, name: "V Position".into(), value: 50.0 });
    h.cmd(Command::SetConstraintParameter { id: b.0, name: "U Position".into(), value: 25.0 });
    assert!((pos(&h) - (centre + glam::Vec3::Y * 20.0)).length() < 1e-2, "U 25 on the equator: +Y {}", pos(&h));
    h.cmd(Command::SetConstraintParameter { id: b.0, name: "Alignment".into(), value: 1.0 });
    let z = h.app.state.scene.world_matrix(b).z_axis.truncate().normalize();
    assert!(z.dot(glam::Vec3::Y) > 0.99, "Z on the normal: {z}");
    // The Motion panel shows its rollout.
    h.app.panel_tab = crate::Tab::Motion;
    h.frames(2);
    assert!(!h.text_rects("Surface Controller Parameters").is_empty());
    h.click(h.find(Role::RadioButton, "Align to V").center());
    assert!(matches!(h.app.state.scene.node(b).unwrap().constraints.position, Some(loom_scene::constraint::PositionConstraint::Surface { align: 2, .. })));
}

/// Hide by Hit: a plain click hides the clicked object; with Ctrl its
/// children are hidden with it.
#[test]
fn hide_by_hit_with_ctrl_hides_the_children_too() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [20.0; 3] });
    let parent = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [60.0, 0.0, 0.0], size: [20.0; 3] });
    let child = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::SetParent { id: child.0, parent: Some(parent.0) });
    h.cmd(Command::SelectNone);
    framed(&mut h);
    h.cmd(Command::HideByHit);
    let p = h.screen_of(glam::Vec3::new(5.0, -10.0, 15.0));
    h.mods = egui::Modifiers::CTRL;
    h.click(p);
    h.mods = egui::Modifiers::NONE;
    let hidden = |h: &Harness, id: loom_scene::NodeId| h.app.state.scene.node(id).unwrap().hidden;
    assert!(hidden(&h, parent) && hidden(&h, child), "Ctrl: the box and its child");
}
