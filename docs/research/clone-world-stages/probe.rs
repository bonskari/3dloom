use std::time::{Duration,Instant};
#[test]
fn actual_clone_world_317(){
    let path=std::env::var("LOOM_PERF_SCENE").unwrap();
    let value:serde_json::Value=serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let scene:loom_scene::Scene=serde_json::from_value(value["scene"].clone()).unwrap();
    let ids:Vec<_>=scene.nodes().filter(|n|n.modifiers.iter().any(|m|matches!(&m.modifier,loom_scene::modifier::Modifier::Skin(_)))).map(|n|n.id).collect();assert_eq!(ids.len(),5);
    let cages:Vec<_>=ids.iter().map(|id|scene.node(*id).unwrap().cage_mesh().into_owned()).collect();
    let modifiers:Vec<_>=ids.iter().map(|id|serde_json::to_vec(&scene.node(*id).unwrap().modifiers).unwrap()).collect();
    let pool=bevy_tasks::TaskPoolBuilder::new().num_threads(4).build();
    let mut clone_time=Duration::ZERO;let mut actual_transform=clone_time;let mut positions_only=clone_time;let mut invariant_time=clone_time;let mut parallel_time=clone_time;let mut count=0;
    let frames=(0..=157).map(|f|f as f32).chain([39.25]).chain((0..=157).map(|f|f as f32+0.5));
    for frame in frames {
        scene.set_time(frame);
        let outputs:Vec<_>=ids.iter().map(|id|scene.eval_mesh(*id)).collect();
        let matrices:Vec<_>=ids.iter().map(|id|scene.object_matrix(*id)).collect();
        let input_copy:Vec<_>=outputs.iter().map(|e|e.value.mesh.clone()).collect();
        let begin=Instant::now();let mut baseline:Vec<_>=outputs.iter().map(|e|e.value.mesh.clone()).collect();clone_time+=begin.elapsed();
        let begin=Instant::now();for(mesh,matrix)in baseline.iter_mut().zip(&matrices){loom_mesh::edit_ops::transform_mesh(mesh,*matrix).unwrap();}actual_transform+=begin.elapsed();
        let mut isolated:Vec<_>=outputs.iter().map(|e|e.value.mesh.clone()).collect();
        let begin=Instant::now();for(mesh,matrix)in isolated.iter_mut().zip(&matrices){for vertex in mesh.vertices_mut(){vertex.position=matrix.transform_point3(vertex.position);}}positions_only+=begin.elapsed();
        let begin=Instant::now();for mesh in &isolated{mesh.check_invariants().unwrap();}invariant_time+=begin.elapsed();
        let begin=Instant::now();let mut parallel=pool.scope(|scope|{
            for(i,(output,matrix))in outputs.iter().zip(&matrices).enumerate(){scope.spawn(async move{let mut mesh=output.value.mesh.clone();loom_mesh::edit_ops::transform_mesh(&mut mesh,*matrix).unwrap();(i,mesh)});}
        });parallel.sort_unstable_by_key(|(i,_)|*i);parallel_time+=begin.elapsed();
        for i in 0..5 {
            assert_eq!(baseline[i],isolated[i],"isolated full Mesh frame={frame} node={:?}",ids[i]);
            assert_eq!(baseline[i],parallel[i].1,"parallel full Mesh frame={frame} node={:?}",ids[i]);
            for (a,b)in baseline[i].vertices().iter().zip(parallel[i].1.vertices()){assert_eq!(a.position.to_array().map(f32::to_bits),b.position.to_array().map(f32::to_bits));}
            assert_eq!(input_copy[i],outputs[i].value.mesh);
            assert_eq!(&cages[i],scene.node(ids[i]).unwrap().cage_mesh().as_ref());

            count+=1;
        }
    }
    for(i,id)in ids.iter().enumerate(){assert_eq!(modifiers[i],serde_json::to_vec(&scene.node(*id).unwrap().modifiers).unwrap());}
    let ms=|t:Duration|t.as_secs_f64()*1000.0/317.0;
    eprintln!("CLONE_WORLD poses=317 meshes={count} clone_ms={:.6} transform_with_validation_ms={:.6} position_only_ms={:.6} invariants_ms={:.6} baseline_ms={:.6} parallel4_ms={:.6} saving_ms={:.6}",ms(clone_time),ms(actual_transform),ms(positions_only),ms(invariant_time),ms(clone_time+actual_transform),ms(parallel_time),ms(clone_time+actual_transform)-ms(parallel_time));
}
