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

#[cfg(test)]
mod tests {
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
