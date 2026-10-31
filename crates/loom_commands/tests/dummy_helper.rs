//! Helpers > Dummy: the reference's box plus line-drawn Display Types
//! (Sphere, Cross, Axis, Circle, Diamond, Arrow, Square, Triangle), per-axis
//! Length, own Color, Constant Screen Size, Draw On Top, Show Name and an
//! axis tripod; saved in .loom files and exported to FBX as a null.

use glam::Vec3;
use loom_commands::file::{read, write, Document, SceneSettings, FORMAT};
use loom_commands::helpers::{helper_labels, helper_lines, LineKind};
use loom_commands::{execute, AppState, Command};
use loom_scene::node::DUMMY_DISPLAY_TYPES;
use loom_scene::{NodeId, ObjectKind};

fn dummy(s: &mut AppState, size: f32) -> NodeId {
    execute(s, Command::CreateHelper { kind: "dummy".into(), position: [0.0; 3], size }).unwrap();
    *s.selection.iter().next().unwrap()
}

fn set(s: &mut AppState, id: NodeId, name: &str, value: f64) {
    execute(s, Command::SetParameter { id: id.0, name: name.into(), value }).unwrap();
}

fn display(name: &str) -> f64 {
    DUMMY_DISPLAY_TYPES.iter().position(|t| *t == name).unwrap() as f64
}

fn lines(s: &AppState, id: NodeId) -> Vec<(Vec3, Vec3, LineKind)> {
    helper_lines(s, id, &|_| 1.0)
}

#[test]
fn a_new_dummy_is_the_reference_box_and_draws_no_extra_lines() {
    let mut s = AppState::new();
    let d = dummy(&mut s, 10.0);
    let n = s.scene.node(d).unwrap();
    assert!(matches!(&n.object, ObjectKind::Dummy { display, .. } if display.get() == 0), "Box by default");
    let m = s.scene.eval_mesh(d).value.mesh.clone();
    assert_eq!(m.face_count(), 6, "the reference's cube");
    let (lo, hi) = m.aabb();
    assert!((lo - Vec3::splat(-5.0)).length() < 1e-4 && (hi - Vec3::splat(5.0)).length() < 1e-4, "{lo} {hi}");
    assert!(lines(&s, d).is_empty(), "the box is its mesh");
}

#[test]
fn every_display_type_draws_lines_within_its_extents_and_has_no_faces() {
    let mut s = AppState::new();
    let d = dummy(&mut s, 20.0);
    let mut counts = Vec::new();
    for (i, name) in DUMMY_DISPLAY_TYPES.iter().enumerate().skip(1) {
        set(&mut s, d, "Display Type", i as f64);
        let l = lines(&s, d);
        assert!(!l.is_empty(), "{name} draws lines");
        assert!(l.iter().all(|(a, b, _)| a.abs().max_element() <= 10.0 + 1e-3 && b.abs().max_element() <= 10.0 + 1e-3), "{name} stays within Size");
        let m = s.scene.eval_mesh(d).value.mesh.clone();
        assert_eq!(m.face_count(), 0, "{name}: no surface, only lines");
        let (lo, hi) = m.aabb();
        assert!((hi - lo - Vec3::splat(20.0)).length() < 1e-3, "{name}: the mesh keeps the extents for Zoom Extents ({lo} {hi})");
        counts.push((name, l.len()));
    }
    assert_eq!(counts.iter().find(|c| *c.0 == "Cross").unwrap().1, 3);
    assert_eq!(counts.iter().find(|c| *c.0 == "Square").unwrap().1, 4);
    assert_eq!(counts.iter().find(|c| *c.0 == "Triangle").unwrap().1, 3);
    assert_eq!(counts.iter().find(|c| *c.0 == "Diamond").unwrap().1, 12);
    set(&mut s, d, "Display Type", display("Axis"));
    let l = lines(&s, d);
    assert_eq!(l.iter().map(|x| x.2).collect::<Vec<_>>(), vec![LineKind::Axis(0), LineKind::Axis(1), LineKind::Axis(2)]);
    let labels: Vec<String> = helper_labels(&s, d).into_iter().map(|l| l.1).collect();
    assert_eq!(labels, ["X", "Y", "Z"], "the tripod is labelled");
}

#[test]
fn size_and_per_axis_length_scale_the_lines_and_follow_the_node() {
    let mut s = AppState::new();
    let d = dummy(&mut s, 10.0);
    set(&mut s, d, "Display Type", display("Cross"));
    set(&mut s, d, "Length X", 3.0);
    set(&mut s, d, "Size", 20.0);
    execute(&mut s, Command::MoveSelection { delta: [100.0, 0.0, 0.0], system: None }).unwrap();
    let l = lines(&s, d);
    let x = l.iter().find(|(a, b, _)| (b - a).normalize().x.abs() > 0.99).unwrap();
    assert!(((x.1 - x.0).length() - 60.0).abs() < 1e-3, "X: Size 20 times Length X 3");
    assert!(((x.0 + x.1) * 0.5 - Vec3::new(100.0, 0.0, 0.0)).length() < 1e-3, "drawn at the node");
}

#[test]
fn constant_screen_size_measures_size_in_pixels() {
    let mut s = AppState::new();
    let d = dummy(&mut s, 10.0);
    set(&mut s, d, "Display Type", display("Square"));
    set(&mut s, d, "Constant Screen Size", 1.0);
    let near = helper_lines(&s, d, &|_| 0.5);
    let far = helper_lines(&s, d, &|_| 2.0);
    let len = |l: &[(Vec3, Vec3, LineKind)]| (l[0].1 - l[0].0).length();
    assert!((len(&far) / len(&near) - 4.0).abs() < 1e-3, "the world size follows units per pixel");
    // A screen-sized Box draws as lines too.
    set(&mut s, d, "Display Type", 0.0);
    assert_eq!(helper_lines(&s, d, &|_| 1.0).len(), 12);
    assert_eq!(s.scene.eval_mesh(d).value.mesh.face_count(), 0);
}

#[test]
fn axis_tripod_show_name_and_options_save_and_undo() {
    let mut s = AppState::new();
    let d = dummy(&mut s, 10.0);
    set(&mut s, d, "Display Type", display("Sphere"));
    set(&mut s, d, "Axis Tripod", 1.0);
    set(&mut s, d, "Show Name", 1.0);
    set(&mut s, d, "Draw On Top", 1.0);
    set(&mut s, d, "Use Color", 1.0);
    set(&mut s, d, "Color", 0x112233 as f64);
    let l = lines(&s, d);
    assert_eq!(l.iter().filter(|x| matches!(x.2, LineKind::Axis(_))).count(), 3, "the tripod on top of the sphere");
    let name = s.scene.node(d).unwrap().name.clone();
    assert_eq!(helper_labels(&s, d), vec![(Vec3::ZERO, name)]);
    let path = std::env::temp_dir().join("loom-dummy-helper.loom");
    write(&path, &Document { format: FORMAT, app: "test".into(), scene: s.scene.clone(), settings: SceneSettings::from_state(&s) }).unwrap();
    let doc = read(&path).unwrap();
    assert_eq!(doc.scene.node(d).unwrap().object, s.scene.node(d).unwrap().object, "every option is saved");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(matches!(&s.scene.node(d).unwrap().object, ObjectKind::Dummy { use_color, .. } if use_color.get()), "one undo per change: the colour");
    assert!(matches!(&s.scene.node(d).unwrap().object, ObjectKind::Dummy { color, .. } if color.get() != 0x112233));
}

#[test]
fn old_files_load_a_dummy_as_the_box() {
    let d = ObjectKind::dummy(7.0);
    let mut v = serde_json::to_value(&d).unwrap();
    let inner = v.get_mut("Dummy").unwrap().as_object_mut().unwrap();
    inner.retain(|k, _| k == "size");
    let back: ObjectKind = serde_json::from_value(v).unwrap();
    assert_eq!(back, d, "missing options take the defaults (Box, lengths 1)");
}

#[test]
fn fbx_export_writes_a_null_with_the_display_type() {
    let mut s = AppState::new();
    let d = dummy(&mut s, 10.0);
    set(&mut s, d, "Display Type", display("Diamond"));
    let dir = std::env::temp_dir().join("loom-dummy-helper");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("d.fbx");
    execute(&mut s, Command::ExportFile { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let has = |t: &str| bytes.windows(t.len()).any(|w| w == t.as_bytes());
    assert!(has("DummyDisplayType") && has("Diamond"), "the display type is a user property");
    let scene = ufbx::load_file(path.to_str().unwrap(), ufbx::LoadOpts::default()).unwrap();
    let node = scene.nodes.iter().find(|n| n.element.name.starts_with("Dummy")).expect("the dummy node");
    assert!(node.mesh.is_none(), "a null, not geometry");
}
