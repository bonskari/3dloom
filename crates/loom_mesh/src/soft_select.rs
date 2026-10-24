//! Editable Poly > Soft Selection: per-vertex weights from the distance to
//! the selection through a falloff / pinch / bubble curve.

use crate::select_ops::vertex_neighbours;
use crate::{Mesh, VertexId};
use glam::Vec3;
use std::collections::BinaryHeap;

/// The AffectRegion curve: 1 on the selection, 0 at `falloff`.
pub fn curve(dist: f32, falloff: f32, pinch: f32, bubble: f32) -> f32 {
    if falloff <= 0.0 || dist >= falloff {
        return 0.0;
    }
    let u = (falloff - dist) / falloff;
    let u2 = u * u;
    let s = 1.0 - u;
    ((3.0 * u * bubble * s + 3.0 * u2 * (1.0 - pinch)) * s + u * u2).clamp(0.0, 1.0)
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    dist: f32,
    hops: u32,
    v: u32,
}
impl PartialEq for Entry {
    fn eq(&self, o: &Self) -> bool {
        self.dist == o.dist
    }
}
impl Eq for Entry {}
impl PartialOrd for Entry {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Entry {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        o.dist.total_cmp(&self.dist)
    }
}

/// Distance of every vertex to the nearest selected vertex: straight-line,
/// or along at most `edge_distance` edges when Edge Distance is on
/// (vertices farther than that get infinity).
pub fn distances(mesh: &Mesh, selected: &[VertexId], edge_distance: Option<u32>) -> Vec<f32> {
    let n = mesh.vertex_count();
    let mut out = vec![f32::INFINITY; n];
    match edge_distance {
        None => {
            let sel: Vec<Vec3> = selected.iter().filter_map(|v| mesh.vertex_position(*v)).collect();
            for (i, v) in mesh.vertices().iter().enumerate() {
                out[i] = sel.iter().map(|p| p.distance(v.position)).fold(f32::INFINITY, f32::min);
            }
        }
        Some(max_hops) => {
            let mut heap = BinaryHeap::new();
            for v in selected {
                if (v.0 as usize) < n {
                    out[v.0 as usize] = 0.0;
                    heap.push(Entry { dist: 0.0, hops: 0, v: v.0 });
                }
            }
            while let Some(e) = heap.pop() {
                if e.dist > out[e.v as usize] || e.hops >= max_hops {
                    continue;
                }
                let p = mesh.vertices()[e.v as usize].position;
                for nb in vertex_neighbours(mesh, VertexId(e.v)) {
                    let d = e.dist + p.distance(mesh.vertices()[nb.0 as usize].position);
                    if d < out[nb.0 as usize] {
                        out[nb.0 as usize] = d;
                        heap.push(Entry { dist: d, hops: e.hops + 1, v: nb.0 });
                    }
                }
            }
        }
    }
    out
}

/// Soft selection weight of every vertex (1 on the selection).
#[allow(clippy::too_many_arguments)]
pub fn weights(mesh: &Mesh, selected: &[VertexId], falloff: f32, pinch: f32, bubble: f32, edge_distance: Option<u32>, affect_backfacing: bool, view_dir: Option<Vec3>) -> Vec<f32> {
    let d = distances(mesh, selected, edge_distance);
    let mut w: Vec<f32> = d.iter().map(|d| curve(*d, falloff, pinch, bubble)).collect();
    for v in selected {
        if (v.0 as usize) < w.len() {
            w[v.0 as usize] = 1.0;
        }
    }
    if let (false, Some(dir)) = (affect_backfacing, view_dir) {
        for (i, wi) in w.iter_mut().enumerate() {
            let vid = VertexId(i as u32);
            if selected.contains(&vid) {
                continue;
            }
            let faces = mesh.faces_around_vertex(vid);
            if !faces.is_empty() && faces.iter().all(|f| mesh.face_normal(*f).dot(dir) > 0.0) {
                *wi = 0.0;
            }
        }
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weight_is_one_on_selection_zero_past_falloff() {
        let m = crate::plane_segmented(40.0, 40.0, 4, 4);
        let sel = VertexId(12);
        let w = weights(&m, &[sel], 15.0, 0.0, 0.0, None, true, None);
        assert_eq!(w[12], 1.0);
        let p = m.vertex_position(sel).unwrap();
        for (i, v) in m.vertices().iter().enumerate() {
            let d = v.position.distance(p);
            if d >= 15.0 {
                assert_eq!(w[i], 0.0, "vertex {i} at {d}");
            } else if i != 12 {
                assert!(w[i] > 0.0 && w[i] < 1.0, "vertex {i} at {d}: {}", w[i]);
            }
        }
        // Halfway through the falloff the plain curve is 0.5.
        assert!((curve(7.5, 15.0, 0.0, 0.0) - 0.5).abs() < 1e-6);
        assert!(curve(7.5, 15.0, 1.0, 0.0) < 0.5, "pinch pulls the curve in");
        assert!(curve(7.5, 15.0, 0.0, 1.0) > 0.5, "bubble pushes it out");
    }

    #[test]
    fn edge_distance_limits_rings() {
        let m = crate::plane_segmented(40.0, 40.0, 4, 4);
        let w = weights(&m, &[VertexId(12)], 100.0, 0.0, 0.0, Some(1), true, None);
        let on: usize = w.iter().filter(|x| **x > 0.0).count();
        assert_eq!(on, 5, "one ring: the vertex and its four edge neighbours");
    }
}
