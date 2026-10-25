//! File > Import / Export of 3D Studio (.3ds), STL and DXF files.

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
