use loom_mesh::Mesh;
use std::{hint::black_box,time::Instant,process::{Command,Stdio},io::Write};
fn current_source_positions(mesh: &Mesh) -> Result<Vec<[f32; 4]>, String> {
    if mesh.vertex_count() == 0 {
        return Err("GPU runtime requires nonempty source positions".into());
    }
    mesh.vertices()
        .iter()
        .map(|v| {
            if v.position.is_finite() {
                Ok([v.position.x, v.position.y, v.position.z, 1.0])
            } else {
                Err("nonfinite current world position".into())
            }
        })
        .collect()
}
fn reserved_source_positions(mesh: &Mesh) -> Result<Vec<[f32; 4]>, String> {
    if mesh.vertex_count() == 0 {
        return Err("GPU runtime requires nonempty source positions".into());
    }
    let mut values = Vec::with_capacity(mesh.vertex_count());
    for v in mesh.vertices() {
        if !v.position.is_finite() {
            return Err("nonfinite current world position".into());
        }
        values.push([v.position.x, v.position.y, v.position.z, 1.0]);
    }
    Ok(values)
}

fn scene_sha(scene: &loom_scene::Scene) -> String {
    let bytes = serde_json::to_vec(scene).unwrap();
    let mut child=Command::new("sha256sum").stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(&bytes).unwrap();
    let out=child.wait_with_output().unwrap();assert!(out.status.success());String::from_utf8(out.stdout).unwrap().split_whitespace().next().unwrap().to_owned()
}
fn equal_bits(a:&[[f32;4]],b:&[[f32;4]]) {assert_eq!(a.len(),b.len());for(a,b)in a.iter().zip(b){assert_eq!(a.map(f32::to_bits),b.map(f32::to_bits));}}
fn main(){
    let path=std::env::var("LOOM_PERF_SCENE").unwrap();let input=std::fs::read(&path).unwrap();let wrapper:serde_json::Value=serde_json::from_slice(&input).unwrap();
    let scene:loom_scene::Scene=serde_json::from_value(wrapper["scene"].clone()).unwrap();scene.set_time(39.0);
    let ids:Vec<_>=scene.nodes().filter(|n|n.modifiers.iter().any(|m|matches!(&m.modifier,loom_scene::modifier::Modifier::Skin(_)))).map(|n|n.id).collect();assert_eq!(ids.len(),5);
    let meshes:Vec<_>=ids.iter().map(|id|{let mut mesh=scene.eval_mesh(*id).value.mesh.clone();loom_mesh::edit_ops::transform_mesh(&mut mesh,scene.object_matrix(*id)).unwrap();mesh}).collect();
    let before=scene_sha(&scene);
    for(id,mesh)in ids.iter().zip(&meshes){let a=current_source_positions(mesh).unwrap();let b=reserved_source_positions(mesh).unwrap();equal_bits(&a,&b);println!("SOURCE_CAPACITY node={:?} vertices={} current_len={} current_capacity={} reserved_len={} reserved_capacity={} output_bits_exact=PASS",id,mesh.vertex_count(),a.len(),a.capacity(),b.len(),b.capacity());}
    let empty=Mesh::new();assert_eq!(current_source_positions(&empty).unwrap_err(),reserved_source_positions(&empty).unwrap_err());assert_eq!(current_source_positions(&empty).unwrap_err(),"GPU runtime requires nonempty source positions");
    let mut cases=0;
    for invalid in [f32::NAN,f32::INFINITY,f32::NEG_INFINITY] {for index in [0,meshes[0].vertex_count()/2,meshes[0].vertex_count()-1]{let mut mesh=meshes[0].clone();mesh.vertices_mut()[index].position.x=invalid;assert_eq!(current_source_positions(&mesh).unwrap_err(),reserved_source_positions(&mesh).unwrap_err());assert_eq!(current_source_positions(&mesh).unwrap_err(),"nonfinite current world position");cases+=1;}}
    let mut zeros=meshes[0].clone();zeros.vertices_mut()[0].position=glam::Vec3::new(-0.0,0.0,f32::MAX);equal_bits(&current_source_positions(&zeros).unwrap(),&reserved_source_positions(&zeros).unwrap());
    println!("SOURCE_ERROR_SEMANTICS empty=PASS nonfinite_cases={cases} signed_zero_maxfinite_bits=PASS");
    const ROUNDS:usize=64;const REPEATS:usize=16;
    let mut old=0.0;let mut new=0.0;
    for round in 0..ROUNDS {
        let mut times=[0.0;4];
        for (slot,mode) in [0usize,1,1,0].into_iter().enumerate(){let start=Instant::now();for _ in 0..REPEATS{for mesh in &meshes{let values=if mode==0{current_source_positions(black_box(mesh)).unwrap()}else{reserved_source_positions(black_box(mesh)).unwrap()};black_box(&values);drop(values);}}times[slot]=start.elapsed().as_secs_f64()*1e3/REPEATS as f64;}
        let a=(times[0]+times[3])*0.5;let b=(times[1]+times[2])*0.5;old+=a;new+=b;println!("SOURCE_ABBA round={round} current_ms_per5mesh={a:.9} reserved_ms_per5mesh={b:.9} delta_ms={:.9}",a-b);
    }
    let after=scene_sha(&scene);assert_eq!(before,after);println!("SOURCE_SCENE_SHA before={before} after={after} exact=PASS");
    println!("SOURCE_SUMMARY rounds={ROUNDS} repeats={REPEATS} meshes=5 current_ms_per5mesh={:.9} reserved_ms_per5mesh={:.9} delta_ms={:.9} output_bits_exact=PASS error_semantics=PASS",old/ROUNDS as f64,new/ROUNDS as f64,(old-new)/ROUNDS as f64);
}
