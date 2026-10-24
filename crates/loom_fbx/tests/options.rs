//! The FBX Exporter options in the writer (file type and version, units,
//! triangulate, split normals, tangents, materials and textures, embedded
//! media, instances, selection sets, subdivision, curve filters) and what
//! the reader brings back, checked against ufbx.

use glam::Vec3;
use loom_fbx::{CurveFilters, FbxAnim, FbxDocument, FbxKind, FbxMaterial, FbxNode, FbxTake, FbxTexture, ReadOpts, WriteOpts};

fn box_node(name: &str) -> FbxNode {
    let mut mesh = loom_mesh::box_segmented(10.0, 10.0, 10.0, [1, 1, 1]);
    for f in mesh.faces_mut() {
        f.smoothing_groups = 0;
    }
    FbxNode::new(name, FbxKind::Mesh { mesh, materials: vec!["Paint".into()] })
}

fn doc(nodes: Vec<FbxNode>) -> FbxDocument {
    FbxDocument { nodes, unit_metres: 0.01, ..Default::default() }
}

fn mesh_of(d: &FbxDocument, i: usize) -> &loom_mesh::Mesh {
    match &d.nodes[i].kind {
        FbxKind::Mesh { mesh, .. } => mesh,
        k => panic!("{k:?}"),
    }
}

fn plain(bytes: &[u8]) -> ufbx::SceneRoot {
    ufbx::load_memory(bytes, ufbx::LoadOpts { strict: true, ..Default::default() }).unwrap()
}

#[test]
fn ascii_and_version_7500_files_read_back_the_same() {
    let d = doc(vec![box_node("Box")]);
    for (ascii, version) in [(false, 7400), (false, 7500), (true, 7400), (true, 7500)] {
        let bytes = loom_fbx::write(&d, &WriteOpts { ascii, version, ..Default::default() });
        assert_eq!(bytes.starts_with(b"Kaydara FBX Binary"), !ascii, "{ascii} {version}");
        let info = loom_fbx::info(&bytes).unwrap();
        assert_eq!((info.ascii, info.version), (ascii, version));
        let back = loom_fbx::read(&bytes, 0.01).unwrap();
        let m = mesh_of(&back, 0);
        assert_eq!((m.vertex_count(), m.face_count()), (8, 6), "{ascii} {version}");
        assert!(m.vertices().iter().any(|v| (v.position - Vec3::new(5.0, 5.0, 10.0)).length() < 1e-4), "{ascii} {version}");
        plain(&bytes);
    }
}

#[test]
fn info_reports_axis_units_creator_and_counts() {
    let mut cam = FbxNode::new("Cam", FbxKind::Camera { fov_deg: 50.0 });
    cam.translation = Vec3::new(0.0, -10.0, 0.0);
    let bytes = loom_fbx::write(&doc(vec![box_node("Box"), cam]), &WriteOpts { axes: loom_fbx::WriteAxes::YUp, file_unit_cm: 2.54, ..Default::default() });
    let i = loom_fbx::info(&bytes).unwrap();
    assert_eq!(i.up_axis, "Y-up");
    assert!((i.unit_metres - 0.0254).abs() < 1e-9, "{}", i.unit_metres);
    assert!(i.creator.starts_with("3dloom"), "{}", i.creator);
    assert_eq!(i.creation_time, "1970-01-01 10:00:00:000");
    assert_eq!((i.meshes, i.cameras, i.materials), (1, 1, 1));
}

#[test]
fn file_units_scale_the_numbers_not_the_size() {
    let d = doc(vec![box_node("Box")]);
    // Inches: the numbers shrink by 2.54, the scale factor says so, and the box reads back the same size.
    let bytes = loom_fbx::write(&d, &WriteOpts { file_unit_cm: 2.54, ..Default::default() });
    let raw = ufbx::load_memory(&bytes, ufbx::LoadOpts::default()).unwrap();
    let xmax = raw.meshes[0].vertices.iter().map(|v| v.x).fold(f64::MIN, f64::max);
    assert!((xmax - 5.0 / 2.54).abs() < 1e-4, "{xmax}");
    let back = loom_fbx::read(&bytes, 0.01).unwrap();
    let (lo, hi) = mesh_of(&back, 0).aabb();
    assert!(((hi - lo) - Vec3::splat(10.0)).length() < 1e-3, "{lo} {hi}");
}

#[test]
fn triangulate_split_normals_and_tangents() {
    let d = doc(vec![box_node("Box")]);
    let tri = loom_fbx::read(&loom_fbx::write(&d, &WriteOpts { triangulate: true, ..Default::default() }), 0.01).unwrap();
    let m = mesh_of(&tri, 0);
    assert_eq!(m.face_count(), 12);
    assert!((0..12).all(|f| m.face_vertex_ids(loom_mesh::FaceId(f)).unwrap().len() == 3));
    // Flat faces: every corner of the box has its own normal, so each face gets its own 4 points.
    let split = loom_fbx::write(&d, &WriteOpts { split_normals: true, ..Default::default() });
    assert_eq!(plain(&split).meshes[0].num_vertices, 24);
    assert_eq!(plain(&loom_fbx::write(&d, &WriteOpts::default())).meshes[0].num_vertices, 8);
    // Tangents and binormals from the box's map channel 1, orthogonal to the normals.
    let t = plain(&loom_fbx::write(&d, &WriteOpts { tangents: true, ..Default::default() }));
    let m = &t.meshes[0];
    assert!(m.vertex_tangent.exists && m.vertex_bitangent.exists);
    for c in 0..m.num_indices {
        let n = m.vertex_normal.values[m.vertex_normal.indices[c] as usize];
        let tg = m.vertex_tangent.values[m.vertex_tangent.indices[c] as usize];
        let b = m.vertex_bitangent.values[m.vertex_bitangent.indices[c] as usize];
        let dot = |a: ufbx::Vec3, b: ufbx::Vec3| a.x * b.x + a.y * b.y + a.z * b.z;
        assert!(dot(n, tg).abs() < 1e-4 && dot(n, b).abs() < 1e-4 && dot(tg, b).abs() < 1e-4, "corner {c}");
        assert!((dot(tg, tg) - 1.0).abs() < 1e-4);
    }
    assert!(!plain(&loom_fbx::write(&d, &WriteOpts::default())).meshes[0].vertex_tangent.exists);
}

#[test]
fn split_points_keep_skin_weights() {
    let bone = FbxNode::new("Bone", FbxKind::Bone { length: 10.0, size: 1.0 });
    let mut b = box_node("Box");
    let FbxKind::Mesh { mesh, .. } = &b.kind else { unreachable!() };
    let n = mesh.vertex_count();
    b.skin = Some(loom_fbx::FbxSkin { bones: vec![(0, glam::Mat4::IDENTITY)], mesh_bind: glam::Mat4::IDENTITY, weights: vec![vec![(0, 1.0)]; n] });
    let bytes = loom_fbx::write(&doc(vec![bone, b]), &WriteOpts { split_normals: true, ..Default::default() });
    let s = plain(&bytes);
    assert_eq!(s.skin_clusters[0].vertices.len(), 24, "every split point is weighted");
}

#[test]
fn materials_and_textures_both_ways_with_embedded_media() {
    let png = {
        let mut out = std::io::Cursor::new(Vec::new());
        image_png(&mut out);
        out.into_inner()
    };
    let mut m = FbxMaterial::new("Paint");
    m.diffuse = [0.6, 0.2, 0.1];
    m.ambient = [0.1, 0.1, 0.1];
    m.specular = [0.9, 0.9, 0.9];
    m.specular_factor = 0.5;
    m.shininess = 32.0;
    m.opacity = 0.75;
    m.emissive_factor = 0.25;
    m.textures = vec![
        FbxTexture { property: "DiffuseColor".into(), path: "/maps/wood.png".into(), relative: String::new(), amount: 1.0, content: Some(png.clone()) },
        FbxTexture { property: "Bump".into(), path: "/maps/bumps.png".into(), relative: String::new(), amount: 0.3, content: None },
    ];
    let mut d = doc(vec![box_node("Box")]);
    d.materials = vec![m.clone()];
    for embed in [false, true] {
        let bytes = loom_fbx::write(&d, &WriteOpts { embed_media: embed, ..Default::default() });
        let back = loom_fbx::read(&bytes, 0.01).unwrap();
        let r = back.materials.iter().find(|x| x.name == "Paint").expect("the material");
        for (a, b) in [(r.diffuse, m.diffuse), (r.ambient, m.ambient), (r.specular, m.specular)] {
            assert!(Vec3::from(a).distance(Vec3::from(b)) < 1e-5, "{a:?} {b:?}");
        }
        assert!((r.specular_factor - 0.5).abs() < 1e-5 && (r.shininess - 32.0).abs() < 1e-4 && (r.opacity - 0.75).abs() < 1e-5, "{r:?}");
        assert!((r.emissive_factor - 0.25).abs() < 1e-5, "{}", r.emissive_factor);
        let diffuse = r.textures.iter().find(|t| t.property == "DiffuseColor").expect("diffuse map");
        assert!(diffuse.path.ends_with("wood.png"), "{}", diffuse.path);
        assert_eq!(diffuse.content.as_deref(), embed.then_some(png.as_slice()), "embedded only with Embed Media");
        let bump = r.textures.iter().find(|t| t.property == "Bump").expect("bump map");
        assert!((bump.amount - 0.3).abs() < 1e-5);
        assert!(bump.content.is_none(), "a missing file has nothing to embed");
    }
}

/// A 2×2 PNG, written by hand (no image crate here): IHDR, IDAT, IEND.
fn image_png(out: &mut std::io::Cursor<Vec<u8>>) {
    use std::io::Write;
    fn crc(data: &[u8]) -> u32 {
        let mut c = 0xffff_ffffu32;
        for b in data {
            c ^= *b as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }
    let chunk = |out: &mut std::io::Cursor<Vec<u8>>, kind: &[u8], data: &[u8]| {
        out.write_all(&(data.len() as u32).to_be_bytes()).unwrap();
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.write_all(&body).unwrap();
        out.write_all(&crc(&body).to_be_bytes()).unwrap();
    };
    out.write_all(b"\x89PNG\r\n\x1a\n").unwrap();
    chunk(out, b"IHDR", &[0, 0, 0, 2, 0, 0, 0, 2, 8, 2, 0, 0, 0]);
    let raw = [0u8, 255, 0, 0, 0, 255, 0, 0, 0, 0, 255, 255, 255, 255];
    chunk(out, b"IDAT", &miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6));
    chunk(out, b"IEND", &[]);
}

#[test]
fn instances_selection_sets_and_subdivision_round_trip() {
    let mut d = doc(vec![box_node("A"), box_node("B"), box_node("C")]);
    d.nodes[1].translation = Vec3::X * 30.0;
    d.instances = vec![(1, 0)];
    d.selection_sets = vec![("Pair".into(), vec![0, 2])];
    d.subdivision = vec![(2, 2, 3)];
    let bytes = loom_fbx::write(&d, &WriteOpts::default());
    let s = plain(&bytes);
    assert_eq!(s.meshes.len(), 2, "A and B share one geometry");
    let back = loom_fbx::read(&bytes, 0.01).unwrap();
    assert_eq!(back.instances, vec![(1, 0)]);
    assert_eq!(back.selection_sets, vec![("Pair".to_string(), vec![0, 2])]);
    assert_eq!(back.subdivision, vec![(2, 2, 3)]);
}

#[test]
fn curve_filters_and_remove_single_key() {
    // X moves linearly then holds; Y and Z hold.
    let frames: Vec<f32> = (0..=20).map(|f| f as f32).collect();
    let tr: Vec<Vec3> = frames.iter().map(|f| Vec3::new(f.min(10.0), 0.0, 0.0)).collect();
    let anim = FbxAnim { frames: frames.clone(), translation: tr, rotation_deg: vec![Vec3::ZERO; 21], scale: vec![Vec3::ONE; 21] };
    let mut d = doc(vec![box_node("Box")]);
    d.anim = Some(FbxTake { name: "Take 001".into(), fps: 30.0, start: 0, end: 20, tracks: vec![(0, anim)] });
    let key_counts = |filters: CurveFilters| -> Vec<usize> {
        let s = plain(&loom_fbx::write(&d, &WriteOpts { curve_filters: filters, ..Default::default() }));
        let mut v: Vec<usize> = s.anim_curves.iter().map(|c| c.keyframes.len()).collect();
        v.sort_unstable();
        v
    };
    assert!(key_counts(CurveFilters::default()).iter().all(|n| *n == 21), "baked: a key per frame");
    let ckr = key_counts(CurveFilters { constant: Some([1e-4, 1e-3, 1e-3, 1e-3]), ..Default::default() });
    assert_eq!(ckr.iter().filter(|n| **n == 1).count(), 8, "flat curves keep one key: {ckr:?}");
    assert_eq!(*ckr.last().unwrap(), 12, "X keeps its ramp and the hold's end: {ckr:?}");
    let kr = key_counts(CurveFilters { key_reducer: Some((1e-3, false)), ..Default::default() });
    assert_eq!(*kr.last().unwrap(), 3, "X: start, corner, end: {kr:?}");
    let synced = key_counts(CurveFilters { key_reducer: Some((1e-3, true)), ..Default::default() });
    assert!(synced[..3].iter().all(|n| *n == 3), "XYZ synchronization: translation curves share X's keys: {synced:?}");
    let single = key_counts(CurveFilters { constant: Some([1e-4, 1e-3, 1e-3, 1e-3]), remove_single_key: true, ..Default::default() });
    assert_eq!(single, vec![12], "only X is written: {single:?}");
    // The animation still reads back the same.
    let back = loom_fbx::read(&loom_fbx::write(&d, &WriteOpts { curve_filters: CurveFilters { key_reducer: Some((1e-3, false)), remove_single_key: true, ..Default::default() }, ..Default::default() }), 0.01).unwrap();
    let (_, a) = &back.anim.as_ref().unwrap().tracks[0];
    for (f, t) in a.frames.iter().zip(&a.translation) {
        assert!((t.x - f.min(10.0)).abs() < 1e-3, "frame {f}: {t}");
    }
}

#[test]
fn reader_options_take_layers_skins_and_smoothing() {
    let rig = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sitting_idle.fbx")).unwrap();
    let mut ro = ReadOpts::new(0.01);
    ro.skins = false;
    ro.animation = false;
    let d = loom_fbx::read_with(&rig, &ro).unwrap();
    assert!(d.anim.is_none());
    assert!(d.nodes.iter().all(|n| n.skin.is_none()));
    ro.animation = true;
    ro.take = Some("no such take".into());
    assert!(loom_fbx::read_with(&rig, &ro).unwrap().anim.is_none());
    let first = loom_fbx::info(&rig).unwrap().takes[0].clone();
    ro.take = Some(first.clone());
    assert_eq!(loom_fbx::read_with(&rig, &ro).unwrap().anim.unwrap().name, first);
    // Smoothing Groups off: groups come from the normals, not the file's bits.
    let mut b = box_node("Box");
    let FbxKind::Mesh { mesh, .. } = &mut b.kind else { unreachable!() };
    for f in mesh.faces_mut() {
        f.smoothing_groups = 1;
    }
    let bytes = loom_fbx::write(&doc(vec![b]), &WriteOpts::default());
    let mut ro = ReadOpts::new(0.01);
    assert!(mesh_of(&loom_fbx::read_with(&bytes, &ro).unwrap(), 0).faces().iter().all(|f| f.smoothing_groups == 1));
    ro.smoothing_groups = false;
    let groups: Vec<u32> = mesh_of(&loom_fbx::read_with(&bytes, &ro).unwrap(), 0).faces().iter().map(|f| f.smoothing_groups).collect();
    assert!(groups.iter().all(|g| *g != 0), "the shared normals keep one smooth region: {groups:?}");
}
