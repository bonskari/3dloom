use super::*;
use std::time::{Duration,Instant};
fn compare(a:&Evaluated,b:&Evaluated){
    assert_eq!(a.warnings,b.warnings);assert_eq!(a.value.mesh,b.value.mesh);assert_eq!(a.value.selection,b.value.selection);assert_eq!(a.value.shape_selection,b.value.shape_selection);assert_eq!(a.value.warnings,b.value.warnings);assert_eq!(a.value.patch_steps,b.value.patch_steps);assert_eq!(format!("{:?}",a.value.shape),format!("{:?}",b.value.shape));assert_eq!(format!("{:?}",a.value.patch),format!("{:?}",b.value.patch));
    for(a,b)in a.value.mesh.vertices().iter().zip(b.value.mesh.vertices()){
        assert_eq!(a.position.to_array().map(f32::to_bits),b.position.to_array().map(f32::to_bits));assert_eq!(a.color.map(f32::to_bits),b.color.map(f32::to_bits));assert_eq!(a.illum.map(f32::to_bits),b.illum.map(f32::to_bits));assert_eq!(a.alpha.to_bits(),b.alpha.to_bits());
    }
    for(channel,a)in a.value.mesh.maps(){let b=b.value.mesh.map(*channel).unwrap();for(a,b)in a.verts.iter().zip(&b.verts){assert_eq!(a.to_array().map(f32::to_bits),b.to_array().map(f32::to_bits));}}
}
#[test]
fn actual_skin_batch_stages_317(){
    let value:serde_json::Value=serde_json::from_str(&std::fs::read_to_string(std::env::var("LOOM_PERF_SCENE").unwrap()).unwrap()).unwrap();
    let scene:Scene=serde_json::from_value(value["scene"].clone()).unwrap();let serial=scene.clone();
    let ids:Vec<_>=scene.nodes().filter(|n|n.modifiers.iter().any(|m|matches!(&m.modifier,Modifier::Skin(_)))).map(|n|n.id).collect();assert_eq!(ids.len(),5);
    let cages:Vec<_>=ids.iter().map(|id|scene.node(*id).unwrap().cage_mesh().into_owned()).collect();let modifiers:Vec<_>=ids.iter().map(|id|serde_json::to_vec(&scene.node(*id).unwrap().modifiers).unwrap()).collect();
    for id in &ids{
        let n=scene.node(*id).unwrap();let Modifier::Skin(data)=&n.modifiers[0].modifier else{panic!("actual Skin")};
        eprintln!("FLAGS node={} fast_updates={} always_deform={} back_transform={} gizmos={} patch={} influences_min={} influences_max={} weights={} bones={}",n.name,data.fast_updates,data.always_deform,data.back_transform,data.gizmos.len(),n.object.patch().is_some(),data.weights.iter().map(Vec::len).min().unwrap(),data.weights.iter().map(Vec::len).max().unwrap(),data.weights.len(),data.bones.len());
    }
    let pool=bevy_tasks::TaskPoolBuilder::new().num_threads(4).build();
    let mut preparation=Duration::ZERO;let mut executor=preparation;let mut publication=preparation;let mut base=preparation;let mut apply=preparation;let mut critical_apply=preparation;let mut finish=preparation;let mut inner=[Duration::ZERO;7];let mut inner_critical=[Duration::ZERO;7];let mut fast_jobs=0;
    for frame in (0..=157).map(|f|f as f32).chain([39.25]).chain((0..=157).map(|f|f as f32+0.5)){
        scene.set_time(frame);serial.set_time(frame);STAGE_PROBE.lock().unwrap().clear();BASE_STAGE_PROBE.lock().unwrap().clear();crate::skin::APPLY_STAGE_PROBE.lock().unwrap().clear();
        let begin=Instant::now();let mut prep=Duration::ZERO;let mut execution=Duration::ZERO;
        let actual=scene.evaluated_batch(&ids,|jobs|{
            prep=begin.elapsed();assert_eq!(jobs.len(),5);let t=Instant::now();let results=pool.scope(|scope|{for job in jobs{scope.spawn(async move{job.run()});}});execution=t.elapsed();results
        });
        let total=begin.elapsed();preparation+=prep;executor+=execution;publication+=total.saturating_sub(prep+execution);
        let stages=std::mem::take(&mut *STAGE_PROBE.lock().unwrap());assert_eq!(stages.len(),5);for(a,f)in &stages{apply+=*a;finish+=*f;}critical_apply+=stages.iter().map(|v|v.0).max().unwrap();
        let bases=std::mem::take(&mut *BASE_STAGE_PROBE.lock().unwrap());assert_eq!(bases.len(),5);base+=bases.into_iter().sum::<Duration>();
        let detail=std::mem::take(&mut *crate::skin::APPLY_STAGE_PROBE.lock().unwrap());assert_eq!(detail.len(),5);for(values,fast)in &detail{fast_jobs+=usize::from(*fast);for i in 0..7{inner[i]+=values[i];}}for i in 0..7{inner_critical[i]+=detail.iter().map(|v|v.0[i]).max().unwrap();}
        for(i,(id,actual))in ids.iter().zip(actual).enumerate(){compare(&actual,&serial.evaluated(*id,None));assert_eq!(&cages[i],scene.node(*id).unwrap().cage_mesh().as_ref());}
    }
    for(i,id)in ids.iter().enumerate(){assert_eq!(modifiers[i],serde_json::to_vec(&scene.node(*id).unwrap().modifiers).unwrap());}
    let ms=|t:Duration|t.as_secs_f64()*1000.0/317.0;
    eprintln!("BATCH poses=317 jobs=1585 prepare_ms={:.6} base_display_ms={:.6} executor_wall_ms={:.6} publish_ms={:.6} apply_sum_ms={:.6} apply_max_ms={:.6} finish_sum_ms={:.6} fast_jobs={fast_jobs}",ms(preparation),ms(base),ms(executor),ms(publication),ms(apply),ms(critical_apply),ms(finish));
    for(i,name)in["weighted_point_copy","matrix_setup","out_copy","all_weight_loop","gizmos","write_positions","point_weights_replay"].into_iter().enumerate(){eprintln!("APPLY stage={name} sum_ms={:.6} max_ms={:.6}",ms(inner[i]),ms(inner_critical[i]));}
}
