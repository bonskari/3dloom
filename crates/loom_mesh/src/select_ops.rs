//! Editable Poly > Selection rollout: Grow, Shrink, Ring/Loop stepping,
//! By Vertex, level conversion, borders and elements.

use crate::{FaceId, HalfEdgeId, Mesh, VertexId};
use std::collections::BTreeSet;

/// Sub-object level: Vertex / Edge / Border / Polygon / Element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubLevel {
    Vertex,
    Edge,
    Border,
    Face,
    Element,
}

fn canonical(mesh: &Mesh, e: HalfEdgeId) -> Option<HalfEdgeId> {
    mesh.canonical_edge_id(e)
}

/// Edges touching a vertex (canonical ids).
pub fn vertex_edges(mesh: &Mesh, v: VertexId) -> Vec<HalfEdgeId> {
    let mut out = BTreeSet::new();
    for he in mesh.outgoing_half_edges(v) {
        if let Some(c) = canonical(mesh, he) {
            out.insert(c);
        }
    }
    out.into_iter().collect()
}

/// Vertices joined to `v` by an edge.
pub fn vertex_neighbours(mesh: &Mesh, v: VertexId) -> Vec<VertexId> {
    vertex_edges(mesh, v).into_iter().filter_map(|e| mesh.edge_vertices(e)).map(|(a, b)| if a == v { b } else { a }).collect()
}

/// The vertices of a set of sub-objects.
pub fn vertices_of(mesh: &Mesh, level: SubLevel, ids: &[u32]) -> BTreeSet<VertexId> {
    let mut out = BTreeSet::new();
    for id in ids {
        match level {
            SubLevel::Vertex => {
                if (*id as usize) < mesh.vertex_count() {
                    out.insert(VertexId(*id));
                }
            }
            SubLevel::Edge | SubLevel::Border => {
                if (*id as usize) >= mesh.edge_count() {
                    continue;
                }
                if let Some((a, b)) = mesh.edge_vertices(HalfEdgeId(*id)) {
                    out.insert(a);
                    out.insert(b);
                }
            }
            SubLevel::Face | SubLevel::Element => {
                if let Some(vs) = mesh.face_vertex_ids(FaceId(*id)) {
                    out.extend(vs);
                }
            }
        }
    }
    out
}

/// Faces around a vertex.
fn vertex_faces(mesh: &Mesh, v: VertexId) -> Vec<FaceId> {
    mesh.faces_around_vertex(v)
}

/// An edge is a border when one side has no face.
pub fn is_border_edge(mesh: &Mesh, e: HalfEdgeId) -> bool {
    let he = &mesh.half_edges()[e.0 as usize];
    let twin = &mesh.half_edges()[he.twin.0 as usize];
    he.face.is_none() || twin.face.is_none()
}

/// All border edges (canonical ids).
pub fn border_edges(mesh: &Mesh) -> Vec<HalfEdgeId> {
    (0..mesh.half_edges().len() as u32)
        .map(HalfEdgeId)
        .filter(|h| canonical(mesh, *h) == Some(*h) && is_border_edge(mesh, *h))
        .collect()
}

/// Every face connected to `seed` through shared edges.
pub fn element_of(mesh: &Mesh, seed: FaceId) -> Vec<FaceId> {
    let mut seen: BTreeSet<FaceId> = BTreeSet::new();
    let mut stack = vec![seed];
    while let Some(f) = stack.pop() {
        if f.0 as usize >= mesh.face_count() || !seen.insert(f) {
            continue;
        }
        let start = mesh.faces()[f.0 as usize].he;
        let mut he = start;
        loop {
            let h = &mesh.half_edges()[he.0 as usize];
            if let Some(nf) = mesh.half_edges()[h.twin.0 as usize].face {
                if !seen.contains(&nf) {
                    stack.push(nf);
                }
            }
            he = h.next;
            if he == start {
                break;
            }
        }
    }
    seen.into_iter().collect()
}

/// Grow: add every sub-object adjacent (sharing a vertex) to the selection.
pub fn grow(mesh: &Mesh, level: SubLevel, ids: &[u32]) -> Vec<u32> {
    let verts = vertices_of(mesh, level, ids);
    let mut out: BTreeSet<u32> = ids.iter().copied().collect();
    match level {
        SubLevel::Vertex => {
            for v in &verts {
                for n in vertex_neighbours(mesh, *v) {
                    out.insert(n.0);
                }
            }
        }
        SubLevel::Edge | SubLevel::Border => {
            for v in &verts {
                for e in vertex_edges(mesh, *v) {
                    if level == SubLevel::Edge || is_border_edge(mesh, e) {
                        out.insert(e.0);
                    }
                }
            }
        }
        SubLevel::Face | SubLevel::Element => {
            for v in &verts {
                for f in vertex_faces(mesh, *v) {
                    out.insert(f.0);
                }
            }
        }
    }
    out.into_iter().collect()
}

/// A vertex on an open edge of the mesh.
pub fn is_border_vertex(mesh: &Mesh, v: VertexId) -> bool {
    vertex_edges(mesh, v).iter().any(|e| is_border_edge(mesh, *e))
}

/// Shrink: drop every sub-object on the boundary of the selection (one that
/// touches an unselected sub-object or the mesh border).
pub fn shrink(mesh: &Mesh, level: SubLevel, ids: &[u32]) -> Vec<u32> {
    let selected: BTreeSet<u32> = ids.iter().copied().collect();
    let keep = |id: u32| -> bool {
        match level {
            SubLevel::Vertex => !is_border_vertex(mesh, VertexId(id)) && vertex_neighbours(mesh, VertexId(id)).iter().all(|n| selected.contains(&n.0)),
            SubLevel::Edge => {
                let Some((a, b)) = mesh.edge_vertices(HalfEdgeId(id)) else { return false };
                [a, b].iter().all(|v| !is_border_vertex(mesh, *v) && vertex_edges(mesh, *v).iter().all(|e| selected.contains(&e.0)))
            }
            SubLevel::Border => {
                let Some((a, b)) = mesh.edge_vertices(HalfEdgeId(id)) else { return false };
                [a, b].iter().all(|v| vertex_edges(mesh, *v).iter().filter(|e| is_border_edge(mesh, **e)).all(|e| selected.contains(&e.0)))
            }
            SubLevel::Face | SubLevel::Element => {
                let Some(vs) = mesh.face_vertex_ids(FaceId(id)) else { return false };
                vs.iter().all(|v| !is_border_vertex(mesh, *v) && vertex_faces(mesh, *v).iter().all(|f| selected.contains(&f.0)))
            }
        }
    };
    ids.iter().copied().filter(|id| keep(*id)).collect()
}

/// Edge loop: from each seed, walk straight on through every valence-4
/// vertex (the edge sharing no face with the one we came from) until the
/// loop closes, hits the border, or meets a pole.
pub fn edge_loop(mesh: &Mesh, seeds: &[HalfEdgeId]) -> Vec<HalfEdgeId> {
    let mut out: BTreeSet<HalfEdgeId> = BTreeSet::new();
    for seed in seeds {
        let Some(c) = canonical(mesh, *seed) else { continue };
        out.insert(c);
        let Some((a, b)) = mesh.edge_vertices(c) else { continue };
        for (mut e, mut v) in [(c, a), (c, b)] {
            loop {
                let Some(n) = loop_continuation(mesh, e, v) else { break };
                if !out.insert(n) {
                    break;
                }
                let Some((x, y)) = mesh.edge_vertices(n) else { break };
                v = if x == v { y } else { x };
                e = n;
            }
        }
    }
    out.into_iter().collect()
}

/// Edge ring: across every quad from each seed, both ways, until the ring
/// closes or runs out of quads.
pub fn edge_ring(mesh: &Mesh, seeds: &[HalfEdgeId]) -> Vec<HalfEdgeId> {
    let mut out: BTreeSet<HalfEdgeId> = BTreeSet::new();
    let mut queue: Vec<HalfEdgeId> = seeds.iter().filter_map(|s| canonical(mesh, *s)).collect();
    while let Some(e) = queue.pop() {
        if !out.insert(e) {
            continue;
        }
        for side in [false, true] {
            if let Some(o) = opposite_in_quad(mesh, e, side) {
                if !out.contains(&o) {
                    queue.push(o);
                }
            }
        }
    }
    out.into_iter().collect()
}

/// Edges and faces touching any of `verts` (Select By Vertex).
pub fn by_vertex(mesh: &Mesh, verts: &[VertexId], level: SubLevel) -> Vec<u32> {
    let mut out = BTreeSet::new();
    for v in verts {
        match level {
            SubLevel::Edge => out.extend(vertex_edges(mesh, *v).into_iter().map(|e| e.0)),
            SubLevel::Border => out.extend(vertex_edges(mesh, *v).into_iter().filter(|e| is_border_edge(mesh, *e)).map(|e| e.0)),
            SubLevel::Face => out.extend(vertex_faces(mesh, *v).into_iter().map(|f| f.0)),
            SubLevel::Element => {
                for f in vertex_faces(mesh, *v) {
                    out.extend(element_of(mesh, f).into_iter().map(|f| f.0));
                }
            }
            SubLevel::Vertex => {
                out.insert(v.0);
            }
        }
    }
    out.into_iter().collect()
}

/// Ctrl-click (touching) / Shift-click (contained) level conversion.
pub fn convert_selection(mesh: &Mesh, from: SubLevel, ids: &[u32], to: SubLevel, contained: bool) -> Vec<u32> {
    let verts = vertices_of(mesh, from, ids);
    let vlist: Vec<VertexId> = verts.iter().copied().collect();
    let candidates = by_vertex(mesh, &vlist, to);
    if !contained || to == SubLevel::Vertex {
        return candidates;
    }
    candidates
        .into_iter()
        .filter(|id| vertices_of(mesh, to, &[*id]).iter().all(|v| verts.contains(v)))
        .collect()
}

/// The quad-opposite edge of `e` in the face on the given side.
fn opposite_in_quad(mesh: &Mesh, e: HalfEdgeId, twin_side: bool) -> Option<HalfEdgeId> {
    let he = HalfEdgeId(if twin_side { mesh.half_edges()[e.0 as usize].twin.0 } else { e.0 });
    let h = &mesh.half_edges()[he.0 as usize];
    h.face?;
    // A quad: next.next is the opposite half-edge.
    let n1 = mesh.half_edges()[h.next.0 as usize].next;
    let n2 = mesh.half_edges()[n1.0 as usize].next;
    let n3 = mesh.half_edges()[n2.0 as usize].next;
    if n3 != he {
        return None;
    }
    canonical(mesh, n1)
}

/// Loop Shift: move each edge to the edge across its adjacent quad
/// (up = the face of the canonical half-edge, down = the twin's face).
pub fn loop_step(mesh: &Mesh, edges: &[HalfEdgeId], up: bool) -> Vec<HalfEdgeId> {
    let mut out = BTreeSet::new();
    for e in edges {
        let Some(c) = canonical(mesh, *e) else { continue };
        if let Some(o) = opposite_in_quad(mesh, c, !up) {
            out.insert(o);
        }
    }
    out.into_iter().collect()
}

/// The edge continuing a loop through `v` past `e` (the edge across the
/// vertex that shares no face with `e`), when the vertex has valence 4.
fn loop_continuation(mesh: &Mesh, e: HalfEdgeId, v: VertexId) -> Option<HalfEdgeId> {
    let around = vertex_edges(mesh, v);
    // A border loop follows the mesh border: the other border edge at `v`.
    if is_border_edge(mesh, e) {
        return around.into_iter().find(|o| *o != e && is_border_edge(mesh, *o));
    }
    if around.len() != 4 {
        return None;
    }
    let faces_of = |x: HalfEdgeId| -> Vec<FaceId> {
        let h = &mesh.half_edges()[x.0 as usize];
        let t = &mesh.half_edges()[h.twin.0 as usize];
        h.face.into_iter().chain(t.face).collect()
    };
    let fe = faces_of(e);
    around.into_iter().find(|o| *o != e && faces_of(*o).iter().all(|f| !fe.contains(f)))
}

/// Ring Shift: move each edge one step along its loop direction (up = past
/// the canonical half-edge's destination, down = past its origin).
pub fn ring_step(mesh: &Mesh, edges: &[HalfEdgeId], up: bool) -> Vec<HalfEdgeId> {
    let mut out = BTreeSet::new();
    for e in edges {
        let Some(c) = canonical(mesh, *e) else { continue };
        let Some((a, b)) = mesh.edge_vertices(c) else { continue };
        let v = if up { b } else { a };
        if let Some(n) = loop_continuation(mesh, c, v) {
            out.insert(n);
        }
    }
    out.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn grow_and_shrink_faces_on_a_box() {
        let m = crate::box_primitive(Vec3::splat(0.5));
        let g = grow(&m, SubLevel::Face, &[0]);
        assert_eq!(g.len(), 5, "a box face touches four others by vertex");
        let s = shrink(&m, SubLevel::Face, &g);
        assert_eq!(s, vec![0]);
    }

    #[test]
    fn border_edge_loop_runs_around_the_plane() {
        let m = crate::plane_segmented(10.0, 10.0, 2, 2);
        let b = border_edges(&m);
        assert_eq!(b.len(), 8);
        let l = edge_loop(&m, &[b[0]]);
        assert_eq!(l.len(), 8, "a border edge loops around the whole border");
        // An interior edge loops straight across.
        let interior = (0..m.half_edges().len() as u32).map(HalfEdgeId).find(|h| canonical(&m, *h) == Some(*h) && !is_border_edge(&m, *h)).unwrap();
        assert_eq!(edge_loop(&m, &[interior]).len(), 2);
    }

    #[test]
    fn convert_contained_requires_all_vertices() {
        let m = crate::box_primitive(Vec3::splat(0.5));
        let vs: Vec<u32> = m.face_vertex_ids(FaceId(0)).unwrap().iter().map(|v| v.0).collect();
        assert_eq!(convert_selection(&m, SubLevel::Vertex, &vs, SubLevel::Face, true), vec![0]);
        assert_eq!(convert_selection(&m, SubLevel::Vertex, &vs, SubLevel::Face, false).len(), 5);
    }
}
