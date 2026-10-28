use std::{collections::BTreeSet,hint::black_box,time::Instant};
use loom_mesh::Mesh;
use sha2::{Digest,Sha256};
fn bitset_ids(rows:impl Iterator<Item=(u16,bool)>)->Vec<Option<u16>> {
 let mut bits=[0u64;1024];
 for(id,hidden)in rows{if !hidden{bits[id as usize/64]|=1u64<<(id%64);}}
 let mut ids=Vec::new();
 for (word,mut set)in bits.into_iter().enumerate(){while set!=0{let bit=set.trailing_zeros();ids.push(Some((word*64+bit as usize)as u16));set&=set-1;}}
 ids
}
#[inline(never)]fn tree(mesh:&Mesh)->Vec<Option<u16>>{mesh.faces().iter().filter(|f|!f.hidden).map(|f|f.material_id).collect::<BTreeSet<_>>().into_iter().map(Some).collect()}
#[inline(never)]fn bitset(mesh:&Mesh)->Vec<Option<u16>>{bitset_ids(mesh.faces().iter().map(|f|(f.material_id,f.hidden)))}
fn verify(){
 let check=|rows:Vec<(u16,bool)>|{let expected:Vec<_>=rows.iter().filter(|(_,h)|!*h).map(|(id,_)|*id).collect::<BTreeSet<_>>().into_iter().map(Some).collect();assert_eq!(bitset_ids(rows.into_iter()),expected);};
 check(vec![]);check(vec![(0,false),(63,false),(64,false),(65535,false),(64,true),(1,true),(63,false)]);
 for variant in 0..4{check((0..=u16::MAX).flat_map(|id|[(id,match variant{0=>false,1=>true,2=>id%3==0,_=>id%2==0}),(id,true)]).collect());}
}
fn main(){
 verify();let input="/tmp/loom-conditioning-capture/input.loom";let bytes=std::fs::read(input).unwrap();let input_sha=format!("{:x}",Sha256::digest(&bytes));drop(bytes);assert_eq!(input_sha,"84fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935");
 let doc=loom_commands::file::read(std::path::Path::new(input)).unwrap();let mut state=loom_commands::AppState::new();state.scene=doc.scene;doc.settings.apply_to(&mut state);state.current_frame=39;state.scene.set_time(39.);
 let mut objects=Vec::new();let mut metadata=Vec::new();
 for node in state.scene.nodes().filter(|n|n.modifiers.iter().any(|m|matches!(m.modifier,loom_scene::Modifier::Skin(_)))){
  assert!(state.is_visible(node));
  let display=state.scene.display_of(node);let gray=node.frozen&&display.show_frozen_in_gray;
  let wanted=state.scene.node_surfaces(node.id).filter(|_|!gray&&state.display.shaded_color==loom_commands::display::ColorSource::Material);
  let bevy=state.scene.node_surfaces_for(node.id,loom_scene::material::ShellView::Viewport);
  let per_id=matches!(bevy,Some(loom_scene::material::Surfaces::PerId(_)));
  let eval=state.scene.evaluated(node.id,None);let mesh=eval.value.mesh.clone();
  assert_eq!(tree(&mesh),bitset(&mesh));
  metadata.push(serde_json::json!({"id":node.id.0,"name":node.name,"faces":mesh.faces().len(),"hidden_faces":mesh.faces().iter().filter(|f|f.hidden).count(),"ids":tree(&mesh),"wanted_per_id":matches!(wanted,Some(loom_scene::material::Surfaces::PerId(_))),"bevy_viewport_per_id":per_id,"frozen_gray":gray,"shaded_color_material":state.display.shaded_color==loom_commands::display::ColorSource::Material}));
  objects.push((mesh,per_id));
 }
 assert_eq!(objects.len(),5);
 let call=|new:bool|{for(mesh,_)in&objects{black_box(if new{bitset(black_box(mesh))}else{tree(black_box(mesh))});}};
 for new in[false,true]{for _ in 0..30{call(new)}}
 let mut rows=Vec::new();
 for cycle in 0..4{for new in[false,true,true,false]{
  let mut per_object=Vec::new();let batch_start=Instant::now();
  for(index,(mesh,_))in objects.iter().enumerate(){let start=Instant::now();for _ in 0..300{black_box(if new{bitset(black_box(mesh))}else{tree(black_box(mesh))});}per_object.push(serde_json::json!({"object":index,"elapsed_ms":start.elapsed().as_secs_f64()*1000.,"iterations":300}));}
  rows.push(serde_json::json!({"cycle":cycle,"new_bitset":new,"batch_elapsed_ms":batch_start.elapsed().as_secs_f64()*1000.,"per_object":per_object}));
 }}
 let actual_call=|new:bool|{for(mesh,per_id)in&objects{black_box(if !black_box(*per_id){vec![None]}else if new{bitset(black_box(mesh))}else{tree(black_box(mesh))});}};
 for new in [false,true]{for _ in 0..50{actual_call(new);}}
 let mut actual_rows=Vec::new();for cycle in 0..4{for new in [false,true,true,false]{let start=Instant::now();for _ in 0..600{actual_call(new);}actual_rows.push(serde_json::json!({"cycle":cycle,"new_bitset":new,"iterations":600,"elapsed_ms":start.elapsed().as_secs_f64()*1000.}));}}
 println!("{}",serde_json::to_string_pretty(&serde_json::json!({"input_sha256":input_sha,"frame":39,"nodes":state.scene.len(),"boundary_and_full_u16_hidden_cases_passed":true,"objects":metadata,"rows":rows,"actual_five_branch_rows":actual_rows,"scope":"ID collection only; all five separately, actual Bevy Viewport Surfaces::PerId classification plus exact UI wanted classification"})).unwrap());
}
