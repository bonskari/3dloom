use super::*;
use crate::{Evaluated,Modifier};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration,Instant};
fn compare(a:&Evaluated,b:&Evaluated){
 assert_eq!(a.warnings,b.warnings);assert_eq!(a.value.mesh,b.value.mesh);assert_eq!(a.value.selection,b.value.selection);assert_eq!(a.value.shape_selection,b.value.shape_selection);assert_eq!(a.value.warnings,b.value.warnings);assert_eq!(a.value.patch_steps,b.value.patch_steps);assert_eq!(format!("{:?}",a.value.shape),format!("{:?}",b.value.shape));assert_eq!(format!("{:?}",a.value.patch),format!("{:?}",b.value.patch));
 for(a,b)in a.value.mesh.vertices().iter().zip(b.value.mesh.vertices()){assert_eq!(a.position.to_array().map(f32::to_bits),b.position.to_array().map(f32::to_bits));assert_eq!(a.color.map(f32::to_bits),b.color.map(f32::to_bits));assert_eq!(a.illum.map(f32::to_bits),b.illum.map(f32::to_bits));assert_eq!(a.alpha.to_bits(),b.alpha.to_bits());}
 for(channel,a)in a.value.mesh.maps(){let b=b.value.mesh.map(*channel).unwrap();for(a,b)in a.verts.iter().zip(&b.verts){assert_eq!(a.to_array().map(f32::to_bits),b.to_array().map(f32::to_bits));}}
}
fn batch(scene:&Scene,ids:&[NodeId],pool:&bevy_tasks::TaskPool,enabled:bool)->(Vec<Arc<Evaluated>>,Duration){
 crate::skin::SINGLE_PROBE.store(enabled,Ordering::Relaxed);let begin=Instant::now();let result=scene.evaluated_batch(ids,|jobs|{assert_eq!(jobs.len(),5);pool.scope(|scope|{for job in jobs{scope.spawn(async move{job.run()});}})});(result,begin.elapsed())
}
#[test]
fn actual_single_weight_317(){
 let value:serde_json::Value=serde_json::from_str(&std::fs::read_to_string(std::env::var("LOOM_PERF_SCENE").unwrap()).unwrap()).unwrap();let scene:Scene=serde_json::from_value(value["scene"].clone()).unwrap();let uncached=scene.clone();let serial=scene.clone();let scene2=scene.clone();let uncached2=scene.clone();
 let ids:Vec<_>=scene.nodes().filter(|n|n.modifiers.iter().any(|m|matches!(&m.modifier,Modifier::Skin(_)))).map(|n|n.id).collect();assert_eq!(ids.len(),5);
 let cages:Vec<_>=ids.iter().map(|id|scene.node(*id).unwrap().cage_mesh().into_owned()).collect();let modifiers:Vec<_>=ids.iter().map(|id|serde_json::to_vec(&scene.node(*id).unwrap().modifiers).unwrap()).collect();
 let pool=bevy_tasks::TaskPoolBuilder::new().num_threads(4).build();let mut old=Duration::ZERO;let mut new=old;let mut cold=old;

 for(index,frame)in (0..=157).map(|f|f as f32).chain([39.25]).chain((0..=157).map(|f|f as f32+0.5)).enumerate(){
  scene.set_time(frame);uncached.set_time(frame);serial.set_time(frame);scene2.set_time(frame);uncached2.set_time(frame);
  let(a,b,ta,tb)=if index%2==0{let(b,tb)=batch(&uncached,&ids,&pool,false);let(a,ta)=batch(&scene,&ids,&pool,true);(a,b,ta,tb)}else{let(a,ta)=batch(&scene,&ids,&pool,true);let(b,tb)=batch(&uncached,&ids,&pool,false);(a,b,ta,tb)};
  let(c,d,tc,td)=if index%2==0{let(c,tc)=batch(&scene2,&ids,&pool,true);let(d,td)=batch(&uncached2,&ids,&pool,false);(c,d,tc,td)}else{let(d,td)=batch(&uncached2,&ids,&pool,false);let(c,tc)=batch(&scene2,&ids,&pool,true);(c,d,tc,td)};
  new+=ta+tc;old+=tb+td;if index==0{cold=ta;}
  crate::skin::SINGLE_PROBE.store(false,Ordering::Relaxed);
  for i in 0..ids.len(){compare(&a[i],&c[i]);compare(&b[i],&d[i]);}
  for(i,id)in ids.iter().enumerate(){compare(&a[i],&b[i]);compare(&a[i],&serial.evaluated(*id,None));assert_eq!(&cages[i],scene.node(*id).unwrap().cage_mesh().as_ref());}
 }
 for(i,id)in ids.iter().enumerate(){assert_eq!(modifiers[i],serde_json::to_vec(&scene.node(*id).unwrap().modifiers).unwrap());}
 assert_eq!(serde_json::to_value(&scene).unwrap(),serde_json::to_value(&uncached).unwrap());
 let ms=|t:Duration|t.as_secs_f64()*1000.0/634.0;eprintln!("SINGLE poses=317 meshes=1585 old_batch_ms={:.6} new_batch_ms={:.6} saved_ms={:.6}",ms(old),ms(new),ms(old)-ms(new));
}
