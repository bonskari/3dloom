//! Scene files and the files they use: File > Replace, Set Project Folder,
//! the startup scene, Asset Tracking (every file the scene names, found or
//! missing, and repathing) and the Missing External Files check on open.

use crate::execute::commit;
use crate::state::AppState;
use loom_scene::{Change, NodeId, ObjectKind};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Folders Set Project Folder makes (the reference's project layout).
pub const PROJECT_FOLDERS: [&str; 17] = [
    "animations", "archives", "autoback", "downloads", "export", "express", "import", "matlibs", "previews", "proxies", "renderassets", "renderoutput", "scenes",
    "sceneassets/animations", "sceneassets/images", "sceneassets/photometric", "vpost",
];

/// The scene a new session and Reset start from: `scenes/startup.loom` in
/// the project folder, else `startup.loom` in the settings folder.
pub const STARTUP_SCENE: &str = "startup.loom";

/// File types Asset Tracking lists (bitmaps, photometric webs, point
/// caches and referenced scenes).
const ASSET_EXT: [&str; 18] = ["png", "jpg", "jpeg", "tga", "bmp", "tif", "tiff", "dds", "hdr", "exr", "gif", "psd", "ies", "pc2", "loom", "max", "fbx", "obj"];

fn is_asset(s: &str) -> bool {
    let p = Path::new(s);
    !s.is_empty() && p.extension().and_then(|e| e.to_str()).is_some_and(|e| ASSET_EXT.contains(&e.to_ascii_lowercase().as_str()))
}

fn walk(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) if is_asset(s) && !out.contains(s) => out.push(s.clone()),
        Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
        Value::Object(o) => o.values().for_each(|x| walk(x, out)),
        _ => {}
    }
}

/// Every file the scene names, in first-use order.
pub fn scene_assets(state: &AppState) -> Vec<String> {
    let mut out = Vec::new();
    walk(&serde_json::to_value(&state.scene).unwrap_or(Value::Null), &mut out);
    walk(&serde_json::to_value(crate::file::SceneSettings::from_state(state)).unwrap_or(Value::Null), &mut out);
    out
}

/// The files the scene names that do not exist.
pub fn missing_assets(state: &AppState) -> Vec<String> {
    scene_assets(state).into_iter().filter(|p| !Path::new(p).exists()).collect()
}

/// Asset Tracking's list: each file with its status.
pub fn asset_list(state: &AppState) -> Value {
    let rows: Vec<Value> = scene_assets(state).iter().map(|p| json!({ "path": p, "status": if Path::new(p).exists() { "OK" } else { "Missing" } })).collect();
    json!({ "assets": rows })
}

/// `v` with every string equal to `from` replaced by `to` (None: unchanged).
fn repathed<T: Serialize + DeserializeOwned>(v: &T, from: &str, to: &str) -> Option<T> {
    fn swap(v: &mut Value, from: &str, to: &str) -> bool {
        match v {
            Value::String(s) if s == from => {
                *s = to.to_owned();
                true
            }
            Value::Array(a) => a.iter_mut().fold(false, |c, x| swap(x, from, to) | c),
            Value::Object(o) => o.values_mut().fold(false, |c, x| swap(x, from, to) | c),
            _ => false,
        }
    }
    let mut j = serde_json::to_value(v).ok()?;
    if !swap(&mut j, from, to) {
        return None;
    }
    serde_json::from_value(j).ok()
}

/// Asset Tracking > Set Path: every reference to `from` names `to` instead
/// (materials, objects, display bitmaps, modifier stacks: one undo step;
/// viewport backgrounds too). Returns how many places changed.
pub fn repath(state: &mut AppState, pairs: &[(String, String)], label: &str) -> Result<usize, String> {
    let mut changes = Vec::new();
    let mut count = 0;
    let mut mats = (state.scene.materials.clone(), state.scene.material_editor.clone());
    for (from, to) in pairs {
        if let Some(m) = repathed(&mats, from, to) {
            mats = m;
            count += 1;
        }
    }
    if mats.0 != state.scene.materials || mats.1 != state.scene.material_editor {
        changes.push(Change::SetMaterials { before: Box::new((state.scene.materials.clone(), state.scene.material_editor.clone())), after: Box::new(mats) });
    }
    for n in state.scene.nodes() {
        let (mut object, mut props, mut stack) = (n.object.clone(), n.props(), n.modifiers.clone());
        for (from, to) in pairs {
            if let Some(o) = repathed(&object, from, to) {
                object = o;
                count += 1;
            }
            if let Some(p) = repathed(&props, from, to) {
                props = p;
                count += 1;
            }
            if let Some(s) = repathed(&stack, from, to) {
                stack = s;
                count += 1;
            }
        }
        if object != n.object {
            changes.push(Change::ReplaceObject { id: n.id, before: Box::new(n.object.clone()), after: Box::new(object) });
        }
        if props != n.props() {
            changes.push(Change::SetNodeProps { id: n.id, before: Box::new(n.props()), after: Box::new(props) });
        }
        if stack != n.modifiers {
            changes.push(Change::SetStack { id: n.id, before: n.modifiers.clone(), after: stack });
        }
    }
    for vp in state.viewports.iter_mut() {
        for (from, to) in pairs {
            if vp.background.as_deref() == Some(from.as_str()) {
                vp.background = Some(to.clone());
                vp.background_version += 1;
                count += 1;
            }
        }
    }
    if !changes.is_empty() {
        commit(state, label, Change::Group(changes))?;
    }
    state.mesh_dirty = true;
    Ok(count)
}

/// Missing External Files > Browse: each missing file found by name in
/// `folder` is repathed there. Returns the files found.
pub fn find_missing(state: &mut AppState, folder: &Path) -> Result<Vec<String>, String> {
    if !folder.is_dir() {
        return Err(format!("{} is not a folder", folder.display()));
    }
    let pairs: Vec<(String, String)> = missing_assets(state)
        .into_iter()
        .filter_map(|m| {
            let name = Path::new(&m).file_name()?.to_owned();
            let there = folder.join(name);
            there.is_file().then(|| (m, there.to_string_lossy().into_owned()))
        })
        .collect();
    repath(state, &pairs, "Find Missing Files")?;
    Ok(pairs.into_iter().map(|(_, to)| to).collect())
}

/// Set Project Folder: makes the project's folders under `path` and makes
/// it the folder the file dialogs start in.
pub fn set_project_folder(state: &mut AppState, path: &Path) -> Result<Value, String> {
    for f in PROJECT_FOLDERS {
        let d = path.join(f);
        std::fs::create_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    state.project_folder = Some(path.to_path_buf());
    Ok(json!({ "project_folder": path, "folders": PROJECT_FOLDERS }))
}

/// The folder of the project the file dialogs use for `kind` (scenes,
/// import, export, renderoutput, sceneassets/images), when a project is set.
pub fn project_dir(state: &AppState, kind: &str) -> Option<PathBuf> {
    let d = state.project_folder.as_ref()?.join(kind);
    d.is_dir().then_some(d)
}

/// The startup scene's path, when the file exists.
pub fn startup_scene(state: &AppState) -> Option<PathBuf> {
    let in_project = state.project_folder.as_ref().map(|p| p.join("scenes").join(STARTUP_SCENE));
    let in_settings = crate::settings::config_dir().map(|d| d.join(STARTUP_SCENE));
    [in_project, in_settings].into_iter().flatten().find(|p| p.is_file())
}

/// Loads the startup scene as an untitled, unmodified scene (a new
/// session and Reset). Returns its path when there was one.
pub fn load_startup_scene(state: &mut AppState) -> Result<Option<PathBuf>, String> {
    let Some(path) = startup_scene(state) else { return Ok(None) };
    let doc = crate::file::read(&path)?;
    crate::execute::load_document(state, doc, None);
    state.undo.mark_saved();
    Ok(Some(path))
}

/// The objects File > Replace can replace from `doc`: those whose name an
/// object in the scene has (geometry and shapes, not lights, cameras or groups).
pub fn replace_candidates(state: &AppState, doc: &crate::file::Document) -> Vec<String> {
    let scene: BTreeSet<&str> = state.scene.nodes().map(|n| n.name.as_str()).collect();
    doc.scene
        .nodes()
        .filter(|n| !matches!(n.object, ObjectKind::Light(_) | ObjectKind::Camera { .. } | ObjectKind::Group { .. }) && scene.contains(n.name.as_str()))
        .map(|n| n.name.clone())
        .collect()
}

/// File > Replace: the objects of the scene named like ones in `path` (all
/// candidates, or `names`) take that object's geometry and modifier stack;
/// their instances follow; transforms, hierarchy and the rest stay. With
/// `materials` they also take its material. One undo step.
pub fn replace(state: &mut AppState, path: &Path, names: Option<&[String]>, materials: bool) -> Result<Value, String> {
    let doc = crate::file::read(path)?;
    let mut wanted = replace_candidates(state, &doc);
    if let Some(names) = names {
        for n in names {
            if !wanted.contains(n) {
                return Err(format!("{n:?} is not an object both the scene and the file have"));
            }
        }
        wanted.retain(|n| names.contains(n));
    }
    if wanted.is_empty() {
        return Err("no object of the file has the name of an object in the scene".into());
    }
    let mut changes = Vec::new();
    let mut library = state.scene.materials.clone();
    let mut replaced = Vec::new();
    for name in &wanted {
        let src = doc.scene.nodes().find(|n| &n.name == name).expect("a candidate");
        let src_id = src.id;
        // The source's stack with instanced modifiers resolved.
        let stack: Vec<loom_scene::modifier::ModifierSlot> = src
            .modifiers
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let mut s = s.clone();
                if s.instance_of.is_some() {
                    if let Some(m) = doc.scene.resolved_modifier(src_id, i) {
                        s.modifier = m;
                    }
                    s.instance_of = None;
                }
                s
            })
            .collect();
        let object = match src.instance_of.and_then(|r| doc.scene.node(r)) {
            Some(root) => root.object.clone(),
            None => src.object.clone(),
        };
        let material = if materials {
            src.material.and_then(|m| doc.scene.materials.get(&m)).map(|m| {
                let id = loom_scene::material::next_material_id(&library);
                library.insert(id, m.clone());
                id
            })
        } else {
            None
        };
        let Some(dst) = state.scene.nodes().find(|n| &n.name == name) else { continue };
        let root = dst.instance_of.unwrap_or(dst.id);
        let group: Vec<NodeId> = state.scene.nodes().filter(|n| n.id == root || n.instance_of == Some(root)).map(|n| n.id).collect();
        for id in group {
            let n = state.scene.node(id).expect("in the scene");
            changes.push(Change::ReplaceObject { id, before: Box::new(n.object.clone()), after: Box::new(object.clone()) });
            changes.push(Change::SetStack { id, before: n.modifiers.clone(), after: stack.clone() });
            if materials && material.is_some() {
                changes.push(Change::SetMaterial { id, before: n.material, after: material });
            }
            replaced.push(id.0);
        }
    }
    if library != state.scene.materials {
        changes.insert(0, Change::SetMaterials { before: Box::new((state.scene.materials.clone(), state.scene.material_editor.clone())), after: Box::new((library, state.scene.material_editor.clone())) });
    }
    commit(state, "Replace", Change::Group(changes))?;
    state.mesh_dirty = true;
    Ok(json!({ "replaced": replaced }))
}
