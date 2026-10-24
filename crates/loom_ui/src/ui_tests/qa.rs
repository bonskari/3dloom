//! QA sweep regressions: basic features driven the way a Max user would.

use super::*;
use loom_commands::Tool;

/// Runs one frame and returns its painted shapes (to find widgets by label).
fn run_shapes(h: &mut Harness, events: Vec<egui::Event>) -> Vec<egui::epaint::ClippedShape> {
    h.time += 1.0 / 30.0;
    let input = egui::RawInput { screen_rect: Some(SCREEN), time: Some(h.time), modifiers: h.mods, events, ..Default::default() };
    let app = &mut h.app;
    h.ctx.run(input, |ctx| app.frame(ctx)).shapes
}

fn texts(shape: &egui::Shape, out: &mut Vec<(String, egui::Rect)>) {
    match shape {
        egui::Shape::Vec(v) => v.iter().for_each(|s| texts(s, out)),
        egui::Shape::Text(t) => out.push((t.galley.text().to_string(), t.galley.rect.translate(t.pos.to_vec2()))),
        _ => {}
    }
}

/// The screen rectangle of the last painted text exactly equal to `label`.
fn find_text(h: &mut Harness, label: &str) -> Option<egui::Rect> {
    let shapes = run_shapes(h, Vec::new());
    let mut all = Vec::new();
    for s in &shapes {
        texts(&s.shape, &mut all);
    }
    all.into_iter().rev().find(|(t, _)| t == label).map(|(_, r)| r)
}

fn click_text(h: &mut Harness, label: &str) {
    let r = find_text(h, label).unwrap_or_else(|| panic!("no widget labelled {label:?}"));
    h.click(r.center());
}

fn selected(h: &Harness) -> loom_scene::NodeId {
    *h.app.state.selection.iter().next().expect("a selection")
}

fn pos_of(h: &Harness, id: loom_scene::NodeId) -> Vec3 {
    h.app.state.scene.node(id).unwrap().transform.position.get()
}

/// A 30-unit box at the origin, selected, in a maximized framed Perspective view.
fn box_scene(h: &mut Harness) -> loom_scene::NodeId {
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [30.0; 3] });
    framed(h);
    selected(h)
}

/// A point on the box's side face, away from the gizmo at the pivot.
fn body_point(h: &Harness) -> egui::Pos2 {
    h.screen_of(Vec3::new(10.0, -15.0, 20.0))
}

#[test]
fn moving_with_x_constraint_moves_only_along_x_and_undoes() {
    let mut h = Harness::new();
    let id = box_scene(&mut h);
    h.key(egui::Key::W);
    assert_eq!(h.app.state.active_tool, Tool::Move);
    h.key(egui::Key::F5);
    let p = body_point(&h);
    h.drag(p, p + egui::vec2(60.0, -40.0));
    let moved = pos_of(&h, id);
    assert!(moved.x.abs() > 1.0, "moved along X: {moved}");
    assert!(moved.y.abs() < 1e-3 && moved.z.abs() < 1e-3, "only along X: {moved}");
    h.mods = egui::Modifiers::CTRL;
    h.key(egui::Key::Z);
    h.mods = egui::Modifiers::NONE;
    assert!(pos_of(&h, id).length() < 1e-4, "Ctrl+Z undoes the whole drag: {}", pos_of(&h, id));
    h.mods = egui::Modifiers::CTRL;
    h.key(egui::Key::Y);
    h.mods = egui::Modifiers::NONE;
    assert!(pos_of(&h, id).distance(moved) < 1e-4, "Ctrl+Y redoes it");
}

#[test]
fn rotate_and_scale_drags_change_the_object_and_undo_in_one_step() {
    let mut h = Harness::new();
    let id = box_scene(&mut h);
    h.key(egui::Key::E);
    h.key(egui::Key::F7);
    let p = body_point(&h);
    h.drag(p, p + egui::vec2(0.0, -50.0));
    let q = h.app.state.scene.node(id).unwrap().transform.rotation.get();
    assert!(q.length() > 0.01, "rotated");
    h.cmd(Command::UndoSceneOperation);
    assert!(h.app.state.scene.node(id).unwrap().transform.rotation.get().length() < 1e-4);
    h.key(egui::Key::R);
    assert_eq!(h.app.state.active_tool, Tool::Scale);
    let p = body_point(&h);
    h.drag(p, p + egui::vec2(0.0, -50.0));
    let s = h.app.state.scene.node(id).unwrap().transform.scale.get();
    assert!((s - Vec3::ONE).length() > 0.01, "scaled: {s}");
    h.cmd(Command::UndoSceneOperation);
    assert!((h.app.state.scene.node(id).unwrap().transform.scale.get() - Vec3::ONE).length() < 1e-4);
}

#[test]
fn shift_drag_clones_through_the_clone_options_dialog() {
    let mut h = Harness::new();
    let id = box_scene(&mut h);
    h.key(egui::Key::W);
    let p = body_point(&h);
    h.mods = egui::Modifiers::SHIFT;
    h.drag(p, p + egui::vec2(80.0, 0.0));
    h.mods = egui::Modifiers::NONE;
    h.frames(2);
    click_text(&mut h, "OK");
    h.frames(2);
    assert_eq!(h.app.state.scene.len(), 2, "a copy was made");
    assert!(pos_of(&h, id).length() < 1e-4, "the original stays put: {}", pos_of(&h, id));
    let copy = selected(&h);
    assert_ne!(copy, id, "the copy is selected");
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(h.app.state.scene.len(), 1, "one undo removes the clone");
}

#[test]
fn move_tool_drag_on_an_unselected_object_selects_and_moves_it() {
    let mut h = Harness::new();
    let id = box_scene(&mut h);
    h.cmd(Command::SelectNone);
    h.key(egui::Key::W);
    let p = body_point(&h);
    h.drag(p, p + egui::vec2(60.0, 0.0));
    assert!(h.app.state.selection.contains(&id), "the press selects it");
    assert!(pos_of(&h, id).length() > 1.0, "and the drag moves it (XY plane): {}", pos_of(&h, id));
    assert!(pos_of(&h, id).z.abs() < 1e-3, "the default XY constraint keeps it on the plane");
    // A press on empty space still starts a region selection, not a move.
    h.cmd(Command::SelectNone);
    let r = h.app.viewport_rects[h.app.state.active_viewport];
    h.drag(r.min + egui::vec2(5.0, 5.0), r.max - egui::vec2(5.0, 5.0));
    assert!(h.app.state.selection.contains(&id));
}

/// A drag whose last pointer move arrives in the same frame as the release
/// (a quick flick, or a slow frame) must still end where the pointer let go.
#[test]
fn a_move_drag_released_in_the_same_frame_as_its_last_motion_keeps_that_motion() {
    let end = |same_frame: bool| {
        let mut h = Harness::new();
        let id = box_scene(&mut h);
        h.key(egui::Key::W);
        let a = body_point(&h);
        let b = a + egui::vec2(80.0, 0.0);
        h.run(vec![egui::Event::PointerMoved(a)]);
        h.run(vec![egui::Event::PointerButton { pos: a, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE }]);
        h.run(vec![egui::Event::PointerMoved(a + egui::vec2(10.0, 0.0))]);
        let release = egui::Event::PointerButton { pos: b, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE };
        if same_frame {
            h.run(vec![egui::Event::PointerMoved(b), release]);
        } else {
            h.run(vec![egui::Event::PointerMoved(b)]);
            h.run(vec![release]);
        }
        h.frames(1);
        pos_of(&h, id)
    };
    let (split, joined) = (end(false), end(true));
    assert!(split.length() > 1.0, "the drag moved the box: {split}");
    assert!(joined.distance(split) < 1e-3, "release frame keeps the last motion: {joined} vs {split}");
}

#[test]
fn select_by_name_opens_with_the_selection_and_select_all_skips_unlisted_objects() {
    let mut h = Harness::new();
    let a = box_scene(&mut h);
    h.key(egui::Key::H);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::SelectFromScene));
    click_text(&mut h, "Select");
    assert!(h.app.state.dialog.is_none(), "Select closes the dialog");
    assert!(h.app.state.selection.contains(&a), "the selection the dialog opened with stays selected");
    // A hidden object is not listed (Display: Hidden off), so Select All leaves it out.
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [50.0, 0.0, 0.0], size: [10.0; 3] });
    let b = selected(&h);
    h.cmd(Command::HideSelection);
    h.key(egui::Key::H);
    click_text(&mut h, "Select All");
    click_text(&mut h, "Select");
    assert!(h.app.state.selection.contains(&a));
    assert!(!h.app.state.selection.contains(&b), "a hidden, unlisted object is not selected");
}

#[test]
fn select_by_name_click_picks_one_and_double_click_selects_and_closes() {
    let mut h = Harness::new();
    let a = box_scene(&mut h);
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Sphere, position: [50.0, 0.0, 0.0], size: [10.0; 3] });
    let b = selected(&h);
    let name_a = h.app.state.scene.node(a).unwrap().name.clone();
    let name_b = h.app.state.scene.node(b).unwrap().name.clone();
    h.key(egui::Key::H);
    // A plain click on a name makes it the only chosen one, as in Max's list.
    click_text(&mut h, &name_a);
    click_text(&mut h, "Select");
    assert_eq!(h.app.state.selection.iter().copied().collect::<Vec<_>>(), vec![a]);
    h.key(egui::Key::H);
    // Earlier clicks more than egui's triple-click window ago.
    h.frames(30);
    let r = find_text(&mut h, &name_b).expect("listed");
    let m = h.mods;
    h.run(vec![egui::Event::PointerMoved(r.center())]);
    for _ in 0..2 {
        h.run(vec![egui::Event::PointerButton { pos: r.center(), button: egui::PointerButton::Primary, pressed: true, modifiers: m }]);
        h.run(vec![egui::Event::PointerButton { pos: r.center(), button: egui::PointerButton::Primary, pressed: false, modifiers: m }]);
    }
    h.frames(2);
    assert!(h.app.state.dialog.is_none(), "a double-click closes the dialog");
    assert_eq!(h.app.state.selection.iter().copied().collect::<Vec<_>>(), vec![b], "and selects the name");
}

#[test]
fn delete_key_removes_and_undo_restores() {
    let mut h = Harness::new();
    box_scene(&mut h);
    h.key(egui::Key::Delete);
    assert_eq!(h.app.state.scene.len(), 0);
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(h.app.state.scene.len(), 1);
}

#[test]
fn frozen_objects_cannot_be_clicked_and_hidden_ones_vanish() {
    let mut h = Harness::new();
    let id = box_scene(&mut h);
    h.cmd(Command::FreezeSelection);
    h.cmd(Command::SelectNone);
    let p = body_point(&h);
    h.click(p);
    assert!(h.app.state.selection.is_empty(), "a frozen object is not selectable");
    h.cmd(Command::UnfreezeAll);
    h.click(p);
    assert!(h.app.state.selection.contains(&id));
    h.cmd(Command::HideSelection);
    assert!(h.app.state.selection.is_empty(), "hiding deselects");
    h.click(p);
    assert!(h.app.state.selection.is_empty(), "a hidden object is not selectable");
    h.cmd(Command::UnhideAll);
    h.click(p);
    assert!(h.app.state.selection.contains(&id));
}

#[test]
fn region_drag_crossing_and_window() {
    let mut h = Harness::new();
    let id = box_scene(&mut h);
    h.cmd(Command::SelectNone);
    h.key(egui::Key::Q);
    let c = h.screen_of(Vec3::new(0.0, 0.0, 15.0));
    // Crossing (default): a small rectangle touching the box selects it.
    h.drag(c + egui::vec2(-5.0, -5.0), c + egui::vec2(5.0, 5.0));
    assert!(h.app.state.selection.contains(&id), "crossing selects a touched object");
    h.cmd(Command::SelectNone);
    h.app.state.window_selection = true;
    h.drag(c + egui::vec2(-5.0, -5.0), c + egui::vec2(5.0, 5.0));
    assert!(h.app.state.selection.is_empty(), "window needs the whole object inside");
    let r = h.app.viewport_rects[h.app.state.active_viewport];
    h.drag(r.min + egui::vec2(5.0, 5.0), r.max - egui::vec2(5.0, 5.0));
    assert!(h.app.state.selection.contains(&id), "window around the whole box selects it");
}

fn middle_drag(h: &mut Harness, a: egui::Pos2, b: egui::Pos2) {
    let m = h.mods;
    h.run(vec![egui::Event::PointerMoved(a)]);
    h.run(vec![egui::Event::PointerButton { pos: a, button: egui::PointerButton::Middle, pressed: true, modifiers: m }]);
    for i in 1..=8 {
        h.run(vec![egui::Event::PointerMoved(a + (b - a) * (i as f32 / 8.0))]);
    }
    h.run(vec![egui::Event::PointerButton { pos: b, button: egui::PointerButton::Middle, pressed: false, modifiers: m }]);
    h.frames(1);
}

#[test]
fn middle_drag_pans_alt_middle_orbits_and_wheel_zooms() {
    let mut h = Harness::new();
    box_scene(&mut h);
    let r = h.app.viewport_rects[h.app.state.active_viewport];
    let c = r.center();
    let cam = h.app.state.active().camera.clone();
    middle_drag(&mut h, c, c + egui::vec2(50.0, 0.0));
    let after = h.app.state.active().camera.clone();
    assert!(after.target.distance(cam.target) > 0.1, "MMB pans");
    assert_eq!(after.yaw, cam.yaw);
    h.mods = egui::Modifiers::ALT;
    middle_drag(&mut h, c, c + egui::vec2(50.0, 0.0));
    h.mods = egui::Modifiers::NONE;
    assert_ne!(h.app.state.active().camera.yaw, after.yaw, "Alt+MMB orbits");
    let d = h.app.state.active().camera.distance;
    h.run(vec![egui::Event::PointerMoved(c)]);
    h.run(vec![egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Line, delta: egui::vec2(0.0, 1.0), modifiers: egui::Modifiers::NONE }]);
    h.frames(1);
    assert!(h.app.state.active().camera.distance < d, "wheel up zooms in");
}

#[test]
fn view_keys_switch_the_active_viewport() {
    use loom_commands::ViewKind;
    let mut h = Harness::new();
    box_scene(&mut h);
    let i = h.app.state.active_viewport;
    for (k, v) in [(egui::Key::T, ViewKind::Top), (egui::Key::F, ViewKind::Front), (egui::Key::L, ViewKind::Left), (egui::Key::P, ViewKind::Perspective), (egui::Key::U, ViewKind::Orthographic)] {
        h.key(k);
        assert_eq!(h.app.state.viewports[i].kind, v, "{k:?}");
    }
    h.mods = egui::Modifiers::ALT;
    h.key(egui::Key::W);
    h.mods = egui::Modifiers::NONE;
    assert!(!h.app.state.maximized, "Alt+W restores the four views");
    let shading = h.app.state.viewports[i].shading;
    h.key(egui::Key::F3);
    assert_ne!(h.app.state.viewports[i].shading, shading, "F3 toggles wireframe");
    let edged = h.app.state.viewports[i].edged_faces;
    h.key(egui::Key::F4);
    assert_ne!(h.app.state.viewports[i].edged_faces, edged, "F4 toggles Edged Faces");
}

#[test]
fn hold_and_fetch_restore_the_scene() {
    let mut h = Harness::new();
    box_scene(&mut h);
    h.cmd(Command::Hold);
    h.key(egui::Key::Delete);
    assert_eq!(h.app.state.scene.len(), 0);
    h.cmd(Command::Fetch);
    h.frames(2);
    assert_eq!(h.app.state.scene.len(), 1, "Fetch brings the held scene back");
}

#[test]
fn modifier_add_delete_and_collapse() {
    let mut h = Harness::new();
    let id = box_scene(&mut h);
    h.cmd(Command::AddModifier { name: "Bend".into() });
    h.cmd(Command::AddModifier { name: "Taper".into() });
    assert_eq!(h.app.state.scene.node(id).unwrap().modifiers.len(), 2);
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(h.app.state.scene.node(id).unwrap().modifiers.len(), 1);
    h.cmd(Command::CollapseAll);
    assert_eq!(h.app.state.scene.node(id).unwrap().modifiers.len(), 0);
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(h.app.state.scene.node(id).unwrap().modifiers.len(), 1);
}

#[test]
fn every_primitive_drag_creates_in_every_viewport() {
    let kinds = [PrimitiveKind::Box, PrimitiveKind::Sphere, PrimitiveKind::Cylinder, PrimitiveKind::Cone, PrimitiveKind::Torus, PrimitiveKind::Teapot, PrimitiveKind::Plane, PrimitiveKind::Pyramid, PrimitiveKind::Tube, PrimitiveKind::GeoSphere];
    for vp in 0..4 {
        for kind in kinds {
            let mut h = Harness::new();
            h.cmd(Command::ActiveViewport { index: vp });
            h.cmd(Command::MaximizeViewportToggle);
            h.app.create_kind = Some(crate::CreateKind::Primitive(kind));
            let r = h.app.viewport_rects[vp as usize];
            let a = r.center() + egui::vec2(-20.0, 30.0);
            let b = a + egui::vec2(60.0, 40.0);
            h.drag(a, b);
            // Steps after the first drag: move and click until the tool finishes.
            let mut q = b;
            for _ in 0..3 {
                if h.app.create.is_none() {
                    break;
                }
                let n = q + egui::vec2(0.0, -30.0);
                hover_to(&mut h, q, n);
                h.click(n);
                q = n;
            }
            assert!(h.app.create.is_none(), "vp {vp} {kind:?}: creation finished");
            assert_eq!(h.app.state.scene.len(), 1, "vp {vp} {kind:?}: one object");
            let id = selected(&h);
            let (lo, hi) = h.app.state.scene.node(id).unwrap().object.mesh().aabb();
            assert!((hi - lo).max_element() > 0.5, "vp {vp} {kind:?}: has a size {lo} {hi}");
        }
    }
}

fn face_count(h: &Harness, id: loom_scene::NodeId) -> usize {
    h.app.state.scene.node(id).unwrap().cage_mesh().face_count()
}

fn edge_count(h: &Harness, id: loom_scene::NodeId) -> usize {
    h.app.state.scene.node(id).unwrap().cage_mesh().edge_count()
}

/// Editable Poly box, Modify tab, framed.
fn poly_scene(h: &mut Harness) -> loom_scene::NodeId {
    let id = box_scene(h);
    h.cmd(Command::ConvertToEditablePoly);
    h.app.panel_tab = crate::Tab::Modify;
    h.frames(2);
    id
}

#[test]
fn extrude_button_then_drag_on_the_face_extrudes_it() {
    let mut h = Harness::new();
    let id = poly_scene(&mut h);
    h.key(egui::Key::Num4);
    let p = h.screen_of(top_face_point());
    h.click(p);
    assert_eq!(h.sub_count(), 1);
    let before = face_count(&h, id);
    click_text(&mut h, "Extrude");
    h.drag(p, p + egui::vec2(0.0, -40.0));
    assert_eq!(face_count(&h, id), before + 4, "the extrusion adds four side faces");
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(face_count(&h, id), before, "one undo step");
}

/// In Extrude mode the drag belongs to the mode, even when it starts on the
/// Move gizmo (which sits on the selected face).
#[test]
fn extrude_drag_starting_on_the_move_gizmo_makes_one_extrusion() {
    let mut h = Harness::new();
    let id = poly_scene(&mut h);
    h.key(egui::Key::W);
    h.key(egui::Key::Num4);
    let p = h.screen_of(top_face_point());
    h.click(p);
    let before = face_count(&h, id);
    let centre = h.app.selection_pivot();
    click_text(&mut h, "Extrude");
    let g = h.screen_of(centre);
    h.drag(g, g + egui::vec2(0.0, -40.0));
    assert_eq!(face_count(&h, id), before + 4, "one extrusion, not one per frame");
    assert!(h.app.transform_drag.is_none());
    let top: Vec<Vec3> = {
        let n = h.app.state.scene.node(id).unwrap();
        let m = n.cage_mesh();
        m.vertices().iter().enumerate().filter_map(|(i, _)| m.vertex_position(loom_mesh::VertexId(i as u32))).filter(|v| v.z > 30.5).collect()
    };
    assert_eq!(top.len(), 4, "the extruded cap is four vertices above the box");
    assert!(top.iter().all(|v| v.x.abs() <= 15.0 + 1e-3 && v.y.abs() <= 15.0 + 1e-3), "and the gizmo did not move them sideways: {top:?}");
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(face_count(&h, id), before, "one undo step");
}

#[test]
fn ring_loop_and_connect_buttons_on_edges() {
    let mut h = Harness::new();
    let id = poly_scene(&mut h);
    h.key(egui::Key::Num2);
    let p = h.screen_of(Vec3::new(15.0, 0.0, 30.0));
    h.click(p);
    assert_eq!(h.sub_count(), 1);
    click_text(&mut h, "Ring");
    assert_eq!(h.sub_count(), 4, "Ring");
    let before = edge_count(&h, id);
    // The Connect button sits below the fold of the panel here; use its shortcut.
    h.mods = egui::Modifiers::CTRL | egui::Modifiers::SHIFT;
    h.key(egui::Key::E);
    h.mods = egui::Modifiers::NONE;
    assert_eq!(edge_count(&h, id), before + 8, "Connect splits the four ring edges and joins them with four new edges");
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(edge_count(&h, id), before);
}

#[test]
fn chamfer_mode_drag_on_an_edge_chamfers_it() {
    let mut h = Harness::new();
    let id = poly_scene(&mut h);
    h.key(egui::Key::Num2);
    let p = h.screen_of(Vec3::new(15.0, 0.0, 30.0));
    h.click(p);
    let before = face_count(&h, id);
    // The Edit Edges rollout is below the fold at 1280×800: Chamfer Mode by its shortcut.
    h.mods = egui::Modifiers::CTRL | egui::Modifiers::SHIFT;
    h.key(egui::Key::C);
    h.mods = egui::Modifiers::NONE;
    h.drag(p, p + egui::vec2(0.0, -30.0));
    assert_eq!(face_count(&h, id), before + 1, "a one-segment chamfer adds a face");
}

#[test]
fn grid_snap_moves_onto_grid_points() {
    let mut h = Harness::new();
    let id = box_scene(&mut h);
    h.cmd(Command::SetTransform { id: id.0, position: Some([3.0, 4.0, 0.0]), rotation_deg: None, scale: None });
    h.key(egui::Key::W);
    h.key(egui::Key::S);
    assert!(h.app.state.snaps_on, "S toggles snaps");
    // Release a few pixels off the grid point (40, 0, 0), inside the snap strength.
    let p = h.screen_of(Vec3::new(13.0, -11.0, 20.0));
    h.drag(p, h.screen_of(Vec3::new(40.0, 0.0, 0.0)) + egui::vec2(3.0, -2.0));
    let q = pos_of(&h, id);
    let g = h.app.state.home_grid.spacing;
    for c in [q.x, q.y] {
        assert!((c / g - (c / g).round()).abs() < 1e-3, "on the {g}-unit grid: {q}");
    }
}

#[test]
fn angle_snap_rotates_in_whole_steps() {
    let mut h = Harness::new();
    let id = box_scene(&mut h);
    h.key(egui::Key::E);
    h.key(egui::Key::A);
    assert!(h.app.state.angle_snap_on, "A toggles angle snap");
    h.key(egui::Key::F7);
    let p = body_point(&h);
    h.drag(p, p + egui::vec2(-37.0, -23.0));
    let r = h.app.state.scene.node(id).unwrap().transform.rotation.get();
    let deg = r.z;
    assert!(deg.abs() > 1.0, "rotated: {deg}");
    assert!((deg / 5.0 - (deg / 5.0).round()).abs() < 1e-3, "in 5 degree steps: {deg}");
}

#[test]
fn local_coordinate_system_moves_along_the_rotated_axis() {
    let mut h = Harness::new();
    let id = box_scene(&mut h);
    h.cmd(Command::SetTransform { id: id.0, position: None, rotation_deg: Some([0.0, 0.0, 90.0]), scale: None });
    // Each transform tool keeps its own reference system: pick the tool first.
    h.key(egui::Key::W);
    h.cmd(Command::ReferenceCoordinateSystem { system: loom_commands::CoordSystem::Local });
    h.key(egui::Key::F5);
    let p = body_point(&h);
    h.drag(p, p + egui::vec2(60.0, -40.0));
    let q = pos_of(&h, id);
    assert!(q.y.abs() > 1.0 && q.x.abs() < 1e-3, "local X of a box turned 90 degrees is world Y: {q}");
}

#[test]
fn selection_center_rotates_two_objects_about_their_middle() {
    let mut h = Harness::new();
    let a = box_scene(&mut h);
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [60.0, 0.0, 0.0], size: [30.0; 3] });
    let b = selected(&h);
    h.cmd(Command::SelectNodes { ids: vec![a.0, b.0], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::UseCenter { mode: loom_commands::CenterMode::Selection });
    h.cmd(Command::RotateSelection { axis: loom_commands::Axis::Z, degrees: 180.0, system: Some(loom_commands::CoordSystem::World) });
    assert!(pos_of(&h, a).distance(Vec3::new(60.0, 0.0, 0.0)) < 1e-2, "{}", pos_of(&h, a));
    assert!(pos_of(&h, b).distance(Vec3::ZERO) < 1e-2, "{}", pos_of(&h, b));
}

#[test]
fn modifier_reorder_and_delete_with_undo() {
    let mut h = Harness::new();
    let id = box_scene(&mut h);
    h.cmd(Command::AddModifier { name: "Bend".into() });
    h.cmd(Command::AddModifier { name: "Taper".into() });
    let names = |h: &Harness| h.app.state.scene.node(id).unwrap().modifiers.iter().map(|m| m.name.clone()).collect::<Vec<_>>();
    let start = names(&h);
    h.cmd(Command::MoveModifier { from: 1, to: 0 });
    assert_eq!(names(&h), vec![start[1].clone(), start[0].clone()]);
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(names(&h), start);
    h.cmd(Command::DeleteModifier { index: 0 });
    assert_eq!(names(&h).len(), 1);
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(names(&h), start);
}

#[test]
fn save_new_open_export_import_merge_round_trip() {
    let dir = std::env::temp_dir().join(format!("loom-qa-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let scene = dir.join("a.loom").to_string_lossy().into_owned();
    let obj = dir.join("a.obj").to_string_lossy().into_owned();
    let mut h = Harness::new();
    box_scene(&mut h);
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Sphere, position: [60.0, 0.0, 0.0], size: [10.0; 3] });
    h.cmd(Command::SaveFileAs { path: Some(scene.clone()) });
    h.cmd(Command::ExportFile { path: Some(obj.clone()) });
    h.cmd(Command::NewScene);
    h.frames(2);
    assert_eq!(h.app.state.scene.len(), 0, "New empties the scene");
    h.cmd(Command::OpenFile { path: Some(scene.clone()) });
    assert_eq!(h.app.state.scene.len(), 2, "Open loads both objects");
    h.cmd(Command::ImportFile { path: Some(obj.clone()) });
    h.frames(2);
    assert_eq!(h.app.state.scene.len(), 4, "Import adds the two OBJ objects");
    h.cmd(Command::MergeFile { path: Some(scene.clone()), names: None });
    h.frames(2);
    assert_eq!(h.app.state.scene.len(), 6, "Merge adds the file's two objects");
    let names: std::collections::BTreeSet<String> = h.app.state.scene.nodes().map(|n| n.name.clone()).collect();
    assert_eq!(names.len(), 6, "merged names do not collide");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn time_slider_keys_and_set_frame() {
    let mut h = Harness::new();
    h.cmd(Command::SetFrame { frame: 50 });
    assert_eq!(h.app.state.current_frame, 50);
    h.key(egui::Key::Period);
    assert_eq!(h.app.state.current_frame, 51, ". steps forward");
    h.key(egui::Key::Comma);
    assert_eq!(h.app.state.current_frame, 50, ", steps back");
    h.key(egui::Key::End);
    assert_eq!(h.app.state.current_frame, h.app.state.frame_range.1);
    h.key(egui::Key::Home);
    assert_eq!(h.app.state.current_frame, h.app.state.frame_range.0);
}

#[test]
fn selection_changes_undo_and_ctrl_a_d_i() {
    let mut h = Harness::new();
    let a = box_scene(&mut h);
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [60.0, 0.0, 0.0], size: [10.0; 3] });
    let b = selected(&h);
    h.mods = egui::Modifiers::CTRL;
    h.key(egui::Key::A);
    assert_eq!(h.app.state.selection.len(), 2, "Ctrl+A");
    h.key(egui::Key::D);
    assert!(h.app.state.selection.is_empty(), "Ctrl+D");
    h.cmd(Command::SelectNodes { ids: vec![a.0], op: loom_commands::SelectOp::Replace });
    h.key(egui::Key::I);
    h.mods = egui::Modifiers::NONE;
    assert_eq!(h.app.state.selection.iter().copied().collect::<Vec<_>>(), vec![b], "Ctrl+I");
}

#[test]
fn ctrl_v_clone_dialog_ok_copies_in_place() {
    let mut h = Harness::new();
    let a = box_scene(&mut h);
    h.mods = egui::Modifiers::CTRL;
    h.key(egui::Key::V);
    h.mods = egui::Modifiers::NONE;
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::CloneOptions));
    click_text(&mut h, "OK");
    assert_eq!(h.app.state.scene.len(), 2);
    let c = selected(&h);
    assert_ne!(c, a, "the clone is selected");
    assert!(pos_of(&h, c).distance(pos_of(&h, a)) < 1e-4, "a clone in place");
    let (na, nc) = (h.app.state.scene.node(a).unwrap().name.clone(), h.app.state.scene.node(c).unwrap().name.clone());
    assert_ne!(na, nc, "with a new name");
}

#[test]
fn space_locks_the_selection_against_clicks() {
    let mut h = Harness::new();
    let a = box_scene(&mut h);
    // Space locks only with Preferences > General > Space locks the selection.
    h.cmd(Command::SetPreference { name: "Space Locks Selection".into(), value: 1.0 });
    h.key(egui::Key::Space);
    assert!(h.app.state.selection_locked);
    let r = h.app.viewport_rects[h.app.state.active_viewport];
    h.click(r.min + egui::vec2(20.0, 20.0));
    assert!(h.app.state.selection.contains(&a), "a click on empty space keeps a locked selection");
    h.key(egui::Key::Space);
    h.click(r.min + egui::vec2(20.0, 20.0));
    assert!(h.app.state.selection.is_empty());
}

#[test]
fn isolate_selection_and_exit() {
    let mut h = Harness::new();
    let a = box_scene(&mut h);
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [60.0, 0.0, 0.0], size: [10.0; 3] });
    let b = selected(&h);
    h.cmd(Command::SelectNodes { ids: vec![a.0], op: loom_commands::SelectOp::Replace });
    h.mods = egui::Modifiers::ALT;
    h.key(egui::Key::Q);
    h.mods = egui::Modifiers::NONE;
    assert!(h.app.state.scene.node(b).unwrap().hidden, "Alt+Q hides the rest");
    h.cmd(Command::ExitIsolation);
    assert!(!h.app.state.scene.node(b).unwrap().hidden);
}

#[test]
fn creating_by_drag_undoes_in_one_step() {
    let mut h = Harness::new();
    creating(&mut h, PrimitiveKind::Box);
    let a = ground_point(&h, Vec3::ZERO);
    let b = ground_point(&h, Vec3::new(20.0, 20.0, 0.0));
    h.drag(a, b);
    let c = b + egui::vec2(0.0, -40.0);
    hover_to(&mut h, b, c);
    h.click(c);
    assert_eq!(h.app.state.scene.len(), 1);
    h.mods = egui::Modifiers::CTRL;
    h.key(egui::Key::Z);
    h.mods = egui::Modifiers::NONE;
    assert_eq!(h.app.state.scene.len(), 0, "Ctrl+Z removes the new box");
    h.mods = egui::Modifiers::CTRL;
    h.key(egui::Key::Y);
    h.mods = egui::Modifiers::NONE;
    assert_eq!(h.app.state.scene.len(), 1, "Ctrl+Y brings it back");
}

#[test]
fn circular_region_selects_and_ctrl_region_adds() {
    let mut h = Harness::new();
    let a = box_scene(&mut h);
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [-40.0, 40.0, 0.0], size: [10.0; 3] });
    let b = selected(&h);
    h.cmd(Command::ZoomExtentsAll);
    h.cmd(Command::SelectNone);
    h.cmd(Command::SelectionRegion { shape: loom_commands::SelectionRegion::Circular });
    let ca = h.screen_of(Vec3::new(0.0, 0.0, 15.0));
    h.drag(ca, ca + egui::vec2(10.0, 0.0));
    assert_eq!(h.app.state.selection.iter().copied().collect::<Vec<_>>(), vec![a], "circle around the big box");
    let cb = h.screen_of(Vec3::new(-40.0, 40.0, 5.0));
    h.mods = egui::Modifiers::CTRL;
    h.drag(cb, cb + egui::vec2(6.0, 0.0));
    h.mods = egui::Modifiers::NONE;
    assert!(h.app.state.selection.contains(&b), "Ctrl+region adds");
    assert_eq!(h.app.state.selection.len(), 2);
}

fn panel_width(h: &Harness) -> f32 {
    egui::containers::panel::PanelState::load(&h.ctx, egui::Id::new("command_panel")).expect("panel state").rect.width()
}

#[test]
fn the_command_panel_keeps_its_width_across_tabs_and_poly_levels() {
    let mut h = Harness::new();
    let start = panel_width(&h);
    let id = poly_scene(&mut h);
    let _ = id;
    for level in [egui::Key::Num1, egui::Key::Num2, egui::Key::Num3, egui::Key::Num4, egui::Key::Num5] {
        h.key(level);
        h.frames(2);
        let w = panel_width(&h);
        if (w - start).abs() > 1.0 {
            h.time += 0.03;
            let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 4000.0))), time: Some(h.time), ..Default::default() };
            let app = &mut h.app;
            let shapes = h.ctx.run(input, |ctx| app.frame(ctx)).shapes;
            let mut all = Vec::new();
            for s in &shapes {
                texts(&s.shape, &mut all);
            }
            all.sort_by(|a, b| b.1.max.x.total_cmp(&a.1.max.x));
            eprintln!("level {level:?}: panel {w} vs {start}; rightmost texts on a tall screen: {:?}", &all[..14.min(all.len())]);
        }
        assert!((w - start).abs() <= 1.0, "level {level:?}: the panel grew from {start} to {w}");
    }
    h.key(egui::Key::Num6);
    for tab in crate::Tab::ALL {
        h.app.panel_tab = tab;
        h.frames(3);
        assert!((panel_width(&h) - start).abs() <= 1.0, "{tab:?}: the panel grew from {start} to {}", panel_width(&h));
    }
    h.app.panel_tab = crate::Tab::Create;
    for cat in ["Shapes", "Lights", "Cameras", "Helpers", "Space Warps", "Systems", "Geometry"] {
        click_text(&mut h, cat);
        h.frames(2);
        assert!((panel_width(&h) - start).abs() <= 1.0, "Create > {cat}: the panel grew from {start} to {}", panel_width(&h));
    }
}

/// Max: a right-click while dragging a transform cancels it and puts the
/// selection back where the drag began.
#[test]
fn right_click_during_a_move_drag_cancels_it() {
    let mut h = Harness::new();
    let id = box_scene(&mut h);
    h.key(egui::Key::W);
    let a = body_point(&h);
    h.run(vec![egui::Event::PointerMoved(a)]);
    h.run(vec![egui::Event::PointerButton { pos: a, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE }]);
    for i in 1..=6 {
        h.run(vec![egui::Event::PointerMoved(a + egui::vec2(10.0 * i as f32, 0.0))]);
    }
    assert!(pos_of(&h, id).length() > 1.0, "the drag moves the box");
    let b = a + egui::vec2(60.0, 0.0);
    h.run(vec![egui::Event::PointerButton { pos: b, button: egui::PointerButton::Secondary, pressed: true, modifiers: egui::Modifiers::NONE }]);
    h.run(vec![egui::Event::PointerButton { pos: b, button: egui::PointerButton::Secondary, pressed: false, modifiers: egui::Modifiers::NONE }]);
    h.run(vec![egui::Event::PointerButton { pos: b, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE }]);
    h.frames(2);
    assert!(pos_of(&h, id).length() < 1e-4, "right-click cancels the move: {}", pos_of(&h, id));
    assert!(h.app.quad_menu.is_none(), "and opens no quad menu");
    assert!(h.app.transform_drag.is_none());
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(h.app.state.scene.len(), 0, "the cancelled drag left no undo step: undo removes the box itself");
}

#[test]
fn instance_clone_follows_parameter_edits() {
    let mut h = Harness::new();
    let a = box_scene(&mut h);
    h.cmd(Command::CloneOptions { kind: loom_commands::CloneKind::Instance, copies: 1, name: None, offset: Some([50.0, 0.0, 0.0]), rotate_deg: None, scale: None });
    let c = selected(&h);
    assert_ne!(a, c);
    h.cmd(Command::SetParameter { id: a.0, name: "Height".into(), value: 55.0 });
    assert!((param(&h, "Height") - 55.0).abs() < 1e-4, "the instance has the new height");
}

#[test]
fn delete_at_polygon_level_deletes_the_faces() {
    let mut h = Harness::new();
    let id = poly_scene(&mut h);
    h.key(egui::Key::Num4);
    let p = h.screen_of(top_face_point());
    h.click(p);
    let before = face_count(&h, id);
    h.key(egui::Key::Delete);
    assert_eq!(face_count(&h, id), before - 1, "Delete removes the selected polygon, not the object");
    assert_eq!(h.app.state.scene.len(), 1);
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(face_count(&h, id), before);
}

#[test]
fn moving_a_parent_carries_its_child_and_group_selects_as_one() {
    let mut h = Harness::new();
    let p = box_scene(&mut h);
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [60.0, 0.0, 0.0], size: [10.0; 3] });
    let c = selected(&h);
    h.cmd(Command::SetParent { id: c.0, parent: Some(p.0) });
    h.cmd(Command::SelectNodes { ids: vec![p.0], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::MoveSelection { delta: [0.0, 10.0, 0.0], system: Some(loom_commands::CoordSystem::World) });
    let cw = h.app.state.scene.world_matrix(c).w_axis.truncate();
    assert!(cw.distance(Vec3::new(60.0, 10.0, 0.0)) < 1e-3, "the child follows: {cw}");
    h.cmd(Command::SetParent { id: c.0, parent: None });
    h.cmd(Command::SelectNodes { ids: vec![p.0, c.0], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::Group { name: None });
    assert_eq!(h.app.state.scene.len(), 3, "a group head");
    h.cmd(Command::SelectNone);
    let at = h.screen_of(Vec3::new(10.0, -5.0, 20.0));
    h.click(at);
    assert_eq!(h.app.state.selection.len(), 1, "a click on a member selects the group head");
    let head = selected(&h);
    assert!(head != p && head != c);
    h.cmd(Command::Ungroup);
    assert_eq!(h.app.state.scene.len(), 2);
}

#[test]
fn hide_and_freeze_unselected() {
    let mut h = Harness::new();
    let a = box_scene(&mut h);
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [60.0, 0.0, 0.0], size: [10.0; 3] });
    let b = selected(&h);
    h.cmd(Command::SelectNodes { ids: vec![a.0], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::FreezeUnselected);
    assert!(h.app.state.scene.node(b).unwrap().frozen && !h.app.state.scene.node(a).unwrap().frozen);
    h.cmd(Command::UnfreezeAll);
    h.cmd(Command::HideUnselected);
    assert!(h.app.state.scene.node(b).unwrap().hidden && !h.app.state.scene.node(a).unwrap().hidden);
    h.cmd(Command::UndoSceneOperation);
    assert!(!h.app.state.scene.node(b).unwrap().hidden, "Hide Unselected undoes");
}

#[test]
fn mirror_and_array_dialog_commands() {
    let mut h = Harness::new();
    let a = box_scene(&mut h);
    h.cmd(Command::SetTransform { id: a.0, position: Some([20.0, 0.0, 0.0]), rotation_deg: None, scale: None });
    h.cmd(Command::UseCenter { mode: loom_commands::CenterMode::TransformCoordinate });
    h.cmd(Command::ReferenceCoordinateSystem { system: loom_commands::CoordSystem::World });
    h.cmd(Command::MirrorObjects { axis: loom_commands::MirrorAxis::X, offset: 0.0, clone: Some(loom_commands::CloneKind::Copy) });
    assert_eq!(h.app.state.scene.len(), 2);
    let m = selected(&h);
    assert!(pos_of(&h, m).distance(Vec3::new(-20.0, 0.0, 0.0)) < 1e-3, "mirrored about the world origin: {}", pos_of(&h, m));
}

#[test]
fn navigation_buttons_zoom_pan_orbit_and_right_click_leaves_them() {
    let mut h = Harness::new();
    box_scene(&mut h);
    let r = h.app.viewport_rects[h.app.state.active_viewport];
    let c = r.center();
    let cam = h.app.state.active().camera.clone();
    click_text(&mut h, "Zoom");
    assert_eq!(h.app.state.nav_mode, loom_input::NavMode::Zoom);
    h.drag(c, c + egui::vec2(0.0, -60.0));
    assert!((h.app.state.active().camera.distance - cam.distance).abs() > 0.1, "Zoom drag changes the distance");
    assert!(h.app.state.selection.len() == 1, "and does not touch the selection");
    click_text(&mut h, "Pan");
    let t = h.app.state.active().camera.target;
    h.drag(c, c + egui::vec2(60.0, 0.0));
    assert!(h.app.state.active().camera.target.distance(t) > 0.1, "Pan drag moves the target");
    click_text(&mut h, "Orbit");
    let yaw = h.app.state.active().camera.yaw;
    h.drag(c, c + egui::vec2(60.0, 0.0));
    assert_ne!(h.app.state.active().camera.yaw, yaw, "Orbit drag orbits");
    right_click(&mut h, c);
    assert_eq!(h.app.state.nav_mode, loom_input::NavMode::None, "right-click leaves the navigation mode");
    assert!(h.app.quad_menu.is_none());
}

fn top_vertices(h: &Harness, id: loom_scene::NodeId) -> Vec<Vec3> {
    let n = h.app.state.scene.node(id).unwrap();
    let m = n.cage_mesh();
    (0..m.vertices().len() as u32).filter_map(|i| m.vertex_position(loom_mesh::VertexId(i))).filter(|v| v.z > 29.0).collect()
}

/// Sub-object level, Move tool: dragging on a polygon (not the gizmo) moves
/// it, selecting it first when it was not selected, as in Max.
#[test]
fn move_tool_drag_on_a_polygon_moves_it() {
    let mut h = Harness::new();
    let id = poly_scene(&mut h);
    h.key(egui::Key::W);
    h.key(egui::Key::Num4);
    h.key(egui::Key::F5);
    let p = h.screen_of(top_face_point());
    h.drag(p, p + egui::vec2(60.0, -30.0));
    assert_eq!(h.sub_count(), 1, "the press selected the polygon");
    let top = top_vertices(&h, id);
    assert_eq!(top.len(), 4);
    assert!(top.iter().all(|v| (v.x.abs() - 15.0).abs() > 1.0 || v.x.abs() > 16.0), "the top face moved along X: {top:?}");
    assert!(top.iter().all(|v| (v.y.abs() - 15.0).abs() < 1e-3 && (v.z - 30.0).abs() < 1e-3), "only along X: {top:?}");
    h.cmd(Command::UndoSceneOperation);
    assert!(top_vertices(&h, id).iter().all(|v| (v.x.abs() - 15.0).abs() < 1e-3), "one undo step");
}

fn texts_now(h: &mut Harness) -> Vec<String> {
    let shapes = run_shapes(h, Vec::new());
    let mut all = Vec::new();
    for s in &shapes {
        texts(&s.shape, &mut all);
    }
    all.into_iter().map(|(t, _)| t).collect()
}

/// Menu toggles show their state with a check mark, as in Max (and as the
/// viewport label menus already do).
#[test]
fn menu_toggles_show_a_check_mark() {
    let mut h = Harness::new();
    click_text(&mut h, "Customize");
    let t = texts_now(&mut h);
    assert!(t.iter().any(|s| s.starts_with("✔ Show Main Toolbar")), "the main toolbar is shown: {t:?}");
    assert!(t.iter().any(|s| s.starts_with("    Lock UI Layout")), "the layout is not locked");
    h.key(egui::Key::Escape);
    click_text(&mut h, "Views");
    let t = texts_now(&mut h);
    assert!(t.iter().any(|s| s.starts_with("✔ Show Grid")), "{t:?}");
    assert!(t.iter().any(|s| s.starts_with("    Edged Faces")), "{t:?}");
}

#[test]
fn display_toggle_keys() {
    let mut h = Harness::new();
    let id = box_scene(&mut h);
    let i = h.app.state.active_viewport;
    let grid = h.app.state.viewports[i].show_grid;
    h.key(egui::Key::G);
    assert_ne!(h.app.state.viewports[i].show_grid, grid, "G");
    let stats = h.app.state.show_statistics;
    h.key(egui::Key::Num7);
    assert_ne!(h.app.state.show_statistics, stats, "7");
    let br = h.app.state.selection_brackets;
    h.key(egui::Key::J);
    assert_ne!(h.app.state.selection_brackets, br, "J");
    let shade = h.app.state.shade_selected_faces;
    h.key(egui::Key::F2);
    assert_ne!(h.app.state.shade_selected_faces, shade, "F2");
    let see = h.app.state.scene.node(id).unwrap().display.see_through;
    h.mods = egui::Modifiers::ALT;
    h.key(egui::Key::X);
    h.mods = egui::Modifiers::NONE;
    assert_ne!(h.app.state.scene.node(id).unwrap().display.see_through, see, "Alt+X");
    h.mods = egui::Modifiers::CTRL;
    h.key(egui::Key::X);
    assert!(h.app.state.expert_mode, "Ctrl+X");
    h.key(egui::Key::X);
    h.mods = egui::Modifiers::NONE;
    assert!(!h.app.state.expert_mode, "Ctrl+X again");
}

/// Status bar X field: click it, type a value, Enter: the selection moves there.
#[test]
fn status_bar_type_in_moves_the_selection() {
    let mut h = Harness::new();
    let id = box_scene(&mut h);
    h.key(egui::Key::W);
    let x = find_text(&mut h, "X:").expect("status bar X label");
    let field = egui::pos2(x.max.x + 22.0, x.center().y);
    h.click(field);
    h.run(vec![egui::Event::Key { key: egui::Key::A, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::CTRL }]);
    h.run(vec![egui::Event::Text("25".into())]);
    h.run(vec![egui::Event::Key { key: egui::Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }]);
    h.frames(3);
    assert!((pos_of(&h, id).x - 25.0).abs() < 1e-3, "X = 25: {}", pos_of(&h, id));
    h.cmd(Command::UndoSceneOperation);
    assert!(pos_of(&h, id).length() < 1e-4, "undoable");
}

#[test]
fn command_panel_tabs_switch_on_click() {
    let mut h = Harness::new();
    for (label, tab) in [("Modify", crate::Tab::Modify), ("Hierarchy", crate::Tab::Hierarchy), ("Motion", crate::Tab::Motion), ("Display", crate::Tab::Display), ("Utilities", crate::Tab::Utilities), ("Create", crate::Tab::Create)] {
        click_text(&mut h, label);
        assert_eq!(h.app.panel_tab, tab, "{label}");
    }
}
