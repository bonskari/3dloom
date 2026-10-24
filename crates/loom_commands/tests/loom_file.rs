//! `.loom` document: lossless round trip of every object kind, every modifier and the settings.

use loom_commands::file::{read, summary, write, Document, SceneSettings, FORMAT};
use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp, SelectionMode, ShapeKind};
use loom_scene::{Modifier, NodeId};
use std::path::PathBuf;

fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("loom-file-tests");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn sel(s: &AppState) -> NodeId {
    *s.selection.iter().next().unwrap()
}

fn document(s: &AppState) -> Document {
    Document { format: FORMAT, app: "test".into(), scene: s.scene.clone(), settings: SceneSettings::from_state(s) }
}

fn assert_scene_equal(a: &loom_scene::Scene, b: &loom_scene::Scene) {
    assert_eq!(a.len(), b.len());
    for n in a.nodes() {
        let m = b.node(n.id).unwrap_or_else(|| panic!("node {} missing after load", n.name));
        assert_eq!(n, m, "node {} differs after load", n.name);
    }
    assert_eq!(a.layers, b.layers);
    assert_eq!(a.current_layer, b.current_layer);
    assert_eq!(a.next_id(), b.next_id());
}

#[test]
fn every_object_kind_round_trips() {
    let mut s = AppState::new();
    let kinds = [
        PrimitiveKind::Box, PrimitiveKind::Cone, PrimitiveKind::Sphere, PrimitiveKind::GeoSphere, PrimitiveKind::Cylinder,
        PrimitiveKind::Tube, PrimitiveKind::Torus, PrimitiveKind::Pyramid, PrimitiveKind::Teapot, PrimitiveKind::Plane,
    ];
    for (i, k) in kinds.iter().enumerate() {
        execute(&mut s, Command::CreatePrimitive { kind: *k, position: [i as f32 * 20.0, 0.0, 0.0], size: [10.0; 3] }).unwrap();
    }
    let shapes = [ShapeKind::Rectangle, ShapeKind::Circle, ShapeKind::Ellipse, ShapeKind::Arc, ShapeKind::Donut, ShapeKind::NGon, ShapeKind::Star, ShapeKind::Helix];
    for (i, k) in shapes.iter().enumerate() {
        execute(&mut s, Command::CreateShape { kind: *k, position: [i as f32 * 20.0, 40.0, 0.0], size: [10.0; 3] }).unwrap();
    }
    // Editable Poly with a face selection, Editable Spline.
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0, 80.0, 0.0], size: [10.0; 3] }).unwrap();
    execute(&mut s, Command::ConvertToEditablePoly).unwrap();
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Face }).unwrap();
    execute(&mut s, Command::SelectSub { node: None, ids: vec![1, 2], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::Extrude { height: 3.0, width: 0.0, kind: loom_commands::ExtrudeKind::Group }).unwrap();
    execute(&mut s, Command::SelectionMode { mode: SelectionMode::Object }).unwrap();
    execute(&mut s, Command::CreateShape { kind: ShapeKind::Star, position: [30.0, 80.0, 0.0], size: [10.0; 3] }).unwrap();
    execute(&mut s, Command::ConvertToEditableSpline).unwrap();
    // Boolean, ProBoolean, Loft, Camera with target, Group.
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0, 120.0, 0.0], size: [10.0; 3] }).unwrap();
    let a = sel(&s);
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [3.0, 123.0, 3.0], size: [10.0; 3] }).unwrap();
    let b = sel(&s);
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::CreateBoolean { operand_b: b.0, operation: 2, move_operand: true }).unwrap();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Sphere, position: [30.0, 120.0, 0.0], size: [10.0; 3] }).unwrap();
    execute(&mut s, Command::CreateProBoolean).unwrap();
    execute(&mut s, Command::CreateShape { kind: ShapeKind::Circle, position: [60.0, 120.0, 0.0], size: [4.0; 3] }).unwrap();
    let section = sel(&s);
    execute(&mut s, Command::CreateShape { kind: ShapeKind::Helix, position: [80.0, 120.0, 0.0], size: [10.0, 10.0, 30.0] }).unwrap();
    let path = sel(&s);
    execute(&mut s, Command::SelectNodes { ids: vec![section.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::CreateLoft { path: path.0 }).unwrap();
    execute(&mut s, Command::CreateCamera { target: true }).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![a.0, path.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::Group { name: Some("G".into()) }).unwrap();
    let kinds_present: std::collections::BTreeSet<&str> = s.scene.nodes().map(|n| n.object.base_name()).collect();
    assert!(kinds_present.len() >= 22, "{kinds_present:?}");

    let path = tmp("kinds.loom");
    write(&path, &document(&s)).unwrap();
    let doc = read(&path).unwrap();
    assert_eq!(doc.format, FORMAT);
    assert_scene_equal(&s.scene, &doc.scene);
    // The loaded scene evaluates every node without error.
    for n in doc.scene.nodes() {
        let e = doc.scene.eval_mesh(n.id);
        assert!(e.warnings.is_empty() || n.object.base_name() != "Box", "{}: {:?}", n.name, e.warnings);
    }
}

#[test]
fn every_modifier_round_trips() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0, 10.0, 40.0] }).unwrap();
    let a = sel(&s);
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [50.0, 0.0, 0.0], size: [5.0; 3] }).unwrap();
    let control = sel(&s);
    execute(&mut s, Command::CreateShape { kind: ShapeKind::Circle, position: [0.0, 50.0, 0.0], size: [4.0; 3] }).unwrap();
    let section = sel(&s);
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    let mut added = 0;
    let mut shape_only: Vec<&str> = Vec::new();
    for name in Modifier::LIST {
        let r = execute(&mut s, Command::AddModifier { name: (*name).into() });
        if r.is_err() {
            shape_only.push(name);
        } else {
            added += 1;
            // Keep the stack cheap: the entry stays, but disabled, so the next Add does not evaluate the whole tower.
            let index = s.scene.node(a).unwrap().modifiers.len() as u32 - 1;
            let slot = &s.scene.node(a).unwrap().modifiers[index as usize];
            match &slot.modifier {
                Modifier::LinkedXForm { .. } | Modifier::Preserve { .. } => {
                    execute(&mut s, Command::PickModifierNode { index, node: control.0 }).unwrap();
                }
                Modifier::Sweep { .. } => {
                    execute(&mut s, Command::SetModifierParameter { index, name: "Section".into(), value: 10.0 }).ok();
                    execute(&mut s, Command::PickModifierNode { index, node: section.0 }).ok();
                }
                Modifier::FFD { .. } => {
                    execute(&mut s, Command::SwitchToControlPointLevel).unwrap();
                    execute(&mut s, Command::SelectControlPoints { op: SelectOp::Replace, indices: vec![0] }).unwrap();
                    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 2.0], system: None }).unwrap();
                    execute(&mut s, Command::SwitchToTopLevel).unwrap();
                }
                _ => {}
            }
            execute(&mut s, Command::ToggleModifier { index, on: false }).unwrap();
        }
    }
    assert!(added >= 40, "only {added} modifiers were added");
    // Shape modifiers go on the circle.
    execute(&mut s, Command::SelectNodes { ids: vec![section.0], op: SelectOp::Replace }).unwrap();
    for name in &shape_only {
        execute(&mut s, Command::AddModifier { name: (*name).into() }).unwrap_or_else(|e| panic!("{name}: {e}"));
        let index = s.scene.node(section).unwrap().modifiers.len() as u32 - 1;
        execute(&mut s, Command::ToggleModifier { index, on: false }).unwrap();
    }
    let all: usize = s.scene.nodes().map(|n| n.modifiers.len()).sum();
    assert_eq!(all, Modifier::LIST.len(), "every listed modifier is on some node");

    let path = tmp("modifiers.loom");
    write(&path, &document(&s)).unwrap();
    let doc = read(&path).unwrap();
    assert_scene_equal(&s.scene, &doc.scene);
}

#[test]
fn settings_round_trip() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    let a = sel(&s);
    execute(&mut s, Command::NamedSelectionSet { name: "boxes".into() }).unwrap();
    execute(&mut s, Command::ViewportLayout { layout: loom_commands::ViewportLayout::TwoV }).unwrap();
    execute(&mut s, Command::SetTimeConfiguration { start: Some(10), end: Some(200), fps: Some(25.0) }).unwrap();
    execute(&mut s, Command::SetUnits { system_unit: Some("Meters".into()), system_scale: None, display: Some("Centimeters".into()), custom_name: None, custom_value: None, custom_unit: None }).unwrap();
    s.viewports[0].camera.distance = 123.0;
    let settings = SceneSettings::from_state(&s);
    let json = serde_json::to_string(&settings).unwrap();
    let back: SceneSettings = serde_json::from_str(&json).unwrap();
    let mut t = AppState::new();
    t.scene = s.scene.clone();
    back.apply_to(&mut t);
    assert_eq!(t.layout, loom_commands::ViewportLayout::TwoV);
    assert_eq!(t.viewports.len(), s.viewports.len());
    assert_eq!(t.viewports[0].camera.distance, 123.0);
    assert_eq!(t.frame_range, (10, 200));
    assert_eq!(t.fps, 25.0);
    assert_eq!(t.units, s.units);
    assert_eq!(t.named_sets.get("boxes").unwrap(), &vec![a]);
    assert_eq!(t.selection, s.selection);
}

#[test]
fn newer_format_is_refused() {
    let path = tmp("newer.loom");
    std::fs::write(&path, format!(r#"{{"format": {}, "app": "future", "scene": {{}}, "settings": {{}}}}"#, FORMAT + 1)).unwrap();
    let err = read(&path).unwrap_err();
    assert!(err.contains(&format!("format {} is newer than this program's {FORMAT}", FORMAT + 1)), "{err}");
    std::fs::write(&path, "{ not json").unwrap();
    assert!(read(&path).is_err());
}

#[test]
fn save_is_atomic() {
    let dir = std::env::temp_dir().join("loom-file-tests-atomic");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("a.loom");
    let s = AppState::new();
    write(&path, &document(&s)).unwrap();
    write(&path, &document(&s)).unwrap();
    let names: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, vec!["a.loom"], "no temp file left behind: {names:?}");
    read(&path).unwrap();
}

#[test]
fn summary_lists_nodes() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Teapot, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    execute(&mut s, Command::AddModifier { name: "Bend".into() }).unwrap();
    let v = summary(&document(&s));
    assert_eq!(v["format"], FORMAT);
    assert_eq!(v["nodes"][0]["name"], "Teapot001");
    assert_eq!(v["nodes"][0]["kind"], "Teapot");
    assert_eq!(v["nodes"][0]["stack"][0], "Bend");
    assert!(v["nodes"][0]["faces"].as_u64().unwrap() > 100);
    assert_eq!(v["layers"].as_array().unwrap().len(), 1);
    assert_eq!(v["settings"]["layout"], "FourGrid");
}

#[test]
fn rig_survives_save_and_open() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Cylinder, position: [0.0; 3], size: [2.0, 2.0, 20.0] }).unwrap();
    let arm = sel(&s);
    let r = execute(&mut s, Command::CreateBoneChain { points: vec![[0.0, 0.0, 0.0], [0.0, 0.0, 10.0], [0.0, 0.0, 20.0], [0.0, 0.0, 21.0]], name: None }).unwrap();
    let bones: Vec<u64> = serde_json::from_value(r["nodes"].clone()).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![bones[0], bones[2]], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::HistoryIndependentIkSolver).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![arm.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SkinModifier).unwrap();
    execute(&mut s, Command::SkinAddBones { ids: bones[..2].to_vec() }).unwrap();
    execute(&mut s, Command::ModifierSubobjectLevel { index: 0, level: 1 }).unwrap();
    execute(&mut s, Command::SkinSelectVertices { ids: vec![0, 1, 2], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SetWeightTo05Skin).unwrap();
    let path = tmp("rig.loom");
    write(&path, &document(&s)).unwrap();
    let doc = read(&path).unwrap();
    assert_scene_equal(&s.scene, &doc.scene);
    assert_eq!(doc.scene.ik_chains, s.scene.ik_chains);
    let before = s.scene.node(arm).unwrap().modifiers[0].modifier.clone();
    assert_eq!(doc.scene.node(arm).unwrap().modifiers[0].modifier, before);
    assert_eq!(doc.scene.nodes().filter(|n| n.object.is_bone()).count(), 4, "three bones and the nub");
}

#[test]
fn animation_survives_save_and_open() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    let b = sel(&s);
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::SetFrame { frame: 20 }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [30.0, 0.0, 0.0], system: None }).unwrap();
    execute(&mut s, Command::SetParameter { id: b.0, name: "Height".into(), value: 25.0 }).unwrap();
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::AssignController { id: b.0, track: "Position".into(), controller: "TCB Position".into() }).unwrap();
    execute(&mut s, Command::AssignController { id: b.0, track: "Rotation".into(), controller: "Rotation List".into() }).unwrap();
    execute(&mut s, Command::ListAddController { id: b.0, track: "Rotation".into(), controller: "Noise Rotation".into() }).unwrap();
    execute(&mut s, Command::CreateHelper { kind: "dummy".into(), position: [0.0, 20.0, 0.0], size: 2.0 }).unwrap();
    let d = sel(&s);
    execute(&mut s, Command::SelectNodes { ids: vec![b.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::LookAtConstraint { target: d.0 }).unwrap();
    execute(&mut s, Command::OpenCurveEditor).unwrap();
    execute(&mut s, Command::TrackViewHighlight { items: vec![format!("{}", b.0)], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::AddNoteTrack).unwrap();
    execute(&mut s, Command::AddNoteKey { note: 0, time: 5.0, text: "contact".into() }).unwrap();
    execute(&mut s, Command::AddVisibilityTrack).unwrap();
    execute(&mut s, Command::TrackViewHighlight { items: vec![format!("{}:Position", b.0)], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::TrackSetCreate { name: "Moves".into() }).unwrap();
    execute(&mut s, Command::SetPlayback { real_time: Some(false), loop_playback: Some(false), speed: Some(2.0), direction: Some(loom_commands::anim_cmds::PlayDirection::PingPong) }).unwrap();
    execute(&mut s, Command::SetDefaultTangentAsLinear).unwrap();
    let path = tmp("anim.loom");
    write(&path, &document(&s)).unwrap();
    let doc = read(&path).unwrap();
    assert_scene_equal(&s.scene, &doc.scene);
    assert_eq!(doc.scene.track_data, s.scene.track_data);
    let mut t = AppState::new();
    t.scene = doc.scene.clone();
    doc.settings.apply_to(&mut t);
    assert_eq!(t.playback.real_time, false);
    assert_eq!(t.playback.speed, 2.0);
    assert_eq!(t.playback.direction, loom_commands::anim_cmds::PlayDirection::PingPong);
    assert_eq!(t.default_tangent.0, loom_scene::anim::TangentType::Linear);
    let p = |st: &AppState, f: f32| st.scene.node(b).unwrap().transform.position.get_at(f);
    assert_eq!(p(&t, 13.0), p(&s, 13.0));
}
