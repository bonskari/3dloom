//! The parity28 round.

use super::*;

fn perspective_index(h: &Harness) -> usize {
    h.app.state.viewports.iter().position(|v| v.kind == loom_commands::ViewKind::Perspective).expect("a Perspective view")
}

/// Half the horizontal field of view's tangent of viewport `i` as drawn.
fn half_tan_x(h: &Harness, i: usize) -> f32 {
    let r = h.app.viewport_rects[i];
    1.0 / h.app.camera_for(i).projection_matrix(r.width() / r.height()).x_axis.x
}

/// The Perspective view's 45 degree FOV is measured across its width, in the
/// quad layout and maximized alike; Create Camera From View gives the camera
/// that same horizontal FOV and the camera view draws the same width.
#[test]
fn the_perspective_fov_spans_the_view_width() {
    let mut h = Harness::new();
    let i = perspective_index(&h);
    assert!((h.app.state.viewports[i].fov_radians.to_degrees() - 45.0).abs() < 1e-4);
    let t = 22.5f32.to_radians().tan();
    assert!((half_tan_x(&h, i) - t).abs() < 1e-4, "quad: {}", half_tan_x(&h, i));
    h.cmd(Command::ActiveViewport { index: i as u32 });
    maximized(&mut h);
    h.frames(2);
    let r = h.app.viewport_rects[i];
    assert!((half_tan_x(&h, i) - t).abs() < 1e-4, "maximized ({}x{}): {}", r.width(), r.height(), half_tan_x(&h, i));
    h.cmd(Command::CreateCameraFromView);
    let loom_commands::ViewKind::Camera(id) = h.app.state.viewports[i].kind else { panic!("not looking through the new camera") };
    let Some(loom_scene::ObjectKind::Camera { fov, .. }) = h.app.state.scene.node(id).map(|n| &n.object) else { panic!("no camera") };
    assert!((fov.get() - 45.0).abs() < 1e-3, "camera FOV {}", fov.get());
    assert!((half_tan_x(&h, i) - t).abs() < 1e-4, "the camera view draws the same width");
}
