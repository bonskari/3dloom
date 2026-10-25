//! File > Import of .max scenes. Reads a .max file from a local asset folder
//! (not in the repository) named by the `LOOM_TEST_ASSETS` environment
//! variable; skips when it is unset or the file is missing.

use loom_commands::{execute, AppState, Command};
use loom_scene::{NodeId, ObjectKind};

#[test]
fn import_max_keeps_world_placement_and_undoes() {
    let Some(assets) = std::env::var_os("LOOM_TEST_ASSETS") else {
        eprintln!("skipped: LOOM_TEST_ASSETS is not set");
        return;
    };
    let truck = std::path::Path::new(&assets).join("Models/Truck/Turck.max");
    if !truck.exists() {
        eprintln!("skipped: {} is not available", truck.display());
        return;
    }
    let mut s = AppState::new();
    let r = execute(&mut s, Command::ImportFile { path: Some(truck.to_string_lossy().into_owned()) }).unwrap();
    let ids: Vec<NodeId> = r["nodes"].as_array().unwrap().iter().map(|v| NodeId(v.as_u64().unwrap())).collect();
    let doc = loom_max::read(&std::fs::read(&truck).unwrap()).unwrap();
    assert_eq!(ids.len(), doc.nodes.len());
    // Every node's world and object matrices are the file's (parent chain × local, × pivot offset).
    let world = |i: usize| {
        let mut m = doc.nodes[i].local;
        let mut p = doc.nodes[i].parent;
        while let Some(pi) = p {
            m = doc.nodes[pi].local * m;
            p = doc.nodes[pi].parent;
        }
        m
    };
    for (i, id) in ids.iter().enumerate() {
        let want = world(i) * doc.nodes[i].offset;
        let got = s.scene.object_matrix(*id);
        for (a, b) in got.to_cols_array().iter().zip(want.to_cols_array()) {
            assert!((a - b).abs() < 1e-2 * (1.0 + b.abs()), "{}: {got} vs {want}", doc.nodes[i].name);
        }
        assert_eq!(s.scene.node(*id).unwrap().name.is_empty(), false);
    }
    // What was left out is reported for the Import Summary window.
    assert!(s.import_report.iter().any(|w| w.contains("Edit Mesh is not imported")), "{:?}", s.import_report);
    // Parametric primitives stay parametric.
    assert!(ids.iter().any(|id| matches!(s.scene.node(*id).unwrap().object, loom_scene::ObjectKind::Cylinder { .. })));
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.nodes().count(), 0, "one undo step");
}

fn asset(rel: &str) -> Option<std::path::PathBuf> {
    let Some(assets) = std::env::var_os("LOOM_TEST_ASSETS") else {
        eprintln!("skipped: LOOM_TEST_ASSETS is not set");
        return None;
    };
    let p = std::path::Path::new(&assets).join("Models").join(rel);
    if !p.exists() {
        eprintln!("skipped: {} is not available", p.display());
        return None;
    }
    Some(p)
}

fn import(s: &mut AppState, p: &std::path::Path) -> Vec<NodeId> {
    let r = execute(s, Command::ImportFile { path: Some(p.to_string_lossy().into_owned()) }).unwrap();
    r["nodes"].as_array().unwrap().iter().map(|v| NodeId(v.as_u64().unwrap())).collect()
}

fn by_name(s: &AppState, name: &str) -> NodeId {
    s.scene.nodes().find(|n| n.name == name).unwrap_or_else(|| panic!("no {name}")).id
}

fn near(a: glam::Vec3, b: glam::Vec3, tol: f32) -> bool {
    (a - b).abs().max_element() < tol
}

#[test]
fn imported_stacks_evaluate_to_the_bounds_the_file_stored() {
    let Some(p) = asset("Truck/Turck.max") else { return };
    let mut s = AppState::new();
    import(&mut s, &p);
    // Cylinder29: a 50-tall cylinder under a 90° Bend limited to 0..20.98. The file kept the
    // bounds of the Bend's result (the input of the Edit Mesh above it).
    let id = by_name(&s, "Cylinder29");
    let n = s.scene.node(id).unwrap();
    assert_eq!(n.modifiers.iter().map(|m| m.modifier.name()).collect::<Vec<_>>(), ["Bend"]);
    let (lo, hi) = s.scene.evaluated(id, Some(0)).value.mesh.aabb();
    assert!(near(lo, glam::Vec3::new(-3.2, -3.2, 0.0), 0.05) && near(hi, glam::Vec3::new(42.38, 3.2, 16.56), 0.05), "{lo} {hi}");
    // A Lathe whose axis was 0.59 off the line's pivot: the solid sits around that axis in the world.
    let lathed: Vec<NodeId> = s.scene.nodes().filter(|n| n.modifiers.len() == 1 && n.modifiers[0].modifier.name() == "Lathe").map(|n| n.id).collect();
    assert!(lathed.len() >= 5);
    let id = lathed[0];
    let m = s.scene.evaluated(id, None).value.mesh.clone();
    assert!(m.face_count() > 0);
    let (lo, hi) = m.aabb();
    // Revolved about its Direction axis through the (moved) pivot: symmetric across it.
    let loom_scene::Modifier::Lathe { direction, .. } = &s.scene.node(id).unwrap().modifiers[0].modifier else { panic!() };
    for k in (0..3).filter(|k| *k != direction.get() as usize) {
        assert!((lo[k] + hi[k]).abs() < 0.02, "axis {}: {lo} {hi}", direction.get());
    }
    let doc = loom_max::read(&std::fs::read(&p).unwrap()).unwrap();
    let file_node = doc.nodes.iter().find(|n| n.name == s.scene.node(id).unwrap().name).unwrap();
    let axis_world = s.scene.object_matrix(id).transform_point3(glam::Vec3::ZERO);
    let want = file_node.local * file_node.offset;
    assert!(near(axis_world, want.transform_point3(glam::Vec3::ZERO), 1e-3));
    // Layers by name: the file's "0" is the scene's layer 0.
    assert!(s.scene.nodes().all(|n| n.layer == 0));
}

#[test]
fn imported_ffd_and_symmetry_match_the_files_bounds() {
    let Some(p) = asset("Monster/Monster_HEad.max") else { return };
    let mut s = AppState::new();
    import(&mut s, &p);
    let id = by_name(&s, "Group01");
    let names: Vec<String> = s.scene.node(id).unwrap().modifiers.iter().map(|m| m.modifier.name().to_owned()).collect();
    assert_eq!(names, ["Symmetry", "FFD 4x4x4", "TurboSmooth"]);
    // The file's bounds are in modifier context space: object space less the pivot offset (0, -0.39, -78.91).
    let ctx = glam::Vec3::new(0.0, -0.39, -78.91);
    let (lo, hi) = s.scene.evaluated(id, Some(0)).value.mesh.aabb();
    assert!(near(lo + ctx, glam::Vec3::new(-6.89, -14.38, -10.85), 0.05) && near(hi + ctx, glam::Vec3::new(6.89, 9.5, 14.64), 0.05), "Symmetry: {} {}", lo + ctx, hi + ctx);
    let (lo, hi) = s.scene.evaluated(id, Some(1)).value.mesh.aabb();
    assert!(near(lo + ctx, glam::Vec3::new(-6.89, -12.52, -10.86), 0.1) && near(hi + ctx, glam::Vec3::new(6.89, 8.98, 11.04), 0.1), "FFD: {} {}", lo + ctx, hi + ctx);
}

#[test]
fn imported_materials_lights_and_unwrap() {
    let Some(p) = asset("Cone/TrafficCone.max") else { return };
    let mut s = AppState::new();
    import(&mut s, &p);
    let n = s.scene.node(by_name(&s, "Box001")).unwrap();
    let multi = &s.scene.materials[&n.material.unwrap()];
    let loom_scene::material::MaterialKind::MultiSub(subs) = &multi.kind else { panic!("{:?}", multi.kind) };
    assert_eq!(subs.iter().map(|x| x.id).collect::<Vec<_>>(), [1, 2]);
    let red = &s.scene.materials[&subs[0].material.unwrap()];
    assert_eq!(red.name, "_traffic_conecone_red");
    assert!(near(red.diffuse.get(), glam::Vec3::new(0.992, 0.259, 0.008), 2e-3));
    // One undo takes the materials away with the nodes.
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(s.scene.materials.values().all(|m| m.name != "_traffic_conecone_red"));

    if let Some(p) = asset("Monster/Monster_HEad.max") {
        let mut s = AppState::new();
        import(&mut s, &p);
        let (d, t) = (by_name(&s, "Direct02"), by_name(&s, "Direct02.Target"));
        let ObjectKind::Light(l) = &s.scene.node(d).unwrap().object else { panic!() };
        assert_eq!((l.light_type, l.target), (loom_scene::light::LightType::Directional, Some(t)));
        assert!((l.hotspot.get() - 109.22).abs() < 1e-3);
        // Aimed at the target: its -Z points there.
        let w = s.scene.world_matrix(d);
        let to = (s.scene.world_matrix(t).w_axis - w.w_axis).truncate().normalize();
        assert!((w.transform_vector3(-glam::Vec3::Z).normalize() - to).length() < 1e-3);
        let ObjectKind::Camera { fov, .. } = &s.scene.node(by_name(&s, "Camera01")).unwrap().object else { panic!() };
        assert!((fov.get() - 36.0).abs() < 1e-3);
    }

    let Some(p) = asset("Core_Lightning/Unit89.max") else { return };
    let mut s = AppState::new();
    import(&mut s, &p);
    assert!(!s.import_report.iter().any(|w| w.contains("Unwrap")), "{:?}", s.import_report);
    let id = by_name(&s, "unit89_body1");
    let mesh = s.scene.evaluated(id, None).value.mesh.clone();
    let doc = loom_max::read(&std::fs::read(&p).unwrap()).unwrap();
    let uvs = doc.nodes.iter().find(|n| n.name == "unit89_body1").unwrap().modifiers[0].uvs.clone().unwrap();
    let map = mesh.map(1).unwrap();
    // Polygon 0 (vertices 5, 4, 12) carries the file's texture vertices 198, 28, 142.
    let corners = mesh.face_corners(loom_mesh::FaceId(0)).unwrap();
    let got: Vec<glam::Vec3> = corners.iter().map(|h| map.verts[map.corners[h.0 as usize] as usize]).collect();
    let verts: Vec<u32> = corners.iter().map(|h| mesh.half_edges()[h.0 as usize].origin.0).collect();
    let k = verts.iter().position(|v| *v == 5).unwrap();
    for (j, want) in [198usize, 28, 142].iter().enumerate() {
        assert!(near(got[(k + j) % 3], uvs.verts[*want], 1e-6), "{got:?}");
    }
}

#[test]
fn merge_reads_max_files() {
    let Some(p) = asset("Monster/Monster_HEad.max") else { return };
    let mut s = AppState::new();
    let r = execute(&mut s, Command::MergeFile { path: Some(p.to_string_lossy().into_owned()), names: Some(vec!["Direct02".into(), "Direct02.Target".into()]), duplicates: None }).unwrap();
    assert_eq!(r["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(s.scene.nodes().count(), 2);
    let (d, t) = (by_name(&s, "Direct02"), by_name(&s, "Direct02.Target"));
    let ObjectKind::Light(l) = &s.scene.node(d).unwrap().object else { panic!() };
    assert_eq!(l.target, Some(t));
    // A light left without its target keeps none.
    let mut s = AppState::new();
    execute(&mut s, Command::MergeFile { path: Some(p.to_string_lossy().into_owned()), names: Some(vec!["Direct02".into()]), duplicates: None }).unwrap();
    let ObjectKind::Light(l) = &s.scene.node(by_name(&s, "Direct02")).unwrap().object else { panic!() };
    assert_eq!(l.target, None);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.nodes().count(), 0);
}
