//! Materials: the Standard material (Blinn) and the Material Editor's
//! 24 sample slots, assignment to objects, maps, undo and the .loom file.

use loom_commands::file::{read, write, Document, SceneSettings, FORMAT};
use loom_commands::{execute, AppState, Command, PrimitiveKind};
use loom_scene::NodeId;

fn boxed(s: &mut AppState) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    *s.selection.iter().next().unwrap()
}

fn param(slot: u32, name: &str, value: f64) -> Command {
    Command::SetMaterialParameter { slot: Some(slot), name: name.into(), value }
}

#[test]
fn material_editor_toggle_opens_and_closes_the_editor() {
    let mut s = AppState::new();
    execute(&mut s, Command::MaterialEditorToggle).unwrap();
    assert!(s.material_editor_open);
    execute(&mut s, Command::MaterialEditorToggle).unwrap();
    assert!(!s.material_editor_open);
}

#[test]
fn empty_slots_show_max_defaults_and_editing_creates_the_material() {
    let mut s = AppState::new();
    let m = s.scene.slot_material(0);
    assert_eq!(m.name, "01 - Default");
    assert_eq!(s.scene.slot_material(23).name, "24 - Default");
    assert_eq!((m.specular_level.get(), m.glossiness.get(), m.opacity.get()), (0.0, 10.0, 100.0));
    assert!(s.scene.materials.is_empty(), "looking at a slot creates nothing");

    execute(&mut s, Command::MaterialEditorSlot { slot: 2 }).unwrap();
    execute(&mut s, Command::SetMaterialColor { slot: None, which: "Diffuse".into(), rgb: [200, 30, 10] }).unwrap();
    assert_eq!(s.scene.materials.len(), 1);
    let d = s.scene.slot_material(2).diffuse.get();
    assert!((d.x - 200.0 / 255.0).abs() < 1e-4 && (d.z - 10.0 / 255.0).abs() < 1e-4);
    // Ambient is locked to Diffuse by default.
    assert_eq!(s.scene.slot_material(2).ambient.get(), d);

    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(s.scene.materials.is_empty());
    assert_eq!(s.scene.slot_material(2).name, "03 - Default");
}

#[test]
fn parameters_are_clamped_to_max_ranges() {
    let mut s = AppState::new();
    execute(&mut s, param(0, "Glossiness", 250.0)).unwrap();
    execute(&mut s, param(0, "Specular Level", 5000.0)).unwrap();
    execute(&mut s, param(0, "Opacity", -3.0)).unwrap();
    execute(&mut s, param(0, "2-Sided", 1.0)).unwrap();
    let m = s.scene.slot_material(0);
    assert_eq!((m.glossiness.get(), m.specular_level.get(), m.opacity.get()), (100.0, 999.0, 0.0));
    assert!(m.two_sided);
    assert!(execute(&mut s, param(0, "Shininess", 1.0)).is_err());
}

#[test]
fn assign_pick_and_reset() {
    let mut s = AppState::new();
    let a = boxed(&mut s);
    execute(&mut s, param(0, "Specular Level", 40.0)).unwrap();
    execute(&mut s, Command::AssignMaterialToSelection).unwrap();
    let id = s.scene.node(a).unwrap().material.expect("assigned");
    assert_eq!(s.scene.materials[&id].specular_level.get(), 40.0);

    // Pick Material From Object fills the active slot with the object's material.
    execute(&mut s, Command::MaterialEditorSlot { slot: 5 }).unwrap();
    execute(&mut s, Command::GetMaterialFromObject { node: Some(a.0) }).unwrap();
    assert_eq!(s.scene.material_editor.slots[5], Some(id));

    execute(&mut s, Command::UndoSceneOperation).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.node(a).unwrap().material, None, "assignment undoes");

    execute(&mut s, Command::RedoSceneOperation).unwrap();
    execute(&mut s, Command::ResetMaterial { slot: Some(0) }).unwrap();
    assert_eq!(s.scene.slot_material(0).specular_level.get(), 0.0);
}

#[test]
fn maps_set_and_clear() {
    let mut s = AppState::new();
    execute(&mut s, Command::SetMaterialMap { slot: Some(0), map: "Diffuse Color".into(), path: Some("/tmp/wood.png".into()), amount: None }).unwrap();
    execute(&mut s, Command::SetMaterialMap { slot: Some(0), map: "Bump".into(), path: Some("/tmp/bump.png".into()), amount: Some(60.0) }).unwrap();
    let m = s.scene.slot_material(0);
    assert_eq!(m.diffuse_map.as_ref().map(|b| (b.path.as_str(), b.amount)), Some(("/tmp/wood.png", 100.0)));
    assert_eq!(m.bump_map.as_ref().map(|b| b.amount), Some(60.0), "the bump default is 30; 60 was given");
    execute(&mut s, Command::SetMaterialMap { slot: Some(0), map: "Diffuse Color".into(), path: None, amount: None }).unwrap();
    assert!(s.scene.slot_material(0).diffuse_map.is_none());
    assert!(execute(&mut s, Command::SetMaterialMap { slot: Some(0), map: "Sheen".into(), path: None, amount: None }).is_err());
}

#[test]
fn materials_round_trip_through_the_loom_file() {
    let mut s = AppState::new();
    let a = boxed(&mut s);
    execute(&mut s, Command::RenameMaterial { slot: Some(0), name: "Brick".into() }).unwrap();
    execute(&mut s, Command::AssignMaterialToSelection).unwrap();
    let path = std::env::temp_dir().join("loom-material-test.loom");
    write(&path, &Document { format: FORMAT, app: "test".into(), scene: s.scene.clone(), settings: SceneSettings::from_state(&s) }).unwrap();
    let back = read(&path).unwrap().scene;
    let id = back.node(a).unwrap().material.unwrap();
    assert_eq!(back.materials[&id].name, "Brick");
    assert_eq!(back.material_editor.slots[0], Some(id));
}
