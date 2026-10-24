//! Phase 9 subdivision and caches: HSDS, Subdivide (WSM), Point Cache and
//! Point Cache (WSM) with .pc2 files.

use glam::Vec3;
use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp};
use loom_scene::NodeId;
use std::path::PathBuf;

fn boxed(s: &mut AppState, size: f32) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [size; 3] }).unwrap();
    *s.selection.iter().next().unwrap()
}

fn add(s: &mut AppState, name: &str) {
    execute(s, Command::AddModifier { name: name.into() }).unwrap();
}

fn set(s: &mut AppState, index: u32, name: &str, value: f64) {
    execute(s, Command::SetModifierParameter { index, name: name.into(), value }).unwrap();
}

fn faces(s: &AppState, id: NodeId) -> usize {
    s.scene.eval_mesh(id).value.mesh.face_count()
}

fn verts(s: &AppState, id: NodeId) -> Vec<Vec3> {
    s.scene.eval_mesh(id).value.mesh.vertices().iter().map(|v| v.position).collect()
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("loom-caches9-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.join(name)
}

#[test]
fn hsds_subdivides_selected_faces() {
    let mut s = AppState::new();
    let b = boxed(&mut s, 10.0);
    add(&mut s, "HSDS");
    execute(&mut s, Command::ModifierSubobjectLevel { index: 0, level: 3 }).unwrap();
    execute(&mut s, Command::SelectSub { node: None, ids: vec![0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::HsdsSubdivide { faces: None }).unwrap();
    assert_eq!(faces(&s, b), 6 - 1 + 4, "one face into four, the neighbours pick up the new edge points");
    // A second level from the finer mesh: a face of the first level splits again.
    let m = s.scene.eval_mesh(b).value.mesh.clone();
    let quad = (0..m.face_count() as u32).find(|f| m.face_corners(loom_mesh::FaceId(*f)).unwrap().len() == 4).unwrap();
    execute(&mut s, Command::HsdsSubdivide { faces: Some(vec![quad]) }).unwrap();
    assert_eq!(faces(&s, b), 9 - 1 + 4, "a quad of level 1 into four");
    match &s.scene.node(b).unwrap().modifiers[0].modifier {
        loom_scene::Modifier::Hsds(d) => assert_eq!(d.levels.len(), 2),
        other => panic!("{other:?}"),
    }
    // Smooth Result rounds the subdivided region (here every face, so no crease edge holds it).
    let mut u = AppState::new();
    let d = boxed(&mut u, 10.0);
    add(&mut u, "HSDS");
    execute(&mut u, Command::HsdsSubdivide { faces: Some((0..6).collect()) }).unwrap();
    let linear = verts(&u, d);
    set(&mut u, 0, "Smooth Result", 1.0);
    let smooth = verts(&u, d);
    assert_eq!(smooth.len(), linear.len());
    assert!(smooth.iter().zip(&linear).any(|(a, c)| a.distance(*c) > 0.1), "smoothed");
    // Adaptive Subdivision: faces with a sharp edge split until the level limit.
    let mut t = AppState::new();
    let c = boxed(&mut t, 10.0);
    add(&mut t, "HSDS");
    execute(&mut t, Command::HsdsAdaptiveSubdivide { max_level: 2, max_angle: 30.0 }).unwrap();
    assert_eq!(faces(&t, c), 96, "every face has a 90° edge at both levels");
    // A topology change below drops the levels with a warning.
    execute(&mut t, Command::SetParameter { id: c.0, name: "Length Segs".into(), value: 3.0 }).unwrap();
    let e = t.scene.eval_mesh(c);
    assert!(e.warnings.iter().any(|w| w.contains("HSDS")), "{:?}", e.warnings);
}

#[test]
fn subdivide_wsm_uses_world_size() {
    let count = |name: &str, scale: f32| {
        let mut s = AppState::new();
        let b = boxed(&mut s, 20.0);
        execute(&mut s, Command::SetTransform { id: b.0, position: None, rotation_deg: None, scale: Some([scale; 3]) }).unwrap();
        add(&mut s, name);
        let i = s.scene.node(b).unwrap().modifiers.len() as u32 - 1;
        set(&mut s, i, "Size", 10.0);
        faces(&s, b)
    };
    assert_eq!(count("Subdivide", 1.0), count("Subdivide", 3.0), "object space ignores the node scale");
    assert!(count("Subdivide (WSM)", 3.0) > count("Subdivide (WSM)", 1.0), "world size: a bigger object gets more faces");
    assert_eq!(count("Subdivide (WSM)", 1.0), count("Subdivide", 1.0));
}

fn animated_bend(s: &mut AppState) -> NodeId {
    let b = boxed(s, 10.0);
    execute(s, Command::SetParameter { id: b.0, name: "Height Segs".into(), value: 6.0 }).unwrap();
    add(s, "Bend");
    execute(s, Command::AutoKeyModeToggle).unwrap();
    execute(s, Command::SetFrame { frame: 10 }).unwrap();
    set(s, 0, "Angle", 90.0);
    execute(s, Command::AutoKeyModeToggle).unwrap();
    execute(s, Command::SetFrame { frame: 0 }).unwrap();
    b
}

#[test]
fn point_cache_records_and_plays() {
    let mut s = AppState::new();
    let b = animated_bend(&mut s);
    let at = |s: &mut AppState, f: i32| {
        execute(s, Command::SetFrame { frame: f }).unwrap();
        verts(s, b)
    };
    let want5 = at(&mut s, 5);
    let want10 = at(&mut s, 10);
    add(&mut s, "Point Cache");
    let file = scratch("bend.pc2");
    execute(&mut s, Command::PointCacheRecord { index: 1, path: file.to_string_lossy().into(), start: 0, end: 10 }).unwrap();
    // The file is a real .pc2: signature, 11 samples of every vertex.
    let bytes = std::fs::read(&file).unwrap();
    assert_eq!(&bytes[..12], b"POINTCACHE2\0");
    let pc = loom_mesh::pc2::Pc2::read(&bytes[..]).unwrap();
    assert_eq!(pc.samples.len(), 11);
    assert_eq!(pc.points(), want5.len());
    // Turn the Bend off: the cache alone plays the motion back.
    execute(&mut s, Command::ToggleModifier { index: 0, on: false }).unwrap();
    for (f, want) in [(5, &want5), (10, &want10)] {
        let got = at(&mut s, f);
        for (a, c) in got.iter().zip(want.iter()) {
            assert!(a.distance(*c) < 1e-3, "frame {f}: {a} vs {c}");
        }
    }
    // Strength 0.5 with Relative Offset on: half the recorded motion on top of the input.
    set(&mut s, 1, "Relative Offset", 1.0);
    set(&mut s, 1, "Strength", 0.5);
    let rest = {
        execute(&mut s, Command::SetFrame { frame: 0 }).unwrap();
        verts(&s, b)
    };
    let half = at(&mut s, 10);
    for ((h, r), w) in half.iter().zip(&rest).zip(&want10) {
        assert!(h.distance(*r + (*w - *r) * 0.5) < 1e-3);
    }
    // The cache survives a save and reload through its file.
    let loom = scratch("cache.loom");
    execute(&mut s, Command::SaveFileAs { path: Some(loom.to_string_lossy().into()) }).unwrap();
    let mut t = AppState::new();
    execute(&mut t, Command::OpenFile { path: Some(loom.to_string_lossy().into()) }).unwrap();
    execute(&mut t, Command::SetFrame { frame: 10 }).unwrap();
    assert_eq!(verts(&t, b).len(), half.len());
}

#[test]
fn point_cache_count_mismatch_warns() {
    let mut s = AppState::new();
    let b = animated_bend(&mut s);
    add(&mut s, "Point Cache");
    let file = scratch("mismatch.pc2");
    execute(&mut s, Command::PointCacheRecord { index: 1, path: file.to_string_lossy().into(), start: 0, end: 3 }).unwrap();
    // More vertices below than the file has: warned, passed through.
    execute(&mut s, Command::SetParameter { id: b.0, name: "Height Segs".into(), value: 9.0 }).unwrap();
    execute(&mut s, Command::SetFrame { frame: 2 }).unwrap();
    let e = s.scene.eval_mesh(b);
    assert!(e.warnings.iter().any(|w| w.contains("Point Cache") && w.contains("vertices")), "{:?}", e.warnings);
    let before = s.scene.evaluated(b, Some(0)).value.mesh.vertex_count();
    assert_eq!(e.value.mesh.vertex_count(), before);
    // A truncated file is refused at load, never a panic.
    let bad = scratch("truncated.pc2");
    std::fs::write(&bad, &std::fs::read(&file).unwrap()[..40]).unwrap();
    assert!(execute(&mut s, Command::PointCacheLoad { index: 1, path: bad.to_string_lossy().into() }).is_err());
    std::fs::write(&bad, b"garbage").unwrap();
    assert!(execute(&mut s, Command::PointCacheLoad { index: 1, path: bad.to_string_lossy().into() }).is_err());
}

#[test]
fn point_cache_wsm_records_world_space() {
    let mut s = AppState::new();
    let b = boxed(&mut s, 10.0);
    execute(&mut s, Command::SetTransform { id: b.0, position: Some([50.0, 0.0, 0.0]), rotation_deg: None, scale: None }).unwrap();
    add(&mut s, "Point Cache (WSM)");
    let file = scratch("world.pc2");
    execute(&mut s, Command::PointCacheRecord { index: 0, path: file.to_string_lossy().into(), start: 0, end: 1 }).unwrap();
    let pc = loom_mesh::pc2::Pc2::read(&std::fs::read(&file).unwrap()[..]).unwrap();
    assert!(pc.samples[0].iter().all(|p| p.x > 40.0), "world positions");
    // Moving the object afterwards: the WSM cache pins the points in the world.
    execute(&mut s, Command::SetTransform { id: b.0, position: Some([0.0, 0.0, 0.0]), rotation_deg: None, scale: None }).unwrap();
    let world = s.scene.object_matrix(b);
    assert!(verts(&s, b).iter().all(|p| world.transform_point3(*p).x > 40.0));
}

#[test]
fn point_cache_record_refuses_a_changing_vertex_count() {
    let mut s = AppState::new();
    let b = boxed(&mut s, 10.0);
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::SetFrame { frame: 2 }).unwrap();
    execute(&mut s, Command::SetParameter { id: b.0, name: "Height Segs".into(), value: 4.0 }).unwrap();
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::SetFrame { frame: 0 }).unwrap();
    let n0 = s.scene.eval_mesh(b).value.mesh.vertex_count();
    execute(&mut s, Command::SetFrame { frame: 3 }).unwrap();
    assert_ne!(s.scene.eval_mesh(b).value.mesh.vertex_count(), n0, "the topology is animated");
    add(&mut s, "Point Cache");
    let file = scratch("changing.pc2");
    let _ = std::fs::remove_file(&file);
    let e = execute(&mut s, Command::PointCacheRecord { index: 0, path: file.to_string_lossy().into(), start: 0, end: 3 }).unwrap_err();
    assert!(e.contains("vertex count"), "{e}");
    assert!(!file.exists(), "no corrupt file left behind");
}
