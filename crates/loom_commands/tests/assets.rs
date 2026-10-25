//! File > Replace, Set Project Folder, the startup scene, Asset Tracking
//! and the Missing External Files check on open.

use loom_commands::{execute, AppState, Command, Dialog};
use loom_scene::{NodeId, ObjectKind};
use std::path::PathBuf;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("loom-assets-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn prim(s: &mut AppState, kind: loom_commands::PrimitiveKind, at: [f32; 3]) -> NodeId {
    execute(s, Command::CreatePrimitive { kind, position: at, size: [10.0; 3] }).unwrap();
    *s.selection.iter().next().unwrap()
}

#[test]
fn set_project_folder_makes_the_folders_and_is_a_setting() {
    let d = dir("project");
    let mut s = AppState::new();
    let r = execute(&mut s, Command::SetProjectFolder { path: None }).unwrap();
    assert_eq!(r["dialog"], "ProjectFolder");
    execute(&mut s, Command::SetProjectFolder { path: Some(d.to_string_lossy().into_owned()) }).unwrap();
    for f in ["scenes", "import", "export", "renderoutput", "sceneassets/images", "autoback"] {
        assert!(d.join(f).is_dir(), "{f}");
    }
    assert_eq!(s.project_folder.as_deref(), Some(d.as_path()));
    assert_eq!(loom_commands::assets::project_dir(&s, "scenes"), Some(d.join("scenes")));
    let settings = loom_commands::settings::Settings::capture(&s);
    let mut t = AppState::new();
    loom_commands::settings::Settings::from_json(&settings.to_json()).unwrap().apply(&mut t);
    assert_eq!(t.project_folder, s.project_folder, "kept between sessions");
}

#[test]
fn reset_starts_from_the_projects_startup_scene() {
    let d = dir("startup");
    let mut s = AppState::new();
    execute(&mut s, Command::SetProjectFolder { path: Some(d.to_string_lossy().into_owned()) }).unwrap();
    prim(&mut s, loom_commands::PrimitiveKind::Teapot, [0.0; 3]);
    let startup = d.join("scenes").join(loom_commands::assets::STARTUP_SCENE);
    execute(&mut s, Command::SaveFileAsCopy { path: Some(startup.to_string_lossy().into_owned()) }).unwrap();
    prim(&mut s, loom_commands::PrimitiveKind::Box, [50.0, 0.0, 0.0]);
    execute(&mut s, Command::ResetFile).unwrap();
    assert_eq!(s.scene.len(), 1, "the startup scene's teapot");
    assert!(matches!(s.scene.nodes().next().unwrap().object, ObjectKind::Teapot { .. }));
    assert!(s.file.is_none(), "untitled");
    assert!(s.undo.is_saved(), "unmodified");
}

#[test]
fn replace_swaps_the_geometry_of_same_named_objects_and_keeps_their_transforms() {
    let d = dir("replace");
    // The file: a sphere called Box001 with a Bend, and a teapot no scene object is named after.
    let mut f = AppState::new();
    let sphere = prim(&mut f, loom_commands::PrimitiveKind::Sphere, [0.0; 3]);
    execute(&mut f, Command::Rename { id: sphere.0, name: "Box001".into() }).unwrap();
    execute(&mut f, Command::AddModifier { name: "Bend".into() }).unwrap();
    prim(&mut f, loom_commands::PrimitiveKind::Teapot, [0.0; 3]);
    let path = d.join("parts.loom");
    execute(&mut f, Command::SaveFile { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    // The scene: Box001 somewhere else, and an instance of it.
    let mut s = AppState::new();
    let b = prim(&mut s, loom_commands::PrimitiveKind::Box, [30.0, 0.0, 0.0]);
    execute(&mut s, Command::SelectNodes { ids: vec![b.0], op: loom_commands::command::SelectOp::Replace }).unwrap();
    execute(&mut s, Command::CloneOptions { kind: loom_commands::state::CloneKind::Instance, copies: 1, name: None, offset: Some([0.0, 30.0, 0.0]), rotate_deg: None, scale: None }).unwrap();
    let before_t = s.scene.node(b).unwrap().transform.clone();
    let r = execute(&mut s, Command::ReplaceFile { path: Some(path.to_string_lossy().into_owned()), names: None, materials: None }).unwrap();
    assert_eq!(r["objects"], serde_json::json!(["Box001"]), "only the same-named object is offered");
    assert_eq!(s.dialog, Some(Dialog::ReplaceObjects));
    execute(&mut s, Command::ReplaceFile { path: Some(path.to_string_lossy().into_owned()), names: Some(vec!["Box001".into()]), materials: Some(false) }).unwrap();
    let n = s.scene.node(b).unwrap();
    assert!(matches!(n.object, ObjectKind::Sphere { .. }), "the geometry comes from the file");
    assert_eq!(n.modifiers.len(), 1, "with its stack");
    assert_eq!(n.transform, before_t, "the transform stays");
    for inst in s.scene.nodes().filter(|x| x.instance_of == Some(b)) {
        assert!(matches!(inst.object, ObjectKind::Sphere { .. }), "instances follow");
    }
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(matches!(s.scene.node(b).unwrap().object, ObjectKind::Box { .. }), "one undo step");
    assert!(execute(&mut s, Command::ReplaceFile { path: Some(path.to_string_lossy().into_owned()), names: Some(vec!["Teapot001".into()]), materials: Some(false) }).is_err());
}

#[test]
fn asset_tracking_lists_repaths_and_finds_missing_files() {
    let d = dir("tracking");
    let found = d.join("maps");
    std::fs::create_dir_all(&found).unwrap();
    let tex = found.join("wood.png");
    image_bytes(&tex);
    let mut s = AppState::new();
    let b = prim(&mut s, loom_commands::PrimitiveKind::Box, [0.0; 3]);
    let gone = "/no/such/folder/wood.png";
    execute(&mut s, Command::SetTextureDisplay { id: b.0, kind: "bitmap".into(), path: Some(gone.into()) }).unwrap();
    let r = execute(&mut s, Command::AssetTracking).unwrap();
    assert_eq!(s.dialog, Some(Dialog::AssetTracking));
    assert_eq!(r["assets"][0]["path"], gone);
    assert_eq!(r["assets"][0]["status"], "Missing");
    // Saved and opened again: the Missing External Files dialog.
    let scene = d.join("scene.loom");
    execute(&mut s, Command::SaveFile { path: Some(scene.to_string_lossy().into_owned()) }).unwrap();
    let mut t = AppState::new();
    let r = execute(&mut t, Command::OpenFile { path: Some(scene.to_string_lossy().into_owned()) }).unwrap();
    assert_eq!(r["missing"], serde_json::json!([gone]));
    assert_eq!(t.dialog, Some(Dialog::MissingFiles));
    // Browse: found by name in the folder.
    let r = execute(&mut t, Command::FindMissingFiles { folder: found.to_string_lossy().into_owned() }).unwrap();
    assert_eq!(r["missing"], serde_json::json!([]));
    assert_eq!(loom_commands::assets::scene_assets(&t), vec![tex.to_string_lossy().into_owned()]);
    // Set Path back, then undo.
    execute(&mut t, Command::RepathAsset { from: tex.to_string_lossy().into_owned(), to: "elsewhere/wood.png".into() }).unwrap();
    assert_eq!(loom_commands::assets::scene_assets(&t), vec!["elsewhere/wood.png".to_owned()]);
    execute(&mut t, Command::UndoSceneOperation).unwrap();
    assert_eq!(loom_commands::assets::scene_assets(&t), vec![tex.to_string_lossy().into_owned()]);
}

/// A 1×1 PNG.
fn image_bytes(path: &std::path::Path) {
    const PNG: [u8; 67] = [
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
        0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
        0x42, 0x60, 0x82,
    ];
    std::fs::write(path, PNG).unwrap();
}
