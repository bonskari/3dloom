use super::*;
use crate::convert::{mesh_data, mesh_data_by_id, MeshData};
use glam::Vec3;

fn serial(data: &MeshData) -> Option<Vec<[f32; 4]>> {
    let mut geometry = MeshGeometry { data, tangents: vec![[0.0; 4]; data.positions.len()] };
    if !unsafe { generated::genTangSpace(&mut geometry, 180.0, None) } { return None; }
    for tangent in &mut geometry.tangents { tangent[3] = -tangent[3]; }
    Some(geometry.tangents)
}

fn bits(values: &[[f32; 4]]) -> Vec<[u32; 4]> { values.iter().map(|t| t.map(f32::to_bits)).collect() }

#[test]
fn prepared_groups_preserve_complete_mikk_results() {
    let pool = bevy_tasks::TaskPoolBuilder::new().num_threads(4).build();
    let mut sphere = loom_mesh::sphere_segmented(1.25, 32, 0.0);
    loom_mesh::uvmap::auto_map(&mut sphere, loom_mesh::uvmap::AutoMap::Spherical, 1);
    let mut larger = loom_mesh::sphere_segmented(1.25, 64, 0.0);
    loom_mesh::uvmap::auto_map(&mut larger, loom_mesh::uvmap::AutoMap::Spherical, 1);
    let mut cases = vec![mesh_data(&larger), mesh_data(&sphere), mesh_data(&loom_mesh::box_primitive(Vec3::ONE)), mesh_data(&loom_mesh::quad_primitive(Vec3::ZERO, Vec3::X, Vec3::Y))];
    let mut mirrored = cases[0].clone();
    for uv in &mut mirrored.uvs { uv[0] = -uv[0]; }
    cases.push(mirrored);
    let mut reflected_normals = cases[0].clone();
    for normal in &mut reflected_normals.normals { for c in normal { *c = -*c; } }
    cases.push(reflected_normals);
    let mut split_normals = cases[0].clone();
    for normal in split_normals.normals.iter_mut().take(24) { *normal = [1.0, 0.0, 0.0]; }
    cases.push(split_normals);
    let mut zero_uv = cases[0].clone();
    for uv in zero_uv.uvs.iter_mut().take(21) { *uv = [0.0; 2]; }
    cases.push(zero_uv);
    let mut collapsed = cases[0].clone();
    collapsed.positions[1] = collapsed.positions[0];
    cases.push(collapsed);
    let mut overlap = cases[0].clone();
    overlap.indices.extend_from_within(0..30);
    cases.push(overlap);
    for (i, data) in cases.iter().enumerate() {
        let expected = serial(data).map(|v| bits(&v));
        let actual = generate_parallel(&pool, data).map(|v| bits(&v));
        assert_eq!(actual, expected, "case {i}");
    }
}

#[test]
#[ignore = "release parity/performance test requires LOOM_PERF_SCENE"]
fn actual_all_ray_poses_match_fresh_corrected_mikk() {
    let path = std::env::var("LOOM_PERF_SCENE").expect("provide the real saved scene");
    let value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let scene: loom_scene::Scene = serde_json::from_value(value["scene"].clone()).unwrap();
    let start = value["settings"]["frame_range"][0].as_i64().unwrap();
    let end = value["settings"]["frame_range"][1].as_i64().unwrap();
    let ids: Vec<_> = scene.nodes().filter(|n| n.name == "Body001" || n.name == "Face001").map(|n| (n.id, n.name.clone())).collect();
    assert_eq!(ids.len(), 2);
    let pool = bevy_tasks::TaskPoolBuilder::new().num_threads(4).build();
    let mut totals = [(0.0, 0.0, 0); 2];
    let mut caches = [crate::prepared_geometry::GeometryCache::default(), crate::prepared_geometry::GeometryCache::default()];
    for frame in (start..=end).map(|f| f as f32).chain([39.25]) {
        scene.set_time(frame);
        for (index, (id, name)) in ids.iter().enumerate() {
            let mut mesh = scene.eval_mesh(*id).value.mesh.clone();
            loom_mesh::edit_ops::transform_mesh(&mut mesh, scene.object_matrix(*id)).unwrap();
            let data = mesh_data_by_id(&mesh).into_iter().find(|(id, _)| *id == 0).unwrap().1;
            let cached_full = caches[index].mesh_data(&mesh);
            let cached = crate::convert::mesh_data_part(&mesh, &cached_full, Some(0), false);
            assert_eq!(cached.indices, data.indices, "{name} indices frame {frame}");
            for (actual, expected) in [(&cached.positions, &data.positions), (&cached.normals, &data.normals)] {
                assert_eq!(actual.len(), expected.len());
                assert!(actual.iter().flatten().zip(expected.iter().flatten()).all(|(a,b)| a.to_bits() == b.to_bits()), "{name} geometry bits frame {frame}");
            }
            assert_eq!(cached.uvs.len(), data.uvs.len());
            assert!(cached.uvs.iter().flatten().zip(data.uvs.iter().flatten()).all(|(a,b)| a.to_bits() == b.to_bits()), "{name} UV bits frame {frame}");
            let t = std::time::Instant::now();
            let expected = serial(&data).unwrap();
            totals[index].0 += t.elapsed().as_secs_f64() * 1000.0;
            let t = std::time::Instant::now();
            let actual = generate_parallel(&pool, &cached).expect("prepared generation");
            totals[index].1 += t.elapsed().as_secs_f64() * 1000.0;
            totals[index].2 += 1;
            assert_eq!(bits(&actual), bits(&expected), "{name} frame {frame}");
        }
        if frame as i64 % 30 == 0 { eprintln!("PARITY frame={frame} complete"); }
        loom_render::profile::report_if_due();
    }
    for ((_, name), (serial_ms, parallel_ms, count)) in ids.iter().zip(totals) {
        eprintln!("PREPARED name={name} poses={count} serial_avg_ms={:.3} parallel_avg_ms={:.3}", serial_ms/count as f64, parallel_ms/count as f64);
    }
}

#[test]
fn prepared_groups_support_nested_existing_pool_jobs() {
    let pool = bevy_tasks::TaskPoolBuilder::new().num_threads(4).build();
    let mut mesh = loom_mesh::sphere_segmented(1.0, 32, 0.0);
    loom_mesh::uvmap::auto_map(&mut mesh, loom_mesh::uvmap::AutoMap::Spherical, 1);
    let data = mesh_data(&mesh);
    let expected = bits(&serial(&data).unwrap());
    let actual = pool.scope(|scope| {
        for _ in 0..3 {
            let data = &data;
            let pool = &pool;
            scope.spawn(async move { bits(&generate_parallel(pool, data).unwrap()) });
        }
    });
    for value in actual { assert_eq!(value, expected); }
}

#[test]
fn quad_geometry_keeps_original_serial_averaging() {
    #[derive(Clone)]
    struct Quads { values: Vec<[f32; 4]> }
    impl Geometry for Quads {
        fn num_faces(&self) -> usize { 32 * 32 }
        fn num_vertices_of_face(&self, _: usize) -> usize { 4 }
        fn position(&self, face: usize, corner: usize) -> [f32; 3] {
            let x = (face % 32) as f32 + [0.0, 1.0, 1.0, 0.0][corner];
            let y = (face / 32) as f32 + [0.0, 0.0, 1.0, 1.0][corner];
            [x, y, (x * 0.1).sin() * (y * 0.1).cos()]
        }
        fn normal(&self, _: usize, _: usize) -> [f32; 3] { [0.0, 0.0, 1.0] }
        fn tex_coord(&self, face: usize, corner: usize) -> [f32; 2] { let p = self.position(face, corner); [p[0]/32.0, p[1]/32.0] }
        fn set_tangent_encoded(&mut self, value: [f32; 4], face: usize, corner: usize) { self.values[face*4+corner] = value; }
    }
    let mut original = Quads { values: vec![[0.0; 4]; 32*32*4] };
    let mut prepared = original.clone();
    let pool = bevy_tasks::TaskPoolBuilder::new().num_threads(4).build();
    assert!(unsafe { generated::genTangSpace(&mut original, 180.0, None) });
    assert!(unsafe { generated::genTangSpace(&mut prepared, 180.0, Some(&pool)) });
    assert_eq!(bits(&prepared.values), bits(&original.values));
}

#[test]
#[ignore = "actual preparation hit-rate probe requires LOOM_PERF_SCENE"]
fn actual_ray_preparation_guard_hit_rates() {
    let path = std::env::var("LOOM_PERF_SCENE").expect("real scene");
    let value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let scene: loom_scene::Scene = serde_json::from_value(value["scene"].clone()).unwrap();
    let start = value["settings"]["frame_range"][0].as_i64().unwrap();
    let end = value["settings"]["frame_range"][1].as_i64().unwrap();
    let ids: Vec<_> = scene.nodes().filter(|n| n.name == "Body001" || n.name == "Face001").map(|n| (n.id, n.name.clone())).collect();
    assert_eq!(ids.len(), 2);
    let pool = bevy_tasks::TaskPoolBuilder::new().num_threads(4).build();
    let mut previous: [Option<(capture::Snapshot, Vec<usize>)>; 2] = [None, None];
    let mut counts = [[0usize; 5]; 2];
    capture::enable();
    for frame in (start..=end).map(|f| f as f32).chain([39.25]) {
        scene.set_time(frame);
        for (index, (id, _)) in ids.iter().enumerate() {
            let mut mesh = scene.eval_mesh(*id).value.mesh.clone();
            loom_mesh::edit_ops::transform_mesh(&mut mesh, scene.object_matrix(*id)).unwrap();
            let data = mesh_data_by_id(&mesh).into_iter().find(|(id, _)| *id == 0).unwrap().1;
            generate_parallel(&pool, &data).unwrap();
            let snapshot = capture::take();
            let mut representatives = std::collections::HashMap::new();
            let classes: Vec<_> = snapshot.indices.iter().map(|rep| { let next = representatives.len(); *representatives.entry(*rep).or_insert(next) }).collect();
            if let Some((old, old_classes)) = &previous[index] {
                counts[index][0] += 1;
                counts[index][1] += usize::from(snapshot == *old);
                counts[index][2] += usize::from(snapshot.indices == old.indices);
                counts[index][3] += usize::from(snapshot.flags == old.flags);
                counts[index][4] += usize::from(classes == *old_classes && snapshot.flags == old.flags);
            }
            previous[index] = Some((snapshot, classes));
        }
    }
    for ((_, name), count) in ids.iter().zip(counts) {
        eprintln!("GUARD name={name} transitions={} exact_hits={} representatives_unchanged={} flags_unchanged={} canonical_and_flags_hits={}", count[0], count[1], count[2], count[3], count[4]);
    }
}

#[test]
fn neighbor_cache_checks_changed_topology_welding_and_degeneracy() {
    let pool = bevy_tasks::TaskPoolBuilder::new().num_threads(4).build();
    let base = MeshData {
        positions: vec![[0.0,0.0,0.0],[1.0,0.0,0.0],[0.0,1.0,0.0],[0.0,1.0,0.0],[1.0,0.0,0.0],[1.0,1.0,0.0]],
        normals: vec![[0.0,0.0,1.0];6],
        uvs: vec![[0.0,0.0],[1.0,0.0],[0.0,1.0],[0.0,1.0],[1.0,0.0],[1.0,1.0]],
        indices: vec![0,1,2,3,4,5],
    };
    for change in 0..6 {
        let mut cache = NeighborCache::default();
        assert_eq!(generate_cached(&pool,&base,&mut cache).map(|t|bits(&t)),serial(&base).map(|t|bits(&t)));
        generate_cached(&pool,&base,&mut cache).unwrap();
        assert_eq!((cache.hits,cache.misses),(1,1));
        let mut data=base.clone();
        match change {
            0 => data.positions[3][2]=1.0,
            1 => data.normals[3][0]=0.5,
            2 => data.uvs[3][0]=0.25,
            3 => data.indices[4]=0,
            4 => data.positions[3]=data.positions[4],
            _ => {for uv in &mut data.uvs {uv[0]=-uv[0];}},
        }
        assert_eq!(generate_cached(&pool,&data,&mut cache).map(|t|bits(&t)),serial(&data).map(|t|bits(&t)),"change={change}");
        assert_eq!(cache.misses,2,"change={change} must invalidate adjacency");
    }
    let mut nonmanifold=base.clone();
    nonmanifold.indices.extend([0,1,2]);
    let mut cache=NeighborCache::default();
    for _ in 0..2 {
        assert_eq!(generate_cached(&pool,&nonmanifold,&mut cache).map(|t|bits(&t)),serial(&nonmanifold).map(|t|bits(&t)));
    }
    assert_eq!((cache.hits,cache.misses),(0,2),"ambiguous edges always prepare fresh");
    let mut cache=NeighborCache::default();
    generate_cached(&pool,&base,&mut cache).unwrap();
    let mut moved=base.clone();
    for p in &mut moved.positions {p[1]+=p[0]*0.5;}
    let output=generate_cached(&pool,&moved,&mut cache).unwrap();
    assert_eq!(bits(&output),bits(&serial(&moved).unwrap()));
    assert_eq!(cache.hits,1,"same adjacency can survive deformation");
    assert_ne!(bits(&output),bits(&serial(&base).unwrap()),"current tangent math must still run");
}

#[test]
#[ignore = "paired actual allocation benchmark requires LOOM_PERF_SCENE"]
fn actual_ray_subgroup_storage_reuse_matches_original_and_times() {
    let path=std::env::var("LOOM_PERF_SCENE").expect("real scene");
    let value:serde_json::Value=serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let scene:loom_scene::Scene=serde_json::from_value(value["scene"].clone()).unwrap();
    let ids:Vec<_>=scene.nodes().filter(|n|n.name=="Body001"||n.name=="Face001").map(|n|(n.id,n.name.clone())).collect();
    assert_eq!(ids.len(),2);
    let pool=bevy_tasks::TaskPoolBuilder::new().num_threads(4).build();
    let mut old:[NeighborCache;2]=Default::default();let mut new:[NeighborCache;2]=Default::default();
    let mut totals=[(0.0,0.0);2];
    for (pose,frame) in (0..=157).map(|f|f as f32).chain([39.25]).enumerate() {
        scene.set_time(frame);
        for (i,(id,_)) in ids.iter().enumerate() {
            let mut mesh=scene.eval_mesh(*id).value.mesh.clone();
            loom_mesh::edit_ops::transform_mesh(&mut mesh,scene.object_matrix(*id)).unwrap();
            let data=mesh_data_by_id(&mesh).into_iter().find(|(id,_)|*id==0).unwrap().1;
            let mut values=[None,None];
            for candidate in if pose%2==0 {[false,true]}else{[true,false]} {
                let start=std::time::Instant::now();
                let value=if candidate {generate_cached_allocations(&pool,&data,&mut new[i])}else{generate_cached_original(&pool,&data,&mut old[i])};
                let elapsed=start.elapsed().as_secs_f64()*1000.0;
                if candidate {totals[i].1+=elapsed;}else{totals[i].0+=elapsed;}
                values[usize::from(candidate)]=value;
            }
            assert_eq!(values[0].as_ref().map(|v|bits(v)),values[1].as_ref().map(|v|bits(v)),"frame={frame} node={}",ids[i].1);
        }
    }
    for ((_,name),(original,reused)) in ids.iter().zip(totals) {eprintln!("SUBGROUP_STORAGE name={name} poses=159 original_ms={:.3} reused_ms={:.3}",original/159.0,reused/159.0);}
}

#[test]
fn cpu_group_fixtures_are_bounded_and_do_not_change_tangent_output() {
    let mut mesh=loom_mesh::sphere_segmented(1.0,32,0.0);
    loom_mesh::uvmap::auto_map(&mut mesh,loom_mesh::uvmap::AutoMap::Spherical,1);
    let data=mesh_data(&mesh);
    let expected=generate_serial(&data).unwrap();
    enable_group_capture();
    let actual=generate_serial(&data).unwrap();
    assert_eq!(bits(&actual),bits(&expected));
    let fixtures:Vec<GroupFixture>=take_group_captures();
    assert_eq!(fixtures.len(),256);
    assert!(fixtures.iter().any(|f|f.faces.len()>1));
    for fixture in fixtures {
        assert!(fixture.expected_s.iter().chain(&fixture.expected_t).all(|v|v.is_finite()));
        assert!(fixture.expected_mag_s.is_finite()&&fixture.expected_mag_t.is_finite());
        for face in &fixture.faces {
            let face:&GroupFace=face;
            assert!(face.normal.iter().chain(&face.position).chain(&face.prev).chain(&face.next).chain(&face.os).chain(&face.ot).all(|v|v.is_finite()));
            assert!(face.mag_s.is_finite()&&face.mag_t.is_finite());
            if !face.skip {assert!(face.mag_s>=0.0&&face.mag_t>=0.0);}
        }
    }
    generate_serial(&data).unwrap();
    assert!(take_group_captures().is_empty(),"recording stops after take");
}

#[test]
#[ignore = "paired actual projection benchmark requires LOOM_PERF_SCENE"]
fn actual_ray_projection_reuse_matches_allocation_baseline_and_times() {
    let path=std::env::var("LOOM_PERF_SCENE").expect("real scene");
    let value:serde_json::Value=serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let scene:loom_scene::Scene=serde_json::from_value(value["scene"].clone()).unwrap();
    let ids:Vec<_>=scene.nodes().filter(|n|n.name=="Body001"||n.name=="Face001").map(|n|(n.id,n.name.clone())).collect();
    assert_eq!(ids.len(),2);
    let pool=bevy_tasks::TaskPoolBuilder::new().num_threads(4).build();
    let mut old:[NeighborCache;2]=Default::default();let mut new:[NeighborCache;2]=Default::default();
    let mut totals=[(0.0,0.0);2];
    for (pose,frame) in (0..=157).map(|f|f as f32).chain([39.25]).enumerate() {
        scene.set_time(frame);
        for (i,(id,_)) in ids.iter().enumerate() {
            let mut mesh=scene.eval_mesh(*id).value.mesh.clone();
            loom_mesh::edit_ops::transform_mesh(&mut mesh,scene.object_matrix(*id)).unwrap();
            let data=mesh_data_by_id(&mesh).into_iter().find(|(id,_)|*id==0).unwrap().1;
            let mut values=[None,None];
            for candidate in if pose%2==0 {[false,true]}else{[true,false]} {
                let start=std::time::Instant::now();
                let value=if candidate {generate_cached(&pool,&data,&mut new[i])}else{generate_cached_allocations(&pool,&data,&mut old[i])};
                let elapsed=start.elapsed().as_secs_f64()*1000.0;
                if candidate {totals[i].1+=elapsed;}else{totals[i].0+=elapsed;}
                values[usize::from(candidate)]=value;
            }
            assert_eq!(values[0].as_ref().map(|v|bits(v)),values[1].as_ref().map(|v|bits(v)),"frame={frame} node={}",ids[i].1);
        }
    }
    for ((_,name),(original,reused)) in ids.iter().zip(totals) {eprintln!("GROUP_PROJECTION name={name} poses=159 original_ms={:.3} reused_ms={:.3}",original/159.0,reused/159.0);}
}
