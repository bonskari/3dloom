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
    /// The last frame's shapes, to find a widget by its text.
    shapes: Vec<egui::epaint::ClippedShape>,
    /// The window size (taller for panels with long rollouts).
    screen: egui::Rect,
}

impl Harness {
    fn new() -> Self {
        let mut h = Harness { ctx: egui::Context::default(), app: LoomApp::headless(), time: 0.0, mods: egui::Modifiers::NONE, shapes: Vec::new(), screen: SCREEN };
        h.frames(3);
        h
    }

    fn run(&mut self, events: Vec<egui::Event>) {
        self.time += 1.0 / 30.0;
        let mut input = egui::RawInput { screen_rect: Some(self.screen), time: Some(self.time), modifiers: self.mods, events, ..Default::default() };
        // As eframe does before each frame.
        self.app.raw_input_hook(&self.ctx, &mut input);
        let app = &mut self.app;
        self.shapes = self.ctx.run(input, |ctx| app.frame(ctx)).shapes;
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
        self.drag_with(egui::PointerButton::Primary, a, b);
    }

    fn drag_with(&mut self, button: egui::PointerButton, a: egui::Pos2, b: egui::Pos2) {
        let m = self.mods;
        self.run(vec![egui::Event::PointerMoved(a)]);
        self.run(vec![egui::Event::PointerButton { pos: a, button, pressed: true, modifiers: m }]);
        for i in 1..=8 {
            self.run(vec![egui::Event::PointerMoved(a + (b - a) * (i as f32 / 8.0))]);
        }
        self.run(vec![egui::Event::PointerButton { pos: b, button, pressed: false, modifiers: m }]);
        self.frames(1);
    }

    /// Where a world point lands in the active viewport.
    fn screen_of(&self, world: Vec3) -> egui::Pos2 {
        let i = self.app.state.active_viewport;
        let rect = self.app.viewport_rects[i];
        let vp = self.app.camera_for(i).view_projection(rect.width() / rect.height());
        crate::viewport_widget::project_world_to_screen(world, vp, rect).expect("point in front of the camera")
    }

    /// Screen rect of the text `text` painted last frame inside `within`.
    fn text_rect(&self, text: &str, within: egui::Rect) -> Option<egui::Rect> {
        fn find(shape: &egui::Shape, text: &str, within: egui::Rect) -> Option<egui::Rect> {
            match shape {
                egui::Shape::Vec(v) => v.iter().find_map(|s| find(s, text, within)),
                egui::Shape::Text(t) if t.galley.text() == text => Some(t.galley.rect.translate(t.pos.to_vec2())).filter(|r| within.contains_rect(*r)),
                _ => None,
            }
        }
        self.shapes.iter().find_map(|c| find(&c.shape, text, within))
    }

    fn sub_count(&self) -> usize {
        self.app.state.sub_selection.len()
    }

    /// Centres of the visible texts that read `text` (buttons, labels), in paint order.
    fn texts(&self, text: &str) -> Vec<egui::Pos2> {
        fn walk(shape: &egui::Shape, clip: egui::Rect, text: &str, out: &mut Vec<egui::Pos2>) {
            match shape {
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, clip, text, out)),
                egui::Shape::Text(t) if t.galley.text() == text => {
                    let c = t.galley.rect.translate(t.pos.to_vec2()).center();
                    if clip.contains(c) {
                        out.push(c);
                    }
                }
                _ => {}
            }
        }
        let mut out = Vec::new();
        for s in &self.shapes {
            walk(&s.shape, s.clip_rect, text, &mut out);
        }
        out
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
        // Pick one edge between two quads (a pole triangle stops a ring at once),
        // by command (picking is covered elsewhere), then ring it from the keyboard.
        let edge = {
            let mesh = h.app.state.scene.node(node).unwrap().cage_mesh().into_owned();
            let quad = |f: Option<loom_mesh::FaceId>| f.and_then(|f| mesh.face_positions(f)).is_some_and(|p| p.len() == 4);
            (0..mesh.half_edges().len() as u32)
                .map(loom_mesh::HalfEdgeId)
                .find(|e| mesh.canonical_edge_id(*e) == Some(*e) && quad(mesh.half_edges()[e.0 as usize].face) && quad(mesh.half_edges()[mesh.half_edges()[e.0 as usize].twin.0 as usize].face))
                .expect("a quad edge")
        };
        h.cmd(Command::SelectSub { node: Some(node.0), ids: vec![edge.0], op: loom_commands::SelectOp::Replace });
        assert_eq!(h.sub_count(), 1);
        let p = h.screen_of(Vec3::new(0.0, 0.0, 0.0));
        h.run(vec![egui::Event::PointerMoved(p)]);
        h.mods = egui::Modifiers::ALT;
        h.key(egui::Key::R);
        h.mods = egui::Modifiers::NONE;
        assert!(h.sub_count() > 1, "{kind:?}: Alt+R rings the edge ({} selected)", h.sub_count());
    }
}

fn right_click(h: &mut Harness, p: egui::Pos2) {
    let m = h.mods;
    h.run(vec![egui::Event::PointerMoved(p)]);
    h.run(vec![egui::Event::PointerButton { pos: p, button: egui::PointerButton::Secondary, pressed: true, modifiers: m }]);
    h.run(vec![egui::Event::PointerButton { pos: p, button: egui::PointerButton::Secondary, pressed: false, modifiers: m }]);
    h.frames(1);
}

#[test]
fn right_click_or_the_select_tool_ends_the_create_mode() {
    let mut h = Harness::new();
    maximized(&mut h);
    let r = h.app.viewport_rects[h.app.state.active_viewport];
    // Make a sphere, then right-click: the create mode ends (no quad menu, no new object).
    h.app.create_kind = Some(crate::CreateKind::Primitive(PrimitiveKind::Sphere));
    let a = r.center() + egui::vec2(0.0, 80.0);
    h.drag(a, a + egui::vec2(60.0, 0.0));
    assert!(h.app.create_kind.is_some(), "still creating spheres after one");
    right_click(&mut h, r.center());
    assert!(h.app.create_kind.is_none(), "right-click ends the create mode");
    assert!(h.app.quad_menu.is_none(), "and does not open the quad menu");
    // Q (Select Object) ends it too.
    h.app.create_kind = Some(crate::CreateKind::Primitive(PrimitiveKind::Box));
    h.key(egui::Key::Q);
    assert!(h.app.create_kind.is_none(), "the Select tool ends the create mode");
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
    let b = ground_point(&h, Vec3::new(3.0, 0.0, 0.0));
    h.drag(a, b);
    let c = b + egui::vec2(0.0, 60.0);
    hover_to(&mut h, b, c);
    h.click(c);
    assert!(param(&h, "Height") < -0.3, "moving down builds below the plane: {}", param(&h, "Height"));
    assert!(pivot(&h).length() < 1e-2, "pivot at the base centre, on the click: {}", pivot(&h));
}

#[test]
fn edge_creation_method_draws_a_sphere_edge_to_edge() {
    let mut h = Harness::new();
    creating(&mut h, PrimitiveKind::Sphere);
    h.app.set_creation_method(PrimitiveKind::Sphere, "Edge");
    let a = ground_point(&h, Vec3::new(-3.0, 0.0, 0.0));
    let b = ground_point(&h, Vec3::new(3.0, 0.0, 0.0));
    h.drag(a, b);
    assert!((param(&h, "Radius") - 3.0).abs() < 0.1, "radius is half the drag: {}", param(&h, "Radius"));
    assert!(pivot(&h).length() < 0.1, "the centre is midway: {}", pivot(&h));
}

#[test]
fn cone_second_radius_starts_at_the_first_and_decreases_towards_a_point() {
    let mut h = Harness::new();
    creating(&mut h, PrimitiveKind::Cone);
    let a = ground_point(&h, Vec3::ZERO);
    let b = ground_point(&h, Vec3::new(3.0, 0.0, 0.0));
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
    let b = ground_point(&h, Vec3::new(3.0, 1.5, 0.0));
    h.drag(a, b);
    assert!(h.app.create.is_none(), "a cube is a one-step operation");
    let (l, w, ht) = (param(&h, "Length"), param(&h, "Width"), param(&h, "Height"));
    assert!(l > 0.5 && (l - w).abs() < 1e-4 && (l - ht).abs() < 1e-4, "{l} {w} {ht}");
}

#[test]
fn square_plane_and_centre_pyramid_creation_methods() {
    let mut h = Harness::new();
    creating(&mut h, PrimitiveKind::Plane);
    h.app.set_creation_method(PrimitiveKind::Plane, "Square");
    let a = ground_point(&h, Vec3::ZERO);
    let b = ground_point(&h, Vec3::new(3.0, 1.5, 0.0));
    h.drag(a, b);
    assert!((param(&h, "Length") - param(&h, "Width")).abs() < 1e-4 && param(&h, "Width") > 0.5);

    let mut h = Harness::new();
    creating(&mut h, PrimitiveKind::Pyramid);
    h.app.set_creation_method(PrimitiveKind::Pyramid, "Center");
    let a = ground_point(&h, Vec3::ZERO);
    let b = ground_point(&h, Vec3::new(3.0, 1.5, 0.0));
    h.drag(a, b);
    assert!((param(&h, "Width") - 6.0).abs() < 0.1 && (param(&h, "Depth") - 3.0).abs() < 0.1, "base drawn from the centre out");
    assert!(pivot(&h).length() < 1e-2, "pivot stays at the click: {}", pivot(&h));
}

#[test]
fn m_opens_the_material_editor_and_an_assigned_material_colours_the_viewport() {
    let mut h = Harness::new();
    poly_box(&mut h);
    let p = h.screen_of(Vec3::new(0.0, 0.0, 15.0));
    h.run(vec![egui::Event::PointerMoved(p)]);
    h.key(egui::Key::M);
    assert!(h.app.state.material_editor_open);
    h.cmd(Command::SetMaterialColor { slot: None, which: "Diffuse".into(), rgb: [255, 0, 0] });
    h.cmd(Command::SetMaterialParameter { slot: None, name: "Specular Level".into(), value: 80.0 });
    h.cmd(Command::AssignMaterialToSelection);
    let style = h.app.world_styles[0];
    assert!(style.color[0] > 0.9 && style.color[1] < 0.05, "faces take the diffuse colour: {:?}", style.color);
    assert!(style.specular[3] > 0.0, "and the highlight");
    assert!(style.wire_color.is_some(), "edges keep the object colour");
    h.key(egui::Key::M);
    assert!(!h.app.state.material_editor_open, "M again closes it");
}

/// The Create panel's Object Type buttons go through `start_create`: the
/// Select tool must not switch the new create tool straight off again.
#[test]
fn create_panel_buttons_keep_their_tool_on() {
    let mut h = Harness::new();
    maximized(&mut h);
    h.key(egui::Key::W);
    for kind in [crate::CreateKind::Primitive(PrimitiveKind::Box), crate::CreateKind::Helper(crate::HelperKind::Dummy), crate::CreateKind::Bones, crate::CreateKind::Line] {
        h.app.start_create(Some(kind));
        h.frames(2);
        assert_eq!(h.app.create_kind, Some(kind), "the {kind:?} tool stays on");
        assert_eq!(h.app.state.active_tool, loom_commands::Tool::Select);
    }
    h.app.start_create(None);
    assert!(h.app.create_kind.is_none());
}

/// The maximized Perspective view pulled back so a 200-unit area shows.
fn wide_view(h: &mut Harness) {
    maximized(h);
    h.app.state.active_mut().camera = loom_input::OrbitCamera { target: Vec3::ZERO, distance: 300.0, yaw: 0.75, pitch: 0.6 };
    h.frames(2);
}

fn camera_target(h: &Harness, cam: loom_scene::NodeId) -> Option<loom_scene::NodeId> {
    match h.app.state.scene.node(cam).unwrap().object {
        loom_scene::ObjectKind::Camera { target, .. } => target,
        _ => panic!("not a camera"),
    }
}

#[test]
fn dragging_a_target_camera_puts_the_camera_at_the_press_and_the_target_at_the_release() {
    let mut h = Harness::new();
    wide_view(&mut h);
    h.app.create_kind = Some(crate::CreateKind::Camera { target: true });
    let (from, to) = (Vec3::new(-40.0, -40.0, 0.0), Vec3::new(30.0, 20.0, 0.0));
    let (a, b) = (h.screen_of(from), h.screen_of(to));
    h.drag(a, b);
    let cam = *h.app.state.selection.iter().next().expect("the new camera is selected");
    let t = camera_target(&h, cam).expect("a target camera");
    let eye = h.app.state.scene.world_matrix(cam).w_axis.truncate();
    let at = h.app.state.scene.world_matrix(t).w_axis.truncate();
    assert!(eye.distance(from) < 0.5, "camera at the press: {eye}");
    assert!(at.distance(to) < 0.5, "target at the release: {at}");
    let forward = -h.app.state.scene.world_matrix(cam).z_axis.truncate().normalize();
    assert!(forward.distance((to - from).normalize()) < 1e-2, "the camera looks at its target");
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(h.app.state.scene.len(), 0, "one undo step removes camera and target");
}

#[test]
fn clicking_a_free_camera_places_it_looking_down_and_its_icon_selects_it() {
    let mut h = Harness::new();
    wide_view(&mut h);
    h.app.create_kind = Some(crate::CreateKind::Camera { target: false });
    let at = Vec3::new(10.0, 10.0, 0.0);
    let p = h.screen_of(at);
    h.click(p);
    let cam = *h.app.state.selection.iter().next().expect("a free camera");
    assert!(camera_target(&h, cam).is_none());
    let w = h.app.state.scene.world_matrix(cam);
    assert!(w.w_axis.truncate().distance(at) < 0.5);
    assert!((-w.z_axis.truncate()).distance(-Vec3::Z) < 1e-4, "a free camera made in Perspective looks down");
    // Clicking its icon (the body behind the lens) selects it.
    h.app.create_kind = None;
    h.cmd(Command::SelectNone);
    let body = w.transform_point3(Vec3::new(0.0, 0.0, 10.0));
    let p = h.screen_of(body);
    h.click(p);
    assert!(h.app.state.selection.contains(&cam), "a click on the icon selects the camera");
}

#[test]
fn camera_view_honours_clip_manually_and_orthographic_projection() {
    let mut h = Harness::new();
    let r = h.app.dispatch(Command::CreateCameraAt { target: false, position: [0.0, -100.0, 20.0], look_at: [0.0, 0.0, 20.0] }).unwrap();
    let cam = loom_scene::NodeId(r["id"].as_u64().unwrap());
    h.cmd(Command::CameraView);
    let i = h.app.state.active_viewport;
    assert_eq!(h.app.state.viewports[i].kind, loom_commands::ViewKind::Camera(cam));
    let c = h.app.camera_for(i);
    assert!((c.near - loom_scene::camera::AUTO_NEAR_CLIP).abs() < 1e-6, "Clip Manually off: nothing nearer than 3 units");
    for (name, v) in [("Clip Manually", 1.0), ("Near Clip", 50.0), ("Far Clip", 300.0)] {
        h.cmd(Command::SetParameter { id: cam.0, name: name.into(), value: v });
    }
    let c = h.app.camera_for(i);
    assert_eq!((c.near, c.far), (50.0, 300.0));
    assert!(c.perspective);
    h.cmd(Command::SetParameter { id: cam.0, name: "Orthographic Projection".into(), value: 1.0 });
    let c = h.app.camera_for(i);
    assert!(!c.perspective && c.clip == Some((50.0, 300.0)));
    // The view looks down the camera's -Z: the point it looks at projects to the centre.
    let rect = h.app.viewport_rects[i];
    let centre = crate::viewport_widget::project_world_to_screen(Vec3::new(0.0, 0.0, 20.0), c.view_projection(rect.width() / rect.height()), rect).unwrap();
    assert!(centre.distance(rect.center()) < 1.0, "{centre:?} vs {:?}", rect.center());
}

// ---- object picking: mesh surfaces, See-Through, repeat clicks, wireframe edges ----

/// Long enough between two clicks that they are not a double-click.
fn pause(h: &mut Harness) {
    h.frames(12);
}

fn selected(h: &Harness) -> Vec<loom_scene::NodeId> {
    h.app.state.selection.iter().copied().collect()
}

fn created(h: &mut Harness, kind: PrimitiveKind, position: [f32; 3], size: f32) -> loom_scene::NodeId {
    h.cmd(Command::CreatePrimitive { kind, position, size: [size; 3] });
    *h.app.state.selection.iter().next().unwrap()
}

#[test]
fn a_click_through_the_hole_of_a_torus_selects_nothing() {
    let mut h = Harness::new();
    // Radius 1 = 15, radius 2 = 5, lying on the ground: the hole is 10 units wide at z = 5.
    let torus = created(&mut h, PrimitiveKind::Torus, [0.0; 3], 40.0);
    framed(&mut h);
    let p = h.screen_of(Vec3::new(2.0, 2.0, 5.0));
    h.click(p);
    assert!(h.app.state.selection.is_empty(), "the hole is inside the bounding box but not on the surface");
    let p = h.screen_of(Vec3::new(15.0, 0.0, 10.0));
    h.click(p);
    assert_eq!(selected(&h), vec![torus], "a click on the tube selects the torus");
}

#[test]
fn a_box_inside_a_see_through_box_is_picked_and_a_repeat_click_goes_deeper() {
    let mut h = Harness::new();
    let outer = created(&mut h, PrimitiveKind::Box, [0.0; 3], 60.0);
    let inner = created(&mut h, PrimitiveKind::Box, [0.0; 3], 10.0);
    h.cmd(Command::SelectNodes { ids: vec![outer.0], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::DisplayAsSeeThroughToggle);
    h.cmd(Command::SelectNone);
    framed(&mut h);
    let p = h.screen_of(Vec3::new(2.0, 2.0, 10.0));
    h.click(p);
    assert_eq!(selected(&h), vec![inner], "the opaque box inside wins over the See-Through one in front");
    pause(&mut h);
    h.click(p + egui::vec2(1.0, 1.0));
    assert_eq!(selected(&h), vec![outer], "a repeat click at the same spot picks the next object along the ray");
    pause(&mut h);
    h.click(p);
    assert_eq!(selected(&h), vec![inner], "and comes round again");
    pause(&mut h);
    let q = p + egui::vec2(6.0, 0.0);
    h.click(q);
    assert_eq!(selected(&h), vec![inner], "a click elsewhere starts again from the front");
}

#[test]
fn a_wireframe_view_picks_objects_by_their_edges() {
    let mut h = Harness::new();
    let b = created(&mut h, PrimitiveKind::Box, [0.0; 3], 30.0);
    h.cmd(Command::SelectNone);
    maximized(&mut h);
    h.cmd(Command::TopView);
    if h.app.state.active().shading != loom_commands::Shading::Wireframe {
        h.cmd(Command::WireframeSmoothHighlightsToggle);
    }
    h.cmd(Command::ZoomExtentsAll);
    let p = h.screen_of(Vec3::new(5.0, 5.0, 30.0));
    h.click(p);
    assert!(h.app.state.selection.is_empty(), "inside the wires there is nothing to click");
    let p = h.screen_of(Vec3::new(15.0, 5.0, 30.0)) + egui::vec2(3.0, 0.0);
    h.click(p);
    assert_eq!(selected(&h), vec![b], "a click 3 px from an edge selects the box");
}

#[test]
fn double_clicking_an_object_selects_its_hierarchy() {
    let mut h = Harness::new();
    let a = created(&mut h, PrimitiveKind::Box, [0.0; 3], 30.0);
    let b = created(&mut h, PrimitiveKind::Box, [60.0, 0.0, 0.0], 20.0);
    let c = created(&mut h, PrimitiveKind::Box, [100.0, 0.0, 0.0], 10.0);
    let other = created(&mut h, PrimitiveKind::Box, [-60.0, 0.0, 0.0], 20.0);
    h.cmd(Command::SetParent { id: b.0, parent: Some(a.0) });
    h.cmd(Command::SetParent { id: c.0, parent: Some(b.0) });
    h.cmd(Command::SelectNone);
    framed(&mut h);
    let p = h.screen_of(Vec3::new(7.0, 7.0, 30.0));
    h.click(p);
    assert_eq!(selected(&h), vec![a], "a single click selects the parent alone");
    h.click(p);
    let mut want = vec![a, b, c];
    want.sort();
    assert_eq!(selected(&h), want, "a double-click selects it and all its descendants");
    assert!(!h.app.state.selection.contains(&other));
}

#[test]
fn double_clicking_an_edge_selects_its_loop() {
    let mut h = Harness::new();
    // A 40-unit plane of 4 x 4 polygons: the edges on x = 10 form one loop of four.
    poly_plane(&mut h);
    h.key(egui::Key::Num2);
    let p = h.screen_of(Vec3::new(10.0, 5.0, 0.0));
    h.click(p);
    assert_eq!(h.sub_count(), 1, "a single click selects the edge");
    h.click(p);
    assert_eq!(h.sub_count(), 4, "a double-click selects its loop");
    pause(&mut h);
    h.click(p);
    assert_eq!(h.sub_count(), 1, "a later single click is a plain pick again");
}

// ---- a click on empty space clears the selection at the current level ----

/// A spot of the active viewport away from objects framed by Zoom Extents.
fn empty_point(h: &Harness) -> egui::Pos2 {
    let r = h.app.viewport_rects[h.app.state.active_viewport];
    egui::pos2(r.left() + r.width() * 0.06, r.top() + r.height() * 0.6)
}

/// The keys 1-5 are Editable Poly / Edit Poly shortcuts; other objects
/// change level through the command.
fn level(h: &mut Harness, mode: SelectionMode) {
    h.cmd(Command::SelectionMode { mode });
}

/// The first two knots / segments / the spline of the selected Editable Spline.
fn spline_pick(h: &mut Harness) {
    let node = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::SelectSub { node: Some(node.0), ids: vec![0, 1], op: loom_commands::SelectOp::Replace });
}

fn poly_plane(h: &mut Harness) {
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0; 3], size: [40.0, 40.0, 0.0] });
    h.cmd(Command::ConvertToEditablePoly);
    framed(h);
}

fn plane_with(h: &mut Harness, modifier: &str) {
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0; 3], size: [40.0, 40.0, 0.0] });
    h.cmd(Command::AddModifier { name: modifier.into() });
    framed(h);
}

fn editable_rectangle(h: &mut Harness) {
    h.cmd(Command::CreateShape { kind: loom_commands::ShapeKind::Rectangle, position: [0.0; 3], size: [40.0, 40.0, 0.0] });
    h.cmd(Command::ConvertToEditableSpline);
    framed(h);
}

/// A cylinder skinned to two bones, selected with Skin current in the stack.
fn skinned(h: &mut Harness) {
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Cylinder, position: [0.0; 3], size: [10.0, 10.0, 40.0] });
    let arm = *h.app.state.selection.iter().next().unwrap();
    let r = h.app.dispatch(Command::CreateBoneChain { points: vec![[0.0, 0.0, 0.0], [0.0, 0.0, 20.0], [0.0, 0.0, 40.0], [0.0, 0.0, 42.0]], name: None }).unwrap();
    let bones: Vec<u64> = serde_json::from_value(r["nodes"].clone()).unwrap();
    h.cmd(Command::SelectNodes { ids: vec![arm.0], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::SkinModifier);
    h.cmd(Command::SkinAddBones { ids: bones[..2].to_vec() });
    framed(h);
}

#[test]
fn a_skinned_object_deselects_on_an_empty_click_at_object_level_and_in_edit_envelopes() {
    let mut h = Harness::new();
    skinned(&mut h);
    assert!(h.app.state.skin_entry().is_some());
    let p = empty_point(&h);
    h.click(p);
    assert!(h.app.state.selection.is_empty(), "object level: the skinned object deselects");

    let mut h = Harness::new();
    skinned(&mut h);
    h.cmd(Command::ModifierSubobjectLevel { index: 0, level: 1 });
    assert!(h.app.state.skin_active(), "Edit Envelopes is on");
    h.cmd(Command::SkinSelectVertices { ids: vec![0, 1, 2], op: loom_commands::SelectOp::Replace });
    assert_eq!(h.app.state.skin_selection.len(), 3);
    let p = empty_point(&h);
    h.click(p);
    assert!(h.app.state.skin_selection.is_empty(), "Edit Envelopes: the vertices deselect");
}

#[test]
fn a_click_on_empty_space_clears_the_selection_at_every_level_and_tool() {
    type Setup = fn(&mut Harness);
    type Count = fn(&Harness) -> usize;
    let objects: Count = |h| h.app.state.selection.len();
    let subs: Count = |h| h.sub_count();
    let cases: Vec<(&str, Setup, Count)> = vec![
        ("object", |h| poly_box(h), objects),
        ("Editable Poly vertex", |h| { poly_plane(h); h.key(egui::Key::Num1); h.cmd(Command::SelectAll); }, subs),
        ("Editable Poly edge", |h| { poly_plane(h); h.key(egui::Key::Num2); h.cmd(Command::SelectAll); }, subs),
        ("Editable Poly border", |h| { poly_plane(h); h.key(egui::Key::Num3); h.cmd(Command::SelectAll); }, subs),
        ("Editable Poly polygon", |h| { poly_plane(h); h.key(egui::Key::Num4); h.cmd(Command::SelectAll); }, subs),
        ("Editable Poly element", |h| { poly_plane(h); h.key(egui::Key::Num5); h.cmd(Command::SelectAll); }, subs),
        ("Edit Poly vertex", |h| { plane_with(h, "Edit Poly"); h.key(egui::Key::Num1); h.cmd(Command::SelectAll); }, subs),
        ("Edit Poly polygon", |h| { plane_with(h, "Edit Poly"); h.key(egui::Key::Num4); h.cmd(Command::SelectAll); }, subs),
        ("Poly Select edge", |h| { plane_with(h, "Poly Select"); level(h, SelectionMode::Edge); h.cmd(Command::SelectAll); }, subs),
        ("Mesh Select face", |h| { plane_with(h, "Mesh Select"); level(h, SelectionMode::Face); h.cmd(Command::SelectAll); }, subs),
        ("Editable Spline vertex", |h| { editable_rectangle(h); level(h, SelectionMode::Vertex); spline_pick(h); }, subs),
        ("Editable Spline segment", |h| { editable_rectangle(h); level(h, SelectionMode::Edge); spline_pick(h); }, subs),
        ("Editable Spline spline", |h| { editable_rectangle(h); level(h, SelectionMode::Face); spline_pick(h); }, subs),
        ("Skin object", |h| skinned(h), objects),
        (
            "Skin envelope vertices",
            |h| {
                skinned(h);
                h.cmd(Command::ModifierSubobjectLevel { index: 0, level: 1 });
                h.cmd(Command::SkinSelectVertices { ids: vec![0, 1, 2], op: loom_commands::SelectOp::Replace });
            },
            |h| h.app.state.skin_selection.len(),
        ),
        (
            "FFD control points",
            |h| {
                plane_with(h, "FFD 2x2x2");
                h.cmd(Command::ModifierSubobjectLevel { index: 0, level: 1 });
                h.cmd(Command::SelectControlPoints { op: loom_commands::SelectOp::Replace, indices: vec![0, 1] });
            },
            |h| h.app.state.ffd_selection.len(),
        ),
    ];
    let mut failures = Vec::new();
    for (name, setup, count) in &cases {
        for (tool, key) in [("Select", egui::Key::Q), ("Move", egui::Key::W), ("Rotate", egui::Key::E), ("Scale", egui::Key::R)] {
            let mods: &[(&str, egui::Modifiers)] = if tool == "Select" { &[("", egui::Modifiers::NONE), ("Ctrl+", egui::Modifiers::CTRL), ("Alt+", egui::Modifiers::ALT)] } else { &[("", egui::Modifiers::NONE)] };
            for (mod_name, m) in mods {
                let mut h = Harness::new();
                setup(&mut h);
                h.key(key);
                let before = count(&h);
                if before == 0 {
                    failures.push(format!("{name}, {tool} tool: the setup selected nothing"));
                    continue;
                }
                let p = empty_point(&h);
                h.mods = *m;
                h.click(p);
                h.mods = egui::Modifiers::NONE;
                let after = count(&h);
                let want = if m.is_none() { 0 } else { before };
                if after != want {
                    failures.push(format!("{name}, {tool} tool, {mod_name}click on empty space: {before} -> {after}, want {want}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn region_selection_uses_the_drawn_geometry_not_the_bounding_box() {
    let mut h = Harness::new();
    let torus = created(&mut h, PrimitiveKind::Torus, [0.0; 3], 40.0);
    h.cmd(Command::SelectNone);
    framed(&mut h);
    h.app.state.prefs.auto_window_crossing = false;
    h.app.state.window_selection = false;
    let c = h.screen_of(Vec3::new(0.0, 0.0, 5.0));
    h.drag(c - egui::vec2(6.0, 6.0), c + egui::vec2(6.0, 6.0));
    assert!(h.app.state.selection.is_empty(), "Crossing inside the hole touches nothing");
    // Window around the drawn torus (a little margin): its box corners stick out, its vertices do not.
    let i = h.app.state.active_viewport;
    let rect = h.app.viewport_rects[i];
    let vp = h.app.camera_for(i).view_projection(rect.width() / rect.height());
    let mesh = &h.app.world_meshes()[0].1;
    let pts: Vec<egui::Pos2> = mesh.vertices().iter().filter_map(|v| crate::viewport_widget::project_world_to_screen(v.position, vp, rect)).collect();
    let bounds = egui::Rect::from_points(&pts).expand(3.0);
    h.app.state.window_selection = true;
    h.drag(bounds.min, bounds.max);
    assert_eq!(selected(&h), vec![torus], "Window around the drawn torus selects it");
}

/// An imported skinned character: one bracket per selected object, and the
/// mesh's bracket goes round the posed (Skin) result, not the bind-pose cage.
#[test]
fn an_imported_skinned_character_brackets_the_posed_mesh() {
    let mut h = Harness::new();
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../loom_fbx/tests/fixtures/sitting_idle.fbx");
    h.cmd(Command::ImportFile { path: Some(path.into()) });
    let boxes = crate::viewport_widget::selection_bracket_boxes(&h.app);
    assert_eq!(boxes.len(), h.app.state.selection.len(), "one bracket per selected object");
    let skinned = h.app.state.scene.nodes().find(|n| !n.modifiers.is_empty()).expect("the skinned mesh").id;
    h.cmd(Command::SelectNodes { ids: vec![skinned.0], op: loom_commands::SelectOp::Replace });
    let boxes = crate::viewport_widget::selection_bracket_boxes(&h.app);
    assert_eq!(boxes.len(), 1);
    let mut posed = h.app.state.scene.eval_mesh(skinned).value.mesh.clone();
    let _ = loom_mesh::edit_ops::transform_mesh(&mut posed, h.app.state.scene.object_matrix(skinned));
    let (pmin, pmax) = posed.aabb();
    assert!(boxes[0].0.distance(pmin) < 1e-3 && boxes[0].1.distance(pmax) < 1e-3, "bracket {:?} vs posed {:?}", boxes[0], (pmin, pmax));
    let (cmin, cmax) = h.app.state.bounds_of(std::iter::once(skinned)).unwrap();
    assert!((cmax - cmin).z > (pmax - pmin).z * 1.3, "the sitting pose is lower than the standing bind pose: cage {:?}, posed {:?}", (cmin, cmax), (pmin, pmax));
}

/// Create > Systems > Biped: in the Front view the press puts the feet down
/// and the drag up sets the height; one undo removes the whole biped.
#[test]
fn dragging_a_biped_sets_its_height_from_the_feet() {
    let mut h = Harness::new();
    maximized(&mut h);
    h.cmd(Command::FrontView);
    let cam = &mut h.app.state.active_mut().camera;
    cam.target = Vec3::new(0.0, 0.0, 30.0);
    cam.distance = 150.0;
    h.frames(2);
    h.app.start_create(Some(crate::CreateKind::Biped));
    h.frames(1);
    let (feet, top) = (Vec3::new(10.0, 0.0, 0.0), Vec3::new(10.0, 0.0, 60.0));
    let (a, b) = (h.screen_of(feet), h.screen_of(top));
    assert!(a.y - b.y > 100.0, "the drag is long enough on screen: {a:?} {b:?}");
    h.drag(a, b);
    let root = *h.app.state.selection.iter().next().expect("the centre of mass is selected");
    assert_eq!(h.app.state.selection.len(), 1);
    let node = h.app.state.scene.node(root).unwrap();
    assert_eq!(node.name, "Bip001");
    let loom_scene::ObjectKind::Biped(s) = &node.object else { panic!("not a biped") };
    assert!((s.height.get() - 60.0).abs() < 1.5, "height {}", s.height.get());
    assert!(s.figure_mode);
    let head_top = h.app.state.scene.nodes().find(|n| n.name == "Bip001 HeadNub").map(|n| h.app.state.scene.world_matrix(n.id).w_axis.truncate()).unwrap();
    assert!(head_top.distance(top) < 2.0, "the head reaches the release point: {head_top}");
    assert_eq!(h.app.create_kind, Some(crate::CreateKind::Biped), "the Biped tool stays on");
    assert_eq!(h.app.state.scene.len(), 35);
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(h.app.state.scene.len(), 0, "the drag is one undo step");
}

#[test]
fn realistic_shading_from_the_label_menu_runs_without_a_gpu() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Teapot, position: [0.0; 3], size: [25.0; 3] });
    let index = 3;
    let entry = crate::label_menus::shading_entries(&h.app, index).into_iter().find(|e| e.label == "Realistic (Bevy)").expect("Realistic entry");
    let crate::label_menus::Action::Command(cmd) = entry.action else { panic!("Realistic is a command") };
    h.cmd(Command::ActiveViewport { index: index as u32 });
    h.cmd(cmd);
    assert_eq!(h.app.state.viewports[index].shading, loom_commands::Shading::Realistic);
    assert!(crate::label_menus::shading_entries(&h.app, index).iter().any(|e| e.label == "Realistic (Bevy)" && e.checked));
    // Headless: no wgpu device, so the view falls back to the shaded drawing.
    assert!(!h.app.realistic_available());
    h.frames(5);
    let c = h.app.viewport_rects[index].center();
    h.drag(c, c + egui::vec2(30.0, 10.0));
    // A Render without a GPU reports it instead of panicking.
    h.cmd(Command::Render { path: Some(std::env::temp_dir().join("loom-headless-render.png").to_string_lossy().into()), width: Some(32), height: Some(32) });
    assert!(h.app.state.pending_render.is_none());
    assert!(h.app.last_error.as_deref().is_some_and(|e| e.contains("Render")), "{:?}", h.app.last_error);
}

/// A 40-unit cylinder along Z skinned to two 20-unit bones, at Edit
/// Envelopes in the Modify panel, framed in a maximized Perspective view.
fn skinned_cylinder(h: &mut Harness) -> (loom_scene::NodeId, Vec<u64>) {
    // Tall enough that the whole Skin Parameters rollout shows without scrolling.
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 2400.0));
    h.frames(2);
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Cylinder, position: [0.0; 3], size: [8.0, 8.0, 40.0] });
    let arm = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::SetParameter { id: arm.0, name: "Height Segments".into(), value: 8.0 });
    let r = h.app.dispatch(Command::CreateBoneChain { points: vec![[0.0, 0.0, 0.0], [0.0, 0.0, 20.0], [0.0, 0.0, 40.0], [0.0, 0.0, 42.0]], name: None }).unwrap();
    let bones: Vec<u64> = serde_json::from_value(r["nodes"].clone()).unwrap();
    h.cmd(Command::SelectNodes { ids: vec![arm.0], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::SkinModifier);
    h.cmd(Command::SkinAddBones { ids: bones[..2].to_vec() });
    h.cmd(Command::ModifierSubobjectLevel { index: 0, level: 1 });
    h.app.panel_tab = crate::Tab::Modify;
    framed(h);
    assert!(h.app.state.skin_active());
    (arm, bones)
}

/// The weight the Skin gives vertex `v` from bone `b` (list index).
fn skin_weight(h: &Harness, v: usize, b: u32) -> f32 {
    let e = loom_commands::skin::entry(&h.app.state).unwrap();
    let p = e.data.mesh_bind.transform_point3(e.input.vertices()[v].position);
    e.data.vertex_weights(v, p).into_iter().find(|(x, _)| *x == b).map(|x| x.1).unwrap_or(0.0)
}

/// Skinned vertices facing the camera, on screen inside the view and at
/// least 12 px from every other vertex: (vertex, screen point).
fn clear_front_vertices(h: &Harness) -> Vec<(u32, egui::Pos2)> {
    let (_, world) = loom_commands::skin::vertex_world_positions(&h.app.state).unwrap();
    let normals = loom_commands::skin::vertex_world_normals(&h.app.state).unwrap();
    let rect = h.app.viewport_rects[h.app.state.active_viewport].shrink(40.0);
    let cam = h.app.state.active().camera.eye_position();
    let screen: Vec<egui::Pos2> = world.iter().map(|q| h.screen_of(*q)).collect();
    (0..world.len())
        .filter(|i| normals[*i].dot(cam - world[*i]) > 0.0 && rect.contains(screen[*i]))
        .filter(|i| (0..world.len()).all(|j| j == *i || screen[j].distance(screen[*i]) > 12.0))
        .map(|i| (i as u32, screen[i]))
        .collect()
}

/// A viewport point at least 40 px from every skinned vertex.
fn empty_spot(h: &Harness) -> egui::Pos2 {
    let (_, world) = loom_commands::skin::vertex_world_positions(&h.app.state).unwrap();
    let screen: Vec<egui::Pos2> = world.iter().map(|q| h.screen_of(*q)).collect();
    let rect = h.app.viewport_rects[h.app.state.active_viewport];
    let p = egui::pos2(rect.right() - 120.0, rect.bottom() - 60.0);
    assert!(screen.iter().all(|s| s.distance(p) > 40.0), "the corner is clear of the mesh");
    p
}

/// A text inside the window titled `window`.
fn window_text(h: &Harness, window: &str, text: &str) -> egui::Pos2 {
    let area = h.ctx.memory(|m| m.area_rect(egui::Id::new(window))).unwrap_or_else(|| panic!("no {window} window"));
    let all: Vec<egui::Pos2> = h.texts(text).into_iter().filter(|p| area.contains(*p)).collect();
    assert_eq!(all.len(), 1, "{text:?} in {window} {} times", all.len());
    all[0]
}

/// A text in the command panel (right of the maximized viewport).
fn panel_text(h: &Harness, text: &str) -> egui::Pos2 {
    let right = h.app.viewport_rects[h.app.state.active_viewport].right();
    let all: Vec<egui::Pos2> = h.texts(text).into_iter().filter(|p| p.x > right).collect();
    assert_eq!(all.len(), 1, "{text:?} in the command panel {} times", all.len());
    all[0]
}

#[test]
fn skin_select_vertices_click_ctrl_click_and_an_empty_click_clears() {
    let mut h = Harness::new();
    let (arm, _) = skinned_cylinder(&mut h);
    let front = clear_front_vertices(&h);
    assert!(front.len() >= 2, "visible vertices to click");
    h.click(front[0].1);
    assert_eq!(loom_commands::skin::selection(&h.app.state), vec![front[0].0], "a click selects the vertex");
    h.mods = egui::Modifiers::CTRL;
    h.click(front[1].1);
    h.mods = egui::Modifiers::NONE;
    let mut both = vec![front[0].0, front[1].0];
    both.sort();
    assert_eq!(loom_commands::skin::selection(&h.app.state), both, "Ctrl adds");
    h.mods = egui::Modifiers::ALT;
    h.click(front[0].1);
    h.mods = egui::Modifiers::NONE;
    assert_eq!(loom_commands::skin::selection(&h.app.state), vec![front[1].0], "Alt removes");
    // A plain click on nothing clears the vertices and stays in Edit Envelopes on the object.
    h.click(empty_spot(&h));
    assert!(loom_commands::skin::selection(&h.app.state).is_empty(), "an empty click clears the vertex selection");
    assert!(h.app.state.selection.contains(&arm) && h.app.state.skin_active());
    // Select > Vertices off (the panel checkbox): clicks no longer pick vertices, an empty click still keeps the level.
    h.click(panel_text(&h, "Vertices"));
    assert!(!h.app.state.skin_filter_vertices);
    h.click(front[0].1);
    assert!(loom_commands::skin::selection(&h.app.state).is_empty(), "no vertex picking with Vertices off");
    h.click(empty_spot(&h));
    assert!(h.app.state.selection.contains(&arm) && h.app.state.skin_active(), "the object and its level stay");
}

#[test]
fn skin_backface_cull_vertices_skips_vertices_facing_away() {
    let mut h = Harness::new();
    skinned_cylinder(&mut h);
    let (_, world) = loom_commands::skin::vertex_world_positions(&h.app.state).unwrap();
    let normals = loom_commands::skin::vertex_world_normals(&h.app.state).unwrap();
    let cam = h.app.state.active().camera.eye_position();
    let back: Vec<u32> = (0..world.len() as u32).filter(|i| normals[*i as usize].dot(cam - world[*i as usize]) < -0.5).collect();
    assert!(!back.is_empty());
    let rect = h.app.viewport_rects[h.app.state.active_viewport];
    let region = (egui::pos2(rect.left() + 2.0, rect.top() + 40.0), egui::pos2(rect.right() - 2.0, rect.bottom() - 2.0));
    h.drag(region.0, region.1);
    let all = loom_commands::skin::selection(&h.app.state);
    assert!(back.iter().all(|v| all.contains(v)), "without the cull a window selects the back vertices too");
    h.click(panel_text(&h, "Backface Cull Vertices"));
    assert!(h.app.state.skin_backface_cull);
    h.drag(region.0, region.1);
    let culled = loom_commands::skin::selection(&h.app.state);
    assert!(!culled.is_empty() && back.iter().all(|v| !culled.contains(v)), "Backface Cull Vertices leaves the far side out");
}

#[test]
fn weight_tool_preset_buttons_set_the_current_bone_on_the_selection() {
    let mut h = Harness::new();
    let (_, bones) = skinned_cylinder(&mut h);
    // A vertex both envelopes reach (z 12..28), so its weight can move between the bones.
    let (_, world) = loom_commands::skin::vertex_world_positions(&h.app.state).unwrap();
    let front: Vec<(u32, egui::Pos2)> = clear_front_vertices(&h).into_iter().filter(|(v, _)| (12.0..28.0).contains(&world[*v as usize].z)).collect();
    let v = front[0].0;
    h.click(front[0].1);
    h.cmd(Command::SkinSelectBone { id: bones[1] });
    h.click(panel_text(&h, "Weight Tool"));
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::WeightTool));
    for (label, w) in [(".25", 0.25), (".5", 0.5), (".75", 0.75), (".1", 0.1), (".9", 0.9), ("0", 0.0), ("1", 1.0)] {
        let undo = h.app.state.undo.len();
        h.click(window_text(&h, "Weight Tool", label));
        assert!((skin_weight(&h, v as usize, 1) - w).abs() < 1e-4, "{label}: {}", skin_weight(&h, v as usize, 1));
        assert_eq!(h.app.state.undo.len(), undo + 1, "{label} is one undo entry");
    }
    h.click(window_text(&h, "Weight Tool", ".5"));
    // Scale Weight scales by its own field (0.95), Set Weight sets the Set Weight field (0.5).
    h.click(window_text(&h, "Weight Tool", "Scale Weight"));
    assert!((skin_weight(&h, v as usize, 1) - 0.475).abs() < 1e-4);
    h.click(window_text(&h, "Weight Tool", "Set Weight"));
    assert!((skin_weight(&h, v as usize, 1) - 0.5).abs() < 1e-4);
    window_text(&h, "Weight Tool", "0 Copied   1 Selected");
}

#[test]
fn paint_weights_stroke_brushes_the_current_bone_alt_erases_and_right_click_ends() {
    let mut h = Harness::new();
    let (_, bones) = skinned_cylinder(&mut h);
    h.cmd(Command::SkinSelectBone { id: bones[1] });
    h.cmd(Command::SkinSetPainterOption { name: "Max Size".into(), value: 8.0 });
    h.click(panel_text(&h, "Paint Weights"));
    assert!(loom_commands::skin::painting(&h.app.state), "the Paint Weights button turns the brush on");
    // Stroke across the lower front of the cylinder, where the upper bone has no weight yet.
    let (_, world) = loom_commands::skin::vertex_world_positions(&h.app.state).unwrap();
    let low: Vec<(u32, egui::Pos2)> = clear_front_vertices(&h).into_iter().filter(|(v, _)| (world[*v as usize].z - 5.0).abs() < 0.1).collect();
    assert!(low.len() >= 2, "two front vertices on the z = 5 ring");
    let before: Vec<f32> = low.iter().map(|(v, _)| skin_weight(&h, *v as usize, 1)).collect();
    let undo = h.app.state.undo.len();
    h.drag(low[0].1, low[1].1);
    let after: Vec<f32> = low.iter().map(|(v, _)| skin_weight(&h, *v as usize, 1)).collect();
    assert!(after.iter().zip(&before).all(|(a, b)| a > b), "the stroke weights the upper bone: {before:?} -> {after:?}");
    assert_eq!(h.app.state.undo.len(), undo + 1, "one stroke, one undo entry");
    assert!(loom_commands::skin::selection(&h.app.state).is_empty(), "painting does not select");
    // Alt+drag erases.
    h.mods = egui::Modifiers::ALT;
    h.drag(low[0].1, low[1].1);
    h.mods = egui::Modifiers::NONE;
    let erased: Vec<f32> = low.iter().map(|(v, _)| skin_weight(&h, *v as usize, 1)).collect();
    assert!(erased.iter().zip(&after).all(|(e, a)| e < a), "Alt takes weight away: {after:?} -> {erased:?}");
    // Ctrl+Shift drag sizes the brush.
    h.mods = egui::Modifiers::CTRL | egui::Modifiers::SHIFT;
    h.drag(low[0].1, low[0].1 + egui::vec2(60.0, 0.0));
    h.mods = egui::Modifiers::NONE;
    assert!(h.app.state.skin_painter.size > 8.0, "{}", h.app.state.skin_painter.size);
    let r = h.app.viewport_rects[h.app.state.active_viewport];
    right_click(&mut h, r.center());
    assert!(!loom_commands::skin::painting(&h.app.state), "right-click ends Paint Weights");
    assert!(h.app.quad_menu.is_none(), "without opening the quad menu");
}

#[test]
fn weight_table_cells_edit_by_right_click_and_drag_and_ids_select() {
    let mut h = Harness::new();
    let (_, bones) = skinned_cylinder(&mut h);
    h.click(panel_text(&h, "Weight Table"));
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::WeightTable));
    h.frames(2);
    let names: Vec<String> = bones[..2].iter().map(|b| h.app.state.scene.node(loom_scene::NodeId(*b)).unwrap().name.clone()).collect();
    let (row, col) = (window_text(&h, "Weight Table", "#1"), window_text(&h, "Weight Table", &names[1]));
    let cell = egui::pos2(col.x, row.y);
    right_click(&mut h, cell);
    assert_eq!(skin_weight(&h, 1, 1), 0.0, "right-click enters 0");
    h.mods = egui::Modifiers::CTRL;
    right_click(&mut h, cell);
    h.mods = egui::Modifiers::NONE;
    assert!((skin_weight(&h, 1, 1) - 1.0).abs() < 1e-4, "Ctrl+right-click enters 1");
    // Dragging the cell to the left lowers it, as one undo entry.
    let undo = h.app.state.undo.len();
    h.drag(cell, cell - egui::vec2(40.0, 0.0));
    let w = skin_weight(&h, 1, 1);
    assert!(w < 0.99 && w > 0.0, "{w}");
    assert_eq!(h.app.state.undo.len(), undo + 1);
    // A click on a vertex ID selects it.
    h.click(window_text(&h, "Weight Table", "#2"));
    assert_eq!(loom_commands::skin::selection(&h.app.state), vec![2]);
}

fn line_world_knots(h: &Harness, id: loom_scene::NodeId) -> Vec<Vec3> {
    let m = h.app.state.scene.object_matrix(id);
    let n = h.app.state.scene.node(id).unwrap();
    n.edit_spline().unwrap().shape.splines[0].knots.iter().map(|k| m.transform_point3(k.pos)).collect()
}

#[test]
fn shift_lines_a_new_line_knot_up_with_the_last() {
    let mut h = Harness::new();
    wide_view(&mut h);
    h.app.start_create(Some(crate::CreateKind::Line));
    h.frames(1);
    let a = ground_point(&h, Vec3::ZERO);
    h.click(a);
    let id = h.app.line.as_ref().expect("the line started").node;
    h.mods = egui::Modifiers::SHIFT;
    let b = ground_point(&h, Vec3::new(4.0, 30.0, 0.0));
    h.click(b);
    let c = ground_point(&h, Vec3::new(30.0, 34.0, 0.0));
    h.click(c);
    h.mods = egui::Modifiers::NONE;
    let k = line_world_knots(&h, id);
    assert_eq!(k.len(), 3);
    assert!((k[1].x - k[0].x).abs() < 1e-4 && k[1].y > 25.0, "Shift keeps the second knot straight above the first: {:?}", k[1]);
    assert!((k[2].y - k[1].y).abs() < 1e-3 && k[2].x > 25.0, "and the third level with the second: {:?}", k[2]);
}

#[test]
fn clicking_the_first_line_knot_asks_close_spline() {
    let mut h = Harness::new();
    wide_view(&mut h);
    h.app.start_create(Some(crate::CreateKind::Line));
    h.frames(1);
    for p in [Vec3::ZERO, Vec3::new(40.0, 0.0, 0.0), Vec3::new(40.0, 40.0, 0.0)] {
        let s = ground_point(&h, p);
        h.click(s);
    }
    let id = h.app.line.as_ref().unwrap().node;
    let first = ground_point(&h, Vec3::ZERO);
    h.click(first);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::CloseSpline), "Close spline? is asked");
    assert!(h.app.line.is_some(), "the line waits for the answer");
    assert!(!h.app.state.scene.node(id).unwrap().edit_spline().unwrap().shape.splines[0].closed);
    // Clicks in the viewport wait too.
    let other = ground_point(&h, Vec3::new(-30.0, 20.0, 0.0));
    h.click(other);
    assert_eq!(line_world_knots(&h, id).len(), 3);
    h.cmd(Command::LineCloseReply { close: true });
    assert!(h.app.line.is_none(), "Yes ends the line");
    let sh = h.app.state.scene.node(id).unwrap().edit_spline().unwrap().shape.clone();
    assert!(sh.splines[0].closed && sh.splines[0].knots.len() == 3);
    assert_eq!(h.app.state.undo.labels().last().map(String::as_str), Some("Create Line"), "one undo entry for the whole line");
}

/// A 40-unit square Editable Spline at the origin, selected and framed.
fn spline_square(h: &mut Harness) -> loom_scene::NodeId {
    h.cmd(Command::CreateShape { kind: loom_commands::ShapeKind::Rectangle, position: [0.0; 3], size: [40.0, 40.0, 0.0] });
    h.cmd(Command::ConvertToEditableSpline);
    framed(h);
    *h.app.state.selection.iter().next().unwrap()
}

fn spline_shape(h: &Harness, id: loom_scene::NodeId) -> loom_shape::Shape {
    h.app.state.scene.node(id).unwrap().edit_spline().unwrap().shape.clone()
}

#[test]
fn refine_with_connect_by_clicks_and_right_click_ends_it() {
    let mut h = Harness::new();
    let id = spline_square(&mut h);
    h.cmd(Command::SplineSettings { name: "Refine Connect".into(), value: 1.0 });
    h.cmd(Command::RefineSpline);
    for p in [Vec3::new(0.0, -20.0, 0.0), Vec3::new(0.0, 20.0, 0.0)] {
        let s = h.screen_of(p);
        h.click(s);
    }
    let sh = spline_shape(&h, id);
    assert_eq!(sh.splines.len(), 2, "the two refined vertices are connected");
    assert!(sh.splines[1].knots[0].pos.distance(Vec3::new(0.0, -20.0, 0.0)) < 0.5 && sh.splines[1].knots[1].pos.distance(Vec3::new(0.0, 20.0, 0.0)) < 0.5, "{:?}", sh.splines[1].knots);
    let r = h.app.viewport_rects[h.app.state.active_viewport];
    right_click(&mut h, r.right_bottom() - egui::vec2(60.0, 60.0));
    assert_eq!(h.app.state.pick_mode, None, "right-click ends Refine");
    // A click on an existing vertex asks Refine / Connect Only.
    h.cmd(Command::RefineSpline);
    let corner = h.screen_of(Vec3::new(-20.0, -20.0, 0.0));
    h.click(corner);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::RefineVertex));
    h.cmd(Command::SplineRefineReply { connect_only: true, dont_show: false });
    let s = h.screen_of(Vec3::new(20.0, 0.0, 0.0));
    h.click(s);
    let sh = spline_shape(&h, id);
    assert_eq!(sh.splines.len(), 3, "Connect Only started a new connection at the corner");
    assert!(sh.splines[2].knots[0].pos.distance(Vec3::new(-20.0, -20.0, 0.0)) < 1e-3, "{:?}", sh.splines[2].knots);
}

#[test]
fn fillet_mode_drags_on_a_selected_vertex() {
    let mut h = Harness::new();
    let id = spline_square(&mut h);
    h.cmd(Command::SelectionMode { mode: SelectionMode::Vertex });
    h.cmd(Command::SelectSub { node: None, ids: vec![0, 1], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::SplineFilletMode { chamfer: false });
    let undo = h.app.state.undo.len();
    let a = h.screen_of(Vec3::new(-20.0, -20.0, 0.0));
    h.drag(a, a - egui::vec2(0.0, 40.0));
    assert_eq!(spline_shape(&h, id).splines[0].knots.len(), 6, "both selected corners are rounded");
    assert_eq!(h.app.state.undo.len(), undo + 1, "the drag is one undo entry");
    assert_eq!(h.app.state.pick_mode, Some(loom_commands::PickMode::SplineFillet), "Fillet stays on");
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(spline_shape(&h, id).splines[0].knots.len(), 4);
}

#[test]
fn extrude_along_spline_dialog_previews_the_picked_spline() {
    let mut h = Harness::new();
    poly_box(&mut h);
    let b = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::LineStart { point: [0.0, 0.0, 30.0] });
    let line = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::LineAddKnot { node: line.0, point: [0.0, 0.0, 60.0], smooth: false });
    h.cmd(Command::LineEnd { node: line.0 });
    h.cmd(Command::SelectNodes { ids: vec![b.0], op: loom_commands::SelectOp::Replace });
    h.key(egui::Key::Num4);
    let p = h.screen_of(top_face_point());
    h.click(p);
    assert_eq!(h.sub_count(), 1);
    let faces = |h: &Harness| h.app.state.scene.node(b).unwrap().object.mesh().face_count();
    let before = faces(&h);
    h.cmd(Command::ExtrudeAlongSplineSettings);
    h.cmd(Command::ExtrudeAlongSplineMode);
    h.cmd(Command::PickHit { id: line.0 });
    h.frames(2);
    assert!(faces(&h) > before, "the dialog previews the extrusion once the spline is picked");
    h.cmd(Command::CloseDialog);
    h.frames(2);
    assert_eq!(faces(&h), before, "closing without OK takes the preview back");
}

fn camera(h: &Harness) -> (f32, f32, f32, Vec3) {
    let c = &h.app.state.active().camera;
    (c.yaw, c.pitch, c.distance, c.target)
}

#[test]
fn viewport_navigation_orbits_pans_and_zooms() {
    let mut h = Harness::new();
    poly_box(&mut h);
    let c = h.app.viewport_rects[h.app.state.active_viewport].center() + egui::vec2(150.0, 100.0);
    // Alt+MMB orbits.
    let before = camera(&h);
    h.mods = egui::Modifiers::ALT;
    h.drag_with(egui::PointerButton::Middle, c, c + egui::vec2(120.0, 40.0));
    h.mods = egui::Modifiers::NONE;
    let after = camera(&h);
    assert!(after.0 != before.0 && after.1 != before.1, "Alt+MMB orbits: {before:?} -> {after:?}");
    // MMB pans.
    let before = camera(&h);
    h.drag_with(egui::PointerButton::Middle, c, c + egui::vec2(80.0, 30.0));
    let after = camera(&h);
    assert_ne!(after.3, before.3, "MMB pans");
    assert_eq!(after.0, before.0, "a pan does not orbit");
    // The wheel zooms.
    let before = camera(&h);
    h.run(vec![egui::Event::PointerMoved(c)]);
    h.run(vec![egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Line, delta: egui::vec2(0.0, 3.0), modifiers: egui::Modifiers::NONE }]);
    h.frames(1);
    assert_ne!(camera(&h).2, before.2, "the wheel zooms");
    // Alt+LMB drag orbits.
    let before = camera(&h);
    h.mods = egui::Modifiers::ALT;
    h.drag(c, c + egui::vec2(-100.0, 50.0));
    h.mods = egui::Modifiers::NONE;
    assert_ne!(camera(&h).0, before.0, "Alt+LMB drag orbits");
}

/// A button pressed with `mods` at `a`, moved to `b`, then the window loses the
/// pointer and the focus (screen lock, Alt+Tab) and the release never arrives;
/// the windowing layer has cleared the modifiers. Focus comes back with no
/// button down.
/// `apart`: the pointer leaves some frames before the focus goes.
fn press_then_lose_focus(h: &mut Harness, button: egui::PointerButton, mods: egui::Modifiers, a: egui::Pos2, b: egui::Pos2, apart: bool) {
    h.mods = mods;
    h.run(vec![egui::Event::PointerMoved(a)]);
    h.run(vec![egui::Event::PointerButton { pos: a, button, pressed: true, modifiers: mods }]);
    for i in 1..=4 {
        h.run(vec![egui::Event::PointerMoved(a + (b - a) * (i as f32 / 4.0))]);
    }
    h.mods = egui::Modifiers::NONE;
    if apart {
        h.run(vec![egui::Event::PointerGone]);
        h.frames(3);
        h.run(vec![egui::Event::WindowFocused(false)]);
    } else {
        h.run(vec![egui::Event::PointerGone, egui::Event::WindowFocused(false)]);
    }
    h.frames(3);
    h.run(vec![egui::Event::WindowFocused(true)]);
    h.frames(1);
}

/// After the lost release: hovering does not navigate, and Alt+MMB, MMB,
/// Alt+LMB drag and Alt+click work as before.
fn navigation_works_again(h: &mut Harness, c: egui::Pos2, label: &str) {
    let before = camera(h);
    hover_to(h, c, c + egui::vec2(60.0, 40.0));
    assert_eq!(camera(h), before, "{label}: moving the pointer with no button down does not navigate");
    let before = camera(h);
    h.mods = egui::Modifiers::ALT;
    h.drag_with(egui::PointerButton::Middle, c, c + egui::vec2(120.0, 40.0));
    h.mods = egui::Modifiers::NONE;
    assert_ne!(camera(h).0, before.0, "{label}: Alt+MMB orbits");
    let before = camera(h);
    h.drag_with(egui::PointerButton::Middle, c, c + egui::vec2(60.0, 20.0));
    assert_ne!(camera(h).3, before.3, "{label}: MMB pans");
    assert_eq!(camera(h).0, before.0, "{label}: MMB does not orbit");
    let before = camera(h);
    h.mods = egui::Modifiers::ALT;
    h.drag(c, c + egui::vec2(-100.0, 30.0));
    h.mods = egui::Modifiers::NONE;
    assert_ne!(camera(h).0, before.0, "{label}: Alt+LMB drag orbits");
    // Alt+click on the box removes it from the selection.
    h.cmd(Command::ZoomExtentsAll);
    let p = h.screen_of(Vec3::new(0.0, 0.0, 15.0));
    h.click(p);
    assert_eq!(h.app.state.selection.len(), 1, "{label}: a click selects the box");
    let before = camera(h);
    h.mods = egui::Modifiers::ALT;
    h.click(p);
    h.mods = egui::Modifiers::NONE;
    assert!(h.app.state.selection.is_empty(), "{label}: Alt+click subtracts");
    assert_eq!(camera(h), before, "{label}: Alt+click does not orbit");
    // A plain drag is a selection drag, not navigation.
    let before = camera(h);
    h.drag(c + egui::vec2(-200.0, -150.0), c + egui::vec2(-180.0, -130.0));
    assert_eq!(camera(h), before, "{label}: plain LMB drag does not navigate");
}

#[test]
fn a_move_drag_ends_when_the_focus_is_lost() {
    let mut h = Harness::new();
    poly_box(&mut h);
    h.key(egui::Key::W);
    let a = h.screen_of(pivot(&h));
    press_then_lose_focus(&mut h, egui::PointerButton::Primary, egui::Modifiers::NONE, a, a + egui::vec2(40.0, 0.0), true);
    assert!(h.app.transform_drag.is_none(), "the move drag ended with the focus");
    let moved = pivot(&h);
    assert_ne!(moved, Vec3::ZERO, "the drag before the focus loss moved the box");
    hover_to(&mut h, a + egui::vec2(40.0, 0.0), a + egui::vec2(120.0, 30.0));
    assert_eq!(pivot(&h), moved, "moving the pointer afterwards does not move the box");
}

#[test]
fn navigation_recovers_after_a_release_lost_to_focus_loss() {
    use egui::PointerButton::{Middle, Primary};
    let cases = [
        ("Alt+MMB orbit", Middle, egui::Modifiers::ALT),
        ("MMB pan", Middle, egui::Modifiers::NONE),
        ("Alt+LMB orbit", Primary, egui::Modifiers::ALT),
        ("Alt+LMB press", Primary, egui::Modifiers::ALT),
        ("LMB region", Primary, egui::Modifiers::NONE),
    ];
    let mut failures = Vec::new();
    for ((label, button, mods), apart) in cases.into_iter().flat_map(|c| [(c, false), (c, true)]) {
        let result = std::panic::catch_unwind(|| {
            let mut h = Harness::new();
            poly_box(&mut h);
            let c = h.app.viewport_rects[h.app.state.active_viewport].center() + egui::vec2(150.0, 100.0);
            let b = if label == "Alt+LMB press" { c + egui::vec2(1.0, 0.0) } else { c + egui::vec2(80.0, 30.0) };
            press_then_lose_focus(&mut h, button, mods, c, b, apart);
            navigation_works_again(&mut h, c, &format!("{label} (pointer left first: {apart})"));
        });
        if let Err(e) = result {
            failures.push(e.downcast_ref::<String>().cloned().unwrap_or_else(|| label.to_string()));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// The Material Editor window's rect.
fn material_editor_rect(h: &Harness) -> egui::Rect {
    h.ctx.memory(|m| m.area_rect(egui::Id::new("Material Editor"))).expect("the Material Editor is shown")
}

fn editor_material(h: &Harness) -> loom_scene::material::StandardMaterial {
    h.app.state.scene.slot_material(h.app.state.scene.material_editor.active)
}

/// The Material Editor open (M) on a selected box, with the in-app file chooser.
fn material_editor(h: &mut Harness) {
    poly_box(h);
    h.app.native_file_dialogs = false;
    h.cmd(Command::MaterialEditorToggle);
    assert!(h.app.state.material_editor_open);
}

/// Clicks the text `label` inside `within` (or `dx` past its right end, for the field beside a label).
fn click_text(h: &mut Harness, label: &str, within: egui::Rect, dx: f32) {
    let r = h.text_rect(label, within).unwrap_or_else(|| panic!("{label:?} is not shown in {within:?}"));
    h.click(r.center() + egui::vec2(if dx > 0.0 { r.width() / 2.0 + dx } else { 0.0 }, 0.0));
    h.frames(2);
}

#[test]
fn material_editor_stays_open_across_a_map_pick_and_cancel() {
    let mut h = Harness::new();
    material_editor(&mut h);
    h.cmd(Command::MaterialEditorSlot { slot: 2 });
    let win = material_editor_rect(&h);
    // Diffuse Color's map button opens the bitmap chooser over the editor.
    click_text(&mut h, "None", win, 0.0);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::MaterialMap { channel: 0 }));
    assert!(h.app.state.material_editor_open, "the chooser leaves the editor open");
    assert!(h.text_rect("Get Material", win).is_some(), "and the editor is still drawn");
    click_text(&mut h, "Cancel", SCREEN, 0.0);
    assert_eq!(h.app.state.dialog, None, "Cancel closes the chooser");
    assert!(h.app.state.material_editor_open, "but not the editor");
    assert_eq!(h.app.state.scene.material_editor.active, 2, "on the same slot");
    assert!(editor_material(&h).map("Diffuse Color").is_none());
    // Choose a file: the map goes into the slot and the editor stays.
    click_text(&mut h, "None", win, 0.0);
    click_text(&mut h, "File name:", SCREEN, 40.0);
    h.run(vec![egui::Event::Text("wood.png".into())]);
    click_text(&mut h, "Open", SCREEN, 0.0);
    assert_eq!(h.app.state.dialog, None);
    assert!(h.app.state.material_editor_open);
    assert_eq!(h.app.state.scene.material_editor.active, 2);
    let map = editor_material(&h).map("Diffuse Color").cloned().expect("the chosen bitmap is the Diffuse Color map");
    assert!(map.path.ends_with("wood.png"), "{}", map.path);
    assert!(h.text_rect("wood.png", material_editor_rect(&h)).is_some(), "the button shows the bitmap");
}

#[test]
fn only_the_map_buttons_open_a_chooser_from_the_material_editor() {
    let mut h = Harness::new();
    material_editor(&mut h);
    let stays = |h: &Harness, what: &str| {
        assert_eq!(h.app.state.dialog, None, "{what} opened a dialog");
        assert!(h.app.state.material_editor_open, "{what} closed the editor");
    };
    // Sample slot 8 (second row, second column).
    let win = material_editor_rect(&h);
    let get = h.text_rect("Get Material", win).unwrap();
    let slot_h = (get.top() - win.top() - 30.0) / 4.0;
    h.click(egui::pos2(win.left() + 6.0 + 68.0 + 32.0, win.top() + 30.0 + slot_h * 1.5));
    h.frames(2);
    assert_eq!(h.app.state.scene.material_editor.active, 7, "the slot click lands");
    stays(&h, "a sample slot");
    for (label, dx) in [("Get Material", 0.0), ("Assign to Selection", 0.0), ("Reset", 0.0), ("Show Map in Viewport", 0.0), ("Wire", 0.0), ("2-Sided", 0.0), ("Faceted", 0.0), ("Lock Ambient and Diffuse", 0.0), ("Self-Illumination", 20.0), ("Opacity", 20.0), ("Glossiness", 20.0), ("Standard", 0.0), ("Shader Basic Parameters", 0.0)] {
        let win = material_editor_rect(&h);
        click_text(&mut h, label, win, dx);
        stays(&h, label);
    }
    let m = editor_material(&h);
    assert!(m.wire && m.two_sided && m.faceted && m.show_map_in_viewport, "the clicks landed: {m:?}");
    assert_eq!(h.app.state.scene.material_editor.active, 7);
    // The name field.
    let name = m.name.clone();
    let win = material_editor_rect(&h);
    click_text(&mut h, &name, win, 0.0);
    h.run(vec![egui::Event::Text("x".into())]);
    h.key(egui::Key::Enter);
    stays(&h, "the name field");
    assert_ne!(editor_material(&h).name, name, "the name edit landed");
    // A colour swatch opens its picker, not a file chooser.
    let win = material_editor_rect(&h);
    click_text(&mut h, "Diffuse:", win, 20.0);
    assert!(h.ctx.memory(|m| m.any_popup_open()), "the colour picker opened");
    stays(&h, "the Diffuse swatch");
    h.key(egui::Key::Escape);
    stays(&h, "Escape");
}

/// The rect of a viewport label of the active viewport: 0 = [+], 1 = POV, 2 = shading.
fn label_rect(h: &Harness, which: usize) -> egui::Rect {
    let i = h.app.state.active_viewport;
    let rect = h.app.viewport_rects[i];
    let texts = ["[ + ]".to_owned(), format!("[ {} ]", h.app.state.view_label(i)), format!("[ {} ]", h.app.state.viewports[i].shading.label())];
    let mut x = rect.min.x + 6.0;
    for (n, t) in texts.into_iter().enumerate() {
        let size = h.ctx.fonts(|f| f.layout_no_wrap(t, egui::FontId::proportional(13.0), egui::Color32::WHITE).size());
        let r = egui::Rect::from_min_size(egui::pos2(x, rect.min.y + 4.0), size);
        if n == which {
            return r;
        }
        x += size.x + 10.0;
    }
    unreachable!()
}

/// Open menus (and submenus), left to right.
fn menus(h: &Harness) -> Vec<egui::Rect> {
    let mut v: Vec<egui::Rect> = h.ctx.memory(|m| {
        m.areas().visible_layer_ids().into_iter().filter(|l| l.order == egui::Order::Foreground).filter_map(|l| m.area_rect(l.id)).collect()
    });
    v.sort_by(|a, b| a.min.x.total_cmp(&b.min.x));
    v
}

/// The middle of entry `row` (counted before any separator) of a menu.
fn menu_row(h: &Harness, menu: egui::Rect, row: usize) -> egui::Pos2 {
    let s = h.ctx.style();
    let pitch = s.spacing.interact_size.y + s.spacing.item_spacing.y;
    egui::pos2(menu.min.x + 40.0, menu.min.y + s.spacing.menu_margin.top as f32 + row as f32 * pitch + s.spacing.interact_size.y * 0.5)
}

fn hover(h: &mut Harness, p: egui::Pos2) {
    h.run(vec![egui::Event::PointerMoved(p)]);
    h.frames(2);
}

#[test]
fn pov_label_menu_runs_an_item_and_closes() {
    let mut h = Harness::new();
    maximized(&mut h);
    assert!(menus(&h).is_empty());
    h.click(label_rect(&h, 1).center());
    let open = menus(&h);
    assert_eq!(open.len(), 1, "clicking the POV label opens its menu");
    // Perspective, Orthographic, Front, Back, Top.
    h.click(menu_row(&h, open[0], 4));
    assert_eq!(h.app.state.active().kind, loom_commands::ViewKind::Top, "the item runs");
    assert!(menus(&h).is_empty(), "and closes the menu");
}

#[test]
fn label_menus_close_on_a_click_elsewhere_and_on_escape() {
    let mut h = Harness::new();
    maximized(&mut h);
    let centre = h.app.viewport_rects[h.app.state.active_viewport].center();
    for which in 0..3 {
        let l = label_rect(&h, which).center();
        h.click(l);
        assert_eq!(menus(&h).len(), 1, "label {which} opens");
        h.click(centre);
        assert!(menus(&h).is_empty(), "label {which}: a click elsewhere closes it");
        h.click(label_rect(&h, which).center());
        assert_eq!(menus(&h).len(), 1);
        h.key(egui::Key::Escape);
        assert!(menus(&h).is_empty(), "label {which}: Escape closes it");
        h.click(label_rect(&h, which).center());
        assert_eq!(menus(&h).len(), 1);
        h.click(label_rect(&h, which).center());
        assert!(menus(&h).is_empty(), "label {which}: clicking the label again closes it");
    }
}

#[test]
fn label_right_click_menus_run_items_and_close() {
    let mut h = Harness::new();
    maximized(&mut h);
    let p = label_rect(&h, 2).center();
    right_click(&mut h, p);
    let open = menus(&h);
    assert_eq!(open.len(), 1, "right-clicking the shading label opens its menu");
    assert!(h.app.quad_menu.is_none(), "not the quad menu");
    // Smooth + Highlights, Wireframe.
    h.click(menu_row(&h, open[0], 1));
    assert_eq!(h.app.state.active().shading, loom_commands::Shading::Wireframe, "the item runs");
    assert!(menus(&h).is_empty(), "and closes the menu");
    let centre = h.app.viewport_rects[h.app.state.active_viewport].center();
    let p = label_rect(&h, 1).center();
    right_click(&mut h, p);
    assert_eq!(menus(&h).len(), 1);
    h.click(centre);
    assert!(menus(&h).is_empty(), "a click elsewhere closes it");
    let p = label_rect(&h, 1).center();
    right_click(&mut h, p);
    assert_eq!(menus(&h).len(), 1);
    h.key(egui::Key::Escape);
    assert!(menus(&h).is_empty(), "Escape closes it");
}

#[test]
fn label_submenus_open_to_the_right() {
    let mut h = Harness::new();
    let r = h.app.dispatch(Command::CreateCameraAt { target: false, position: [0.0, -100.0, 20.0], look_at: [0.0, 0.0, 20.0] }).unwrap();
    let cam = loom_scene::NodeId(r["id"].as_u64().unwrap());
    maximized(&mut h);
    // (label, submenu row): POV > Cameras, [+] > Active Viewport, shading > Other.
    for (which, row) in [(1, 0), (0, 1), (2, 3)] {
        h.click(label_rect(&h, which).center());
        let parent = menus(&h)[0];
        let p = menu_row(&h, parent, row);
        hover(&mut h, p);
        let open = menus(&h);
        assert_eq!(open.len(), 2, "label {which}: hovering the submenu entry opens it");
        assert!(open[1].min.x >= parent.max.x - 1.0, "label {which}: the submenu {:?} is right of its menu {parent:?}", open[1]);
        h.key(egui::Key::Escape);
        assert!(menus(&h).is_empty(), "label {which}: Escape closes menu and submenu");
    }
    // Picking a camera in the submenu sets the view and closes both menus.
    h.click(label_rect(&h, 1).center());
    let parent = menus(&h)[0];
    let p = menu_row(&h, parent, 0);
    hover(&mut h, p);
    let sub = menus(&h)[1];
    let p = menu_row(&h, sub, 0);
    hover(&mut h, egui::pos2(sub.min.x + 5.0, p.y));
    h.click(p);
    assert_eq!(h.app.state.active().kind, loom_commands::ViewKind::Camera(cam));
    assert!(menus(&h).is_empty(), "picking a camera closes the menus");
}

#[test]
fn quad_menu_closes_on_an_item_a_click_anywhere_and_escape_and_reopens_on_a_new_right_click() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [30.0; 3] });
    let id = *h.app.state.selection.iter().next().unwrap();
    maximized(&mut h);
    let r = h.app.viewport_rects[h.app.state.active_viewport];
    let at = r.center() + egui::vec2(-150.0, 50.0);
    right_click(&mut h, at);
    assert!(h.app.quad_menu.is_some(), "right-click opens the quad menu");
    // The display quadrant sits up and right of the click, Hide Selection at its bottom.
    let display = menus(&h).into_iter().find(|m| m.min.x > at.x && m.max.y < at.y).expect("the display quadrant");
    h.click(egui::pos2(display.center().x, display.max.y - h.ctx.style().spacing.menu_margin.bottom as f32 - 10.0));
    assert!(h.app.state.scene.node(id).unwrap().hidden, "the item runs");
    assert!(h.app.quad_menu.is_none() && menus(&h).is_empty(), "and closes the menu");
    // In the viewport, on the Command Panel, on the empty end of the menu bar.
    let elsewhere = [r.center() + egui::vec2(250.0, -200.0), egui::pos2(SCREEN.max.x - 60.0, 300.0), egui::pos2(SCREEN.max.x - 30.0, 10.0)];
    for p in elsewhere {
        h.app.create_kind = None; // the Command Panel click may have picked a Create button
        right_click(&mut h, at);
        assert!(h.app.quad_menu.is_some());
        h.click(p);
        assert!(h.app.quad_menu.is_none() && menus(&h).is_empty(), "a click at {p:?} closes it ({:?} {:?})", h.app.quad_menu, menus(&h));
    }
    right_click(&mut h, at);
    h.key(egui::Key::Escape);
    assert!(h.app.quad_menu.is_none() && menus(&h).is_empty(), "Escape closes it");
    right_click(&mut h, at);
    let other = r.center() + egui::vec2(200.0, 150.0);
    right_click(&mut h, other);
    let open = h.app.quad_menu.expect("a right-click elsewhere reopens it");
    assert_eq!(open.at, other, "at the new spot");
    h.frames(2);
    assert!(h.app.quad_menu.is_some(), "and it stays open");
}

/// The rect of a side or top/bottom panel, by its id.
fn panel(h: &Harness, id: &str) -> egui::Rect {
    egui::containers::panel::PanelState::load(&h.ctx, egui::Id::new(id)).expect("panel shown").rect
}

/// Right-clicks over the points until one opens a menu or popup (not the quad
/// menu); Escape between tries. Returns that point. `accept` filters the menu by its rect.
fn find_context_menu(h: &mut Harness, xs: std::ops::Range<i32>, ys: std::ops::Range<i32>, step: usize, accept: impl Fn(egui::Rect) -> bool) -> egui::Pos2 {
    for y in ys.clone().step_by(step) {
        for x in xs.clone().step_by(step) {
            let p = egui::pos2(x as f32, y as f32);
            right_click(h, p);
            if h.app.quad_menu.is_none() && menus(h).len() == 1 && accept(menus(h)[0]) {
                return p;
            }
            h.key(egui::Key::Escape);
            h.app.quad_menu = None;
        }
    }
    panic!("no context menu in x {xs:?}, y {ys:?}");
}

/// The menu opened by a right-click at `p` closes on Escape, a click on an empty
/// spot, a right-click on an empty spot and (last, it may deselect) a click in the viewport.
fn assert_context_menu_closes(h: &mut Harness, p: egui::Pos2, what: &str) {
    let empty = egui::pos2(SCREEN.max.x - 30.0, 10.0);
    let open = |h: &mut Harness| {
        right_click(h, p);
        assert_eq!(menus(h).len(), 1, "{what}: right-click opens it");
    };
    open(h);
    h.frames(3);
    assert_eq!(menus(h).len(), 1, "{what}: it stays open");
    h.key(egui::Key::Escape);
    assert!(menus(h).is_empty(), "{what}: Escape closes it");
    open(h);
    h.click(empty);
    assert!(menus(h).is_empty(), "{what}: a click elsewhere closes it");
    open(h);
    right_click(h, empty);
    assert!(menus(h).is_empty(), "{what}: a right-click elsewhere closes it");
    open(h);
    let v = h.app.viewport_rects[h.app.state.active_viewport];
    h.click(v.center() + egui::vec2(v.width() * 0.3, v.height() * 0.3));
    assert!(menus(h).is_empty() && h.app.quad_menu.is_none(), "{what}: a click in the viewport closes it");
}

#[test]
fn modifier_stack_right_click_menus_close() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [30.0; 3] });
    let id = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::AddModifier { name: "Bend".into() });
    h.app.panel_tab = crate::Tab::Modify;
    h.frames(2);
    let cp = panel(&h, "command_panel");
    let bend = find_context_menu(&mut h, cp.min.x as i32..cp.max.x as i32, cp.min.y as i32..cp.max.y as i32, 6, |_| true);
    // Rename, Delete, Off.
    let m = menus(&h)[0];
    h.click(menu_row(&h, m, 2));
    assert!(!h.app.state.scene.node(id).unwrap().modifiers[0].enabled, "Off runs");
    assert!(menus(&h).is_empty(), "and closes the menu");
    let base = find_context_menu(&mut h, bend.x as i32..bend.x as i32 + 1, bend.y as i32..cp.max.y as i32, 3, |r| (r.height() - m.height()).abs() > 1.0);
    let m = menus(&h)[0];
    let row = menu_row(&h, m, 0);
    hover(&mut h, row);
    let open = menus(&h);
    assert_eq!(open.len(), 2, "Convert To opens");
    assert!(open[1].min.x >= m.max.x - 1.0, "to the right: {:?} vs {m:?}", open[1]);
    h.key(egui::Key::Escape);
    assert!(menus(&h).is_empty());
    // The base object's menu last: the viewport click deselects the box.
    assert_context_menu_closes(&mut h, bend, "modifier entry");
    h.cmd(Command::SelectNodes { ids: vec![id.0], op: loom_commands::SelectOp::Replace });
    assert_context_menu_closes(&mut h, base, "base object");
}

#[test]
fn time_slider_right_click_menu_closes() {
    let mut h = Harness::new();
    maximized(&mut h);
    let cx = SCREEN.center().x as i32;
    let p = find_context_menu(&mut h, cx..cx + 1, SCREEN.max.y as i32 - 140..SCREEN.max.y as i32, 3, |_| true);
    // Key Properties (a submenu), then Delete Key below a separator.
    let m = menus(&h)[0];
    let s = h.ctx.style();
    let delete = menu_row(&h, m, 1) + egui::vec2(0.0, 6.0 + s.spacing.item_spacing.y);
    h.click(delete);
    assert!(menus(&h).is_empty(), "Delete Key closes the menu");
    assert_context_menu_closes(&mut h, p, "track bar");
}

#[test]
fn track_view_right_click_menu_closes() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [30.0; 3] });
    h.app.state.track_view.open = true;
    h.frames(3);
    let win = h.ctx.memory(|m| m.areas().visible_layer_ids().into_iter().filter(|l| l.order == egui::Order::Middle).filter_map(|l| m.area_rect(l.id)).next()).expect("the Track View window");
    let (x, y) = (win.min.x as i32, win.min.y as i32);
    let p = find_context_menu(&mut h, x + 10..x + 200, y + 30..win.max.y as i32, 6, |_| true);
    // Assign Controller..., Copy.
    let m = menus(&h)[0];
    h.click(menu_row(&h, m, 1));
    assert!(menus(&h).is_empty(), "Copy closes the menu");
    assert_context_menu_closes(&mut h, p, "Track View controller list");
}

#[test]
fn uv_editor_right_click_menu_stays_open_and_closes() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [30.0; 3] });
    h.cmd(Command::AddModifier { name: "Unwrap UVW".into() });
    h.cmd(Command::EditUvws);
    h.frames(3);
    let win = h.ctx.memory(|m| m.areas().visible_layer_ids().into_iter().filter(|l| l.order == egui::Order::Middle).filter_map(|l| m.area_rect(l.id)).next()).expect("the Edit UVWs window");
    let p = win.center();
    right_click(&mut h, p);
    h.frames(3);
    let open = menus(&h);
    assert_eq!(open.len(), 1, "right-click on the UV canvas opens its menu and it stays open");
    // Break.
    h.click(menu_row(&h, open[0], 0));
    assert!(menus(&h).is_empty(), "Break closes the menu");
    assert_context_menu_closes(&mut h, p, "UV canvas");
}

#[test]
fn status_bar_flyouts_close() {
    let mut h = Harness::new();
    maximized(&mut h);
    let bar = panel(&h, "statusbar");
    // The navigation block at the right end holds the flyouts (below the time slider edge).
    let p = find_context_menu(&mut h, bar.max.x as i32 - 300..bar.max.x as i32, bar.min.y as i32 + 6..bar.max.y as i32, 4, |_| true);
    let m = menus(&h)[0];
    h.click(menu_row(&h, m, 0));
    assert!(menus(&h).is_empty(), "an item closes the flyout");
    assert_context_menu_closes(&mut h, p, "status bar flyout");
}

/// Expert Mode leaves a Cancel Expert Mode button in the bottom-right corner;
/// clicking it brings the interface back.
#[test]
fn cancel_expert_mode_button_brings_the_interface_back() {
    let mut h = Harness::new();
    h.frames(2);
    h.cmd(Command::ExpertModeToggle);
    h.frames(2);
    assert!(h.app.state.expert_mode);
    let screen = h.ctx.screen_rect();
    click_text(&mut h, "Cancel Expert Mode", screen, 0.0);
    assert!(!h.app.state.expert_mode, "the button ends Expert Mode");
}
