//! The default Perspective view looks at the origin from the front-left
//! (the ViewCube shows TOP, LEFT and FRONT; X runs right, Y runs left).

use loom_commands::{Viewport, ViewKind};

#[test]
fn default_perspective_looks_from_front_left() {
    let eye = Viewport::new(ViewKind::Perspective).camera.eye_position();
    assert!(eye.x < 0.0 && eye.y < 0.0 && eye.z > 0.0, "eye {eye:?}");
    assert_eq!((Viewport::new(ViewKind::Perspective).camera.yaw, Viewport::new(ViewKind::Perspective).camera.pitch), loom_commands::viewport::HOME_PERSPECTIVE);
}
