use glam::Vec3;
use loom_commands::{execute, AppState, Command};
use loom_scene::loomrig::{HumanoidMapping, LimbMapping};
use loom_scene::{Node, ObjectKind};

fn rigged() -> AppState {
    let mut s = AppState::new();
    let mut bones = Vec::new();
    for i in 0..4 {
        let mut n = Node::new(&format!("joint{i}"), ObjectKind::bone(10.0));
        if i > 0 { n.parent = Some(bones[i - 1]); n.transform.position.set(Vec3::X * 10.0); }
        bones.push(s.scene.add_node(n));
    }
    let mapping = HumanoidMapping { root: bones[0], pelvis: bones[1], spine: vec![], head: bones[3],
        limbs: vec![LimbMapping { name: "Arm".into(), start: bones[1], middle: bones[2], end: bones[3] }] };
    execute(&mut s, Command::CreateLoomRig { root: bones[0].0, mapping: Some(mapping) }).unwrap();
    s
}
fn path(name: &str) -> String { std::env::temp_dir().join(format!("loom-rig-persistence-{name}.loom")).to_string_lossy().into_owned() }
fn document(s: &AppState) -> loom_commands::file::Document {
    loom_commands::file::Document { format: loom_commands::file::FORMAT, app: "3dloom".into(), scene: s.scene.clone(), settings: loom_commands::file::SceneSettings::from_state(s) }
}

#[test]
fn deleting_a_rig_bone_or_control_prunes_only_affected_metadata_and_undo_restores_it() {
    for control in [false, true] {
        let mut s = rigged();
        let second = document(&rigged());
        loom_commands::file::merge_map(&mut s, &second, None, &[], "Second rig").unwrap();
        let unrelated = s.scene.add_node(Node::new("Unrelated", ObjectKind::dummy(1.0)));
        let before = s.scene.loom_rigs.clone();
        let id = if control { before[0].limbs[0].pole } else { before[0].mapping.head };
        let remaining: Vec<_> = s.scene.nodes().map(|n| n.id).filter(|n| *n != id).collect();
        s.selection = [id].into();
        execute(&mut s, Command::DeleteObjects).unwrap();
        assert_eq!(s.scene.loom_rigs, before[1..]);
        assert!(remaining.iter().all(|n| s.scene.node(*n).is_some()));
        assert!(s.scene.node(unrelated).is_some());
        execute(&mut s, Command::UndoSceneOperation).unwrap();
        assert_eq!(s.scene.loom_rigs, before);
        assert!(s.scene.node(id).is_some());
        execute(&mut s, Command::RedoSceneOperation).unwrap();
        assert_eq!(s.scene.loom_rigs, before[1..]);
    }
}

#[test]
fn merge_remaps_complete_rigs_and_allocates_unique_rig_ids_atomically() {
    let source = rigged();
    let doc = document(&source);
    let original = source.scene.loom_rigs[0].clone();
    let mut target = rigged();
    let target_original = target.scene.loom_rigs.clone();
    let map = loom_commands::file::merge_map(&mut target, &doc, None, &[], "Merge rig").unwrap();
    assert_eq!(target.scene.loom_rigs.len(), 2);
    let added = &target.scene.loom_rigs[1];
    assert_ne!(added.id, target_original[0].id);
    assert_eq!(added.mapping.root, map[&original.mapping.root]);
    assert_eq!(added.mapping.head, map[&original.mapping.head]);
    assert_eq!(added.master, map[&original.master]);
    for (a, b) in original.bindings.iter().zip(&added.bindings) {
        assert_eq!((b.bone, b.control), (map[&a.bone], map[&a.control]));
    }
    assert_eq!(added.limbs[0].goal, map[&original.limbs[0].goal]);
    assert_eq!(added.limbs[0].pole, map[&original.limbs[0].pole]);
    assert_eq!(added.limbs[0].mapping.middle, map[&original.limbs[0].mapping.middle]);
    execute(&mut target, Command::UndoSceneOperation).unwrap();
    assert_eq!(target.scene.loom_rigs, target_original);
    execute(&mut target, Command::RedoSceneOperation).unwrap();
    assert_eq!(target.scene.loom_rigs.len(), 2);
    let mut partial = AppState::new();
    loom_commands::file::merge_map(&mut partial, &doc, Some(&["joint0".into()]), &[], "Partial").unwrap();
    assert_eq!(partial.scene.nodes().count(), 1);
    assert!(partial.scene.loom_rigs.is_empty());
}

#[test]
fn selected_save_keeps_complete_rig_and_drops_partial_without_touching_source() {
    let mut s = rigged();
    let before = s.scene.loom_rigs.clone();
    let full = path("full");
    s.selection = s.scene.nodes().map(|n| n.id).collect();
    execute(&mut s, Command::SaveSelected { path: Some(full.clone()) }).unwrap();
    assert_eq!(loom_commands::file::read(std::path::Path::new(&full)).unwrap().scene.loom_rigs, before);
    let partial = path("partial");
    s.selection = [before[0].mapping.root].into();
    execute(&mut s, Command::SaveSelected { path: Some(partial.clone()) }).unwrap();
    let saved = loom_commands::file::read(std::path::Path::new(&partial)).unwrap();
    assert!(saved.scene.loom_rigs.is_empty());
    assert_eq!(s.scene.loom_rigs, before);
    std::fs::remove_file(full).unwrap(); std::fs::remove_file(partial).unwrap();
}

#[test]
fn new_scene_prunes_rigs_and_undo_restores_the_complete_character() {
    let mut s = rigged();
    let before = s.scene.loom_rigs.clone();
    execute(&mut s, Command::NewScene).unwrap();
    assert!(s.scene.loom_rigs.is_empty());
    assert_eq!(s.scene.nodes().count(), 0);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.loom_rigs, before);
}

#[test]
fn new_keep_preserves_static_rig_with_hierarchy_and_visible_pose_without_hierarchy() {
    for keep in ["objects_and_hierarchy", "objects"] {
        let mut s = rigged();
        let goal = s.scene.loom_rigs[0].limbs[0].goal;
        execute(&mut s, Command::SetLoomRigLimb { rig: 1, limb: "Arm".into(), solver: "limb".into(), blend: 1.0 }).unwrap();
        s.scene.node_mut(goal).unwrap().transform.position.set(Vec3::new(12.0, 8.0, 0.0));
        let before_rigs = s.scene.loom_rigs.clone();
        let worlds: Vec<_> = s.scene.nodes().map(|n| (n.id, s.scene.world_matrix(n.id))).collect();
        execute(&mut s, Command::NewSceneKeep { keep: keep.into() }).unwrap();
        if keep == "objects" {
            assert!(s.scene.loom_rigs.is_empty());
            assert!(s.scene.nodes().all(|n| n.parent.is_none()));
        } else { assert_eq!(s.scene.loom_rigs, before_rigs); }
        for (id, world) in worlds { assert!(s.scene.world_matrix(id).abs_diff_eq(world, 0.001), "{keep} changed {}: before {world:?}, after {:?}", id.0, s.scene.world_matrix(id)); }
        assert!(s.scene.animation_clips.is_empty());
    }
}

#[test]
fn new_keep_hierarchy_freezes_native_rig_metadata_tracks_at_current_values() {
    let mut s = rigged();
    let limb = &mut s.scene.loom_rigs[0].limbs[0];
    let linear = (loom_scene::anim::TangentType::Linear, loom_scene::anim::TangentType::Linear);
    limb.blend.ensure_keys().unwrap().tracks[0].set_key(0.0, 0.7, linear);
    let mut offset = loom_scene::Param::new(Vec3::ZERO);
    offset.ensure_keys().unwrap().tracks[2].set_key(0.0, 15.0, linear);
    limb.pose_offsets = vec![offset];
    let expected = (limb.blend.get(), limb.pose_offsets[0].get());
    execute(&mut s, Command::NewSceneKeep { keep: "objects_and_hierarchy".into() }).unwrap();
    let limb = &s.scene.loom_rigs[0].limbs[0];
    assert_eq!((limb.blend.get(), limb.pose_offsets[0].get()), expected);
    assert!(limb.blend.keys().is_none());
    assert!(limb.pose_offsets.iter().all(|p| p.keys().is_none()));
}

#[test]
fn authored_contacts_survive_save_merge_mapping_and_undo() {
    use loom_scene::loomrig_contact::{ContactKey, ContactReference, ContactSpace, ContactTrack};
    let mut source = rigged();
    source.scene.loom_rigs[0].limbs[0].contacts = ContactTrack::new(
        [(7, ContactReference { space: ContactSpace::World, position: Vec3::new(4., 5., 6.), rotation: glam::Quat::IDENTITY })].into(),
        vec![ContactKey { time: 10., reference: 7, blend: 1. }, ContactKey { time: 20., reference: 7, blend: 0.5 }],
    ).unwrap();
    let expected = source.scene.loom_rigs[0].limbs[0].contacts.clone();
    let mapping = source.scene.loom_rigs[0].mapping.clone();
    execute(&mut source, Command::SetLoomRigMapping { rig: 1, mapping }).unwrap();
    assert_eq!(source.scene.loom_rigs[0].limbs[0].contacts, expected);
    let saved_path = path("contacts");
    source.selection = source.scene.nodes().map(|n| n.id).collect();
    execute(&mut source, Command::SaveSelected { path: Some(saved_path.clone()) }).unwrap();
    let doc = loom_commands::file::read(std::path::Path::new(&saved_path)).unwrap();
    assert_eq!(doc.scene.loom_rigs[0].limbs[0].contacts, expected);
    let mut target = AppState::new();
    loom_commands::file::merge_map(&mut target, &doc, None, &[], "Contacts").unwrap();
    assert_eq!(target.scene.loom_rigs[0].limbs[0].contacts, expected);
    execute(&mut target, Command::UndoSceneOperation).unwrap();
    assert!(target.scene.loom_rigs.is_empty());
    execute(&mut target, Command::RedoSceneOperation).unwrap();
    assert_eq!(target.scene.loom_rigs[0].limbs[0].contacts, expected);
    std::fs::remove_file(saved_path).unwrap();
}
