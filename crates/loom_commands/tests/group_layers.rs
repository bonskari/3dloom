use loom_commands::{execute, AppState, Command, PickMode, PrimitiveKind, SelectOp};
use loom_scene::{NodeId, ObjectKind};

fn boxes(s: &mut AppState, n: usize) -> Vec<NodeId> {
    (0..n)
        .map(|i| {
            execute(s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [i as f32 * 20.0, 0.0, 0.0], size: [10.0; 3] }).unwrap();
            *s.selection.iter().next().unwrap()
        })
        .collect()
}

fn world_pos(s: &AppState, id: NodeId) -> glam::Vec3 {
    s.scene.world_matrix(id).w_axis.truncate()
}

#[test]
fn group_makes_head_and_parents_members() {
    let mut s = AppState::new();
    let ids = boxes(&mut s, 2);
    execute(&mut s, Command::SelectNodes { ids: ids.iter().map(|i| i.0).collect(), op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::Group { name: None }).unwrap();
    let head = *s.selection.iter().next().unwrap();
    let hn = s.scene.node(head).unwrap();
    assert!(matches!(hn.object, ObjectKind::Group { open: false }));
    assert_eq!(hn.name, "Group001");
    assert!((world_pos(&s, head) - glam::Vec3::new(10.0, 0.0, 5.0)).length() < 1e-3, "head at the members' centre");
    for id in &ids {
        assert_eq!(s.scene.node(*id).unwrap().parent, Some(head));
    }
    assert!((world_pos(&s, ids[1]) - glam::Vec3::new(20.0, 0.0, 0.0)).length() < 1e-3, "members keep their world position");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(s.scene.node(head).is_none());
    assert_eq!(s.scene.node(ids[0]).unwrap().parent, None);
}

#[test]
fn closed_group_selects_as_one_open_selects_members() {
    let mut s = AppState::new();
    let ids = boxes(&mut s, 2);
    execute(&mut s, Command::SelectNodes { ids: ids.iter().map(|i| i.0).collect(), op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::Group { name: Some("Pair".into()) }).unwrap();
    let head = *s.selection.iter().next().unwrap();
    // Clicking a member of a closed group selects the head.
    execute(&mut s, Command::SelectNodes { ids: vec![ids[0].0], op: SelectOp::Replace }).unwrap();
    assert_eq!(s.selection.iter().copied().collect::<Vec<_>>(), vec![head]);
    execute(&mut s, Command::GroupOpen).unwrap();
    assert!(matches!(s.scene.node(head).unwrap().object, ObjectKind::Group { open: true }));
    execute(&mut s, Command::SelectNodes { ids: vec![ids[0].0], op: SelectOp::Replace }).unwrap();
    assert_eq!(s.selection.iter().copied().collect::<Vec<_>>(), vec![ids[0]], "open group: the member itself");
    execute(&mut s, Command::GroupClose).unwrap();
    assert_eq!(s.selection.iter().copied().collect::<Vec<_>>(), vec![head], "closing selects the group");
    // Attach a third box, detach the first.
    let third = boxes(&mut s, 1)[0];
    execute(&mut s, Command::GroupAttach).unwrap();
    assert_eq!(s.pick_mode, Some(PickMode::GroupAttach));
    execute(&mut s, Command::PickHit { id: head.0 }).unwrap();
    assert_eq!(s.scene.node(third).unwrap().parent, Some(head));
    execute(&mut s, Command::GroupOpen).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![ids[0].0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::GroupDetach).unwrap();
    assert_eq!(s.scene.node(ids[0]).unwrap().parent, None);
}

#[test]
fn ungroup_explode_restore_world_transforms() {
    let mut s = AppState::new();
    let ids = boxes(&mut s, 2);
    execute(&mut s, Command::SelectNodes { ids: ids.iter().map(|i| i.0).collect(), op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::Group { name: None }).unwrap();
    let inner = *s.selection.iter().next().unwrap();
    let c = boxes(&mut s, 1)[0];
    execute(&mut s, Command::SelectNodes { ids: vec![inner.0, c.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::Group { name: None }).unwrap();
    let outer = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::MoveSelection { delta: [0.0, 0.0, 7.0], system: None }).unwrap();
    let before: Vec<glam::Vec3> = ids.iter().chain([&c]).map(|i| world_pos(&s, *i)).collect();
    assert!((before[0].z - 7.0).abs() < 1e-3, "moving the group moves the members");
    execute(&mut s, Command::GroupExplode).unwrap();
    assert!(s.scene.node(outer).is_none() && s.scene.node(inner).is_none());
    for (i, id) in ids.iter().chain([&c]).enumerate() {
        assert_eq!(s.scene.node(*id).unwrap().parent, None);
        assert!((world_pos(&s, *id) - before[i]).length() < 1e-3, "world transform kept");
    }
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(s.scene.node(outer).is_some() && s.scene.node(inner).is_some());
    assert_eq!(s.scene.node(ids[0]).unwrap().parent, Some(inner));
    // Ungroup only removes the selected (outer) head.
    execute(&mut s, Command::SelectNodes { ids: vec![outer.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::Ungroup).unwrap();
    assert!(s.scene.node(inner).is_some());
    assert_eq!(s.scene.node(inner).unwrap().parent, None);
}

#[test]
fn layer_hidden_hides_members_in_picks() {
    let mut s = AppState::new();
    let ids = boxes(&mut s, 2);
    execute(&mut s, Command::SelectNodes { ids: vec![ids[0].0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::CreateLayer { name: Some("Walls".into()) }).unwrap();
    let layer = s.scene.layers.iter().find(|l| l.name == "Walls").unwrap().id;
    assert_eq!(s.scene.current_layer, layer, "a new layer becomes current");
    assert_eq!(s.scene.node(ids[0]).unwrap().layer, layer, "the selection moved onto it");
    execute(&mut s, Command::SetLayerHidden { layer, on: true }).unwrap();
    assert!(!s.is_visible(s.scene.node(ids[0]).unwrap()));
    assert!(s.selection.is_empty(), "hidden objects drop out of the selection");
    execute(&mut s, Command::SelectAll).unwrap();
    assert_eq!(s.selection.iter().copied().collect::<Vec<_>>(), vec![ids[1]]);
    execute(&mut s, Command::SetLayerHidden { layer, on: false }).unwrap();
    execute(&mut s, Command::SetLayerFrozen { layer, on: true }).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![ids[0].0], op: SelectOp::Replace }).unwrap();
    assert!(s.selection.is_empty(), "frozen layer objects cannot be selected");
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert!(!s.scene.layer(layer).unwrap().frozen);
}

#[test]
fn move_to_layer_and_current_layer_for_new_objects() {
    let mut s = AppState::new();
    execute(&mut s, Command::CreateLayer { name: None }).unwrap();
    let l1 = s.scene.current_layer;
    assert_eq!(s.scene.layer(l1).unwrap().name, "Layer001");
    let a = boxes(&mut s, 1)[0];
    assert_eq!(s.scene.node(a).unwrap().layer, l1, "new objects take the current layer");
    execute(&mut s, Command::SetCurrentLayer { layer: 0 }).unwrap();
    let b = boxes(&mut s, 1)[0];
    assert_eq!(s.scene.node(b).unwrap().layer, 0);
    execute(&mut s, Command::SelectNodes { ids: vec![b.0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::MoveToLayer { layer: l1 }).unwrap();
    assert_eq!(s.scene.node(b).unwrap().layer, l1);
    execute(&mut s, Command::SelectLayerObjects { layer: l1 }).unwrap();
    assert_eq!(s.selection.len(), 2);
    assert!(execute(&mut s, Command::DeleteLayer { layer: l1 }).is_err(), "not empty");
    execute(&mut s, Command::MoveToLayer { layer: 0 }).unwrap();
    execute(&mut s, Command::DeleteLayer { layer: l1 }).unwrap();
    assert!(s.scene.layer(l1).is_none());
    assert!(execute(&mut s, Command::DeleteLayer { layer: 0 }).is_err());
    execute(&mut s, Command::RenameLayer { layer: 0, name: "Base".into() }).unwrap();
    assert_eq!(s.scene.layer(0).unwrap().name, "Base");
}

#[test]
fn select_ancestor_child_children_similar() {
    let mut s = AppState::new();
    let ids = boxes(&mut s, 2);
    execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Sphere, position: [0.0, 40.0, 0.0], size: [10.0; 3] }).unwrap();
    let sphere = *s.selection.iter().next().unwrap();
    execute(&mut s, Command::SetParent { id: ids[1].0, parent: Some(ids[0].0) }).unwrap();
    execute(&mut s, Command::SetParent { id: sphere.0, parent: Some(ids[0].0) }).unwrap();
    execute(&mut s, Command::SelectNodes { ids: vec![ids[1].0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SelectAncestor).unwrap();
    assert_eq!(s.selection.iter().copied().collect::<Vec<_>>(), vec![ids[0]]);
    execute(&mut s, Command::SelectChildren).unwrap();
    assert_eq!(s.selection.len(), 2);
    execute(&mut s, Command::SelectNodes { ids: vec![ids[0].0], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SelectChild).unwrap();
    assert_eq!(s.selection.len(), 1);
    execute(&mut s, Command::SelectSimilar).unwrap();
    let kinds: std::collections::BTreeSet<&str> = s.selection.iter().map(|i| s.scene.node(*i).unwrap().object.base_name()).collect();
    assert_eq!(kinds.len(), 1, "only one kind selected: {kinds:?}");
    assert_eq!(s.selection.len(), 2, "both boxes");
    assert_eq!(s.hotkey(loom_commands::parse_chord("PageUp").unwrap()), Some("Select Ancestor"));
}
