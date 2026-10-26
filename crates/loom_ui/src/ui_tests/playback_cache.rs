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
