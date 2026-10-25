//! Merge duplicate names and layers, layer properties, Save Selected,
//! New Scene options, File Properties.

use loom_commands::{execute, AppState, Command, MergeDuplicate, PrimitiveKind, SelectOp};
use loom_scene::NodeId;
use std::path::PathBuf;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join("loom-scene-files-tests").join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn p(path: &std::path::Path) -> Option<String> {
    Some(path.to_string_lossy().into_owned())
}

fn named_box(s: &mut AppState, name: &str, x: f32) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [x, 0.0, 0.0], size: [10.0; 3] }).unwrap();
    let id = *s.selection.iter().next().unwrap();
    execute(s, Command::Rename { id: id.0, name: name.into() }).unwrap();
    id
}

fn names(s: &AppState) -> Vec<String> {
    let mut v: Vec<String> = s.scene.nodes().map(|n| n.name.clone()).collect();
    v.sort();
    v
}

/// A file with Blade (x = 100) and Hub.
fn blades(path: &std::path::Path) {
    let mut s = AppState::new();
    named_box(&mut s, "Blade", 100.0);
    named_box(&mut s, "Hub", 120.0);
    execute(&mut s, Command::SaveFile { path: p(path) }).unwrap();
}

fn dup(name: &str, action: &str, new_name: Option<&str>) -> MergeDuplicate {
    MergeDuplicate { name: name.into(), action: action.into(), new_name: new_name.map(Into::into) }
}

#[test]
fn merge_duplicate_names_skip_merge_delete_old_rename_and_auto_rename() {
    let d = dir("dups");
    let path = d.join("src.loom");
    blades(&path);
    let merge = |s: &mut AppState, dups: Vec<MergeDuplicate>| execute(s, Command::MergeFile { path: p(&path), names: Some(vec!["Blade".into(), "Hub".into()]), duplicates: Some(dups) });

    // Skip: the scene's Blade stays alone, Hub comes in.
    let mut s = AppState::new();
    named_box(&mut s, "Blade", 0.0);
    merge(&mut s, vec![dup("Blade", "skip", None)]).unwrap();
    assert_eq!(names(&s), ["Blade", "Hub"]);

    // Merge: both Blades, same name.
    let mut s = AppState::new();
    named_box(&mut s, "Blade", 0.0);
    merge(&mut s, vec![dup("Blade", "merge", None)]).unwrap();
    assert_eq!(names(&s), ["Blade", "Blade", "Hub"]);

    // Delete Old: the scene's Blade goes, the file's (x = 100) takes its place; one undo brings it back.
    let mut s = AppState::new();
    let old = named_box(&mut s, "Blade", 0.0);
    merge(&mut s, vec![dup("Blade", "delete_old", None)]).unwrap();
    assert_eq!(names(&s), ["Blade", "Hub"]);
    assert!(s.scene.node(old).is_none());
    let blade = s.scene.nodes().find(|n| n.name == "Blade").unwrap().id;
    assert!((s.scene.world_matrix(blade).w_axis.x - 100.0).abs() < 1e-3);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(s.scene.node(old).is_some());
    assert_eq!(names(&s), ["Blade"]);

    // Rename: the new name typed in the dialog.
    let mut s = AppState::new();
    named_box(&mut s, "Blade", 0.0);
    merge(&mut s, vec![dup("Blade", "rename", Some("NewBlade"))]).unwrap();
    assert_eq!(names(&s), ["Blade", "Hub", "NewBlade"]);

    // Auto-Rename (also what happens without an answer): Blade001.
    let mut s = AppState::new();
    named_box(&mut s, "Blade", 0.0);
    merge(&mut s, vec![dup("Blade", "auto_rename", None)]).unwrap();
    assert_eq!(names(&s), ["Blade", "Blade001", "Hub"]);
    let mut s = AppState::new();
    named_box(&mut s, "Blade", 0.0);
    execute(&mut s, Command::MergeFile { path: p(&path), names: None, duplicates: None }).unwrap();
    assert_eq!(names(&s), ["Blade", "Blade001", "Hub"]);

    // A rename onto a name that is taken, or an unknown answer, is refused.
    let mut s = AppState::new();
    named_box(&mut s, "Blade", 0.0);
    assert!(merge(&mut s, vec![dup("Blade", "rename", Some("Blade"))]).is_err());
    assert!(merge(&mut s, vec![dup("Blade", "maybe", None)]).is_err());
    assert_eq!(names(&s), ["Blade"]);
}

#[test]
fn merged_objects_keep_their_layers_by_name() {
    let d = dir("layers");
    let path = d.join("src.loom");
    {
        let mut s = AppState::new();
        named_box(&mut s, "Wheel", 0.0);
        execute(&mut s, Command::CreateLayer { name: Some("Props".into()) }).unwrap();
        named_box(&mut s, "Crate", 20.0);
        execute(&mut s, Command::CreateLayer { name: Some("Walls".into()) }).unwrap();
        execute(&mut s, Command::SaveFile { path: p(&path) }).unwrap();
    }
    let mut s = AppState::new();
    execute(&mut s, Command::CreateLayer { name: Some("Walls".into()) }).unwrap();
    let walls = s.scene.current_layer;
    execute(&mut s, Command::SetCurrentLayer { layer: 0 }).unwrap();
    execute(&mut s, Command::MergeFile { path: p(&path), names: None, duplicates: None }).unwrap();
    let layer_of = |s: &AppState, n: &str| {
        let l = s.scene.nodes().find(|x| x.name == n).unwrap().layer;
        s.scene.layer(l).unwrap().name.clone()
    };
    assert_eq!(layer_of(&s, "Crate"), "Walls", "an existing layer of that name is reused");
    assert_eq!(s.scene.nodes().find(|x| x.name == "Crate").unwrap().layer, walls);
    assert_eq!(layer_of(&s, "Wheel"), "Props", "a missing layer is created");
    assert_eq!(s.scene.layers.iter().filter(|l| l.name == "Walls").count(), 1);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(!s.scene.layers.iter().any(|l| l.name == "Props"), "undo removes the created layer");
}

#[test]
fn set_current_layer_to_selections_layer() {
    let mut s = AppState::new();
    let a = named_box(&mut s, "A", 0.0);
    execute(&mut s, Command::CreateLayer { name: Some("Props".into()) }).unwrap();
    let props = s.scene.current_layer;
    execute(&mut s, Command::SetCurrentLayer { layer: 0 }).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![a.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SetCurrentLayerToSelection).unwrap();
    assert_eq!(s.scene.current_layer, props);
}

#[test]
fn layer_color_render_and_objects_by_layer() {
    let mut s = AppState::new();
    let a = named_box(&mut s, "A", 0.0);
    execute(&mut s, Command::CreateLayer { name: Some("Props".into()) }).unwrap();
    let props = s.scene.current_layer;
    let own = s.scene.node(a).unwrap().wire_color;
    execute(&mut s, Command::SetLayerColor { layer: props, rgb: [10, 200, 30] }).unwrap();
    assert_eq!(s.scene.layer(props).unwrap().color, [10, 200, 30]);
    assert_eq!(s.scene.display_color(s.scene.node(a).unwrap()), own, "By Object: the object's own colour");
    execute(&mut s, Command::SetColorByLayer { on: true }).unwrap();
    assert_eq!(s.scene.display_color(s.scene.node(a).unwrap()), [10, 200, 30], "By Layer: the layer's colour");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.display_color(s.scene.node(a).unwrap()), own);
    assert!(s.scene.is_renderable(s.scene.node(a).unwrap()));
    execute(&mut s, Command::SetLayerRenderable { layer: props, on: false }).unwrap();
    assert!(!s.scene.is_renderable(s.scene.node(a).unwrap()));
    // New layers get different colours.
    execute(&mut s, Command::CreateLayer { name: Some("Walls".into()) }).unwrap();
    assert_ne!(s.scene.layer(s.scene.current_layer).unwrap().color, s.scene.layer(0).unwrap().color);
}

#[test]
fn hide_and_freeze_all_layers_in_one_step() {
    let mut s = AppState::new();
    named_box(&mut s, "A", 0.0);
    execute(&mut s, Command::CreateLayer { name: Some("Props".into()) }).unwrap();
    let before = s.undo.len();
    execute(&mut s, Command::SetAllLayersHidden { on: true }).unwrap();
    assert!(s.scene.layers.iter().all(|l| l.hidden));
    assert!(s.selection.is_empty(), "hidden objects leave the selection");
    execute(&mut s, Command::SetAllLayersHidden { on: false }).unwrap();
    execute(&mut s, Command::SetAllLayersFrozen { on: true }).unwrap();
    assert!(s.scene.layers.iter().all(|l| l.frozen));
    assert_eq!(s.undo.len(), before + 3);
}

#[test]
fn layer_colors_and_file_properties_are_saved_in_the_scene_file() {
    let d = dir("props");
    let path = d.join("scene.loom");
    let mut s = AppState::new();
    named_box(&mut s, "A", 0.0);
    execute(&mut s, Command::CreateLayer { name: Some("Props".into()) }).unwrap();
    let props = s.scene.current_layer;
    execute(&mut s, Command::SetLayerColor { layer: props, rgb: [1, 2, 3] }).unwrap();
    execute(&mut s, Command::SetLayerRenderable { layer: props, on: false }).unwrap();
    execute(&mut s, Command::SetFileProperty { name: "Author".into(), value: "Johannes".into() }).unwrap();
    execute(&mut s, Command::SetFileProperty { name: "Description".into(), value: "A test scene".into() }).unwrap();
    execute(&mut s, Command::SetCustomFileProperty { name: "Level".into(), value: Some("3".into()) }).unwrap();
    assert!(s.dirty(), "a property edit is unsaved work");
    assert!(execute(&mut s, Command::SetFileProperty { name: "Nope".into(), value: "x".into() }).is_err());
    execute(&mut s, Command::SaveFile { path: p(&path) }).unwrap();
    let mut t = AppState::new();
    execute(&mut t, Command::OpenFile { path: p(&path) }).unwrap();
    let l = t.scene.layer(props).unwrap();
    assert_eq!((l.color, l.renderable), ([1, 2, 3], false));
    assert_eq!(t.file_properties.author, "Johannes");
    assert_eq!(t.file_properties.description, "A test scene");
    assert_eq!(t.file_properties.custom.get("Level").map(String::as_str), Some("3"));
    execute(&mut t, Command::SetCustomFileProperty { name: "Level".into(), value: None }).unwrap();
    assert!(t.file_properties.custom.is_empty());
    let info = execute(&mut t, Command::SummaryInfo).unwrap();
    assert_eq!(info["objects"], 1);
    assert_eq!(info["faces"], 6);
    assert_eq!(info["description"], "A test scene");
    assert_eq!(t.dialog, Some(loom_commands::Dialog::SummaryInfo));
}

#[test]
fn new_scene_keeps_objects_and_hierarchy_or_only_objects() {
    let mut s = AppState::new();
    let parent = named_box(&mut s, "P", 0.0);
    let child = named_box(&mut s, "C", 20.0);
    execute(&mut s, Command::SetParent { id: child.0, parent: Some(parent.0) }).unwrap();
    // A key on the child.
    execute(&mut s, Command::SelectNodes { ids: vec![child.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    execute(&mut s, Command::SetFrame { frame: 10 }).unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 5.0], system: None }).unwrap();
    execute(&mut s, Command::AutoKeyModeToggle).unwrap();
    assert!(s.scene.node(child).unwrap().transform.position.is_animated());
    let world = s.scene.world_matrix(child);
    execute(&mut s, Command::NewSceneKeep { keep: "objects_and_hierarchy".into() }).unwrap();
    assert_eq!(s.scene.len(), 2);
    assert_eq!(s.scene.node(child).unwrap().parent, Some(parent), "the hierarchy stays");
    assert!(!s.scene.node(child).unwrap().transform.position.is_animated(), "the keys go");
    assert!(s.file.is_none() && !s.dirty() && !s.undo.can_undo());
    execute(&mut s, Command::NewSceneKeep { keep: "objects".into() }).unwrap();
    assert_eq!(s.scene.node(child).unwrap().parent, None, "the links go");
    assert!((s.scene.world_matrix(child).w_axis - world.w_axis).length() < 1e-3, "the object stays where it was");
    execute(&mut s, Command::NewSceneKeep { keep: "all".into() }).unwrap();
    assert_eq!(s.scene.len(), 0);
    assert!(execute(&mut s, Command::NewSceneKeep { keep: "some".into() }).is_err());
}

#[test]
fn reset_asks_to_confirm_after_the_save_prompt() {
    use loom_commands::{Dialog, PendingFileAction};
    let mut s = AppState::new();
    named_box(&mut s, "A", 0.0);
    s.dialog = Some(Dialog::SaveChanges(PendingFileAction::Reset));
    execute(&mut s, Command::SaveChangesReply { reply: loom_commands::SaveChangesReply::No }).unwrap();
    assert_eq!(s.dialog, Some(Dialog::ConfirmReset));
    assert_eq!(s.scene.len(), 1, "nothing is reset before Yes");
    execute(&mut s, Command::ResetFile).unwrap();
    assert_eq!(s.scene.len(), 0);
}

#[test]
fn save_selected_writes_the_selection_with_its_ancestors() {
    let d = dir("save_selected");
    let path = d.join("part.loom");
    let mut s = AppState::new();
    let root = named_box(&mut s, "Root", 0.0);
    let mid = named_box(&mut s, "Mid", 20.0);
    let leaf = named_box(&mut s, "Leaf", 40.0);
    let other = named_box(&mut s, "Other", 60.0);
    execute(&mut s, Command::SetParent { id: mid.0, parent: Some(root.0) }).unwrap();
    execute(&mut s, Command::SetParent { id: leaf.0, parent: Some(mid.0) }).unwrap();
    // Inst instances Other, which stays behind.
    execute(&mut s, Command::SelectNodes { ids: vec![other.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::CloneOptions { kind: loom_commands::CloneKind::Instance, copies: 1, name: Some("Inst".into()), offset: Some([0.0, 30.0, 0.0]), rotate_deg: None, scale: None }).unwrap();
    let inst = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![leaf.0, inst.0], op: SelectOp::Replace }).unwrap();
    let r = execute(&mut s, Command::SaveSelected { path: p(&path) }).unwrap();
    assert_eq!(r["nodes"], 4);
    assert!(s.file.is_none(), "the current file does not change");
    assert_eq!(s.scene.len(), 5);
    let mut t = AppState::new();
    execute(&mut t, Command::OpenFile { path: p(&path) }).unwrap();
    let mut got: Vec<String> = t.scene.nodes().map(|n| n.name.clone()).collect();
    got.sort();
    assert_eq!(got, ["Inst", "Leaf", "Mid", "Root"]);
    let i = t.scene.nodes().find(|n| n.name == "Inst").unwrap();
    assert!(i.instance_of.is_none(), "an instance of an object left behind becomes a copy");
    assert_eq!(t.scene.eval_mesh(i.id).value.mesh.face_count(), 6);
    let l = t.scene.nodes().find(|n| n.name == "Leaf").unwrap();
    assert!((t.scene.world_matrix(l.id).w_axis.x - 40.0).abs() < 1e-3);
    execute(&mut s, Command::SelectNodes { ids: vec![], op: SelectOp::Replace }).unwrap();
    assert!(execute(&mut s, Command::SaveSelected { path: p(&path) }).is_err());
    execute(&mut s, Command::SaveSelected { path: None }).unwrap();
    assert_eq!(s.dialog, Some(loom_commands::Dialog::SaveSelected));
}

#[test]
fn archive_zips_the_scene_and_its_bitmaps_or_lists_them() {
    let d = dir("archive");
    let tex = d.join("crate_diffuse.png");
    image::RgbImage::from_pixel(4, 4, image::Rgb([200, 100, 50])).save(&tex).unwrap();
    let mut s = AppState::new();
    let a = named_box(&mut s, "A", 0.0);
    execute(&mut s, Command::SetTextureDisplay { id: a.0, kind: "bitmap".into(), path: p(&tex) }).unwrap();
    execute(&mut s, Command::SaveFile { path: p(&d.join("level.loom")) }).unwrap();
    let zip = d.join("level.zip");
    let r = execute(&mut s, Command::ArchiveFile { path: p(&zip) }).unwrap();
    assert_eq!(r["files"], serde_json::json!(["level.loom", "maps/crate_diffuse.png"]));
    let bytes = std::fs::read(&zip).unwrap();
    assert_eq!(&bytes[..4], b"PK\x03\x04");
    assert_eq!(&bytes[bytes.len() - 22..bytes.len() - 18], b"PK\x05\x06", "an end of central directory record closes it");
    let list = d.join("level.txt");
    execute(&mut s, Command::ArchiveFile { path: p(&list) }).unwrap();
    let text = std::fs::read_to_string(&list).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines, [d.join("level.loom").to_string_lossy().as_ref(), tex.to_string_lossy().as_ref()]);
}

#[test]
fn archive_entries_inflate_back_to_the_files() {
    use std::io::Read;
    let data = b"hello hello hello archive".to_vec();
    let bytes = loom_commands::scene_file::zip(&[("a.txt".into(), data.clone())]).unwrap();
    // Local header: 30 bytes + name, then the deflated data.
    let crc = u32::from_le_bytes(bytes[14..18].try_into().unwrap());
    let packed_len = u32::from_le_bytes(bytes[18..22].try_into().unwrap()) as usize;
    let start = 30 + "a.txt".len();
    let mut out = Vec::new();
    flate2::read::DeflateDecoder::new(&bytes[start..start + packed_len]).read_to_end(&mut out).unwrap();
    assert_eq!(out, data);
    assert_eq!(crc, crc32fast::hash(&data));
}

#[test]
fn layers_toolbar_toggles() {
    let mut s = AppState::new();
    assert!(!s.show_layers_toolbar);
    execute(&mut s, Command::ShowLayersToolbarToggle).unwrap();
    assert!(s.show_layers_toolbar);
}

#[test]
fn fbx_export_and_import_keep_the_layers() {
    let d = dir("fbx_layers");
    let path = d.join("level.fbx");
    {
        let mut s = AppState::new();
        named_box(&mut s, "Floor", 0.0);
        named_box(&mut s, "Crate", 20.0);
        execute(&mut s, Command::CreateLayer { name: Some("Props".into()) }).unwrap();
        let props = s.scene.current_layer;
        execute(&mut s, Command::SetLayerColor { layer: props, rgb: [255, 128, 0] }).unwrap();
        execute(&mut s, Command::ExportFile { path: p(&path) }).unwrap();
    }
    let mut s = AppState::new();
    execute(&mut s, Command::ImportFile { path: p(&path) }).unwrap();
    let layer_name = |s: &AppState, n: &str| {
        let l = s.scene.nodes().find(|x| x.name == n).unwrap().layer;
        s.scene.layer(l).unwrap().clone()
    };
    assert_eq!(layer_name(&s, "Floor").id, 0, "layer 0 stays layer 0");
    let props = layer_name(&s, "Crate");
    assert_eq!(props.name, "Props");
    assert_eq!(props.color, [255, 128, 0]);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.layers.len(), 1, "one undo removes the imported layer too");
}

