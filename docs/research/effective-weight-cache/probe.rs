use super::*;
use crate::skin::effective_weights_probe as cache;
use std::sync::atomic::Ordering;
use std::time::{Duration,Instant};
fn compare(a:&Evaluated,b:&Evaluated){
 assert_eq!(a.warnings,b.warnings);assert_eq!(a.value.mesh,b.value.mesh);assert_eq!(a.value.selection,b.value.selection);assert_eq!(a.value.shape_selection,b.value.shape_selection);assert_eq!(a.value.warnings,b.value.warnings);assert_eq!(a.value.patch_steps,b.value.patch_steps);assert_eq!(format!("{:?}",a.value.shape),format!("{:?}",b.value.shape));assert_eq!(format!("{:?}",a.value.patch),format!("{:?}",b.value.patch));
 for(a,b)in a.value.mesh.vertices().iter().zip(b.value.mesh.vertices()){assert_eq!(a.position.to_array().map(f32::to_bits),b.position.to_array().map(f32::to_bits));assert_eq!(a.color.map(f32::to_bits),b.color.map(f32::to_bits));assert_eq!(a.illum.map(f32::to_bits),b.illum.map(f32::to_bits));assert_eq!(a.alpha.to_bits(),b.alpha.to_bits());}
 for(channel,a)in a.value.mesh.maps(){let b=b.value.mesh.map(*channel).unwrap();for(a,b)in a.verts.iter().zip(&b.verts){assert_eq!(a.to_array().map(f32::to_bits),b.to_array().map(f32::to_bits));}}
}
#[test]
fn exact_effective_cache_edits(){
 let mesh=loom_mesh::box_primitive(glam::Vec3::ONE);let count=mesh.vertex_count();
 let bones=[(NodeId(1),Mat4::IDENTITY,crate::skin::Envelope::default()),(NodeId(2),Mat4::IDENTITY,crate::skin::Envelope::default())];
 let original=SkinData::from_file(&bones,Mat4::IDENTITY,vec![vec![(0,0.25),(1,0.75)];count]);
 let lookup=|id:NodeId|Some(Mat4::from_translation(if id.0==1{glam::Vec3::X}else{glam::Vec3::Y}));let ctx=crate::EvalCtx{world_lookup:Some(&lookup),..Default::default()};
 for change in 0..11 {
  let mut c=cache::ExplicitWeightCache::default();let first=c.refresh(&original,count).unwrap();assert!(Arc::ptr_eq(&first,&c.refresh(&original,count).unwrap()));let mut data=original.clone();
  match change{0=>data.weights[0][0].1=0.5,1=>data.weights[0].reverse(),2=>data.rigid[0]=true,3=>data.rigid_all=true,4=>data.bone_affect_limit=1,5=>{data.bones.pop();},6=>data.explicit[0]=false,7=>{data.weights.pop();},8=>data.weights[0][0].1=-0.0,9=>data.weights[0][0].1=f32::NAN,_=>data.weights[0][0].1=f32::INFINITY}
  let effective=c.refresh(&data,count);if let Some(e)=&effective{assert!(!Arc::ptr_eq(&first,e));}else{assert!(matches!(change,6|7|9|10));}
  let input=StackValue{mesh:mesh.clone(),selection:None,shape:None,shape_selection:None,warnings:Vec::new(),patch:None,patch_steps:None};
  let expected=crate::skin::apply_owned(&data,input.clone(),&ctx);let actual=crate::skin::apply_owned_cached_weights(&data,input,&ctx,effective.as_deref().map(Vec::as_slice));compare(&Evaluated{value:actual,warnings:vec![]},&Evaluated{value:expected,warnings:vec![]});
 }
 let mut data=original.clone();data.weights[0][0].1=0.0;let mut c=cache::ExplicitWeightCache::default();let a=c.refresh(&data,count).unwrap();data.weights[0][0].1=-0.0;let b=c.refresh(&data,count).unwrap();assert!(!Arc::ptr_eq(&a,&b));
}
fn batch(scene:&Scene,ids:&[NodeId],pool:&bevy_tasks::TaskPool,enabled:bool)->(Vec<Arc<Evaluated>>,Duration){
 cache::ENABLED.store(enabled,Ordering::Relaxed);let begin=Instant::now();let result=scene.evaluated_batch(ids,|jobs|{assert_eq!(jobs.len(),5);pool.scope(|scope|{for job in jobs{scope.spawn(async move{job.run()});}})});(result,begin.elapsed())
}
#[test]
fn actual_effective_weight_cache_317(){
 let value:serde_json::Value=serde_json::from_str(&std::fs::read_to_string(std::env::var("LOOM_PERF_SCENE").unwrap()).unwrap()).unwrap();let scene:Scene=serde_json::from_value(value["scene"].clone()).unwrap();let uncached=scene.clone();let serial=scene.clone();
 let ids:Vec<_>=scene.nodes().filter(|n|n.modifiers.iter().any(|m|matches!(&m.modifier,Modifier::Skin(_)))).map(|n|n.id).collect();assert_eq!(ids.len(),5);
 let cages:Vec<_>=ids.iter().map(|id|scene.node(*id).unwrap().cage_mesh().into_owned()).collect();let modifiers:Vec<_>=ids.iter().map(|id|serde_json::to_vec(&scene.node(*id).unwrap().modifiers).unwrap()).collect();
 let pool=bevy_tasks::TaskPoolBuilder::new().num_threads(4).build();let mut old=Duration::ZERO;let mut new=old;let mut cold=old;
 cache::HITS.store(0,Ordering::Relaxed);cache::MISSES.store(0,Ordering::Relaxed);*cache::GUARD_NS.lock().unwrap()=Duration::ZERO;
 for(index,frame)in (0..=157).map(|f|f as f32).chain([39.25]).chain((0..=157).map(|f|f as f32+0.5)).enumerate(){
  scene.set_time(frame);uncached.set_time(frame);serial.set_time(frame);
  let(a,b,ta,tb)=if index%2==0{let(b,tb)=batch(&uncached,&ids,&pool,false);let(a,ta)=batch(&scene,&ids,&pool,true);(a,b,ta,tb)}else{let(a,ta)=batch(&scene,&ids,&pool,true);let(b,tb)=batch(&uncached,&ids,&pool,false);(a,b,ta,tb)};
  new+=ta;old+=tb;if index==0{cold=ta;}
  for(i,id)in ids.iter().enumerate(){compare(&a[i],&b[i]);compare(&a[i],&serial.evaluated(*id,None));assert_eq!(&cages[i],scene.node(*id).unwrap().cage_mesh().as_ref());}
 }
 for(i,id)in ids.iter().enumerate(){assert_eq!(modifiers[i],serde_json::to_vec(&scene.node(*id).unwrap().modifiers).unwrap());}
 assert_eq!(serde_json::to_value(&scene).unwrap(),serde_json::to_value(&uncached).unwrap());
 let ms=|t:Duration|t.as_secs_f64()*1000.0/317.0;let guard=*cache::GUARD_NS.lock().unwrap();eprintln!("EFFECTIVE_CACHE poses=317 meshes=1585 old_batch_ms={:.6} new_batch_ms={:.6} saved_ms={:.6} guard_ms={:.6} cold_ms={:.6} hits={} misses={}",ms(old),ms(new),ms(old)-ms(new),ms(guard),cold.as_secs_f64()*1000.0,cache::HITS.load(Ordering::Relaxed),cache::MISSES.load(Ordering::Relaxed));
}
