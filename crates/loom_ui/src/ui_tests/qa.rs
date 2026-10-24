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
