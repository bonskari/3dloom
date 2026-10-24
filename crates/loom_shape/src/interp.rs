//! Cubic Bezier math.

use glam::Vec3;

pub type Bezier = [Vec3; 4];

pub fn point_at(b: &Bezier, t: f32) -> Vec3 {
    let u = 1.0 - t;
    b[0] * (u * u * u) + b[1] * (3.0 * u * u * t) + b[2] * (3.0 * u * t * t) + b[3] * (t * t * t)
}

pub fn tangent_at(b: &Bezier, t: f32) -> Vec3 {
    let u = 1.0 - t;
    let d = (b[1] - b[0]) * (3.0 * u * u) + (b[2] - b[1]) * (6.0 * u * t) + (b[3] - b[2]) * (3.0 * t * t);
    if d.length_squared() < 1e-12 {
        (b[3] - b[0]).normalize_or_zero()
    } else {
        d.normalize()
    }
}

/// De Casteljau split at `t`: the two halves share the split point.
pub fn split_at(b: &Bezier, t: f32) -> (Bezier, Bezier) {
    let p01 = b[0].lerp(b[1], t);
    let p12 = b[1].lerp(b[2], t);
    let p23 = b[2].lerp(b[3], t);
    let p012 = p01.lerp(p12, t);
    let p123 = p12.lerp(p23, t);
    let m = p012.lerp(p123, t);
    ([b[0], p01, p012, m], [m, p123, p23, b[3]])
}

/// Straight segment: both inner control points on the chord.
pub fn is_straight(b: &Bezier) -> bool {
    let chord = b[3] - b[0];
    let len = chord.length();
    if len < 1e-9 {
        return (b[1] - b[0]).length() < 1e-6 && (b[2] - b[3]).length() < 1e-6;
    }
    let dev = |p: Vec3| (p - b[0]).cross(chord).length() / len;
    dev(b[1]) < 1e-5 && dev(b[2]) < 1e-5
}

/// Parameter of the point nearest to `p` (coarse sampling, then refinement).
pub fn nearest_t(b: &Bezier, p: Vec3) -> f32 {
    let mut best = (0.0f32, f32::INFINITY);
    for i in 0..=32 {
        let t = i as f32 / 32.0;
        let d = point_at(b, t).distance_squared(p);
        if d < best.1 {
            best = (t, d);
        }
    }
    let mut step = 1.0 / 64.0;
    let mut t = best.0;
    for _ in 0..24 {
        for cand in [t - step, t + step] {
            let c = cand.clamp(0.0, 1.0);
            let d = point_at(b, c).distance_squared(p);
            if d < best.1 {
                best = (c, d);
                t = c;
            }
        }
        step *= 0.5;
    }
    t
}

/// Approximate arc length by sampling.
pub fn length(b: &Bezier, samples: usize) -> f32 {
    let n = samples.max(1);
    let mut len = 0.0;
    let mut prev = b[0];
    for i in 1..=n {
        let p = point_at(b, i as f32 / n as f32);
        len += prev.distance(p);
        prev = p;
    }
    len
}

/// Parameter at which `distance` of arc length has been travelled from t = 0.
pub fn t_at_length(b: &Bezier, distance: f32) -> f32 {
    let n = 64;
    let mut acc = 0.0;
    let mut prev = b[0];
    for i in 1..=n {
        let t = i as f32 / n as f32;
        let p = point_at(b, t);
        let d = prev.distance(p);
        if acc + d >= distance {
            let f = if d > 1e-9 { (distance - acc) / d } else { 0.0 };
            return ((i - 1) as f32 + f) / n as f32;
        }
        acc += d;
        prev = p;
    }
    1.0
}

/// Flat enough for the Adaptive interpolation: the control polygon deviates
/// from the chord by less than the tolerance.
pub fn flat_enough(b: &Bezier, tolerance: f32) -> bool {
    let chord = b[3] - b[0];
    let len = chord.length();
    if len < 1e-9 {
        return true;
    }
    let dev = |p: Vec3| (p - b[0]).cross(chord).length() / len;
    dev(b[1]) <= tolerance && dev(b[2]) <= tolerance
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bezier_endpoints_split_and_nearest() {
        let b = [Vec3::ZERO, Vec3::new(0.0, 5.0, 0.0), Vec3::new(10.0, 5.0, 0.0), Vec3::new(10.0, 0.0, 0.0)];
        assert_eq!(point_at(&b, 0.0), b[0]);
        assert_eq!(point_at(&b, 1.0), b[3]);
        let (l, r) = split_at(&b, 0.5);
        assert_eq!(l[3], r[0]);
        assert!((l[3] - point_at(&b, 0.5)).length() < 1e-6);
        assert!((point_at(&l, 1.0) - point_at(&b, 0.5)).length() < 1e-6);
        assert!((point_at(&r, 0.5) - point_at(&b, 0.75)).length() < 1e-5);
        let t = nearest_t(&b, Vec3::new(5.0, 10.0, 0.0));
        assert!((t - 0.5).abs() < 1e-3, "{t}");
        assert!(!is_straight(&b));
        assert!(is_straight(&[Vec3::ZERO, Vec3::ZERO, Vec3::X * 2.0, Vec3::X * 2.0]));
        let straight = [Vec3::ZERO, Vec3::X, Vec3::X * 2.0, Vec3::X * 3.0];
        assert!((length(&straight, 16) - 3.0).abs() < 1e-5);
        assert!((point_at(&straight, t_at_length(&straight, 1.5)).x - 1.5).abs() < 0.05);
    }
}
