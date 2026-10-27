use super::*;
use loom_scene::{Node,ObjectKind,PolyObject,Scene};
#[test]
fn playback_reuses_static_editable_cages_and_invalidates_world_and_scene_changes(){
 let mut h=Harness::new();
 let id=h.app.state.scene.add_node(Node::new("Body",ObjectKind::EditablePoly(PolyObject::from_mesh(loom_mesh::box_primitive(Vec3::ONE)))));
 h.app.rebuild_world_meshes();
 let first=h.app.world_meshes()[0].1.vertices().as_ptr();
 h.cmd(Command::SetFrame{frame:1});
 assert_eq!(h.app.world_meshes()[0].1.vertices().as_ptr(),first,"a pose tick must not clone the unchanged editing cage");
 h.app.state.scene.node_mut(id).unwrap().transform.position.set(Vec3::X*10.);
 h.app.rebuild_world_meshes();
 assert!((h.app.world_meshes()[0].1.aabb().0.x-9.).abs()<1e-6);
 h.app.state.scene=Scene::new();
 let replacement=h.app.state.scene.add_node(Node::new("Body",ObjectKind::EditablePoly(PolyObject::from_mesh(loom_mesh::box_primitive(Vec3::splat(3.))))));
 assert_eq!(id,replacement);
 h.app.rebuild_world_meshes();
 assert_eq!(h.app.world_meshes()[0].1.aabb(),(-Vec3::splat(3.),Vec3::splat(3.)));
}

#[test]
fn rigid_bone_geometry_survives_pose_changes_but_tracks_animated_dimensions(){
 let mut h=Harness::new();let mut bone=Node::new("limb",ObjectKind::bone(10.));
 loom_scene::anim::with(0.,true,||bone.transform.position.key_now());
 loom_scene::anim::with(10.,true,||bone.transform.position.set(Vec3::X*10.));
 let id=h.app.state.scene.add_node(bone);h.app.rebuild_world_meshes();
 let initial=h.app.bone_geometry.get(&id).expect("rigid bone geometry should be cached").mesh.clone();
 let bounds=h.app.world_meshes()[0].1.aabb();
 h.cmd(Command::SetFrame{frame:10});
 assert!(std::sync::Arc::ptr_eq(&initial,&h.app.bone_geometry[&id].mesh));
 assert!((h.app.world_meshes()[0].1.aabb().0-bounds.0-Vec3::X*10.).length()<1e-5);
 if let ObjectKind::Bone{length,..}=&mut h.app.state.scene.node_mut(id).unwrap().object{
  loom_scene::anim::with(0.,true,||length.key_now());
  loom_scene::anim::with(10.,true,||length.set(20.));
 }
 h.cmd(Command::SetFrame{frame:0});let first=h.app.bone_geometry[&id].mesh.clone();
 h.cmd(Command::SetFrame{frame:10});
 assert!(!std::sync::Arc::ptr_eq(&first,&h.app.bone_geometry[&id].mesh));
 let expected=h.app.state.scene.node(id).unwrap().object.mesh().into_owned();
 assert_eq!(*h.app.bone_geometry[&id].mesh,expected,"animated dimensions must never reuse the rest shape");
}

#[test]
fn batched_skin_display_matches_serial_evaluation_and_keeps_editing_cages() {
 let mut h=Harness::new();
 let bone=h.app.state.scene.add_node(Node::new("driver",ObjectKind::bone(3.)));
 loom_scene::anim::with(0.,true,||h.app.state.scene.transform_mut(bone).unwrap().transform.position.key_now());
 loom_scene::anim::with(10.,true,||h.app.state.scene.transform_mut(bone).unwrap().transform.position.set(Vec3::new(3.,2.,1.)));
 let mut skins=Vec::new();
 for size in [1.,2.] {
  let mut poly=PolyObject::from_mesh(loom_mesh::box_primitive(Vec3::splat(size)));
  poly.passes_up=Some((loom_mesh::select_ops::SubLevel::Vertex,vec![0,2]));
  let weights=vec![vec![(0,1.)];poly.mesh.vertex_count()];
  let data=loom_scene::skin::SkinData::from_file(&[(bone,glam::Mat4::IDENTITY,loom_scene::skin::Envelope::default())],glam::Mat4::IDENTITY,weights);
  let mut node=Node::new("shirt",ObjectKind::EditablePoly(poly));
  node.modifiers.push(loom_scene::ModifierSlot::new(loom_scene::Modifier::Skin(Box::new(data))));
  skins.push(h.app.state.scene.add_node(node));
 }
 for frame in [0.,3.,39.25] {
  h.app.state.scene.set_time(frame);
  let serial=h.app.state.scene.clone();
  h.app.rebuild_world_meshes();
  for id in &skins {
   let i=h.app.world_meshes().iter().position(|(n,_)|n==id).unwrap();
   let mut expected=serial.eval_mesh(*id).value.mesh.clone();
   loom_mesh::edit_ops::transform_mesh(&mut expected,serial.object_matrix(*id)).unwrap();
   assert_eq!(h.app.world_display[i],expected);
   for (a,b) in h.app.world_display[i].vertices().iter().zip(expected.vertices()) {
    assert_eq!(a.position.to_array().map(f32::to_bits),b.position.to_array().map(f32::to_bits));
   }
   let mut cage=serial.node(*id).unwrap().cage_mesh().into_owned();
   loom_mesh::edit_ops::transform_mesh(&mut cage,serial.object_matrix(*id)).unwrap();
   assert_eq!(h.app.world_meshes()[i].1,cage,"picking must retain the undeformed editing cage");
  }
 }
}
