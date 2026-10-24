use glam::{Mat4, Vec3};
use loom_input::OrbitCamera;

/// Aspect-aware view + projection matrices derived from an OrbitCamera.
pub struct Camera {
    pub orbit: OrbitCamera,
    pub perspective: bool,
    pub fov_y_radians: f32,
    pub near: f32,
    pub far: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            orbit: OrbitCamera::default(),
            perspective: true,
            fov_y_radians: 45f32.to_radians(),
            near: 0.01,
            far: 1000.0,
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
        let eye = if self.perspective {
            self.orbit.eye_position()
        } else {
            let dir = (self.orbit.eye_position() - self.orbit.target).normalize_or_zero();
            self.orbit.target + dir * Self::ORTHO_EYE
        };
        Mat4::look_at_rh(eye, self.orbit.target, Vec3::Z)
    }

    pub fn projection_matrix(&self, aspect: f32) -> Mat4 {
        if self.perspective {
            Mat4::perspective_rh(self.fov_y_radians, aspect, self.near, self.far)
        } else {
            // Zoom level comes from `distance` alone (not the perspective FOV).
            let height = (self.orbit.distance * 0.8284).max(0.01);
            let width = height * aspect;
            Mat4::orthographic_rh(
                -width * 0.5,
                width * 0.5,
                -height * 0.5,
                height * 0.5,
                0.01,
                Self::ORTHO_EYE * 2.0,
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
}
