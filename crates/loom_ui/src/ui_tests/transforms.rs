//! Transform frames, the Hierarchy panel's pivot tools and Ctrl+click
//! selection, clicked as a person does.

use super::*;

/// Ctrl+click toggles what it hits (the reference before its 2011 change):
/// an unselected object joins the selection, a selected one leaves it.
#[test]
fn ctrl_click_toggles_an_object_in_the_selection() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [-30.0, 0.0, 0.0], size: [20.0; 3] });
    let a = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [30.0, 0.0, 0.0], size: [20.0; 3] });
    let b = *h.app.state.selection.iter().next().unwrap();
    framed(&mut h);
    let pa = h.screen_of(Vec3::new(-30.0, 0.0, 20.0));
    let pb = h.screen_of(Vec3::new(30.0, 0.0, 20.0));
    h.click(pa);
    assert_eq!(h.app.state.selection.iter().copied().collect::<Vec<_>>(), vec![a]);
    h.mods = egui::Modifiers::CTRL;
    h.click(pb);
    assert!(h.app.state.selection.contains(&a) && h.app.state.selection.contains(&b), "Ctrl+click adds an unselected object");
    h.click(pa);
    h.mods = egui::Modifiers::NONE;
    assert_eq!(h.app.state.selection.iter().copied().collect::<Vec<_>>(), vec![b], "Ctrl+click on a selected object removes it");
}

#[test]
fn ctrl_click_toggles_a_polygon() {
    let mut h = Harness::new();
    poly_box(&mut h);
    h.key(egui::Key::Num4);
    let p = h.screen_of(top_face_point());
    h.click(p);
    assert_eq!(h.sub_count(), 1);
    h.mods = egui::Modifiers::CTRL;
    h.click(p);
    h.mods = egui::Modifiers::NONE;
    assert_eq!(h.sub_count(), 0, "Ctrl+click on a selected polygon deselects it");
}

fn hierarchy_panel(h: &mut Harness) {
    // Tall enough for every Pivot rollout.
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 1400.0));
    h.app.panel_tab = crate::Tab::Hierarchy;
    h.frames(3);
}

/// Hierarchy > Pivot: the three Affect buttons are exclusive and the
/// Alignment buttons follow them (Center to Object / Align to Object for the
/// pivot, Center to Pivot / Align to Pivot for the object).
#[test]
fn hierarchy_pivot_buttons_follow_the_affect_mode() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [20.0; 3] });
    let id = *h.app.state.selection.iter().next().unwrap();
    framed(&mut h);
    hierarchy_panel(&mut h);
    h.click(panel_text(&h, "Affect Pivot Only"));
    assert!(h.app.state.affect_pivot_only);
    assert!(h.texts("Align to Object").len() == 1 && h.texts("Center to Pivot").is_empty());
    h.click(panel_text(&h, "Affect Object Only"));
    assert!(h.app.state.affect_object_only && !h.app.state.affect_pivot_only);
    assert!(h.texts("Center to Object").is_empty(), "Affect Object Only relabels the alignment buttons");
    // Move the mesh off the pivot, then Center to Pivot brings it back.
    h.cmd(Command::MoveSelection { delta: [5.0, 0.0, 0.0], system: Some(loom_commands::CoordSystem::World) });
    h.click(panel_text(&h, "Center to Pivot"));
    let (lo, hi) = h.app.state.object_bounds(id).unwrap();
    let pivot = h.app.state.scene.world_matrix(id).w_axis.truncate();
    assert!(((lo + hi) * 0.5).abs_diff_eq(pivot, 1e-3), "{} vs {pivot}", (lo + hi) * 0.5);
    h.click(panel_text(&h, "Affect Hierarchy Only"));
    assert!(h.app.state.affect_hierarchy_only && !h.app.state.affect_object_only);
    h.click(panel_text(&h, "Don't Affect Children"));
    assert!(h.app.state.dont_affect_children);
    // Adjust Transform > Reset: Scale.
    h.cmd(Command::SetTransform { id: id.0, position: None, rotation_deg: None, scale: Some([2.0, 1.0, 1.0]) });
    h.click(panel_text(&h, "Scale"));
    assert_eq!(h.app.state.scene.node(id).unwrap().transform.scale.get(), Vec3::ONE);
}

/// Reference Coordinate System > Pick, then a click on an object: its name
/// shows in the list and stays there for later.
#[test]
fn pick_in_the_coordinate_list_takes_the_clicked_object() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [20.0; 3] });
    let id = *h.app.state.selection.iter().next().unwrap();
    let name = h.app.state.scene.node(id).unwrap().name.clone();
    framed(&mut h);
    h.key(egui::Key::W);
    let bar = panel(&h, "main_toolbar");
    click_text(&mut h, "View", bar, 0.0);
    let m = menus(&h)[0];
    click_text(&mut h, "Pick", m, 0.0);
    assert_eq!(h.app.state.pick_mode, Some(loom_commands::PickMode::CoordSystem));
    let p = h.screen_of(Vec3::new(5.0, -5.0, 20.0));
    h.click(p);
    assert_eq!((h.app.state.coord_system, h.app.state.pick_coord_node), (loom_commands::CoordSystem::Pick, Some(id)));
    assert!(h.text_rect(&name, bar).is_some(), "the list shows the picked object's name");
    // Back to World; the name is still in the list and picks it again.
    h.cmd(Command::ReferenceCoordinateSystem { system: loom_commands::CoordSystem::World });
    click_text(&mut h, "World", bar, 0.0);
    let m = menus(&h)[0];
    click_text(&mut h, &name, m, 0.0);
    assert_eq!(h.app.state.coord_system, loom_commands::CoordSystem::Pick);
}

/// Hierarchy > Working Pivot: Use Working Pivot puts "USE WP" under the
/// viewport name and the list on Working; Edit Working Pivot shows "EDIT WP".
#[test]
fn working_pivot_buttons_and_viewport_legend() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [20.0; 3] });
    framed(&mut h);
    hierarchy_panel(&mut h);
    let vp = h.app.viewport_rects[h.app.state.active_viewport];
    assert!(h.text_rect("USE WP", vp).is_none());
    h.click(panel_text(&h, "Use Working Pivot"));
    assert!(h.app.state.use_working_pivot);
    assert_eq!(h.app.state.coord_system, loom_commands::CoordSystem::Working);
    assert!(h.text_rect("USE WP", vp).is_some(), "the viewport legend reads USE WP");
    h.click(panel_text(&h, "Edit Working Pivot"));
    assert!(h.app.state.edit_working_pivot);
    assert!(h.text_rect("EDIT WP", vp).is_some());
    // Place Pivot To: Surface, then a click on the box's top puts the working pivot there.
    h.click(panel_text(&h, "Edit Working Pivot"));
    h.click(panel_text(&h, "Surface"));
    assert_eq!(h.app.state.pick_mode, Some(loom_commands::PickMode::WorkingPivotSurface));
    let p = h.screen_of(Vec3::new(5.0, -5.0, 20.0));
    h.click(p);
    let wp = h.app.state.working_pivot;
    assert!((wp.origin.z - 20.0).abs() < 1e-2, "on the top face: {}", wp.origin);
    assert!((wp.rotation * Vec3::Z).abs_diff_eq(Vec3::Z, 1e-3), "Z along the face normal");
}
