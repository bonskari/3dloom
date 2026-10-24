use glam::{Mat4, Vec3};
use loom_input::OrbitCamera;

/// Aspect-aware view + projection matrices derived from an OrbitCamera.
pub struct Camera {
    pub orbit: OrbitCamera,
    pub perspective: bool,
    pub fov_y_radians: f32,
    pub near: f32,
    pub far: f32,
    /// Viewport Clipping in an axis view: (near, far) distances from the view's
    /// eye point; perspective views use `near` / `far` directly.
    pub clip: Option<(f32, f32)>,
    /// Looking through a camera object: its world frame (looks down local
    /// -Z, local +Y up), which keeps its roll and works straight down.
    pub frame: Option<Mat4>,
    /// Looking through a camera object: its horizontal field of view, which
    /// spans the viewport's width (orthographic: the width at `orbit.distance`).
    pub fov_x_radians: Option<f32>,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            orbit: OrbitCamera::default(),
            perspective: true,
            fov_y_radians: 45f32.to_radians(),
            near: 0.01,
            far: 1000.0,
            clip: None,
            frame: None,
            fov_x_radians: None,
        }
    }
}

impl Camera {
    /// Orthographic views keep the eye far behind the target so zooming in
    /// (small `distance`) never clips geometry in front of the target.
    const ORTHO_EYE: f32 = 2000.0;

    /// Perspective near / far planes for an orbit `distance`: a fixed ratio
    /// around the target, so zooming out never clips the far scene and
    /// zooming in never clips the near one (24-bit depth keeps ~1e-4 of the
    /// distance resolvable at the target).
    pub fn auto_clip(distance: f32) -> (f32, f32) {
        let d = distance.max(1e-3);
        ((d * 1e-3).max(1e-4), (d * 1e3).max(1000.0))
    }

    pub fn view_matrix(&self) -> Mat4 {
        if let Some(f) = self.frame {
            let (x, y, z) = (f.x_axis.truncate().normalize_or_zero(), f.y_axis.truncate().normalize_or_zero(), f.z_axis.truncate().normalize_or_zero());
            let mut eye = f.w_axis.truncate();
            if !self.perspective {
                // Orthographic: the projection's eye sits ORTHO_EYE behind the view's eye point.
                eye += z * (Self::ORTHO_EYE - self.orbit.distance);
            }
            return Mat4::from_cols(x.extend(0.0), y.extend(0.0), z.extend(0.0), eye.extend(1.0)).inverse();
        }
        let eye = if self.perspective {
            self.orbit.eye_position()
        } else {
            let dir = (self.orbit.eye_position() - self.orbit.target).normalize_or_zero();
            self.orbit.target + dir * Self::ORTHO_EYE
        };
        Mat4::look_at_rh(eye, self.orbit.target, self.orbit.up())
    }

    pub fn projection_matrix(&self, aspect: f32) -> Mat4 {
        let half_x = self.fov_x_radians.map(|f| (f * 0.5).tan());
        if self.perspective {
            let fov_y = half_x.map_or(self.fov_y_radians, |t| 2.0 * (t / aspect.max(1e-6)).atan());
            Mat4::perspective_rh(fov_y, aspect, self.near, self.far)
        } else {
            // Zoom level comes from `distance` alone (not the perspective FOV),
            // except through a camera, whose FOV spans the width at `distance`.
            let (width, height) = match half_x {
                Some(t) => {
                    let w = (2.0 * self.orbit.distance * t).max(0.01);
                    (w, w / aspect.max(1e-6))
                }
                None => {
                    let h = (self.orbit.distance * 0.8284).max(0.01);
                    (h * aspect, h)
                }
            };
            Mat4::orthographic_rh(
                -width * 0.5,
                width * 0.5,
                -height * 0.5,
                height * 0.5,
                // The projection's eye sits ORTHO_EYE behind the target; the
                // view's eye point is `distance` in front of it.
                self.clip.map_or(0.01, |(n, _)| (Self::ORTHO_EYE - self.orbit.distance + n).max(0.01)),
                self.clip.map_or(Self::ORTHO_EYE * 2.0, |(n, f)| (Self::ORTHO_EYE - self.orbit.distance + f).max(Self::ORTHO_EYE - self.orbit.distance + n + 1e-3)),
            )
        }
    }

    pub fn view_projection(&self, aspect: f32) -> Mat4 {
        self.projection_matrix(aspect) * self.view_matrix()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orthographic_projection_ignores_depth_for_size() {
        let mut cam = Camera::default();
        cam.perspective = false;
        let p = cam.projection_matrix(1.0);
        // Orthographic: w stays 1 for any depth.
        let a = p * glam::Vec4::new(1.0, 0.0, -1.0, 1.0);
        let b = p * glam::Vec4::new(1.0, 0.0, -50.0, 1.0);
        assert!((a.w - 1.0).abs() < 1e-6 && (b.w - 1.0).abs() < 1e-6);
        assert!((a.x - b.x).abs() < 1e-6);
    }

    #[test]
    fn orthographic_view_does_not_clip_near_the_target_and_ignores_fov() {
        let mut cam = Camera::default();
        cam.perspective = false;
        cam.orbit.distance = 0.1;
        let vp = cam.view_projection(1.0);
        // A point 50 units in front of the target (toward the eye) stays in the clip volume.
        let dir = (cam.orbit.eye_position() - cam.orbit.target).normalize();
        let p = vp * glam::Vec4::from((cam.orbit.target + dir * 50.0, 1.0));
        assert!(p.z / p.w >= 0.0 && p.z / p.w <= 1.0, "clipped: {}", p.z / p.w);
        let a = cam.projection_matrix(1.0);
        cam.fov_y_radians = 1.5;
        assert_eq!(a, cam.projection_matrix(1.0));
    }

    /// Near / far follow the orbit distance: zoomed far out, a scene thousands of
    /// units across stays in view; zoomed in close, nothing near the target clips.
    #[test]
    fn perspective_clip_planes_follow_the_orbit_distance() {
        for distance in [0.5_f32, 12.0, 400.0, 20_000.0] {
            let (near, far) = Camera::auto_clip(distance);
            assert!(near > 0.0 && near < distance * 0.01, "near {near} at {distance}");
            assert!(far >= distance * 100.0, "far {far} at {distance}");
        }
        let mut cam = Camera::default();
        cam.orbit.distance = 400.0;
        (cam.near, cam.far) = Camera::auto_clip(cam.orbit.distance);
        let dir = (cam.orbit.target - cam.orbit.eye_position()).normalize();
        let p = cam.view_projection(1.0) * glam::Vec4::from((cam.orbit.eye_position() + dir * 30_000.0, 1.0));
        assert!(p.z / p.w <= 1.0, "a point 30 000 units away is clipped: {}", p.z / p.w);
    }

    /// Viewport Clipping in an axis view: the planes are distances from the
    /// view's eye point (orbit distance in front of the target), as in Perspective.
    #[test]
    fn orthographic_clipping_cuts_by_distance_from_the_eye() {
        let mut cam = Camera::default();
        cam.perspective = false;
        cam.orbit.distance = 50.0;
        cam.clip = Some((40.0, 60.0));
        let eye = cam.orbit.eye_position();
        let dir = (cam.orbit.target - eye).normalize();
        let depth = |d: f32| {
            let p = cam.view_projection(1.0) * glam::Vec4::from((eye + dir * d, 1.0));
            p.z / p.w
        };
        assert!((0.0..=1.0).contains(&depth(50.0)), "the target (50 from the eye) is kept");
        assert!(depth(30.0) < 0.0, "nearer than near is cut");
        assert!(depth(70.0) > 1.0, "farther than far is cut");
    }

    /// The view keeps the orbit's roll and turns upside down past a pole.
    #[test]
    fn view_matrix_follows_roll_and_pitch_over_the_pole() {
        let mut cam = Camera::default();
        cam.orbit = OrbitCamera { target: Vec3::ZERO, distance: 10.0, yaw: 0.0, pitch: 0.0, roll: std::f32::consts::FRAC_PI_2 };
        let ndc = cam.view_projection(1.0).project_point3(Vec3::Z);
        assert!(ndc.x > 0.1 && ndc.y.abs() < 1e-4, "world up lands screen-right when rolled 90 degrees: {ndc}");
        cam.orbit.roll = 0.0;
        cam.orbit.pitch = 2.0;
        let ndc = cam.view_projection(1.0).project_point3(Vec3::Z * 0.1);
        assert!(ndc.y < 0.0, "upside down past the pole: {ndc}");
    }

    #[test]
    fn view_projection_finite_for_default_camera() {
        let cam = Camera::default();
        let vp = cam.view_projection(16.0 / 9.0);
        for &v in vp.as_ref() {
            assert!(
                v.is_finite(),
                "view-projection matrix must be finite, got {v}"
            );
        }
    }

    #[test]
    fn camera_frame_looks_straight_down_and_its_fov_spans_the_width() {
        // A camera 100 above the origin looking down -Z (local +Y = world +Y).
        let frame = Mat4::from_translation(Vec3::new(0.0, 0.0, 100.0));
        let cam = Camera { frame: Some(frame), fov_x_radians: Some(90f32.to_radians()), near: 1.0, far: 1000.0, ..Camera::default() };
        let vp = cam.view_projection(2.0);
        let ndc = |p: Vec3| vp.project_point3(p);
        // 90 degrees horizontal: x = ±100 at the ground is the frame's edge; y = ±50.
        assert!((ndc(Vec3::new(100.0, 0.0, 0.0)).x - 1.0).abs() < 1e-4);
        assert!((ndc(Vec3::new(0.0, 50.0, 0.0)).y - 1.0).abs() < 1e-4);
        let ortho = Camera { perspective: false, orbit: OrbitCamera { distance: 100.0, ..OrbitCamera::default() }, ..cam };
        let vp = ortho.view_projection(2.0);
        assert!((vp.project_point3(Vec3::new(100.0, 0.0, -30.0)).x - 1.0).abs() < 1e-4, "orthographic width is the FOV's width at the distance");
    }
}
