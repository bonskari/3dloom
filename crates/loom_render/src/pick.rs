//! Ray casting helpers for object picking.

use glam::{Mat4, Vec2, Vec3, Vec4};

/// Build a world-space ray from a viewport-relative screen position.
///
/// `local_pos` is in pixels, relative to the viewport top-left (Y down).
/// `viewport_size` is the viewport rect's width and height in pixels.
/// `view` and `proj` are the camera's view and projection matrices.
/// Returns (origin, direction) where direction is normalized.
pub fn screen_to_world_ray(
    local_pos: Vec2,
    viewport_size: Vec2,
    view: Mat4,
    proj: Mat4,
) -> (Vec3, Vec3) {
    let w = viewport_size.x.max(1.0);
    let h = viewport_size.y.max(1.0);
    // NDC: x in [-1,1] right-positive, y in [-1,1] up-positive (flip Y from screen-down).
    let ndc = Vec2::new(2.0 * local_pos.x / w - 1.0, 1.0 - 2.0 * local_pos.y / h);
    // Unproject the pixel on the near and far planes (wgpu depth 0..1); this
    // is correct for perspective and orthographic projections alike.
    let inv = (proj * view).inverse();
    let unproject = |z: f32| {
        let p = inv * Vec4::new(ndc.x, ndc.y, z, 1.0);
        Vec3::new(p.x, p.y, p.z) / p.w
    };
    let near = unproject(0.0);
    let far = unproject(1.0);
    (near, (far - near).normalize_or_zero())
}

/// Ray–AABB intersection using the slab method.
///
/// Returns the distance `t` at which the ray (origin + t*dir) first enters the box,
/// or `None` if there is no intersection (including misses behind the origin).
pub fn ray_aabb(origin: Vec3, dir: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let inv_dir = Vec3::new(
        if dir.x.abs() > f32::EPSILON {
            1.0 / dir.x
        } else {
            f32::INFINITY
        },
        if dir.y.abs() > f32::EPSILON {
            1.0 / dir.y
        } else {
            f32::INFINITY
        },
        if dir.z.abs() > f32::EPSILON {
            1.0 / dir.z
        } else {
            f32::INFINITY
        },
    );
    let t1 = (min - origin) * inv_dir;
    let t2 = (max - origin) * inv_dir;
    let tmin = t1.min(t2);
    let tmax = t1.max(t2);
    let near = tmin.x.max(tmin.y).max(tmin.z);
    let far = tmax.x.min(tmax.y).min(tmax.z);
    if near <= far && far >= 0.0 {
        Some(near.max(0.0))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ray_hits_unit_cube_from_camera_aimed_at_origin() {
        // Box from (-0.5,-0.5,-0.5) to (0.5,0.5,0.5)
        let min = Vec3::splat(-0.5);
        let max = Vec3::splat(0.5);
        // Ray from (5, 0, 0) toward origin
        let origin = Vec3::new(5.0, 0.0, 0.0);
        let dir = (-Vec3::X).normalize();
        let t = ray_aabb(origin, dir, min, max);
        assert!(t.is_some());
        let t = t.unwrap();
        // Should hit at x=0.5, distance from origin = 5 - 0.5 = 4.5
        assert!((t - 4.5).abs() < 1e-3, "expected t ≈ 4.5, got {t}");
    }

    #[test]
    fn ray_misses_when_aimed_away() {
        let min = Vec3::splat(-0.5);
        let max = Vec3::splat(0.5);
        let origin = Vec3::new(5.0, 0.0, 0.0);
        let dir = Vec3::X; // pointing AWAY from box
        assert!(ray_aabb(origin, dir, min, max).is_none());
    }

    #[test]
    fn ray_misses_parallel_offset() {
        let min = Vec3::splat(-0.5);
        let max = Vec3::splat(0.5);
        // Ray above the box, parallel to X
        let origin = Vec3::new(5.0, 0.0, 2.0);
        let dir = -Vec3::X;
        assert!(ray_aabb(origin, dir, min, max).is_none());
    }
}

#[cfg(test)]
mod ray_tests {
    use super::*;

    #[test]
    fn orthographic_rays_are_parallel_with_distinct_origins() {
        let view = Mat4::look_at_rh(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO, Vec3::Y);
        let proj = Mat4::orthographic_rh(-5.0, 5.0, -5.0, 5.0, 0.1, 100.0);
        let size = Vec2::new(100.0, 100.0);
        let (o1, d1) = screen_to_world_ray(Vec2::new(10.0, 50.0), size, view, proj);
        let (o2, d2) = screen_to_world_ray(Vec2::new(90.0, 50.0), size, view, proj);
        assert!(d1.abs_diff_eq(d2, 1e-5) && d1.abs_diff_eq(-Vec3::Z, 1e-4), "{d1} {d2}");
        assert!((o2.x - o1.x - 8.0).abs() < 1e-3, "{o1} {o2}");
    }

    #[test]
    fn perspective_rays_start_near_the_eye() {
        let eye = Vec3::new(0.0, -10.0, 5.0);
        let view = Mat4::look_at_rh(eye, Vec3::ZERO, Vec3::Z);
        let proj = Mat4::perspective_rh(1.0, 1.0, 0.01, 1000.0);
        let (o, d) = screen_to_world_ray(Vec2::new(50.0, 50.0), Vec2::new(100.0, 100.0), view, proj);
        assert!(o.distance(eye) < 0.05, "{o}");
        assert!(d.abs_diff_eq((Vec3::ZERO - eye).normalize(), 1e-3), "{d}");
    }
}
