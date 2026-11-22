//! File > Import / Export of 3D Studio (.3ds), STL, DXF and ASCII Scene Export (.ase) files.

use glam::Vec3;
use loom_commands::{execute, AppState, Command};
use loom_scene::{NodeId, ObjectKind};

fn dir() -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("loom-geom-formats-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn boxed(s: &mut AppState, at: [f32; 3]) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: loom_commands::PrimitiveKind::Box, position: at, size: [10.0, 20.0, 30.0] }).unwrap();
    *s.selection.iter().next().unwrap()
}

fn bounds(s: &AppState, id: NodeId) -> (Vec3, Vec3) {
    let m = s.scene.eval_mesh(id).value.mesh.clone();
    let w = s.scene.object_matrix(id);
    m.vertices().iter().map(|v| w.transform_point3(v.position)).fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(a, b), p| (a.min(p), b.max(p)))
}

fn close(a: (Vec3, Vec3), b: (Vec3, Vec3)) -> bool {
    a.0.distance(b.0) < 1e-3 && a.1.distance(b.1) < 1e-3
}

#[test]
fn three_ds_round_trip_keeps_meshes_pivots_materials_lights_and_cameras() {
    let mut s = AppState::new();
    let b = boxed(&mut s, [20.0, 0.0, 0.0]);
    execute(&mut s, Command::SetTransform { id: b.0, position: None, rotation_deg: Some([0.0, 0.0, 30.0]), scale: None }).unwrap();
    let want = bounds(&s, b);
    let pivot = s.scene.world_matrix(b).w_axis.truncate();
    execute(&mut s, Command::CreateLight { kind: loom_commands::light::LightKind::TargetSpot, position: [0.0, -80.0, 60.0], target: [0.0; 3] }).unwrap();
    execute(&mut s, Command::CreateCameraAt { target: true, position: [0.0, -150.0, 40.0], look_at: [0.0; 3] }).unwrap();
    let path = dir().join("scene.3ds").to_string_lossy().into_owned();
    let r = execute(&mut s, Command::ExportFile { path: Some(path.clone()) }).unwrap();
    assert_eq!((r["objects"].as_u64(), r["lights"].as_u64(), r["cameras"].as_u64()), (Some(1), Some(1), Some(1)));
    let mut t = AppState::new();
    execute(&mut t, Command::ImportFile { path: Some(path) }).unwrap();
    let mesh = t.scene.nodes().find(|n| matches!(n.object, ObjectKind::EditableMesh(_))).expect("an Editable Mesh").id;
    assert_eq!(t.scene.node(mesh).unwrap().name, "Box001");
    assert!(close(bounds(&t, mesh), want), "{:?} vs {want:?}", bounds(&t, mesh));
    assert!(t.scene.world_matrix(mesh).w_axis.truncate().distance(pivot) < 1e-3, "the pivot comes from the mesh matrix");
    let m = t.scene.eval_mesh(mesh).value.mesh.clone();
    assert_eq!(m.face_count(), 12, "triangles");
    assert_eq!(m.invisible_edges().len(), 6, "each quad's diagonal stays hidden");
    let spot = t.scene.nodes().find(|n| n.name == "Spot001").expect("the spot");
    assert!(matches!(&spot.object, ObjectKind::Light(l) if l.target.is_some()));
    let cam = t.scene.nodes().find(|n| n.name == "Camera001").expect("the camera").id;
    let fov = match &t.scene.node(cam).unwrap().object { ObjectKind::Camera { fov, .. } => fov.get(), _ => 0.0 };
    assert!((fov - loom_scene::camera::DEFAULT_FOV).abs() < 0.05, "{fov}");
    assert!(t.scene.world_matrix(cam).w_axis.truncate().distance(Vec3::new(0.0, -150.0, 40.0)) < 1e-3);
    execute(&mut t, Command::UndoSceneOperation).unwrap();
    assert_eq!(t.scene.len(), 0, "one undo entry");
}

#[test]
fn stl_binary_and_ascii_round_trips_weld_into_one_mesh() {
    for binary in [true, false] {
        let mut s = AppState::new();
        let b = boxed(&mut s, [0.0, 0.0, 0.0]);
        let want = bounds(&s, b);
        execute(&mut s, Command::SetExchangeOption { dialog: "stl_export".into(), name: "binary".into(), value: serde_json::json!(binary) }).unwrap();
        let path = dir().join(format!("box-{binary}.stl")).to_string_lossy().into_owned();
        let r = execute(&mut s, Command::ExportSelected { path: Some(path.clone()) }).unwrap();
        assert_eq!(r["triangles"], 12);
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.starts_with(b"solid"), !binary);
        let mut t = AppState::new();
        execute(&mut t, Command::ImportFile { path: Some(path) }).unwrap();
        let id = *t.selection.iter().next().unwrap();
        let m = t.scene.eval_mesh(id).value.mesh.clone();
        assert_eq!((m.vertex_count(), m.face_count()), (8, 12), "welded");
        assert!(close(bounds(&t, id), want));
        // Weld off: every face keeps its own vertices.
        execute(&mut t, Command::SetExchangeOption { dialog: "stl_import".into(), name: "weld".into(), value: serde_json::json!(false) }).unwrap();
        let path = dir().join(format!("box-{binary}.stl")).to_string_lossy().into_owned();
        execute(&mut t, Command::ImportFile { path: Some(path) }).unwrap();
        let id = *t.selection.iter().next().unwrap();
        assert_eq!(t.scene.eval_mesh(id).value.mesh.vertex_count(), 36);
    }
}

#[test]
fn dxf_round_trip_puts_each_object_on_its_layer() {
    let mut s = AppState::new();
    let a = boxed(&mut s, [0.0, 0.0, 0.0]);
    let b = boxed(&mut s, [50.0, 0.0, 0.0]);
    let (wa, wb) = (bounds(&s, a), bounds(&s, b));
    let path = dir().join("boxes.dxf").to_string_lossy().into_owned();
    execute(&mut s, Command::ExportFile { path: Some(path.clone()) }).unwrap();
    let mut t = AppState::new();
    execute(&mut t, Command::ImportFile { path: Some(path) }).unwrap();
    let names: Vec<String> = t.scene.nodes().map(|n| n.name.clone()).collect();
    assert_eq!(names, ["Box001", "Box002"]);
    let ids: Vec<NodeId> = t.scene.nodes().map(|n| n.id).collect();
    assert!(close(bounds(&t, ids[0]), wa) && close(bounds(&t, ids[1]), wb));
    let m = t.scene.eval_mesh(ids[0]).value.mesh.clone();
    assert_eq!((m.face_count(), m.invisible_edges().len()), (12, 6), "quads come in as Editable Mesh triangles with hidden diagonals");
}

#[test]
fn ase_round_trip_keeps_meshes_pivots_and_multi_sub_materials() {
    use loom_scene::material::{next_material_id, MaterialKind, StandardMaterial, SubMaterial};
    let mut s = AppState::new();
    let b = boxed(&mut s, [20.0, 0.0, 0.0]);
    execute(&mut s, Command::SetTransform { id: b.0, position: None, rotation_deg: Some([0.0, 0.0, 30.0]), scale: None }).unwrap();
    let want = bounds(&s, b);
    let pivot = s.scene.world_matrix(b).w_axis.truncate();
    let mut red = StandardMaterial::new("Red");
    red.diffuse.set(Vec3::new(1.0, 0.0, 0.0));
    let red_id = next_material_id(&s.scene.materials);
    s.scene.materials.insert(red_id, red);
    let mut multi = StandardMaterial::new("Multi");
    multi.kind = MaterialKind::MultiSub(vec![SubMaterial { id: 1, name: String::new(), material: Some(red_id), enabled: true }]);
    let multi_id = next_material_id(&s.scene.materials);
    s.scene.materials.insert(multi_id, multi);
    s.scene.node_mut(b).unwrap().material = Some(multi_id);
    let path = dir().join("scene.ase").to_string_lossy().into_owned();
    let r = execute(&mut s, Command::ExportFile { path: Some(path.clone()) }).unwrap();
    assert_eq!((r["objects"].as_u64(), r["materials"].as_u64()), (Some(1), Some(1)));
    let mut t = AppState::new();
    execute(&mut t, Command::ImportFile { path: Some(path) }).unwrap();
    let n = t.scene.nodes().find(|n| matches!(n.object, ObjectKind::EditableMesh(_))).expect("an Editable Mesh, not the OBJ reader's poly");
    assert_eq!(n.name, "Box001");
    let mesh = n.id;
    assert!(close(bounds(&t, mesh), want), "{:?} vs {want:?}", bounds(&t, mesh));
    assert!(t.scene.world_matrix(mesh).w_axis.truncate().distance(pivot) < 1e-3, "the pivot comes from the node matrix");
    let m = t.scene.eval_mesh(mesh).value.mesh.clone();
    assert_eq!(m.face_count(), 12);
    assert_eq!(m.invisible_edges().len(), 6, "each quad's diagonal stays hidden");
    let top = t.scene.materials.get(&n.material.expect("the reference")).unwrap();
    assert_eq!(top.name, "Multi");
    let MaterialKind::MultiSub(list) = &top.kind else { panic!("{:?}", top.kind) };
    let sub = t.scene.materials.get(&list[0].material.unwrap()).unwrap();
    assert_eq!((list[0].id, sub.name.as_str(), sub.diffuse.get()), (1, "Red", Vec3::new(1.0, 0.0, 0.0)));
    execute(&mut t, Command::UndoSceneOperation).unwrap();
    assert_eq!(t.scene.len(), 0, "one undo entry");
}

#[test]
fn vrml_import_places_shapes_with_their_materials_z_up() {
    let path = dir().join("world.wrl");
    std::fs::write(&path, "#VRML V2.0 utf8\nDEF Post Transform { translation 0 5 0 children [ Shape { appearance Appearance { material Material { diffuseColor 1 0 0 } } geometry Box { size 2 10 2 } } ] }\n").unwrap();
    let mut s = AppState::new();
    let r = execute(&mut s, Command::ImportFile { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    assert_eq!(r["nodes"].as_array().map(|a| a.len()), Some(1));
    let n = s.scene.nodes().find(|n| n.name == "Post").expect("named after its DEF");
    assert!(matches!(n.object, ObjectKind::EditableMesh(_)));
    let (lo, hi) = bounds(&s, n.id);
    assert!(close((lo, hi), (Vec3::new(-1.0, -1.0, 0.0), Vec3::new(1.0, 1.0, 10.0))), "VRML +Y up is +Z: {lo} {hi}");
    let m = s.scene.materials.get(&n.material.unwrap()).unwrap();
    assert_eq!(m.diffuse.get(), Vec3::new(1.0, 0.0, 0.0));
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.len(), 0, "one undo entry");
}
