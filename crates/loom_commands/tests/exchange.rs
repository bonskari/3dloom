//! FBX and OBJ import / export options (Set Exchange Option) and their
//! effect on the files and the scene; materials and textures both ways.

use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp};
use loom_scene::material::{BitmapMap, MaterialId, StandardMaterial};
use loom_scene::{Modifier, NodeId, ObjectKind};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join("loom-exchange-tests").join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn boxed(s: &mut AppState, x: f32) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [x, 0.0, 0.0], size: [10.0; 3] }).unwrap();
    *s.selection.iter().next().unwrap()
}

fn opt(s: &mut AppState, dialog: &str, name: &str, value: Value) {
    execute(s, Command::SetExchangeOption { dialog: dialog.into(), name: name.into(), value }).unwrap();
}

fn p(path: &Path) -> Option<String> {
    Some(path.to_string_lossy().into_owned())
}

fn export(s: &mut AppState, path: &Path) {
    execute(s, Command::ExportFile { path: p(path) }).unwrap();
}

fn import(s: &mut AppState, path: &Path) -> Value {
    execute(s, Command::ImportFile { path: p(path) }).unwrap()
}

/// A 4×4 PNG of one colour.
fn png(path: &Path, rgb: [u8; 3]) {
    image::RgbaImage::from_pixel(4, 4, image::Rgba([rgb[0], rgb[1], rgb[2], 255])).save(path).unwrap();
}

/// A Standard material with a diffuse map, assigned to `id`.
fn assign(s: &mut AppState, id: NodeId, name: &str, map: &Path) -> MaterialId {
    let mut m = StandardMaterial::new(name);
    m.set_color("Diffuse", [200, 50, 25]).unwrap();
    m.specular_level.set(40.0);
    m.glossiness.set(30.0);
    m.opacity.set(80.0);
    m.diffuse_map = Some(BitmapMap { path: map.to_string_lossy().into_owned(), amount: 100.0, enabled: true });
    m.bump_map = Some(BitmapMap { path: map.to_string_lossy().into_owned(), amount: 45.0, enabled: true });
    let mid = loom_scene::material::next_material_id(&s.scene.materials);
    s.scene.materials.insert(mid, m);
    s.scene.node_mut(id).unwrap().material = Some(mid);
    mid
}

#[test]
fn options_are_set_by_command_and_listed() {
    let mut s = AppState::new();
    let r = execute(&mut s, Command::ExchangeOptions { dialog: "fbx_export".into() }).unwrap();
    assert_eq!(r["options"]["up_axis"], "Y-up", "the default preset exports Y-up: {r}");
    assert!(r["presets"].as_array().unwrap().iter().any(|p| p == "Media & Entertainment"));
    opt(&mut s, "fbx_export", "triangulate", json!(true));
    assert!(s.exchange.fbx_export.triangulate);
    assert_eq!(s.exchange.fbx_export.preset, "User defined");
    let e = execute(&mut s, Command::SetExchangeOption { dialog: "fbx_export".into(), name: "no_such".into(), value: json!(1) }).unwrap_err();
    assert!(e.contains("triangulate"), "the error lists the options: {e}");
    assert!(execute(&mut s, Command::SetExchangeOption { dialog: "nonsense".into(), name: "x".into(), value: json!(1) }).is_err());
    execute(&mut s, Command::SaveExchangePreset { dialog: "fbx_export".into(), name: "Game".into() }).unwrap();
    opt(&mut s, "fbx_export", "preset", json!("Media & Entertainment"));
    assert!(!s.exchange.fbx_export.triangulate);
    opt(&mut s, "fbx_export", "preset", json!("Game"));
    assert!(s.exchange.fbx_export.triangulate);
    // The options and user presets go into the settings file.
    let settings = loom_commands::settings::Settings::capture(&s);
    let mut t = AppState::new();
    loom_commands::settings::Settings::from_json(&settings.to_json()).unwrap().apply(&mut t);
    assert!(t.exchange.fbx_export.triangulate && t.exchange.user_presets["fbx_export"].contains_key("Game"));
}

#[test]
fn fbx_axis_units_format_and_triangulate() {
    let d = dir("fbx_axis");
    let mut s = AppState::new();
    boxed(&mut s, 0.0);
    let f = d.join("a.fbx");
    export(&mut s, &f);
    let i = loom_fbx::info(&std::fs::read(&f).unwrap()).unwrap();
    assert_eq!(i.up_axis, "Y-up");
    assert!((i.unit_metres - 0.01).abs() < 1e-9, "centimetres by default: {}", i.unit_metres);
    opt(&mut s, "fbx_export", "up_axis", json!("Z-up"));
    opt(&mut s, "fbx_export", "units_automatic", json!(true));
    opt(&mut s, "fbx_export", "ascii", json!(true));
    opt(&mut s, "fbx_export", "version", json!(7500));
    opt(&mut s, "fbx_export", "triangulate", json!(true));
    export(&mut s, &f);
    let bytes = std::fs::read(&f).unwrap();
    let i = loom_fbx::info(&bytes).unwrap();
    assert_eq!((i.up_axis.as_str(), i.ascii, i.version), ("Z-up", true, 7500));
    assert!((i.unit_metres - 0.0254).abs() < 1e-9, "Automatic: the system unit (inches): {}", i.unit_metres);
    execute(&mut s, Command::NewScene).unwrap();
    import(&mut s, &f);
    let n = s.scene.nodes().next().unwrap();
    let m = s.scene.eval_mesh(n.id).value.mesh.clone();
    assert_eq!(m.face_count(), 12, "triangulated");
    let (lo, hi) = m.aabb();
    assert!(((hi - lo) - glam::Vec3::splat(10.0)).length() < 1e-3, "same size: {lo} {hi}");
}

#[test]
fn y_up_export_keeps_cameras_and_objects_in_place() {
    // (Skins through a Y-up file: tests/fbx_skin.rs exports with the default, Y-up, options.)
    let d = dir("fbx_yup");
    let mut s = AppState::new();
    let b = boxed(&mut s, 0.0);
    execute(&mut s, Command::RotateSelection { axis: loom_commands::Axis::Z, degrees: 30.0, system: None }).unwrap();
    let box_world = s.scene.world_matrix(b);
    execute(&mut s, Command::CreateCamera { target: false }).unwrap();
    let cam = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::MoveSelection { delta: [10.0, -40.0, 5.0], system: None }).unwrap();
    let cam_world = s.scene.world_matrix(cam);
    let f = d.join("yup.fbx");
    export(&mut s, &f);
    execute(&mut s, Command::NewScene).unwrap();
    import(&mut s, &f);
    let cam2 = s.scene.nodes().find(|n| matches!(n.object, ObjectKind::Camera { .. })).unwrap().id;
    assert!(s.scene.world_matrix(cam2).abs_diff_eq(cam_world, 1e-3), "a camera comes back where it was through a Y-up file");
    let b2 = s.scene.nodes().find(|n| n.name == "Box001").unwrap().id;
    assert!(s.scene.world_matrix(b2).abs_diff_eq(box_world, 1e-3));
}

#[test]
fn fbx_include_options_cameras_animation_and_deformations() {
    let d = dir("fbx_include");
    let mut s = AppState::new();
    let b = boxed(&mut s, 0.0);
    execute(&mut s, Command::CreateCamera { target: false }).unwrap();
    // Animate the box: keys at 0 and 10.
    execute(&mut s, Command::SelectNodes { ids: vec![b.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::SetFrame { frame: 10 }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [50.0, 0.0, 0.0], system: None }).unwrap();
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::SetFrame { frame: 0 }).unwrap();
    let f = d.join("inc.fbx");
    // The key times of the box's X translation curve, in frames.
    let frames = |path: &Path| -> Vec<f64> {
        let sc = ufbx::load_memory(&std::fs::read(path).unwrap(), ufbx::LoadOpts::default()).unwrap();
        let fps = sc.settings.frames_per_second;
        sc.anim_curves.iter().map(|c| c.keyframes.iter().map(|k| (k.time * fps).round()).collect::<Vec<f64>>()).max_by_key(|v| v.len()).unwrap_or_default()
    };
    export(&mut s, &f);
    assert_eq!(frames(&f).len(), 101, "baked every frame of 0–100");
    opt(&mut s, "fbx_export", "bake_step", json!(10));
    export(&mut s, &f);
    assert_eq!(frames(&f), (0..=10).map(|i| i as f64 * 10.0).collect::<Vec<_>>());
    opt(&mut s, "fbx_export", "bake_animation", json!(false));
    export(&mut s, &f);
    assert_eq!(frames(&f), vec![0.0, 10.0, 100.0], "unbaked: the key frames and the range ends");
    opt(&mut s, "fbx_export", "animation", json!(false));
    opt(&mut s, "fbx_export", "cameras", json!(false));
    export(&mut s, &f);
    let doc = loom_fbx::read(&std::fs::read(&f).unwrap(), 0.01).unwrap();
    assert!(doc.anim.is_none(), "Animation off");
    assert!(!doc.nodes.iter().any(|n| matches!(n.kind, loom_fbx::FbxKind::Camera { .. })), "Cameras off");
    // Import: cameras off drops the camera; Update animation keys the existing box.
    opt(&mut s, "fbx_export", "animation", json!(true));
    opt(&mut s, "fbx_export", "cameras", json!(true));
    opt(&mut s, "fbx_export", "bake_animation", json!(true));
    opt(&mut s, "fbx_export", "bake_step", json!(1));
    export(&mut s, &f);
    let mut t = AppState::new();
    opt(&mut t, "fbx_import", "cameras", json!(false));
    import(&mut t, &f);
    assert!(!t.scene.nodes().any(|n| matches!(n.object, ObjectKind::Camera { .. })));
    let mut u = AppState::new();
    let ub = boxed(&mut u, 0.0);
    opt(&mut u, "fbx_import", "file_content", json!("Update animation"));
    let before = u.scene.len();
    import(&mut u, &f);
    assert_eq!(u.scene.len(), before, "Update animation adds nothing");
    let x = |st: &AppState, f: f32| st.scene.at_time(f, || st.scene.world_matrix(ub).w_axis.x);
    assert!((x(&u, 10.0) - 50.0).abs() < 1e-3 && x(&u, 0.0).abs() < 1e-3, "the existing Box001 moves like the file's");
}

#[test]
fn fbx_materials_and_textures_round_trip_with_embedded_media() {
    let d = dir("fbx_materials");
    let mut s = AppState::new();
    let b = boxed(&mut s, 0.0);
    let map = d.join("wood.png");
    png(&map, [10, 200, 30]);
    assign(&mut s, b, "Wood", &map);
    let f = d.join("mat.fbx");
    opt(&mut s, "fbx_export", "embed_media", json!(true));
    export(&mut s, &f);
    // The image is gone: the embedded copy comes out next to the FBX.
    std::fs::remove_file(&map).unwrap();
    let mut t = AppState::new();
    import(&mut t, &f);
    let n = t.scene.nodes().find(|n| n.name == "Box001").unwrap();
    let m = t.scene.node_material(n.id).expect("the material is assigned").clone();
    assert_eq!(m.name, "Wood");
    assert!((m.diffuse.get() - glam::Vec3::new(200.0, 50.0, 25.0) / 255.0).length() < 1e-4);
    assert!((m.specular_level.get() - 40.0).abs() < 1e-3 && (m.glossiness.get() - 30.0).abs() < 1e-2 && (m.opacity.get() - 80.0).abs() < 1e-3, "{m:?}");
    let dm = m.diffuse_map.as_ref().expect("diffuse map");
    let extracted = d.join("mat.fbm").join("wood.png");
    assert_eq!(Path::new(&dm.path), extracted, "extracted into <file>.fbm");
    assert_eq!(image::open(&extracted).unwrap().to_rgba8().get_pixel(0, 0).0, [10, 200, 30, 255]);
    assert!((m.bump_map.as_ref().unwrap().amount - 45.0).abs() < 1e-3);
    assert!(m.show_map_in_viewport);
    // Not embedded: the path is written and resolved.
    let mut s2 = AppState::new();
    let b2 = boxed(&mut s2, 0.0);
    let map2 = d.join("stone.png");
    png(&map2, [1, 2, 3]);
    assign(&mut s2, b2, "Stone", &map2);
    let f2 = d.join("plain.fbx");
    export(&mut s2, &f2);
    let raw = std::fs::read(&f2).unwrap();
    assert!(loom_fbx::read(&raw, 0.01).unwrap().materials.iter().all(|m| m.textures.iter().all(|t| t.content.is_none())));
    let mut t2 = AppState::new();
    import(&mut t2, &f2);
    let m2 = t2.scene.materials.values().find(|m| m.name == "Stone").unwrap();
    assert_eq!(Path::new(&m2.diffuse_map.as_ref().unwrap().path), map2);
}

#[test]
fn fbx_import_bones_as_dummies_and_units_override() {
    let d = dir("fbx_bones");
    let rig = concat!(env!("CARGO_MANIFEST_DIR"), "/../loom_fbx/tests/fixtures/sitting_idle.fbx");
    let mut s = AppState::new();
    opt(&mut s, "fbx_import", "bones_as_dummies", json!(true));
    opt(&mut s, "fbx_import", "animation", json!(false));
    execute(&mut s, Command::ImportFile { path: Some(rig.into()) }).unwrap();
    assert!(!s.scene.nodes().any(|n| matches!(n.object, ObjectKind::Bone { .. })));
    assert!(s.scene.nodes().any(|n| matches!(n.object, ObjectKind::Dummy { .. })));
    let skinned = s.scene.nodes().find(|n| n.modifiers.iter().any(|m| matches!(m.modifier, Modifier::Skin(_)))).expect("the skin still binds to the dummies");
    let _ = skinned;
    // Units: the file's numbers read as metres make the box 100 times bigger than centimetres would.
    let mut a = AppState::new();
    boxed(&mut a, 0.0);
    let f = d.join("u.fbx");
    export(&mut a, &f);
    let size = |st: &AppState| {
        let n = st.scene.nodes().next().unwrap().id;
        let (lo, hi) = st.scene.eval_mesh(n).value.mesh.aabb();
        (hi - lo).x
    };
    let mut cm = AppState::new();
    opt(&mut cm, "fbx_import", "units_automatic", json!(false));
    opt(&mut cm, "fbx_import", "units", json!("Centimeters"));
    import(&mut cm, &f);
    let mut m = AppState::new();
    opt(&mut m, "fbx_import", "units_automatic", json!(false));
    opt(&mut m, "fbx_import", "units", json!("Meters"));
    import(&mut m, &f);
    assert!((size(&cm) / size(&m) - 100.0).abs() < 1e-2, "{} vs {}", size(&cm), size(&m));
    let info = execute(&mut m, Command::FbxFileInfo { path: f.to_string_lossy().into_owned() }).unwrap();
    assert_eq!(info["file_units"], "Centimeters");
    assert!((info["scale_factor"].as_f64().unwrap() - 0.01).abs() < 1e-9, "{info}");
    assert_eq!(info["file_axis_direction"], "Y-up");
}

#[test]
fn fbx_export_selection_sets_instances_turbosmooth_and_dummy_bones() {
    let d = dir("fbx_geometry");
    let mut s = AppState::new();
    let a = boxed(&mut s, 0.0);
    execute(&mut s, Command::CloneOptions { kind: loom_commands::CloneKind::Instance, copies: 1, name: None, offset: Some([30.0, 0.0, 0.0]), rotate_deg: None, scale: None }).unwrap();
    let inst = *s.selection.iter().next().unwrap();
    assert_ne!(inst, a);
    let c = boxed(&mut s, 60.0);
    execute(&mut s, Command::AddModifier { name: "TurboSmooth".into() }).unwrap();
    s.named_sets.insert("Pair".into(), vec![a, c]);
    opt(&mut s, "fbx_export", "selection_sets", json!(true));
    opt(&mut s, "fbx_export", "turbosmooth", json!(true));
    let f = d.join("geo.fbx");
    export(&mut s, &f);
    let raw = std::fs::read(&f).unwrap();
    let doc = loom_fbx::read(&raw, 0.01).unwrap();
    assert_eq!(doc.instances.len(), 1, "the instance shares its geometry");
    assert_eq!(doc.selection_sets.len(), 1);
    assert_eq!(doc.subdivision.len(), 1, "the TurboSmooth box goes as a cage");
    let cage = doc.nodes.iter().find(|n| n.name == "Box003").unwrap();
    let loom_fbx::FbxKind::Mesh { mesh, .. } = &cage.kind else { panic!() };
    assert_eq!(mesh.face_count(), 6, "the cage, not the smoothed mesh");
    let mut t = AppState::new();
    import(&mut t, &f);
    let c2 = t.scene.nodes().find(|n| n.name == "Box003").unwrap();
    assert!(c2.modifiers.iter().any(|m| matches!(m.modifier, Modifier::TurboSmooth { .. })), "TurboSmooth comes back");
    let b2 = t.scene.nodes().find(|n| n.name == "Box002").unwrap();
    assert!(b2.instance_of.is_some(), "an instance again");
    assert_eq!(t.named_sets.get("Pair").map(|v| v.len()), Some(2));
    // TurboSmooth off: the smoothed mesh is written.
    opt(&mut s, "fbx_export", "turbosmooth", json!(false));
    export(&mut s, &f);
    let doc = loom_fbx::read(&std::fs::read(&f).unwrap(), 0.01).unwrap();
    let loom_fbx::FbxKind::Mesh { mesh, .. } = &doc.nodes.iter().find(|n| n.name == "Box003").unwrap().kind else { panic!() };
    assert!(mesh.face_count() > 6);
}

#[test]
fn obj_export_options_change_the_text() {
    let d = dir("obj_export");
    let mut s = AppState::new();
    let b = boxed(&mut s, 0.0);
    let map = d.join("tex.png");
    png(&map, [9, 9, 9]);
    assign(&mut s, b, "Painted", &map);
    let f = d.join("o.obj");
    export(&mut s, &f);
    let text = std::fs::read_to_string(&f).unwrap();
    assert!(text.contains("mtllib o.mtl") && text.contains("usemtl Painted"), "{text}");
    let mtl = std::fs::read_to_string(d.join("o.mtl")).unwrap();
    assert!(mtl.contains("newmtl Painted") && mtl.contains("map_Kd tex.png") && mtl.contains("map_bump -bm 0.45 tex.png"), "{mtl}");
    assert!(mtl.contains("Kd 0.784314 0.196078 0.098039"), "{mtl}");
    assert!(mtl.contains("d 0.8"), "{mtl}");
    let faces = |t: &str| t.lines().filter(|l| l.starts_with("f ")).map(|l| l.split_whitespace().count() - 1).collect::<Vec<_>>();
    assert_eq!(faces(&text), vec![4; 6]);
    opt(&mut s, "obj_export", "faces", json!("Triangles"));
    opt(&mut s, "obj_export", "normals", json!(false));
    opt(&mut s, "obj_export", "texture_coordinates", json!(false));
    opt(&mut s, "obj_export", "smoothing_groups", json!(false));
    opt(&mut s, "obj_export", "relative_numbers", json!(true));
    opt(&mut s, "obj_export", "target_windows", json!(true));
    opt(&mut s, "obj_export", "flip_yz", json!(true));
    opt(&mut s, "obj_export", "scale", json!(2.0));
    opt(&mut s, "obj_export", "precision", json!(2));
    opt(&mut s, "obj_export", "use_material", json!(false));
    std::fs::remove_file(d.join("o.mtl")).unwrap();
    export(&mut s, &f);
    let text = std::fs::read_to_string(&f).unwrap();
    assert!(text.contains("\r\n"));
    let text = text.replace("\r\n", "\n");
    assert_eq!(faces(&text), vec![3; 12]);
    assert!(!text.contains("vn ") && !text.contains("vt ") && !text.contains("\ns ") && !text.contains("mtllib"), "{text}");
    assert!(!d.join("o.mtl").exists());
    assert!(text.lines().filter(|l| l.starts_with("f ")).all(|l| l.split_whitespace().skip(1).all(|i| i.starts_with('-'))), "relative indices");
    // The box spans z 0..10; flipped Y-up and scaled 2, y spans 0..20.
    let ys: Vec<f32> = text.lines().filter(|l| l.starts_with("v ")).map(|l| l.split_whitespace().nth(2).unwrap().parse().unwrap()).collect();
    assert!(ys.iter().any(|y| (*y - 20.0).abs() < 1e-3) && ys.iter().all(|y| *y >= -1e-3), "{ys:?}");
    // Quads split an n-gon: a hexagon becomes 2 quads.
    opt(&mut s, "obj_export", "faces", json!("Quads"));
    execute(&mut s, Command::NewScene).unwrap();
    let hex = d.join("hex.obj");
    std::fs::write(&hex, "v 2 0 0\nv 1 1.7 0\nv -1 1.7 0\nv -2 0 0\nv -1 -1.7 0\nv 1 -1.7 0\nf 1 2 3 4 5 6\n").unwrap();
    import(&mut s, &hex);
    export(&mut s, &f);
    let text = std::fs::read_to_string(&f).unwrap().replace("\r\n", "\n");
    assert_eq!(faces(&text), vec![4, 4]);
}

#[test]
fn obj_import_options_objects_materials_and_geometry() {
    let d = dir("obj_import");
    png(&d.join("red.png"), [255, 0, 0]);
    std::fs::write(d.join("two.mtl"), "newmtl Red\nKd 1 0 0\nKs 0.5 0.5 0.5\nNs 100\nd 0.5\nmap_Kd red.png\nbump red.png\n").unwrap();
    // A flat-shaded cube (vn per face, no s lines) and a triangle far away.
    let cube = "mtllib two.mtl\no Cube\nv 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nv 0 0 1\nv 1 0 1\nv 1 1 1\nv 0 1 1\nvn 0 0 -1\nvn 0 0 1\nvn 0 -1 0\nvn 1 0 0\nvn 0 1 0\nvn -1 0 0\nusemtl Red\nf 1//1 4//1 3//1 2//1\nf 5//2 6//2 7//2 8//2\nf 1//3 2//3 6//3 5//3\nf 2//4 3//4 7//4 6//4\nf 3//5 4//5 8//5 7//5\nf 4//6 1//6 5//6 8//6\no Tri\nv 10 0 0\nv 11 0 0\nv 10 0 1\nf 9 10 11\n";
    let f = d.join("two.obj");
    std::fs::write(&f, cube).unwrap();
    let info = execute(&mut AppState::new(), Command::ObjFileInfo { path: f.to_string_lossy().into_owned() }).unwrap();
    assert_eq!(info["objects"], json!([{ "name": "Cube", "faces": 6 }, { "name": "Tri", "faces": 1 }]));
    assert_eq!((info["normals"].clone(), info["texture_coordinates"].clone()), (json!(true), json!(false)));
    let mut s = AppState::new();
    let existing = boxed(&mut s, 0.0);
    opt(&mut s, "obj_import", "objects", json!(["Cube"]));
    opt(&mut s, "obj_import", "prefix", json!("imp_"));
    opt(&mut s, "obj_import", "center_pivots", json!(true));
    import(&mut s, &f);
    let names: Vec<String> = s.scene.nodes().map(|n| n.name.clone()).collect();
    assert_eq!(names, vec!["Box001".to_string(), "imp_Cube".to_string()], "only the checked object, prefixed");
    let c = s.scene.nodes().find(|n| n.name == "imp_Cube").unwrap();
    assert!((c.transform.position.get() - glam::Vec3::splat(0.5)).length() < 1e-5, "pivot at the centre");
    let mesh = s.scene.eval_mesh(c.id).value.mesh.clone();
    // Import from file: the per-face normals make every edge hard.
    let groups: Vec<u32> = mesh.faces().iter().map(|f| f.smoothing_groups).collect();
    let ids = |f: usize| mesh.face_vertex_ids(loom_mesh::FaceId(f as u32)).unwrap();
    for a in 0..groups.len() {
        for b in a + 1..groups.len() {
            if ids(a).iter().any(|v| ids(b).contains(v)) {
                assert_eq!(groups[a] & groups[b], 0, "neighbouring flat faces share no group: {groups:?}");
            }
        }
    }
    let m = s.scene.node_material(c.id).expect("the MTL material is assigned").clone();
    assert_eq!(m.name, "Red");
    assert!((m.opacity.get() - 50.0).abs() < 1e-3 && (m.specular_level.get() - 50.0).abs() < 1e-3);
    assert_eq!(Path::new(&m.diffuse_map.as_ref().unwrap().path), d.join("red.png"));
    assert!((m.bump_map.as_ref().unwrap().amount - 30.0).abs() < 1e-3, "Default bump");
    assert!(s.scene.material_editor.slots.iter().flatten().any(|id| s.scene.materials[id].name == "Red"), "Import into Mat-Editor");
    // Faceted / From SM group / single mesh / flip ZY / reset scene.
    opt(&mut s, "obj_import", "objects", Value::Null);
    opt(&mut s, "obj_import", "prefix", json!(""));
    opt(&mut s, "obj_import", "center_pivots", json!(false));
    opt(&mut s, "obj_import", "normals", json!("From SM group"));
    opt(&mut s, "obj_import", "single_mesh", json!(true));
    opt(&mut s, "obj_import", "flip_zy", json!(true));
    opt(&mut s, "obj_import", "reset_scene", json!(true));
    let undo = s.undo.len();
    import(&mut s, &f);
    assert_eq!(s.undo.len(), undo + 1, "one undo step");
    assert!(s.scene.node(existing).is_none(), "Reset scene");
    assert_eq!(s.scene.len(), 1, "a single mesh");
    let n = s.scene.nodes().next().unwrap();
    assert_eq!(n.name, "two");
    let mesh = s.scene.eval_mesh(n.id).value.mesh.clone();
    assert!(mesh.faces().iter().all(|f| f.smoothing_groups == 1), "no s lines: group 1");
    // The triangle at file z 0..1 (Y-up depth) lies along -Y after Flip ZY.
    let (lo, _) = mesh.aabb();
    assert!((lo.y + 1.0).abs() < 1e-5, "{lo}");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(s.scene.node(existing).is_some(), "undo brings the scene back");
}

#[test]
fn obj_export_import_round_trip_keeps_materials() {
    let d = dir("obj_round");
    let mut s = AppState::new();
    let b = boxed(&mut s, 0.0);
    let map = d.join("t.png");
    png(&map, [1, 1, 1]);
    assign(&mut s, b, "Coat", &map);
    let f = d.join("r.obj");
    export(&mut s, &f);
    let mut t = AppState::new();
    import(&mut t, &f);
    let n = t.scene.nodes().next().unwrap();
    let m = t.scene.node_material(n.id).unwrap();
    let orig = &s.scene.materials.values().next().unwrap();
    assert!((m.diffuse.get() - orig.diffuse.get()).length() < 1e-4);
    assert!((m.glossiness.get() - orig.glossiness.get()).abs() < 1e-2 && (m.opacity.get() - orig.opacity.get()).abs() < 1e-3);
    assert_eq!(Path::new(&m.diffuse_map.as_ref().unwrap().path), map);
}

#[test]
fn obj_hidden_objects_option() {
    let d = dir("obj_hidden");
    let mut s = AppState::new();
    let a = boxed(&mut s, 0.0);
    boxed(&mut s, 40.0);
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::HideSelection).unwrap();
    let f = d.join("h.obj");
    export(&mut s, &f);
    assert!(!std::fs::read_to_string(&f).unwrap().contains("o Box001"));
    opt(&mut s, "obj_export", "hidden_objects", json!(true));
    export(&mut s, &f);
    assert!(std::fs::read_to_string(&f).unwrap().contains("o Box001"));
}

#[test]
fn fbx_lights_option_both_ways() {
    let d = dir("fbx_lights");
    let mut s = AppState::new();
    boxed(&mut s, 0.0);
    execute(&mut s, Command::CreateLight { kind: loom_commands::light::LightKind::Omni, position: [0.0, 0.0, 50.0], target: [0.0; 3] }).unwrap();
    let f = d.join("l.fbx");
    let lights = |path: &Path| loom_fbx::read(&std::fs::read(path).unwrap(), 0.01).unwrap().nodes.iter().filter(|n| matches!(n.kind, loom_fbx::FbxKind::Light(_))).count();
    export(&mut s, &f);
    assert_eq!(lights(&f), 1);
    let mut t = AppState::new();
    opt(&mut t, "fbx_import", "lights", json!(false));
    import(&mut t, &f);
    assert!(!t.scene.nodes().any(|n| matches!(n.object, ObjectKind::Light(_))), "Lights off on import");
    opt(&mut s, "fbx_export", "lights", json!(false));
    export(&mut s, &f);
    assert_eq!(lights(&f), 0, "Lights off on export");
}

#[test]
fn render_output_saves_every_listed_image_format() {
    let dir = dir("image_formats");
    let mut img = image::RgbaImage::new(8, 4);
    for (x, y, p) in img.enumerate_pixels_mut() {
        *p = image::Rgba([(x * 30) as u8, (y * 60) as u8, 128, 255]);
    }
    for ext in loom_commands::render::IMAGE_FORMATS {
        let path = dir.join(format!("t.{ext}"));
        loom_commands::render::save_image(&img, &path.to_string_lossy()).unwrap_or_else(|e| panic!("{ext}: {e}"));
        let back = image::open(&path).unwrap_or_else(|e| panic!("{ext}: {e}"));
        assert_eq!((back.width(), back.height()), (8, 4), "{ext}");
    }
    // The read-only formats are in the bitmap list, and every listed format decodes with this build.
    for ext in loom_commands::render::IMAGE_READ_FORMATS {
        assert!(image::ImageFormat::from_extension(ext).is_some_and(|f| f.reading_enabled()), "{ext}");
    }
}
