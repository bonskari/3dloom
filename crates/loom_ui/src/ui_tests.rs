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
    /// The last frame's painted shapes, for finding widgets by their text.
    shapes: Vec<egui::epaint::ClippedShape>,
}

impl Harness {
    fn new() -> Self {
        let mut h = Harness { ctx: egui::Context::default(), app: LoomApp::headless(), time: 0.0, mods: egui::Modifiers::NONE, shapes: Vec::new() };
        h.frames(3);
        h
    }

    fn run(&mut self, events: Vec<egui::Event>) {
        self.time += 1.0 / 30.0;
        let input = egui::RawInput { screen_rect: Some(SCREEN), time: Some(self.time), modifiers: self.mods, events, ..Default::default() };
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
