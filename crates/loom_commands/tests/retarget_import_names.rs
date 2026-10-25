use loom_commands::{
    execute,
    retarget::{auto_mapping, retarget},
    AppState, Command,
};
use loom_scene::{loomrig::HumanoidMapping, Modifier, NodeId};
use std::{collections::BTreeSet, path::Path};
fn canonical(name: &str) -> &str {
    name.rsplit([':', '|']).next().unwrap()
}
fn bones(s: &AppState, ids: &[NodeId]) -> Vec<(NodeId, String)> {
    ids.iter()
        .filter_map(|id| {
            let n = s.scene.node(*id).unwrap();
            n.object.is_bone().then(|| (*id, n.name.clone()))
        })
        .collect()
}
#[test]
fn repeated_animated_fbx_import_preserves_canonical_numbered_bone_names() {
    let path = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../loom_fbx/tests/fixtures/sitting_idle.fbx"
    ));
    let mut s = AppState::new();
    let mut previous = None;
    let mut all_names = BTreeSet::new();
    for _ in 0..3 {
        let (ids, _) = loom_commands::fbx_io::import_animation(&mut s, path).unwrap();
        let bones = bones(&s, &ids);
        let names: Vec<_> = bones
            .iter()
            .map(|(_, name)| canonical(name).to_owned())
            .collect();
        if let Some(expected) = &previous {
            assert_eq!(
                &names, expected,
                "canonical source identities changed on repeated import"
            );
        }
        for (_, name) in bones {
            assert!(all_names.insert(name), "source imports share display names");
        }
        previous = Some(names);
    }
}
#[test]
#[ignore = "requires LOOM_RETARGET_FBX and LOOM_RETARGET_BVH actual production assets"]
fn repeated_ray_and_kimodo_imports_characterize_and_retarget_independently() {
    let fbx = std::env::var("LOOM_RETARGET_FBX").expect("LOOM_RETARGET_FBX");
    let bvh = std::env::var("LOOM_RETARGET_BVH").expect("LOOM_RETARGET_BVH");
    let mut s = AppState::new();
    s.exchange.fbx_import.metahuman_body_only = true;
    let mut characters = Vec::new();
    let mut canonical_bones = None;
    let mut all_names = BTreeSet::new();
    for _ in 0..2 {
        let ids = loom_commands::fbx_io::import(&mut s, Path::new(&fbx)).unwrap();
        let current = bones(&s, &ids);
        let names: Vec<_> = current
            .iter()
            .map(|(_, n)| canonical(n).to_owned())
            .collect();
        if let Some(expected) = &canonical_bones {
            assert_eq!(&names, expected, "MetaHuman canonical bone names changed");
        }
        canonical_bones = Some(names);
        for (_, name) in &current {
            assert!(all_names.insert(name.clone()));
        }
        let root = current
            .iter()
            .map(|(id, _)| *id)
            .find(|id| {
                s.scene
                    .node(*id)
                    .unwrap()
                    .parent
                    .and_then(|p| s.scene.node(p))
                    .is_none_or(|p| !p.object.is_bone())
            })
            .unwrap();
        HumanoidMapping::metahuman(&s.scene, root).unwrap();
        let members: BTreeSet<_> = ids.iter().copied().collect();
        let mut skins = 0;
        for id in &ids {
            for slot in &s.scene.node(*id).unwrap().modifiers {
                if let Modifier::Skin(skin) = &slot.modifier {
                    skins += 1;
                    assert!(
                        skin.bones.iter().all(|b| members.contains(&b.node)),
                        "skin references another character"
                    );
                }
            }
        }
        assert!(skins > 0);
        characters.push((root, current));
    }
    let mut source_names = None;
    let mut start = 0.0;
    for _ in 0..3 {
        let ids = loom_commands::bvh_io::import_animation(&mut s, Path::new(&bvh)).unwrap();
        let names: Vec<_> = bones(&s, &ids)
            .into_iter()
            .map(|(_, n)| canonical(&n).to_owned())
            .collect();
        if let Some(expected) = &source_names {
            assert_eq!(
                &names, expected,
                "Kimodo canonical finger/spine names changed"
            );
        }
        source_names = Some(names);
        let clip = s.scene.animation_clips.last().unwrap().clone();
        execute(&mut s, Command::MoveAnimationClip { id: clip.id, start }).unwrap();
        for (root, _) in &characters {
            let mapping = auto_mapping(&s, clip.id, *root).unwrap();
            assert_eq!(mapping.len(), 61);
            retarget(&mut s, clip.id, *root, &mapping).unwrap();
        }
        start += clip.end - clip.start + 10.25;
    }
    for (_, original) in characters {
        for (id, name) in original {
            assert_eq!(s.scene.node(id).unwrap().name, name);
        }
    }
}
