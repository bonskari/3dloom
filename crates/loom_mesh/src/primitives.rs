use crate::Mesh;
use glam::Vec3;
use std::collections::HashMap;

/// Constructs a box mesh centered at the origin with the given half-extents.
/// 8 vertices, 6 quad faces, 12 edges (24 half-edges).
pub fn box_primitive(half_extents: Vec3) -> Mesh {
    use crate::half_edge::{Face, FaceId, HalfEdge, HalfEdgeId, Vertex, VertexId};

    let e = half_extents;
    let positions = [
        Vec3::new(-e.x, -e.y, -e.z), // 0
        Vec3::new(e.x, -e.y, -e.z),  // 1
        Vec3::new(-e.x, e.y, -e.z),  // 2
        Vec3::new(e.x, e.y, -e.z),   // 3
        Vec3::new(-e.x, -e.y, e.z),  // 4
        Vec3::new(e.x, -e.y, e.z),   // 5
        Vec3::new(-e.x, e.y, e.z),   // 6
        Vec3::new(e.x, e.y, e.z),    // 7
    ];
    // Faces as CCW-from-outside vertex loops:
    let faces_v: [[u32; 4]; 6] = [
        [0, 2, 3, 1], // -Z
        [4, 5, 7, 6], // +Z
        [0, 1, 5, 4], // -Y
        [2, 6, 7, 3], // +Y
        [0, 4, 6, 2], // -X
        [1, 3, 7, 5], // +X
    ];

    let mut mesh = Mesh::default();
    let half_edges=std::sync::Arc::make_mut(&mut mesh.half_edges).edges_mut();
    for p in positions {
        mesh.vertices.push(Vertex::new(p, None));
    }

    // Allocate half-edges per face: each face has 4 half-edges,
    // arranged so that he[face*4 + i].next = he[face*4 + (i+1) % 4].
    // Twin pairing is done in a second pass using an edge-keyed map.
    use std::collections::HashMap;
    let mut edge_map: HashMap<(u32, u32), HalfEdgeId> = HashMap::new();

    for (fi, loop_) in faces_v.iter().enumerate() {
        let face_id = FaceId(fi as u32);
        let base = half_edges.len() as u32;
        for i in 0..4 {
            let from = loop_[i];
            let to = loop_[(i + 1) % 4];
            let he_id = HalfEdgeId(base + i as u32);
            let next = HalfEdgeId(base + ((i as u32 + 1) % 4));
            let prev = HalfEdgeId(base + ((i as u32 + 3) % 4));
            half_edges.push(HalfEdge {
                origin: VertexId(from),
                twin: HalfEdgeId(u32::MAX), // filled in below
                next,
                prev,
                face: Some(face_id),
            });
            edge_map.insert((from, to), he_id);
            let v = &mut mesh.vertices[from as usize];
            if v.he.is_none() {
                v.he = Some(he_id);
            }
        }
        mesh.faces.push(Face::new(HalfEdgeId(base)));
    }

    // Pair twins
    for ((from, to), &he_id) in edge_map.iter() {
        let twin_id = edge_map
            .get(&(*to, *from))
            .copied()
            .expect("box: every directed edge must have a twin");
        half_edges[he_id.0 as usize].twin = twin_id;
    }

    mesh
}

/// Constructs an open one-face quad mesh given a corner and two edge vectors.
/// The quad's normal is `edge_u × edge_v` (so the corners are CCW from outside).
/// Boundary half-edges (the four twins of the face's loop) have `face = None`.
pub fn quad_primitive(corner: Vec3, edge_u: Vec3, edge_v: Vec3) -> Mesh {
    use crate::half_edge::{Face, FaceId, HalfEdge, HalfEdgeId, Vertex, VertexId};

    let positions = [
        corner,
        corner + edge_u,
        corner + edge_u + edge_v,
        corner + edge_v,
    ];

    let mut mesh = Mesh::default();
    let half_edges=std::sync::Arc::make_mut(&mut mesh.half_edges).edges_mut();
    for p in positions {
        mesh.vertices.push(Vertex::new(p, None));
    }

    // Interior half-edges 0..3 around the single face.
    for i in 0u32..4 {
        let next = (i + 1) % 4;
        let prev = (i + 3) % 4;
        half_edges.push(HalfEdge {
            origin: VertexId(i),
            twin: HalfEdgeId(i + 4),
            next: HalfEdgeId(next),
            prev: HalfEdgeId(prev),
            face: Some(FaceId(0)),
        });
    }
    // Boundary half-edges 4..7 around the open boundary in reverse direction.
    for i in 0u32..4 {
        let origin = (i + 1) % 4;
        let next_bdry = ((i + 3) % 4) + 4;
        let prev_bdry = ((i + 1) % 4) + 4;
        half_edges.push(HalfEdge {
            origin: VertexId(origin),
            twin: HalfEdgeId(i),
            next: HalfEdgeId(next_bdry),
            prev: HalfEdgeId(prev_bdry),
            face: None,
        });
    }
    for v in 0..4 {
        mesh.vertices[v].he = Some(HalfEdgeId(v as u32));
    }
    mesh.faces.push(Face::new(HalfEdgeId(0)));

    mesh
}

pub fn cylinder_primitive(radius: f32, height: f32, segments: u32) -> Mesh {
    let n = segments.max(3);
    let h = height * 0.5;
    let mut bottom = Vec::new();
    let mut top = Vec::new();
    for i in 0..n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        let p = Vec3::new(a.cos() * radius, a.sin() * radius, 0.0);
        bottom.push(p - Vec3::Z * h);
        top.push(p + Vec3::Z * h);
    }
    let mut polygons = Vec::new();
    polygons.push(bottom.iter().rev().copied().collect::<Vec<_>>());
    polygons.push(top.clone());
    for i in 0..n as usize {
        let j = (i + 1) % n as usize;
        polygons.push(vec![bottom[i], bottom[j], top[j], top[i]]);
    }
    build_closed(&polygons)
}

pub fn sphere_primitive(radius: f32, segments: u32, rings: u32) -> Mesh {
    let seg = segments.max(8);
    let ring = rings.max(4);
    let top = Vec3::Z * radius;
    let bottom = -Vec3::Z * radius;
    let mut rows = Vec::new();
    for r in 1..ring {
        let v = r as f32 / ring as f32;
        let theta = v * std::f32::consts::PI;
        let z = theta.cos() * radius;
        let rr = theta.sin() * radius;
        let mut row = Vec::new();
        for s in 0..seg {
            let a = s as f32 / seg as f32 * std::f32::consts::TAU;
            row.push(Vec3::new(a.cos() * rr, a.sin() * rr, z));
        }
        rows.push(row);
    }
    let mut polygons = Vec::new();
    for s in 0..seg as usize {
        let j = (s + 1) % seg as usize;
        polygons.push(vec![top, rows[0][s], rows[0][j]]);
    }
    for r in 0..rows.len() - 1 {
        for s in 0..seg as usize {
            let j = (s + 1) % seg as usize;
            polygons.push(vec![rows[r][s], rows[r + 1][s], rows[r + 1][j], rows[r][j]]);
        }
    }
    let last = rows.len() - 1;
    for s in 0..seg as usize {
        let j = (s + 1) % seg as usize;
        polygons.push(vec![rows[last][j], rows[last][s], bottom]);
    }
    build_closed(&polygons)
}

fn build_closed(polygons: &[Vec<Vec3>]) -> Mesh {
    use crate::half_edge::{Face, FaceId, HalfEdge, HalfEdgeId, Vertex, VertexId};
    let mut mesh = Mesh::default();
    let half_edges=std::sync::Arc::make_mut(&mut mesh.half_edges).edges_mut();
    let mut vertex_map: HashMap<(u32, u32, u32), VertexId> = HashMap::new();
    let mut edge_map: HashMap<(VertexId, VertexId), HalfEdgeId> = HashMap::new();
    for (face_i, poly) in polygons.iter().enumerate() {
        let face = FaceId(face_i as u32);
        let mut ids = Vec::new();
        for pos in poly {
            let key = (pos.x.to_bits(), pos.y.to_bits(), pos.z.to_bits());
            let id = *vertex_map.entry(key).or_insert_with(|| {
                let id = VertexId(mesh.vertices.len() as u32);
                mesh.vertices.push(Vertex::new(*pos, None));
                id
            });
            ids.push(id);
        }
        let base = half_edges.len() as u32;
        for i in 0..ids.len() {
            let he = HalfEdgeId(base + i as u32);
            let next = HalfEdgeId(base + ((i + 1) % ids.len()) as u32);
            let prev = HalfEdgeId(base + ((i + ids.len() - 1) % ids.len()) as u32);
            edge_map.insert((ids[i], ids[(i + 1) % ids.len()]), he);
            if mesh.vertices[ids[i].0 as usize].he.is_none() {
                mesh.vertices[ids[i].0 as usize].he = Some(he);
            }
            half_edges.push(HalfEdge {
                origin: ids[i],
                twin: he,
                next,
                prev,
                face: Some(face),
            });
        }
        mesh.faces.push(Face::new(HalfEdgeId(base)));
    }
    for ((from, to), he) in edge_map.clone() {
        if let Some(twin) = edge_map.get(&(to, from)) {
            half_edges[he.0 as usize].twin = *twin;
        }
    }
    mesh
}

/// Builds a five-quad "test room" — a floor + four inward-facing walls.
/// Returns one mesh per surface so each can be selected independently.
/// `half_xy` is the room's half-extent on X and Y; `height` is the wall height (Z).
pub fn test_room(half_xy: f32, height: f32) -> Vec<Mesh> {
    let s = half_xy;
    let h = height;
    vec![
        // Floor (normal +Z)
        quad_primitive(
            Vec3::new(-s, -s, 0.0),
            Vec3::new(2.0 * s, 0.0, 0.0),
            Vec3::new(0.0, 2.0 * s, 0.0),
        ),
        // North wall at +Y (normal -Y, facing into the room)
        quad_primitive(
            Vec3::new(-s, s, 0.0),
            Vec3::new(2.0 * s, 0.0, 0.0),
            Vec3::new(0.0, 0.0, h),
        ),
        // South wall at -Y (normal +Y)
        quad_primitive(
            Vec3::new(s, -s, 0.0),
            Vec3::new(-2.0 * s, 0.0, 0.0),
            Vec3::new(0.0, 0.0, h),
        ),
        // East wall at +X (normal -X)
        quad_primitive(
            Vec3::new(s, s, 0.0),
            Vec3::new(0.0, -2.0 * s, 0.0),
            Vec3::new(0.0, 0.0, h),
        ),
        // West wall at -X (normal +X)
        quad_primitive(
            Vec3::new(-s, -s, 0.0),
            Vec3::new(0.0, 2.0 * s, 0.0),
            Vec3::new(0.0, 0.0, h),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_has_correct_element_counts() {
        let m = box_primitive(Vec3::splat(0.5));
        assert_eq!(m.vertex_count(), 8);
        assert_eq!(m.edge_count(), 12);
        assert_eq!(m.face_count(), 6);
    }

    #[test]
    fn box_invariants_hold() {
        let m = box_primitive(Vec3::splat(0.5));
        m.check_invariants()
            .expect("box should be a valid half-edge mesh");
    }

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn box_invariants_hold_for_any_size(
            x in 0.01f32..100.0,
            y in 0.01f32..100.0,
            z in 0.01f32..100.0,
        ) {
            let m = box_primitive(Vec3::new(x, y, z));
            prop_assert!(m.check_invariants().is_ok());
            prop_assert_eq!(m.vertex_count(), 8);
            prop_assert_eq!(m.edge_count(), 12);
            prop_assert_eq!(m.face_count(), 6);
        }
    }

    #[test]
    fn quad_has_one_face_four_edges_eight_half_edges() {
        let q = quad_primitive(Vec3::ZERO, Vec3::X, Vec3::Y);
        assert_eq!(q.vertex_count(), 4);
        assert_eq!(q.face_count(), 1);
        assert_eq!(q.edge_count(), 4); // 8 half-edges / 2
        q.check_invariants().expect("quad invariants must hold");
    }

    #[test]
    fn test_room_has_five_meshes() {
        let r = test_room(2.5, 3.0);
        assert_eq!(r.len(), 5);
        for m in &r {
            m.check_invariants()
                .expect("room piece invariants must hold");
        }
    }
}
