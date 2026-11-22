//! The parity33 round.

use super::*;
use egui::accesskit::Role;

/// The topmost entry `label` of an open popup menu.
fn menu_entry(h: &Harness, label: &str) -> egui::Pos2 {
    let open = menus(h);
    let all = h.find_all(Role::Button, label.trim());
    all.iter().copied().find(|r| open.iter().any(|m| m.contains_rect(*r))).unwrap_or_else(|| panic!("no menu entry {label:?} in {open:?}: {all:?}")).center()
}

fn line(h: &mut Harness, pts: &[[f32; 3]]) -> loom_scene::NodeId {
    let r = h.app.dispatch(Command::LineStart { point: pts[0] }).unwrap();
    let id = r["id"].as_u64().unwrap();
    for p in &pts[1..] {
        h.cmd(Command::LineAddKnot { node: id, point: *p, smooth: false });
    }
    h.cmd(Command::LineEnd { node: id });
    loom_scene::NodeId(id)
}

fn window(h: &Harness, id: egui::Id) -> Option<egui::Rect> {
    h.ctx.memory(|m| m.area_rect(id)).filter(|_| h.ctx.memory(|m| m.areas().visible_layer_ids().iter().any(|l| l.id == id)))
}

/// POV label menu > Shape (with a shape selected): the view looks down the
/// shape's local Z with its local Y up, centred on it, labelled Shape;
/// orbiting turns it into an Orthographic view.
#[test]
fn shape_view_from_the_pov_menu_looks_square_onto_the_shape() {
    let mut h = Harness::new();
    maximized(&mut h);
    h.cmd(Command::CreateShape { kind: loom_commands::ShapeKind::Circle, position: [30.0, 10.0, 5.0], size: [40.0, 40.0, 0.0] });
    let id = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::RotateSelection { axis: loom_commands::Axis::X, degrees: 90.0, system: Some(loom_commands::state::CoordSystem::World) });
    h.cmd(Command::RotateSelection { axis: loom_commands::Axis::Z, degrees: 30.0, system: Some(loom_commands::state::CoordSystem::World) });
    h.cmd(Command::SelectNodes { ids: vec![id.0], op: loom_commands::SelectOp::Replace });
    h.click(label_rect(&h, 1).center());
    assert_eq!(h.app.state.selection.len(), 1, "a click on the label keeps the selection");
    h.click(menu_entry(&h, "Shape"));
    let vp = h.app.state.active();
    assert_eq!(vp.kind, loom_commands::ViewKind::Shape);
    assert_eq!(h.app.state.view_label(h.app.state.active_viewport), "Shape");
    assert!(vp.is_orthographic());
    let world = h.app.state.scene.world_matrix(id);
    let (right, up, to_eye) = vp.camera.basis();
    assert!(to_eye.dot(world.transform_vector3(Vec3::Z).normalize()) > 0.999, "looks down the local Z: {to_eye}");
    assert!(up.dot(world.transform_vector3(Vec3::Y).normalize()) > 0.999, "local Y is up: {up}");
    assert!(right.dot(world.transform_vector3(Vec3::X).normalize()) > 0.999, "local X is right: {right}");
    assert!(vp.camera.target.distance(world.transform_point3(Vec3::ZERO)) < 0.5, "centred on the circle: {}", vp.camera.target);
    // The shape fills the view: its edge is on screen, near the border.
    let edge = h.screen_of(world.transform_point3(Vec3::new(20.0, 0.0, 0.0)));
    let rect = h.app.viewport_rects[h.app.state.active_viewport];
    assert!(rect.contains(edge) && edge.x > rect.center().x + rect.width() * 0.1, "zoomed to the shape: {edge:?} in {rect:?}");
    // The POV menu ticks Shape.
    h.click(label_rect(&h, 1).center());
    assert!(h.has(Role::Button, "✔ Shape"), "Shape is ticked");
    h.key(egui::Key::Escape);
    // Alt+middle drag orbits: an Orthographic view now.
    h.mods = egui::Modifiers::ALT;
    let c = rect.center();
    h.drag_with(egui::PointerButton::Middle, c, c + egui::vec2(60.0, 20.0));
    h.mods = egui::Modifiers::NONE;
    assert_eq!(h.app.state.active().kind, loom_commands::ViewKind::Orthographic);
    // Without a shape selected the entry is not offered.
    h.cmd(Command::SelectNone);
    h.click(label_rect(&h, 1).center());
    assert!(!h.has(Role::Button, "    Shape"), "no Shape entry without a selected shape");
}

/// The Viewports quad (V) has Camera (C) and Light ($): C shows the camera,
/// Shift+4 the spot light, in the viewport the quad opened over.
#[test]
fn the_viewports_quad_picks_the_camera_with_c_and_the_light_with_dollar() {
    let mut h = Harness::new();
    h.cmd(Command::CreateCamera { target: false });
    let cam = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::CreateLight { kind: loom_commands::light::LightKind::FreeSpot, position: [0.0, 0.0, 80.0], target: [0.0; 3] });
    let light = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::SelectNone);
    let top = h.app.state.viewports.iter().position(|v| v.kind == loom_commands::ViewKind::Top).unwrap();
    assert_ne!(top, h.app.state.active_viewport, "the Top view is not the active one");
    h.run(vec![egui::Event::PointerMoved(h.app.viewport_rects[top].center())]);
    h.key(egui::Key::V);
    assert!(h.has(Role::Button, "Camera  C") && h.has(Role::Button, "Light  $"), "the entries show with their keys");
    h.key(egui::Key::C);
    assert!(h.app.quad_menu.is_none(), "C closes the quad");
    assert_eq!(h.app.state.viewports[top].kind, loom_commands::ViewKind::Camera(cam), "C looks through the camera in the viewport under the pointer");
    h.key(egui::Key::V);
    h.mods = egui::Modifiers::SHIFT;
    h.key(egui::Key::Num4);
    h.mods = egui::Modifiers::NONE;
    assert!(h.app.quad_menu.is_none());
    assert_eq!(h.app.state.viewports[top].kind, loom_commands::ViewKind::Camera(light), "$ looks through the light");
    // A click on an entry works too.
    h.key(egui::Key::V);
    h.click(h.find(Role::Button, "Camera  C").center());
    assert_eq!(h.app.state.viewports[top].kind, loom_commands::ViewKind::Camera(cam));
}

/// Tools > Selection Floater: Find highlights, Select selects and the
/// floater stays open; By Size sorts by faces; List Types hide rows;
/// Influences adds a target camera's target; Close closes it.
#[test]
fn the_selection_floater_finds_sorts_and_selects_and_stays_open() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1000.0));
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Sphere, position: [0.0; 3], size: [20.0; 3] });
    let sphere = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [40.0, 0.0, 0.0], size: [10.0; 3] });
    let bx = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::SelectNone);
    super::widgets::menu_path(&mut h, &["Tools", "Selection Floater..."]);
    h.frames(2);
    assert!(h.app.state.selection_floater);
    let win = window(&h, egui::Id::new("Selection Floater")).expect("the Selection Floater");
    let name = |h: &Harness, id: loom_scene::NodeId| h.app.state.scene.node(id).unwrap().name.clone();
    let (sname, bname) = (name(&h, sphere), name(&h, bx));
    // Alphabetical: Box before Sphere; By Size: the box (fewer faces) first too, then flip by checking the sphere is below.
    let y = |h: &Harness, n: &str| h.text_rect(n, window(h, egui::Id::new("Selection Floater")).unwrap()).unwrap_or_else(|| panic!("{n} listed")).center().y;
    assert!(y(&h, &bname) < y(&h, &sname), "alphabetical");
    h.click(h.text_rect("By Size", win).unwrap().center());
    assert!(y(&h, &bname) < y(&h, &sname), "the box has fewer faces");
    // Find highlights the names that begin with the text; Select selects them.
    let find = h.find(Role::TextInput, "Selection Floater Find");
    h.click(find.center());
    h.run(vec![egui::Event::Text(sname[..3].to_owned())]);
    h.frames(2);
    h.click(h.find_all(Role::Button, "Select").into_iter().find(|r| window(&h, egui::Id::new("Selection Floater")).unwrap().contains_rect(*r)).unwrap().center());
    assert_eq!(h.app.state.selection.iter().copied().collect::<Vec<_>>(), vec![sphere], "Select selects the found object");
    assert!(h.app.state.selection_floater && window(&h, egui::Id::new("Selection Floater")).is_some(), "the floater stays open");
    // List Types: Geometry off lists no objects.
    let win = window(&h, egui::Id::new("Selection Floater")).unwrap();
    h.click(h.text_rect("Geometry", win).unwrap().center());
    assert!(h.text_rect(&bname, window(&h, egui::Id::new("Selection Floater")).unwrap()).is_none(), "geometry is not listed");
    h.click(h.text_rect("Geometry", win).unwrap().center());
    // Influences: a target camera's target comes along.
    h.cmd(Command::CreateCamera { target: true });
    let cam = h.app.state.scene.nodes().find(|n| matches!(n.object, loom_scene::ObjectKind::Camera { .. })).unwrap().id;
    let target = match &h.app.state.scene.node(cam).unwrap().object {
        loom_scene::ObjectKind::Camera { target, .. } => target.unwrap(),
        _ => unreachable!(),
    };
    h.cmd(Command::SelectNodes { ids: vec![cam.0], op: loom_commands::SelectOp::Replace });
    h.frames(2);
    let win = window(&h, egui::Id::new("Selection Floater")).unwrap();
    h.click(h.text_rect("Display Influences", win).unwrap().center());
    let tname = name(&h, target);
    let blue = h.shapes.iter().any(|s| {
        fn walk(s: &egui::Shape, t: &str) -> bool {
            match s {
                egui::Shape::Vec(v) => v.iter().any(|s| walk(s, t)),
                egui::Shape::Text(x) => x.galley.text() == t && x.galley.job.sections.iter().any(|sec| sec.format.color == crate::dialogs::selection_floater::INFLUENCE_BLUE),
                _ => false,
            }
        }
        walk(&s.shape, &tname)
    });
    assert!(blue, "the camera's target shows in blue");
    h.click(h.find_all(Role::Button, "Influences").into_iter().find(|r| win.contains_rect(*r)).unwrap().center());
    h.click(h.find_all(Role::Button, "Select").into_iter().find(|r| win.contains_rect(*r)).unwrap().center());
    assert_eq!(h.app.state.selection.len(), 2, "Influences highlighted the target");
    assert!(h.app.state.selection.contains(&target));
    h.click(h.find_all(Role::Button, "Close").into_iter().find(|r| win.contains_rect(*r)).unwrap().center());
    assert!(!h.app.state.selection_floater, "Close closes it");
}

/// Utilities > Color Clipboard: New Floater opens a clipboard; a swatch
/// dragged onto another asks Copy or Swap; the floater saves and opens .ccb files.
#[test]
fn the_color_clipboard_copies_by_drag_and_saves_ccb_files() {
    let dir = std::env::temp_dir().join(format!("loom-ui-ccb-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut h = Harness::new();
    h.app.native_file_dialogs = false;
    h.app.panel_tab = crate::Tab::Utilities;
    h.frames(2);
    h.click(h.find(Role::Button, "Color Clipboard").center());
    assert_eq!(h.app.utility, crate::command_panel::utilities::Utility::ColorClipboard, "a default Utilities button");
    h.cmd(Command::ColorClipboardSet { board: 0, slot: 0, rgb: [200, 10, 20] });
    h.click(h.find(Role::Button, "New Floater").center());
    h.frames(2);
    assert_eq!(h.app.state.color_clipboards.boards.len(), 2, "a floater");
    let floater = window(&h, egui::Id::new(("color_clipboard_floater", 1u32))).expect("the floater window");
    // Drag the panel's first swatch onto the floater's second, then Copy.
    let from = h.find(Role::ColorWell, "Color Clipboard 0 Swatch 1");
    let to = h.find_all(Role::ColorWell, "Color Clipboard 1 Swatch 2").into_iter().find(|r| floater.contains_rect(*r)).expect("the floater's swatch");
    h.drag(from.center(), to.center());
    h.frames(2);
    assert!(window(&h, egui::Id::new("color_clipboard_copy_swap")).is_some(), "Copy or Swap asks");
    h.click(h.find(Role::Button, "Copy").center());
    assert_eq!(h.app.state.color_clipboards.boards[&1][1], [200, 10, 20], "copied");
    assert_eq!(h.app.state.color_clipboards.boards[&0][0], [200, 10, 20], "the source keeps its colour");
    // Save... writes the .ccb: one R G B line per swatch.
    let floater = window(&h, egui::Id::new(("color_clipboard_floater", 1u32))).unwrap();
    h.click(h.find_all(Role::Button, "Save...").into_iter().find(|r| floater.contains_rect(*r)).unwrap().center());
    assert_eq!(h.app.state.dialog, Some(loom_commands::Dialog::ColorClipboardFile { board: 1, save: true }));
    let path = dir.join("colors.ccb");
    super::choose_file(&mut h, &path, "Save");
    h.frames(2);
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(text.lines().count(), 12);
    assert_eq!(text.lines().nth(1), Some("200 10 20"));
    // Open... reads one back.
    std::fs::write(&path, text.replacen("200 10 20", "1 2 3", 1)).unwrap();
    let floater = window(&h, egui::Id::new(("color_clipboard_floater", 1u32))).unwrap();
    h.click(h.find_all(Role::Button, "Open...").into_iter().find(|r| floater.contains_rect(*r)).unwrap().center());
    super::choose_file(&mut h, &path, "Open");
    h.frames(2);
    assert_eq!(h.app.state.color_clipboards.boards[&1][1], [1, 2, 3], "opened");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Utilities > More... > Shape Check: Pick Object, then a click on a line
/// that crosses itself shows the crossing as a red box and says so; Close ends it.
#[test]
fn shape_check_picks_a_shape_and_marks_where_it_crosses_itself() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1000.0));
    maximized(&mut h);
    let id = line(&mut h, &[[0.0, 0.0, 0.0], [40.0, 40.0, 0.0], [40.0, 0.0, 0.0], [0.0, 40.0, 0.0]]);
    h.cmd(Command::SelectNone);
    h.cmd(Command::ZoomExtentsAll);
    h.app.panel_tab = crate::Tab::Utilities;
    h.app.utility = crate::command_panel::utilities::Utility::ShapeCheck;
    h.frames(2);
    h.click(h.find(Role::Button, "Pick Object").center());
    assert_eq!(h.app.state.pick_mode, Some(loom_commands::PickMode::ShapeCheck));
    h.click(h.screen_of(Vec3::new(40.0, 20.0, 0.0)));
    assert_eq!(h.app.state.shape_check, Some(id), "the clicked line is checked");
    assert_eq!(h.app.state.pick_mode, None);
    assert!(!h.text_rects("Shape self-intersects (1 point)").is_empty(), "the text says so");
    let red = |h: &Harness| {
        fn walk(s: &egui::Shape) -> usize {
            match s {
                egui::Shape::Vec(v) => v.iter().map(walk).sum(),
                egui::Shape::Rect(r) if r.stroke.color == crate::viewport_widget::SHAPE_CHECK_RED => 1,
                _ => 0,
            }
        }
        h.shapes.iter().map(|s| walk(&s.shape)).sum::<usize>()
    };
    assert_eq!(red(&h), 1, "one red box");
    let at = h.shapes.iter().find_map(|s| {
        fn walk(s: &egui::Shape) -> Option<egui::Pos2> {
            match s {
                egui::Shape::Vec(v) => v.iter().find_map(walk),
                egui::Shape::Rect(r) if r.stroke.color == crate::viewport_widget::SHAPE_CHECK_RED => Some(r.rect.center()),
                _ => None,
            }
        }
        walk(&s.shape)
    });
    assert!(at.unwrap().distance(h.screen_of(Vec3::new(20.0, 20.0, 0.0))) < 2.0, "at the crossing");
    // An untangled line: no crossings.
    let straight = line(&mut h, &[[0.0, 60.0, 0.0], [40.0, 60.0, 0.0], [40.0, 90.0, 0.0]]);
    let r = h.app.dispatch(Command::ShapeCheck { id: Some(straight.0) }).unwrap();
    assert!(r["intersections"].as_array().unwrap().is_empty());
    h.frames(2);
    assert!(!h.text_rects("No self-intersection").is_empty());
    h.click(h.find_all(Role::Button, "Close").into_iter().max_by(|a, b| a.min.y.total_cmp(&b.min.y)).unwrap().center());
    assert_eq!(h.app.state.shape_check, None, "Close ends the check");
    assert_eq!(red(&h), 0);
}
