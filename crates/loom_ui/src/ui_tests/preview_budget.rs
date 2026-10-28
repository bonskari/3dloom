use super::*;
use loom_scene::{Node,ObjectKind,material::{MaterialId,StandardMaterial,Map}};
#[test]
fn material_variants_share_budget_and_hidden_bakes_are_evicted(){
 let mut h=Harness::new();h.app.state.prefs.viewport_texture_budget_mb=16;
 let path=std::env::temp_dir().join(format!("loom-variants-{}.png",std::process::id()));image::RgbaImage::from_pixel(32,32,image::Rgba([100,80,60,255])).save(&path).unwrap();
 let mut nodes=Vec::new();
 for i in 0..20 {let id=MaterialId(i+1);let mut material=StandardMaterial::new(&format!("variant{i}"));let mut map=Map::bitmap(path.to_string_lossy().into_owned(),100.);map.crop.apply=true;map.crop.place=true;map.crop.w=0.1+i as f32*0.01;material.diffuse_map=Some(map);material.show_map_in_viewport=true;h.app.state.scene.materials.insert(id,material);let mut node=Node::new(&format!("box{i}"),ObjectKind::box_object(1.,1.,1.));node.material=Some(id);nodes.push(h.app.state.scene.add_node(node));}
 h.app.rebuild_world_meshes();
 assert!(h.app.preview_max_side<=457,"20 distinct GPU variants must share16MiB, got maxside{}",h.app.preview_max_side);
 let deadline=std::time::Instant::now()+std::time::Duration::from_secs(10);
 while h.app.baked_cache.len()<20 {h.frames(1);assert!(std::time::Instant::now()<deadline);std::thread::sleep(std::time::Duration::from_millis(1));}
 let bytes:usize=h.app.baked_cache.values().flatten().map(|b|b.rgba.len()).sum();assert!(bytes<=16*1024*1024);
 for id in nodes.into_iter().skip(1){h.app.state.scene.remove_node(id);}
 h.app.rebuild_world_meshes();assert!(h.app.baked_cache.len()<=1,"hidden variants release their preview images immediately");
 let deadline=std::time::Instant::now()+std::time::Duration::from_secs(5);while h.app.baked_cache.is_empty(){h.frames(1);assert!(std::time::Instant::now()<deadline);std::thread::sleep(std::time::Duration::from_millis(1));}assert_eq!(h.app.baked_cache.len(),1);
 std::fs::remove_file(path).unwrap();
}

#[test]
fn actual_opacity_map_reaches_viewport_alpha_without_gamma_or_pending_hiding(){
 use loom_scene::material::Map;
 let mut h=Harness::new();
 let path=std::env::temp_dir().join(format!("loom-viewport-opacity-{}.png",std::process::id()));
 image::RgbaImage::from_fn(4,1,|x,_|{let value=[0,4,128,255][x as usize];image::Rgba([value,value,value,255])}).save(&path).unwrap();
 let mut material=StandardMaterial::new("Native opacity");material.diffuse.set(Vec3::new(1.,0.,0.));material.opacity_map=Some(Map::bitmap(path.to_string_lossy().into_owned(),100.));
 h.app.state.scene.materials.insert(MaterialId(1),material);let mut node=Node::new("opacity object",ObjectKind::box_object(1.,1.,1.));node.material=Some(MaterialId(1));h.app.state.scene.add_node(node);
 h.app.rebuild_world_meshes();assert!(!h.app.world_styles()[0].texture_alpha,"pending composition must not hide geometry");
 let deadline=std::time::Instant::now()+std::time::Duration::from_secs(5);
 while !h.app.world_styles()[0].texture_alpha{h.frames(1);assert!(std::time::Instant::now()<deadline,"opacity never reached viewport");std::thread::sleep(std::time::Duration::from_millis(1));}
 let upload=h.app.pending_upload.lock().unwrap();let bitmap=&upload.as_ref().unwrap().bitmaps[0].1;
 assert_eq!(bitmap.rgba.chunks_exact(4).map(|p|p[3]).collect::<Vec<_>>(),vec![0,4,128,255]);
 std::fs::remove_file(path).unwrap();
}
