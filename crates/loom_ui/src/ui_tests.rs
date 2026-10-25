//! UI regression tests: the whole app runs frame by frame without a window and
//! receives real pointer and key events, so viewport picking, the level keys
//! and the panels are tested the way a person uses them.

use crate::LoomApp;
use eframe::egui;
use glam::Vec3;
use loom_commands::{Command, PrimitiveKind, SelectionMode};

mod icons;
mod transforms;
mod menus;
mod display;
mod mesh_patch;
mod widgets;
mod ribbon;
mod precision;
mod explorer;
mod bake;

const SCREEN: egui::Rect = egui::Rect { min: egui::pos2(0.0, 0.0), max: egui::pos2(1280.0, 800.0) };

struct Harness {
    ctx: egui::Context,
    app: LoomApp,
    time: f64,
    mods: egui::Modifiers,
    /// The last frame's widgets (role, text, rect), as a screen reader sees them.
    widgets: Vec<(egui::accesskit::Role, String, egui::Rect)>,
    /// The last frame's shapes, to find a widget by its text.
    shapes: Vec<egui::epaint::ClippedShape>,
    /// The window size (taller for panels with long rollouts).
    screen: egui::Rect,
    /// Every output command so far (clipboard copies).
    commands: Vec<egui::OutputCommand>,
    /// Every texture upload so far.
    textures: Vec<(egui::TextureId, egui::epaint::ImageDelta)>,
    /// Seconds between frames (a loaded machine draws fewer per second).
    frame_time: f64,
}

impl Harness {
    fn new() -> Self {
        let mut h = Harness { ctx: egui::Context::default(), app: LoomApp::headless(), time: 0.0, mods: egui::Modifiers::NONE, widgets: Vec::new(), shapes: Vec::new(), screen: SCREEN, commands: Vec::new(), textures: Vec::new(), frame_time: 1.0 / 30.0 };
        h.ctx.enable_accesskit();
        h.frames(3);
        h
    }

    /// The app on a real GPU (Realistic viewports and Render run), or None
    /// on a machine without a Vulkan / Metal / DirectX 12 adapter.
    fn with_gpu() -> Option<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let rs = pollster::block_on(egui_wgpu::RenderState::create(&crate::wgpu_configuration(), &instance, None, None, 1, false)).ok()?;
        if !loom_bevy::supported(&rs.adapter) {
            return None;
        }
        let mut h = Harness::new();
        h.app.gpu = crate::realistic::Gpu::Ready(rs);
        Some(h)
    }

    fn run(&mut self, events: Vec<egui::Event>) {
        self.time += self.frame_time;
        let mut input = egui::RawInput { screen_rect: Some(self.screen), time: Some(self.time), modifiers: self.mods, events, ..Default::default() };
        // As eframe does before each frame.
        self.app.raw_input_hook(&self.ctx, &mut input);
        let app = &mut self.app;
        let out = self.ctx.run(input, |ctx| app.frame(ctx));
        self.commands.extend(out.platform_output.commands);
        self.textures.extend(out.textures_delta.set);
        self.shapes = out.shapes;
        if let Some(tree) = out.platform_output.accesskit_update {
            self.widgets = tree
                .nodes
                .iter()
                .filter_map(|(_, n)| {
                    let b = n.bounds()?;
                    let text = n.label().or(n.value()).unwrap_or_default().to_owned();
                    Some((n.role(), text, egui::Rect::from_min_max(egui::pos2(b.x0 as f32, b.y0 as f32), egui::pos2(b.x1 as f32, b.y1 as f32))))
                })
                .collect();
        }
    }

    /// Every text `label` painted last frame in the Command Panel, top to bottom.
    fn widgets_labelled(&self, label: &str) -> Vec<egui::Rect> {
        let panel = egui::containers::panel::PanelState::load(&self.ctx, egui::Id::new("command_panel")).expect("the Command Panel").rect;
        self.text_rects(label).into_iter().filter(|r| panel.contains_rect(*r)).collect()
    }

    /// Every text `label` painted last frame, top to bottom (its rect).
    fn text_rects(&self, label: &str) -> Vec<egui::Rect> {
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
        let v = self.text_rects(label);
        assert_eq!(v.len(), 1, "one {label:?} shown: {v:?}");
        self.click(v[0].center());
    }

    /// The rects of the widgets with this role whose text is `text`, top to bottom.
    fn find_all(&self, role: egui::accesskit::Role, text: &str) -> Vec<egui::Rect> {
        let mut v: Vec<egui::Rect> = self.widgets.iter().filter(|(r, t, _)| *r == role && t.trim() == text).map(|w| w.2).collect();
        v.sort_by(|a, b| a.min.y.total_cmp(&b.min.y).then(a.min.x.total_cmp(&b.min.x)));
        v
    }

    /// The topmost widget with this role whose text is `text`.
    fn find(&self, role: egui::accesskit::Role, text: &str) -> egui::Rect {
        *self.find_all(role, text).first().unwrap_or_else(|| panic!("no {role:?} \"{text}\" on screen"))
    }

    fn has(&self, role: egui::accesskit::Role, text: &str) -> bool {
        self.widgets.iter().any(|(r, t, _)| *r == role && t.trim() == text)
    }

    /// The spinner right of the label `label` on the same row.
    fn spinner_after(&self, label: &str) -> egui::Rect {
        let l = self.find(egui::accesskit::Role::Label, label);
        self.widgets
            .iter()
            .filter(|(r, _, w)| *r == egui::accesskit::Role::SpinButton && w.min.x >= l.max.x - 1.0 && (w.center().y - l.center().y).abs() < l.height())
            .min_by(|a, b| a.2.min.x.total_cmp(&b.2.min.x))
            .map(|w| w.2)
            .unwrap_or_else(|| panic!("no spinner after \"{label}\""))
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
    fn text_centers(&self, text: &str) -> Vec<egui::Pos2> {
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
    keep_creating(&mut h, true);
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
    keep_creating(&mut h, true);
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
    let style = h.app.world_styles[0].clone();
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

/// Sets Keep Creating (tests do not lean on its default).
fn keep_creating(h: &mut Harness, on: bool) {
    h.cmd(Command::SetPreference { name: "Keep Creating".into(), value: if on { 1.0 } else { 0.0 } });
}

/// Drags out a box (base, then height and a click), `dx` pixels right of the
/// viewport centre.
fn drag_a_box(h: &mut Harness, dx: f32) {
    let r = h.app.viewport_rects[h.app.state.active_viewport];
    let a = r.center() + egui::vec2(dx - 60.0, 60.0);
    let b = r.center() + egui::vec2(dx + 60.0, 140.0);
    h.drag(a, b);
    let c = b + egui::vec2(0.0, -60.0);
    hover_to(h, b, c);
    h.click(c);
}

/// Keep Creating off (the default): one box, then the Select tool with the
/// box selected and its parameters in the Modify panel; a second drag selects.
#[test]
fn a_box_drag_ends_the_create_mode_when_keep_creating_is_off() {
    let mut h = Harness::new();
    maximized(&mut h);
    keep_creating(&mut h, false);
    h.click_widget("Box");
    assert_eq!(h.app.create_kind, Some(crate::CreateKind::Primitive(PrimitiveKind::Box)));
    drag_a_box(&mut h, -100.0);
    assert_eq!(h.app.state.scene.len(), 1);
    let id = *h.app.state.selection.iter().next().expect("the new box is selected");
    assert!(h.app.create_kind.is_none() && h.app.create.is_none(), "the create mode ended");
    assert_eq!(h.app.state.active_tool, loom_commands::Tool::Select);
    assert_eq!(h.app.panel_tab, crate::Tab::Modify, "the Modify panel shows the new box");
    h.frames(2);
    assert_eq!(h.widgets_labelled("Parameters").len(), 1, "its Parameters rollout is shown");
    drag_a_box(&mut h, 120.0);
    assert_eq!(h.app.state.scene.len(), 1, "a second drag makes no box");
    assert!(h.app.state.selection.is_empty() || h.app.state.selection.contains(&id), "it selects instead");
}

/// Keep Creating on: the Box tool stays on and the next drag makes another box.
#[test]
fn a_box_drag_keeps_the_tool_on_when_keep_creating_is_on() {
    let mut h = Harness::new();
    maximized(&mut h);
    keep_creating(&mut h, true);
    h.click_widget("Box");
    drag_a_box(&mut h, -100.0);
    assert_eq!(h.app.create_kind, Some(crate::CreateKind::Primitive(PrimitiveKind::Box)), "still creating boxes");
    assert_eq!(h.app.panel_tab, crate::Tab::Create);
    drag_a_box(&mut h, 120.0);
    assert_eq!(h.app.state.scene.len(), 2, "the second drag made a second box");
}

/// The Pin in every Object Type rollout toggles Keep Creating.
#[test]
fn the_object_type_pin_toggles_keep_creating_in_every_category() {
    let mut h = Harness::new();
    let mut on = loom_commands::state::KEEP_CREATING_DEFAULT;
    for category in ["Geometry", "Shapes", "Lights", "Cameras", "Helpers", "Systems"] {
        h.click(h.find(egui::accesskit::Role::Button, category).center());
        h.click(h.find(egui::accesskit::Role::Button, crate::command_panel::create::PIN_TIP).center());
        on = !on;
        assert_eq!(h.app.state.prefs.keep_creating, on, "{category}: the Pin switched Keep Creating");
    }
}

/// Multi-step creations end the mode only when the object is done; the Line
/// tool keeps adding knots until its right-click.
#[test]
fn cylinder_and_line_end_the_create_mode_only_when_finished() {
    let mut h = Harness::new();
    keep_creating(&mut h, false);
    creating(&mut h, PrimitiveKind::Cylinder);
    let a = ground_point(&h, Vec3::ZERO);
    let b = ground_point(&h, Vec3::new(3.0, 0.0, 0.0));
    h.drag(a, b);
    assert!(h.app.create.is_some() && h.app.create_kind.is_some(), "the height step is still running");
    let c = b + egui::vec2(0.0, -60.0);
    hover_to(&mut h, b, c);
    h.click(c);
    assert!(h.app.create_kind.is_none(), "the height click finished the cylinder and the mode");

    let mut h = Harness::new();
    keep_creating(&mut h, false);
    wide_view(&mut h);
    h.app.start_create(Some(crate::CreateKind::Line));
    h.frames(1);
    for p in [Vec3::ZERO, Vec3::new(40.0, 0.0, 0.0), Vec3::new(40.0, 40.0, 0.0)] {
        let s = ground_point(&h, p);
        h.click(s);
        assert_eq!(h.app.create_kind, Some(crate::CreateKind::Line), "the Line tool keeps adding knots");
    }
    let s = ground_point(&h, Vec3::new(-20.0, 30.0, 0.0));
    right_click(&mut h, s);
    assert!(h.app.line.is_none() && h.app.create_kind.is_none(), "the right-click ends the line and the mode");
    assert_eq!(h.app.state.scene.len(), 1);
    assert!(h.app.quad_menu.is_none());
}

/// The Keyboard Entry Create button, a Target camera drag, a Dummy and an Omni click
/// each make one object and end the mode.
#[test]
fn keyboard_entry_camera_and_helper_creation_end_the_create_mode() {
    let mut h = Harness::new();
    keep_creating(&mut h, false);
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 1400.0));
    h.click_widget("Box");
    h.click_widget("Keyboard Entry");
    let button = *h.widgets_labelled("Create").last().expect("the Keyboard Entry Create button");
    h.click(button.center());
    assert_eq!(h.app.state.scene.len(), 1);
    assert!(h.app.create_kind.is_none(), "Keyboard Entry Create ends the mode");

    let mut h = Harness::new();
    keep_creating(&mut h, false);
    wide_view(&mut h);
    h.app.start_create(Some(crate::CreateKind::Camera { target: true }));
    let (a, b) = (h.screen_of(Vec3::new(-40.0, -40.0, 0.0)), h.screen_of(Vec3::new(30.0, 20.0, 0.0)));
    h.drag(a, b);
    assert!(h.app.create_kind.is_none(), "the target drag ends the mode");

    let mut h = Harness::new();
    keep_creating(&mut h, false);
    wide_view(&mut h);
    h.app.start_create(Some(crate::CreateKind::Helper(crate::HelperKind::Dummy)));
    let p = h.screen_of(Vec3::new(10.0, 10.0, 0.0));
    h.click(p);
    assert_eq!(h.app.state.scene.len(), 1);
    assert!(h.app.create_kind.is_none(), "a Dummy click ends the mode");

    let mut h = Harness::new();
    keep_creating(&mut h, false);
    wide_view(&mut h);
    h.app.start_create(Some(crate::CreateKind::Light(loom_commands::light::LightKind::Omni)));
    let p = h.screen_of(Vec3::new(20.0, -10.0, 0.0));
    h.click(p);
    assert_eq!(h.app.state.scene.len(), 1);
    assert!(h.app.create_kind.is_none(), "an Omni click ends the mode");
}

/// Create menu > Keep Creating and Preferences > General switch it too.
#[test]
fn keep_creating_in_the_create_menu_and_preferences() {
    let mut h = Harness::new();
    keep_creating(&mut h, false);
    h.click(h.find(egui::accesskit::Role::Button, "Create").center());
    h.click(h.find(egui::accesskit::Role::CheckBox, "Keep Creating").center());
    assert!(h.app.state.prefs.keep_creating);
    h.cmd(Command::Preferences);
    h.click(h.find(egui::accesskit::Role::CheckBox, "Keep Creating").center());
    assert!(!h.app.state.prefs.keep_creating);
}

/// The maximized Perspective view pulled back so a 200-unit area shows.
fn wide_view(h: &mut Harness) {
    maximized(h);
    h.app.state.active_mut().camera = loom_input::OrbitCamera { target: Vec3::ZERO, distance: 300.0, yaw: 0.75, pitch: 0.6, roll: 0.0 };
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
    keep_creating(&mut h, false);
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
    assert!(h.app.create_kind.is_none(), "one biped, then the Select tool (Keep Creating off)");
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

/// Runs injected input the way the app's control socket feeds it: one step per frame.
fn inject(h: &mut Harness, events: serde_json::Value) {
    let mut q = crate::live::InputQueue::default();
    q.push(crate::live::parse_input(&events).unwrap());
    while !q.is_empty() {
        h.time += h.frame_time;
        let mut input = egui::RawInput { screen_rect: Some(SCREEN), time: Some(h.time), ..Default::default() };
        q.apply(&mut input);
        let app = &mut h.app;
        let _ = h.ctx.run(input, |ctx| app.frame(ctx));
    }
    h.frames(1);
}

#[test]
fn diagnostics_show_a_held_alt_press_until_reset_interaction() {
    let mut h = Harness::new();
    poly_box(&mut h);
    let p = h.screen_of(Vec3::new(0.0, 0.0, 15.0));
    h.mods = egui::Modifiers::ALT;
    h.run(vec![egui::Event::PointerMoved(p)]);
    h.run(vec![egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: h.mods }]);
    h.frames(1);
    let d = h.app.diagnostics(&h.ctx);
    assert_eq!(d["egui"]["input"]["modifiers"]["alt"], true, "{}", d["egui"]);
    assert_eq!(d["egui"]["input"]["pointer"]["buttons_down"], serde_json::json!(["primary"]));
    assert_eq!(d["interaction"]["alt_click"], serde_json::json!([p.x, p.y]));
    assert_eq!(d["interaction"]["nav_mode"], "None");
    assert_eq!(d["viewports"]["active"], h.app.state.active_viewport);
    assert_eq!(d["modes"]["sub_object_level"], "Object");
    let events = d["input_events"].as_array().unwrap();
    assert!(events.iter().any(|e| e["event"].as_str().unwrap().starts_with("PointerButton")), "{events:?}");
    assert!(events.iter().any(|e| e["event"] == "Modifiers(alt)"), "{events:?}");
    // The release never arrives (focus lost): reset clears what it left behind.
    let cleared = h.app.reset_interaction(&h.ctx);
    assert!(cleared.contains(&"alt_click"), "{cleared:?}");
    assert!(h.app.alt_click.is_none());
    assert_eq!(h.app.diagnostics(&h.ctx)["interaction"]["alt_click"], serde_json::Value::Null);
}

#[test]
fn injected_alt_drag_orbits_like_a_mouse() {
    let mut h = Harness::new();
    poly_box(&mut h);
    let p = h.screen_of(Vec3::new(0.0, 0.0, 15.0));
    let yaw = h.app.state.active().camera.yaw;
    inject(&mut h, serde_json::json!([{ "type": "drag", "from": [p.x, p.y], "to": [p.x + 120.0, p.y], "mods": ["alt"] }]));
    assert_ne!(h.app.state.active().camera.yaw, yaw, "the injected Alt+drag orbits");
    assert_eq!(h.app.state.selection.len(), 1, "and keeps the selection");
    // Held modifiers and keys: Ctrl+A selects all, after a new box.
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [50.0, 0.0, 0.0], size: [10.0; 3] });
    inject(&mut h, serde_json::json!([{ "type": "modifiers", "mods": ["ctrl"] }, { "type": "key", "key": "A", "mods": ["ctrl"] }, { "type": "modifiers", "mods": [] }]));
    assert_eq!(h.app.state.selection.len(), 2, "Ctrl+A selects all");
}

#[test]
fn middle_drag_pans_even_when_frames_come_slowly() {
    // At 8 frames a second egui's smoothed pointer velocity stays zero (it
    // wants three positions within 0.1 s), yet every frame the pointer moved.
    let mut h = Harness::new();
    poly_box(&mut h);
    h.frame_time = 1.0 / 8.0;
    let c = h.app.viewport_rects[h.app.state.active_viewport].center();
    let target = h.app.state.active().camera.target;
    h.drag_with(egui::PointerButton::Middle, c, c + egui::vec2(80.0, 40.0));
    assert_ne!(h.app.state.active().camera.target, target, "a slow mouse drag pans");
    let target = h.app.state.active().camera.target;
    inject(&mut h, serde_json::json!([{ "type": "drag", "button": "middle", "from": [c.x, c.y], "to": [c.x + 80.0, c.y + 40.0] }]));
    assert_ne!(h.app.state.active().camera.target, target, "so does a slow injected drag");
}

/// Eight frames a second: egui's smoothed pointer velocity stays zero, so
/// a drag must follow each frame's pointer move.
const SLOW: f64 = 1.0 / 8.0;

#[test]
fn slow_frames_drag_create_a_box() {
    let mut h = Harness::new();
    maximized(&mut h);
    h.frame_time = SLOW;
    h.app.create_kind = Some(crate::CreateKind::Primitive(PrimitiveKind::Box));
    let r = h.app.viewport_rects[h.app.state.active_viewport];
    let (a, b) = (r.center() + egui::vec2(-80.0, 60.0), r.center() + egui::vec2(80.0, 140.0));
    h.drag(a, b);
    let c = b + egui::vec2(0.0, -60.0);
    for i in 1..=6 {
        h.run(vec![egui::Event::PointerMoved(b + (c - b) * (i as f32 / 6.0))]);
    }
    h.click(c);
    assert_eq!(h.app.state.scene.len(), 1, "one box");
    let id = *h.app.state.selection.iter().next().unwrap();
    let params = h.app.state.scene.node(id).unwrap().object.parameters();
    for name in ["Length", "Width", "Height"] {
        let v = params.iter().find(|(n, _)| *n == name).map(|(_, v)| format!("{v:?}")).unwrap_or_default();
        assert!(!v.contains("(0.0)"), "{name} was set by the slow drag: {v}");
    }
}

#[test]
fn slow_frames_move_gizmo_drag_moves_the_object() {
    let mut h = Harness::new();
    poly_box(&mut h);
    h.key(egui::Key::W);
    h.frame_time = SLOW;
    let id = *h.app.state.selection.iter().next().unwrap();
    let before = h.app.state.scene.node(id).unwrap().transform.position.get();
    let p = h.screen_of(Vec3::ZERO);
    h.drag(p, p + egui::vec2(60.0, 0.0));
    assert_ne!(h.app.state.scene.node(id).unwrap().transform.position.get(), before, "the slow gizmo drag moves the box");
}

#[test]
fn slow_frames_lasso_region_selects() {
    let mut h = Harness::new();
    let id = created(&mut h, PrimitiveKind::Box, [0.0; 3], 30.0);
    h.cmd(Command::SelectNone);
    framed(&mut h);
    h.cmd(Command::SelectionRegion { shape: loom_commands::SelectionRegion::Lasso });
    h.frame_time = SLOW;
    let i = h.app.state.active_viewport;
    let rect = h.app.viewport_rects[i];
    let vp = h.app.camera_for(i).view_projection(rect.width() / rect.height());
    let pts: Vec<egui::Pos2> = h.app.world_meshes()[0].1.vertices().iter().filter_map(|v| crate::viewport_widget::project_world_to_screen(v.position, vp, rect)).collect();
    let b = egui::Rect::from_points(&pts).expand(6.0);
    // Ends short of the start: the region closes itself, and a release on the press point would be a click.
    let corners = [b.left_top(), b.right_top(), b.right_bottom(), b.left_bottom(), b.left_top() + egui::vec2(0.0, 30.0)];
    h.run(vec![egui::Event::PointerMoved(corners[0])]);
    h.run(vec![egui::Event::PointerButton { pos: corners[0], button: egui::PointerButton::Primary, pressed: true, modifiers: h.mods }]);
    for w in corners.windows(2) {
        for k in 1..=4 {
            h.run(vec![egui::Event::PointerMoved(w[0] + (w[1] - w[0]) * (k as f32 / 4.0))]);
        }
    }
    h.run(vec![egui::Event::PointerButton { pos: corners[4], button: egui::PointerButton::Primary, pressed: false, modifiers: h.mods }]);
    h.frames(1);
    assert_eq!(selected(&h), vec![id], "a slow lasso around the box selects it ({} lasso points, viewport {rect:?}, lasso {b:?})", h.app.region_points.len());
}

#[test]
fn slow_frames_line_drag_shapes_the_knot() {
    let mut h = Harness::new();
    wide_view(&mut h);
    h.app.start_create(Some(crate::CreateKind::Line));
    h.frames(1);
    h.frame_time = SLOW;
    let a = ground_point(&h, Vec3::ZERO);
    h.click(a);
    let id = h.app.line.as_ref().expect("the line started").node;
    let b = ground_point(&h, Vec3::new(40.0, 0.0, 0.0));
    h.drag(b, b + egui::vec2(0.0, 60.0));
    let knots = h.app.state.scene.node(id).unwrap().edit_spline().unwrap().shape.splines[0].knots.clone();
    assert_eq!(knots.len(), 2, "{knots:?}");
    assert_ne!(knots[1].out_vec, Vec3::ZERO, "the slow drag gives the second knot a Bezier handle: {knots:?}");
}

#[test]
fn slow_frames_extrude_drag_extrudes() {
    let mut h = Harness::new();
    poly_box(&mut h);
    h.key(egui::Key::Num4);
    let p = h.screen_of(top_face_point());
    h.click(p);
    assert_eq!(h.sub_count(), 1);
    // The drag's press on the same face must not make a double-click.
    pause(&mut h);
    h.cmd(Command::ExtrudeMode);
    h.frame_time = SLOW;
    assert_eq!(h.sub_count(), 1, "Extrude Mode keeps the picked face");
    let before = h.app.world_meshes()[0].1.vertices().len();
    let q = p + egui::vec2(0.0, -60.0);
    h.run(vec![egui::Event::PointerMoved(p)]);
    h.run(vec![egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: h.mods }]);
    assert!(h.app.poly_dragging, "the press starts the Extrude drag ({} sub-objects)", h.sub_count());
    for i in 1..=8 {
        h.run(vec![egui::Event::PointerMoved(p + (q - p) * (i as f32 / 8.0))]);
    }
    let during = (h.app.world_meshes()[0].1.vertices().len(), h.sub_count(), h.app.state.undo.len(), h.app.last_error.clone());
    h.run(vec![egui::Event::PointerButton { pos: q, button: egui::PointerButton::Primary, pressed: false, modifiers: h.mods }]);
    h.frames(1);
    assert!(h.app.world_meshes()[0].1.vertices().len() > before, "the slow Extrude drag extrudes the face: during (vertices, subs, undo, error) {during:?}, after {} sub-objects, undo {}", h.sub_count(), h.app.state.undo.len());
}

#[test]
fn slow_frames_paint_stroke_paints_along_the_drag() {
    let mut h = Harness::new();
    let (_, bones) = skinned_cylinder(&mut h);
    h.cmd(Command::SkinSelectBone { id: bones[1] });
    h.cmd(Command::SkinSetPainterOption { name: "Max Size".into(), value: 8.0 });
    h.click(panel_text(&h, "Paint Weights"));
    let (_, world) = loom_commands::skin::vertex_world_positions(&h.app.state).unwrap();
    let low: Vec<(u32, egui::Pos2)> = clear_front_vertices(&h).into_iter().filter(|(v, _)| (world[*v as usize].z - 5.0).abs() < 0.1).collect();
    assert!(low.len() >= 2, "two front vertices on the z = 5 ring");
    let before = skin_weight(&h, low[1].0 as usize, 1);
    h.frame_time = SLOW;
    h.drag(low[0].1, low[1].1);
    assert!(skin_weight(&h, low[1].0 as usize, 1) > before, "the slow stroke reaches the far vertex");
}

#[test]
fn injected_input_rejects_bad_events() {
    use crate::live::parse_input;
    assert!(parse_input(&serde_json::json!([{ "type": "key", "key": "NoSuchKey" }])).is_err());
    assert!(parse_input(&serde_json::json!([{ "type": "click", "pos": [1] }])).is_err());
    assert!(parse_input(&serde_json::json!([{ "type": "click", "pos": [1, 2], "mods": ["hyper"] }])).is_err());
    assert!(parse_input(&serde_json::json!({ "type": "move" })).is_err());
    assert_eq!(parse_input(&serde_json::json!([{ "type": "click", "pos": [1, 2] }])).unwrap().len(), 3);
    assert_eq!(parse_input(&serde_json::json!([{ "type": "wait", "frames": 4 }])).unwrap().len(), 4);
}

#[test]
fn ui_hit_names_the_viewport_and_panel_under_a_point() {
    let mut h = Harness::new();
    poly_box(&mut h);
    let p = h.screen_of(Vec3::new(0.0, 0.0, 15.0));
    h.run(vec![egui::Event::PointerMoved(p)]);
    h.frames(1);
    let hit = h.app.ui_hit(&h.ctx, p);
    assert_eq!(hit["viewport"], h.app.state.active_viewport);
    assert_eq!(hit["pointer_here"], true);
    let menu = h.app.ui_hit(&h.ctx, egui::pos2(5.0, 5.0));
    assert_eq!(menu["panel"], "menubar", "{menu}");
    assert_eq!(menu["viewport"], serde_json::Value::Null);
    let tree = h.app.ui_tree(&h.ctx);
    assert!(tree["panels"].as_array().unwrap().iter().any(|p| p["name"] == "command_panel"), "{tree}");
    assert_eq!(tree["viewports"].as_array().unwrap().len(), h.app.viewport_rects.len());
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
    let all: Vec<egui::Pos2> = h.text_centers(text).into_iter().filter(|p| area.contains(*p)).collect();
    assert_eq!(all.len(), 1, "{text:?} in {window} {} times", all.len());
    all[0]
}

/// A text in the command panel (right of the maximized viewport).
fn panel_text(h: &Harness, text: &str) -> egui::Pos2 {
    let right = h.app.viewport_rects[h.app.state.active_viewport].right();
    let all: Vec<egui::Pos2> = h.text_centers(text).into_iter().filter(|p| p.x > right).collect();
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

/// A 60-unit plane of 4 x 4 cells (15 units) as an Editable Poly, framed.
fn poly_plane_4x4(h: &mut Harness) -> loom_scene::NodeId {
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Plane, position: [0.0; 3], size: [60.0, 60.0, 0.0] });
    let id = *h.app.state.selection.iter().next().unwrap();
    for n in ["Length Segs", "Width Segs"] {
        h.cmd(Command::SetParameter { id: id.0, name: n.into(), value: 4.0 });
    }
    h.cmd(Command::ConvertToEditablePoly);
    framed(h);
    id
}

fn cage(h: &Harness, id: loom_scene::NodeId) -> loom_mesh::Mesh {
    h.app.state.scene.node(id).unwrap().cage_mesh().into_owned()
}

/// The canonical edge between the vertices at two positions.
fn edge_between(m: &loom_mesh::Mesh, a: Vec3, b: Vec3) -> u32 {
    let at = |p: Vec3| (0..m.vertex_count() as u32).map(loom_mesh::VertexId).find(|v| m.vertex_position(*v).unwrap().distance(p) < 1e-3).expect("vertex there");
    let (va, vb) = (at(a), at(b));
    (0..m.half_edges().len() as u32)
        .map(loom_mesh::HalfEdgeId)
        .find(|e| m.canonical_edge_id(*e) == Some(*e) && m.edge_vertices(*e).is_some_and(|(x, y)| (x == va && y == vb) || (x == vb && y == va)))
        .expect("edge there")
        .0
}

#[test]
fn backspace_removes_vertices_and_ctrl_backspace_cleans_edges() {
    let mut h = Harness::new();
    poly_box(&mut h);
    let id = *h.app.state.selection.iter().next().unwrap();
    h.key(egui::Key::Num1);
    h.cmd(Command::SelectSub { node: Some(id.0), ids: vec![0], op: loom_commands::SelectOp::Replace });
    h.key(egui::Key::Backspace);
    assert_eq!(cage(&h, id).vertex_count(), 7, "Backspace at Vertex level removes the vertex");
    h.cmd(Command::UndoSceneOperation);
    // A connected loop, then Ctrl+Backspace: the edges and their vertices go.
    h.key(egui::Key::Num2);
    h.cmd(Command::SelectSub { node: Some(id.0), ids: vec![0], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::SelectEdgeRing);
    h.cmd(Command::ConnectEdges { segments: 1, pinch: 0.0, slide: 0.0 });
    assert_eq!(cage(&h, id).vertex_count(), 12);
    h.mods = egui::Modifiers::CTRL;
    h.key(egui::Key::Backspace);
    h.mods = egui::Modifiers::NONE;
    let m = cage(&h, id);
    assert_eq!((m.face_count(), m.vertex_count()), (6, 8));
}

#[test]
fn swift_loop_click_inserts_a_loop() {
    let mut h = Harness::new();
    poly_box(&mut h);
    let id = *h.app.state.selection.iter().next().unwrap();
    h.key(egui::Key::Num2);
    h.cmd(Command::SwiftLoopMode);
    // On the top face, close to its +X edge: the loop runs across that edge's ring.
    let p = h.screen_of(Vec3::new(13.0, 5.0, 30.0));
    h.click(p);
    let m = cage(&h, id);
    m.check_invariants().unwrap();
    assert_eq!(m.face_count(), 10, "a closed loop of four new edges");
    assert_eq!(h.app.state.pick_mode, Some(loom_commands::PickMode::SwiftLoop));
    right_click(&mut h, p);
    assert_eq!(h.app.state.pick_mode, None, "right-click leaves Swift Loop");
}

#[test]
fn cut_clicks_chain_until_right_click() {
    let mut h = Harness::new();
    poly_box(&mut h);
    let id = *h.app.state.selection.iter().next().unwrap();
    h.key(egui::Key::Num4);
    h.cmd(Command::Cut);
    for (x, y) in [(-14.5, -7.0), (14.5, -7.0), (-14.5, 7.0)] {
        let p = h.screen_of(Vec3::new(x, y, 30.0));
        h.click(p);
    }
    let m = cage(&h, id);
    m.check_invariants().unwrap();
    assert_eq!(m.face_count(), 8, "the third click cut on from the second");
    let p = h.screen_of(Vec3::new(0.0, 0.0, 30.0));
    right_click(&mut h, p);
    assert_eq!(h.app.state.pick_mode, Some(loom_commands::PickMode::CutA), "one right-click ends the cut");
    right_click(&mut h, p);
    assert_eq!(h.app.state.pick_mode, None, "a second leaves Cut");
}

#[test]
fn loop_mode_click_selects_the_edge_loop() {
    let mut h = Harness::new();
    let id = poly_plane_4x4(&mut h);
    h.key(egui::Key::Num2);
    h.cmd(Command::LoopMode);
    let p = h.screen_of(Vec3::new(0.0, 7.5, 0.0));
    h.click(p);
    assert_eq!(h.sub_count(), 4, "the click selects the whole line of edges");
    let m = cage(&h, id);
    assert!(h.app.state.sub_selection.iter().all(|(_, s)| match s {
        loom_commands::SubObjectId::Edge(e) => m.edge_vertices(e).is_some_and(|(a, b)| m.vertex_position(a).unwrap().x.abs() < 1e-3 && m.vertex_position(b).unwrap().x.abs() < 1e-3),
        _ => false,
    }));
}

#[test]
fn pick_hinge_and_pick_bridge_edge_take_a_clicked_edge() {
    let mut h = Harness::new();
    let id = poly_plane_4x4(&mut h);
    h.key(egui::Key::Num4);
    h.cmd(Command::PickHinge);
    let p = h.screen_of(Vec3::new(0.0, 7.5, 0.0));
    h.click(p);
    let m = cage(&h, id);
    let want = edge_between(&m, Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 15.0, 0.0));
    assert_eq!(h.app.state.poly_settings.hinge_edge, Some((id, want)), "the clicked edge is the hinge");
    assert_eq!(h.app.state.pick_mode, None);
    // Bridge Pick Edge 1 takes a border edge.
    h.key(egui::Key::Num2);
    h.cmd(Command::PickBridgeEdge { which: 1 });
    let p = h.screen_of(Vec3::new(-30.0, 7.5, 0.0));
    h.click(p);
    let border = edge_between(&m, Vec3::new(-30.0, 0.0, 0.0), Vec3::new(-30.0, 15.0, 0.0));
    assert_eq!(h.app.state.poly_settings.bridge_edges[0], Some((id, border)));
}

#[test]
fn bridge_edges_dialog_takes_edges_selected_while_it_is_open() {
    let mut h = Harness::new();
    let id = poly_plane_4x4(&mut h);
    // A one-cell hole at x 0..15, y 0..15.
    h.key(egui::Key::Num4);
    let m = cage(&h, id);
    let f = (0..m.face_count() as u32)
        .find(|f| {
            let p = m.face_positions(loom_mesh::FaceId(*f)).unwrap();
            (p.iter().sum::<Vec3>() / p.len() as f32).distance(Vec3::new(7.5, 7.5, 0.0)) < 0.1
        })
        .unwrap();
    h.cmd(Command::SelectSub { node: Some(id.0), ids: vec![f], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::DeleteObjects);
    assert_eq!(cage(&h, id).face_count(), 15);
    h.key(egui::Key::Num2);
    let m = cage(&h, id);
    let top = edge_between(&m, Vec3::new(0.0, 15.0, 0.0), Vec3::new(15.0, 15.0, 0.0));
    h.cmd(Command::SelectSub { node: Some(id.0), ids: vec![top], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::BridgeSettings);
    assert_eq!(cage(&h, id).face_count(), 15, "one edge: nothing to bridge yet");
    // Ctrl+click the hole's bottom edge in the viewport while the dialog is open.
    h.mods = egui::Modifiers::CTRL;
    let p = h.screen_of(Vec3::new(7.5, 0.0, 0.0));
    h.click(p);
    h.mods = egui::Modifiers::NONE;
    h.frames(2);
    assert_eq!(cage(&h, id).face_count(), 16, "the preview bridges the newly selected pair");
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
    h.click(h.find(egui::accesskit::Role::Button, "Modify").center());
    assert!(!h.app.state.show_end_result, "off by default");
    assert!((drawn_width(&h, a) - 20.0).abs() < 1e-3, "the top entry is active: the whole stack shows");
    // The base object in the stack: A shows its box, B (not being edited) its end result.
    h.click_widget("Box");
    assert_eq!(h.app.state.scene.node(a).unwrap().active_entry, None);
    assert!((drawn_width(&h, a) - 10.0).abs() < 1e-3, "{}", drawn_width(&h, a));
    assert!((drawn_width(&h, b) - 20.0).abs() < 1e-3, "{}", drawn_width(&h, b));
    // Leaving the Modify panel shows the end result again.
    let cp = panel(&h, "command_panel");
    let tab = *h.find_all(egui::accesskit::Role::Button, "Create").iter().find(|r| cp.contains(r.center())).expect("the Create tab");
    h.click(tab.center());
    assert!((drawn_width(&h, a) - 20.0).abs() < 1e-3);
    h.click(h.find(egui::accesskit::Role::Button, "Modify").center());
    assert!((drawn_width(&h, a) - 10.0).abs() < 1e-3);
    // Show End Result on: the whole stack.
    h.click(h.find(egui::accesskit::Role::Button, "Show End Result On/Off Toggle").center());
    assert!(h.app.state.show_end_result);
    assert!((drawn_width(&h, a) - 20.0).abs() < 1e-3);
}

#[test]
fn smooth_modifier_has_32_smoothing_group_buttons() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let id = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::AddModifier { name: "Smooth".into() });
    h.click(h.find(egui::accesskit::Role::Button, "Modify").center());
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
    h.click(h.find(egui::accesskit::Role::Button, "Modify").center());
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
    assert_eq!(h.widgets_labelled("Bend  (instanced)").len(), 1, "the new entry is marked as an instance");
    // A plain drag moves: the lower Taper (bottom row) onto the Bend above it.
    let rows = h.widgets_labelled("Taper");
    h.drag(rows[1].center(), h.widget("Bend").center());
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
    h.click(h.find(egui::accesskit::Role::Button, "Modify").center());
    h.click_widget("Box");
    let height = |h: &Harness| h.app.state.scene.node(id).unwrap().object.parameters().into_iter().find(|(n, _)| *n == "Height").unwrap().1;
    // The spinner's edit goes through dispatch, as a drag does.
    let _ = h.app.dispatch(Command::SetParameter { id: id.0, name: "Height".into(), value: 20.0 });
    h.frames(2);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::TopologyWarning));
    assert_eq!(height(&h), loom_scene::ParamValue::Float(10.0), "nothing changes before the answer");
    assert_eq!(h.text_centers("Hold/Yes").len(), 1);
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
    // The Maps rollout (closed by default), below the collapsed Blinn Basic Parameters.
    let win = material_editor_rect(&h);
    click_text(&mut h, "Blinn Basic Parameters", win, 0.0);
    click_in_editor(&mut h, "Maps", 0.0);
    // Diffuse Color's map button goes through the Material/Map Browser first.
    h.click(row_text(&h, "Diffuse Color", "None", material_editor_rect(&h)));
    h.frames(2);
    let diffuse = loom_scene::material::MAP_SLOTS.iter().position(|s| *s == "Diffuse Color").unwrap() as u8;
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::MaterialBrowser(loom_commands::BrowserFor::Map(diffuse))));
    assert!(h.app.state.material_editor_open, "the Browser leaves the editor open");
    // Bitmap, double-clicked: the bitmap chooser follows.
    h.frames(12);
    let bitmap = window_text(&h, "Material/Map Browser", "Bitmap");
    h.click(bitmap);
    h.click(bitmap);
    h.frames(2);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::MaterialBitmap));
    assert!(h.app.state.material_editor_open);
    click_text(&mut h, "Cancel", SCREEN, 0.0);
    assert_eq!(h.app.state.dialog, None, "Cancel closes the chooser");
    assert!(h.app.state.material_editor_open, "but not the editor");
    assert_eq!(h.app.state.scene.material_editor.active, 2, "on the same slot");
    let map = editor_material(&h).map("Diffuse Color").cloned().expect("the Bitmap stays, with no file yet, as in the reference");
    assert!(map.path.is_empty());
    assert!(h.text_rect("Bitmap Parameters", material_editor_rect(&h)).is_some(), "the editor shows the Bitmap's level");
    // Bitmap: None opens the chooser; the file goes into the map and the editor stays.
    h.click(row_text(&h, "Bitmap:", "None", material_editor_rect(&h)));
    h.frames(2);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::MaterialBitmap));
    click_text(&mut h, "File name:", SCREEN, 40.0);
    h.run(vec![egui::Event::Text("wood.png".into())]);
    click_text(&mut h, "Open", SCREEN, 0.0);
    assert_eq!(h.app.state.dialog, None);
    assert!(h.app.state.material_editor_open);
    assert_eq!(h.app.state.scene.material_editor.active, 2);
    let map = editor_material(&h).map("Diffuse Color").cloned().unwrap();
    assert!(map.path.ends_with("wood.png"), "{}", map.path);
    // Go to Parent: back at the material, its map button shows the bitmap.
    click_in_editor(&mut h, "Go to Parent", 0.0);
    assert!(h.app.state.scene.material_editor.path.is_empty(), "Go to Parent goes up to the material");
    assert!(h.text_rect("Shader Basic Parameters", material_editor_rect(&h)).is_some());
    assert!(h.text_rect("wood.png", material_editor_rect(&h)).is_some(), "the button shows the bitmap");
    // The map button goes back down to the map.
    click_in_editor(&mut h, "wood.png", 0.0);
    assert_eq!(h.app.state.scene.material_editor.path, vec![loom_scene::material::NavStep::Map("Diffuse Color".into())]);
}

/// Clicks the text `label` in the Material Editor window.
fn click_in_editor(h: &mut Harness, label: &str, dx: f32) {
    let win = material_editor_rect(h);
    click_text(h, label, win, dx);
}

/// The text `text` on the same row as the label `row` inside `within`.
fn row_text(h: &Harness, row: &str, text: &str, within: egui::Rect) -> egui::Pos2 {
    let label = h.text_centers(row).into_iter().find(|p| within.contains(*p)).unwrap_or_else(|| panic!("{row:?} is not shown"));
    h.text_centers(text).into_iter().filter(|p| within.contains(*p) && (p.y - label.y).abs() < 8.0 && p.x > label.x).min_by(|a, b| a.x.total_cmp(&b.x)).unwrap_or_else(|| panic!("no {text:?} on the {row:?} row"))
}

#[test]
fn only_the_map_buttons_open_a_chooser_from_the_material_editor() {
    let mut h = Harness::new();
    material_editor(&mut h);
    h.cmd(Command::SetMaterialEditorOption { name: "Slots".into(), value: 24.0 });
    let stays = |h: &Harness, what: &str| {
        assert_eq!(h.app.state.dialog, None, "{what} opened a dialog");
        assert!(h.app.state.material_editor_open, "{what} closed the editor");
    };
    // Sample slot 8 (second row, second column) of the 6 × 4 windows.
    let win = material_editor_rect(&h);
    let menu = h.text_rect("Material", win).expect("the menu bar");
    let top = menu.bottom() + 6.0;
    let pitch = 56.0 + 4.0;
    h.click(egui::pos2(win.left() + 8.0 + pitch + 28.0, top + pitch + 28.0));
    h.frames(2);
    assert_eq!(h.app.state.scene.material_editor.active, 7, "the slot click lands");
    stays(&h, "a sample slot");
    for (label, dx) in [("Assign to Selection", 0.0), ("Reset", 0.0), ("Show Map in Viewport", 0.0), ("Wire", 0.0), ("2-Sided", 0.0), ("Faceted", 0.0), ("Lock Ambient and Diffuse", 0.0), ("Self-Illumination", 20.0), ("Opacity", 20.0), ("Glossiness", 20.0), ("Shader Basic Parameters", 0.0)] {
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
    // Get Material and the Type button open the Material/Map Browser, as in the reference.
    click_in_editor(&mut h, "Get Material", 0.0);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::MaterialBrowser(loom_commands::BrowserFor::GetMaterial)));
    click_text(&mut h, "Cancel", SCREEN, 0.0);
    stays(&h, "the Browser's Cancel");
    click_in_editor(&mut h, "Standard", 0.0);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::MaterialBrowser(loom_commands::BrowserFor::MaterialType)));
}

/// The Type button, the Browser and the Replace Material dialog make a
/// Multi/Sub-Object material; faces then draw with their ID's colour in the
/// shaded viewport (the vertex data the wgpu renderer uploads).
#[test]
fn multi_sub_from_the_type_button_colours_faces_by_material_id() {
    let mut h = Harness::new();
    material_editor(&mut h);
    let node = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::SetMaterialColor { slot: None, which: "Diffuse".into(), rgb: [255, 0, 0] });
    click_in_editor(&mut h, "Standard", 0.0);
    // Single clicks, apart (not a double-click): select, then OK.
    h.frames(12);
    h.click(window_text(&h, "Material/Map Browser", "Multi/Sub-Object"));
    h.frames(12);
    h.click(window_text(&h, "Material/Map Browser", "OK"));
    h.frames(2);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::ReplaceMaterial { kind: 1, map: false }));
    h.click(window_text(&h, "Replace Material", "Keep old material as sub-material?"));
    h.frames(12);
    h.click(window_text(&h, "Replace Material", "OK"));
    h.frames(2);
    assert_eq!(h.app.state.dialog, None);
    let m = editor_material(&h);
    let loom_scene::material::MaterialKind::MultiSub(list) = &m.kind else { panic!("a Multi/Sub-Object material: {:?}", m.kind) };
    assert_eq!(list.len(), 10);
    assert!(h.text_rect("Multi/Sub-Object Basic Parameters", material_editor_rect(&h)).is_some());
    // Set Number opens its dialog; 2 sub-materials.
    click_in_editor(&mut h, "Set Number", 0.0);
    window_text(&h, "Set Number of Materials", "Number of Materials:");
    h.click(window_text(&h, "Set Number of Materials", "Cancel"));
    assert!(h.text_centers("Number of Materials:").is_empty(), "Cancel closes it");
    h.cmd(Command::MultiSubSetNumber { number: 2 });
    h.cmd(Command::MultiSubSetEntry { index: 1, id: None, name: None, enabled: None, color: Some([0, 0, 255]) });
    click_in_editor(&mut h, "Assign to Selection", 0.0);
    assert!(h.app.state.scene.node(node).unwrap().material.is_some());
    // Polygon level: Set ID 2 on two faces.
    h.cmd(Command::SelectionMode { mode: loom_commands::SelectionMode::Face });
    h.cmd(Command::SelectSub { node: Some(node.0), ids: vec![0, 1], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::SetMaterialId { id: 2 });
    h.cmd(Command::SelectSub { node: Some(node.0), ids: vec![2, 3, 4, 5], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::SetMaterialId { id: 1 });
    let style = h.app.world_styles[0].clone();
    assert!(style.sub.is_some(), "a per-ID style");
    let mesh = h.app.world_display()[0].clone();
    let (verts, _, _) = loom_render::triangulate_styled(&mesh, &style);
    // sRGB 255 → linear 1.0: red faces (ID 1, the old material) and blue ones (ID 2).
    let mut red = 0;
    let mut blue = 0;
    let mut first = 0usize;
    for (i, f) in mesh.faces().iter().enumerate().filter(|(_, f)| !f.hidden) {
        let n = mesh.face_vertex_ids(loom_mesh::FaceId(i as u32)).unwrap().len();
        let c = verts[first].color;
        match f.material_id + 1 {
            1 => {
                assert!(c[0] > 0.9 && c[2] < 0.05, "ID 1 is red: {c:?}");
                red += 1;
            }
            2 => {
                assert!(c[2] > 0.9 && c[0] < 0.05, "ID 2 is blue: {c:?}");
                blue += 1;
            }
            other => panic!("unexpected ID {other}"),
        }
        first += n;
    }
    assert_eq!((red, blue), (4, 2));
    // Go to a sub-material and back up with Go to Parent.
    h.cmd(Command::SelectionMode { mode: loom_commands::SelectionMode::Object });
    let loom_scene::material::MaterialKind::MultiSub(list) = &editor_material(&h).kind else { unreachable!() };
    let sub = list[1].material.unwrap();
    let sub_label = format!("{} (Standard)", h.app.state.scene.materials[&sub].name);
    click_in_editor(&mut h, &sub_label, 0.0);
    assert_eq!(h.app.state.scene.material_editor.path, vec![loom_scene::material::NavStep::Sub(1)]);
    assert!(h.text_rect("Blinn Basic Parameters", material_editor_rect(&h)).is_some(), "the sub-material's rollouts");
    click_in_editor(&mut h, "Go to Parent", 0.0);
    assert!(h.app.state.scene.material_editor.path.is_empty());
}

/// Get Material picks from the scene's materials in the Browser; the
/// eyedropper picks from an object clicked in the viewport; a slot dragged
/// onto an object assigns it.
#[test]
fn browser_get_material_eyedropper_and_dragging_a_slot_onto_an_object() {
    let mut h = Harness::new();
    material_editor(&mut h);
    let node = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::RenameMaterial { slot: None, name: "Brick".into() });
    h.cmd(Command::AssignMaterialToSelection);
    let brick = h.app.state.scene.material_editor.slots[0].unwrap();
    h.cmd(Command::MaterialEditorSlot { slot: 3 });
    click_in_editor(&mut h, "Get Material", 0.0);
    let entry = window_text(&h, "Material/Map Browser", "Brick [Standard]");
    h.click(entry);
    h.click(entry);
    h.frames(2);
    assert_eq!(h.app.state.dialog, None);
    assert_eq!(h.app.state.scene.material_editor.slots[3], Some(brick), "Browse From Scene put Brick in the slot");
    // A point on the box the editor window does not cover.
    let win = material_editor_rect(&h);
    let vp = h.app.viewport_rects[h.app.state.active_viewport];
    let on_box = [Vec3::new(15.0, 0.0, 15.0), Vec3::new(15.0, 10.0, 25.0), Vec3::new(10.0, -15.0, 25.0), Vec3::new(0.0, 0.0, 30.0), Vec3::new(14.0, 14.0, 30.0)]
        .into_iter()
        .map(|p| h.screen_of(p))
        .find(|p| !win.contains(*p) && vp.contains(*p))
        .expect("part of the box is outside the editor window");
    // Eyedropper.
    h.cmd(Command::MaterialEditorSlot { slot: 5 });
    click_in_editor(&mut h, "Pick", 0.0);
    assert_eq!(h.app.state.pick_mode, Some(loom_commands::PickMode::PickMaterial));
    h.click(on_box);
    assert_eq!(h.app.state.scene.material_editor.slots[5], Some(brick), "the eyedropper picked the box's material");
    assert_eq!(h.app.state.pick_mode, None);
    // Drag sample slot 1 (top row, second) onto the box.
    h.cmd(Command::SetMaterialColor { slot: Some(1), which: "Diffuse".into(), rgb: [0, 200, 0] });
    let win = material_editor_rect(&h);
    let menu = h.text_rect("Material", win).unwrap();
    let slot1 = egui::pos2(win.left() + 8.0 + 100.0 + 48.0, menu.bottom() + 6.0 + 48.0);
    h.drag(slot1, on_box);
    h.frames(2);
    let assigned = h.app.state.scene.node(node).unwrap().material;
    assert_eq!(assigned, h.app.state.scene.material_editor.slots[1], "the dragged slot's material is on the box");
    assert!(h.app.world_styles[0].color[1] > 0.3, "and colours it");
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
    // The stack rows by their names (the panel's other rows have menus of their own).
    let in_panel = |h: &Harness, t: &str| *h.text_rects(t).iter().find(|r| cp.contains_rect(**r)).unwrap_or_else(|| panic!("{t} in the stack"));
    let bend = in_panel(&h, "Bend").center();
    right_click(&mut h, bend);
    // Rename, Delete, Off.
    let m = menus(&h)[0];
    h.click(menu_row(&h, m, 2));
    assert!(!h.app.state.scene.node(id).unwrap().modifiers[0].enabled, "Off runs");
    assert!(menus(&h).is_empty(), "and closes the menu");
    let base = in_panel(&h, "Box").center();
    right_click(&mut h, base);
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

fn light_of(h: &Harness, id: loom_scene::NodeId) -> loom_scene::light::LightObject {
    h.app.state.scene.node(id).unwrap().object.light().expect("a light").clone()
}

#[test]
fn the_lights_category_buttons_start_light_creation() {
    let mut h = Harness::new();
    h.app.panel_tab = crate::Tab::Create;
    h.frames(2);
    let cp = panel(&h, "command_panel");
    // The category buttons are icons, named for screen readers.
    h.click(h.find(egui::accesskit::Role::Button, "Lights").center());
    click_text(&mut h, "Target Spot", cp, 0.0);
    assert_eq!(h.app.create_kind, Some(crate::CreateKind::Light(loom_commands::light::LightKind::TargetSpot)));
    click_text(&mut h, "Omni", cp, 0.0);
    assert_eq!(h.app.create_kind, Some(crate::CreateKind::Light(loom_commands::light::LightKind::Omni)));
}

#[test]
fn clicking_an_omni_places_it_and_its_icon_selects_it() {
    let mut h = Harness::new();
    wide_view(&mut h);
    h.app.create_kind = Some(crate::CreateKind::Light(loom_commands::light::LightKind::Omni));
    let at = Vec3::new(20.0, -10.0, 0.0);
    let p = h.screen_of(at);
    h.click(p);
    let id = *h.app.state.selection.iter().next().expect("the new omni is selected");
    assert_eq!(light_of(&h, id).light_type, loom_scene::light::LightType::Omni);
    assert!(h.app.state.scene.world_matrix(id).w_axis.truncate().distance(at) < 0.5);
    h.app.create_kind = None;
    h.cmd(Command::SelectNone);
    h.click(p);
    assert!(h.app.state.selection.contains(&id), "a click on the icon selects the light");
}

#[test]
fn dragging_a_target_spot_puts_the_light_at_the_press_and_the_target_at_the_release() {
    let mut h = Harness::new();
    wide_view(&mut h);
    h.app.create_kind = Some(crate::CreateKind::Light(loom_commands::light::LightKind::TargetSpot));
    let (from, to) = (Vec3::new(-40.0, -40.0, 0.0), Vec3::new(30.0, 20.0, 0.0));
    let (a, b) = (h.screen_of(from), h.screen_of(to));
    h.drag(a, b);
    let spot = *h.app.state.selection.iter().next().expect("the new spot is selected");
    let t = light_of(&h, spot).target.expect("a target spot");
    let eye = h.app.state.scene.world_matrix(spot).w_axis.truncate();
    let at = h.app.state.scene.world_matrix(t).w_axis.truncate();
    assert!(eye.distance(from) < 0.5, "light at the press: {eye}");
    assert!(at.distance(to) < 0.5, "target at the release: {at}");
    let forward = -h.app.state.scene.world_matrix(spot).z_axis.truncate().normalize();
    assert!(forward.distance((to - from).normalize()) < 1e-2, "the spot aims at its target");
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(h.app.state.scene.len(), 0, "one undo step removes light and target");
}

#[test]
fn modify_rollouts_edit_a_light() {
    let mut h = Harness::new();
    let r = h.app.dispatch(Command::CreateLight { kind: loom_commands::light::LightKind::FreeSpot, position: [0.0, 0.0, 80.0], target: [0.0; 3] }).unwrap();
    let spot = loom_scene::NodeId(r["id"].as_u64().unwrap());
    h.app.panel_tab = crate::Tab::Modify;
    h.frames(3);
    let cp = panel(&h, "command_panel");
    assert!(h.text_rect("Free Spot", cp).is_some(), "the stack names the light type");
    assert!(h.text_rect("Intensity/Color/Attenuation", cp).is_some());
    click_text(&mut h, "Targeted", cp, 0.0);
    let t = light_of(&h, spot).target.expect("Targeted adds a target");
    assert!(h.app.state.scene.node(t).is_some());
    click_text(&mut h, "Exclude...", cp, 0.0);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::LightExclude { light: spot.0 }));
    click_text(&mut h, "Cancel", SCREEN, 0.0);
    assert_eq!(h.app.state.dialog, None);
    // Shift+4: the view from the selected spot.
    h.mods = egui::Modifiers::SHIFT;
    h.key(egui::Key::Num4);
    h.mods = egui::Modifiers::NONE;
    assert_eq!(h.app.state.active().kind, loom_commands::ViewKind::Camera(spot));
    let c = h.app.camera_for(h.app.state.active_viewport);
    assert!((c.fov_x_radians.unwrap().to_degrees() - 45.0).abs() < 1e-3, "the light view spans the falloff");
}

// --- Rendering: the Rendered Frame Window, Render Setup, the render region ---

fn rfw_rect(h: &Harness) -> egui::Rect {
    h.ctx.memory(|m| m.area_rect(egui::Id::new("Rendered Frame Window"))).expect("the Rendered Frame Window is shown")
}

/// A 64 x 32 test picture: red rises to the right, green downwards, blue 200.
fn test_picture() -> image::RgbaImage {
    image::RgbaImage::from_fn(64, 32, |x, y| image::Rgba([(x * 4) as u8, (y * 8) as u8, 200, 255]))
}

/// The Rendered Frame Window open on the test picture.
fn rfw_with_picture(h: &mut Harness) -> egui::Rect {
    h.app.state.rendered.set_image(Some(std::sync::Arc::new(test_picture())));
    h.cmd(Command::RenderedFrameWindow);
    h.frames(3);
    rfw_rect(h)
}

/// The window's texture as last uploaded in full.
fn rfw_texture_pixel(h: &Harness, x: usize, y: usize) -> [u8; 3] {
    let id = h.app.rfw.main.texture_id().expect("the image has a texture");
    let (_, delta) = h.textures.iter().rev().find(|(t, d)| *t == id && d.pos.is_none()).expect("a full upload");
    let egui::ImageData::Color(img) = &delta.image else { panic!("a colour image") };
    let c = img.pixels[y * img.size[0] + x];
    [c.r(), c.g(), c.b()]
}

#[test]
fn shift_q_opens_the_rendered_frame_window() {
    let mut h = Harness::new();
    h.mods = egui::Modifiers::SHIFT;
    h.key(egui::Key::Q);
    h.mods = egui::Modifiers::NONE;
    h.frames(2);
    assert!(h.app.state.rendered.open, "Render opens the window, not a file dialog");
    assert_eq!(h.app.state.dialog, None);
    let win = rfw_rect(&h);
    for label in ["Area to Render:", "Viewport:", "Lock to Viewport", "Render", "Save Image", "Copy Image", "Clone", "Clear", "R", "G", "B", "Alpha", "Mono"] {
        assert!(h.text_rect(label, win).is_some(), "{label:?} is in the window");
    }
    // Headless: no GPU, reported in the status bar.
    assert!(h.app.last_error.as_deref().is_some_and(|e| e.contains("Render")), "{:?}", h.app.last_error);
    // The toolbar button closes and reopens it.
    h.cmd(Command::RenderedFrameWindow);
    assert!(!h.app.state.rendered.open);
}

#[test]
fn channel_toggles_change_what_the_window_shows() {
    let mut h = Harness::new();
    let win = rfw_with_picture(&mut h);
    assert_eq!(rfw_texture_pixel(&h, 10, 3), [40, 24, 200]);
    click_text(&mut h, "R", win, 0.0);
    assert!(!h.app.state.rendered.channels.red);
    assert_eq!(rfw_texture_pixel(&h, 10, 3), [0, 24, 200], "red off");
    click_text(&mut h, "R", win, 0.0);
    click_text(&mut h, "Mono", win, 0.0);
    let g = loom_commands::render::mono([40, 24, 200]);
    assert_eq!(rfw_texture_pixel(&h, 10, 3), [g, g, g], "monochrome");
    click_text(&mut h, "Mono", win, 0.0);
    click_text(&mut h, "Alpha", win, 0.0);
    assert_eq!(rfw_texture_pixel(&h, 10, 3), [255, 255, 255], "display alpha: opaque is white");
}

#[test]
fn the_pixel_under_the_pointer_is_read_out() {
    let mut h = Harness::new();
    let win = rfw_with_picture(&mut h);
    let img = h.app.rfw.main.image_rect;
    assert_eq!(img.size(), egui::vec2(64.0, 32.0), "1:1");
    h.run(vec![egui::Event::PointerMoved(img.min + egui::vec2(10.5, 3.5))]);
    h.frames(2);
    assert_eq!(h.app.rfw.main.hover, Some(([10, 3], [40, 24, 200, 255])));
    assert!(h.text_rect("(10, 3)  R 40 G 24 B 200 A 255", win).is_some(), "the readout shows it");
}

#[test]
fn wheel_zooms_and_middle_drag_pans_the_image() {
    let mut h = Harness::new();
    rfw_with_picture(&mut h);
    let img = h.app.rfw.main.image_rect;
    let p = img.center();
    h.run(vec![egui::Event::PointerMoved(p)]);
    h.run(vec![egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: egui::vec2(0.0, 100.0), modifiers: egui::Modifiers::NONE }]);
    h.frames(2);
    let zoomed = h.app.rfw.main.image_rect;
    assert!(h.app.rfw.main.zoom > 1.2, "zoom {}", h.app.rfw.main.zoom);
    assert!((zoomed.center() - p).length() < 1.5, "zooms about the pointer");
    let before = h.app.rfw.main.pan;
    h.run(vec![egui::Event::PointerButton { pos: p, button: egui::PointerButton::Middle, pressed: true, modifiers: egui::Modifiers::NONE }]);
    for i in 1..=5 {
        h.run(vec![egui::Event::PointerMoved(p + egui::vec2(6.0 * i as f32, 3.0 * i as f32))]);
    }
    h.run(vec![egui::Event::PointerButton { pos: p + egui::vec2(30.0, 15.0), button: egui::PointerButton::Middle, pressed: false, modifiers: egui::Modifiers::NONE }]);
    h.frames(1);
    let moved = h.app.rfw.main.pan - before;
    assert!(moved.x > 20.0 && moved.y > 8.0, "pan {moved:?}");
}

#[test]
fn save_image_writes_the_picture_through_the_file_dialog() {
    let mut h = Harness::new();
    h.app.native_file_dialogs = false;
    let win = rfw_with_picture(&mut h);
    click_text(&mut h, "Save Image", win, 0.0);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::SaveRenderedImage));
    let path = std::env::temp_dir().join(format!("loom-rfw-save-{}.png", std::process::id()));
    let _ = std::fs::remove_file(&path);
    click_text(&mut h, "File name:", SCREEN, 40.0);
    h.run(vec![egui::Event::Text(path.to_string_lossy().into_owned())]);
    click_text(&mut h, "Save", SCREEN, 0.0);
    let back = image::open(&path).expect("Save Image wrote the file").to_rgba8();
    let _ = std::fs::remove_file(&path);
    assert_eq!(back.dimensions(), (64, 32));
    assert_eq!(back.get_pixel(10, 3).0, [40, 24, 200, 255]);
}

#[test]
fn copy_clone_and_clear() {
    let mut h = Harness::new();
    let win = rfw_with_picture(&mut h);
    click_text(&mut h, "R", win, 0.0);
    click_text(&mut h, "Copy Image", win, 0.0);
    let copied = h.commands.iter().find_map(|c| match c {
        egui::OutputCommand::CopyImage(img) => Some(img.clone()),
        _ => None,
    });
    let copied = copied.expect("Copy Image puts the picture on the clipboard");
    assert_eq!(copied.size, [64, 32]);
    let c = copied.pixels[3 * 64 + 10];
    assert_eq!([c.r(), c.g(), c.b()], [0, 24, 200], "as displayed (red off)");
    click_text(&mut h, "Clone", win, 0.0);
    assert_eq!(h.app.state.rendered.clones.len(), 1);
    h.frames(2);
    let clone = h.ctx.memory(|m| m.area_rect(egui::Id::new(("Rendered Frame Window clone", 1u32)))).expect("the clone window shows");
    assert!(clone.width() > 50.0);
    let win = rfw_rect(&h);
    click_text(&mut h, "Clear", win, 0.0);
    assert!(h.app.state.rendered.image.is_none(), "Clear empties the window");
    assert_eq!(h.app.state.rendered.clones[0].image.dimensions(), (64, 32), "the clone keeps its copy");
}

#[test]
fn edit_region_drags_the_render_region_in_the_viewport() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [30.0; 3] });
    framed(&mut h);
    let selected = h.app.state.selection.clone();
    h.cmd(Command::EditRegion);
    assert_eq!(h.app.state.scene.render.area, loom_scene::render::RenderArea::Region);
    let index = h.app.state.active_viewport;
    let vp = h.app.viewport_rects[index];
    let frame = crate::rendered_frame::output_frame(&h.app, index, vp);
    assert!((frame.height() - vp.height()).abs() < 1.0 && (frame.width() / frame.height() - 640.0 / 480.0).abs() < 1e-3, "the 4:3 output inside the view");
    let undo = h.app.state.undo.len();
    // Drag the top-left handle up and left.
    let corner = frame.min + egui::vec2(0.25, 0.25) * frame.size();
    h.drag(corner, corner - egui::vec2(frame.width() * 0.1, frame.height() * 0.1));
    let r = h.app.state.scene.render.region;
    assert!((r[0] - 0.15).abs() < 0.01 && (r[1] - 0.15).abs() < 0.01 && r[2] == 0.75 && r[3] == 0.75, "{r:?}");
    assert_eq!(h.app.state.undo.len(), undo + 1, "the drag is one undo step");
    // Drag inside moves it.
    let inside = frame.min + egui::vec2(0.5, 0.5) * frame.size();
    h.drag(inside, inside + egui::vec2(frame.width() * 0.1, 0.0));
    let m = h.app.state.scene.render.region;
    assert!((m[0] - 0.25).abs() < 0.01 && (m[2] - 0.85).abs() < 0.01 && (m[1] - 0.15).abs() < 0.01, "{m:?}");
    assert_eq!(h.app.state.selection, selected, "the region drags do not select in the viewport");
    h.cmd(Command::UndoSceneOperation);
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(h.app.state.scene.render.region, [0.25, 0.25, 0.75, 0.75]);
}

#[test]
fn render_setup_dialog_sets_the_output_size_and_renderer() {
    let mut h = Harness::new();
    h.key(egui::Key::F10);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::RenderSetup));
    let win = h.ctx.memory(|m| m.area_rect(egui::Id::new("Render Setup"))).expect("Render Setup shows");
    for label in ["Time Output", "Output Size", "Image Aspect:", "Pixel Aspect:", "Options", "Render Hidden Geometry", "Force 2-Sided", "Render Output", "Save File", "Files...", "Assign Renderer", "Production: Bevy (PBR rasteriser)", "Viewport:", "Lock View", "Render"] {
        assert!(h.text_rect(label, win).is_some(), "{label:?} in Render Setup");
    }
    click_text(&mut h, "1280x720", win, 0.0);
    assert_eq!((h.app.state.scene.render.width, h.app.state.scene.render.height), (1280, 720));
    click_text(&mut h, "Force 2-Sided", win, 0.0);
    assert!(h.app.state.scene.render.force_two_sided);
    click_text(&mut h, "Renderer", win, 0.0);
    let win = h.ctx.memory(|m| m.area_rect(egui::Id::new("Render Setup"))).unwrap();
    for label in ["Antialiasing (4x MSAA)", "Supersampling:", "Shadows", "Map Size:", "Background:", "Ambient:"] {
        assert!(h.text_rect(label, win).is_some(), "{label:?} in the Renderer panel");
    }
    click_text(&mut h, "Shadows", win, 0.0);
    assert!(!h.app.state.scene.render.bevy.shadows);
    h.cmd(Command::UndoSceneOperation);
    assert!(h.app.state.scene.render.bevy.shadows, "each change is an undo step");
}

/// Renders until the render in progress ends (at most `frames` frames).
fn finish_render(h: &mut Harness, frames: usize) {
    for _ in 0..frames {
        h.frames(1);
        if h.app.state.rendered.progress.is_none() && h.app.state.pending_render.is_none() {
            return;
        }
    }
    panic!("the render did not finish in {frames} frames");
}

#[test]
fn a_render_runs_in_buckets_into_the_window_and_saves_its_file() {
    let Some(mut h) = Harness::with_gpu() else {
        eprintln!("a_render_runs_in_buckets_into_the_window_and_saves_its_file: no GPU adapter, skipped");
        return;
    };
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Teapot, position: [0.0; 3], size: [25.0; 3] });
    framed(&mut h);
    let path = std::env::temp_dir().join(format!("loom-ui-render-{}.png", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let r = h.app.dispatch_json(&serde_json::json!({ "cmd": "Render SetUp...", "args": { "width": 600, "height": 300, "output": path.to_string_lossy() } }));
    assert_eq!(r["ok"], true, "{r}");
    h.mods = egui::Modifiers::SHIFT;
    h.key(egui::Key::Q);
    h.mods = egui::Modifiers::NONE;
    // 600 x 300 is 3 x 2 buckets: the UI keeps running between them.
    let p = h.app.state.rendered.progress.expect("rendering");
    assert_eq!(p.total, 6);
    assert!(p.done < p.total, "not done in the frame it started");
    finish_render(&mut h, 60);
    assert_eq!(h.app.last_error, None);
    let img = h.app.state.rendered.image.clone().expect("the image is in the window");
    assert_eq!(img.dimensions(), (600, 300));
    let wire = h.app.state.scene.node(*h.app.state.selection.iter().next().unwrap()).unwrap().wire_color;
    let lit = img.pixels().filter(|p| p.0[3] == 255).count();
    assert!(lit > 5000, "the teapot fills part of the frame: {lit} opaque pixels (wire colour {wire:?})");
    assert_eq!(img.get_pixel(1, 1).0[3], 0, "background alpha 0");
    let saved = image::open(&path).expect("Save File wrote the render").to_rgba8();
    let _ = std::fs::remove_file(&path);
    assert_eq!(saved.dimensions(), (600, 300));
    // F9 renders again, without saving.
    h.key(egui::Key::F9);
    finish_render(&mut h, 60);
    assert!(!path.exists(), "Render Last does not save");
}

#[test]
fn cancel_stops_a_render_and_keeps_the_buckets_done() {
    let Some(mut h) = Harness::with_gpu() else {
        eprintln!("cancel_stops_a_render_and_keeps_the_buckets_done: no GPU adapter, skipped");
        return;
    };
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Teapot, position: [0.0; 3], size: [25.0; 3] });
    framed(&mut h);
    h.app.dispatch_json(&serde_json::json!({ "cmd": "Render SetUp...", "args": { "width": 2048, "height": 1536 } }));
    h.cmd(Command::Render { path: None, width: None, height: None });
    h.frames(6);
    let p = h.app.state.rendered.progress.expect("still rendering");
    assert!(p.done > 0 && p.done < p.total, "{p:?}");
    let win = rfw_rect(&h);
    click_text(&mut h, "Cancel", win, 0.0);
    assert!(h.app.state.rendered.progress.is_none(), "cancelled");
    assert!(h.app.state.status_prompt.contains("cancelled"), "{}", h.app.state.status_prompt);
    assert_eq!(h.app.state.rendered.image.as_ref().map(|i| i.dimensions()), Some((2048, 1536)), "the partial image stays");
    // Esc cancels too.
    h.cmd(Command::Render { path: None, width: None, height: None });
    h.frames(3);
    assert!(h.app.state.rendered.progress.is_some());
    h.key(egui::Key::Escape);
    assert!(h.app.state.rendered.progress.is_none(), "Esc cancels");
}

#[test]
fn an_iterative_region_render_keeps_the_rest_of_the_window() {
    let Some(mut h) = Harness::with_gpu() else {
        eprintln!("an_iterative_region_render_keeps_the_rest_of_the_window: no GPU adapter, skipped");
        return;
    };
    // An empty scene: every pixel is background, so the region shows exactly.
    maximized(&mut h);
    h.app.dispatch_json(&serde_json::json!({ "cmd": "Render SetUp...", "args": { "width": 320, "height": 240 } }));
    h.app.dispatch_json(&serde_json::json!({ "cmd": "Bevy Renderer Settings", "args": { "background": [0.0, 0.0, 1.0] } }));
    h.cmd(Command::Render { path: None, width: None, height: None });
    finish_render(&mut h, 60);
    h.app.dispatch_json(&serde_json::json!({ "cmd": "Bevy Renderer Settings", "args": { "background": [0.0, 1.0, 0.0] } }));
    h.app.dispatch_json(&serde_json::json!({ "cmd": "Area to Render", "args": { "area": "Region", "region": [0.0, 0.0, 0.25, 0.25] } }));
    h.cmd(Command::RenderIterative);
    finish_render(&mut h, 60);
    let img = h.app.state.rendered.image.clone().unwrap();
    assert_eq!(img.get_pixel(2, 2).0, [0, 255, 0, 0], "the region renders with the new background");
    assert_eq!(img.get_pixel(300, 230).0, [0, 0, 255, 0], "outside it the earlier render stays");
    // Production clears the window first.
    h.cmd(Command::Render { path: None, width: None, height: None });
    finish_render(&mut h, 60);
    let img = h.app.state.rendered.image.clone().unwrap();
    assert_eq!(img.get_pixel(300, 230).0, [0, 255, 0, 0], "Production starts from a cleared frame");
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



/// An image file dropped from the file manager onto selected polygons gets
/// a material of its own there (a Multi/Sub-Object material is built).
#[test]
fn an_image_file_dropped_on_selected_polygons_assigns_it_there() {
    let mut h = Harness::new();
    poly_box(&mut h);
    let node = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::SelectionMode { mode: loom_commands::SelectionMode::Face });
    h.cmd(Command::SelectSub { node: Some(node.0), ids: vec![0], op: loom_commands::SelectOp::Replace });
    let p = h.screen_of(Vec3::new(0.0, 0.0, 15.0));
    h.run(vec![egui::Event::PointerMoved(p)]);
    h.time += 1.0 / 30.0;
    let input = egui::RawInput { screen_rect: Some(h.screen), time: Some(h.time), dropped_files: vec![egui::DroppedFile { path: Some("/x/bfrontgarage.jpg".into()), ..Default::default() }], ..Default::default() };
    let app = &mut h.app;
    h.shapes = h.ctx.run(input, |ctx| app.frame(ctx)).shapes;
    h.frames(2);
    let multi = h.app.state.scene.node(node).unwrap().material.expect("the drop assigned a material");
    let loom_scene::material::MaterialKind::MultiSub(list) = &h.app.state.scene.materials[&multi].kind else { panic!("a Multi/Sub-Object material") };
    let mesh = h.app.state.scene.node(node).unwrap().cage_mesh().into_owned();
    let row = list.iter().find(|e| e.id == mesh.faces()[0].material_id + 1).expect("the face's ID has a sub-material");
    assert_eq!(h.app.state.scene.materials[&row.material.unwrap()].diffuse_map.as_ref().unwrap().path, "/x/bfrontgarage.jpg");
    assert!(h.app.world_styles[0].sub.is_some(), "the viewport draws it per ID");
}

/// AutoGrid: with a create tool waiting, a tripod sits on the surface under
/// the cursor with Z along the face normal; off the surface there is none.
#[test]
fn autogrid_tripod_follows_the_cursor_over_surfaces() {
    let mut h = Harness::new();
    poly_box(&mut h);
    h.cmd(Command::SelectNone);
    h.app.create_kind = Some(crate::CreateKind::Primitive(PrimitiveKind::Sphere));
    h.app.autogrid = true;
    let i = h.app.state.active_viewport;
    let p = h.screen_of(top_face_point());
    let (at, (_, _, z)) = crate::viewport_widget::autogrid_tripod(&h.app, i, p).expect("a tripod on the box");
    assert!((at.z - 30.0).abs() < 0.1, "on the top face: {at}");
    assert!(z.distance(Vec3::Z) < 1e-3, "Z along the face normal: {z}");
    assert!(crate::viewport_widget::autogrid_tripod(&h.app, i, empty_point(&h)).is_none(), "none off the surface");
    h.app.autogrid = false;
    assert!(crate::viewport_widget::autogrid_tripod(&h.app, i, p).is_none(), "only with AutoGrid on");
}

// ---- Viewport Background ----

#[test]
fn viewport_background_dialog_sets_aspect_and_picks_the_viewport() {
    let mut h = Harness::new();
    h.cmd(Command::ActiveViewport { index: 1 }); // Front
    h.cmd(Command::SetViewportBackground { path: Some("/tmp/ref.png".into()), aspect: None, lock: None, display: None, all_views: None });
    h.cmd(Command::ViewportBackground);
    let screen = h.ctx.screen_rect();
    click_text(&mut h, "Match Bitmap", screen, 0.0);
    click_text(&mut h, "Lock Zoom/Pan", screen, 0.0);
    click_text(&mut h, "OK", screen, 0.0);
    let vp = &h.app.state.viewports[1];
    assert_eq!(vp.background_aspect, loom_commands::viewport::BackgroundAspect::Bitmap);
    assert!(vp.background_locked && vp.background_anchor.is_some());
    assert_eq!(h.app.state.dialog, None, "OK closes it");
    // The Viewport list makes another viewport active and shows its settings.
    h.cmd(Command::ViewportBackground);
    click_text(&mut h, "Front", screen, 0.0);
    click_text(&mut h, "Left", screen, 0.0);
    assert_eq!(h.app.state.active_viewport, 2, "Left is now active");
    // Files... opens the file browser, which comes back to the dialog.
    click_text(&mut h, "Files...", screen, 0.0);
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::BackgroundFile));
}

#[test]
fn background_rect_keeps_the_bitmap_aspect_and_follows_the_zoom_when_locked() {
    let mut vp = loom_commands::Viewport::new(loom_commands::ViewKind::Front);
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1000.0, 600.0));
    let vp_mat = glam::Mat4::IDENTITY;
    assert_eq!(crate::viewport_widget::background_rect(&vp, rect, [200, 100], (640, 480), vp_mat), rect, "Match Viewport stretches");
    vp.background_aspect = loom_commands::viewport::BackgroundAspect::Bitmap;
    let r = crate::viewport_widget::background_rect(&vp, rect, [200, 100], (640, 480), vp_mat);
    assert_eq!((r.width(), r.height()), (1000.0, 500.0));
    assert_eq!(r.center(), rect.center());
    vp.background_aspect = loom_commands::viewport::BackgroundAspect::Rendering;
    let r = crate::viewport_widget::background_rect(&vp, rect, [200, 100], (640, 480), vp_mat);
    assert_eq!((r.width(), r.height()), (800.0, 600.0), "640 x 480 is 4:3");
    vp.background_locked = true;
    vp.background_anchor = Some((Vec3::ZERO, vp.camera.distance * 2.0));
    let r = crate::viewport_widget::background_rect(&vp, rect, [200, 100], (640, 480), vp_mat);
    assert_eq!((r.width(), r.height()), (1600.0, 1200.0), "zoomed in twice as far since the lock");
}

// ---- viewport navigation: ViewCube, Orbit trackball, I, camera viewport controls ----

/// The ViewCube of the active viewport.
fn cube_rect(h: &Harness) -> egui::Rect {
    let i = h.app.state.active_viewport;
    crate::viewport_gizmo::rect(h.app.viewport_rects[i], h.app.state.viewcube.size)
}

/// A point on the ViewCube that hits `want`, found by scanning the cube.
fn cube_point(h: &Harness, want: crate::viewport_gizmo::Hit) -> egui::Pos2 {
    let i = h.app.state.active_viewport;
    let (r, cam) = (cube_rect(h), h.app.state.viewports[i].camera.clone());
    let perspective = !h.app.state.viewports[i].is_orthographic();
    for y in 0..56 {
        for x in 0..56 {
            let p = r.min + egui::vec2(x as f32 * r.width() / 56.0, y as f32 * r.height() / 56.0);
            if crate::viewport_gizmo::hit_test(p, r, &cam, perspective) == Some(want) {
                return p;
            }
        }
    }
    panic!("{want:?} is not on the cube");
}

fn eye_dir(h: &Harness) -> Vec3 {
    let c = &h.app.state.active().camera;
    (c.eye_position() - c.target).normalize()
}

#[test]
fn viewcube_corner_click_gives_the_three_quarter_view_and_a_current_face_keeps_its_view() {
    use crate::viewport_gizmo::Hit;
    let mut h = Harness::new();
    maximized(&mut h);
    let corner = glam::IVec3::new(-1, -1, 1);
    let p = cube_point(&h, Hit::Spot(corner));
    h.click(p);
    h.frames(12);
    assert_eq!(h.app.state.active().kind, loom_commands::ViewKind::Perspective, "Perspective stays Perspective");
    assert!(eye_dir(&h).distance(corner.as_vec3().normalize()) < 1e-3, "looking down the corner's diagonal: {}", eye_dir(&h));
    // Front face, then a click on the current FRONT face keeps the Front view.
    pause(&mut h);
    let p = cube_point(&h, Hit::Spot(glam::IVec3::new(0, -1, 0)));
    h.click(p);
    h.frames(12);
    assert_eq!(h.app.state.active().kind, loom_commands::ViewKind::Front);
    pause(&mut h);
    let p = cube_point(&h, Hit::Spot(glam::IVec3::new(0, -1, 0)));
    h.click(p);
    h.frames(12);
    assert_eq!(h.app.state.active().kind, loom_commands::ViewKind::Front, "the face already shown keeps its view");
}

#[test]
fn viewcube_triangles_go_to_the_adjacent_face_and_the_arrows_roll() {
    use crate::viewport_gizmo::{Hit, Side};
    let mut h = Harness::new();
    maximized(&mut h);
    h.cmd(Command::FrontView);
    let p = cube_point(&h, Hit::Adjacent(Side::Right));
    h.click(p);
    h.frames(12);
    assert_eq!(h.app.state.active().kind, loom_commands::ViewKind::Right);
    pause(&mut h);
    let p = cube_point(&h, Hit::RollRight);
    h.click(p);
    h.frames(12);
    let up = h.app.state.active().camera.up();
    assert!(up.z.abs() < 1e-3, "rolled a quarter turn: up {up}");
    assert_eq!(h.app.state.active().kind, loom_commands::ViewKind::Right, "a rolled face view keeps its name");
    // Twice round to the back (the tutorial's two clicks).
    h.cmd(Command::FrontView);
    for _ in 0..2 {
        pause(&mut h);
        let p = cube_point(&h, Hit::Adjacent(Side::Left));
        h.click(p);
        h.frames(12);
    }
    assert_eq!(h.app.state.active().kind, loom_commands::ViewKind::Back);
}

#[test]
fn dragging_the_viewcube_orbits_and_turns_an_axis_view_orthographic() {
    let mut h = Harness::new();
    h.cmd(Command::ActiveViewport { index: 0 });
    maximized(&mut h);
    let c = cube_rect(&h).center();
    let before = h.app.state.active().camera.clone();
    h.drag(c, c + egui::vec2(40.0, 0.0));
    let after = h.app.state.active().camera.clone();
    assert_eq!(h.app.state.active().kind, loom_commands::ViewKind::Orthographic);
    assert!((after.yaw - before.yaw).abs() > 0.1, "the drag orbited: {} -> {}", before.yaw, after.yaw);
    assert!(h.app.state.selection.is_empty());
}

#[test]
fn viewcube_menu_sets_the_home_view_and_alt_ctrl_v_hides_the_cube() {
    let mut h = Harness::new();
    maximized(&mut h);
    h.cmd(Command::Orbit { yaw: Some(20.0), pitch: Some(10.0), distance: Some(80.0), target: None, roll: None });
    let c = cube_rect(&h).center();
    right_click(&mut h, c);
    assert!(h.app.quad_menu.is_none(), "the cube's own menu, not the quad menu");
    let screen = h.ctx.screen_rect();
    click_text(&mut h, "    Set Current View as Home", screen, 0.0);
    assert!(h.app.state.home_view.is_some_and(|v| (v.distance - 80.0).abs() < 1e-3));
    h.mods = egui::Modifiers { alt: true, ctrl: true, ..Default::default() };
    h.key(egui::Key::V);
    h.mods = egui::Modifiers::NONE;
    assert!(!h.app.state.viewcube.show);
    let kind = h.app.state.active().kind;
    let cam = h.app.state.active().camera.clone();
    h.click(c);
    h.frames(12);
    assert_eq!(h.app.state.active().kind, kind, "a hidden cube does not change the view");
    assert_eq!(h.app.state.active().camera.yaw, cam.yaw);
}

#[test]
fn orbit_mode_outside_the_trackball_rolls_the_view() {
    let mut h = Harness::new();
    maximized(&mut h);
    h.cmd(Command::OrbitViewMode);
    let r = h.app.viewport_rects[h.app.state.active_viewport];
    let (c, rad) = (r.center(), r.width().min(r.height()) * 0.375);
    let (yaw, pitch) = (h.app.state.active().camera.yaw, h.app.state.active().camera.pitch);
    // From the right of the circle, round toward the top, outside it.
    let m = h.mods;
    let at = |a: f32| c + egui::vec2(a.cos(), -a.sin()) * (rad + 40.0);
    h.run(vec![egui::Event::PointerMoved(at(0.0))]);
    h.run(vec![egui::Event::PointerButton { pos: at(0.0), button: egui::PointerButton::Primary, pressed: true, modifiers: m }]);
    for i in 1..=8 {
        h.run(vec![egui::Event::PointerMoved(at(i as f32 / 8.0 * 0.8))]);
    }
    h.run(vec![egui::Event::PointerButton { pos: at(0.8), button: egui::PointerButton::Primary, pressed: false, modifiers: m }]);
    h.frames(1);
    let cam = &h.app.state.active().camera;
    assert!(cam.roll.abs() > 0.5, "rolled: {}", cam.roll);
    assert_eq!((cam.yaw, cam.pitch), (yaw, pitch));
}

#[test]
fn i_pans_the_point_under_the_cursor_to_the_centre() {
    let mut h = Harness::new();
    h.cmd(Command::ActiveViewport { index: 0 }); // Top
    maximized(&mut h);
    let p = h.screen_of(Vec3::new(2.0, 1.0, 0.0));
    h.run(vec![egui::Event::PointerMoved(p)]);
    h.key(egui::Key::I);
    let t = h.app.state.active().camera.target;
    assert!((t.x - 2.0).abs() < 0.05 && (t.y - 1.0).abs() < 0.05, "{t}");
    assert_eq!(h.app.state.nav_mode, loom_input::NavMode::None, "I recentres; it does not start the Pan tool");
}

#[test]
fn camera_view_swaps_the_navigation_buttons_and_truck_moves_the_camera_with_one_undo() {
    let mut h = Harness::new();
    let r = h.app.dispatch(Command::CreateCameraAt { target: true, position: [0.0, -100.0, 20.0], look_at: [0.0, 0.0, 20.0] }).unwrap();
    let cam = loom_scene::NodeId(r["id"].as_u64().unwrap());
    h.cmd(Command::CameraView);
    maximized(&mut h);
    h.cmd(Command::SelectNone);
    use egui::accesskit::Role;
    assert!(h.has(Role::Button, "Truck Camera Mode") && h.has(Role::Button, "Roll Camera Mode"), "camera viewport controls");
    assert!(!h.has(Role::Button, "Pan View"));
    h.click(h.find(Role::Button, "Truck Camera Mode").center());
    assert_eq!(h.app.state.nav_mode, loom_input::NavMode::TruckCamera);
    let eye = |h: &Harness| h.app.state.scene.world_matrix(cam).w_axis.truncate();
    let before = eye(&h);
    let c = h.app.viewport_rects[h.app.state.active_viewport].center() + egui::vec2(0.0, 80.0);
    h.drag(c, c + egui::vec2(-60.0, 0.0));
    let after = eye(&h);
    assert!((after.x - before.x).abs() > 1.0 && (after.y - before.y).abs() < 1e-3, "trucked sideways: {before} -> {after}");
    h.cmd(Command::UndoSceneOperation);
    assert!(eye(&h).distance(before) < 1e-3, "the whole drag is one undo step");
}

fn customize_ui_rect(h: &Harness) -> egui::Rect {
    h.ctx.memory(|m| m.area_rect(egui::Id::new("Customize User Interface"))).expect("the Customize User Interface dialog is shown")
}

/// Customize User Interface > Keyboard: pick an action, press a key in the
/// Hotkey field (it names the action that has it), Assign, Remove.
#[test]
fn keyboard_panel_assigns_a_pressed_hotkey_and_names_its_current_owner() {
    let mut h = Harness::new();
    h.cmd(Command::CustomizeUserInterface);
    let win = customize_ui_rect(&h);
    click_text(&mut h, "Search:", win, 40.0);
    h.run(vec![egui::Event::Text("and Move".into())]);
    h.frames(2);
    let win = customize_ui_rect(&h);
    click_text(&mut h, "Select and Move", win, 0.0);
    let hotkey = h.text_rect("Hotkey:", win).expect("the Hotkey field's label");
    h.click(egui::pos2(hotkey.left() + 40.0, hotkey.bottom() + 14.0));
    h.frames(1);
    assert!(h.ctx.memory(|m| m.has_focus(egui::Id::new(crate::dialogs::customize_ui::HOTKEY_FIELD))), "the click focuses the Hotkey field");
    h.key(egui::Key::E);
    assert_eq!(h.app.state.active_tool, loom_commands::Tool::Select, "the key went to the field, not to Select and Rotate");
    assert!(h.text_rect("Select and Rotate", win).is_some(), "Assigned to: names the action E runs now");
    h.mods = egui::Modifiers::ALT | egui::Modifiers::SHIFT;
    h.key(egui::Key::M);
    h.mods = egui::Modifiers::NONE;
    assert!(h.text_rect("Alt+Shift+M", win).is_some(), "the field shows the pressed chord");
    let alt_shift_m = loom_commands::parse_chord("Alt+Shift+M").unwrap();
    assert_eq!(h.app.state.hotkey(alt_shift_m), None);
    click_text(&mut h, "Assign", win, 0.0);
    assert_eq!(h.app.state.hotkey(alt_shift_m), Some("Select and Move"));
    assert!(h.text_rect("Alt+Shift+M, W", customize_ui_rect(&h)).is_some(), "the action list shows both keys");
    // The new key works in the viewports once the dialog is closed.
    h.cmd(Command::CloseDialog);
    h.mods = egui::Modifiers::ALT | egui::Modifiers::SHIFT;
    h.key(egui::Key::M);
    h.mods = egui::Modifiers::NONE;
    assert_eq!(h.app.state.active_tool, loom_commands::Tool::Move);
    h.cmd(Command::CustomizeUserInterface);
    h.frames(2);
    let win = customize_ui_rect(&h);
    click_text(&mut h, "Remove", win, 0.0);
    assert_eq!(h.app.state.hotkey(loom_commands::parse_chord("W").unwrap()), None, "Remove clears every key of the action");
    assert_eq!(h.app.state.hotkey(alt_shift_m), None);
}

/// The settings file is written when a setting changes and read back by the next start.
#[test]
fn settings_are_written_on_change_and_read_at_the_next_start() {
    let dir = std::env::temp_dir().join("loom-ui-settings-test");
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("settings.json");
    let mut h = Harness::new();
    h.app.use_settings_file(path.clone());
    h.frames(1);
    assert!(!path.exists(), "nothing changed, nothing written");
    h.cmd(Command::SetPreference { name: "Gizmo Size".into(), value: 33.0 });
    h.cmd(Command::ShowMainToolbarToggle);
    h.cmd(Command::AssignShortcut { group: "Main UI".into(), action: "Select and Move".into(), chord: "Alt+Shift+M".into() });
    assert!(path.exists(), "a change writes the file");
    let mut next = Harness::new();
    next.app.use_settings_file(path);
    next.frames(2);
    assert_eq!(next.app.state.prefs.gizmos.size, 33.0);
    assert!(!next.app.state.show_main_toolbar);
    next.mods = egui::Modifiers::ALT | egui::Modifiers::SHIFT;
    next.key(egui::Key::M);
    assert_eq!(next.app.state.active_tool, loom_commands::Tool::Move, "the saved shortcut works after a restart");
}

/// Customize > Show UI > Show Command Panel hides the panel and the viewports take its room.
#[test]
fn hiding_the_command_panel_widens_the_viewports() {
    let mut h = Harness::new();
    let right = |h: &Harness| h.app.viewport_rects.iter().map(|r| r.max.x).fold(0.0, f32::max);
    let before = right(&h);
    h.cmd(Command::ShowCommandPanelToggle);
    assert!(right(&h) > before + 100.0, "{} -> {}", before, right(&h));
    h.cmd(Command::ShowCommandPanelToggle);
    assert!((right(&h) - before).abs() < 1.0);
}

/// `loom scene.loom` opens the scene (flags are skipped) and lists it under File > Recent.
#[test]
fn a_scene_named_on_the_command_line_opens() {
    let dir = std::env::temp_dir().join("loom-ui-command-line-test");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("start.loom");
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    h.cmd(Command::SaveFile { path: Some(path.to_string_lossy().into_owned()) });
    let mut next = Harness::new();
    next.app.open_command_line(["-q".to_owned(), path.to_string_lossy().into_owned()]);
    next.frames(2);
    assert_eq!(next.app.state.scene.len(), 1);
    assert_eq!(next.app.state.file.as_deref(), Some(path.as_path()));
    assert_eq!(next.app.state.recent_files.first(), Some(&path));
}

/// A 30-unit box with an Edit Normals modifier (at its Normal level), framed.
fn normals_box(h: &mut Harness) -> loom_scene::NodeId {
    // Tall enough for the whole Modify panel.
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 2400.0));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [30.0; 3] });
    let id = selected(h)[0];
    h.cmd(Command::AddModifier { name: "Edit Normals".into() });
    h.app.panel_tab = crate::Tab::Modify;
    framed(h);
    id
}

/// Each normal's (world start, direction), by id.
fn normal_lines(h: &Harness) -> Vec<(u32, Vec3, Vec3)> {
    let (id, index) = loom_commands::normals::entry(&h.app.state).unwrap();
    let input = loom_commands::normals::input_mesh(&h.app.state, id, index);
    let loom_scene::Modifier::EditNormals(d) = &h.app.state.scene.node(id).unwrap().modifiers[index].modifier else { panic!() };
    let groups = d.groups_for(&input);
    let all = d.normals(&input, &groups);
    let world = h.app.state.scene.object_matrix(id);
    let mut out: Vec<(u32, Vec3, Vec3)> = Vec::new();
    for (i, g) in groups.iter().enumerate() {
        if *g != loom_scene::edit_normals::NO_GROUP && !out.iter().any(|o| o.0 == *g) {
            let p = world.transform_point3(input.vertex_position(input.half_edges()[i].origin).unwrap());
            out.push((*g, p, world.transform_vector3(all[g]).normalize()));
        }
    }
    out
}

/// Screen points 60% along the normals that face the viewer, well apart from every other normal's line.
fn clear_normal_points(h: &Harness) -> Vec<(u32, egui::Pos2)> {
    let eye = h.app.camera_for(h.app.state.active_viewport).view_matrix().inverse().w_axis.truncate();
    let lines = normal_lines(h);
    let seg: Vec<(u32, egui::Pos2, egui::Pos2)> = lines.iter().map(|(g, p, d)| (*g, h.screen_of(*p), h.screen_of(*p + *d * 10.0))).collect();
    let dist = |q: egui::Pos2, a: egui::Pos2, b: egui::Pos2| {
        let ab = b - a;
        let t = if ab.length_sq() < 1e-6 { 0.0 } else { ((q - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) };
        q.distance(a + ab * t)
    };
    lines
        .iter()
        .filter(|(_, p, d)| d.dot(eye - *p) > 0.0)
        .map(|(g, p, d)| (*g, h.screen_of(*p + *d * 6.0)))
        .filter(|(g, s)| seg.iter().all(|(o, a, b)| o == g || dist(*s, *a, *b) > 12.0))
        .filter(|(_, s)| h.app.viewport_rects[h.app.state.active_viewport].shrink(20.0).contains(*s))
        .collect()
}

fn normal_selection(h: &Harness) -> Vec<u32> {
    let st = loom_commands::normals::state_json(&h.app.state);
    st["selected"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u32).collect()
}

/// Line segments painted last frame in `color`.
fn segments_coloured(h: &Harness, color: egui::Color32) -> usize {
    fn walk(s: &egui::Shape, c: egui::Color32, n: &mut usize) {
        match s {
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, c, n)),
            egui::Shape::LineSegment { stroke, .. } if stroke.color == c => *n += 1,
            _ => {}
        }
    }
    let mut n = 0;
    h.shapes.iter().for_each(|c| walk(&c.shape, color, &mut n));
    n
}

#[test]
fn edit_normals_click_ctrl_alt_and_empty_click_select_normals() {
    let mut h = Harness::new();
    let id = normals_box(&mut h);
    let pts = clear_normal_points(&h);
    assert!(pts.len() >= 2, "visible normals to click: {pts:?}");
    let (a, b) = (pts[0], pts[1]);
    h.click(a.1);
    assert_eq!(normal_selection(&h), vec![a.0], "a click selects the normal");
    assert!(h.app.state.selection.contains(&id), "the object stays selected");
    panel_text(&h, &format!("Normal {} Selected", a.0 + 1));
    h.mods = egui::Modifiers::CTRL;
    h.click(b.1);
    h.mods = egui::Modifiers::NONE;
    let mut both = vec![a.0, b.0];
    both.sort();
    assert_eq!(normal_selection(&h), both, "Ctrl adds");
    panel_text(&h, "2 Normals Selected");
    h.mods = egui::Modifiers::ALT;
    h.click(a.1);
    h.mods = egui::Modifiers::NONE;
    assert_eq!(normal_selection(&h), vec![b.0], "Alt removes");
    let rect = h.app.viewport_rects[h.app.state.active_viewport];
    h.click(egui::pos2(rect.right() - 60.0, rect.bottom() - 60.0));
    assert!(normal_selection(&h).is_empty(), "an empty click clears the normals");
    assert!(h.app.state.selection.contains(&id), "and keeps the object");
    // A region selects the normals whose lines it holds.
    h.drag(a.1 - egui::vec2(10.0, 10.0), a.1 + egui::vec2(10.0, 10.0));
    assert!(normal_selection(&h).contains(&a.0), "region picks {:?}", normal_selection(&h));
}

#[test]
fn edit_normals_draws_explicit_normals_green_and_the_rotate_tool_turns_them() {
    let mut h = Harness::new();
    let id = normals_box(&mut h);
    h.frames(1);
    assert_eq!(segments_coloured(&h, crate::poly_modes::NORMAL_EXPLICIT), 0, "no explicit normals yet");
    assert!(segments_coloured(&h, crate::poly_modes::NORMAL_UNSPECIFIED) >= 20, "unspecified normals blue");
    let (g, at) = clear_normal_points(&h)[0];
    h.click(at);
    assert_eq!(normal_selection(&h), vec![g]);
    assert_eq!(segments_coloured(&h, crate::poly_modes::NORMAL_SELECTED), 1, "the selected normal red");
    let before = normal_lines(&h).into_iter().find(|l| l.0 == g).unwrap().2;
    let world = h.app.state.scene.world_matrix(id);
    // Select And Rotate (E is Make Explicit at the Normal level, so the toolbar
    // command): drag on an axis ring of the gizmo, which sits on the normal.
    h.cmd(Command::SelectAndRotate);
    let i = h.app.state.active_viewport;
    let rect = h.app.viewport_rects[i];
    let cam = h.app.camera_for(i);
    let projection = crate::gizmo::Projection::new(cam.view_matrix(), cam.projection_matrix(rect.width() / rect.height()), rect);
    let layout = crate::gizmo::layout_for(&h.app, i, rect.height());
    let o = projection.screen(layout.origin).unwrap();
    assert!(o.distance(h.screen_of(normal_lines(&h).into_iter().find(|l| l.0 == g).unwrap().1)) < 1.0, "the gizmo is on the normal");
    let start = (0..360)
        .flat_map(|deg| (10..800).step_by(2).map(move |r| (deg as f32, r as f32)))
        .map(|(deg, r)| o + egui::vec2(deg.to_radians().cos(), deg.to_radians().sin()) * r)
        .find(|p| matches!(crate::gizmo::hit(&h.app, &layout, &projection, *p), Some(crate::gizmo::Handle::Axis(_))))
        .expect("an axis ring on screen");
    let tangent = (start - o).normalized().rot90() * 40.0;
    h.drag(start, start + tangent);
    let after = normal_lines(&h).into_iter().find(|l| l.0 == g).unwrap().2;
    assert!(after.distance(before) > 0.05, "the normal turned: {before} -> {after}");
    assert_eq!(h.app.state.scene.world_matrix(id), world, "the object did not rotate");
    assert_eq!(normal_selection(&h), vec![g]);
    // Deselect: the turned normal is now drawn green (explicit).
    h.cmd(Command::SelectObject);
    h.click(egui::pos2(rect.right() - 60.0, rect.bottom() - 60.0));
    assert_eq!(segments_coloured(&h, crate::poly_modes::NORMAL_EXPLICIT), 1, "the rotated normal is explicit");
}

#[test]
fn hsds_element_button_shows_the_yellow_cage_and_the_level_list_displays_a_level() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 2400.0));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [30.0; 3] });
    let id = selected(&h)[0];
    h.cmd(Command::AddModifier { name: "HSDS".into() });
    h.app.panel_tab = crate::Tab::Modify;
    framed(&mut h);
    assert_eq!(segments_coloured(&h, crate::poly_modes::HSDS_CAGE), 0, "no cage at the top level");
    // The rollout's Element button (the stack lists the levels too, above it).
    let right = h.app.viewport_rects[h.app.state.active_viewport].right();
    let element = h.text_centers("Element").into_iter().filter(|p| p.x > right).max_by(|a, b| a.y.total_cmp(&b.y)).expect("Element");
    h.click(element);
    assert_eq!(h.app.state.scene.node(id).unwrap().modifiers[0].sub_level, 4, "the rollout's Element button");
    assert_eq!(segments_coloured(&h, crate::poly_modes::HSDS_CAGE), 12, "the box's 12 cage edges, yellow");
    h.cmd(Command::SelectSub { node: None, ids: vec![0], op: loom_commands::SelectOp::Replace });
    h.click(panel_text(&h, "Subdivide"));
    let level1 = h.app.state.scene.eval_mesh(id).value.mesh.face_count();
    assert!(level1 > 6, "subdivided");
    panel_text(&h, "Level 1");
    // Display next to Base Level: Level 1 is turned off.
    let base = panel_text(&h, "Base Level");
    let display = h.text_centers("Display").into_iter().find(|p| (p.y - base.y).abs() < 4.0).expect("Display next to Base Level");
    h.click(display);
    assert_eq!(h.app.state.scene.eval_mesh(id).value.mesh.face_count(), 6, "only the base level shows");
    // Picking Base Level in the list: the cage is the original polygons.
    h.click(base);
    assert_eq!(h.app.state.scene.node(id).unwrap().cage_mesh().face_count(), 6);
}

fn vertex_paint_state(h: &mut Harness) -> serde_json::Value {
    h.app.dispatch(Command::State).unwrap()["vertex_paint"].clone()
}

#[test]
fn vertex_paint_dialog_face_selection_paint_all_new_layer_and_eyedropper() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 2400.0));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [30.0; 3] });
    let id = selected(&h)[0];
    h.cmd(Command::AddModifier { name: "VertexPaint".into() });
    h.app.panel_tab = crate::Tab::Modify;
    framed(&mut h);
    for (n, v) in [("Color R", 1.0), ("Color G", 0.0), ("Color B", 0.0)] {
        h.cmd(Command::SetModifierParameter { index: 0, name: n.into(), value: v });
    }
    // The floating dialog opens with the modifier.
    h.click(window_text(&h, "VertexPaint", "Face Selection"));
    assert!(h.app.state.vertex_paint_faces);
    h.click(h.screen_of(top_face_point()));
    let st = vertex_paint_state(&mut h);
    assert_eq!(st["faces"].as_array().unwrap().len(), 1, "a click selects the face under it: {st}");
    assert_eq!(segments_coloured(&h, crate::poly_modes::VERTEX_PAINT_FACE), 4, "the selected quad outlined");
    h.click(window_text(&h, "VertexPaint", "Paint All"));
    let m = h.app.state.scene.eval_mesh(id).value.mesh.clone();
    for v in m.vertices() {
        let red = v.color[0] > 0.99 && v.color[1] < 0.01;
        assert_eq!(red, v.position.z > 29.9, "only the top face's corners red: {v:?}");
    }
    // Layer > New Layer > OK adds a second VertexPaint.
    h.click(window_text(&h, "VertexPaint", "New Layer"));
    h.click(window_text(&h, "New Layer", "OK"));
    let n = h.app.state.scene.node(id).unwrap();
    assert_eq!(n.modifiers.len(), 2);
    assert_eq!(n.active_entry, Some(1));
    // The eyedropper takes the red from the top face into the new layer's swatch.
    h.cmd(Command::SetModifierParameter { index: 1, name: "Color R".into(), value: 0.0 });
    h.cmd(Command::SetModifierParameter { index: 1, name: "Color G".into(), value: 1.0 });
    h.click(window_text(&h, "VertexPaint", "Pick Color"));
    assert!(h.app.state.vertex_paint_pick && !h.app.state.vertex_paint_faces, "one mode at a time");
    let corner = h.screen_of(Vec3::new(14.0, 14.0, 30.0));
    h.drag(corner - egui::vec2(3.0, 3.0), corner);
    let c = vertex_paint_state(&mut h)["color"].clone();
    assert!(c[0].as_f64().unwrap() > 0.9 && c[1].as_f64().unwrap() < 0.1, "picked red: {c}");
    // Closed (its close box), the dialog stays hidden until the rollout's Edit button.
    h.cmd(Command::VertexPaintPickColorToggle);
    crate::dialogs::vertex_paint::set_hidden(&h.ctx, true);
    h.frames(2);
    assert!(h.text_centers("Paint All").is_empty(), "hidden");
    h.click(panel_text(&h, "Edit"));
    h.frames(1);
    assert_eq!(h.text_centers("Paint All").len(), 1, "Edit opens it again");
}

/// A scratch folder for the file-dialog tests.
fn exchange_dir(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join("loom-ui-exchange").join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// The rect of an FBX / OBJ options window (by its id).
fn options_rect(h: &Harness, id: &str) -> egui::Rect {
    h.ctx.memory(|m| m.area_rect(egui::Id::new(id))).unwrap_or_else(|| panic!("the {id} window is shown"))
}

/// The texts painted inside `within` last frame, with their rects (for failure messages).
fn texts_in(h: &Harness, within: egui::Rect) -> Vec<(String, egui::Rect)> {
    fn walk(shape: &egui::Shape, within: egui::Rect, out: &mut Vec<(String, egui::Rect)>) {
        match shape {
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, within, out)),
            egui::Shape::Text(t) => {
                let r = t.galley.rect.translate(t.pos.to_vec2());
                if within.intersects(r) {
                    out.push((t.galley.text().to_owned(), r));
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for c in &h.shapes {
        walk(&c.shape, within, &mut out);
    }
    out
}

/// Clicks a control of an options window, scrolling its options down to it
/// first; says what the window shows if it is not there.
fn click_option(h: &mut Harness, label: &str, id: &str) {
    let win = options_rect(h, id);
    for _ in 0..12 {
        if h.text_rect(label, win).is_some() {
            break;
        }
        let m = h.mods;
        h.run(vec![egui::Event::PointerMoved(win.center()), egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: egui::vec2(0.0, -120.0), modifiers: m }]);
        // Let the scroll settle before looking again.
        h.frames(20);
    }
    if h.text_rect(label, win).is_none() {
        panic!("{label:?} is not in {id} {win:?}: {:?}", texts_in(h, win));
    }
    click_text(h, label, win, 0.0);
}

/// Types `path` into the in-app file dialog's File name field and clicks `button`.
fn choose_file(h: &mut Harness, path: &std::path::Path, button: &str) {
    click_text(h, "File name:", SCREEN, 40.0);
    // Replace the offered name.
    h.run(vec![egui::Event::Key { key: egui::Key::A, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::COMMAND }]);
    h.run(vec![egui::Event::Text(path.to_string_lossy().into_owned())]);
    click_text(h, button, SCREEN, 0.0);
}

/// File > Export to .fbx shows the FBX Export options; a click on Triangulate
/// and OK write a triangulated file.
#[test]
fn fbx_export_goes_through_its_options_dialog() {
    let mut h = Harness::new();
    h.app.native_file_dialogs = false;
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    let path = exchange_dir("fbx_export").join("box.fbx");
    h.cmd(Command::ExportFile { path: None });
    choose_file(&mut h, &path, "Save");
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::FbxExportOptions), "the options come before the file is written");
    assert!(!path.exists());
    assert!(!h.app.state.exchange.fbx_export.triangulate);
    let win = options_rect(&h, "fbx_export_options");
    click_text(&mut h, "Triangulate", win, 0.0);
    assert!(h.app.state.exchange.fbx_export.triangulate, "the check box sets the option");
    assert!(h.text_rect("User defined", SCREEN).is_some(), "the preset shows it was changed");
    click_option(&mut h, "OK", "fbx_export_options");
    assert_eq!(h.app.state.dialog, None);
    assert!(path.exists(), "OK wrote the file ({})", h.app.state.status_prompt);
    let doc = loom_fbx::read(&std::fs::read(&path).unwrap(), 0.01).unwrap();
    let loom_fbx::FbxKind::Mesh { mesh, .. } = &doc.nodes[0].kind else { panic!("{:?}", doc.nodes[0].kind) };
    assert_eq!(mesh.face_count(), 12, "triangulated");
}

/// File > Import of an .fbx: Cancel in the FBX Import options imports nothing;
/// with Cameras off, OK imports the meshes without the camera.
#[test]
fn fbx_import_options_cancel_and_cameras_off() {
    let mut h = Harness::new();
    h.app.native_file_dialogs = false;
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] });
    h.cmd(Command::CreateCamera { target: false });
    let path = exchange_dir("fbx_import").join("scene.fbx");
    h.cmd(Command::ExportFile { path: Some(path.to_string_lossy().into_owned()) });
    h.cmd(Command::NewScene);
    h.cmd(Command::ImportFile { path: None });
    choose_file(&mut h, &path, "Open");
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::FbxImportOptions));
    click_option(&mut h, "Cancel", "fbx_import_options");
    assert_eq!(h.app.state.dialog, None);
    assert_eq!(h.app.state.scene.len(), 0, "Cancel imports nothing");
    h.cmd(Command::ImportFile { path: None });
    choose_file(&mut h, &path, "Open");
    let win = options_rect(&h, "fbx_import_options");
    click_option(&mut h, "Cameras", "fbx_import_options");
    assert!(!h.app.state.exchange.fbx_import.cameras);
    click_option(&mut h, "OK", "fbx_import_options");
    let kinds: Vec<bool> = h.app.state.scene.nodes().map(|n| matches!(n.object, loom_scene::ObjectKind::Camera { .. })).collect();
    assert_eq!(kinds, vec![false], "the box only");
}

/// File > Import of an .obj: the OBJ Import options list the file's objects;
/// unchecking one leaves it out.
#[test]
fn obj_import_options_pick_the_objects() {
    let mut h = Harness::new();
    h.app.native_file_dialogs = false;
    let path = exchange_dir("obj_import").join("two.obj");
    std::fs::write(&path, "o Left\nv 0 0 0\nv 1 0 0\nv 1 1 0\nf 1 2 3\no Right\nv 5 0 0\nv 6 0 0\nv 6 1 0\nf 4 5 6\n").unwrap();
    h.cmd(Command::ImportFile { path: None });
    choose_file(&mut h, &path, "Open");
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::ObjImportOptions));
    let win = options_rect(&h, "obj_import_options");
    assert!(h.text_rect("Left (1)", win).is_some() && h.text_rect("Right (1)", win).is_some(), "the file's objects are listed");
    click_text(&mut h, "Right (1)", win, 0.0);
    click_text(&mut h, "Import", win, 0.0);
    assert_eq!(h.app.state.dialog, None);
    let names: Vec<String> = h.app.state.scene.nodes().map(|n| n.name.clone()).collect();
    assert_eq!(names, vec!["Left".to_string()]);
}

/// At a sub-object level the first empty click clears the sub-objects and
/// keeps the object; a second one lets go of the object and ends the level.
#[test]
fn a_second_empty_click_at_polygon_level_releases_the_object_and_the_level() {
    let mut h = Harness::new();
    poly_plane(&mut h);
    h.key(egui::Key::Num4);
    h.cmd(Command::SelectAll);
    assert!(h.sub_count() > 0);
    let p = empty_point(&h);
    h.click(p);
    h.frames(10);
    assert_eq!(h.sub_count(), 0, "the first click clears the polygons");
    assert_eq!(h.app.state.selection.len(), 1, "and keeps the object");
    assert_eq!(h.app.state.selection_mode, SelectionMode::Face);
    h.click(p);
    h.frames(10);
    assert!(h.app.state.selection.is_empty(), "the second click lets go of the object");
    assert_eq!(h.app.state.selection_mode, SelectionMode::Object, "and ends Polygon level");
}

mod qa;
/// A Checker map with Show Map in Viewport on a Multi/Sub sub-material shows
/// its checks in the shaded viewport (the faces' UVs vary and the texture
/// holds both colours), as the screenshot scene sets it up.
#[test]
fn a_checker_sub_material_shows_checks_in_the_viewport() {
    let mut h = Harness::new();
    // With the editor open, as a person does it.
    material_editor(&mut h);
    let node = *h.app.state.selection.iter().next().unwrap();
    for c in [
        Command::SetMaterialType { kind: "Multi/Sub-Object".into(), keep_old: Some(false) },
        Command::MultiSubSetNumber { number: 3 },
        Command::MultiSubSetEntry { index: 0, id: None, name: Some("Red".into()), enabled: None, color: Some([200, 30, 30]) },
        Command::MaterialEditorGoToSub { index: 2 },
        Command::NewMap { map: "Diffuse Color".into(), kind: "Checker".into() },
        Command::SetMapParameter { name: "Tiling U".into(), value: 3.0 },
        Command::SetMapParameter { name: "Tiling V".into(), value: 3.0 },
        Command::SetMapColor { which: "Color #2".into(), rgb: [240, 200, 40] },
        Command::MaterialEditorGoToParent,
        Command::SetMaterialParameter { slot: None, name: "Show Map in Viewport".into(), value: 1.0 },
        Command::MaterialEditorGoToParent,
        Command::AssignMaterialToSelection,
        Command::SelectionMode { mode: loom_commands::SelectionMode::Face },
        Command::SelectSub { node: Some(node.0), ids: vec![2, 3], op: loom_commands::SelectOp::Replace },
        Command::SetMaterialId { id: 3 },
        Command::SelectionMode { mode: loom_commands::SelectionMode::Object },
        Command::UvwMapModifier,
    ] {
        h.cmd(c);
    }
    let style = h.app.world_styles[0].clone();
    let sub = style.sub.as_ref().expect("per-ID styles");
    let checker = sub.by_id[&2];
    let loom_render::TextureSel::Bitmap(i) = checker.texture else { panic!("ID 3 shows its map: {:?}", checker.texture) };
    let mesh = h.app.world_display()[0].clone();
    let (verts, _, _) = loom_render::triangulate_styled(&mesh, &style);
    let mut first = 0usize;
    let mut uvs = Vec::new();
    for (f, face) in mesh.faces().iter().enumerate().filter(|(_, f)| !f.hidden) {
        let n = mesh.face_vertex_ids(loom_mesh::FaceId(f as u32)).unwrap().len();
        if face.material_id == 2 {
            uvs.extend(verts[first..first + n].iter().map(|v| v.uv));
        }
        first += n;
    }
    assert!(uvs.iter().any(|u| *u != uvs[0]), "the checker faces have varying UVs: {uvs:?}");
    let upload = h.app.pending_upload.lock().unwrap().clone();
    let bitmaps = upload.map(|u| u.2).unwrap_or_default();
    let bm = &bitmaps.get(i as usize).expect("the baked checker is uploaded").1;
    let px: std::collections::BTreeSet<[u8; 3]> = bm.rgba.chunks(4).map(|p| [p[0], p[1], p[2]]).collect();
    assert!(px.contains(&[0, 0, 0]) && px.contains(&[240, 200, 40]), "both checker colours: {} colours", px.len());
    // Each checker face shows both colours: sample the texture as the shader
    // does (repeat, V flipped) over the face's UV quad.
    let at = |u: f32, v: f32| {
        let (w, hh) = (bm.width as f32, bm.height as f32);
        let x = ((u.rem_euclid(1.0) * w) as u32).min(bm.width - 1);
        let y = (((1.0 - v).rem_euclid(1.0) * hh) as u32).min(bm.height - 1);
        let i = ((y * bm.width + x) * 4) as usize;
        [bm.rgba[i], bm.rgba[i + 1], bm.rgba[i + 2]]
    };
    for quad in uvs.chunks(4) {
        let mut seen = std::collections::BTreeSet::new();
        for a in 0..=8 {
            for b in 0..=8 {
                let (s, t) = (a as f32 / 8.0, b as f32 / 8.0);
                let lerp = |p: [f32; 2], q: [f32; 2], k: f32| [p[0] + (q[0] - p[0]) * k, p[1] + (q[1] - p[1]) * k];
                let uv = lerp(lerp(quad[0], quad[1], s), lerp(quad[3], quad[2], s), t);
                seen.insert(at(uv[0], uv[1]));
            }
        }
        assert!(seen.len() > 1, "a checker face shows one colour; its UVs: {quad:?}");
    }
}
mod scene;
