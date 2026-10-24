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
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::MaterialEditor));
    h.cmd(Command::SetMaterialColor { slot: None, which: "Diffuse".into(), rgb: [255, 0, 0] });
    h.cmd(Command::SetMaterialParameter { slot: None, name: "Specular Level".into(), value: 80.0 });
    h.cmd(Command::AssignMaterialToSelection);
    let style = h.app.world_styles[0];
    assert!(style.color[0] > 0.9 && style.color[1] < 0.05, "faces take the diffuse colour: {:?}", style.color);
    assert!(style.specular[3] > 0.0, "and the highlight");
    assert!(style.wire_color.is_some(), "edges keep the object colour");
    h.key(egui::Key::M);
    assert_eq!(h.app.state.dialog, None, "M again closes it");
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
