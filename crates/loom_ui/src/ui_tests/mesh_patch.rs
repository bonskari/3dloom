//! Editable Mesh and Editable Patch the way a person uses them: the quad
//! menu's Convert To, the level keys, clicks on faces and patches, the
//! rollout buttons and dragging out a Quad Patch.

use super::*;

fn selected(h: &Harness) -> loom_scene::NodeId {
    *h.app.state.selection.iter().next().unwrap()
}

/// Clicks the lowest text `label` in the Command Panel (a rollout's, below the stack's level rows).
fn click_last(h: &mut Harness, label: &str) {
    let r = *h.widgets_labelled(label).last().unwrap_or_else(|| panic!("no {label:?} in the Command Panel"));
    h.click(r.center());
}

/// Right-clicks the object at `at`, opens Convert To and clicks `item`.
fn convert_from_quad_menu(h: &mut Harness, at: egui::Pos2, item: &str) {
    right_click(h, at);
    assert!(h.app.quad_menu.is_some(), "right-click opens the quad menu");
    let label = format!("Convert To  {}", crate::icons::SUBMENU);
    let r = h.text_rects(&label);
    assert_eq!(r.len(), 1, "one Convert To entry");
    hover(h, r[0].center());
    if h.text_rects(item).is_empty() {
        // The submenu opens on a click when hovering did not open it.
        h.click(r[0].center());
    }
    let it = h.text_rects(item);
    assert_eq!(it.len(), 1, "{item} in the Convert To submenu");
    hover(h, it[0].center());
    h.click(it[0].center());
}

#[test]
fn quad_menu_converts_to_editable_mesh_and_polygon_picks_the_whole_side() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [30.0; 3] });
    framed(&mut h);
    let id = selected(&h);
    let at = h.screen_of(Vec3::new(0.0, 0.0, 15.0));
    convert_from_quad_menu(&mut h, at, "Convert to Editable Mesh");
    assert!(matches!(h.app.state.scene.node(id).unwrap().object, loom_scene::ObjectKind::EditableMesh(_)), "the box is an Editable Mesh");
    // 4 is Polygon: the top's two triangles; 3 is Face: one.
    h.key(egui::Key::Num4);
    assert_eq!(h.app.state.level_label(h.app.state.selection_mode), "Polygon");
    let p = h.screen_of(top_face_point());
    h.click(p);
    assert_eq!(h.sub_count(), 2, "a click on the top selects its polygon");
    h.key(egui::Key::Num3);
    assert_eq!(h.app.state.level_label(h.app.state.selection_mode), "Face");
    h.click(p);
    assert_eq!(h.sub_count(), 1, "Face selects the triangle");
    h.key(egui::Key::Num3);
    assert_eq!(h.app.state.selection_mode, SelectionMode::Object, "the key again returns to the top level");
}

#[test]
fn editable_mesh_rollouts_extrude_and_hide_edges() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1280.0, 1400.0));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [30.0; 3] });
    h.cmd(Command::ConvertToEditableMesh);
    framed(&mut h);
    h.app.panel_tab = crate::Tab::Modify;
    h.frames(2);
    let id = selected(&h);
    // The Selection rollout's Polygon button, then a click on the top.
    h.click_widget("Polygon");
    assert_eq!(h.app.state.level_label(h.app.state.selection_mode), "Polygon");
    let p = h.screen_of(top_face_point());
    h.click(p);
    assert_eq!(h.sub_count(), 2);
    h.click_widget("Extrude");
    let m = h.app.state.scene.node(id).unwrap().edit_poly().unwrap().mesh.clone();
    assert_eq!(m.face_count(), 20, "Extrude added the side triangles");
    assert!((m.aabb().1.z - 40.0).abs() < 1e-3, "by the default amount of 10");
    // Edge level: Surface Properties > Visible shows every edge.
    click_last(&mut h, "Edge");
    let all: Vec<u32> = (0..m.half_edge_count() as u32).collect();
    h.cmd(Command::SelectSub { node: Some(id.0), ids: all, op: loom_commands::SelectOp::Replace });
    click_last(&mut h, "Visible");
    assert!(!h.app.state.scene.node(id).unwrap().edit_poly().unwrap().mesh.has_invisible_edges());
    click_last(&mut h, "Auto Edge");
    // The ten quads' diagonals, and the four edges where the risers continue the sides.
    assert_eq!(h.app.state.scene.node(id).unwrap().edit_poly().unwrap().mesh.invisible_edges().len(), 14, "coplanar edges hidden again");
}

#[test]
fn edit_mesh_modifier_shows_the_editable_mesh_rollouts() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1280.0, 1400.0));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [30.0; 3] });
    h.cmd(Command::AddModifier { name: "Edit Mesh".into() });
    framed(&mut h);
    h.app.panel_tab = crate::Tab::Modify;
    h.frames(2);
    assert!(h.widgets_labelled("Edit Mesh is edited as Edit Poly.").is_empty(), "no longer an Edit Poly");
    assert_eq!(h.widgets_labelled("Ignore Visible Edges").len(), 1);
    h.click_widget("Polygon");
    let id = selected(&h);
    assert_eq!(h.app.state.scene.node(id).unwrap().modifiers[0].sub_level, 4, "the entry is at its Polygon level");
    h.click(h.screen_of(top_face_point()));
    assert_eq!(h.sub_count(), 2);
}

#[test]
fn quad_patch_drags_out_and_patch_level_extrudes() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1280.0, 1400.0));
    maximized(&mut h);
    h.app.panel_tab = crate::Tab::Create;
    h.app.create_patch_grids = true;
    h.frames(2);
    h.click_widget("Quad Patch");
    assert_eq!(h.app.create_kind, Some(crate::CreateKind::PatchGrid { quad: true }));
    let r = h.app.viewport_rects[h.app.state.active_viewport];
    let a = r.center() + egui::vec2(-80.0, 40.0);
    h.drag(a, a + egui::vec2(160.0, 60.0));
    let id = selected(&h);
    let node = h.app.state.scene.node(id).unwrap();
    assert!(node.name.starts_with("QuadPatch"), "{}", node.name);
    let (lo, hi) = h.app.state.scene.eval_mesh(id).value.mesh.aabb();
    assert!(hi.x - lo.x > 0.05 && hi.y - lo.y > 0.05, "dragged out: {lo} {hi}");
    h.cmd(Command::ConvertToEditablePatch);
    h.cmd(Command::ZoomExtentsAll);
    h.app.panel_tab = crate::Tab::Modify;
    h.frames(2);
    // 2 is Handle, 4 is Patch.
    h.key(egui::Key::Num2);
    assert_eq!(h.app.state.level_label(h.app.state.selection_mode), "Handle");
    h.key(egui::Key::Num4);
    assert_eq!(h.app.state.level_label(h.app.state.selection_mode), "Patch");
    let c = h.app.state.scene.eval_mesh(id).value.mesh.aabb();
    let mid = (c.0 + c.1) * 0.5 + Vec3::new((c.1.x - c.0.x) * 0.2, (c.1.y - c.0.y) * 0.2, 0.0);
    let world = h.app.state.scene.world_matrix(id).transform_point3(mid);
    h.click(h.screen_of(world));
    assert_eq!(h.sub_count(), 1, "a click on the surface picks the patch");
    h.click_widget("Extrude");
    let pm = &h.app.state.scene.node(id).unwrap().object.patch().unwrap().patch;
    assert_eq!(pm.patches.len(), 5, "the patch and four sides");
}
