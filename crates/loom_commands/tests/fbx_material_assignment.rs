use glam::Vec3;
use loom_commands::{execute, AppState, Command};
use loom_fbx::{FbxDocument, FbxKind, FbxMaterial, FbxNode, WriteOpts};
use loom_scene::material::{MaterialKind, Surfaces};

#[test]
fn fbx_mesh_material_slots_bind_native_multi_sub_in_source_order_and_undo_atomically() {
    let mut mesh = loom_mesh::box_primitive(Vec3::ONE);
    for (index, face) in mesh.faces_mut().iter_mut().enumerate() { face.material_id = (index % 2) as u16 + 1; }
    let mut red=FbxMaterial::new("Red");red.diffuse=[1.0,0.0,0.0];
    let mut green=FbxMaterial::new("Green");green.diffuse=[0.0,1.0,0.0];
    let mut first=FbxNode::new("First",FbxKind::Mesh{mesh:mesh.clone(),materials:vec!["Green".into(),"Red".into()]});
    first.translation=Vec3::X;
    let second=FbxNode::new("Second",FbxKind::Mesh{mesh:mesh.clone(),materials:vec!["Green".into(),"Red".into()]});
    let source=FbxDocument{nodes:vec![first,second],materials:vec![red,green],unit_metres:0.0254,..Default::default()};
    let path=std::env::temp_dir().join(format!("loom-fbx-multisub-{}.fbx",std::process::id()));
    std::fs::write(&path,loom_fbx::write(&source,&WriteOpts::default())).unwrap();
    let mut state=AppState::new();let undo=state.undo.len();
    execute(&mut state,Command::ImportFile{path:Some(path.to_string_lossy().into_owned())}).unwrap();
    assert_eq!(state.undo.len(),undo+1);
    let nodes:Vec<_>=state.scene.nodes().collect();assert_eq!(nodes.len(),2);
    assert!(nodes[0].material.is_some(),"FBX material names must become an actual assigned material");
    assert_eq!(nodes[0].material,nodes[1].material,"identical ordered slots share one native Multi/Sub material");
    let MaterialKind::MultiSub(slots)=&state.scene.materials[&nodes[0].material.unwrap()].kind else{panic!("expected Multi/Sub-Object")};
    assert_eq!(slots.iter().map(|s|s.id).collect::<Vec<_>>(),vec![1,2]);
    assert_eq!(slots.iter().map(|s|state.scene.materials[&s.material.unwrap()].name.as_str()).collect::<Vec<_>>(),vec!["Green","Red"]);
    let Surfaces::PerId(surfaces)=state.scene.node_surfaces(nodes[0].id).unwrap() else{panic!("face surfaces missing")};
    assert_eq!(surfaces[&0].diffuse,Vec3::Y);assert_eq!(surfaces[&1].diffuse,Vec3::X);
    assert_eq!(nodes[0].object.mesh().faces().iter().map(|f|f.material_id).collect::<Vec<_>>(),mesh.faces().iter().map(|f|f.material_id - 1).collect::<Vec<_>>());
    let exported=path.with_extension("roundtrip.fbx");
    execute(&mut state,Command::ExportFile{path:Some(exported.to_string_lossy().into_owned())}).unwrap();
    let roundtrip=loom_fbx::read(&std::fs::read(&exported).unwrap(),0.0254).unwrap();
    for node in &roundtrip.nodes {if let FbxKind::Mesh{mesh:exported_mesh,materials}=&node.kind {assert_eq!(materials,&vec!["Green".to_owned(),"Red".to_owned()]);assert_eq!(exported_mesh.faces().iter().map(|f|f.material_id).collect::<Vec<_>>(),mesh.faces().iter().map(|f|f.material_id).collect::<Vec<_>>());}}
    std::fs::remove_file(exported).unwrap();
    execute(&mut state,Command::UndoSceneOperation).unwrap();assert!(state.scene.is_empty());assert!(state.scene.materials.is_empty());
    execute(&mut state,Command::RedoSceneOperation).unwrap();assert!(state.scene.nodes().all(|n|state.scene.node_surfaces(n.id).is_some()));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn external_ray_textured_fbx_binds_all_multi_slot_meshes_without_changing_geometry_or_source() {
    let Some(path)=std::env::var_os("LOOM_METAHUMAN_FBX") else{return};let path=std::path::Path::new(&path);
    let bytes=std::fs::read(path).unwrap();
    let source=loom_fbx::read(&bytes,0.0254).unwrap();
    let source_meshes:Vec<_>=source.nodes.iter().filter_map(|n|match &n.kind{FbxKind::Mesh{mesh,materials}=>Some((mesh,materials)),_=>None}).collect();
    let mut state=AppState::new();state.exchange.fbx_import.metahuman_body_only=true;
    execute(&mut state,Command::ImportFile{path:Some(path.to_string_lossy().into_owned())}).unwrap();
    let meshes:Vec<_>=state.scene.nodes().filter(|n|matches!(n.object,loom_scene::ObjectKind::EditablePoly(_))).collect();
    assert_eq!(meshes.len(),source_meshes.len());let mut multis=0;
    for (node,(source_mesh,names)) in meshes.iter().zip(source_meshes) {
        assert!(node.material.is_some(),"{} has {} names but no assignment",node.name,names.len());
        assert!(state.scene.node_surfaces(node.id).is_some(),"{} does not resolve viewport materials",node.name);
        let mut expected=source_mesh.clone();for face in expected.faces_mut(){face.material_id=face.material_id.saturating_sub(1);}
        assert_eq!(node.object.mesh().as_ref(),&expected,"{} geometry changed beyond material-ID convention",node.name);
        if names.len()>1 {
            multis+=1;let MaterialKind::MultiSub(slots)=&state.scene.materials[&node.material.unwrap()].kind else{panic!("{} lacks Multi/Sub",node.name)};
            assert_eq!(slots.len(),names.len());
            for (index,(slot,name)) in slots.iter().zip(names).enumerate(){assert_eq!(slot.id,index as u16+1);assert_eq!(&state.scene.materials[&slot.material.unwrap()].name,name);}
        }
    }
    assert_eq!(multis,4);assert_eq!(std::fs::read(path).unwrap(),bytes);
    eprintln!("real Ray FBX: {} meshes, {multis} assigned Multi/Sub materials; geometry and source unchanged",meshes.len());
}
