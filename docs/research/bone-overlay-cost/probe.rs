pub use loom_ui::LoomApp;
#[path="helper_overlay_snapshot.rs"] mod helper_overlay;
use std::{time::Instant,hint::black_box};
use sha2::{Digest,Sha256};
fn main(){
 let input="/tmp/loom-conditioning-capture/input.loom";
 let bytes=std::fs::read(input).unwrap();let input_sha=format!("{:x}",Sha256::digest(&bytes));drop(bytes);
 assert_eq!(input_sha,"84fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935");
 let doc=loom_commands::file::read(std::path::Path::new(input)).unwrap();
 let mut app=LoomApp::headless();app.state.scene=doc.scene;doc.settings.apply_to(&mut app.state);
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
 let call=|app:&LoomApp|{let mut counts=[0usize;2];for(vp,rect,realistic)in &views{let(d,t)=helper_overlay::lines(app,*vp,*rect,*realistic);counts[0]+=d.len();counts[1]+=t.len();black_box((d,t));}counts};
 for hidden in [false,true]{app.state.display.hide_bones=hidden;for _ in 0..30{black_box(call(&app));}}
 let mut rows=Vec::new();
 for cycle in 0..4{for hidden in [false,true,true,false]{app.state.display.hide_bones=hidden;
  let reads=loom_scene::anim::animated_reads();let start=Instant::now();let mut counts=[0;2];
  for _ in 0..200{counts=call(&app);}
  rows.push(serde_json::json!({"cycle":cycle,"hide_bones":hidden,"iterations":200,"elapsed_ms":start.elapsed().as_secs_f64()*1000.,"animated_reads_delta":loom_scene::anim::animated_reads().wrapping_sub(reads),"four_view_line_counts":counts}));
 }}
 println!("{}",serde_json::to_string_pretty(&serde_json::json!({"input":input,"input_sha256":input_sha,"scene_nodes":app.state.scene.len(),"frame":39,"scene_mesh_rebuilds":0,"matrix_source":"frozen SceneSettings + LoomApp.camera_for; active orbit bit-checked against live diagnostics","rect_source":"live diagnostics","source":serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string("target/helper-overlay-probe/source.json").unwrap()).unwrap(),"views":views.iter().map(|(vp,r,real)|serde_json::json!({"view_projection":vp.to_cols_array(),"rect":[r.min.x,r.min.y,r.max.x,r.max.y],"realistic":real})).collect::<Vec<_>>(),"rows":rows})).unwrap());
}
