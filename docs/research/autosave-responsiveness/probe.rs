use std::{path::PathBuf,time::Instant,io::{Read,Write}};
use loom_commands::{AppState,file::{self,Document,SceneSettings},autosave};
use sha2::{Sha256,Digest};
fn assert_send<T:Send>(){}
#[cfg(probe_require_sync)]fn assert_sync<T:Sync>(){}
struct DigestWriter(Sha256);
impl Write for DigestWriter{fn write(&mut self,data:&[u8])->std::io::Result<usize>{self.0.update(data);Ok(data.len())}fn flush(&mut self)->std::io::Result<()>{Ok(())}}
fn digest<T:serde::Serialize>(value:&T)->String{let mut writer=DigestWriter(Sha256::new());serde_json::to_writer(&mut writer,value).unwrap();format!("{:x}",writer.0.finalize())}
fn props(scene:&loom_scene::Scene)->serde_json::Value{
 let rows:Vec<_>=["Ray_OfficeDesk","Ray_OfficeChair"].into_iter().map(|name|{
  let node=scene.nodes().find(|n|n.name==name).expect("actual frozen desk/chair");serde_json::json!({"name":name,"id":node.id.0,"node_sha256":digest(node),"world_bits":scene.world_matrix(node.id).to_cols_array().map(f32::to_bits)})
 }).collect();serde_json::json!(rows)
}
fn main(){
 assert_send::<Document>();assert_send::<loom_scene::Scene>();
 #[cfg(probe_require_sync)]assert_sync::<loom_scene::Scene>();
 let input=PathBuf::from(std::env::args_os().nth(1).unwrap());let home=PathBuf::from(std::env::var_os("LOOM_HOME").expect("private child LOOM_HOME"));
 let private=PathBuf::from(env!("PROBE_ROOT"));assert!(home.starts_with(&private));assert!(!home.exists(),"each child gets a new private directory");
 let mut hash=Sha256::new();let mut reader=std::fs::File::open(&input).unwrap();let mut bytes=[0u8;65536];loop{let n=reader.read(&mut bytes).unwrap();if n==0{break}hash.update(&bytes[..n]);}let input_sha=format!("{:x}",hash.finalize());assert_eq!(input_sha,"84fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935");
 let load=Instant::now();let loaded=file::read(&input).unwrap();let load_ms=load.elapsed().as_secs_f64()*1000.0;
 let mut state=AppState::new();state.scene=loaded.scene;loaded.settings.apply_to(&mut state);state.file=Some(input.clone());state.prefs.auto_backup_name="IsolatedRayBackup".into();state.prefs.auto_backup_count=3;state.autosave.next_index=1;
 let count=state.scene.len();let before_props=props(&state.scene);let original_file=state.file.clone();let undo_version=state.undo.version();
 let start=Instant::now();let backup=autosave::backup_now(&mut state).unwrap();let backup_ms=start.elapsed().as_secs_f64()*1000.0;
 assert_eq!(state.file,original_file);assert_eq!(state.undo.version(),undo_version);assert_eq!(state.autosave.next_index,2);assert_eq!(state.autosave.last_version,Some(undo_version));assert!(backup.starts_with(&home));
 let reopen=Instant::now();let saved=file::read(&backup).unwrap();let reopen_ms=reopen.elapsed().as_secs_f64()*1000.0;saved.scene.set_time(state.scene.time());assert_eq!(saved.scene.len(),count);assert_eq!(digest(&state.scene),digest(&saved.scene));assert_eq!(before_props,props(&saved.scene));assert_eq!(serde_json::to_value(&saved.settings).unwrap(),serde_json::to_value(SceneSettings::from_state(&state)).unwrap());
 let mut skin_rows=Vec::new();for node in state.scene.nodes(){for(index,slot)in node.modifiers.iter().enumerate(){if let loom_scene::Modifier::Skin(data)=&slot.modifier{
  let other=&saved.scene.node(node.id).unwrap().modifiers[index];assert_eq!(slot,other);
  let loom_scene::Modifier::Skin(other_data)=&other.modifier else{panic!("reopened Skin")};for(a,b)in data.weights.iter().zip(&other_data.weights){assert_eq!(a.len(),b.len());for((ab,aw),(bb,bw))in a.iter().zip(b){assert_eq!((*ab,aw.to_bits()),(*bb,bw.to_bits()));}}
  skin_rows.push(serde_json::json!({"node":node.name,"vertices":data.weights.len(),"max_influences":data.weights.iter().map(Vec::len).max(),"skin_sha256":digest(&slot.modifier)}));
 }}}
 let clone_start=Instant::now();let snapshot=Document{format:file::FORMAT,app:"3dloom 0.1.0".into(),scene:state.scene.clone(),settings:SceneSettings::from_state(&state)};let clone_ms=clone_start.elapsed().as_secs_f64()*1000.0;
 let serialize_start=Instant::now();let text=serde_json::to_string_pretty(&snapshot).unwrap();let pretty_ms=serialize_start.elapsed().as_secs_f64()*1000.0;let pretty_bytes=text.len();drop(text);
 let mut isolated=snapshot.clone();let animated=state.scene.nodes().find_map(|n|n.transform.position.keys().and_then(|k|k.tracks.iter().position(|t|!t.keys.is_empty())).map(|axis|(n.id,axis))).expect("actual animated position curve");
 let before=digest(&snapshot.scene.node(animated.0).unwrap().transform);let keys=isolated.scene.node_mut(animated.0).unwrap().transform.position.ensure_keys().unwrap();keys.tracks[animated.1].keys[0].value+=1.125;assert_ne!(digest(&isolated.scene.node(animated.0).unwrap().transform),before);assert_eq!(digest(&snapshot.scene.node(animated.0).unwrap().transform),before);assert_eq!(digest(&state.scene.node(animated.0).unwrap().transform),before);
 let worker=std::thread::spawn(move||digest(&isolated.scene.node(animated.0).unwrap().transform));let worker_hash=worker.join().unwrap();assert_ne!(worker_hash,before);
 println!("{}",serde_json::to_string_pretty(&serde_json::json!({"context":"isolated release path during concurrent release build10945; stability32851 terminal; not quiet/native FPS","input":input,"input_sha256":input_sha,"load_ms":load_ms,"original_backup_now_ms":backup_ms,"reopen_ms":reopen_ms,"separate_snapshot_clone_ms":clone_ms,"separate_pretty_serialization_ms":pretty_ms,"pretty_bytes":pretty_bytes,"saved_bytes":std::fs::metadata(&backup).unwrap().len(),"backup":backup,"private_loom_home":home,"nodes":count,"scene_sha256":digest(&state.scene),"skins":skin_rows,"props":before_props,"document_send":true,"scene_send":true,"mutable_animation_clone_isolated":true,"file_and_undo_state_unchanged":true,"retention_index_advanced_after_success":true})).unwrap());
}
