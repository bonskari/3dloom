//! File > XRef Objects and XRef Scene: references to the objects of another
//! .loom or .max file. The objects come in as Merge brings them (fresh ids,
//! layers and materials by name) and are remembered in `Scene::xrefs`; this
//! scene cannot edit their objects, stacks or materials (an XRef Scene's
//! objects cannot be selected or moved either). Update re-reads the file:
//! each referenced object takes the file's object, stack and material again
//! (an XRef Scene object also its placement), objects gone from the file go,
//! and an XRef Scene picks up objects the file gained. With Update
//! Automatically on, a change of the file on disk runs Update by itself.

use crate::file::{merge_changes, read_any};
use crate::state::AppState;
use loom_scene::{Change, NodeId, XrefRecord};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;

fn modified(path: &str) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// XRef Objects (`scene` false: all objects or those in `names`) or XRef
/// Scene (`scene` true: every object, frozen in place), one undo step.
pub fn add(state: &mut AppState, path: &Path, names: Option<&[String]>, scene: bool, automatic: bool) -> Result<Value, String> {
    let key = path.to_string_lossy().into_owned();
    if state.scene.xrefs.iter().any(|r| r.path == key && r.scene == scene) && scene {
        return Err(format!("{key} is already an XRef Scene here"));
    }
    let (doc, warnings) = read_any(path)?;
    let (mut changes, map) = merge_changes(state, &doc, if scene { None } else { names }, &[])?;
    if map.is_empty() {
        return Err("nothing to reference".into());
    }
    let new_ids: Vec<NodeId> = map.values().copied().collect();
    if scene {
        for c in changes.iter_mut() {
            if let Change::AddNode(n) = c {
                if new_ids.contains(&n.id) {
                    n.frozen = true;
                    n.display.show_frozen_in_gray = false;
                }
            }
        }
    }
    let nodes: Vec<(String, NodeId)> = map.iter().filter_map(|(src, new)| doc.scene.node(*src).map(|n| (n.name.clone(), *new))).collect();
    let mut after = state.scene.xrefs.clone();
    after.push(XrefRecord { path: key.clone(), scene, automatic, nodes });
    changes.push(Change::SetXrefs { before: state.scene.xrefs.clone(), after });
    let label = format!("{} {}", if scene { "XRef Scene" } else { "XRef Objects" }, path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    state.undo.commit(&label, Change::Group(changes), &mut state.scene)?;
    state.mesh_dirty = true;
    if !scene {
        state.selection = new_ids.iter().copied().collect();
        state.sub_selection.clear();
    }
    if let Some(t) = modified(&key) {
        state.xref_seen.insert(key, t);
    }
    if crate::max_io::is_max(path) {
        state.import_report = warnings;
    }
    let mut ids = new_ids;
    ids.sort();
    Ok(json!({ "nodes": ids.iter().map(|i| i.0).collect::<Vec<_>>() }))
}

/// Update: re-reads the referenced files (all, each its own undo step, or
/// the one at `path`).
pub fn update(state: &mut AppState, path: Option<&str>) -> Result<Value, String> {
    let Some(path) = path else {
        let all: Vec<String> = state.scene.xrefs.iter().map(|r| r.path.clone()).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
        if all.is_empty() {
            return Err("no XRefs to update".into());
        }
        for p in &all {
            update(state, Some(p))?;
        }
        return Ok(json!({ "updated": all }));
    };
    if !state.scene.xrefs.iter().any(|r| r.path == path) {
        return Err(format!("{path} is not referenced here"));
    }
    let (doc, _) = read_any(Path::new(path))?;
    let mut records = state.scene.xrefs.clone();
    let mut changes = Vec::new();
    let mut library = state.scene.materials.clone();
    for r in records.iter_mut().filter(|r| r.path == path) {
        let mut kept = Vec::new();
        // An XRef Scene takes in the objects the file gained (first, so their
        // materials are in the library the others are matched against).
        if r.scene {
            let known: std::collections::BTreeSet<&str> = r.nodes.iter().map(|(n, _)| n.as_str()).collect();
            let gained: Vec<String> = doc.scene.nodes().map(|n| n.name.clone()).filter(|n| !known.contains(n.as_str())).collect();
            if !gained.is_empty() {
                let (more, map) = merge_changes(state, &doc, Some(&gained), &[])?;
                for c in more {
                    match c {
                        Change::AddNode(mut n) => {
                            n.frozen = true;
                            n.display.show_frozen_in_gray = false;
                            if let Some(src) = map.iter().find(|(_, v)| **v == n.id).and_then(|(k, _)| doc.scene.node(*k)) {
                                kept.push((src.name.clone(), n.id));
                            }
                            changes.push(Change::AddNode(n));
                        }
                        Change::SetMaterials { after, .. } => library = after.0,
                        other => changes.push(other),
                    }
                }
            }
        }
        // File node id -> this scene's node, by name.
        let mut ids: BTreeMap<NodeId, NodeId> = BTreeMap::new();
        for (name, id) in r.nodes.iter().chain(kept.iter()) {
            if let Some(n) = doc.scene.nodes().find(|n| &n.name == name) {
                ids.insert(n.id, *id);
            }
        }
        let mut materials = BTreeMap::new();
        for (name, id) in &r.nodes {
            let Some(cur) = state.scene.node(*id) else { continue };
            let Some(src) = doc.scene.nodes().find(|n| &n.name == name) else {
                changes.push(Change::RemoveNode(*id));
                continue;
            };
            let mut src = src.clone();
            src.remap_ids(&|n| ids.get(&n).copied());
            let material = src.material.and_then(|m| crate::file::carry_material(&doc.scene.materials, m, &mut library, &mut materials, 0));
            if cur.object != src.object {
                changes.push(Change::ReplaceObject { id: *id, before: Box::new(cur.object.clone()), after: Box::new(src.object.clone()) });
            }
            if cur.modifiers != src.modifiers {
                changes.push(Change::SetStack { id: *id, before: cur.modifiers.clone(), after: src.modifiers.clone() });
            }
            if cur.material != material {
                changes.push(Change::SetMaterial { id: *id, before: cur.material, after: material });
            }
            if r.scene && cur.transform != src.transform {
                changes.push(Change::SetTransform { id: *id, before: cur.transform.clone(), after: src.transform.clone() });
            }
            kept.push((name.clone(), *id));
        }
        r.nodes = kept;
    }
    if library != state.scene.materials {
        let slots = state.scene.material_editor.clone();
        changes.insert(0, Change::SetMaterials { before: Box::new((state.scene.materials.clone(), slots.clone())), after: Box::new((library, slots)) });
    }
    if records != state.scene.xrefs {
        changes.push(Change::SetXrefs { before: state.scene.xrefs.clone(), after: records });
    }
    if let Some(t) = modified(path) {
        state.xref_seen.insert(path.to_owned(), t);
    }
    if !changes.is_empty() {
        state.undo.commit("Update XRefs", Change::Group(changes), &mut state.scene)?;
        state.mesh_dirty = true;
    }
    Ok(json!({ "updated": [path] }))
}

/// Remove: the reference and the objects it brought, one undo step.
pub fn remove(state: &mut AppState, path: &str) -> Result<Value, String> {
    let gone: Vec<NodeId> = state.scene.xrefs.iter().filter(|r| r.path == path).flat_map(|r| r.nodes.iter().map(|(_, id)| *id)).filter(|id| state.scene.node(*id).is_some()).collect();
    if !state.scene.xrefs.iter().any(|r| r.path == path) {
        return Err(format!("{path} is not referenced here"));
    }
    let mut ordered = gone.clone();
    ordered.sort_by_key(|id| std::cmp::Reverse(state.scene.depth(*id)));
    let mut changes: Vec<Change> = ordered.iter().map(|id| Change::RemoveNode(*id)).collect();
    let after: Vec<XrefRecord> = state.scene.xrefs.iter().filter(|r| r.path != path).cloned().collect();
    changes.push(Change::SetXrefs { before: state.scene.xrefs.clone(), after });
    state.undo.commit("Remove XRef", Change::Group(changes), &mut state.scene)?;
    state.selection.retain(|id| !gone.contains(id));
    state.mesh_dirty = true;
    state.xref_seen.remove(path);
    Ok(json!({ "removed": gone.iter().map(|i| i.0).collect::<Vec<_>>() }))
}

/// Update Automatically: re-reads every automatic reference whose file
/// changed on disk since it was last read. Returns the updated paths.
pub fn tick(state: &mut AppState) -> Result<Vec<String>, String> {
    let due: Vec<String> = state
        .scene
        .xrefs
        .iter()
        .filter(|r| r.automatic)
        .map(|r| r.path.clone())
        .filter(|p| modified(p).is_some_and(|t| state.xref_seen.get(p) != Some(&t)))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    for p in &due {
        update(state, Some(p))?;
    }
    Ok(due)
}

/// Edits this scene may not make to referenced objects.
pub fn guard(scene: &loom_scene::Scene, change: &Change) -> Result<(), String> {
    if scene.xrefs.is_empty() {
        return Ok(());
    }
    let refd = |id: &NodeId| scene.xrefs.iter().find(|r| r.nodes.iter().any(|(_, n)| n == id));
    let name = |id: &NodeId| scene.node(*id).map(|n| n.name.clone()).unwrap_or_default();
    match change {
        Change::ReplaceObject { id, .. } | Change::SetStack { id, .. } | Change::SetMaterial { id, .. } => {
            if let Some(r) = refd(id) {
                return Err(format!("{} is an XRef from {}: edit it in that file and Update", name(id), r.path));
            }
        }
        Change::SetTransform { id, .. } | Change::SetParent { id, .. } => {
            if let Some(r) = refd(id).filter(|r| r.scene) {
                return Err(format!("{} belongs to the XRef Scene {}", name(id), r.path));
            }
        }
        Change::Group(cs) => {
            for c in cs {
                guard(scene, c)?;
            }
        }
        _ => {}
    }
    Ok(())
}
