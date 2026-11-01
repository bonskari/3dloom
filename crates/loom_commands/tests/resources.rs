//! Utilities > Resource Collector: the scene's bitmaps and photometric webs
//! copied (or zipped) into one folder, optionally with the scene, and the
//! references pointed at the copies.

use loom_commands::light::LightKind;
use loom_commands::{execute, AppState, Command, PrimitiveKind};

const WEB: &str = "IESNA:LM-63-2002\nTILT=NONE\n1 -1 1.0 2 1 1 2 0.1 0.1 0.0\n1.0 1.0 20\n0 90\n0\n100 0\n";

fn scene(dir: &std::path::Path) -> AppState {
    let src = dir.join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("wood.png"), b"bitmap bytes").unwrap();
    std::fs::write(src.join("down.ies"), WEB).unwrap();
    let mut s = AppState::new();
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    execute(&mut s, Command::SetMaterialMap { slot: Some(0), map: "Diffuse Color".into(), path: Some(src.join("wood.png").to_string_lossy().into_owned()), amount: None }).unwrap();
    execute(&mut s, Command::AssignMaterialToSelection).unwrap();
    let r = execute(&mut s, Command::CreateLight { kind: LightKind::FreeLight, position: [0.0, 0.0, 50.0], target: [0.0; 3] }).unwrap();
    let light = r["id"].as_u64().unwrap();
    execute(&mut s, Command::PhotometricWebFile { id: light, path: src.join("down.ies").to_string_lossy().into_owned() }).unwrap();
    s
}

#[test]
fn resource_collector_copies_the_files_and_updates_the_references() {
    let dir = std::env::temp_dir().join(format!("loom-resources-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut s = scene(&dir);
    let out = dir.join("collected");
    let r = execute(&mut s, Command::ResourceCollector { output: out.to_string_lossy().into_owned(), include_scene: true, compress: false, update: true }).unwrap();
    assert_eq!(r["written"].as_array().unwrap().len(), 3, "{r}");
    for f in ["wood.png", "down.ies", "Untitled.loom"] {
        assert!(out.join(f).is_file(), "{f}");
    }
    let assets = loom_commands::assets::scene_assets(&s);
    assert!(assets.iter().all(|p| p.starts_with(out.to_str().unwrap())), "the references name the copies: {assets:?}");
    // The written scene names the copies too.
    let text = std::fs::read_to_string(out.join("Untitled.loom")).unwrap();
    assert!(text.contains(out.join("wood.png").to_str().unwrap()));
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(loom_commands::assets::scene_assets(&s).iter().all(|p| p.contains("/src/")), "one undo step back");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn resource_collector_compresses_into_one_zip_and_reports_missing_files() {
    let dir = std::env::temp_dir().join(format!("loom-resources-zip-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut s = scene(&dir);
    std::fs::remove_file(dir.join("src/wood.png")).unwrap();
    let out = dir.join("zipped");
    let before = loom_commands::assets::scene_assets(&s);
    let r = execute(&mut s, Command::ResourceCollector { output: out.to_string_lossy().into_owned(), include_scene: true, compress: true, update: false }).unwrap();
    assert_eq!(r["missing"].as_array().unwrap().len(), 1, "{r}");
    let zip = std::fs::read(out.join("Untitled.zip")).unwrap();
    let has = |name: &str| zip.windows(name.len()).any(|w| w == name.as_bytes());
    assert!(has("down.ies") && has("Untitled.loom") && !has("wood.png"));
    assert_eq!(loom_commands::assets::scene_assets(&s), before, "Update Materials off: the scene keeps its paths");
    assert!(execute(&mut s, Command::ResourceCollector { output: String::new(), include_scene: false, compress: false, update: false }).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}
