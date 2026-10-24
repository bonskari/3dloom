use crate::mesh::Polygon;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use glam::{Mat4, Vec3};

use crate::{FaceId, HalfEdgeId, Mesh, VertexId};

pub fn translate_vertices(
    mesh: &mut Mesh,
    vertices: &[VertexId],
    delta: Vec3,
) -> Result<(), String> {
    let unique: BTreeSet<VertexId> = vertices.iter().copied().collect();
    for id in unique {
        let Some(v) = mesh.vertices.get_mut(id.0 as usize) else {
            return Err(format!("invalid vertex id {}", id.0));
        };
        v.position += delta;
    }
    mesh.check_invariants()
}

pub fn transform_vertices(
    mesh: &mut Mesh,
    vertices: &[VertexId],
    transform: Mat4,
) -> Result<(), String> {
    let unique: BTreeSet<VertexId> = vertices.iter().copied().collect();
    for id in unique {
        let Some(v) = mesh.vertices.get_mut(id.0 as usize) else {
            return Err(format!("invalid vertex id {}", id.0));
        };
        v.position = transform.transform_point3(v.position);
    }
    mesh.check_invariants()
}

pub fn translate_mesh(mesh: &mut Mesh, delta: Vec3) -> Result<(), String> {
    for v in &mut mesh.vertices {
        v.position += delta;
    }
    mesh.check_invariants()
}

pub fn transform_mesh(mesh: &mut Mesh, transform: Mat4) -> Result<(), String> {
    for v in &mut mesh.vertices {
        v.position = transform.transform_point3(v.position);
    }
    mesh.check_invariants()
}

pub fn edge_ring(mesh: &Mesh, seeds: &[HalfEdgeId]) -> Vec<HalfEdgeId> {
    let mut result = BTreeSet::new();
    let mut queue = VecDeque::new();
    for seed in seeds {
        if let Some(edge) = mesh.canonical_edge_id(*seed) {
            if result.insert(edge) {
                queue.push_back(edge);
            }
        }
    }

    while let Some(edge) = queue.pop_front() {
        for face in adjacent_faces(mesh, edge) {
            let Some(loop_edges) = face_half_edges(mesh, face) else {
                continue;
            };
            if loop_edges.len() != 4 {
                continue;
            }
            let canonical: Vec<HalfEdgeId> = loop_edges
                .iter()
                .filter_map(|id| mesh.canonical_edge_id(*id))
                .collect();
            let Some(pos) = canonical.iter().position(|id| *id == edge) else {
                continue;
            };
            let opposite = canonical[(pos + 2) % 4];
            if result.insert(opposite) {
                queue.push_back(opposite);
            }
        }
    }
    result.into_iter().collect()
}

pub fn edge_loop(mesh: &Mesh, seeds: &[HalfEdgeId]) -> Vec<HalfEdgeId> {
    let mut result = BTreeSet::new();
    let mut queue = VecDeque::new();
    for seed in seeds {
        if let Some(edge) = mesh.canonical_edge_id(*seed) {
            if result.insert(edge) {
                queue.push_back(edge);
            }
        }
    }

    while let Some(edge) = queue.pop_front() {
        let Some((a, b)) = mesh.edge_vertices(edge) else {
            continue;
        };

        if is_coplanar_internal_edge(mesh, edge) {
            for vertex in [a, b] {
                let candidates: Vec<_> = incident_edges(mesh, vertex)
                    .into_iter()
                    .filter(|other| {
                        *other != edge
                            && !result.contains(other)
                            && is_coplanar_internal_edge(mesh, *other)
                    })
                    .collect();
                let preferred: Vec<_> = candidates
                    .iter()
                    .copied()
                    .filter(|other| !edges_share_face(mesh, edge, *other))
                    .collect();
                for other in preferred.into_iter().chain(candidates) {
                    if result.insert(other) {
                        queue.push_back(other);
                    }
                }
            }
            continue;
        }

        let Some(pa) = mesh.vertex_position(a) else {
            continue;
        };
        let Some(pb) = mesh.vertex_position(b) else {
            continue;
        };
        let dir = (pb - pa).normalize_or_zero();
        if dir.length_squared() <= f32::EPSILON {
            continue;
        }
        for other in incident_edges(mesh, a)
            .into_iter()
            .chain(incident_edges(mesh, b))
        {
            if other == edge || result.contains(&other) {
                continue;
            }
            let Some((oa, ob)) = mesh.edge_vertices(other) else {
                continue;
            };
            let Some(poa) = mesh.vertex_position(oa) else {
                continue;
            };
            let Some(pob) = mesh.vertex_position(ob) else {
                continue;
            };
            let other_dir = (pob - poa).normalize_or_zero();
            if dir.dot(other_dir).abs() > 0.92 && result.insert(other) {
                queue.push_back(other);
            }
        }
    }
    result.into_iter().collect()
}

pub fn delete_faces(mesh: &mut Mesh, faces: &[FaceId]) -> Result<(), String> {
    let remove: BTreeSet<FaceId> = faces.iter().copied().collect();
    if remove.is_empty() {
        return Ok(());
    }
    let mut polygons = Vec::new();
    for face_i in 0..mesh.face_count() {
        let face = FaceId(face_i as u32);
        if !remove.contains(&face) {
            polygons.push(mesh.polygon(face).ok_or_else(|| format!("invalid face id {}", face_i))?);
        }
    }
    *mesh = rebuild(mesh, &polygons)?;
    mesh.check_invariants()
}

pub fn delete_vertices(mesh: &mut Mesh, vertices: &[VertexId]) -> Result<(), String> {
    let remove_vertices: BTreeSet<VertexId> = vertices.iter().copied().collect();
    let mut remove_faces = Vec::new();
    for face_i in 0..mesh.face_count() {
        let face = FaceId(face_i as u32);
        let Some(ids) = mesh.face_vertex_ids(face) else {
            return Err(format!("invalid face id {}", face_i));
        };
        if ids.iter().any(|id| remove_vertices.contains(id)) {
            remove_faces.push(face);
        }
    }
    delete_faces(mesh, &remove_faces)
}

pub fn delete_edges(mesh: &mut Mesh, edges: &[HalfEdgeId]) -> Result<(), String> {
    let selected: BTreeSet<HalfEdgeId> = edges
        .iter()
        .filter_map(|id| mesh.canonical_edge_id(*id))
        .collect();
    let mut remove_faces = BTreeSet::new();
    for edge in selected {
        for face in adjacent_faces(mesh, edge) {
            remove_faces.insert(face);
        }
    }
    delete_faces(mesh, &remove_faces.into_iter().collect::<Vec<_>>())
}

pub fn dissolve_edges(mesh: &mut Mesh, edges: &[HalfEdgeId]) -> Result<(), String> {
    let selected: BTreeSet<HalfEdgeId> = edges
        .iter()
        .filter_map(|id| mesh.canonical_edge_id(*id))
        .collect();
    if selected.is_empty() {
        return Ok(());
    }

    let mut consumed_faces = BTreeSet::new();
    let mut polygons = Vec::new();
    for edge in selected {
        let faces = adjacent_faces(mesh, edge);
        if faces.len() != 2
            || consumed_faces.contains(&faces[0])
            || consumed_faces.contains(&faces[1])
        {
            continue;
        }
        let Some((a, b)) = mesh.edge_vertices(edge) else {
            continue;
        };
        let Some(ids_a) = mesh.face_vertex_ids(faces[0]) else {
            continue;
        };
        let Some(ids_b) = mesh.face_vertex_ids(faces[1]) else {
            continue;
        };
        let mut merged = path_around_without_edge(&ids_a, b, a)
            .into_iter()
            .chain(path_around_without_edge(&ids_b, a, b))
            .collect::<Vec<_>>();
        merged.dedup();
        if merged.first() == merged.last() {
            merged.pop();
        }
        if merged.len() >= 3 {
            let src = mesh.polygon(faces[0]).ok_or("invalid face")?;
            polygons.push(Polygon::like(&src, merged.into_iter().filter_map(|id| mesh.vertex_position(id)).collect::<Vec<_>>()));
            consumed_faces.insert(faces[0]);
            consumed_faces.insert(faces[1]);
        }
    }

    for face_i in 0..mesh.face_count() {
        let face = FaceId(face_i as u32);
        if !consumed_faces.contains(&face) {
            polygons.push(mesh.polygon(face).ok_or_else(|| format!("invalid face id {}", face_i))?);
        }
    }
    *mesh = rebuild(mesh, &polygons)?;
    mesh.check_invariants()
}

pub fn connect_edge_ring(mesh: &mut Mesh, edges: &[HalfEdgeId]) -> Result<Vec<HalfEdgeId>, String> {
    let selected: BTreeSet<HalfEdgeId> = edges
        .iter()
        .filter_map(|id| mesh.canonical_edge_id(*id))
        .collect();
    if selected.is_empty() {
        return Ok(Vec::new());
    }

    let mut polygons = Vec::new();
    let mut did_split = false;
    let mut new_edge_segments = Vec::new();
    for face_i in 0..mesh.face_count() {
        let face = FaceId(face_i as u32);
        let loop_edges =
            face_half_edges(mesh, face).ok_or_else(|| format!("invalid face loop {}", face_i))?;
        let src = mesh.polygon(face).ok_or_else(|| format!("invalid face {}", face_i))?;
        let positions = src.points.clone();
        if loop_edges.len() == 4 && positions.len() == 4 {
            let canonical: Vec<HalfEdgeId> = loop_edges
                .iter()
                .filter_map(|id| mesh.canonical_edge_id(*id))
                .collect();
            let s0 = selected.contains(&canonical[0]);
            let s1 = selected.contains(&canonical[1]);
            let s2 = selected.contains(&canonical[2]);
            let s3 = selected.contains(&canonical[3]);
            if s0 && s2 {
                let m0 = (positions[0] + positions[1]) * 0.5;
                let m2 = (positions[2] + positions[3]) * 0.5;
                polygons.push(Polygon::like(&src, vec![positions[0], m0, m2, positions[3]]));
                polygons.push(Polygon::like(&src, vec![m0, positions[1], positions[2], m2]));
                new_edge_segments.push((m0, m2));
                did_split = true;
                continue;
            }
            if s1 && s3 {
                let m1 = (positions[1] + positions[2]) * 0.5;
                let m3 = (positions[3] + positions[0]) * 0.5;
                polygons.push(Polygon::like(&src, vec![positions[0], positions[1], m1, m3]));
                polygons.push(Polygon::like(&src, vec![m3, m1, positions[2], positions[3]]));
                new_edge_segments.push((m1, m3));
                did_split = true;
                continue;
            }
        }
        polygons.push(src);
    }

    if !did_split {
        return Err("connect requires a selected edge ring across quad faces".to_owned());
    }
    *mesh = rebuild(mesh, &polygons)?;
    mesh.check_invariants()?;
    Ok(new_edge_segments
        .into_iter()
        .filter_map(|(a, b)| find_edge_by_positions(mesh, a, b))
        .collect())
}

/// Extrude one closed quad/ngon face by `distance` along its current face normal.
/// Rebuilds the mesh from polygon loops, preserving valid closed half-edge topology.
pub fn extrude_face(mesh: &mut Mesh, face: FaceId, distance: f32) -> Result<FaceId, String> {
    let face_vertices = mesh
        .face_vertex_ids(face)
        .ok_or_else(|| format!("invalid face id {}", face.0))?;
    if face_vertices.len() < 3 {
        return Err("cannot extrude degenerate face".to_owned());
    }
    ensure_closed_face(mesh, face)?;

    let original: Vec<Vec3> = face_vertices
        .iter()
        .map(|id| mesh.vertex_position(*id).unwrap())
        .collect();
    let normal = (original[1] - original[0])
        .cross(original[2] - original[0])
        .normalize_or_zero();
    if normal.length_squared() <= f32::EPSILON {
        return Err("cannot extrude face with zero normal".to_owned());
    }
    let offset: Vec<Vec3> = original.iter().map(|p| *p + normal * distance).collect();

    let src = mesh.polygon(face).ok_or("invalid face")?;
    let mut polygons = Vec::new();
    for i in 0..mesh.face_count() {
        let id = FaceId(i as u32);
        if id != face {
            polygons.push(mesh.polygon(id).ok_or_else(|| format!("invalid face id {}", i))?);
        }
    }

    let cap_face = polygons.len();
    polygons.push(Polygon::like(&src, offset.clone()));
    for i in 0..original.len() {
        let j = (i + 1) % original.len();
        polygons.push(Polygon::like(&src, vec![original[i], original[j], offset[j], offset[i]]));
    }

    *mesh = rebuild(mesh, &polygons)?;
    mesh.check_invariants()?;
    Ok(FaceId(cap_face as u32))
}

fn find_edge_by_positions(mesh: &Mesh, a: Vec3, b: Vec3) -> Option<HalfEdgeId> {
    let eps = 1e-5;
    for (i, _) in mesh.half_edges().iter().enumerate() {
        let id = HalfEdgeId(i as u32);
        let canonical = mesh.canonical_edge_id(id)?;
        if canonical != id {
            continue;
        }
        let (va, vb) = mesh.edge_vertices(id)?;
        let pa = mesh.vertex_position(va)?;
        let pb = mesh.vertex_position(vb)?;
        if (pa.distance(a) < eps && pb.distance(b) < eps)
            || (pa.distance(b) < eps && pb.distance(a) < eps)
        {
            return Some(id);
        }
    }
    None
}

fn face_half_edges(mesh: &Mesh, face: FaceId) -> Option<Vec<HalfEdgeId>> {
    let face = mesh.faces().get(face.0 as usize)?;
    let mut ids = Vec::new();
    let start = face.he;
    let mut he_id = start;
    loop {
        ids.push(he_id);
        let he = mesh.half_edges().get(he_id.0 as usize)?;
        he_id = he.next;
        if he_id == start {
            break;
        }
        if ids.len() > 1024 {
            return None;
        }
    }
    Some(ids)
}

fn is_coplanar_internal_edge(mesh: &Mesh, edge: HalfEdgeId) -> bool {
    let faces = adjacent_faces(mesh, edge);
    if faces.len() != 2 {
        return false;
    }
    let Some(a) = face_normal(mesh, faces[0]) else {
        return false;
    };
    let Some(b) = face_normal(mesh, faces[1]) else {
        return false;
    };
    a.dot(b).abs() > 0.98
}

fn edges_share_face(mesh: &Mesh, a: HalfEdgeId, b: HalfEdgeId) -> bool {
    let af = adjacent_faces(mesh, a);
    let bf = adjacent_faces(mesh, b);
    af.iter().any(|face| bf.contains(face))
}

fn face_normal(mesh: &Mesh, face: FaceId) -> Option<Vec3> {
    let positions = mesh.face_positions(face)?;
    if positions.len() < 3 {
        return None;
    }
    for i in 1..positions.len() - 1 {
        let normal = (positions[i] - positions[0])
            .cross(positions[i + 1] - positions[0])
            .normalize_or_zero();
        if normal.length_squared() > f32::EPSILON {
            return Some(normal);
        }
    }
    None
}

fn adjacent_faces(mesh: &Mesh, edge: HalfEdgeId) -> Vec<FaceId> {
    let mut faces = Vec::new();
    let Some(he) = mesh.half_edges().get(edge.0 as usize) else {
        return faces;
    };
    if let Some(face) = he.face {
        faces.push(face);
    }
    if let Some(twin) = mesh.half_edges().get(he.twin.0 as usize) {
        if let Some(face) = twin.face {
            faces.push(face);
        }
    }
    faces
}

fn incident_edges(mesh: &Mesh, vertex: VertexId) -> Vec<HalfEdgeId> {
    let mut edges = BTreeSet::new();
    for (i, _) in mesh.half_edges().iter().enumerate() {
        let id = HalfEdgeId(i as u32);
        let Some((a, b)) = mesh.edge_vertices(id) else {
            continue;
        };
        if a == vertex || b == vertex {
            if let Some(canonical) = mesh.canonical_edge_id(id) {
                edges.insert(canonical);
            }
        }
    }
    edges.into_iter().collect()
}

fn path_around_without_edge(ids: &[VertexId], from: VertexId, to: VertexId) -> Vec<VertexId> {
    let Some(mut i) = ids.iter().position(|id| *id == from) else {
        return Vec::new();
    };
    let mut out = vec![from];
    for _ in 0..ids.len() {
        i = (i + 1) % ids.len();
        out.push(ids[i]);
        if ids[i] == to {
            break;
        }
    }
    out
}

fn ensure_closed_face(mesh: &Mesh, face: FaceId) -> Result<(), String> {
    let ids = mesh
        .face_vertex_ids(face)
        .ok_or_else(|| format!("invalid face id {}", face.0))?;
    let face_data = &mesh.faces()[face.0 as usize];
    let mut he_id = face_data.he;
    for _ in 0..ids.len() {
        let he = &mesh.half_edges()[he_id.0 as usize];
        let twin = &mesh.half_edges()[he.twin.0 as usize];
        if twin.face.is_none() {
            return Err("face extrusion requires a closed mesh face".to_owned());
        }
        he_id = he.next;
    }
    Ok(())
}

/// Rebuilds `old` from `polygons`, carrying face attributes from the
/// polygons and vertex attributes from `old` by position.
pub fn rebuild(old: &Mesh, polygons: &[Polygon]) -> Result<Mesh, String> {
    let attrs = old.vertex_attrs_by_position();
    let mut mesh = build_mesh_from_polygons(polygons)?;
    for v in mesh.vertices.iter_mut() {
        if let Some((c, il, a, h)) = attrs.get(&(v.position.x.to_bits(), v.position.y.to_bits(), v.position.z.to_bits())) {
            v.color = *c;
            v.illum = *il;
            v.alpha = *a;
            v.hidden = *h;
        }
    }
    Ok(mesh)
}

pub fn build_mesh_from_polygons(polygons: &[Polygon]) -> Result<Mesh, String> {
    use crate::indexed::{IndexedFace, IndexedMesh};
    let mut im = IndexedMesh::default();
    let mut by_pos: BTreeMap<(u32, u32, u32), u32> = BTreeMap::new();
    for polygon in polygons {
        if polygon.points.len() < 3 {
            return Err("polygon must have at least three vertices".to_owned());
        }
        let ids: Vec<u32> = polygon
            .points
            .iter()
            .map(|p| *by_pos.entry((p.x.to_bits(), p.y.to_bits(), p.z.to_bits())).or_insert_with(|| im.add_vertex(*p, None)))
            .collect();
        im.faces.push(IndexedFace { ids, smoothing_groups: polygon.smoothing_groups, material_id: polygon.material_id, hidden: polygon.hidden, uv: polygon.uv.clone() });
    }
    im.build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::box_primitive;

    #[test]
    fn translate_single_vertex_preserves_topology() {
        let mut mesh = box_primitive(Vec3::splat(0.5));
        let before = mesh.vertex_position(VertexId(0)).unwrap();
        translate_vertices(&mut mesh, &[VertexId(0)], Vec3::X).unwrap();
        assert_eq!(mesh.vertex_position(VertexId(0)).unwrap(), before + Vec3::X);
        mesh.check_invariants().unwrap();
    }

    #[test]
    fn transform_mesh_preserves_topology() {
        let mut mesh = box_primitive(Vec3::splat(0.5));
        transform_mesh(&mut mesh, Mat4::from_scale(Vec3::splat(2.0))).unwrap();
        let (min, max) = mesh.aabb();
        assert_eq!(min, Vec3::splat(-1.0));
        assert_eq!(max, Vec3::splat(1.0));
    }

    #[test]
    fn extrude_box_face_preserves_closed_topology() {
        let mut mesh = box_primitive(Vec3::splat(0.5));
        let new_face = extrude_face(&mut mesh, FaceId(0), 0.5).unwrap();
        assert_eq!(new_face, FaceId(5));
        assert_eq!(mesh.face_count(), 10);
        mesh.check_invariants().unwrap();
    }

    #[test]
    fn edge_ring_on_box_finds_parallel_ring() {
        let mesh = box_primitive(Vec3::splat(0.5));
        let ring = edge_ring(&mesh, &[HalfEdgeId(0)]);
        assert!(!ring.is_empty());
        assert!(ring.contains(&mesh.canonical_edge_id(HalfEdgeId(0)).unwrap()));
    }

    #[test]
    fn connect_edge_ring_splits_quad_strip() {
        let mut mesh = box_primitive(Vec3::splat(0.5));
        let ring = edge_ring(&mesh, &[HalfEdgeId(0)]);
        connect_edge_ring(&mut mesh, &ring).unwrap();
        assert!(mesh.face_count() > 6);
        mesh.check_invariants().unwrap();
    }

    #[test]
    fn edge_loop_after_connect_ring_selects_connected_loop() {
        let mut mesh = box_primitive(Vec3::splat(0.5));
        let ring = edge_ring(&mesh, &[HalfEdgeId(0)]);
        let connected = connect_edge_ring(&mut mesh, &ring).unwrap();
        assert!(!connected.is_empty());
        let loop_edges = edge_loop(&mesh, &[connected[0]]);
        assert!(
            connected.iter().all(|edge| loop_edges.contains(edge)),
            "connected edges {:?} should be in loop {:?}",
            connected,
            loop_edges
        );
    }
}
