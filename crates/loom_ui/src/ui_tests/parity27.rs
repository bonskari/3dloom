//! The parity27 round: the dynamic home grid and the default view framing.

use super::*;
use crate::viewport_widget::home_grid_view;
use loom_render::GridPlane;

fn view_index(h: &Harness, kind: loom_commands::ViewKind) -> usize {
    h.app.state.viewports.iter().position(|v| v.kind == kind).expect("a view of that kind")
}

fn wheel_over(h: &mut Harness, index: usize, ticks: i32, delta: f32) {
    let p = h.app.viewport_rects[index].center();
    h.run(vec![egui::Event::PointerMoved(p)]);
    for _ in 0..ticks {
        h.run(vec![egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Line, delta: egui::vec2(0.0, delta), modifiers: egui::Modifiers::NONE }]);
    }
    h.frames(2);
}

#[test]
fn a_fresh_scene_frames_the_perspective_grid_across_its_width_and_shows_grid_squares_in_the_axis_views() {
    let h = Harness::new();
    let i = view_index(&h, loom_commands::ViewKind::Perspective);
    let rect = h.app.viewport_rects[i];
    let cam = h.app.camera_for(i);
    let vp = cam.projection_matrix(rect.width() / rect.height()) * cam.view_matrix();
    let e = h.app.state.home_grid.perspective_cells() as f32 * h.app.state.home_grid.spacing;
    // The 45 degree FOV spans the view's width: the grid's side corners just
    // fit across it and its far corner shows.
    for c in [Vec3::new(-e, e, 0.0), Vec3::new(e, -e, 0.0)] {
        let n = vp.project_point3(c);
        assert!(n.x.abs() < 1.0 && n.x.abs() > 0.9 && n.y.abs() < 1.0, "grid corner {c} at the side of the Perspective view: {n}");
    }
    let n = vp.project_point3(Vec3::new(e, e, 0.0));
    assert!(n.x.abs() < 1.0 && n.y.abs() < 1.0, "the far grid corner shows: {n}");
    for (kind, plane) in [(loom_commands::ViewKind::Top, GridPlane::XY), (loom_commands::ViewKind::Front, GridPlane::XZ), (loom_commands::ViewKind::Left, GridPlane::YZ)] {
        let i = view_index(&h, kind);
        let v = home_grid_view(&h.app, i, h.app.viewport_rects[i], plane);
        assert_eq!((v.level, v.spacing), (0, 10.0), "{kind:?} draws Grid Spacing squares");
        let cam = h.app.camera_for(i);
        let tall = 2.0 / cam.projection_matrix(h.app.viewport_rects[i].aspect_ratio()).y_axis.y;
        assert!(tall / v.spacing > 8.0, "{kind:?} shows several grid squares, not one: {tall} units tall");
    }
}

#[test]
fn zooming_an_axis_view_merges_and_subdivides_the_grid_as_the_inhibit_allows() {
    let mut h = Harness::new();
    let top = view_index(&h, loom_commands::ViewKind::Top);
    let level = |h: &Harness| home_grid_view(&h.app, top, h.app.viewport_rects[top], GridPlane::XY).level;
    wheel_over(&mut h, top, 12, -1.0);
    assert!(level(&h) >= 1, "zoomed out, the major squares become the grid: level {}", level(&h));
    wheel_over(&mut h, top, 40, 1.0);
    assert_eq!(level(&h), 0, "zoomed in, Inhibit Grid Subdivision Below Grid Spacing keeps Grid Spacing");
    h.cmd(Command::HomeGrid { spacing: None, major_every: None, perspective_extent: None, inhibit_subdivision: Some(false), inhibit_resize: None, dynamic_update_all: None });
    assert!(level(&h) < 0, "without the inhibit the squares subdivide: level {}", level(&h));
}

#[test]
fn the_perspective_grid_resizes_with_zoom_only_without_its_inhibit() {
    let mut h = Harness::new();
    let p = view_index(&h, loom_commands::ViewKind::Perspective);
    let level = |h: &Harness| home_grid_view(&h.app, p, h.app.viewport_rects[p], GridPlane::XY).level;
    wheel_over(&mut h, p, 30, -1.0);
    assert_eq!(level(&h), 0, "Inhibit Perspective View Grid Resize keeps one grid at any zoom");
    h.cmd(Command::HomeGrid { spacing: None, major_every: None, perspective_extent: None, inhibit_subdivision: None, inhibit_resize: Some(false), dynamic_update_all: None });
    assert!(level(&h) >= 1, "without it the grid grows as the view zooms out: level {}", level(&h));
}

/// Walkthrough in a camera view: W walks the camera node and a drag turns it
/// in place; with Auto Key on both key the camera.
#[test]
fn walking_a_camera_view_moves_and_keys_the_camera_node() {
    let mut h = Harness::new();
    maximized(&mut h);
    let id = loom_scene::NodeId(h.app.dispatch(Command::CreateCameraAt { target: false, position: [0.0, -100.0, 20.0], look_at: [0.0, 0.0, 20.0] }).unwrap()["id"].as_u64().unwrap());
    h.cmd(Command::ViewportView { index: h.app.state.active_viewport as u32, kind: loom_commands::ViewKind::Camera(id) });
    h.cmd(Command::AutoKeyModeToggle);
    h.cmd(Command::SetFrame { frame: 10 });
    h.cmd(Command::WalkThroughViewMode);
    let eye = |h: &Harness| h.app.state.scene.world_matrix(id).w_axis.truncate();
    let start = eye(&h);
    h.key(egui::Key::W);
    assert!(eye(&h).y > start.y + 1e-3, "W walked the camera forward: {} -> {}", start, eye(&h));
    assert!(h.app.state.scene.node(id).unwrap().transform.position.is_animated(), "and Auto Key keyed it");
    let walked = eye(&h);
    let forward = |h: &Harness| -h.app.state.scene.world_matrix(id).z_axis.truncate().normalize();
    let f0 = forward(&h);
    let r = h.app.viewport_rects[h.app.state.active_viewport];
    h.drag(r.center(), r.center() + egui::vec2(60.0, 0.0));
    assert!((eye(&h) - walked).length() < 1e-2, "the drag turns the camera where it stands: {} -> {}", walked, eye(&h));
    assert!(forward(&h).dot(f0) < 0.9999, "and it looks elsewhere");
}

/// Angle Snap also snaps the FOV drag (and a camera's Roll) to the Options angle.
#[test]
fn angle_snap_snaps_the_field_of_view_drag() {
    let mut h = Harness::new();
    maximized(&mut h);
    h.cmd(Command::FieldOfViewMode);
    h.cmd(Command::AngleSnapToggle);
    let step = h.app.state.snaps.angle_deg;
    let r = h.app.viewport_rects[h.app.state.active_viewport];
    h.drag(r.center(), r.center() + egui::vec2(0.0, 37.0));
    let fov = h.app.state.active().fov_radians.to_degrees();
    assert!((fov - 45.0).abs() > 1e-3, "the drag changed the FOV");
    assert!(((fov / step).round() * step - fov).abs() < 1e-3, "FOV {fov} is a whole number of {step} degree steps");
    // Roll in a camera view: whole steps too.
    let id = loom_scene::NodeId(h.app.dispatch(Command::CreateCameraAt { target: false, position: [0.0, -100.0, 20.0], look_at: [0.0, 0.0, 20.0] }).unwrap()["id"].as_u64().unwrap());
    h.cmd(Command::ViewportView { index: h.app.state.active_viewport as u32, kind: loom_commands::ViewKind::Camera(id) });
    h.cmd(Command::RollCameraMode);
    h.drag(r.center(), r.center() + egui::vec2(43.0, 0.0));
    let up = h.app.state.scene.world_matrix(id).y_axis.truncate().normalize();
    let roll = up.x.atan2(up.z).to_degrees();
    assert!(roll.abs() > 1e-3, "the drag rolled the camera");
    assert!(((roll / step).round() * step - roll).abs() < 1e-2, "roll {roll} is a whole number of {step} degree steps");
}

/// Link Params: pick a link in the list, type its Start Time; the Key Modes
/// radio buttons set what Add Link keys.
#[test]
fn link_params_start_time_and_key_modes_from_the_motion_panel() {
    let mut h = Harness::new();
    h.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 1600.0));
    h.cmd(Command::CreateHelper { kind: "dummy".into(), position: [0.0; 3], size: 10.0 });
    let a = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [30.0, 0.0, 0.0], size: [10.0; 3] });
    let b = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::LinkConstraint { target: None });
    h.cmd(Command::SetFrame { frame: 50 });
    h.cmd(Command::LinkConstraint { target: Some(a.0) });
    h.app.panel_tab = crate::Tab::Motion;
    h.frames(3);
    let name = h.app.state.scene.node(a).unwrap().name.clone();
    h.click(h.widget(&format!("{name}  50")).center());
    let st = h.spinner_after("Start Time:");
    h.click(st.center());
    h.mods = egui::Modifiers::COMMAND;
    h.key(egui::Key::A);
    h.mods = egui::Modifiers::NONE;
    h.run(vec![egui::Event::Text("30".into())]);
    h.key(egui::Key::Enter);
    h.frames(2);
    let l = h.app.state.scene.node(b).unwrap().constraints.link.clone().unwrap();
    assert_eq!(l.links[1], (30.0, Some(a)), "Start Time moved the picked link");
    h.click(h.widget("Key Nodes").center());
    h.frames(1);
    h.click(h.widget("Parents").center());
    h.frames(1);
    let l = h.app.state.scene.node(b).unwrap().constraints.link.clone().unwrap();
    assert_eq!((l.key_mode, l.key_parents), (loom_scene::constraint::LinkKeyMode::KeyNodes, true));
}

/// V opens the Viewports quad at the pointer; a view's letter (K: Back)
/// switches the viewport under it and closes the quad, without the letter's
/// own shortcut (K: Set Key) running.
#[test]
fn v_opens_the_viewports_quad_and_its_letters_pick_a_view() {
    let mut h = Harness::new();
    let top = view_index(&h, loom_commands::ViewKind::Top);
    let at = h.app.viewport_rects[top].center();
    h.run(vec![egui::Event::PointerMoved(at)]);
    h.key(egui::Key::V);
    h.frames(1);
    let open = h.app.quad_menu.expect("V opens a quad menu");
    assert_eq!(open.kind.set_name(&h.app), "Viewports");
    assert!(h.has(egui::accesskit::Role::Button, "Back  K"), "the views show with their letters");
    h.key(egui::Key::K);
    assert!(h.app.quad_menu.is_none(), "the letter closes the quad");
    assert_eq!(h.app.state.viewports[top].kind, loom_commands::ViewKind::Back);
    assert!(h.app.state.scene.nodes().all(|n| !n.transform.position.is_animated()), "K did not Set Key");
    // Clicking an entry works too.
    h.key(egui::Key::V);
    h.frames(1);
    h.click(h.find(egui::accesskit::Role::Button, "Left  L").center());
    assert_eq!(h.app.state.viewports[top].kind, loom_commands::ViewKind::Left);
}

/// Group > Open draws a pink box round the group's members in every
/// viewport (no white brackets for it); Close takes it away.
#[test]
fn an_open_group_shows_a_pink_bounding_box() {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [20.0; 3] });
    let a = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [40.0, 0.0, 0.0], size: [20.0; 3] });
    let b = *h.app.state.selection.iter().next().unwrap();
    h.cmd(Command::SelectNodes { ids: vec![a.0, b.0], op: loom_commands::SelectOp::Replace });
    h.cmd(Command::Group { name: None });
    let pink = |h: &Harness| {
        fn walk(s: &egui::Shape, n: &mut usize) {
            match s {
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, n)),
                egui::Shape::LineSegment { stroke, .. } if stroke.color == crate::viewport_widget::OPEN_GROUP_PINK => *n += 1,
                _ => {}
            }
        }
        let mut n = 0;
        h.shapes.iter().for_each(|s| walk(&s.shape, &mut n));
        n
    };
    h.frames(2);
    assert_eq!(pink(&h), 0, "a closed group has no pink box");
    h.cmd(Command::GroupOpen);
    h.frames(2);
    let boxes = crate::viewport_widget::open_group_boxes(&h.app);
    assert_eq!(boxes.len(), 1);
    assert!((boxes[0].0.x - -10.0).abs() < 1e-3 && (boxes[0].1.x - 50.0).abs() < 1e-3, "round both members: {:?}", boxes[0]);
    assert_eq!(pink(&h), 12 * h.app.viewport_rects.len(), "twelve edges in each viewport");
    assert!(crate::viewport_widget::selection_bracket_boxes(&h.app).len() <= 2, "no white bracket round the open group as a whole");
    h.cmd(Command::GroupClose);
    h.frames(2);
    assert_eq!(pink(&h), 0, "closed again");
}
