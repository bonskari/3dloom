//! Selection regions (Main Toolbar > Selection Region flyout) in screen
//! space, with Window/Crossing semantics.

use eframe::egui;

#[derive(Debug, Clone)]
pub enum Region {
    Rect(egui::Rect),
    Circle { center: egui::Pos2, radius: f32 },
    /// Fence (clicked corners) or Lasso (dragged path), closed implicitly.
    Polygon(Vec<egui::Pos2>),
    /// Paint: everything within `radius` of the dragged path.
    Paint { path: Vec<egui::Pos2>, radius: f32 },
}

impl Region {
    pub fn contains(&self, p: egui::Pos2) -> bool {
        match self {
            Region::Rect(r) => r.contains(p),
            Region::Circle { center, radius } => center.distance(p) < *radius,
            Region::Polygon(poly) => point_in_polygon(p, poly),
            Region::Paint { path, radius } => {
                if path.len() == 1 {
                    return path[0].distance(p) <= *radius;
                }
                path.windows(2).any(|w| segment_distance(p, w[0], w[1]) <= *radius)
            }
        }
    }

    /// Window: every point inside. Crossing: any point inside.
    pub fn selects(&self, points: &[egui::Pos2], window: bool) -> bool {
        if points.is_empty() {
            return false;
        }
        if window {
            points.iter().all(|p| self.contains(*p))
        } else {
            points.iter().any(|p| self.contains(*p))
        }
    }

    /// Crossing selection of a displayed line, including between its endpoints.
    pub fn intersects_segment(&self, a: egui::Pos2, b: egui::Pos2) -> bool {
        if self.contains(a) || self.contains(b) { return true; }
        match self {
            Region::Rect(r) => {
                let corners = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
                corners.windows(2).any(|w| segment_pair_distance(a, b, w[0], w[1]) <= 1e-6)
            }
            Region::Circle { center, radius } => segment_distance(*center, a, b) <= *radius,
            Region::Polygon(points) => points.len() >= 3 && points.iter().zip(points.iter().cycle().skip(1))
                .any(|(c, d)| segment_pair_distance(a, b, *c, *d) <= 1e-6),
            Region::Paint { path, radius } => if path.len() == 1 { segment_distance(path[0], a, b) <= *radius }
                else { path.windows(2).any(|w| segment_pair_distance(a, b, w[0], w[1]) <= *radius) },
        }
    }

    pub fn bounding_rect(&self) -> egui::Rect {
        match self {
            Region::Rect(r) => *r,
            Region::Circle { center, radius } => egui::Rect::from_center_size(*center, egui::vec2(*radius * 2.0, *radius * 2.0)),
            Region::Polygon(pts) => egui::Rect::from_points(pts),
            Region::Paint { path, radius } => egui::Rect::from_points(path).expand(*radius),
        }
    }
}

fn point_in_polygon(p: egui::Pos2, poly: &[egui::Pos2]) -> bool {
    if poly.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn segment_distance(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    if len2 <= f32::EPSILON {
        return a.distance(p);
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    (a + ab * t).distance(p)
}

fn segment_pair_distance(a: egui::Pos2, b: egui::Pos2, c: egui::Pos2, d: egui::Pos2) -> f32 {
    let cross = |u: egui::Vec2, v: egui::Vec2| u.x * v.y - u.y * v.x;
    let ab = b - a;
    let cd = d - c;
    let denominator = cross(ab, cd);
    if denominator.abs() > 1e-6 {
        let t = cross(c - a, cd) / denominator;
        let u = cross(c - a, ab) / denominator;
        if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) { return 0.; }
    }
    segment_distance(a, c, d).min(segment_distance(b, c, d))
        .min(segment_distance(c, a, b)).min(segment_distance(d, a, b))
}

#[cfg(test)]
mod tests {
    #[test]
    fn selection_regions_intersect_control_segments_between_vertices() {
        let regions = [
            Region::Rect(egui::Rect::from_min_max(egui::pos2(4.9, -0.1), egui::pos2(5.1, 0.1))),
            Region::Circle { center: egui::pos2(5., 0.), radius: 0.1 },
            Region::Polygon(vec![egui::pos2(4.9, -0.1), egui::pos2(5.1, -0.1), egui::pos2(5.1, 0.1), egui::pos2(4.9, 0.1)]),
            Region::Paint { path: vec![egui::pos2(5., -1.), egui::pos2(5., 1.)], radius: 0.1 },
        ];
        for region in regions {
            assert!(region.intersects_segment(egui::pos2(0., 0.), egui::pos2(10., 0.)));
            assert!(!region.intersects_segment(egui::pos2(0., 20.), egui::pos2(10., 20.)));
            assert!(region.intersects_segment(egui::pos2(5., 0.), egui::pos2(5., 0.)));
        }
        let edge = Region::Rect(egui::Rect::from_min_max(egui::pos2(4., 0.), egui::pos2(6., 1.)));
        assert!(edge.intersects_segment(egui::pos2(0., 0.), egui::pos2(10., 0.)));
    }
    use super::*;
    use eframe::egui::pos2;

    #[test]
    fn window_crossing_marquee_selects_fully_enclosed_only() {
        let r = Region::Rect(egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(10.0, 10.0)));
        let inside = [pos2(2.0, 2.0), pos2(8.0, 8.0)];
        let half = [pos2(2.0, 2.0), pos2(15.0, 8.0)];
        let outside = [pos2(12.0, 2.0), pos2(15.0, 8.0)];
        assert!(r.selects(&inside, true) && r.selects(&inside, false));
        assert!(!r.selects(&half, true) && r.selects(&half, false));
        assert!(!r.selects(&outside, true) && !r.selects(&outside, false));
    }

    #[test]
    fn circular_and_lasso_regions_contain_points() {
        let c = Region::Circle { center: pos2(0.0, 0.0), radius: 5.0 };
        assert!(c.contains(pos2(3.0, 3.0)) && !c.contains(pos2(5.0, 5.0)));
        let l = Region::Polygon(vec![pos2(0.0, 0.0), pos2(10.0, 0.0), pos2(10.0, 10.0), pos2(0.0, 10.0)]);
        assert!(l.contains(pos2(5.0, 5.0)) && !l.contains(pos2(11.0, 5.0)));
        let p = Region::Paint { path: vec![pos2(0.0, 0.0), pos2(10.0, 0.0)], radius: 2.0 };
        assert!(p.contains(pos2(5.0, 1.0)) && !p.contains(pos2(5.0, 4.0)));
    }
}
