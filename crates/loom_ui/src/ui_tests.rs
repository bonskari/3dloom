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

    /// Every text `label` painted last frame in the Command Panel, top to bottom.
    fn widgets_labelled(&self, label: &str) -> Vec<egui::Rect> {
        let panel = egui::containers::panel::PanelState::load(&self.ctx, egui::Id::new("command_panel")).expect("the Command Panel").rect;
        self.texts(label).into_iter().filter(|r| panel.contains_rect(*r)).collect()
    }

    /// Every text `label` painted last frame, top to bottom (its rect).
    fn texts(&self, label: &str) -> Vec<egui::Rect> {
        fn find(shape: &egui::Shape, text: &str, out: &mut Vec<egui::Rect>) {
            match shape {
                egui::Shape::Vec(v) => v.iter().for_each(|s| find(s, text, out)),
                egui::Shape::Text(t) if t.galley.text() == text => out.push(t.galley.rect.translate(t.pos.to_vec2())),
                _ => {}
            }
        }
        let mut v = Vec::new();
        for c in &self.shapes {
            let mut found = Vec::new();
            find(&c.shape, label, &mut found);
            v.extend(found.into_iter().filter(|r| c.clip_rect.contains_rect(*r)));
        }
        v.sort_by(|a, b| a.min.y.total_cmp(&b.min.y));
        v
    }

    /// The one text `label` painted last frame.
    fn widget(&self, label: &str) -> egui::Rect {
        let v = self.widgets_labelled(label);
        assert_eq!(v.len(), 1, "one {label:?} shown: {v:?}");
        v[0]
    }

    /// Clicks the text `label` in the Command Panel.
    fn click_widget(&mut self, label: &str) {
        let r = self.widget(label);
        self.click(r.center());
    }

    /// Clicks the one text `label` shown anywhere (a dialog's button).
    fn click_text_anywhere(&mut self, label: &str) {
        let v = self.texts(label);
        assert_eq!(v.len(), 1, "one {label:?} shown: {v:?}");
        self.click(v[0].center());
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

/// A 10-unit box at `x` with a Taper of Amount 1: the stack result's top is
/// twice as wide as the box.
fn tapered_box(h: &mut Harness, x: f32) -> loom_scene::NodeId {
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [x, 0.0, 0.0], size: [10.0; 3] });
    let id = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::AddModifier { name: "Taper".into() });
    h.cmd(Command::SetModifierParameter { index: 0, name: "Amount".into(), value: 1.0 });
    id
}

/// The X extent of what the viewports draw for `id`.
fn drawn_width(h: &Harness, id: loom_scene::NodeId) -> f32 {
    let i = h.app.world_meshes().iter().position(|(n, _)| *n == id).expect("drawn");
    let (lo, hi) = h.app.world_display[i].aabb();
    hi.x - lo.x
}

#[test]
fn show_end_result_starts_off_and_only_the_edited_object_shows_up_to_its_active_entry() {
    let mut h = Harness::new();
    let b = tapered_box(&mut h, 40.0);
    let a = tapered_box(&mut h, 0.0);
    h.click_widget("Modify");
    assert!(!h.app.state.show_end_result, "off by default");
    assert!((drawn_width(&h, a) - 20.0).abs() < 1e-3, "the top entry is active: the whole stack shows");
    // The base object in the stack: A shows its box, B (not being edited) its end result.
    h.click_widget("Box");
    assert_eq!(h.app.state.scene.node(a).unwrap().active_entry, None);
    assert!((drawn_width(&h, a) - 10.0).abs() < 1e-3, "{}", drawn_width(&h, a));
    assert!((drawn_width(&h, b) - 20.0).abs() < 1e-3, "{}", drawn_width(&h, b));
    // Leaving the Modify panel shows the end result again.
    h.click_widget("Create");
    assert!((drawn_width(&h, a) - 20.0).abs() < 1e-3);
    h.click_widget("Modify");
    assert!((drawn_width(&h, a) - 10.0).abs() < 1e-3);
    // Show End Result on: the whole stack.
    h.click_widget("⏚");
    assert!(h.app.state.show_end_result);
    assert!((drawn_width(&h, a) - 20.0).abs() < 1e-3);
}

#[test]
fn smooth_modifier_has_32_smoothing_group_buttons() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let id = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::AddModifier { name: "Smooth".into() });
    h.click_widget("Modify");
    let groups = |h: &Harness| match &h.app.state.scene.node(id).unwrap().modifiers[0].modifier {
        loom_scene::Modifier::Smooth { groups, .. } => groups.get(),
        _ => panic!("Smooth"),
    };
    for i in 1..=32 {
        assert_eq!(h.widgets_labelled(&i.to_string()).len(), 1, "group button {i}");
    }
    h.click_widget("3");
    assert_eq!(groups(&h), 1 << 2);
    h.click_widget("32");
    assert_eq!(groups(&h), (1 << 2) | (1 << 31));
    h.click_widget("3");
    assert_eq!(groups(&h), 1 << 31, "a second click clears the group");
    assert!(h.app.state.scene.eval_mesh(id).value.mesh.faces().iter().all(|f| f.smoothing_groups == 1 << 31));
    assert_eq!(h.widgets_labelled("Prevent Indirect Smoothing").len(), 1);
}

/// Dragging an entry onto another row moves it; Shift-drag copies it there,
/// Ctrl-drag puts an instance there.
#[test]
fn shift_and_ctrl_drag_in_the_stack_copy_and_instance_a_modifier() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let id = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::AddModifier { name: "Bend".into() });
    h.cmd(Command::AddModifier { name: "Taper".into() });
    h.click_widget("Modify");
    let names = |h: &Harness| h.app.state.scene.node(id).unwrap().modifiers.iter().map(|m| m.name.clone()).collect::<Vec<_>>();
    let instances = |h: &Harness| h.app.state.scene.node(id).unwrap().modifiers.iter().map(|m| m.instance_of.map(|(_, i)| i)).collect::<Vec<_>>();
    // Shift-drag Taper down onto Bend: a copy below Bend.
    let (taper, bend) = (h.widget("Taper"), h.widget("Bend"));
    h.mods = egui::Modifiers::SHIFT;
    h.drag(taper.center(), bend.center());
    h.mods = egui::Modifiers::NONE;
    assert_eq!(names(&h), ["Taper", "Bend", "Taper"]);
    assert_eq!(instances(&h), [None, None, None]);
    // Ctrl-drag Bend up onto the top Taper: an instance above it.
    let bend = h.widget("Bend");
    let top = h.widgets_labelled("Taper")[0];
    h.mods = egui::Modifiers::CTRL;
    h.drag(bend.center(), top.center());
    h.mods = egui::Modifiers::NONE;
    assert_eq!(names(&h), ["Taper", "Bend", "Taper", "Bend"]);
    assert_eq!(instances(&h), [None, None, None, Some(1)]);
    // A plain drag moves: the lower Taper (bottom row) onto the lower Bend.
    let rows = h.widgets_labelled("Taper");
    let bends = h.widgets_labelled("Bend");
    assert!(rows.len() == 2 && bends.len() == 2, "{rows:?} {bends:?}");
    h.drag(rows[1].center(), bends[1].center());
    assert_eq!(names(&h), ["Bend", "Taper", "Taper", "Bend"]);
    assert_eq!(instances(&h), [None, None, None, Some(0)], "the instance follows its source");
}

/// Changing a base parameter under a topology-dependent modifier asks first:
/// Yes / Hold/Yes / No.
#[test]
fn base_parameter_change_under_edit_poly_asks_yes_hold_yes_no() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let id = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::AddModifier { name: "Edit Poly".into() });
    h.click_widget("Modify");
    h.click_widget("Box");
    let height = |h: &Harness| h.app.state.scene.node(id).unwrap().object.parameters().into_iter().find(|(n, _)| *n == "Height").unwrap().1;
    // The spinner's edit goes through dispatch, as a drag does.
    let _ = h.app.dispatch(Command::SetParameter { id: id.0, name: "Height".into(), value: 20.0 });
    h.frames(2);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::TopologyWarning));
    assert_eq!(height(&h), loom_scene::ParamValue::Float(10.0), "nothing changes before the answer");
    assert_eq!(h.texts("Hold/Yes").len(), 1);
    h.click_text_anywhere("No");
    assert_eq!(h.app.state.dialog, None);
    assert_eq!(height(&h), loom_scene::ParamValue::Float(10.0), "No keeps the old value");
    let _ = h.app.dispatch(Command::SetParameter { id: id.0, name: "Height".into(), value: 20.0 });
    h.frames(2);
    h.click_text_anywhere("Yes");
    assert_eq!(height(&h), loom_scene::ParamValue::Float(20.0), "Yes applies it");
    // Answered once for this visit to the base object: the next change applies directly.
    let _ = h.app.dispatch(Command::SetParameter { id: id.0, name: "Height".into(), value: 25.0 });
    h.frames(2);
    assert_eq!(h.app.state.dialog, None);
    assert_eq!(height(&h), loom_scene::ParamValue::Float(25.0));
    // A plain object without such a modifier never asks.
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [30.0, 0.0, 0.0], size: [10.0; 3] });
    let other = *h.app.state.selection.iter().next().unwrap();
    let _ = h.app.dispatch(Command::SetParameter { id: other.0, name: "Height".into(), value: 5.0 });
    assert_eq!(h.app.state.dialog, None);
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
