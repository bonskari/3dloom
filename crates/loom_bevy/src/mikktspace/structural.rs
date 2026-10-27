//! Owned current-attribute preparation for consumers that compute tangent math.
use super::generated::{SGroup, STriInfo};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct StructuralRecipe {
    pub indices: Vec<i32>,
    pub triangles: Vec<StructuralTriangle>,
    pub initial_flags: Vec<i32>,
    pub active_count: usize,
    pub assigned_groups: Vec<[i32; 3]>,
    pub groups: Vec<StructuralGroup>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StructuralTriangle {
    pub original_face: i32,
    pub flags: i32,
    pub neighbors: [i32; 3],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StructuralGroup {
    pub members: Vec<i32>,
    pub representative: i32,
    pub orientation: bool,
}
impl StructuralRecipe {
    pub(super) fn record(&mut self, indices: &[i32], triangles: &[STriInfo], groups: &[SGroup], active_count: usize, initial_flags: Vec<i32>) {
        self.indices=indices.to_vec();
        self.triangles=triangles.iter().map(|t|StructuralTriangle{original_face:t.iOrgFaceNumber,flags:t.iFlag,neighbors:t.FaceNeighbors}).collect();
        self.initial_flags=initial_flags;
        self.active_count=active_count;
        self.assigned_groups=triangles.iter().map(|t|t.AssignedGroup.map(|g|if g.is_null(){-1}else{
            // Pointers belong to the still-live preparation group allocation.
            let id=unsafe{g.cast_const().offset_from(groups.as_ptr())};
            assert!(id>=0&&(id as usize)<groups.len()); id as i32
        })).collect();
        self.groups=groups.iter().map(|g|StructuralGroup{
            members:unsafe{std::slice::from_raw_parts(g.pFaceIndices,g.iNrFaces as usize)}.to_vec(),
            representative:g.iVertexRepresentative,orientation:g.bOrientPreserving,
        }).collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::{MeshData,mesh_data,mesh_data_part};
    use crate::mikktspace::{compile_structure,capture_preparation,capture_group_preparation};
    fn check(data:&MeshData) {
        let actual=compile_structure(data);
        let initial=capture_preparation(data);
        let expected=capture_group_preparation(data);
        assert_eq!(actual.indices,expected.preparation.indices);
        assert_eq!(actual.active_count,expected.preparation.active_count);
        assert_eq!(actual.initial_flags,initial.triangles.iter().map(|t|t.flags).collect::<Vec<_>>());
        assert_eq!(actual.assigned_groups,expected.assigned_groups);
        for (a,b) in actual.triangles.iter().zip(&expected.preparation.triangles) {
            assert_eq!((a.original_face,a.flags,a.neighbors),(b.original_face,b.flags,b.neighbors));
        }
        assert_eq!(actual.triangles.len(),expected.preparation.triangles.len());
        assert_eq!(actual.groups.len(),expected.groups.len());
        for (a,b) in actual.groups.iter().zip(&expected.groups) {
            assert_eq!((&a.members,a.representative,a.orientation),(&b.members,b.representative,b.orientation));
        }
    }
    #[test]
    fn structural_compiler_matches_capture_without_tangent_emission() {
        let mut mesh=loom_mesh::sphere_segmented(1.0,8,0.0);
        loom_mesh::uvmap::auto_map(&mut mesh,loom_mesh::uvmap::AutoMap::Spherical,1);
        let data=mesh_data(&mesh);
        check(&data);
        let mut changed=data.clone();changed.indices.swap(0,3);check(&changed);
        changed=data.clone();changed.uvs.fill([0.0;2]);check(&changed);
        changed=data.clone();changed.positions.fill([0.0;3]);check(&changed);
        changed=data.clone();changed.positions[1]=changed.positions[0];check(&changed);
        changed=data.clone();for uv in changed.uvs.iter_mut().take(12){*uv=[0.0;2];}check(&changed);
        check(&MeshData{positions:vec![],normals:vec![],uvs:vec![],indices:vec![]});
        struct NoTangents<'a>(&'a MeshData);
        impl bevy_mikktspace::Geometry for NoTangents<'_> {
            fn num_faces(&self)->usize{self.0.indices.len()/3}
            fn num_vertices_of_face(&self,_:usize)->usize{3}
            fn position(&self,f:usize,c:usize)->[f32;3]{self.0.positions[self.0.indices[f*3+c]as usize]}
            fn normal(&self,f:usize,c:usize)->[f32;3]{self.0.normals[self.0.indices[f*3+c]as usize]}
            fn tex_coord(&self,f:usize,c:usize)->[f32;2]{self.0.uvs[self.0.indices[f*3+c]as usize]}
            fn set_tangent_encoded(&mut self,_:[f32;4],_:usize,_:usize){panic!("structural compiler emitted a tangent")}
        }
        let mut output=StructuralRecipe::default();
        unsafe{crate::mikktspace::generated::compile_structure(&mut NoTangents(&data),&mut output);}
        assert!(!output.groups.is_empty());
        // Positive control: ordinary generation must reach the same forbidden setter.
        assert!(std::panic::catch_unwind(||unsafe{crate::mikktspace::generated::genTangSpace(&mut NoTangents(&data),180.0,None)}).is_err());
        // An unrelated nested worker validation state cannot supply this recipe's weld.
        let scope=crate::mikktspace::weld_validation::begin(&changed,Some(Default::default()));
        assert_eq!(compile_structure(&data),output);
        let _=scope.finish();
    }
    #[test]
    #[ignore="requires actual LOOM_PERF_SCENE; full 317-pose structural oracle"]
    fn actual_ray_structural_compiler_matches_capture_317() {
        let path=std::env::var("LOOM_PERF_SCENE").expect("real scene");
        let value:serde_json::Value=serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let scene:loom_scene::Scene=serde_json::from_value(value["scene"].clone()).unwrap();
        let ids:Vec<_>=scene.nodes().filter(|n|n.modifiers.iter().any(|m|matches!(&m.modifier,loom_scene::modifier::Modifier::Skin(_)))).map(|n|n.id).collect();
        assert_eq!(ids.len(),5);
        let mut geometry:Vec<_>=ids.iter().map(|_|crate::prepared_geometry::GeometryCache::default()).collect();
        let benchmark=std::env::var_os("LOOM_STRUCTURAL_BENCH").is_some();
        let frames:Vec<_>=if benchmark{vec![0.0,39.25,149.0]}else{(0..=157).map(|f|f as f32).chain([39.25]).chain((0..=157).map(|f|f as f32+0.5)).collect()};
        let poses=frames.len();let iterations=if benchmark{32}else{1};
        let mut count=0;let mut elapsed=std::time::Duration::ZERO;
        for frame in frames {
            scene.set_time(frame);let mut parts=0;
            for (i,id) in ids.iter().enumerate(){
                let mut mesh=scene.eval_mesh(*id).value.mesh.clone();
                loom_mesh::edit_ops::transform_mesh(&mut mesh,scene.object_matrix(*id)).unwrap();
                let data=geometry[i].mesh_data(&mesh);
                let surfaces=scene.node_surfaces(*id).unwrap();
                let materials:std::collections::BTreeSet<_>=mesh.faces().iter().filter(|f|!f.hidden).map(|f|f.material_id).collect();
                for material in materials {
                    let part=mesh_data_part(&mesh,&data,Some(material),surfaces.for_face(material).faceted);
                    let start=std::time::Instant::now();for _ in 0..iterations{std::hint::black_box(compile_structure(&part));}elapsed+=start.elapsed();
                    if !benchmark{check(&part);}count+=1;parts+=1;
                }
            }
            assert_eq!(parts,13);
        }
        assert_eq!(count,poses*13);
        eprintln!("STRUCTURAL poses={poses} parts={count} iterations={iterations} compile_ms_per_pose={:.3}",elapsed.as_secs_f64()*1000.0/(poses*iterations)as f64);
    }
}
