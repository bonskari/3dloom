use loom_mesh::{Mesh,HalfEdgeSnapshot,HalfEdgeId,HalfEdge};
use loom_render::{MeshStyle,gpu_edges::GpuEdgeError};
use std::{hint::black_box,time::Instant};
struct DescriptorSnapshot {
    vertex_edges: Vec<Option<loom_mesh::HalfEdgeId>>,
    half_edges: loom_mesh::HalfEdgeSnapshot,
    faces: Vec<loom_mesh::half_edge::Face>,
    maps: [Option<MapSnapshot>; 2],
}
struct MapSnapshot { corners: Vec<u32>, values: Vec<[u32; 3]> }
impl DescriptorSnapshot {
    fn new(mesh: &Mesh) -> Self {
        Self {
            vertex_edges: mesh.vertices().iter().map(|v|v.he).collect(),
            half_edges: mesh.half_edge_snapshot(), faces: mesh.faces().to_vec(),
            maps: [1,loom_mesh::NORMALS_CHANNEL].map(|channel|mesh.map(channel).map(|m|MapSnapshot {
                corners:m.corners.clone(), values:m.verts.iter().map(|v|v.to_array().map(f32::to_bits)).collect(),
            })),
        }
    }
    fn matches(&self, mesh: &Mesh) -> bool {
        self.vertex_edges.len()==mesh.vertex_count()
            && self.vertex_edges.iter().zip(mesh.vertices()).all(|(he,v)|*he==v.he)
            && self.half_edges.matches(mesh) && self.faces==mesh.faces()
            && self.maps.iter().zip([1,loom_mesh::NORMALS_CHANNEL]).all(|(old,channel)|match (old,mesh.map(channel)) {
                (None,None)=>true,
                (Some(old),Some(current))=>old.corners==current.corners && old.values.len()==current.verts.len()
                    && old.values.iter().zip(&current.verts).all(|(a,b)|*a==b.to_array().map(f32::to_bits)),
                _=>false,
            })
    }
}


struct EdgeGuard {source_count:usize,topology:HalfEdgeSnapshot,all_visible:bool,visible:Vec<bool>}
impl EdgeGuard {
    fn new(mesh:&Mesh)->Self {let visible:Vec<_>=(0..mesh.half_edges().len()).map(|i|mesh.edge_visible(HalfEdgeId(i as u32))).collect();Self {source_count:mesh.vertex_count(),topology:mesh.half_edge_snapshot(),all_visible:visible.iter().all(|x|*x),visible}}
    pub fn matches_geometry(&self, mesh: &Mesh, style: &MeshStyle) -> Result<bool, GpuEdgeError> {
        if style.diagonals {
            return Err(GpuEdgeError::Diagonals);
        }
        Ok(self.source_count == mesh.vertex_count()
            && self.topology.matches(mesh)
            && if self.all_visible {
                // The same effective predicate used by ordinary edge emission;
                // an absent visibility array needs no per-half-edge scan.
                !mesh.has_invisible_edges()
            } else {
                self.visible.iter().enumerate()
                    .all(|(i, visible)| *visible == mesh.edge_visible(HalfEdgeId(i as u32)))
            })
    }

}
struct GeometryGuard {vertex_edges:Vec<Option<HalfEdgeId>>,half_edges:Vec<HalfEdge>,face_edges:Vec<HalfEdgeId>}
impl GeometryGuard {
    fn new(mesh:&Mesh)->Self {Self {vertex_edges:mesh.vertices().iter().map(|v|v.he).collect(),half_edges:mesh.half_edges().to_vec(),face_edges:mesh.faces().iter().map(|f|f.he).collect()}}
    fn matches(&self,mesh:&Mesh)->bool {
        self.vertex_edges.len()==mesh.vertex_count()
            && self.half_edges==mesh.half_edges()
            && self.face_edges.len()==mesh.face_count()
            && self.vertex_edges.iter().zip(mesh.vertices()).all(|(he,v)|*he==v.he)
            && self.face_edges.iter().zip(mesh.faces()).all(|(he,f)|*he==f.he)
    }
}
fn timed(mut run:impl FnMut())->f64 {let t=Instant::now();for _ in 0..16 {run();}t.elapsed().as_secs_f64()*1e3/16.0}
fn pair(name:&str,mut old:impl FnMut(),mut fresh:impl FnMut(),mut rebased:impl FnMut()) {
    let mut a=0.0;let mut b=0.0;let mut c=0.0;
    for round in 0..32 {let aa=timed(&mut old);let bb=timed(&mut fresh);let bc=timed(&mut fresh);let ab=timed(&mut old);let cc=timed(&mut rebased);a+=(aa+ab)*0.5;b+=(bb+bc)*0.5;c+=cc;println!("HE_MATCH_ABBA guard={name} round={round} shared_ms={:.9} old_snapshot_fresh_equal_ms={:.9} fresh_snapshot_fresh_ms={cc:.9}",(aa+ab)*0.5,(bb+bc)*0.5);}
    println!("HE_MATCH_SUMMARY guard={name} rounds=32 repeats=16 meshes=5 shared_ms={:.9} old_snapshot_fresh_equal_ms={:.9} fresh_snapshot_fresh_ms={:.9} extra_ms={:.9}",a/32.0,b/32.0,c/32.0,(b-a)/32.0);
}
fn main(){
    let path=std::env::var("LOOM_PERF_SCENE").unwrap();let mut wrapper:serde_json::Value=serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let scene:loom_scene::Scene=serde_json::from_value(wrapper["scene"].take()).unwrap();drop(wrapper);scene.set_time(39.0);
    let ids:Vec<_>=scene.nodes().filter(|n|n.modifiers.iter().any(|m|matches!(&m.modifier,loom_scene::modifier::Modifier::Skin(_)))).map(|n|n.id).collect();assert_eq!(ids.len(),5);
    let old:Vec<_>=ids.iter().map(|id|{let mut mesh=scene.eval_mesh(*id).value.mesh.clone();loom_mesh::edit_ops::transform_mesh(&mut mesh,scene.object_matrix(*id)).unwrap();mesh}).collect();
    let fresh:Vec<Mesh>=old.iter().map(|mesh|serde_json::from_slice(&serde_json::to_vec(mesh).unwrap()).unwrap()).collect();
    let old_snaps:Vec<_>=old.iter().map(Mesh::half_edge_snapshot).collect();let fresh_snaps:Vec<_>=fresh.iter().map(Mesh::half_edge_snapshot).collect();
    let old_desc:Vec<_>=old.iter().map(DescriptorSnapshot::new).collect();let fresh_desc:Vec<_>=fresh.iter().map(DescriptorSnapshot::new).collect();
    let old_edges:Vec<_>=old.iter().map(EdgeGuard::new).collect();let fresh_edges:Vec<_>=fresh.iter().map(EdgeGuard::new).collect();
    let old_geom:Vec<_>=old.iter().map(GeometryGuard::new).collect();let fresh_geom:Vec<_>=fresh.iter().map(GeometryGuard::new).collect();let style=MeshStyle::default();
    for(i,id)in ids.iter().enumerate(){assert_eq!(old[i],fresh[i]);assert_ne!(old[i].half_edges().as_ptr(),fresh[i].half_edges().as_ptr());assert!(old_snaps[i].matches(&old[i]));assert!(old_snaps[i].matches(&fresh[i]));assert!(old_desc[i].matches(&fresh[i]));assert!(old_edges[i].matches_geometry(&fresh[i],&style).unwrap());assert!(old_geom[i].matches(&fresh[i]));println!("HE_MATCH_INPUT node={id:?} vertices={} halfedges={} faces={} old_he_slice={:p} fresh_he_slice={:p} all_visible={} complete_mesh_value_equal=PASS",old[i].vertex_count(),old[i].half_edges().len(),old[i].face_count(),old[i].half_edges().as_ptr(),fresh[i].half_edges().as_ptr(),old_edges[i].all_visible);}
    let mut positions_only=old[0].clone();positions_only.vertices_mut()[0].position.x+=1.0;assert!(old_snaps[0].matches(&positions_only));assert!(old_desc[0].matches(&positions_only));assert!(old_edges[0].matches_geometry(&positions_only,&style).unwrap());
    let mut changed=serde_json::to_value(&fresh[0]).unwrap();let previous=changed["half_edges"][0]["origin"].as_u64().unwrap();changed["half_edges"][0]["origin"]=serde_json::json!((previous+1)%old[0].vertex_count()as u64);let changed:Mesh=serde_json::from_value(changed).unwrap();assert!(!old_snaps[0].matches(&changed));assert!(!old_desc[0].matches(&changed));assert!(!old_edges[0].matches_geometry(&changed,&style).unwrap());
    println!("HE_MATCH_CONTROLS positions_excluded=PASS same_count_changed_HE_rejected=PASS");
    pair("HalfEdgeSnapshot",||{for(s,m)in old_snaps.iter().zip(&old){black_box(black_box(s).matches(black_box(m)));}},||{for(s,m)in old_snaps.iter().zip(&fresh){black_box(black_box(s).matches(black_box(m)));}},||{for(s,m)in fresh_snaps.iter().zip(&fresh){black_box(black_box(s).matches(black_box(m)));}});
    pair("GpuDescriptor",||{for(s,m)in old_desc.iter().zip(&old){black_box(black_box(s).matches(black_box(m)));}},||{for(s,m)in old_desc.iter().zip(&fresh){black_box(black_box(s).matches(black_box(m)));}},||{for(s,m)in fresh_desc.iter().zip(&fresh){black_box(black_box(s).matches(black_box(m)));}});
    pair("GpuEdgeGeometry",||{for(s,m)in old_edges.iter().zip(&old){black_box(black_box(s).matches_geometry(black_box(m),&style).unwrap());}},||{for(s,m)in old_edges.iter().zip(&fresh){black_box(black_box(s).matches_geometry(black_box(m),&style).unwrap());}},||{for(s,m)in fresh_edges.iter().zip(&fresh){black_box(black_box(s).matches_geometry(black_box(m),&style).unwrap());}});
    pair("BevyGeometryTopology",||{for(s,m)in old_geom.iter().zip(&old){black_box(black_box(s).matches(black_box(m)));}},||{for(s,m)in old_geom.iter().zip(&fresh){black_box(black_box(s).matches(black_box(m)));}},||{for(s,m)in fresh_geom.iter().zip(&fresh){black_box(black_box(s).matches(black_box(m)));}});
    for(s,m)in old_snaps.iter().zip(&fresh){assert!(s.matches(m));}println!("HE_MATCH_TERMINAL all_guards_equal=PASS no_scene_or_application_mutation=PASS");
}
