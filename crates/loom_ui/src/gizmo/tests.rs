use super::*;
use eframe::egui;
use glam::{Mat3, Mat4, Vec3};

/// Top view: orthographic, looking down -Z, 1 world unit = 10 px, origin at
/// the viewport centre (400, 300).
fn top_projection() -> Projection {
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
    let view = Mat4::look_at_rh(Vec3::new(0.0, 0.0, 100.0), Vec3::ZERO, Vec3::Y);
    let proj = Mat4::orthographic_rh(-40.0, 40.0, -30.0, 30.0, 0.1, 1000.0);
    Projection::new(view, proj, rect)
}

fn layout() -> GizmoLayout {
    GizmoLayout { origin: Vec3::ZERO, axes: Mat3::IDENTITY, size_px: 40.0, ..Default::default() }
}

#[test]
fn squash_keeps_the_volume() {
    let f = scale_gizmo::squash_factors(Handle::Axis(Axis::X), 4.0);
    assert!(f.abs_diff_eq(Vec3::new(4.0, 0.5, 0.5), 1e-6), "{f}");
    let f = scale_gizmo::squash_factors(Handle::Plane(Plane::XY), 2.0);
    assert!(f.abs_diff_eq(Vec3::new(2.0, 2.0, 0.25), 1e-6), "{f}");
    assert_eq!(scale_gizmo::squash_factors(Handle::Uniform, 2.0), Vec3::splat(2.0));
}

#[test]
fn axis_handle_hit_within_8px() {
    let p = top_projection();
    let l = layout();
    // X axis runs 40 px to the right of centre; 6 px below it still hits.
    assert_eq!(move_gizmo::hit(&l, &p, egui::pos2(430.0, 306.0)), Some(Handle::Axis(Axis::X)));
    assert_eq!(move_gizmo::hit(&l, &p, egui::pos2(430.0, 312.0)), None);
    // Y axis points up on screen.
    assert_eq!(move_gizmo::hit(&l, &p, egui::pos2(404.0, 270.0)), Some(Handle::Axis(Axis::Y)));
}

#[test]
fn plane_handle_hit_inside_square() {
    let p = top_projection();
    let l = layout();
    // XY plane square sits between 25 % and 55 % of the axis length (default size 50, offset 50).
    assert_eq!(move_gizmo::hit(&l, &p, egui::pos2(416.0, 284.0)), Some(Handle::Plane(Plane::XY)));
    assert_eq!(move_gizmo::hit(&l, &p, egui::pos2(432.0, 268.0)), None);
}

#[test]
fn centre_hit_when_axes_degenerate() {
    // Looking straight down Z: the Z axis projects to a point, the centre box still works.
    let p = top_projection();
    let l = layout();
    assert_eq!(move_gizmo::hit(&l, &p, egui::pos2(401.0, 299.0)), Some(Handle::CenterBox));
    assert!(move_gizmo::axis_screen_dir(&l, &p, Axis::Z).is_none());
}

#[test]
fn rotate_circle_hit_by_ring_distance() {
    let p = top_projection();
    let l = layout();
    // Z ring: radius 40 px in the screen plane.
    assert_eq!(rotate_gizmo::hit(&l, &p, egui::pos2(400.0 + 40.0, 300.0)), Some(Handle::Axis(Axis::Z)));
    assert_eq!(rotate_gizmo::hit(&l, &p, egui::pos2(400.0, 300.0 - 38.0)), Some(Handle::Axis(Axis::Z)));
    // Screen ring is 25 % larger.
    assert_eq!(rotate_gizmo::hit(&l, &p, egui::pos2(400.0 + 50.0, 300.0)), Some(Handle::Screen));
    // Inside the sphere, off every ring: free rotation.
    assert_eq!(rotate_gizmo::hit(&l, &p, egui::pos2(410.0, 310.0)), Some(Handle::Free));
    assert_eq!(rotate_gizmo::hit(&l, &p, egui::pos2(500.0, 300.0)), None);
}

#[test]
fn scale_uniform_hit_centre() {
    let p = top_projection();
    let l = layout();
    assert_eq!(scale_gizmo::hit(&l, &p, egui::pos2(403.0, 297.0)), Some(Handle::Uniform));
    assert_eq!(scale_gizmo::hit(&l, &p, egui::pos2(438.0, 300.0)), Some(Handle::Axis(Axis::X)));
}

/// Orthographic view from the front-top-right, 10 px per unit, origin at the centre.
fn tilted_projection() -> Projection {
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
    let view = Mat4::look_at_rh(Vec3::new(40.0, -100.0, 100.0), Vec3::ZERO, Vec3::Z);
    let proj = Mat4::orthographic_rh(-40.0, 40.0, -30.0, 30.0, 0.1, 1000.0);
    Projection::new(view, proj, rect)
}

#[test]
fn rotate_ring_back_half_is_not_hittable() {
    let p = tilted_projection();
    let l = layout();
    let r = l.world_len(&p);
    let tv = p.toward_viewer();
    let toward = Vec3::new(tv.x, tv.y, 0.0).normalize();
    // The Z ring point nearest the viewer is a handle; the one farthest away is behind the sphere.
    let front = p.screen(toward * r).unwrap();
    let back = p.screen(-toward * r).unwrap();
    assert_eq!(rotate_gizmo::hit(&l, &p, front), Some(Handle::Axis(Axis::Z)));
    assert_ne!(rotate_gizmo::hit(&l, &p, back), Some(Handle::Axis(Axis::Z)));
}

#[test]
fn rotate_ring_front_half_faces_the_eye_in_perspective() {
    // Perspective camera, gizmo away from the view centre: which half of a
    // ring is in front depends on the direction to the eye, not the camera axis.
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
    let view = Mat4::look_at_rh(Vec3::new(0.0, -10.0, 10.0), Vec3::ZERO, Vec3::Z);
    let proj = Mat4::perspective_rh(60f32.to_radians(), 800.0 / 600.0, 0.1, 1000.0);
    let p = Projection::new(view, proj, rect);
    let l = GizmoLayout { origin: Vec3::new(6.0, 0.0, 0.0), ..layout() };
    let toward_eye = Vec3::new(-1.0, 0.15, 0.0).normalize();
    let away = Vec3::new(1.0, -0.15, 0.0).normalize();
    assert!(toward_eye.dot(p.toward_viewer()) < -0.1 && away.dot(p.toward_viewer()) > 0.1);
    assert!(rotate_gizmo::faces_viewer(&l, &p, toward_eye));
    assert!(!rotate_gizmo::faces_viewer(&l, &p, away));
    // Orthographic: the view direction decides.
    let o = tilted_projection();
    assert!(rotate_gizmo::faces_viewer(&layout(), &o, o.toward_viewer()));
    assert!(!rotate_gizmo::faces_viewer(&layout(), &o, -o.toward_viewer()));
}

#[test]
fn scale_triangle_handles() {
    let p = top_projection();
    let l = layout();
    // Inner triangle reaches 50 % of the axis (20 px), the plane band ends at 70 % (28 px).
    assert_eq!(scale_gizmo::hit(&l, &p, egui::pos2(408.0, 292.0)), Some(Handle::Uniform));
    assert_eq!(scale_gizmo::hit(&l, &p, egui::pos2(413.0, 287.0)), Some(Handle::Plane(Plane::XY)));
    assert_eq!(scale_gizmo::hit(&l, &p, egui::pos2(419.0, 281.0)), None);
    // The axis line inside the inner triangle belongs to the uniform handle, beyond it to the axis.
    assert_eq!(scale_gizmo::hit(&l, &p, egui::pos2(415.0, 300.0)), Some(Handle::Uniform));
    assert_eq!(scale_gizmo::hit(&l, &p, egui::pos2(436.0, 300.0)), Some(Handle::Axis(Axis::X)));
}

#[test]
fn scale_hit_follows_the_scale_size_preference() {
    let p = top_projection();
    let l = GizmoLayout { scale_size: 200.0, ..layout() };
    // Twice the size: the axis reaches 80 px.
    assert_eq!(scale_gizmo::hit(&l, &p, egui::pos2(476.0, 300.0)), Some(Handle::Axis(Axis::X)));
}

#[test]
fn rotate_drag_angle_is_signed_by_axis_toward_viewer() {
    let p = top_projection();
    let l = layout();
    // Counter-clockwise on screen around +Z (toward the viewer) is a positive angle.
    let a = rotate_gizmo::drag_angle_deg(&l, &p, Axis::Z, egui::pos2(440.0, 300.0), egui::pos2(400.0, 260.0));
    assert!((a - 90.0).abs() < 1e-3, "{a}");
}

#[test]
fn angle_snap_accumulates_slow_rotate_drags() {
    use loom_commands::{Command, PrimitiveKind, Tool};
    let mut app = crate::LoomApp::headless();
    let _ = app.dispatch(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [1.0; 3] });
    let _ = app.dispatch(Command::SelectAll);
    let _ = app.dispatch(Command::SelectAndRotate);
    let _ = app.dispatch(Command::AngleSnapToggle);
    assert!(app.state.angle_snap_on && app.state.active_tool == Tool::Rotate);
    let p = top_projection();
    let l = layout();
    let at = |deg: f32| egui::pos2(400.0 + 40.0 * deg.to_radians().cos(), 300.0 - 40.0 * deg.to_radians().sin());
    let mut ds = DragState::new(Handle::Axis(Axis::Z), at(0.0));
    // 1° per frame is below the 5° snap increment: the rotation must still advance.
    for i in 1..=10 {
        drag(&mut app, &mut ds, &l, &p, at(i as f32), 0);
    }
    let id = *app.state.selection.iter().next().unwrap();
    let z = app.state.scene.node(id).unwrap().euler_deg().z;
    assert!((z - 10.0).abs() < 1e-3, "{z}");
    assert!((ds.angle_deg - 10.0).abs() < 1e-3);
}

#[test]
fn preview_undo_only_removes_its_own_entry() {
    use loom_commands::{Command, Dialog, PrimitiveKind};
    let mut app = crate::LoomApp::headless();
    let _ = app.dispatch(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [1.0; 3] });
    let _ = app.dispatch(Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [5.0, 0.0, 0.0], size: [1.0; 3] });
    assert_eq!(app.state.undo.len(), 2);
    // A preview that pushes nothing is not remembered, so undoing it cannot eat the creation.
    app.preview_apply(Dialog::Align, |_| {});
    assert!(app.preview.is_none());
    app.preview_undo();
    assert_eq!(app.state.scene.nodes().count(), 2);
    // A real preview is undone while on top...
    app.preview_apply(Dialog::Align, |app| {
        let _ = app.dispatch(Command::MoveSelection { delta: [1.0, 0.0, 0.0], system: None });
    });
    assert!(app.preview.is_some(), "a real preview is remembered");
    app.preview_undo();
    assert_eq!(app.state.undo.len(), 2);
    // ...but not after another edit landed on top of it.
    app.preview_apply(Dialog::Align, |app| {
        let _ = app.dispatch(Command::MoveSelection { delta: [1.0, 0.0, 0.0], system: None });
    });
    let _ = app.dispatch(Command::MoveSelection { delta: [0.0, 1.0, 0.0], system: None });
    app.preview_undo();
    assert_eq!(app.state.undo.len(), 4, "the user's later move stays");
}

#[test]
fn fk_control_gizmo_drag_accumulates_pointer_steps_without_changing_source() {
    use loom_commands::{Command, CoordSystem, Tool};
    use loom_scene::{loomrig::HumanoidMapping, Node, ObjectKind};
    let mut app = crate::LoomApp::headless();
    let root = app
        .state
        .scene
        .add_node(Node::new("root", ObjectKind::bone(1.)));
    let mut pelvis = Node::new("pelvis", ObjectKind::bone(10.));
    pelvis.parent = Some(root);
    let bone = app.state.scene.add_node(pelvis);
    let mut head = Node::new("head", ObjectKind::bone(1.));
    head.parent = Some(bone);
    head.transform.position.set(Vec3::X * 10.);
    let head = app.state.scene.add_node(head);
    app.dispatch(Command::CreateLoomRig {
        root: root.0,
        mapping: Some(HumanoidMapping {
            root,
            pelvis: bone,
            head,
            spine: vec![],
            limbs: vec![],
        }),
    })
    .unwrap();
    let control = app.state.scene.loom_rigs[0]
        .bindings
        .iter()
        .find(|b| b.bone == bone)
        .unwrap()
        .control;
    app.state.selection = [control].into();
    app.state.active_tool = Tool::Move;
    app.state.coord_system = CoordSystem::World;
    let original = app.state.scene.node(bone).unwrap().transform.clone();
    let projection = top_projection();
    let gizmo = layout();
    let mut state = DragState::new(Handle::Axis(Axis::X), egui::pos2(400., 300.));
    for i in 1..=3 {
        drag(
            &mut app,
            &mut state,
            &gizmo,
            &projection,
            egui::pos2(400. + 100. * i as f32, 300.),
            0,
        );
        let expected = Vec3::X * (10. * i as f32);
        let actual = app.state.scene.world_matrix(bone).w_axis.truncate();
        assert!(
            actual.distance(expected) < 0.001,
            "drag step {i}: {actual:?} != {expected:?}"
        );
        assert!(app
            .state
            .scene
            .world_matrix(control)
            .abs_diff_eq(app.state.scene.world_matrix(bone), 0.001));
    }
    app.state.active_tool = Tool::Rotate;
    let mut rotate_layout = layout();
    rotate_layout.origin = app.state.scene.world_matrix(bone).w_axis.truncate();
    let center = projection.screen(rotate_layout.origin).unwrap();
    let at = |degrees: f32| {
        center
            + egui::vec2(
                40. * degrees.to_radians().cos(),
                -40. * degrees.to_radians().sin(),
            )
    };
    let mut rotating = DragState::new(Handle::Axis(Axis::Z), at(0.));
    for i in 1..=3 {
        drag(
            &mut app,
            &mut rotating,
            &rotate_layout,
            &projection,
            at(15. * i as f32),
            0,
        );
        let actual = app
            .state
            .scene
            .world_matrix(bone)
            .to_scale_rotation_translation()
            .1;
        let expected = glam::Quat::from_rotation_z((15. * i as f32).to_radians());
        assert!(
            actual.abs_diff_eq(expected, 0.001),
            "rotate drag step {i}: {actual:?} != {expected:?}"
        );
    }
    assert_eq!(app.state.scene.node(bone).unwrap().transform, original);
}
