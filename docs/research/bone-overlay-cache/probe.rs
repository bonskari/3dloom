pub struct LoomApp { inner: loom_ui::LoomApp, bone_axes: Vec<(loom_scene::NodeId,glam::Vec3,glam::Vec3)> }
impl std::ops::Deref for LoomApp { type Target=loom_ui::LoomApp;fn deref(&self)->&Self::Target{&self.inner} }
impl std::ops::DerefMut for LoomApp { fn deref_mut(&mut self)->&mut Self::Target{&mut self.inner} }
#[path="old.rs"] mod old;
#[path="new.rs"] mod new;
fn scene_sha(app:&LoomApp)->String { format!("{:x}",Sha256::digest(serde_json::to_vec(&app.state.scene).unwrap())) }
fn bits(lines:&(old::DepthLines,old::TopLines))->(Vec<([u32;3],[u32;3],[u32;3])>,Vec<([u32;3],[u32;3],[u8;3])>) {
 (lines.0.iter().map(|(a,b,c)|(a.to_array().map(f32::to_bits),b.to_array().map(f32::to_bits),c.map(f32::to_bits))).collect(),
 lines.1.iter().map(|(a,b,c)|(a.to_array().map(f32::to_bits),b.to_array().map(f32::to_bits),*c)).collect())
}
use std::{time::Instant,hint::black_box};
use sha2::{Digest,Sha256};
fn main(){
 let input="/tmp/loom-conditioning-capture/input.loom";
 let bytes=std::fs::read(input).unwrap();let input_sha=format!("{:x}",Sha256::digest(&bytes));drop(bytes);
 assert_eq!(input_sha,"84fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935");
 let doc=loom_commands::file::read(std::path::Path::new(input)).unwrap();
 let mut app=LoomApp{inner:loom_ui::LoomApp::headless(),bone_axes:Vec::new()};app.state.scene=doc.scene;doc.settings.apply_to(&mut app.state);
 app.state.current_frame=39;app.state.scene.set_time(39.);
 let diag:serde_json::Value=serde_json::from_str(&std::fs::read_to_string("target/helper-overlay-probe/diagnostics.json").unwrap()).unwrap();
 let mut views=Vec::new();
 for (i,r) in diag["viewports"]["rects"].as_array().unwrap().iter().enumerate(){
  let v:Vec<f32>=r.as_array().unwrap().iter().map(|v|v.as_f64().unwrap()as f32).collect();
  let rect=eframe::egui::Rect::from_min_max(eframe::egui::pos2(v[0],v[1]),eframe::egui::pos2(v[2],v[3]));
  let camera=app.camera_for(i);let vp=camera.projection_matrix(rect.width()/rect.height())*camera.view_matrix();
  views.push((vp,rect,i==3));
 }
 assert_eq!(views.len(),4);assert_eq!(format!("{:?}",app.state.viewports[0].kind),"Top");
 let active=&app.state.viewports[3].camera;let observed=&diag["viewports"]["active_view"]["camera"];
 assert_eq!(active.distance.to_bits(),(observed["distance"].as_f64().unwrap()as f32).to_bits());
 assert_eq!(active.yaw.to_bits(),(observed["yaw"].as_f64().unwrap()as f32).to_bits());
 assert_eq!(active.pitch.to_bits(),(observed["pitch"].as_f64().unwrap()as f32).to_bits());
 app.state.display.hide_bones=false;
 let before=scene_sha(&app);
 let setup=Instant::now();let setup_reads=loom_scene::anim::animated_reads();
 let mut axes=Vec::new();
 for node in app.state.scene.nodes(){
  if !app.state.is_visible(node) || (node.parent.is_some()&&node.display.display_links&&node.display.link_replaces_object){continue;}
  if let Some(tip)=node.object.bone_tip_local(){let world=app.state.scene.world_matrix(node.id);let a=world.transform_point3(glam::Vec3::ZERO);let b=world.transform_point3(tip);if a.is_finite()&&b.is_finite(){axes.push((node.id,a,b));}}
 }
 let setup_ms=setup.elapsed().as_secs_f64()*1000.;let setup_reads=loom_scene::anim::animated_reads().wrapping_sub(setup_reads);app.bone_axes=axes;
 let mut parity=Vec::new();
 for (index,(vp,rect,realistic)) in views.iter().enumerate(){let a=old::lines(&app,*vp,*rect,*realistic);let b=new::lines(&app,*vp,*rect,*realistic);let ab=bits(&a);let bb=bits(&b);parity.push(serde_json::json!({"view":index,"ordered_depth_exact":ab.0==bb.0,"ordered_top_exact":ab.1==bb.1,"old_counts":[a.0.len(),a.1.len()],"new_counts":[b.0.len(),b.1.len()]}));assert_eq!(ab,bb,"ordered line parity viewport {index}");}
 assert_eq!(before,scene_sha(&app));
 let call=|app:&LoomApp,use_new:bool|{let mut counts=[0usize;2];for(vp,rect,realistic)in &views{let(d,t)=if use_new {new::lines(app,*vp,*rect,*realistic)} else {old::lines(app,*vp,*rect,*realistic)};counts[0]+=d.len();counts[1]+=t.len();black_box((d,t));}counts};
 for use_new in [false,true]{for _ in 0..30{black_box(call(&app,use_new));}}
 let mut rows=Vec::new();
 for cycle in 0..4{for use_new in [false,true,true,false]{
  let reads=loom_scene::anim::animated_reads();let start=Instant::now();let mut counts=[0;2];
  for _ in 0..200{counts=call(&app,use_new);}
  rows.push(serde_json::json!({"cycle":cycle,"new_cached":use_new,"iterations":200,"elapsed_ms":start.elapsed().as_secs_f64()*1000.,"animated_reads_delta":loom_scene::anim::animated_reads().wrapping_sub(reads),"four_view_line_counts":counts}));
 }}
 println!("{}",serde_json::to_string_pretty(&serde_json::json!({"input":input,"input_sha256":input_sha,"scene_nodes":app.state.scene.len(),"frame":39,"scene_mesh_rebuilds":0,"matrix_source":"frozen SceneSettings + LoomApp.camera_for; active orbit bit-checked against live diagnostics","rect_source":"live diagnostics","source":serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string("target/helper-overlay-cache-probe/source.json").unwrap()).unwrap(),"views":views.iter().map(|(vp,r,real)|serde_json::json!({"view_projection":vp.to_cols_array(),"rect":[r.min.x,r.min.y,r.max.x,r.max.y],"realistic":real})).collect::<Vec<_>>(),"rows":rows,"source_scene_sha_before":before,"source_scene_sha_after":scene_sha(&app),"parity":parity,"cache_setup_ms":setup_ms,"cache_setup_reads":setup_reads,"cached_axes":app.bone_axes.len(),"setup_measures_full_rebuild":false})).unwrap());
}
