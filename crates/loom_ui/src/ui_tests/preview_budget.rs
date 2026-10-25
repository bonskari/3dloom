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
