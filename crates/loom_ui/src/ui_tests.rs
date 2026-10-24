//! UI regression tests: the whole app runs frame by frame without a window and
//! receives real pointer and key events, so viewport picking, the level keys
//! and the panels are tested the way a person uses them.

use crate::LoomApp;
use eframe::egui;
use glam::Vec3;
use loom_commands::{Command, PrimitiveKind, SelectionMode};

const SCREEN: egui::Rect = egui::Rect { min: egui::pos2(0.0, 0.0), max: egui::pos2(1280.0, 800.0) };

struct Harness {
    ctx: egui::Context,
    app: LoomApp,
    time: f64,
    mods: egui::Modifiers,
}

impl Harness {
    fn new() -> Self {
        let mut h = Harness { ctx: egui::Context::default(), app: LoomApp::headless(), time: 0.0, mods: egui::Modifiers::NONE };
        h.frames(3);
        h
    }

    fn run(&mut self, events: Vec<egui::Event>) {
        self.time += 1.0 / 30.0;
        let input = egui::RawInput { screen_rect: Some(SCREEN), time: Some(self.time), modifiers: self.mods, events, ..Default::default() };
        let app = &mut self.app;
        let _ = self.ctx.run(input, |ctx| app.frame(ctx));
    }

    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            self.run(Vec::new());
        }
    }

    fn cmd(&mut self, c: Command) {
        self.app.dispatch(c).unwrap();
        self.frames(2);
    }

    fn key(&mut self, key: egui::Key) {
        let m = self.mods;
        self.run(vec![egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: m }]);
        self.run(vec![egui::Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers: m }]);
        self.frames(1);
    }

    fn click(&mut self, p: egui::Pos2) {
        let m = self.mods;
        self.run(vec![egui::Event::PointerMoved(p)]);
        self.run(vec![egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: m }]);
        self.run(vec![egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: m }]);
        self.frames(1);
    }

    fn drag(&mut self, a: egui::Pos2, b: egui::Pos2) {
        let m = self.mods;
        self.run(vec![egui::Event::PointerMoved(a)]);
        self.run(vec![egui::Event::PointerButton { pos: a, button: egui::PointerButton::Primary, pressed: true, modifiers: m }]);
        for i in 1..=8 {
            self.run(vec![egui::Event::PointerMoved(a + (b - a) * (i as f32 / 8.0))]);
        }
        self.run(vec![egui::Event::PointerButton { pos: b, button: egui::PointerButton::Primary, pressed: false, modifiers: m }]);
        self.frames(1);
    }

    /// Where a world point lands in the active viewport.
    fn screen_of(&self, world: Vec3) -> egui::Pos2 {
        let i = self.app.state.active_viewport;
        let rect = self.app.viewport_rects[i];
        let vp = self.app.camera_for(i).view_projection(rect.width() / rect.height());
        crate::viewport_widget::project_world_to_screen(world, vp, rect).expect("point in front of the camera")
    }

    fn sub_count(&self) -> usize {
        self.app.state.sub_selection.len()
    }
}

/// A 30-unit box at the origin, as an Editable Poly, selected and framed in a
/// maximized Perspective view.
fn poly_box(h: &mut Harness) {
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [30.0; 3] });
    h.cmd(Command::ConvertToEditablePoly);
    framed(h);
}

/// The default Perspective view, maximized (an empty scene has no extents to frame).
fn maximized(h: &mut Harness) {
    if !h.app.state.maximized {
        h.cmd(Command::MaximizeViewportToggle);
    }
}

fn framed(h: &mut Harness) {
    if !h.app.state.maximized {
        h.cmd(Command::MaximizeViewportToggle);
    }
    h.cmd(Command::ZoomExtentsAll);
}

/// The middle of the box's top face (z = 30), offset off-centre so the
/// gizmo at the pivot is not under the pointer.
fn top_face_point() -> Vec3 {
    Vec3::new(7.0, 7.0, 30.0)
}

#[test]
fn clicking_a_polygon_selects_it() {
    let mut h = Harness::new();
    poly_box(&mut h);
    h.key(egui::Key::Num4);
    assert_eq!(h.app.state.selection_mode, SelectionMode::Face);
    let p = h.screen_of(top_face_point());
    h.click(p);
    assert_eq!(h.sub_count(), 1, "a click on the top face selects it");
}

#[test]
fn clicking_a_polygon_selects_it_with_the_move_tool() {
    let mut h = Harness::new();
    poly_box(&mut h);
    h.key(egui::Key::W);
    h.key(egui::Key::Num4);
    let p = h.screen_of(top_face_point());
    h.click(p);
    assert_eq!(h.sub_count(), 1, "the Move gizmo must not swallow the click");
}

#[test]
fn clicking_a_vertex_selects_it() {
    let mut h = Harness::new();
    poly_box(&mut h);
    h.key(egui::Key::Num1);
    let p = h.screen_of(Vec3::new(15.0, -15.0, 30.0));
    h.click(p);
    assert_eq!(h.sub_count(), 1);
}

#[test]
fn edit_poly_modifier_polygons_pick_and_levels_stay_in_step() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [30.0; 3] });
    h.cmd(Command::AddModifier { name: "Edit Poly".into() });
    framed(&mut h);
    h.key(egui::Key::Num4);
    let id = *h.app.state.selection.iter().next().unwrap();
    assert_eq!(h.app.state.scene.node(id).unwrap().modifiers[0].sub_level, 4, "the stack shows the level the key chose");
    let p = h.screen_of(top_face_point());
    h.click(p);
    assert_eq!(h.sub_count(), 1);
    h.key(egui::Key::Num2);
    assert_eq!(h.app.state.scene.node(id).unwrap().modifiers[0].sub_level, 2);
}

#[test]
fn alt_click_removes_and_alt_drag_orbits() {
    let mut h = Harness::new();
    poly_box(&mut h);
    let p = h.screen_of(Vec3::new(0.0, 0.0, 15.0));
    h.mods = egui::Modifiers::ALT;
    h.click(p);
    assert!(h.app.state.selection.is_empty(), "Alt+click removes the object from the selection");
    h.mods = egui::Modifiers::NONE;
    h.click(p);
    assert_eq!(h.app.state.selection.len(), 1);
    let yaw = h.app.state.active().camera.yaw;
    h.mods = egui::Modifiers::ALT;
    h.drag(p, p + egui::vec2(120.0, 0.0));
    h.mods = egui::Modifiers::NONE;
    assert_ne!(h.app.state.active().camera.yaw, yaw, "Alt+drag orbits");
    assert_eq!(h.app.state.selection.len(), 1, "and keeps the selection");
    // A plain drag afterwards is a selection drag again, not a stuck orbit.
    let yaw = h.app.state.active().camera.yaw;
    h.drag(p + egui::vec2(-200.0, -150.0), p + egui::vec2(-180.0, -130.0));
    assert_eq!(h.app.state.active().camera.yaw, yaw);
}

#[test]
fn alt_r_rings_from_the_keyboard() {
    let mut h = Harness::new();
    poly_box(&mut h);
    h.key(egui::Key::Num2);
    let p = h.screen_of(Vec3::new(15.0, 0.0, 30.0));
    h.click(p);
    assert_eq!(h.sub_count(), 1, "one edge picked");
    h.mods = egui::Modifiers::ALT;
    h.key(egui::Key::R);
    h.mods = egui::Modifiers::NONE;
    assert_eq!(h.sub_count(), 4, "the ring of a box edge is four edges");
}

/// Where the active viewport's construction plane (z = 0) is under a screen point,
/// found by searching: good enough to aim drags at known ground points.
fn ground_point(h: &Harness, world: Vec3) -> egui::Pos2 {
    h.screen_of(world)
}

#[test]
fn drag_creating_a_sphere_centres_it_on_the_click_and_selects_it() {
    let mut h = Harness::new();
    maximized(&mut h);
    h.app.create_kind = Some(crate::CreateKind::Primitive(PrimitiveKind::Sphere));
    let r = h.app.viewport_rects[h.app.state.active_viewport];
    let a = r.center() + egui::vec2(0.0, 80.0);
    h.drag(a, a + egui::vec2(60.0, 0.0));
    let id = *h.app.state.selection.iter().next().expect("the new sphere is selected");
    let p = h.app.state.scene.node(id).unwrap().transform.position.get();
    assert!(p.z.abs() < 1e-3, "the pivot stays on the construction plane (Base To Pivot off): z = {}", p.z);
    assert!(h.app.create_kind.is_some(), "the create mode stays on for the next sphere");
}

#[test]
fn drag_creating_a_box_keeps_it_selected() {
    let mut h = Harness::new();
    maximized(&mut h);
    h.app.create_kind = Some(crate::CreateKind::Primitive(PrimitiveKind::Box));
    // Screen points below the horizon of the default view: both hit the ground.
    let r = h.app.viewport_rects[h.app.state.active_viewport];
    let a = r.center() + egui::vec2(-80.0, 60.0);
    let b = r.center() + egui::vec2(80.0, 140.0);
    h.drag(a, b);
    // Height: move the mouse up, then click.
    let c = b + egui::vec2(0.0, -60.0);
    for i in 1..=6 {
        h.run(vec![egui::Event::PointerMoved(b + (c - b) * (i as f32 / 6.0))]);
    }
    h.click(c);
    assert_eq!(h.app.state.scene.len(), 1, "one box");
    assert_eq!(h.app.state.selection.len(), 1, "the new box stays selected");
    let id = *h.app.state.selection.iter().next().unwrap();
    let params = h.app.state.scene.node(id).unwrap().object.parameters();
    let get = |name: &str| params.iter().find(|(n, _)| *n == name).map(|(_, v)| format!("{v:?}")).unwrap_or_default();
    for p in ["Length", "Width", "Height"] {
        assert!(!get(p).contains("(0.0)"), "{p} was set by the drag: {}", get(p));
    }
}

#[test]
fn alt_r_rings_on_an_edit_poly_modifier_and_a_sphere() {
    for (kind, convert) in [(PrimitiveKind::Box, false), (PrimitiveKind::Sphere, true)] {
        let mut h = Harness::new();
        h.cmd(Command::CreatePrimitive { kind, position: [0.0; 3], size: [30.0; 3] });
        if convert {
            h.cmd(Command::ConvertToEditablePoly);
        } else {
            h.cmd(Command::AddModifier { name: "Edit Poly".into() });
        }
        framed(&mut h);
        h.key(egui::Key::Num2);
        let node = *h.app.state.selection.iter().next().unwrap();
        // Pick one edge by command (picking is covered elsewhere), then ring it from the keyboard.
        h.cmd(Command::SelectSub { node: Some(node.0), ids: vec![0], op: loom_commands::SelectOp::Replace });
        assert_eq!(h.sub_count(), 1);
        let p = h.screen_of(Vec3::new(0.0, 0.0, 0.0));
        h.run(vec![egui::Event::PointerMoved(p)]);
        h.mods = egui::Modifiers::ALT;
        h.key(egui::Key::R);
        h.mods = egui::Modifiers::NONE;
        assert!(h.sub_count() > 1, "{kind:?}: Alt+R rings the edge ({} selected)", h.sub_count());
    }
}

fn param(h: &Harness, name: &str) -> f64 {
    let id = *h.app.state.selection.iter().next().expect("the new object is selected");
    let params = h.app.state.scene.node(id).unwrap().object.parameters();
    params.iter().find(|(n, _)| *n == name).map(|(_, v)| v.as_f64()).unwrap_or_else(|| panic!("no parameter {name}"))
}

fn pivot(h: &Harness) -> Vec3 {
    let id = *h.app.state.selection.iter().next().expect("the new object is selected");
    h.app.state.scene.node(id).unwrap().transform.position.get()
}

/// Moves the pointer (no button) in steps, as during a height or radius step.
fn hover_to(h: &mut Harness, from: egui::Pos2, to: egui::Pos2) {
    for i in 1..=6 {
        h.run(vec![egui::Event::PointerMoved(from + (to - from) * (i as f32 / 6.0))]);
    }
}

fn creating(h: &mut Harness, kind: PrimitiveKind) {
    maximized(h);
    h.app.create_kind = Some(crate::CreateKind::Primitive(kind));
}

#[test]
fn drag_creating_a_cylinder_downwards_gives_a_negative_height() {
    let mut h = Harness::new();
    creating(&mut h, PrimitiveKind::Cylinder);
    let a = ground_point(&h, Vec3::ZERO);
    let b = ground_point(&h, Vec3::new(20.0, 0.0, 0.0));
    h.drag(a, b);
    let c = b + egui::vec2(0.0, 60.0);
    hover_to(&mut h, b, c);
    h.click(c);
    assert!(param(&h, "Height") < -1.0, "moving down builds below the plane: {}", param(&h, "Height"));
    assert!(pivot(&h).length() < 1e-2, "pivot at the base centre, on the click: {}", pivot(&h));
}

#[test]
fn edge_creation_method_draws_a_sphere_edge_to_edge() {
    let mut h = Harness::new();
    creating(&mut h, PrimitiveKind::Sphere);
    h.app.set_creation_method(PrimitiveKind::Sphere, "Edge");
    let a = ground_point(&h, Vec3::new(-20.0, 0.0, 0.0));
    let b = ground_point(&h, Vec3::new(20.0, 0.0, 0.0));
    h.drag(a, b);
    assert!((param(&h, "Radius") - 20.0).abs() < 0.5, "radius is half the drag: {}", param(&h, "Radius"));
    assert!(pivot(&h).length() < 0.5, "the centre is midway: {}", pivot(&h));
}

#[test]
fn cone_second_radius_starts_at_the_first_and_decreases_towards_a_point() {
    let mut h = Harness::new();
    creating(&mut h, PrimitiveKind::Cone);
    let a = ground_point(&h, Vec3::ZERO);
    let b = ground_point(&h, Vec3::new(20.0, 0.0, 0.0));
    h.drag(a, b);
    assert!((param(&h, "Radius 2") - param(&h, "Radius 1")).abs() < 1e-3, "a straight side while the height is set");
    let c = b + egui::vec2(0.0, -60.0);
    hover_to(&mut h, b, c);
    h.click(c);
    let r1 = param(&h, "Radius 1");
    let d = c + egui::vec2(0.0, 30.0);
    hover_to(&mut h, c, d);
    let r2 = param(&h, "Radius 2");
    assert!(r2 < r1 && r2 >= 0.0, "moving down narrows the top: r1 {r1} r2 {r2}");
    h.click(d);
    assert!(h.app.create.is_none(), "the third click finishes the cone");
}

#[test]
fn cube_creation_method_is_one_drag_with_equal_sides() {
    let mut h = Harness::new();
    creating(&mut h, PrimitiveKind::Box);
    h.app.set_creation_method(PrimitiveKind::Box, "Cube");
    let a = ground_point(&h, Vec3::ZERO);
    let b = ground_point(&h, Vec3::new(20.0, 10.0, 0.0));
    h.drag(a, b);
    assert!(h.app.create.is_none(), "a cube is a one-step operation");
    let (l, w, ht) = (param(&h, "Length"), param(&h, "Width"), param(&h, "Height"));
    assert!(l > 1.0 && (l - w).abs() < 1e-4 && (l - ht).abs() < 1e-4, "{l} {w} {ht}");
}

#[test]
fn square_plane_and_centre_pyramid_creation_methods() {
    let mut h = Harness::new();
    creating(&mut h, PrimitiveKind::Plane);
    h.app.set_creation_method(PrimitiveKind::Plane, "Square");
    let a = ground_point(&h, Vec3::ZERO);
    let b = ground_point(&h, Vec3::new(20.0, 10.0, 0.0));
    h.drag(a, b);
    assert!((param(&h, "Length") - param(&h, "Width")).abs() < 1e-4 && param(&h, "Width") > 1.0);

    let mut h = Harness::new();
    creating(&mut h, PrimitiveKind::Pyramid);
    h.app.set_creation_method(PrimitiveKind::Pyramid, "Center");
    let a = ground_point(&h, Vec3::ZERO);
    let b = ground_point(&h, Vec3::new(20.0, 10.0, 0.0));
    h.drag(a, b);
    assert!((param(&h, "Width") - 40.0).abs() < 1.0 && (param(&h, "Depth") - 20.0).abs() < 1.0, "base drawn from the centre out");
    assert!(pivot(&h).length() < 1e-2, "pivot stays at the click: {}", pivot(&h));
}
