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
