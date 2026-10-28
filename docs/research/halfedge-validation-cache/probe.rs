#[cfg(test)]
mod tests {
use std::{sync::Arc,time::{Duration,Instant}};
use original_scene as old;
use cow_scene as cow;
fn meshes(a:&original_mesh::Mesh,b:&cow_mesh::Mesh){
 assert_eq!(a.vertex_count(),b.vertex_count());assert_eq!(a.face_count(),b.face_count());assert_eq!(a.half_edge_count(),b.half_edge_count());
 for(a,b)in a.vertices().iter().zip(b.vertices()){
  assert_eq!(a.position.to_array().map(f32::to_bits),b.position.to_array().map(f32::to_bits));assert_eq!(a.he.map(|h|h.0),b.he.map(|h|h.0));assert_eq!(a.color.map(f32::to_bits),b.color.map(f32::to_bits));assert_eq!(a.illum.map(f32::to_bits),b.illum.map(f32::to_bits));assert_eq!(a.alpha.to_bits(),b.alpha.to_bits());assert_eq!(a.hidden,b.hidden);
 }
 for(a,b)in a.half_edges().iter().zip(b.half_edges()){
  assert_eq!((a.origin.0,a.twin.0,a.next.0,a.prev.0,a.face.map(|f|f.0)),(b.origin.0,b.twin.0,b.next.0,b.prev.0,b.face.map(|f|f.0)));
 }
 for(a,b)in a.faces().iter().zip(b.faces()){assert_eq!((a.he.0,a.smoothing_groups,a.material_id,a.hidden),(b.he.0,b.smoothing_groups,b.material_id,b.hidden));}
 assert_eq!(a.maps().len(),b.maps().len());for(channel,a)in a.maps(){let b=b.map(*channel).unwrap();assert_eq!(a.corners,b.corners);assert_eq!(a.verts.len(),b.verts.len());for(a,b)in a.verts.iter().zip(&b.verts){assert_eq!(a.to_array().map(f32::to_bits),b.to_array().map(f32::to_bits));}}
 assert_eq!(a.has_invisible_edges(),b.has_invisible_edges());for i in 0..a.half_edge_count(){assert_eq!(a.edge_visible(original_mesh::HalfEdgeId(i as u32)),b.edge_visible(cow_mesh::HalfEdgeId(i as u32)));}
}
fn evaluated(a:&old::modifier::Evaluated,b:&cow::modifier::Evaluated){
 meshes(&a.value.mesh,&b.value.mesh);assert_eq!(a.warnings,b.warnings);assert_eq!(a.value.warnings,b.value.warnings);assert_eq!(a.value.patch_steps,b.value.patch_steps);assert_eq!(a.value.shape_selection,b.value.shape_selection);
 assert_eq!(format!("{:?}",a.value.selection),format!("{:?}",b.value.selection));assert_eq!(format!("{:?}",a.value.shape),format!("{:?}",b.value.shape));assert_eq!(format!("{:?}",a.value.patch),format!("{:?}",b.value.patch));
}
#[test]
fn old_and_cow_mesh_json_are_bidirectionally_compatible(){
 let mut original=original_mesh::box_primitive(glam::Vec3::ONE);original_mesh::uvmap::auto_map(&mut original,original_mesh::uvmap::AutoMap::Spherical,1);original.set_edge_visible(original_mesh::HalfEdgeId(0),false);
 let old_json=serde_json::to_value(&original).unwrap();assert!(old_json["half_edges"].is_array());
 let shared:cow_mesh::Mesh=serde_json::from_value(old_json.clone()).unwrap();let new_json=serde_json::to_value(&shared).unwrap();assert_eq!(old_json,new_json);
 let restored:original_mesh::Mesh=serde_json::from_value(new_json).unwrap();assert_eq!(original,restored);meshes(&original,&shared);
}
macro_rules! runner {
 ($name:ident,$scene:ident,$mesh:ident)=>{
 fn $name(scene:&$scene::Scene,ids:&[$scene::NodeId],pool:&bevy_tasks::TaskPool)->(Vec<Arc<$scene::modifier::Evaluated>>,Vec<$mesh::Mesh>,Duration,Duration,Duration){
  let all=Instant::now();let batch=Instant::now();let result=scene.evaluated_batch(ids,|jobs|{assert_eq!(jobs.len(),5);pool.scope(|scope|{for job in jobs{scope.spawn(async move{job.run()});}})});let batch_time=batch.elapsed();
  let matrices:Vec<_>=ids.iter().map(|id|scene.object_matrix(*id)).collect();let display=Instant::now();let mut world=pool.scope(|scope|{for(i,(out,matrix))in result.iter().zip(&matrices).enumerate(){scope.spawn(async move{let mut mesh=out.value.mesh.clone();$mesh::edit_ops::transform_mesh(&mut mesh,*matrix).unwrap();(i,mesh)});}});world.sort_unstable_by_key(|(i,_)|*i);let world=world.into_iter().map(|(_,m)|m).collect();let display_time=display.elapsed();let total=all.elapsed();(result,world,batch_time,display_time,total)
 }
 }
}
runner!(run_old,old,original_mesh);runner!(run_cow,cow,cow_mesh);
#[test]
#[ignore="actual paired317 requires LOOM_PERF_SCENE"]
fn actual_halfedge_validation_317(){
 let path=std::env::var("LOOM_PERF_SCENE").unwrap();let value:serde_json::Value=serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();let original:old::Scene=serde_json::from_value(value["scene"].clone()).unwrap();let shared:cow::Scene=serde_json::from_value(value["scene"].clone()).unwrap();let serial=original.clone();drop(value);
 let ids:Vec<_>=original.nodes().filter(|n|n.modifiers.iter().any(|m|matches!(&m.modifier,old::Modifier::Skin(_)))).map(|n|n.id).collect();assert_eq!(ids.len(),5);let cids:Vec<_>=ids.iter().map(|id|cow::NodeId(id.0)).collect();
 let cages:Vec<_>=ids.iter().map(|id|original.node(*id).unwrap().cage_mesh().into_owned()).collect();let modifiers:Vec<_>=ids.iter().map(|id|serde_json::to_vec(&original.node(*id).unwrap().modifiers).unwrap()).collect();let source_pointers:Vec<_>=cids.iter().map(|id|shared.node(*id).unwrap().object.mesh().half_edges().as_ptr()).collect();
 let pool=bevy_tasks::TaskPoolBuilder::new().num_threads(4).build();let mut old_times=[Duration::ZERO;3];let mut new_times=old_times;
 for(index,frame)in (0..=157).map(|f|f as f32).chain([39.25]).chain((0..=157).map(|f|f as f32+0.5)).enumerate(){
  original.set_time(frame);shared.set_time(frame);serial.set_time(frame);
  let(a,b)=if index%2==0{let a=run_old(&original,&ids,&pool);let b=run_cow(&shared,&cids,&pool);(a,b)}else{let b=run_cow(&shared,&cids,&pool);let a=run_old(&original,&ids,&pool);(a,b)};
  for(i,(x,y))in [a.2,a.3,a.4].into_iter().zip([b.2,b.3,b.4]).enumerate(){old_times[i]+=x;new_times[i]+=y;}
  for i in 0..5{
   evaluated(&a.0[i],&b.0[i]);evaluated(&serial.evaluated(ids[i],None),&b.0[i]);meshes(&a.1[i],&b.1[i]);meshes(&cages[i],shared.node(cids[i]).unwrap().cage_mesh().as_ref());assert_eq!(&cages[i],original.node(ids[i]).unwrap().cage_mesh().as_ref());
   assert_eq!(b.0[i].value.mesh.half_edges().as_ptr(),source_pointers[i]);assert_eq!(b.1[i].half_edges().as_ptr(),source_pointers[i]);
   if index==0{let j=serde_json::to_value(&a.0[i].value.mesh).unwrap();assert_eq!(j,serde_json::to_value(&b.0[i].value.mesh).unwrap());let restored:original_mesh::Mesh=serde_json::from_value(serde_json::to_value(&b.1[i]).unwrap()).unwrap();assert_eq!(restored,a.1[i]);}
  }
 }
 for i in 0..5{assert_eq!(modifiers[i],serde_json::to_vec(&original.node(ids[i]).unwrap().modifiers).unwrap());assert_eq!(modifiers[i],serde_json::to_vec(&shared.node(cids[i]).unwrap().modifiers).unwrap());}
 for(i,name)in["skin_batch","display_clone_world","combined"].into_iter().enumerate(){let ms=|d:Duration|d.as_secs_f64()*1000.0/317.0;eprintln!("HALFEDGE_VALIDATION poses=317 meshes=1585 stage={name} original_ms={:.6} cow_ms={:.6} saved_ms={:.6}",ms(old_times[i]),ms(new_times[i]),ms(old_times[i])-ms(new_times[i]));}
}
#[test]
fn validation_error_and_panic_contract_matches_current_baseline(){
 let original=original_mesh::box_primitive(glam::Vec3::ONE);let json=serde_json::to_value(original).unwrap();
 for field in ["twin","next","prev","face"]{
  let mut changed=json.clone();changed["half_edges"][0][field]=serde_json::json!(if field=="face"{999}else{0});
  let a:original_mesh::Mesh=serde_json::from_value(changed.clone()).unwrap();let b:cow_mesh::Mesh=serde_json::from_value(changed).unwrap();
  assert_eq!(a.check_invariants(),b.check_invariants(),"field={field}");
 }
 let mut changed=json;changed["half_edges"][0]["next"]=serde_json::json!(u32::MAX);
 let a:original_mesh::Mesh=serde_json::from_value(changed.clone()).unwrap();let b:cow_mesh::Mesh=serde_json::from_value(changed).unwrap();
 assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(||a.check_invariants())).is_err());
 assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(||b.check_invariants())).is_err());
}

}
