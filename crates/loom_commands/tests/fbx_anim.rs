//! FBX animation through the commands: import keys a clip, export bakes one.

use glam::Vec3;
use loom_commands::{execute, AppState, Command, PrimitiveKind};
use loom_scene::ObjectKind;
use std::path::PathBuf;

const RIG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../loom_fbx/tests/fixtures/sitting_idle.fbx");

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join("loom-fbx-anim-tests").join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn mixamo_clip_round_trips() {
    let mut s = AppState::new();
    execute(&mut s, Command::ImportFile { path: Some(RIG.into()) }).unwrap();
    let src = loom_fbx::read(&std::fs::read(RIG).unwrap(), 0.0254).unwrap();
    let take = src.anim.as_ref().unwrap();
    assert_eq!(s.frame_range, (take.start, take.end), "the scene takes the clip's range");
    let arm = s.scene.nodes().find(|n| n.name == "mixamorig:LeftArm").unwrap().id;
    assert!(s.scene.node(arm).unwrap().transform.rotation.is_animated());
    let path = dir("rt").join("clip.fbx");
    execute(&mut s, Command::ExportFile { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    let back = loom_fbx::read(&std::fs::read(&path).unwrap(), 0.0254).unwrap();
    let bt = back.anim.as_ref().expect("the export carries the clip");
    let name_of = |d: &loom_fbx::FbxDocument, i: usize| d.nodes[i].name.clone();
    let (si, sa) = take.tracks.iter().find(|(i, _)| name_of(&src, *i) == "mixamorig:LeftArm").unwrap();
    let (_, ba) = bt.tracks.iter().find(|(i, _)| name_of(&back, *i) == name_of(&src, *si)).expect("LeftArm animated in the export");
    let q = |r: Vec3| glam::Quat::from_euler(glam::EulerRot::XYZ, r.x.to_radians(), r.y.to_radians(), r.z.to_radians());
    for f in 0..sa.frames.len().min(ba.frames.len()) {
        let angle = q(sa.rotation_deg[f]).angle_between(q(ba.rotation_deg[f])).to_degrees();
        assert!(angle < 0.5, "frame {f}: {angle}°");
    }
}

#[test]
fn keyed_box_exports_and_reimports() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::SetFrame { frame: 30 }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [40.0, 0.0, 0.0], system: None }).unwrap();
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    let want = s.scene.node(*s.selection.iter().next().unwrap()).unwrap().transform.position.get_at(15.0);
    let path = dir("box").join("box.fbx");
    execute(&mut s, Command::ExportFile { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    let mut t = AppState::new();
    execute(&mut t, Command::ImportFile { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    let n = t.scene.nodes().find(|n| n.name == "Box001").unwrap();
    let got = n.transform.position.get_at(15.0);
    assert!(got.distance(want) < 1e-2, "{got:?} vs {want:?}");
    assert!((n.transform.position.get_at(30.0).x - 40.0).abs() < 1e-2);
}

#[test]
fn imported_clip_plays_on_the_skin() {
    let mut s = AppState::new();
    execute(&mut s, Command::ImportFile { path: Some(RIG.into()) }).unwrap();
    let mesh = s.scene.nodes().find(|n| matches!(n.object, ObjectKind::EditablePoly(_))).unwrap().id;
    let (a, b) = s.frame_range;
    execute(&mut s, Command::SetFrame { frame: a }).unwrap();
    let m0 = s.scene.eval_mesh(mesh).value.mesh.clone();
    execute(&mut s, Command::SetFrame { frame: (a + b) / 2 }).unwrap();
    let m1 = s.scene.eval_mesh(mesh).value.mesh.clone();
    let moved = m0.vertices().iter().zip(m1.vertices()).filter(|(p, q)| p.position.distance(q.position) > 1e-3).count();
    assert!(moved > 100, "{moved} vertices move between the frames");
}
