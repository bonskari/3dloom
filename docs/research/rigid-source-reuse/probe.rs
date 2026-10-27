use std::sync::Arc;
#[test]
fn actual_rigid_arc_probe() {
    let path=std::env::var("LOOM_PERF_SCENE").unwrap();
    let value:serde_json::Value=serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let scene:loom_scene::Scene=serde_json::from_value(value["scene"].clone()).unwrap();
    let bones:Vec<_>=scene.nodes().filter(|n|n.object.is_bone()&&n.modifiers.is_empty()&&n.instance_of.is_none()&&!scene.display_of(n).display_as_box&&!n.hidden&&!scene.layer(n.layer).is_some_and(|l|l.hidden)).map(|n|(n.id,Arc::new(n.cage_mesh().into_owned()))).collect();
    let mut caches:Vec<_>=bones.iter().map(|_|crate::prepared_geometry::GeometryCache::default()).collect();
    let data:Vec<_>=bones.iter().zip(&mut caches).map(|((_,mesh),cache)|cache.mesh_data(mesh)).collect();
    let hashes:Vec<_>=bones.iter().zip(&data).map(|((_,m),d)|d.content_hash()^crate::convert::material_ids_hash(m).rotate_left(1)).collect();
    let retained:Vec<_>=bones.iter().map(|(_,mesh)|mesh.clone()).collect();
    let mut baseline=std::time::Duration::ZERO;let mut reused=baseline;let mut prep=baseline;let mut hashing=baseline;let mut bounds=baseline;
    for f in 0..159 {
        scene.set_time(f as f32);
        let matrices:Vec<_>=bones.iter().map(|(id,_)|scene.object_matrix(*id)).collect();
        let start=std::time::Instant::now();
        for (i,((_,mesh),cache)) in bones.iter().zip(&mut caches).enumerate(){
            let t=std::time::Instant::now();let d=cache.mesh_data(mesh);prep+=t.elapsed();
            let t=std::time::Instant::now();let h=d.content_hash()^crate::convert::material_ids_hash(mesh).rotate_left(1);hashing+=t.elapsed();assert_eq!(h,hashes[i]);
            let t=std::time::Instant::now();std::hint::black_box(d.aabb_transformed(matrices[i]));bounds+=t.elapsed();
        }
        std::hint::black_box(start.elapsed());
        let start=std::time::Instant::now();
        for (i,((_,mesh),cache)) in bones.iter().zip(&mut caches).enumerate(){let d=cache.mesh_data(mesh);std::hint::black_box(d.content_hash()^crate::convert::material_ids_hash(mesh).rotate_left(1));std::hint::black_box(d.aabb_transformed(matrices[i]));}
        baseline+=start.elapsed();
        let start=std::time::Instant::now();
        for (i,(_,mesh)) in bones.iter().enumerate(){
            assert!(Arc::ptr_eq(std::hint::black_box(mesh),&retained[i]));
            std::hint::black_box(hashes[i]);std::hint::black_box(data[i].aabb_transformed(matrices[i]));
        }
        reused+=start.elapsed();
    }
    let ms=|d:std::time::Duration|d.as_secs_f64()*1000.0/159.0;
    eprintln!("RIGID count={} faces={} vertices={} baseline_ms={:.6} prep_ms={:.6} hash_ms={:.6} bounds_ms={:.6} reuse_ms={:.6} saved_ms={:.6}",bones.len(),bones.iter().map(|(_,m)|m.face_count()).sum::<usize>(),bones.iter().map(|(_,m)|m.vertex_count()).sum::<usize>(),ms(baseline),ms(prep),ms(hashing),ms(bounds),ms(reused),ms(baseline-reused));
}
