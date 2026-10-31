//! The mouse cursor follows the context: transform tools over their gizmo,
//! selection over objects and sub-objects, navigation modes, creation, pick
//! modes, paint brushes and spinners.

use super::*;
use egui::CursorIcon;

impl Harness {
    /// Moves the pointer to `p` (no button) and returns the cursor shown.
    fn hover(&mut self, p: egui::Pos2) -> CursorIcon {
        self.run(vec![egui::Event::PointerMoved(p)]);
        self.frames(1);
        self.cursor
    }
}

/// A point on a gizmo handle of the active tool: searched outward from the
/// pivot in every direction, as a person aims at a handle.
fn gizmo_point(h: &Harness, want: impl Fn(crate::gizmo::Handle) -> bool) -> egui::Pos2 {
    let i = h.app.state.active_viewport;
    let rect = h.app.viewport_rects[i];
    let cam = h.app.camera_for(i);
    let projection = crate::gizmo::Projection::new(cam.view_matrix(), cam.projection_matrix(rect.width() / rect.height()), rect);
    let layout = crate::gizmo::layout_for(&h.app, i, rect.height());
    let o = h.screen_of(layout.origin);
    (0..72)
        .flat_map(|a| (4..250).map(move |d| (a, d)))
        .map(|(a, d)| o + egui::Vec2::angled((a as f32 * 5.0).to_radians()) * d as f32)
        .find(|p| crate::gizmo::hit(&h.app, &layout, &projection, *p).is_some_and(&want))
        .expect("a gizmo handle")
}

fn axis(h: crate::gizmo::Handle) -> bool {
    matches!(h, crate::gizmo::Handle::Axis(_))
}

fn boxed() -> Harness {
    let mut h = Harness::new();
    h.cmd(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [30.0; 3] });
    framed(&mut h);
    h
}

/// Over a gizmo axis: Move shows the move arrows, Rotate the rotate cursor,
/// Scale one per scale type.
#[test]
fn gizmo_handles_show_their_transform_cursor() {
    let mut h = boxed();
    h.key(egui::Key::W);
    let p = gizmo_point(&h, axis);
    assert_eq!(h.hover(p), CursorIcon::Move, "Move gizmo axis");
    h.key(egui::Key::E);
    let p = gizmo_point(&h, axis);
    assert_eq!(h.hover(p), CursorIcon::AllScroll, "Rotate gizmo ring");
    h.key(egui::Key::R);
    for (kind, icon) in [(loom_commands::ScaleKind::Uniform, CursorIcon::ResizeNwSe), (loom_commands::ScaleKind::NonUniform, CursorIcon::ResizeNeSw), (loom_commands::ScaleKind::Squash, CursorIcon::ResizeVertical)] {
        h.cmd(Command::ScaleType { kind });
        let p = gizmo_point(&h, axis);
        assert_eq!(h.hover(p), icon, "{kind:?}");
    }
}

/// The cursor stays the tool's while its gizmo drag runs.
#[test]
fn a_gizmo_drag_keeps_the_move_cursor() {
    let mut h = boxed();
    h.key(egui::Key::W);
    let p = gizmo_point(&h, axis);
    h.hover(p);
    h.run(vec![egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE }]);
    h.run(vec![egui::Event::PointerMoved(p + egui::vec2(40.0, 0.0))]);
    assert!(h.app.transform_drag.is_some());
    assert_eq!(h.cursor, CursorIcon::Move);
    h.run(vec![egui::Event::PointerButton { pos: p + egui::vec2(40.0, 0.0), button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE }]);
}

/// Select Object: the selection cross over an object, the arrow off it.
#[test]
fn select_shows_the_cross_over_objects_only() {
    let mut h = boxed();
    h.key(egui::Key::Q);
    let on = h.screen_of(top_face_point());
    assert_eq!(h.hover(on), CursorIcon::Crosshair);
    let rect = h.app.viewport_rects[h.app.state.active_viewport];
    assert_eq!(h.hover(rect.left_top() + egui::vec2(60.0, 120.0)), CursorIcon::Default);
}

/// At a sub-object level the cursor over a polygon is the small cross.
#[test]
fn select_shows_the_small_cross_over_sub_objects() {
    let mut h = Harness::new();
    poly_box(&mut h);
    h.key(egui::Key::Q);
    h.key(egui::Key::Num4);
    assert_eq!(h.hover(h.screen_of(top_face_point())), CursorIcon::Cell);
}

/// Select and Link: the link cursor; another pick mode: the pick cursor.
#[test]
fn link_and_pick_modes_show_their_cursors() {
    let mut h = boxed();
    let p = h.screen_of(top_face_point());
    h.cmd(Command::SelectAndLink);
    assert_eq!(h.hover(p), CursorIcon::Alias);
    h.app.state.pick_mode = Some(loom_commands::PickMode::BooleanOperandB);
    assert_eq!(h.hover(p), CursorIcon::PointingHand);
}

/// The modal navigation tools each show their cursor; Pan grabs while dragging.
#[test]
fn navigation_modes_show_their_cursors() {
    use loom_input::NavMode as N;
    let mut h = boxed();
    let c = h.app.viewport_rects[h.app.state.active_viewport].center();
    for (mode, icon) in [(N::Pan, CursorIcon::Grab), (N::Orbit, CursorIcon::AllScroll), (N::Zoom, CursorIcon::ZoomIn), (N::ZoomRegion, CursorIcon::ZoomIn), (N::Fov, CursorIcon::ZoomIn)] {
        h.app.state.nav_mode = mode;
        assert_eq!(h.hover(c), icon, "{mode:?}");
    }
    h.app.state.nav_mode = N::Pan;
    h.hover(c);
    h.run(vec![egui::Event::PointerButton { pos: c, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE }]);
    h.run(vec![egui::Event::PointerMoved(c + egui::vec2(30.0, 0.0))]);
    assert_eq!(h.cursor, CursorIcon::Grabbing);
    h.run(vec![egui::Event::PointerButton { pos: c + egui::vec2(30.0, 0.0), button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE }]);
}

/// A middle-button drag pans with the grabbing hand.
#[test]
fn middle_button_pan_shows_the_grabbing_hand() {
    let mut h = boxed();
    let c = h.app.viewport_rects[h.app.state.active_viewport].center();
    h.hover(c);
    h.run(vec![egui::Event::PointerButton { pos: c, button: egui::PointerButton::Middle, pressed: true, modifiers: egui::Modifiers::NONE }]);
    h.run(vec![egui::Event::PointerMoved(c + egui::vec2(30.0, 0.0))]);
    assert_eq!(h.cursor, CursorIcon::Grabbing);
    h.run(vec![egui::Event::PointerButton { pos: c + egui::vec2(30.0, 0.0), button: egui::PointerButton::Middle, pressed: false, modifiers: egui::Modifiers::NONE }]);
}

/// Create mode: the crosshair.
#[test]
fn create_mode_shows_the_crosshair() {
    let mut h = Harness::new();
    maximized(&mut h);
    h.app.create_kind = Some(crate::CreateKind::Primitive(PrimitiveKind::Box));
    let c = h.app.viewport_rects[h.app.state.active_viewport].center();
    assert_eq!(h.hover(c), CursorIcon::Crosshair);
}

/// Spinners show the up-down arrows on hover (egui's own is left-right).
#[test]
fn spinners_show_the_up_down_cursor() {
    let mut h = boxed();
    h.key(egui::Key::W);
    let x = h.spinner_after("X:").center();
    assert_eq!(h.hover(x), CursorIcon::ResizeVertical);
}
