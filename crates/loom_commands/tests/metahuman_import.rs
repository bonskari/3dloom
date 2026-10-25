use glam::{Mat4, Vec3};
use loom_commands::{execute, AppState, Command};
use loom_fbx::{FbxAnim, FbxBlendShape, FbxDocument, FbxKind, FbxNode, FbxSkin, FbxTake, WriteOpts};
use loom_scene::{Modifier, ObjectKind};
use serde_json::json;

fn option(s: &mut AppState, name: &str, value: bool) {
    execute(s, Command::SetExchangeOption { dialog: "fbx_import".into(), name: name.into(), value: json!(value) }).unwrap();
}

fn fixture(name: &str, active_morph: bool) -> std::path::PathBuf {
    let body = FbxNode::new("Body", FbxKind::Empty);
    let mut root = FbxNode::new("root", FbxKind::Bone { length: 1.0, size: 0.1 });
    root.parent = Some(0);
    let face = FbxNode::new("Face", FbxKind::Empty);
    let mut face_root = FbxNode::new("root", FbxKind::Bone { length: 1.0, size: 0.1 });
    face_root.parent = Some(2);
    let mut facial = FbxNode::new("FACIAL_C_FacialRoot", FbxKind::Bone { length: 1.0, size: 0.1 });
    facial.parent = Some(3);
    let mesh = loom_mesh::box_segmented(1.0, 1.0, 1.0, [1, 1, 1]);
    let count = mesh.vertex_count();
    let mut head = FbxNode::new("Head", FbxKind::Mesh { mesh, materials: vec![] });
    head.parent = Some(2);
    head.skin = Some(FbxSkin { bones: vec![(4, Mat4::IDENTITY)], mesh_bind: Mat4::IDENTITY, weights: vec![vec![(0, 1.0)]; count] });
    head.blend_shapes.push(FbxBlendShape { name: "Expression".into(), offsets: vec![(0, Vec3::Z)], weight: if active_morph { 50.0 } else { 0.0 } });
    let doc = FbxDocument { nodes: vec![body, root, face, face_root, facial, head], unit_metres: 0.0254,
        anim: Some(FbxTake { name: "Motion".into(), fps: 30.0, start: 0, end: 1,
            tracks: vec![(1, FbxAnim { frames: vec![0.0, 1.0], translation: vec![Vec3::ZERO, Vec3::X], rotation_deg: vec![Vec3::ZERO; 2], scale: vec![Vec3::ONE; 2] })] }), ..Default::default() };
    let path = std::env::temp_dir().join(format!("loom-metahuman-{name}.fbx"));
    std::fs::write(&path, loom_fbx::write(&doc, &WriteOpts::default())).unwrap();
    path
}

#[test]
fn body_profile_option_is_opt_in_persisted_and_imports_native_rest_skin() {
    let path = fixture("rest", false);
    let bytes = std::fs::read(&path).unwrap();
    let mut s = AppState::new();
    let options = execute(&mut s, Command::ExchangeOptions { dialog: "fbx_import".into() }).unwrap();
    assert_eq!(options["options"]["metahuman_body_only"], false);
    option(&mut s, "metahuman_body_only", true);
    for name in ["animation", "deformations", "skins", "morphs"] { option(&mut s, name, false); }
    option(&mut s, "bones_as_dummies", true);
    let settings = loom_commands::settings::Settings::capture(&s);
    let mut restored = AppState::new();
    loom_commands::settings::Settings::from_json(&settings.to_json()).unwrap().apply(&mut restored);
    let options = execute(&mut restored, Command::ExchangeOptions { dialog: "fbx_import".into() }).unwrap();
    assert_eq!(options["options"]["metahuman_body_only"], true);
    let undo = s.undo.len();
    execute(&mut s, Command::ImportFile { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    assert_eq!(s.undo.len(), undo + 1);
    assert_eq!(s.scene.nodes().filter(|n| matches!(n.object, ObjectKind::Bone { .. })).count(), 1);
    assert!(!s.scene.nodes().any(|n| n.name.starts_with("FACIAL_")));
    let head = s.scene.nodes().find(|n| n.name == "Head").unwrap();
    assert!(head.modifiers.iter().any(|m| matches!(m.modifier, Modifier::Skin(_))));
    assert!(!head.modifiers.iter().any(|m| matches!(m.modifier, Modifier::Morpher(_))));
    assert!(s.scene.animation_clips.is_empty());
    assert!(s.scene.nodes().all(|n| n.transform.position.keys().is_none()));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    // A second rest import adds a separate character even with the ordinary
    // importer's default Add and update animation setting.
    execute(&mut s, Command::ImportFile { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    assert_eq!(s.scene.nodes().filter(|n| matches!(n.object, ObjectKind::Bone { .. })).count(), 2);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.nodes().filter(|n| matches!(n.object, ObjectKind::Bone { .. })).count(), 1);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.nodes().count(), 0);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn body_profile_validates_face_morphs_even_when_deformations_disabled() {
    let path = fixture("reject", true);
    let mut s = AppState::new();
    option(&mut s, "metahuman_body_only", true);
    option(&mut s, "deformations", false);
    option(&mut s, "morphs", false);
    let undo = s.undo.len();
    let error = execute(&mut s, Command::ImportFile { path: Some(path.to_string_lossy().into_owned()) }).unwrap_err();
    assert!(error.contains("active Face morph"), "{error}");
    assert_eq!(s.scene.nodes().count(), 0);
    assert_eq!(s.undo.len(), undo);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn animation_clip_bypasses_body_rest_profile_and_restores_options() {
    let path = fixture("animation", false);
    let mut s = AppState::new();
    option(&mut s, "metahuman_body_only", true);
    let before = s.exchange.fbx_import.clone();
    execute(&mut s, Command::ImportAnimationClip { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    assert_eq!(s.exchange.fbx_import, before);
    assert_eq!(s.scene.animation_clips.len(), 1);
    assert!(s.scene.nodes().any(|n| n.name.starts_with("FACIAL_")));
    assert!(s.scene.nodes().any(|n| n.transform.position.keys().is_some()));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn external_metahuman_imports_only_body_skeleton_and_native_skins() {
    let Some(path) = std::env::var_os("LOOM_METAHUMAN_FBX") else { return };
    let path = std::path::Path::new(&path);
    let before = std::fs::read(path).unwrap();
    let mut s = AppState::new();
    option(&mut s, "metahuman_body_only", true);
    execute(&mut s, Command::ImportFile { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    assert_eq!(s.scene.nodes().filter(|n| matches!(n.object, ObjectKind::Bone { .. })).count(), 342);
    assert!(!s.scene.nodes().any(|n| n.name.starts_with("FACIAL_")));
    assert!(s.scene.nodes().any(|n| n.modifiers.iter().any(|m| matches!(m.modifier, Modifier::Skin(_)))));
    assert!(!s.scene.nodes().any(|n| n.modifiers.iter().any(|m| matches!(m.modifier, Modifier::Morpher(_)))));
    assert!(s.scene.animation_clips.is_empty());
    assert_eq!(std::fs::read(path).unwrap(), before);
}
