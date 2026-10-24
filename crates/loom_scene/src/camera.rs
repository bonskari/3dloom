//! Camera objects: lens / field-of-view conversions, the viewport icon and
//! the field-of-view pyramid. A camera looks down its local -Z with local +Y
//! up; `fov` is always the horizontal field of view in degrees.

use glam::Vec3;
use loom_mesh::Mesh;

/// Film (aperture) width in millimetres the Lens value is measured against.
pub const FILM_WIDTH_MM: f32 = 36.0;
/// Output aspect (width / height) of the default 640 × 480 render.
pub use crate::mapping9::RENDER_ASPECT;
/// Horizontal field of view of a new camera, in degrees.
pub const DEFAULT_FOV: f32 = 45.0;
/// Target distance of a new free camera.
pub const DEFAULT_TARGET_DISTANCE: f32 = 160.0;
/// Viewport near plane of a camera view while Clip Manually is off.
pub const AUTO_NEAR_CLIP: f32 = 3.0;
/// The nearest a manual near clipping plane may be.
pub const MIN_NEAR_CLIP: f32 = 0.1;
/// Stock Lenses buttons, in millimetres.
pub const STOCK_LENSES: [f32; 9] = [15.0, 20.0, 24.0, 28.0, 35.0, 50.0, 85.0, 135.0, 200.0];
/// FOV Direction flyout entries.
pub const FOV_DIRECTIONS: [&str; 3] = ["Horizontal", "Vertical", "Diagonal"];

/// Focal length in millimetres for a horizontal field of view in degrees.
pub fn fov_to_lens(fov_deg: f32) -> f32 {
    FILM_WIDTH_MM * 0.5 / (fov_deg.to_radians() * 0.5).tan()
}

/// Horizontal field of view in degrees for a focal length in millimetres.
pub fn lens_to_fov(lens_mm: f32) -> f32 {
    2.0 * (FILM_WIDTH_MM * 0.5 / lens_mm.max(1e-3)).atan().to_degrees()
}

/// The horizontal FOV measured in `direction` (0 horizontal, 1 vertical,
/// 2 diagonal) for an output of `aspect` (width / height).
pub fn fov_in_direction(fov_deg: f32, direction: u32, aspect: f32) -> f32 {
    let t = (fov_deg.to_radians() * 0.5).tan();
    let t = match direction {
        1 => t / aspect,
        2 => t * (1.0 + 1.0 / (aspect * aspect)).sqrt(),
        _ => t,
    };
    2.0 * t.atan().to_degrees()
}

/// Inverse of [`fov_in_direction`]: the horizontal FOV for a value measured in `direction`.
pub fn fov_from_direction(value_deg: f32, direction: u32, aspect: f32) -> f32 {
    let t = (value_deg.to_radians() * 0.5).tan();
    let t = match direction {
        1 => t * aspect,
        2 => t / (1.0 + 1.0 / (aspect * aspect)).sqrt(),
        _ => t,
    };
    2.0 * t.atan().to_degrees()
}

/// Camera-space corners (top-left, top-right, bottom-right, bottom-left) of
/// the field-of-view rectangle `distance` in front of the camera. With
/// `ortho` the rectangle keeps the size it has at `ortho_distance`.
pub fn frame_corners(fov_deg: f32, aspect: f32, distance: f32, ortho: Option<f32>) -> [Vec3; 4] {
    let half_w = ortho.unwrap_or(distance) * (fov_deg.to_radians() * 0.5).tan();
    let half_h = half_w / aspect.max(1e-6);
    let z = -distance;
    [Vec3::new(-half_w, half_h, z), Vec3::new(half_w, half_h, z), Vec3::new(half_w, -half_h, z), Vec3::new(-half_w, -half_h, z)]
}

/// The camera icon: a box body behind a lens hood that opens toward the
/// view (local -Z). The pivot is the centre of the hood's opening, so the
/// whole icon lies behind the eye point.
pub fn icon_mesh() -> Mesh {
    // Lens hood: from the opening (z = 0) narrowing back to the body.
    let (hood_front, hood_back, hood_len) = (Vec3::new(4.0, 3.2, 0.0), Vec3::new(1.8, 1.5, 0.0), 4.0);
    // Body: a box behind the hood.
    let (body_half, body_from, body_to) = (Vec3::new(3.0, 4.5, 0.0), hood_len, hood_len + 13.0);
    let rect = |h: Vec3, z: f32| [Vec3::new(-h.x, -h.y, z), Vec3::new(h.x, -h.y, z), Vec3::new(h.x, h.y, z), Vec3::new(-h.x, h.y, z)];
    let mut polys: Vec<Vec<Vec3>> = Vec::new();
    let mut solid = |a: [Vec3; 4], b: [Vec3; 4], caps: (bool, bool)| {
        let centre = (a.iter().sum::<Vec3>() + b.iter().sum::<Vec3>()) / 8.0;
        let mut faces: Vec<Vec<Vec3>> = (0..4).map(|i| vec![a[i], a[(i + 1) % 4], b[(i + 1) % 4], b[i]]).collect();
        if caps.0 {
            faces.push(a.to_vec());
        }
        if caps.1 {
            faces.push(b.to_vec());
        }
        for mut f in faces {
            // Wind every face to point away from the solid's centre.
            let n = (f[1] - f[0]).cross(f[2] - f[0]);
            let c = f.iter().sum::<Vec3>() / f.len() as f32;
            if n.dot(c - centre) < 0.0 {
                f.reverse();
            }
            polys.push(f);
        }
    };
    solid(rect(hood_front, 0.0), rect(hood_back, hood_len), (false, false));
    solid(rect(body_half, body_from), rect(body_half, body_to), (true, true));
    loom_mesh::build_merged(&polys)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_lens_matches_default_fov() {
        assert!((fov_to_lens(45.0) - 43.456).abs() < 1e-3, "{}", fov_to_lens(45.0));
        assert!((lens_to_fov(43.456) - 45.0).abs() < 1e-3);
        for mm in STOCK_LENSES {
            assert!((fov_to_lens(lens_to_fov(mm)) - mm).abs() < 1e-3);
        }
    }

    #[test]
    fn diagonal_and_vertical_fov_follow_the_render_aspect() {
        // A 45 degree horizontal FOV is 54.7473 degrees diagonally at 4:3.
        assert!((fov_in_direction(45.0, 2, RENDER_ASPECT) - 54.7473).abs() < 1e-3);
        for d in 0..3 {
            let v = fov_in_direction(45.0, d, RENDER_ASPECT);
            assert!((fov_from_direction(v, d, RENDER_ASPECT) - 45.0).abs() < 1e-3);
        }
        assert!(fov_in_direction(45.0, 1, RENDER_ASPECT) < 45.0);
    }

    #[test]
    fn frame_spans_the_fov_in_front_of_the_camera() {
        let c = frame_corners(90.0, 2.0, 10.0, None);
        assert!((c[1].x - 10.0).abs() < 1e-4 && (c[1].y - 5.0).abs() < 1e-4 && (c[1].z + 10.0).abs() < 1e-4);
        let o = frame_corners(90.0, 2.0, 50.0, Some(10.0));
        assert!((o[1].x - 10.0).abs() < 1e-4 && (o[1].z + 50.0).abs() < 1e-4);
    }

    #[test]
    fn icon_lies_behind_the_eye_and_is_closed_where_drawn() {
        let m = icon_mesh();
        let (min, max) = m.aabb();
        assert!(min.z >= -1e-5, "nothing in front of the lens: {min}");
        assert!(max.z > 10.0 && max.x > 2.0 && max.y > 3.0, "{max}");
        assert_eq!(m.face_count(), 4 + 6);
    }
}
