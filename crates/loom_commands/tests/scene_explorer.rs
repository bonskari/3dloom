//! Scene Explorer and Schematic View: the windows' setups, the rows and
//! graph they show, and the scene edits they make (one undo step each).

use loom_commands::explorer::{self, Column, SearchTerm};
use loom_commands::schematic::{self, EntityKind, SchMode, WireKind};
use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp};
use loom_scene::NodeId;

fn named_box(s: &mut AppState, name: &str, x: f32) -> NodeId {
    execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [x, 0.0, 0.0], size: [10.0; 3] }).unwrap();
    let id = *s.selection.iter().next().unwrap();
    execute(s, Command::Rename { id: id.0, name: name.into() }).unwrap();
    id
}

fn names(s: &AppState, rows: &[explorer::Row]) -> Vec<(String, usize)> {
    rows.iter().map(|r| (s.scene.node(r.id).unwrap().name.clone(), r.depth)).collect()
}

#[test]
fn each_new_scene_explorer_is_another_window_and_open_explorer_reopens_the_last() {
    let mut s = AppState::new();
    let r = execute(&mut s, Command::NewSceneExplorer).unwrap();
    assert_eq!(r["name"], "Scene Explorer 1");
    execute(&mut s, Command::NewSceneExplorer).unwrap();
    assert_eq!(s.explorers.list.len(), 2);
    assert!(s.explorers.list.iter().all(|e| e.open));
    assert_ne!(s.explorers.list[0].id, s.explorers.list[1].id);
    execute(&mut s, Command::CloseSceneExplorer { name: None }).unwrap();
    assert!(!s.explorers.list[1].open, "closing leaves it saved");
    execute(&mut s, Command::OpenSceneExplorer { name: None }).unwrap();
    assert!(s.explorers.list[1].open, "Open Explorer reopens the most recent one");
    execute(&mut s, Command::RenameSceneExplorer { explorer: Some("Scene Explorer 1".into()), name: "Lights".into() }).unwrap();
    assert!(execute(&mut s, Command::RenameSceneExplorer { explorer: None, name: "Lights".into() }).is_err(), "names stay unique");
    execute(&mut s, Command::DeleteSceneExplorer { explorer: "Lights".into() }).unwrap();
    assert_eq!(s.explorers.list.len(), 1);
}

#[test]
fn the_default_setup_goes_to_new_explorers() {
    let mut s = AppState::new();
    execute(&mut s, Command::NewSceneExplorer).unwrap();
    execute(&mut s, Command::SceneExplorerColumns { explorer: None, columns: vec!["Name".into(), "Faces".into(), "Hidden".into()] }).unwrap();
    execute(&mut s, Command::SetDefaultSceneExplorer { explorer: "Scene Explorer 1".into() }).unwrap();
    execute(&mut s, Command::NewSceneExplorer).unwrap();
    let e = s.explorers.get(None).unwrap();
    assert_eq!(e.name, "Scene Explorer 2");
    assert_eq!(e.columns, vec![Column::Name, Column::Faces, Column::Hidden]);
}

#[test]
fn explorers_and_schematic_views_are_saved_with_the_scene() {
    let dir = std::env::temp_dir().join(format!("loom_explorer_save_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("explorers.loom");
    let mut s = AppState::new();
    named_box(&mut s, "A", 0.0);
    execute(&mut s, Command::NewSceneExplorer).unwrap();
    execute(&mut s, Command::SceneExplorerColumns { explorer: None, columns: vec!["Name".into(), "Renderable".into()] }).unwrap();
    execute(&mut s, Command::SceneExplorerOption { explorer: None, option: "Cameras".into(), on: false }).unwrap();
    execute(&mut s, Command::NewSchematicView).unwrap();
    execute(&mut s, Command::SchematicOption { view: None, option: "References Mode".into(), on: true }).unwrap();
    execute(&mut s, Command::SaveFileAs { path: Some(path.display().to_string()) }).unwrap();
    let mut t = AppState::new();
    execute(&mut t, Command::OpenFile { path: Some(path.display().to_string()) }).unwrap();
    let e = t.explorers.get(Some("Scene Explorer 1")).expect("the explorer came back");
    assert_eq!(e.columns, vec![Column::Name, Column::Renderable]);
    assert!(!e.shown.contains(&explorer::Category::Camera));
    assert_eq!(t.schematics.get(Some("Schematic View 1")).unwrap().mode, SchMode::References);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rows_hang_children_under_parents_and_sorting_keeps_hierarchies_together() {
    let mut s = AppState::new();
    let b = named_box(&mut s, "B", 0.0);
    let a = named_box(&mut s, "A", 20.0);
    let c = named_box(&mut s, "C", 40.0);
    execute(&mut s, Command::SetParent { id: c.0, parent: Some(b.0) }).unwrap();
    execute(&mut s, Command::NewSceneExplorer).unwrap();
    let e = s.explorers.get(None).unwrap().clone();
    assert_eq!(names(&s, &explorer::rows(&s, &e)), vec![("B".into(), 0), ("C".into(), 1), ("A".into(), 0)]);
    execute(&mut s, Command::SceneExplorerSort { explorer: None, column: "Name".into(), ascending: true, add: None }).unwrap();
    let e = s.explorers.get(None).unwrap().clone();
    assert_eq!(names(&s, &explorer::rows(&s, &e)), vec![("A".into(), 0), ("B".into(), 0), ("C".into(), 1)]);
    execute(&mut s, Command::SceneExplorerSort { explorer: None, column: "Name".into(), ascending: false, add: None }).unwrap();
    let e = s.explorers.get(None).unwrap().clone();
    assert_eq!(names(&s, &explorer::rows(&s, &e)), vec![("B".into(), 0), ("C".into(), 1), ("A".into(), 0)], "the child stays under its parent");
    execute(&mut s, Command::SceneExplorerOption { explorer: None, option: "Display Children".into(), on: false }).unwrap();
    let e = s.explorers.get(None).unwrap().clone();
    assert_eq!(names(&s, &explorer::rows(&s, &e)), vec![("C".into(), 0), ("B".into(), 0), ("A".into(), 0)], "a flat list sorts every object");
    let _ = a;
}

#[test]
fn display_filters_and_closed_groups_leave_rows_out() {
    let mut s = AppState::new();
    let a = named_box(&mut s, "A", 0.0);
    let b = named_box(&mut s, "B", 20.0);
    execute(&mut s, Command::CreateShape { kind: loom_commands::ShapeKind::Circle, position: [0.0; 3], size: [5.0; 3] }).unwrap();
    execute(&mut s, Command::NewSceneExplorer).unwrap();
    execute(&mut s, Command::SceneExplorerOption { explorer: None, option: "Shapes".into(), on: false }).unwrap();
    let e = s.explorers.get(None).unwrap().clone();
    assert_eq!(explorer::rows(&s, &e).len(), 2, "no shapes listed");
    execute(&mut s, Command::HideNodes { ids: vec![b.0] }).unwrap();
    execute(&mut s, Command::SceneExplorerOption { explorer: None, option: "Hidden".into(), on: false }).unwrap();
    let e = s.explorers.get(None).unwrap().clone();
    assert_eq!(names(&s, &explorer::rows(&s, &e)), vec![("A".into(), 0)]);
    execute(&mut s, Command::SceneExplorerOption { explorer: None, option: "Display All".into(), on: true }).unwrap();
    execute(&mut s, Command::UnhideNodes { ids: vec![b.0] }).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![a.0, b.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::Group { name: None }).unwrap();
    let e = s.explorers.get(None).unwrap().clone();
    let shown = names(&s, &explorer::rows(&s, &e));
    assert!(shown.iter().any(|(n, _)| n == "Group001") && !shown.iter().any(|(n, _)| n == "A"), "a closed group lists only its head: {shown:?}");
    execute(&mut s, Command::GroupOpen).unwrap();
    let shown = names(&s, &explorer::rows(&s, &e));
    assert!(shown.contains(&("A".into(), 1)), "an open group shows its members: {shown:?}");
}

#[test]
fn find_matches_the_start_wildcards_and_regular_expressions() {
    let mut e = explorer::SceneExplorer::default();
    assert!(explorer::find_matches(&e, "s", "Sphere01"));
    assert!(!explorer::find_matches(&e, "here", "Sphere01"));
    e.find_case_sensitive = true;
    assert!(!explorer::find_matches(&e, "s", "Sphere01"));
    e.find_case_sensitive = false;
    e.find_wildcards = true;
    for (name, hit) in [("Set11", true), ("Set21", true), ("Set34", false)] {
        assert_eq!(explorer::find_matches(&e, "Set?1", name), hit, "{name}");
    }
    assert!(explorer::find_matches(&e, "*01", "Sphere01"));
    e.find_wildcards = false;
    e.find_regex = true;
    assert!(explorer::find_matches(&e, ".*Target", "Spot01.Target"));
    assert!(explorer::find_matches(&e, "sph.*1", "Sphere01"));
    assert!(!explorer::find_matches(&e, "[", "Sphere01"), "a broken expression matches nothing");
}

#[test]
fn advanced_search_highlights_what_meets_all_or_any_terms() {
    let mut s = AppState::new();
    let house = named_box(&mut s, "House01", 0.0);
    let hut = named_box(&mut s, "Hut", 20.0);
    let shed = named_box(&mut s, "HouseShed", 40.0);
    execute(&mut s, Command::HideNodes { ids: vec![shed.0] }).unwrap();
    execute(&mut s, Command::NewSceneExplorer).unwrap();
    let t = |p: &str, c: &str, v: &str| SearchTerm { property: p.into(), condition: c.into(), value: v.into() };
    let r = execute(&mut s, Command::SceneExplorerSearch { explorer: None, terms: vec![t("Name", "Starts With", "house"), t("Hidden", "Is", "No")], match_all: true }).unwrap();
    assert_eq!(r["found"], serde_json::json!([house.0]));
    assert_eq!(s.selection.iter().copied().collect::<Vec<_>>(), vec![house], "with Sync Selection the hits are selected");
    let r = execute(&mut s, Command::SceneExplorerSearch { explorer: None, terms: vec![t("Name", "Is", "hut"), t("Hidden", "Is", "Yes")], match_all: false }).unwrap();
    assert_eq!(r["found"], serde_json::json!([hut.0, shed.0]));
    let r = execute(&mut s, Command::SceneExplorerSearch { explorer: None, terms: vec![t("Faces", "Is", "6")], match_all: true }).unwrap();
    assert_eq!(r["found"].as_array().unwrap().len(), 3, "a box has six polygons");
    assert!(execute(&mut s, Command::SceneExplorerSearch { explorer: None, terms: vec![t("Weight", "Is", "1")], match_all: true }).is_err());
}

#[test]
fn turning_sync_selection_off_keeps_the_highlight_as_the_explorers_own() {
    let mut s = AppState::new();
    let a = named_box(&mut s, "A", 0.0);
    execute(&mut s, Command::NewSceneExplorer).unwrap();
    execute(&mut s, Command::SceneExplorerOption { explorer: None, option: "Sync Selection".into(), on: false }).unwrap();
    let e = s.explorers.get(None).unwrap();
    assert_eq!(explorer::highlighted(&s, e).into_iter().collect::<Vec<_>>(), vec![a]);
    execute(&mut s, Command::SelectNone).unwrap();
    let e = s.explorers.get(None).unwrap();
    assert_eq!(explorer::highlighted(&s, e).len(), 1, "the scene selection no longer drives it");
}

#[test]
fn set_nodes_property_edits_several_objects_in_one_undo_step() {
    let mut s = AppState::new();
    let a = named_box(&mut s, "A", 0.0);
    let b = named_box(&mut s, "B", 20.0);
    for (name, get) in [
        ("Renderable", (|n: &loom_scene::Node| n.render.renderable) as fn(&loom_scene::Node) -> bool),
        ("See-Through", |n| n.display.see_through),
        ("Backface Cull", |n| n.display.backface_cull),
        ("Hidden", |n| n.hidden),
        ("Frozen", |n| n.frozen),
    ] {
        let before = get(s.scene.node(a).unwrap());
        let undo = s.undo.len();
        execute(&mut s, Command::SetNodesProperty { ids: vec![a.0, b.0], name: name.into(), value: !before }).unwrap();
        assert!(get(s.scene.node(a).unwrap()) != before && get(s.scene.node(b).unwrap()) != before, "{name} on both");
        assert_eq!(s.undo.len(), undo + 1, "{name} is one undo step");
        execute(&mut s, Command::UndoSceneOperation).unwrap();
        assert_eq!(get(s.scene.node(b).unwrap()), before, "{name} undone");
    }
    assert!(execute(&mut s, Command::SetNodesProperty { ids: vec![a.0], name: "Shiny".into(), value: true }).is_err());
}

#[test]
fn rename_nodes_numbers_the_others() {
    let mut s = AppState::new();
    let ids: Vec<NodeId> = (0..3).map(|i| named_box(&mut s, "x", i as f32 * 20.0)).collect();
    execute(&mut s, Command::RenameNodes { ids: ids.iter().map(|i| i.0).collect(), name: "door".into() }).unwrap();
    let got: Vec<&str> = ids.iter().map(|i| s.scene.node(*i).unwrap().name.as_str()).collect();
    assert_eq!(got, ["door", "door01", "door02"]);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(ids.iter().all(|i| s.scene.node(*i).unwrap().name == "x"), "one undo step");
}

#[test]
fn delete_nodes_and_linking_to_a_child_is_refused() {
    let mut s = AppState::new();
    let a = named_box(&mut s, "A", 0.0);
    let b = named_box(&mut s, "B", 20.0);
    execute(&mut s, Command::SetParent { id: b.0, parent: Some(a.0) }).unwrap();
    assert!(execute(&mut s, Command::SetParent { id: a.0, parent: Some(b.0) }).is_err(), "a parent cannot become its child's child");
    assert_eq!(s.scene.node(a).unwrap().parent, None);
    execute(&mut s, Command::DeleteNodes { ids: vec![b.0] }).unwrap();
    assert!(s.scene.node(b).is_none() && s.scene.node(a).is_some());
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.node(b).unwrap().parent, Some(a));
}

#[test]
fn schematic_graph_shows_hierarchy_modifiers_and_shared_materials() {
    let mut s = AppState::new();
    let a = named_box(&mut s, "A", 0.0);
    execute(&mut s, Command::AddModifier { name: "Bend".into() }).unwrap();
    let b = named_box(&mut s, "B", 20.0);
    execute(&mut s, Command::SetParent { id: b.0, parent: Some(a.0) }).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![a.0, b.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::AssignMaterialToSelection).unwrap();
    execute(&mut s, Command::NewSchematicView).unwrap();
    let v = s.schematics.get(None).unwrap().clone();
    let (ents, wires) = schematic::graph(&s, &v);
    let keys: Vec<&str> = ents.iter().map(|e| e.key.as_str()).collect();
    let (ka, kb) = (format!("obj:{}", a.0), format!("obj:{}", b.0));
    assert_eq!(keys[0], ka);
    assert!(ents.iter().any(|e| e.kind == EntityKind::Modifier(0) && e.label == "Bend" && e.owner.as_deref() == Some(ka.as_str())));
    let mats: Vec<_> = ents.iter().filter(|e| matches!(e.kind, EntityKind::Material(_))).collect();
    assert_eq!(mats.len(), 1, "a shared material is one entity");
    assert!(mats[0].instanced);
    assert!(wires.iter().any(|w| w.kind == WireKind::Shared && w.from == kb && w.to == mats[0].key), "the second user is wired to it");
    assert!(wires.iter().any(|w| w.kind == WireKind::Hierarchy && w.from == ka && w.to == kb));
    let pos = schematic::positions(&v, &ents);
    assert!(pos[&kb][0] > pos[&ka][0] && pos[&kb][1] > pos[&ka][1], "Hierarchy Mode indents the child below its parent");
    execute(&mut s, Command::SchematicOption { view: None, option: "References Mode".into(), on: true }).unwrap();
    let v = s.schematics.get(None).unwrap().clone();
    let (ents, _) = schematic::graph(&s, &v);
    let pos = schematic::positions(&v, &ents);
    assert!(pos[&kb][0] > pos[&ka][0] + schematic::BOX_W * 0.5 && pos[&kb][1] == pos[&ka][1], "References Mode sets the objects side by side");
    execute(&mut s, Command::SchematicOption { view: None, option: "Modifier Stack".into(), on: false }).unwrap();
    execute(&mut s, Command::SchematicOption { view: None, option: "Controllers".into(), on: true }).unwrap();
    let v = s.schematics.get(None).unwrap().clone();
    let (ents, _) = schematic::graph(&s, &v);
    assert!(!ents.iter().any(|e| matches!(e.kind, EntityKind::Modifier(_))));
    assert!(ents.iter().any(|e| e.key == format!("ctl:{}:Position", a.0) && e.label.contains("Position")));
    // Boxes are as wide as their labels, and References Mode columns leave room for them.
    let pos = schematic::positions(&v, &ents);
    let ctl = ents.iter().find(|e| e.key == format!("ctl:{}:Position", a.0)).unwrap();
    assert!(schematic::box_width(&ctl.label) > schematic::BOX_W, "{:?} is longer than a plain box", ctl.label);
    let right_a = ents.iter().filter(|e| e.node == a).map(|e| pos[&e.key][0] + schematic::box_width(&e.label)).fold(0.0f32, f32::max);
    assert!(pos[&kb][0] > right_a, "B's column starts right of A's widest box");
}

#[test]
fn schematic_select_syncs_connect_links_and_unlink_is_one_step() {
    let mut s = AppState::new();
    let a = named_box(&mut s, "A", 0.0);
    let b = named_box(&mut s, "B", 20.0);
    let c = named_box(&mut s, "C", 40.0);
    execute(&mut s, Command::NewSchematicView).unwrap();
    execute(&mut s, Command::SchematicSelect { view: None, keys: vec![format!("obj:{}", b.0)], op: SelectOp::Replace }).unwrap();
    assert_eq!(s.selection.iter().copied().collect::<Vec<_>>(), vec![b], "Sync Selection selects in the scene");
    execute(&mut s, Command::SelectNodes { ids: vec![c.0], op: SelectOp::Replace }).unwrap();
    let v = s.schematics.get(None).unwrap();
    assert!(schematic::selected(&s, v).contains(&format!("obj:{}", c.0)), "and the scene selection shows in the view");
    execute(&mut s, Command::SchematicConnect { from: format!("obj:{}", b.0), to: format!("obj:{}", a.0) }).unwrap();
    execute(&mut s, Command::SchematicConnect { from: format!("obj:{}", c.0), to: format!("obj:{}", a.0) }).unwrap();
    assert_eq!(s.scene.node(b).unwrap().parent, Some(a));
    assert!(execute(&mut s, Command::SchematicConnect { from: format!("obj:{}", a.0), to: format!("obj:{}", b.0) }).is_err());
    execute(&mut s, Command::SchematicSelect { view: None, keys: vec![format!("obj:{}", b.0), format!("obj:{}", c.0)], op: SelectOp::Replace }).unwrap();
    let undo = s.undo.len();
    execute(&mut s, Command::SchematicUnlinkSelected { view: None }).unwrap();
    assert_eq!((s.scene.node(b).unwrap().parent, s.scene.node(c).unwrap().parent), (None, None));
    assert_eq!(s.undo.len(), undo + 1);
    execute(&mut s, Command::SchematicDeleteSelected { view: None }).unwrap();
    assert!(s.scene.node(b).is_none() && s.scene.node(c).is_none() && s.scene.node(a).is_some());
}

#[test]
fn free_all_keeps_places_and_always_arrange_ignores_free_positions() {
    let mut s = AppState::new();
    let a = named_box(&mut s, "A", 0.0);
    execute(&mut s, Command::NewSchematicView).unwrap();
    execute(&mut s, Command::SchematicOption { view: None, option: "Free All".into(), on: true }).unwrap();
    let v = s.schematics.get(None).unwrap().clone();
    let ka = format!("obj:{}", a.0);
    let (ents, _) = schematic::graph(&s, &v);
    assert_eq!(v.free.get(&ka), schematic::arrange(&v, &ents).get(&ka), "freed where it was");
    s.schematics.list[0].free.insert(ka.clone(), [300.0, 200.0]);
    let v = s.schematics.get(None).unwrap().clone();
    assert_eq!(schematic::positions(&v, &ents)[&ka], [300.0, 200.0]);
    execute(&mut s, Command::SchematicOption { view: None, option: "Always Arrange".into(), on: true }).unwrap();
    let v = s.schematics.get(None).unwrap().clone();
    assert_eq!(schematic::positions(&v, &ents)[&ka], schematic::arrange(&v, &ents)[&ka]);
}
