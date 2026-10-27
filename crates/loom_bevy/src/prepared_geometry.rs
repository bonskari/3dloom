//! Lightweight Bevy geometry preparation with pose-independent neighbour fans.

use glam::Vec3;
use loom_mesh::{FaceId, HalfEdge, HalfEdgeId, Mesh, VertexId};

/// Topology is compared exactly: poses and corner attributes never invalidate fans.
#[derive(Default)]
pub(crate) struct GeometryCache {
    vertex_edges: Vec<Option<HalfEdgeId>>,
    half_edges: Vec<HalfEdge>,
    face_edges: Vec<HalfEdgeId>,
    fans: Vec<Vec<FaceId>>,
    face_normals: Vec<Vec3>,
    vertex_normals: Vec<(u32, Vec3)>,
}

impl GeometryCache {
    pub(crate) fn mesh_data(&mut self, mesh: &Mesh) -> crate::convert::MeshData {
        let topology_scope = loom_render::profile::Scope::new("bevy.geometry_topology");
        let same = self.vertex_edges.len() == mesh.vertex_count()
            && self.half_edges == mesh.half_edges()
            && self.face_edges.len() == mesh.face_count()
            && self.vertex_edges.iter().zip(mesh.vertices()).all(|(he, v)| *he == v.he)
            && self.face_edges.iter().zip(mesh.faces()).all(|(he, f)| *he == f.he);
        if !same {
            self.vertex_edges = mesh.vertices().iter().map(|v| v.he).collect();
            self.half_edges = mesh.half_edges().to_vec();
            self.face_edges = mesh.faces().iter().map(|f| f.he).collect();
            self.fans = (0..mesh.vertex_count()).map(|i| mesh.faces_around_vertex(VertexId(i as u32))).collect();
        }
        drop(topology_scope);
        let normals_scope = loom_render::profile::Scope::new("bevy.geometry_face_normals");
        self.face_normals.clear();
        self.face_normals.extend((0..mesh.face_count()).map(|i| mesh.face_normal(FaceId(i as u32))));
        self.vertex_normals.resize(mesh.vertex_count(), (0, Vec3::ZERO));
        self.vertex_normals.fill((0, Vec3::ZERO));
        drop(normals_scope);
        let _corners_scope = loom_render::profile::Scope::new("bevy.geometry_corners");
        let mut out = crate::convert::MeshData {
            positions: Vec::with_capacity(mesh.half_edges().len()),
            normals: Vec::with_capacity(mesh.half_edges().len()),
            uvs: Vec::with_capacity(mesh.half_edges().len()),
            indices: Vec::with_capacity(mesh.face_count() * 3),
        };
        let uv_map = mesh.map(1);
        let normal_map = mesh.map(loom_mesh::NORMALS_CHANNEL);
        let corner_value = |map: Option<&loom_mesh::half_edge::MapChannel>, he: HalfEdgeId| {
            let map = map?;
            map.verts.get(*map.corners.get(he.0 as usize)? as usize).copied()
        };
        for (fi, face) in mesh.faces().iter().enumerate() {
            if face.hidden { continue; }
            let start = face.he;
            let mut current = start;
            let base = out.positions.len() as u32;
            loop {
                let he = &mesh.half_edges()[current.0 as usize];
                let vi = he.origin.0 as usize;
                let explicit = corner_value(normal_map, current).and_then(|n| (n.length_squared() > 1e-12).then(|| n.normalize()));
                let normal = match explicit {
                    Some(normal) => normal,
                    None if face.smoothing_groups == 0 => self.face_normals[fi],
                    None => {
                        let cached = &mut self.vertex_normals[vi];
                        if cached.0 == face.smoothing_groups { cached.1 } else {
                            let mut sum = Vec3::ZERO;
                            for f in &self.fans[vi] {
                                if mesh.faces()[f.0 as usize].smoothing_groups & face.smoothing_groups != 0 {
                                    sum += self.face_normals[f.0 as usize];
                                }
                            }
                            let normal = if sum.length_squared() < 1e-12 { self.face_normals[fi] } else { sum.normalize() };
                            if sum.length_squared() >= 1e-12 { *cached = (face.smoothing_groups, normal); }
                            normal
                        }
                    }
                };
                out.positions.push(mesh.vertices()[vi].position.to_array());
                out.normals.push(normal.to_array());
                let uv = corner_value(uv_map, current).map(|t| [t.x, t.y]).unwrap_or([0.0, 0.0]);
                // Preserve the reference identity transform's floating-point operations.
                out.uvs.push([1.0 * uv[0] + 0.0 * uv[1] + 0.0, 1.0 - (0.0 * uv[0] + 1.0 * uv[1] + 0.0)]);
                current = he.next;
                if current == start { break; }
                if out.positions.len() - base as usize > 1024 { panic!("face loop did not close — mesh invariants violated"); }
            }
            let count = out.positions.len() as u32 - base;
            if count < 3 {
                out.positions.truncate(base as usize);
                out.normals.truncate(base as usize);
                out.uvs.truncate(base as usize);
            } else {
                for i in 1..count - 1 { out.indices.extend([base, base + i, base + i + 1]); }
            }
        }
        out
    }
}

/// Prepares independent objects on the existing bounded worker pool. Small
/// meshes stay on the caller; results and their caches retain input order.
pub(crate) fn prepare_batch<K: Send + 'static>(
    pool: &bevy_tasks::TaskPool,
    jobs: Vec<(K, GeometryCache, &Mesh)>,
) -> Vec<(K, GeometryCache, crate::convert::MeshData)> {
    let mut ready = Vec::with_capacity(jobs.len());
    let parallel = pool.scope(|scope| {
        for (index, (key, mut cache, mesh)) in jobs.into_iter().enumerate() {
            if mesh.face_count() < 1024 {
                let data = cache.mesh_data(mesh);
                ready.push((index, (key, cache, data)));
            } else {
                scope.spawn(async move {
                    let data = cache.mesh_data(mesh);
                    (index, (key, cache, data))
                });
            }
        }
    });
    ready.extend(parallel);
    ready.sort_unstable_by_key(|(index, _)| *index);
    ready.into_iter().map(|(_, value)| value).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn exact(cache: &mut GeometryCache, mesh: &loom_mesh::Mesh) {
        let expected = crate::convert::mesh_data(mesh);
        let actual = cache.mesh_data(mesh);
        assert_eq!(actual.indices, expected.indices);
        let bits = |data: &crate::convert::MeshData| data.positions.iter().flatten().chain(data.normals.iter().flatten()).chain(data.uvs.iter().flatten()).map(|x| x.to_bits()).collect::<Vec<_>>();
        assert_eq!(bits(&actual), bits(&expected));
    }
    #[test]
    fn parallel_batch_preserves_input_order_and_returns_reusable_caches() {
        let pool = bevy_tasks::TaskPoolBuilder::new().num_threads(4).build();
        let mut meshes = vec![loom_mesh::sphere_segmented(1.0, 64, 0.0), loom_mesh::box_primitive(Vec3::ONE), loom_mesh::sphere_segmented(1.0, 48, 0.0)];
        let jobs = meshes.iter().enumerate().map(|(i,m)| (i, GeometryCache::default(), m)).collect();
        let first = prepare_batch(&pool, jobs);
        let mut caches = Vec::new();
        for ((id, cache, actual), mesh) in first.into_iter().zip(&meshes) {
            assert_eq!(id, caches.len());
            assert_eq!(actual, crate::convert::mesh_data(mesh));
            caches.push(cache);
        }
        meshes[0].vertices_mut()[0].position.z += 0.25;
        meshes[2].faces_mut()[0].hidden = true;
        let jobs = meshes.iter().zip(caches).enumerate().map(|(i,(m,c))| (i,c,m)).collect();
        let second = prepare_batch(&pool, jobs);
        for ((id, _, actual), mesh) in second.into_iter().zip(&meshes) {
            assert_eq!(actual, crate::convert::mesh_data(mesh), "object {id}");
        }
    }

    #[test]
    fn matches_reference_through_pose_and_corner_property_edits() {
        let mut mesh = loom_mesh::sphere_segmented(1.25, 16, 0.0);
        let mut cache = GeometryCache::default();
        exact(&mut cache, &mesh);
        for (i, face) in mesh.faces_mut().iter_mut().enumerate() {
            face.smoothing_groups = [0, 1, 3, 2][i % 4];
            face.hidden = i % 7 == 0;
            face.material_id = (i % 5) as u16;
        }
        exact(&mut cache, &mesh);
        for (i, v) in mesh.vertices_mut().iter_mut().enumerate() { v.position.z += (i as f32).sin() * 0.07; }
        let he = mesh.faces()[1].he;
        mesh.set_corner_uv(1, he, glam::Vec3::new(-0.0, 0.37, 0.0));
        mesh.set_corner_uv(loom_mesh::NORMALS_CHANNEL, he, glam::Vec3::new(1.0, 2.0, 3.0));
        exact(&mut cache, &mesh);
        mesh.vertices_mut()[0].he = None;
        exact(&mut cache, &mesh);
        exact(&mut cache, &loom_mesh::box_primitive(glam::Vec3::ONE));
        exact(&mut cache, &loom_mesh::Mesh::default());
    }
    #[test]
    fn ngons_degenerate_normals_and_same_count_topology_changes_match() {
        let mut mesh = loom_mesh::cylinder_primitive(1.0, 2.0, 9);
        let mut cache = GeometryCache::default();
        exact(&mut cache, &mesh);
        // Rotate a face's starting corner without changing any counts.
        let next = mesh.half_edges()[mesh.faces()[0].he.0 as usize].next;
        mesh.faces_mut()[0].he = next;
        exact(&mut cache, &mesh);
        // Collapse the real mesh to exercise per-face degenerate fallbacks.
        for v in mesh.vertices_mut() { v.position = glam::Vec3::ZERO; }
        for face in mesh.faces_mut() { face.smoothing_groups = 1; }
        exact(&mut cache, &mesh);
        // Same counts, changed half-edge topology: reverse the primitive.
        let mut json = serde_json::to_value(&mesh).unwrap();
        let edges = json["half_edges"].as_array_mut().unwrap();
        for edge in edges {
            let next = edge["next"].clone();
            edge["next"] = edge["prev"].clone();
            edge["prev"] = next;
        }
        let reversed: loom_mesh::Mesh = serde_json::from_value(json).unwrap();
        exact(&mut cache, &reversed);
    }

    /// Opt in with LOOM_GEOMETRY_SCENE=/path/to/scene.loom.
    #[test]
    fn saved_scene_geometry_matches_reference_at_multiple_poses() {
        let Ok(path) = std::env::var("LOOM_GEOMETRY_SCENE") else { return; };
        let text = std::fs::read_to_string(path).unwrap();
        let mut document: serde_json::Value = serde_json::from_str(&text).unwrap();
        let scene: loom_scene::Scene = serde_json::from_value(document.get_mut("scene").map(serde_json::Value::take).unwrap_or(document)).unwrap();
        let mut caches = std::collections::HashMap::new();
        let mut tested = 0;
        for time in [0.0, 3.0, 11.0] {
            scene.set_time(time);
            for node in scene.nodes() {
                let evaluated = scene.eval_mesh(node.id);
                if evaluated.value.mesh.face_count() == 0 { continue; }
                exact(caches.entry(node.id).or_insert_with(GeometryCache::default), &evaluated.value.mesh);
                tested += 1;
            }
        }
        assert!(tested > 0, "scene must contain evaluated geometry");
    }

    #[test]
    #[ignore = "release parity/performance test requires LOOM_PERF_SCENE"]
    fn actual_all_poses_geometry_performance() {
        let path = std::env::var("LOOM_PERF_SCENE").expect("provide the real saved scene");
        let mut value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let start = value["settings"]["frame_range"][0].as_i64().unwrap();
        let end = value["settings"]["frame_range"][1].as_i64().unwrap();
        let scene: loom_scene::Scene = serde_json::from_value(value["scene"].take()).unwrap();
        let ids: Vec<_> = scene.nodes().filter(|n| n.modifiers.iter().any(|m| matches!(&m.modifier, loom_scene::modifier::Modifier::Skin(_)))).map(|n| (n.id, n.name.clone())).collect();
        assert!(!ids.is_empty());
        let mut caches: Vec<_> = ids.iter().map(|_| GeometryCache::default()).collect();
        let mut totals = vec![(0.0, 0.0, 0); ids.len()];
        let pool = bevy_tasks::TaskPoolBuilder::new().num_threads(4).build();
        let mut batch_caches: Vec<_> = ids.iter().map(|_| GeometryCache::default()).collect();
        let mut batch_total = 0.0;
        let mut pose_count = 0;
        for frame in (start..=end).map(|f| f as f32).chain([39.25]) {
            scene.set_time(frame);
            let mut meshes = Vec::new();
            let mut serial_data = Vec::new();
            for (i, (id, name)) in ids.iter().enumerate() {
                let mut mesh = scene.eval_mesh(*id).value.mesh.clone();
                loom_mesh::edit_ops::transform_mesh(&mut mesh, scene.object_matrix(*id)).unwrap();
                let t = std::time::Instant::now();
                let expected = crate::convert::mesh_data(&mesh);
                totals[i].0 += t.elapsed().as_secs_f64() * 1000.0;
                let t = std::time::Instant::now();
                let actual = caches[i].mesh_data(&mesh);
                totals[i].1 += t.elapsed().as_secs_f64() * 1000.0;
                totals[i].2 += 1;
                assert_eq!(actual.indices, expected.indices, "{name} frame {frame}");
                assert_eq!(actual.content_hash(), expected.content_hash(), "{name} frame {frame}");
                for (a, b) in [(&actual.positions, &expected.positions), (&actual.normals, &expected.normals)] {
                    assert_eq!(a.len(), b.len(), "{name} frame {frame}");
                    assert!(a.iter().flatten().zip(b.iter().flatten()).all(|(a,b)| a.to_bits() == b.to_bits()), "{name} frame {frame}");
                }
                assert_eq!(actual.uvs.len(), expected.uvs.len(), "{name} frame {frame}");
                assert!(actual.uvs.iter().flatten().zip(expected.uvs.iter().flatten()).all(|(a,b)| a.to_bits() == b.to_bits()), "{name} frame {frame}");
                meshes.push(mesh);
                serial_data.push(expected);
            }
            let jobs = meshes.iter().zip(std::mem::take(&mut batch_caches)).enumerate().map(|(i,(m,c))| (i,c,m)).collect();
            let t = std::time::Instant::now();
            let batch = prepare_batch(&pool, jobs);
            batch_total += t.elapsed().as_secs_f64() * 1000.0;
            pose_count += 1;
            for ((i, cache, actual), expected) in batch.into_iter().zip(&serial_data) {
                assert_eq!(i, batch_caches.len());
                assert_eq!(actual.indices, expected.indices);
                for (a,b) in [(&actual.positions, &expected.positions), (&actual.normals, &expected.normals)] {
                    assert_eq!(a.len(), b.len());
                    assert!(a.iter().flatten().zip(b.iter().flatten()).all(|(a,b)| a.to_bits() == b.to_bits()));
                }
                assert_eq!(actual.uvs.len(), expected.uvs.len());
                assert!(actual.uvs.iter().flatten().zip(expected.uvs.iter().flatten()).all(|(a,b)| a.to_bits() == b.to_bits()));
                batch_caches.push(cache);
            }
        }
        eprintln!("GEOMETRY batch_avg_ms={:.3} poses={pose_count}", batch_total/pose_count as f64);
        loom_render::profile::report_if_due();
        for ((_, name), (reference, cached, count)) in ids.iter().zip(totals) {
            eprintln!("GEOMETRY name={name} poses={count} reference_avg_ms={:.3} cached_avg_ms={:.3}", reference/count as f64, cached/count as f64);
        }
    }

}
