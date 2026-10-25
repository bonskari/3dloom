//! Multi/Sub-Object and Blend materials, map types, the Material Editor's
//! levels (Go to Parent / Sibling, the Navigator), drag-and-drop onto
//! objects and selected polygons, the eyedropper, the Browser's Get Material
//! and the editor options.

use loom_commands::file::{read, write, Document, SceneSettings, FORMAT};
use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp, SelectionMode};
use loom_scene::material::{MapType, MaterialKind, NavStep, SubMaterial, Surfaces};
use loom_scene::{MaterialId, NodeId};

fn boxed(s: &mut AppState) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    *s.selection.iter().next().unwrap()
}

fn poly_box(s: &mut AppState) -> NodeId {
    let n = boxed(s);
    execute(s, Command::ConvertToEditablePoly).unwrap();
    n
}

fn select_faces(s: &mut AppState, node: NodeId, faces: Vec<u32>) {
    execute(s, Command::SelectionMode { mode: SelectionMode::Face }).unwrap();
    execute(s, Command::SelectSub { node: Some(node.0), ids: faces, op: SelectOp::Replace }).unwrap();
}

fn diffuse(s: &AppState, id: MaterialId) -> [u8; 3] {
    let d = s.scene.materials[&id].diffuse.get();
    [(d.x * 255.0).round() as u8, (d.y * 255.0).round() as u8, (d.z * 255.0).round() as u8]
}

fn sub_list(s: &AppState, id: MaterialId) -> Vec<SubMaterial> {
    match &s.scene.materials[&id].kind {
        MaterialKind::MultiSub(l) => l.clone(),
        other => panic!("not Multi/Sub: {other:?}"),
    }
}

fn top(s: &AppState) -> MaterialId {
    s.scene.material_editor.slots[s.scene.material_editor.active].unwrap()
}

#[test]
fn type_button_makes_a_multi_sub_and_keeps_the_old_material_as_id_1() {
    let mut s = AppState::new();
    execute(&mut s, Command::SetMaterialColor { slot: None, which: "Diffuse".into(), rgb: [255, 0, 0] }).unwrap();
    execute(&mut s, Command::SetMaterialType { kind: "Multi/Sub-Object".into(), keep_old: Some(true) }).unwrap();
    let id = top(&s);
    let list = sub_list(&s, id);
    assert_eq!(list.len(), 10, "Set Number defaults to 10");
    assert_eq!(list.iter().map(|e| e.id).collect::<Vec<_>>(), (1..=10).collect::<Vec<_>>());
    assert_eq!(diffuse(&s, list[0].material.unwrap()), [255, 0, 0], "the old material is sub-material 1");
    assert_eq!(s.scene.materials[&list[1].material.unwrap()].name, format!("Material #{}", list[1].material.unwrap().0));
    assert_eq!(s.scene.materials[&id].name, "01 - Default", "the slot keeps its name");
    // Discard: a fresh material, not the old one.
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(matches!(s.scene.slot_material(0).kind, MaterialKind::Standard));
    execute(&mut s, Command::SetMaterialType { kind: "Multi/Sub-Object".into(), keep_old: Some(false) }).unwrap();
    let list = sub_list(&s, top(&s));
    assert_ne!(diffuse(&s, list[0].material.unwrap()), [255, 0, 0]);
    assert!(execute(&mut s, Command::SetMaterialType { kind: "Lambert".into(), keep_old: None }).is_err());
}

#[test]
fn set_number_add_delete_and_rows() {
    let mut s = AppState::new();
    execute(&mut s, Command::SetMaterialType { kind: "Multi/Sub-Object".into(), keep_old: None }).unwrap();
    execute(&mut s, Command::MultiSubSetNumber { number: 3 }).unwrap();
    assert_eq!(sub_list(&s, top(&s)).len(), 3);
    execute(&mut s, Command::MultiSubAdd).unwrap();
    assert_eq!(sub_list(&s, top(&s)).last().unwrap().id, 4, "one past the highest ID");
    execute(&mut s, Command::MultiSubDelete { index: 1 }).unwrap();
    assert_eq!(sub_list(&s, top(&s)).iter().map(|e| e.id).collect::<Vec<_>>(), vec![1, 3, 4]);
    execute(&mut s, Command::MultiSubSetEntry { index: 0, id: Some(7), name: Some("Roof".into()), enabled: Some(false), color: Some([0, 0, 255]) }).unwrap();
    let row = sub_list(&s, top(&s))[0].clone();
    assert_eq!((row.id, row.name.as_str(), row.enabled), (7, "Roof", false));
    assert_eq!(diffuse(&s, row.material.unwrap()), [0, 0, 255], "the swatch is the sub-material's Diffuse");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(sub_list(&s, top(&s))[0].id, 1, "one undo step");
    assert!(execute(&mut s, Command::MultiSubDelete { index: 9 }).is_err());
}

#[test]
fn go_to_sub_material_edit_it_and_go_back_to_parent() {
    let mut s = AppState::new();
    execute(&mut s, Command::SetMaterialType { kind: "Multi/Sub-Object".into(), keep_old: None }).unwrap();
    let multi = top(&s);
    execute(&mut s, Command::MaterialEditorGoToSub { index: 1 }).unwrap();
    assert_eq!(s.scene.material_editor.path, vec![NavStep::Sub(1)]);
    // Edits at this level change the sub-material, not the Multi/Sub.
    execute(&mut s, Command::SetMaterialColor { slot: None, which: "Diffuse".into(), rgb: [0, 255, 0] }).unwrap();
    execute(&mut s, Command::RenameMaterial { slot: None, name: "Grass".into() }).unwrap();
    let sub = sub_list(&s, multi)[1].material.unwrap();
    assert_eq!(diffuse(&s, sub), [0, 255, 0]);
    assert_eq!(s.scene.materials[&sub].name, "Grass");
    assert!(matches!(s.scene.materials[&multi].kind, MaterialKind::MultiSub(_)));
    // Go Forward to Sibling: the next sub-material.
    execute(&mut s, Command::MaterialEditorGoForwardToSibling).unwrap();
    assert_eq!(s.scene.material_editor.path, vec![NavStep::Sub(2)]);
    execute(&mut s, Command::MaterialEditorGoToParent).unwrap();
    assert!(s.scene.material_editor.path.is_empty());
    assert!(execute(&mut s, Command::MaterialEditorGoToParent).is_err(), "nothing above the top level");
    assert!(execute(&mut s, Command::MaterialEditorGoForwardToSibling).is_err());
    // Another slot starts at its top level.
    execute(&mut s, Command::MaterialEditorGoToSub { index: 0 }).unwrap();
    execute(&mut s, Command::MaterialEditorSlot { slot: 3 }).unwrap();
    assert!(s.scene.material_editor.path.is_empty());
}

#[test]
fn maps_through_the_browser_with_coordinates_cropping_and_sub_maps() {
    let mut s = AppState::new();
    execute(&mut s, Command::NewMap { map: "Diffuse Color".into(), kind: "Bitmap".into() }).unwrap();
    assert_eq!(s.scene.material_editor.path, vec![NavStep::Map("Diffuse Color".into())], "the editor goes to the new map");
    execute(&mut s, Command::SetMapBitmap { path: "/x/sign.png".into() }).unwrap();
    for (name, v) in [("Tiling U", 2.0), ("Offset V", 0.25), ("Angle W", 30.0), ("Apply", 1.0), ("Place", 1.0), ("U", 0.11), ("V", 0.45), ("W", 0.8), ("H", 0.15)] {
        execute(&mut s, Command::SetMapParameter { name: name.into(), value: v }).unwrap();
    }
    assert!(execute(&mut s, Command::SetMapParameter { name: "Sheen".into(), value: 1.0 }).is_err());
    execute(&mut s, Command::RenameMaterial { slot: None, name: "Sign".into() }).unwrap();
    let m = s.scene.slot_material(0);
    let map = m.diffuse_map.as_ref().unwrap();
    assert_eq!((map.path.as_str(), map.kind, map.name.as_str()), ("/x/sign.png", MapType::Bitmap, "Sign"));
    assert_eq!((map.coords.tiling[0], map.coords.offset[1], map.coords.angle), (2.0, 0.25, 30.0));
    let k = &map.crop;
    assert!(k.apply && k.place && (k.u, k.v, k.w, k.h) == (0.11, 0.45, 0.8, 0.15));
    assert_eq!(m.name, "01 - Default", "renaming at a map level names the map");
    execute(&mut s, Command::MaterialEditorGoToParent).unwrap();
    // The Maps rollout Amount and check box keep the map's settings.
    execute(&mut s, Command::SetMaterialMapAmount { map: "Diffuse Color".into(), amount: 40.0 }).unwrap();
    execute(&mut s, Command::SetMaterialMapEnabled { map: "Diffuse Color".into(), enabled: false }).unwrap();
    let map = s.scene.slot_material(0).diffuse_map.unwrap();
    assert_eq!((map.amount, map.enabled, map.coords.tiling[0]), (40.0, false, 2.0));
    // A Mix map with a Checker in Color #1: two levels down, then the Navigator back up.
    execute(&mut s, Command::NewMap { map: "Bump".into(), kind: "Mix".into() }).unwrap();
    execute(&mut s, Command::NewMap { map: "Color #1".into(), kind: "Checker".into() }).unwrap();
    execute(&mut s, Command::SetMapColor { which: "Color #2".into(), rgb: [255, 0, 0] }).unwrap();
    assert_eq!(s.scene.material_editor.path, vec![NavStep::Map("Bump".into()), NavStep::Map("Color #1".into())]);
    let bump = s.scene.slot_material(0).bump_map.unwrap();
    assert_eq!(bump.kind, MapType::Mix);
    assert_eq!(bump.amount, 30.0, "Bump's default Amount");
    let checker = bump.sub_maps[0].as_ref().unwrap();
    assert_eq!((checker.kind, checker.colors[1]), (MapType::Checker, glam::Vec3::X));
    execute(&mut s, Command::MaterialEditorNavigate { path: vec!["Diffuse Color".into()] }).unwrap();
    assert_eq!(s.scene.material_editor.path, vec![NavStep::Map("Diffuse Color".into())]);
    assert!(execute(&mut s, Command::MaterialEditorNavigate { path: vec!["Opacity".into()] }).is_err(), "no map there");
    // Go Forward to Sibling cycles the maps of one material.
    execute(&mut s, Command::MaterialEditorGoForwardToSibling).unwrap();
    assert_eq!(s.scene.material_editor.path, vec![NavStep::Map("Bump".into())]);
    // Type at a map level, keeping the old map as Color #1.
    execute(&mut s, Command::SetMapType { kind: "Noise".into(), keep_old: Some(true) }).unwrap();
    let bump = s.scene.slot_material(0).bump_map.unwrap();
    assert_eq!((bump.kind, bump.sub_maps[0].as_ref().map(|m| m.kind)), (MapType::Noise, Some(MapType::Mix)));
    // Copy a map button onto another, and Swap.
    execute(&mut s, Command::MaterialEditorGoToParent).unwrap();
    execute(&mut s, Command::CopyMap { from: "Diffuse Color".into(), to: "Opacity".into(), swap: false }).unwrap();
    assert_eq!(s.scene.slot_material(0).opacity_map.unwrap().path, "/x/sign.png");
    execute(&mut s, Command::CopyMap { from: "Diffuse Color".into(), to: "Bump".into(), swap: true }).unwrap();
    let m = s.scene.slot_material(0);
    assert_eq!((m.diffuse_map.unwrap().kind, m.bump_map.unwrap().kind), (MapType::Noise, MapType::Bitmap));
}

#[test]
fn blend_keeps_the_old_material_and_takes_a_mask() {
    let mut s = AppState::new();
    execute(&mut s, Command::SetMaterialType { kind: "Multi/Sub-Object".into(), keep_old: None }).unwrap();
    execute(&mut s, Command::MaterialEditorGoToSub { index: 1 }).unwrap();
    execute(&mut s, Command::SetMaterialColor { slot: None, which: "Diffuse".into(), rgb: [10, 20, 30] }).unwrap();
    let old = sub_list(&s, top(&s))[1].material.unwrap();
    // As in the tutorial: the sub-material becomes a Blend that keeps it as Material 1.
    execute(&mut s, Command::SetMaterialType { kind: "Blend".into(), keep_old: Some(true) }).unwrap();
    let MaterialKind::Blend(b) = s.scene.materials[&old].kind.clone() else { panic!("a Blend") };
    assert_eq!(diffuse(&s, b.materials[0].unwrap()), [10, 20, 30]);
    assert_eq!(s.scene.material_editor.path, vec![NavStep::Sub(1)], "still at the same level, now a Blend");
    execute(&mut s, Command::NewMap { map: "Mask".into(), kind: "Bitmap".into() }).unwrap();
    execute(&mut s, Command::SetMapBitmap { path: "/x/mask.png".into() }).unwrap();
    execute(&mut s, Command::MaterialEditorGoToParent).unwrap();
    execute(&mut s, Command::SetMaterialParameter { slot: None, name: "Mix Amount".into(), value: 140.0 }).unwrap();
    let MaterialKind::Blend(b) = s.scene.materials[&old].kind.clone() else { panic!() };
    assert_eq!(b.mask.unwrap().path, "/x/mask.png");
    assert_eq!(b.mix_amount.get(), 100.0, "clamped");
    // Material 2's diffuse map, then Go to Parent three times reaches the top.
    execute(&mut s, Command::MaterialEditorGoToSub { index: 1 }).unwrap();
    assert_eq!(s.scene.material_editor.path.len(), 2);
    execute(&mut s, Command::NewMap { map: "Diffuse Color".into(), kind: "Bitmap".into() }).unwrap();
    for _ in 0..3 {
        execute(&mut s, Command::MaterialEditorGoToParent).unwrap();
    }
    assert!(s.scene.material_editor.path.is_empty());
}

#[test]
fn faces_draw_with_their_material_id_sub_material() {
    let mut s = AppState::new();
    let n = poly_box(&mut s);
    execute(&mut s, Command::SetMaterialType { kind: "Multi/Sub-Object".into(), keep_old: None }).unwrap();
    execute(&mut s, Command::MultiSubSetNumber { number: 2 }).unwrap();
    execute(&mut s, Command::MultiSubSetEntry { index: 0, id: None, name: None, enabled: None, color: Some([255, 0, 0]) }).unwrap();
    execute(&mut s, Command::MultiSubSetEntry { index: 1, id: None, name: None, enabled: None, color: Some([0, 0, 255]) }).unwrap();
    execute(&mut s, Command::AssignMaterialToSelection).unwrap();
    select_faces(&mut s, n, vec![0, 1]);
    execute(&mut s, Command::SetMaterialId { id: 2 }).unwrap();
    select_faces(&mut s, n, vec![2, 3, 4, 5]);
    execute(&mut s, Command::SetMaterialId { id: 1 }).unwrap();
    let Some(Surfaces::PerId(per)) = s.scene.node_surfaces(n) else { panic!("per-ID surfaces") };
    assert_eq!((per[&0].diffuse, per[&1].diffuse), (glam::Vec3::X, glam::Vec3::Z));
    let mesh = s.scene.node(n).unwrap().cage_mesh().into_owned();
    assert_eq!(mesh.faces()[0].material_id, 1);
    let surfaces = s.scene.node_surfaces(n).unwrap();
    assert_eq!(surfaces.for_face(mesh.faces()[0].material_id).diffuse, glam::Vec3::Z, "Set ID 2 faces are blue");
    assert_eq!(surfaces.for_face(mesh.faces()[3].material_id).diffuse, glam::Vec3::X);
    // Select ID finds them again.
    execute(&mut s, Command::SelectByMaterialId { id: 2, clear: true }).unwrap();
    assert_eq!(s.sub_selection.by_node().map(|(_, f)| f.len()).sum::<usize>(), 2);
}

#[test]
fn dropping_materials_on_selected_polygons_builds_a_multi_sub() {
    let mut s = AppState::new();
    let n = poly_box(&mut s);
    // Slot 0 red on the whole box, slot 1 blue dropped on two faces.
    execute(&mut s, Command::SetMaterialColor { slot: Some(0), which: "Diffuse".into(), rgb: [255, 0, 0] }).unwrap();
    execute(&mut s, Command::SetMaterialColor { slot: Some(1), which: "Diffuse".into(), rgb: [0, 0, 255] }).unwrap();
    execute(&mut s, Command::AssignMaterialToNode { node: n.0, slot: Some(0) }).unwrap();
    let red = s.scene.node(n).unwrap().material.unwrap();
    select_faces(&mut s, n, vec![0, 1]);
    let undo = s.undo.len();
    execute(&mut s, Command::AssignMaterialToFaces { slot: Some(1) }).unwrap();
    assert_eq!(s.undo.len(), undo + 1, "one undo step");
    let multi = s.scene.node(n).unwrap().material.unwrap();
    let list = sub_list(&s, multi);
    assert_eq!(list.iter().map(|e| (e.id, e.material)).collect::<Vec<_>>(), vec![(1, Some(red)), (2, s.scene.material_editor.slots[1])], "the old material is ID 1, the dropped one ID 2");
    let mesh = s.scene.node(n).unwrap().cage_mesh().into_owned();
    let ids: Vec<u16> = mesh.faces().iter().map(|f| f.material_id + 1).collect();
    assert_eq!(ids, vec![2, 2, 1, 1, 1, 1]);
    // A third material on one more face adds ID 3 to the same Multi/Sub.
    execute(&mut s, Command::SetMaterialColor { slot: Some(2), which: "Diffuse".into(), rgb: [0, 255, 0] }).unwrap();
    select_faces(&mut s, n, vec![4]);
    execute(&mut s, Command::AssignMaterialToFaces { slot: Some(2) }).unwrap();
    assert_eq!(s.scene.node(n).unwrap().material, Some(multi));
    let mesh = s.scene.node(n).unwrap().cage_mesh().into_owned();
    assert_eq!(mesh.faces()[4].material_id + 1, 3);
    assert_eq!(s.scene.node_surfaces(n).unwrap().for_face(2).diffuse, glam::Vec3::Y);
    // The eyedropper over faces of one ID picks that sub-material.
    execute(&mut s, Command::MaterialEditorSlot { slot: 5 }).unwrap();
    select_faces(&mut s, n, vec![0]);
    execute(&mut s, Command::GetMaterialFromObject { node: Some(n.0) }).unwrap();
    assert_eq!(s.scene.material_editor.slots[5], s.scene.material_editor.slots[1]);
    // Undo takes back the drops: the one material and the IDs as they were.
    let mut steps = 0;
    while s.scene.node(n).unwrap().material != Some(red) && steps < 10 {
        execute(&mut s, Command::UndoSceneOperation).unwrap();
        steps += 1;
    }
    assert_eq!(s.scene.node(n).unwrap().material, Some(red));
    let mesh = s.scene.node(n).unwrap().cage_mesh().into_owned();
    assert!(mesh.faces().iter().all(|f| f.material_id == mesh.faces()[0].material_id), "the IDs are back");
}

#[test]
fn eyedropper_slot_copy_browser_get_and_options() {
    let mut s = AppState::new();
    let n = boxed(&mut s);
    execute(&mut s, Command::SetMaterialType { kind: "Blend".into(), keep_old: None }).unwrap();
    execute(&mut s, Command::AssignMaterialToSelection).unwrap();
    let blend = top(&s);
    // Pick Material From Object: the next clicked object.
    execute(&mut s, Command::MaterialEditorSlot { slot: 4 }).unwrap();
    execute(&mut s, Command::PickMaterialFromObject).unwrap();
    assert_eq!(s.pick_mode, Some(loom_commands::PickMode::PickMaterial));
    execute(&mut s, Command::PickHit { id: n.0 }).unwrap();
    assert_eq!(s.scene.material_editor.slots[4], Some(blend));
    assert_eq!(s.pick_mode, None);
    // Drag a slot onto another: a copy with its own sub-materials.
    execute(&mut s, Command::CopyMaterialToSlot { from: 4, to: 7 }).unwrap();
    let copy = s.scene.material_editor.slots[7].unwrap();
    assert_ne!(copy, blend);
    let (MaterialKind::Blend(a), MaterialKind::Blend(b)) = (&s.scene.materials[&blend].kind, &s.scene.materials[&copy].kind) else { panic!() };
    assert_ne!(a.materials[0], b.materials[0]);
    // Browser > Get Material from the scene.
    execute(&mut s, Command::MaterialEditorSlot { slot: 9 }).unwrap();
    execute(&mut s, Command::GetMaterial { material: blend.0 }).unwrap();
    assert_eq!(s.scene.material_editor.slots[9], Some(blend));
    assert!(execute(&mut s, Command::GetMaterial { material: 999 }).is_err());
    // Options and the Sample Type flyout.
    execute(&mut s, Command::SetMaterialEditorOption { name: "Top Light Multiplier".into(), value: 2.0 }).unwrap();
    execute(&mut s, Command::SetMaterialEditorOption { name: "Slots".into(), value: 24.0 }).unwrap();
    assert!(execute(&mut s, Command::SetMaterialEditorOption { name: "Slots".into(), value: 12.0 }).is_err());
    execute(&mut s, Command::SetSampleType { kind: "Cube".into() }).unwrap();
    let ed = &s.scene.material_editor;
    assert_eq!((ed.options.top_multiplier, ed.options.slots_shown), (2.0, 24));
    assert_eq!(ed.sample_type(9), loom_scene::material::SampleType::Cube);
    assert_eq!(ed.sample_type(0), loom_scene::material::SampleType::Sphere);
}

#[test]
fn compound_materials_round_trip_through_the_loom_file() {
    let mut s = AppState::new();
    let a = poly_box(&mut s);
    execute(&mut s, Command::SetMaterialType { kind: "Multi/Sub-Object".into(), keep_old: None }).unwrap();
    execute(&mut s, Command::MaterialEditorGoToSub { index: 0 }).unwrap();
    execute(&mut s, Command::NewMap { map: "Diffuse Color".into(), kind: "Checker".into() }).unwrap();
    execute(&mut s, Command::SetMapParameter { name: "Tiling U".into(), value: 4.0 }).unwrap();
    execute(&mut s, Command::AssignMaterialToNode { node: a.0, slot: None }).unwrap();
    let path = std::env::temp_dir().join("loom-material-multi-test.loom");
    write(&path, &Document { format: FORMAT, app: "test".into(), scene: s.scene.clone(), settings: SceneSettings::from_state(&s) }).unwrap();
    let back = read(&path).unwrap().scene;
    assert_eq!(back.materials, s.scene.materials);
    assert_eq!(back.material_editor, s.scene.material_editor);
    assert!(matches!(back.node_surfaces(a), Some(Surfaces::PerId(_))));
}

#[test]
fn a_bitmap_dropped_on_selected_polygons_gets_its_own_sub_material() {
    let mut s = AppState::new();
    let n = poly_box(&mut s);
    select_faces(&mut s, n, vec![0]);
    let undo = s.undo.len();
    execute(&mut s, Command::DropBitmap { path: "/x/bfrontgarage.jpg".into(), node: Some(n.0) }).unwrap();
    assert_eq!(s.undo.len(), undo + 1, "one undo step");
    let multi = s.scene.node(n).unwrap().material.expect("a Multi/Sub-Object material");
    let list = sub_list(&s, multi);
    let front = list.iter().find(|e| e.material.is_some_and(|m| s.scene.materials[&m].name == "bfrontgarage")).expect("the bitmap's material");
    let m = &s.scene.materials[&front.material.unwrap()];
    assert_eq!(m.diffuse_map.as_ref().unwrap().path, "/x/bfrontgarage.jpg");
    assert!(m.show_map_in_viewport);
    assert_eq!(s.scene.material_editor.slots[0], front.material, "in the first unused sample slot");
    let mesh = s.scene.node(n).unwrap().cage_mesh().into_owned();
    assert_eq!(mesh.faces()[0].material_id + 1, front.id);
    // A second bitmap on another polygon: another ID in the same material.
    select_faces(&mut s, n, vec![1]);
    execute(&mut s, Command::DropBitmap { path: "/x/bsidegarage.jpg".into(), node: Some(n.0) }).unwrap();
    assert_eq!(s.scene.node(n).unwrap().material, Some(multi));
    assert_eq!(sub_list(&s, multi).len(), list.len() + 1);
    assert_eq!(s.scene.material_editor.slots[1].map(|m| s.scene.materials[&m].name.clone()), Some("bsidegarage".to_string()));
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(sub_list(&s, multi).len(), list.len());
}
