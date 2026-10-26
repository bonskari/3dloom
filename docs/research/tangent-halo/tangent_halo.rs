//! Experimental exact MikkTSpace partitioning; not used by the renderer.
use super::*;
use bevy::render::mesh::VertexAttributeValues;
use std::collections::HashMap;

fn tangent_values(mesh: &Mesh) -> Option<&Vec<[f32; 4]>> {
    match mesh.attribute(Mesh::ATTRIBUTE_TANGENT)? {
        VertexAttributeValues::Float32x4(values) => Some(values),
        _ => None,
    }
}

fn generate(pool: &bevy_tasks::TaskPool, data: &MeshData, chunks: usize) -> Option<Vec<[f32; 4]>> {
    if data.positions.len() != data.normals.len() || data.positions.len() != data.uvs.len() || data.indices.len() % 3 != 0 || chunks < 2 { return None; }
    let triangles = data.indices.len() / 3;
    if triangles < chunks { return None; }
    let mut keys = HashMap::<[u32; 8], usize>::new();
    let mut welds = Vec::with_capacity(data.positions.len());
    for ((p, n), uv) in data.positions.iter().zip(&data.normals).zip(&data.uvs) {
        let values = [p[0], p[1], p[2], n[0], n[1], n[2], uv[0], uv[1]];
        if values.iter().any(|v| !v.is_finite()) { return None; }
        let key = values.map(|v| if v == 0.0 { 0 } else { v.to_bits() });
        let next = keys.len();
        welds.push(*keys.entry(key).or_insert(next));
    }
    let mut faces = vec![Vec::new(); keys.len()];
    let mut dependent = vec![false; triangles];
    for (face, triangle) in data.indices.chunks_exact(3).enumerate() {
        let ids = [triangle[0] as usize, triangle[1] as usize, triangle[2] as usize];
        if ids.iter().any(|i| *i >= welds.len()) { return None; }
        let p = ids.map(|i| Vec3::from_array(data.positions[i]));
        let uv = ids.map(|i| glam::Vec2::from_array(data.uvs[i]));
        let d1 = p[1] - p[0];
        let d2 = p[2] - p[0];
        let t1 = uv[1] - uv[0];
        let t2 = uv[2] - uv[0];
        let area = t1.x * t2.y - t1.y * t2.x;
        let os = (t2.y * d1) - (t1.y * d2);
        let ot = (-t2.x * d1) + (t1.x * d2);
        let os_len = os.length();
        let ot_len = ot.length();
        if !area.is_finite() || !os_len.is_finite() || !ot_len.is_finite() { return None; }
        dependent[face] = p[0] == p[1] || p[0] == p[2] || p[1] == p[2]
            || d1.cross(d2).length_squared() <= f32::MIN_POSITIVE
            || area.abs() <= f32::MIN_POSITIVE || os_len <= f32::MIN_POSITIVE || ot_len <= f32::MIN_POSITIVE;
        if !dependent[face] && (!(os_len / area.abs()).is_finite() || !(ot_len / area.abs()).is_finite()) { return None; }
        for id in ids { faces[welds[id]].push(face); }
    }
    let results = pool.scope(|scope| {
        for chunk in 0..chunks {
            let start = triangles * chunk / chunks;
            let end = triangles * (chunk + 1) / chunks;
            let welds = &welds;
            let faces = &faces;
            let dependent = &dependent;
            scope.spawn(async move {
                let mut include = vec![false; triangles];
                let mut pending = Vec::new();
                for triangle in data.indices[start * 3..end * 3].chunks_exact(3) {
                    for id in triangle {
                        for face in &faces[welds[*id as usize]] {
                            if !include[*face] { include[*face] = true; if dependent[*face] { pending.push(*face); } }
                        }
                    }
                }
                // Zero-UV triangles inherit orientation across corners. Include
                // every fan in their connected dependency closure, not just the
                // target corner's fan. Geometric degenerates receive the same
                // conservative treatment for Mikk's final tangent inheritance.
                while let Some(face) = pending.pop() {
                    for id in &data.indices[face * 3..face * 3 + 3] {
                        for neighbour in &faces[welds[*id as usize]] {
                            if !include[*neighbour] {
                                include[*neighbour] = true;
                                if dependent[*neighbour] { pending.push(*neighbour); }
                            }
                        }
                    }
                }
                // A closure covering most of the mesh offers no useful split.
                if include.iter().filter(|yes| **yes).count() * 4 >= triangles * 3 { return None; }
                let mut part = MeshData { positions: Vec::new(), normals: Vec::new(), uvs: Vec::new(), indices: Vec::new() };
                let mut targets = Vec::new();
                // Global face order preserves the original accumulation order.
                for (face, _) in include.iter().enumerate().filter(|(_, yes)| **yes) {
                    for id in &data.indices[face * 3..face * 3 + 3] {
                        let source = *id as usize;
                        let local = part.positions.len();
                        if face >= start && face < end { targets.push((source, local)); }
                        part.positions.push(data.positions[source]);
                        part.normals.push(data.normals[source]);
                        part.uvs.push(data.uvs[source]);
                        part.indices.push(local as u32);
                    }
                }
                let mut mesh = part.to_bevy();
                mesh.generate_tangents().ok()?;
                let values = tangent_values(&mesh)?;
                Some(targets.into_iter().map(|(source, local)| (source, values[local])).collect::<Vec<_>>())
            });
        }
    });
    let mut tangents = vec![[0.0, 0.0, 0.0, -0.0]; data.positions.len()];
    for result in results { for (source, tangent) in result? { tangents[source] = tangent; } }
    Some(tangents)
}

#[test]
fn complete_weld_halos_match_serial_meshes_and_reject_degenerates() {
    let pool = bevy_tasks::TaskPoolBuilder::new().num_threads(4).build();
    let mut sphere = loom_mesh::sphere_segmented(1.25, 24, 0.0);
    loom_mesh::uvmap::auto_map(&mut sphere, loom_mesh::uvmap::AutoMap::Spherical, 1);
    let data = mesh_data(&sphere);
    let mut serial = data.to_bevy();
    serial.generate_tangents().unwrap();
    let actual = generate(&pool, &data, 4).expect("nondegenerate sphere");
    assert_eq!(actual.iter().map(|t| t.map(f32::to_bits)).collect::<Vec<_>>(), tangent_values(&serial).unwrap().iter().map(|t| t.map(f32::to_bits)).collect::<Vec<_>>());
    let mut degenerate = data;
    degenerate.uvs.fill([0.0; 2]);
    assert!(generate(&pool, &degenerate, 4).is_none());
}

#[test]
#[ignore = "actual posed-scene parity/benchmark requires LOOM_PERF_SCENE"]
fn actual_ray_pose_tangents_match_serial_bytes() {
    let path = std::env::var("LOOM_PERF_SCENE").expect("provide the actual saved Ray scene");
    let value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let scene: loom_scene::Scene = serde_json::from_value(value["scene"].clone()).unwrap();
    let range = value["settings"]["frame_range"].as_array().unwrap();
    let start = range[0].as_f64().unwrap() as f32;
    let end = range[1].as_f64().unwrap() as f32;
    let ids: Vec<_> = scene.nodes().filter(|n| n.name == "Body001" || n.name == "Face001").map(|n| (n.id, n.name.clone())).collect();
    assert_eq!(ids.len(), 2);
    let pool = bevy_tasks::TaskPoolBuilder::new().num_threads(4).build();
    for frame in [start, start + (end-start)*0.25, (start+end)*0.5, start + (end-start)*0.75, end] {
        scene.set_time(frame);
        for (id, name) in &ids {
            let mut mesh = scene.eval_mesh(*id).value.mesh.clone();
            loom_mesh::edit_ops::transform_mesh(&mut mesh, scene.object_matrix(*id)).unwrap();
            let data = mesh_data_by_id(&mesh).into_iter().find(|(id, _)| *id == 0).unwrap().1;
            let mut serial = data.to_bevy();
            let started = std::time::Instant::now();
            serial.generate_tangents().unwrap();
            let serial_ms = started.elapsed().as_secs_f64() * 1000.0;
            let started = std::time::Instant::now();
            let actual = generate(&pool, &data, 4).unwrap_or_else(|| {
                let mut stats = [0; 5];
                for tri in data.indices.chunks_exact(3) {
                    let p = [tri[0], tri[1], tri[2]].map(|i| Vec3::from_array(data.positions[i as usize]));
                    let uv = [tri[0], tri[1], tri[2]].map(|i| glam::Vec2::from_array(data.uvs[i as usize]));
                    let d1 = p[1]-p[0]; let d2=p[2]-p[0]; let t1=uv[1]-uv[0]; let t2=uv[2]-uv[0];
                    if p[0]==p[1] || p[0]==p[2] || p[1]==p[2] { stats[0]+=1; }
                    if d1.cross(d2).length_squared() <= f32::MIN_POSITIVE { stats[1]+=1; }
                    if (t1.x*t2.y-t1.y*t2.x).abs() <= f32::MIN_POSITIVE {stats[2]+=1;}
                    if (t2.y*d1-t1.y*d2).length() <= f32::MIN_POSITIVE {stats[3]+=1;}
                    if (-t2.x*d1+t1.x*d2).length() <= f32::MIN_POSITIVE {stats[4]+=1;}
                }
                panic!("{name} frame {frame} not eligible: duplicate_positions/collinear/uv_zero/os_zero/ot_zero={stats:?}")
            });
            let halo_ms = started.elapsed().as_secs_f64() * 1000.0;
            let expected = tangent_values(&serial).unwrap();
            let mismatches = actual.iter().zip(expected).filter(|(a, b)| a.map(f32::to_bits) != b.map(f32::to_bits)).count();
            eprintln!("HALO name={name} frame={frame} triangles={} serial_ms={serial_ms:.3} halo_ms={halo_ms:.3} mismatches={mismatches}", data.indices.len()/3);
            if mismatches > 0 {
                for (i, (a, b)) in actual.iter().zip(expected).enumerate().filter(|(_, (a,b))| a.map(f32::to_bits) != b.map(f32::to_bits)).take(20) {
                    eprintln!("DIFF vertex={i} actual={a:?} expected={b:?} bits={:?}/{:?}", a.map(f32::to_bits), b.map(f32::to_bits));
                }
            }
            assert_eq!(mismatches, 0, "{name} frame {frame}");
        }
    }
}
