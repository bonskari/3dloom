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
fn a_fresh_scene_frames_the_whole_perspective_grid_and_shows_grid_squares_in_the_axis_views() {
    let h = Harness::new();
    let i = view_index(&h, loom_commands::ViewKind::Perspective);
    let rect = h.app.viewport_rects[i];
    let cam = h.app.camera_for(i);
    let vp = cam.projection_matrix(rect.width() / rect.height()) * cam.view_matrix();
    let e = h.app.state.home_grid.perspective_cells() as f32 * h.app.state.home_grid.spacing;
    for c in [Vec3::new(e, e, 0.0), Vec3::new(-e, e, 0.0), Vec3::new(e, -e, 0.0), Vec3::new(-e, -e, 0.0)] {
        let n = vp.project_point3(c);
        assert!(n.x.abs() < 1.0 && n.y.abs() < 1.0, "grid corner {c} shows in the Perspective view: {n}");
    }
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
