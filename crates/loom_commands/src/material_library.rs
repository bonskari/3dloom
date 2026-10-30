//! Material libraries: the Material Editor's current library (Put to
//! Library, Get Material from Mtl Library) and library files in loom's own
//! format (a JSON document of materials, each with its sub-materials):
//! New, Open, Merge, Save, Delete. The library is session state, not part
//! of the scene, so its own edits are not undo steps; getting a material
//! from it into the scene is.

use crate::command::Command as C;
use crate::execute::commit;
use crate::AppState;
use loom_scene::material::{next_material_id, MaterialLibrary, StandardMaterial};
use loom_scene::{Change, MaterialId};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Library files carry this format tag.
pub const LIBRARY_FORMAT: &str = "loom-material-library";
/// The library file extension.
pub const LIBRARY_EXT: &str = "lmat";

/// One library material with its sub-materials (ids local to the entry).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryEntry {
    pub name: String,
    pub root: MaterialId,
    pub materials: MaterialLibrary,
}

impl LibraryEntry {
    pub fn material(&self) -> Option<&StandardMaterial> {
        self.materials.get(&self.root)
    }
}

/// The current material library.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MaterialLibraryState {
    /// The file it was opened from or saved to.
    #[serde(skip)]
    pub path: Option<String>,
    pub entries: Vec<LibraryEntry>,
    /// Changed since it was opened or saved.
    #[serde(skip)]
    pub dirty: bool,
}

#[derive(Serialize, Deserialize)]
struct LibraryFile {
    format: String,
    entries: Vec<LibraryEntry>,
}

pub fn handles(c: &C) -> bool {
    matches!(c, C::PutToLibrary { .. } | C::GetFromLibrary { .. } | C::NewMaterialLibrary | C::OpenMaterialLibrary { .. } | C::MergeMaterialLibrary { .. } | C::SaveMaterialLibrary { .. } | C::DeleteFromLibrary { .. })
}

/// `top` and every material below it, renumbered from 1 (the entry's own ids).
fn extract(lib: &MaterialLibrary, top: MaterialId, name: String) -> LibraryEntry {
    let mut map: std::collections::BTreeMap<MaterialId, MaterialId> = std::collections::BTreeMap::new();
    let mut order = vec![top];
    let mut i = 0;
    while i < order.len() {
        let id = order[i];
        map.insert(id, MaterialId(i as u32 + 1));
        for c in lib.get(&id).map(|m| m.children()).unwrap_or_default().into_iter().flatten() {
            if !order.contains(&c) && lib.contains_key(&c) {
                order.push(c);
            }
        }
        i += 1;
    }
    let materials = order
        .iter()
        .filter_map(|id| {
            let mut m = lib.get(id)?.clone();
            for c in m.children_mut() {
                *c = c.and_then(|c| map.get(&c).copied());
            }
            Some((map[id], m))
        })
        .collect();
    LibraryEntry { name, root: MaterialId(1), materials }
}

/// Copies `entry` into `lib` with new ids; returns the new top material.
fn insert(lib: &mut MaterialLibrary, entry: &LibraryEntry) -> Option<MaterialId> {
    let mut map = std::collections::BTreeMap::new();
    for old in entry.materials.keys() {
        let id = next_material_id(lib);
        // Reserve the id so the next one differs.
        lib.insert(id, StandardMaterial::new(""));
        map.insert(*old, id);
    }
    for (old, m) in &entry.materials {
        let mut m = m.clone();
        for c in m.children_mut() {
            *c = c.and_then(|c| map.get(&c).copied());
        }
        lib.insert(map[old], m);
    }
    map.get(&entry.root).copied()
}

fn read_file(path: &str) -> Result<Vec<LibraryEntry>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("could not read {path}: {e}"))?;
    let f: LibraryFile = serde_json::from_str(&text).map_err(|e| format!("{path} is not a material library: {e}"))?;
    if f.format != LIBRARY_FORMAT {
        return Err(format!("{path} is not a material library"));
    }
    Ok(f.entries)
}

pub fn execute(state: &mut AppState, cmd: C) -> Result<Value, String> {
    match cmd {
        C::PutToLibrary { name } => {
            let ed = &state.scene.material_editor;
            let top = ed.slots[ed.active].filter(|id| state.scene.materials.contains_key(id));
            let entry = match top {
                Some(id) => {
                    let name = name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| state.scene.materials[&id].name.clone());
                    extract(&state.scene.materials, id, name)
                }
                None => {
                    let m = state.scene.slot_material(ed.active);
                    let name = name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| m.name.clone());
                    LibraryEntry { name, root: MaterialId(1), materials: [(MaterialId(1), m)].into_iter().collect() }
                }
            };
            let lib = &mut state.material_library;
            // A material of the same name is replaced, as Put to Library does.
            let replaced = match lib.entries.iter_mut().find(|e| e.name == entry.name) {
                Some(e) => {
                    *e = entry.clone();
                    true
                }
                None => {
                    lib.entries.push(entry.clone());
                    false
                }
            };
            lib.dirty = true;
            Ok(json!({ "name": entry.name, "replaced": replaced, "count": lib.entries.len() }))
        }
        C::GetFromLibrary { name } => {
            let entry = state.material_library.entries.iter().find(|e| e.name == name).cloned().ok_or_else(|| format!("no material \"{name}\" in the library"))?;
            let before = Box::new((state.scene.materials.clone(), state.scene.material_editor.clone()));
            let mut after = before.clone();
            let id = insert(&mut after.0, &entry).ok_or("the library material is empty")?;
            let i = after.1.active;
            after.1.slots[i] = Some(id);
            after.1.path.clear();
            commit(state, "Get Material", Change::SetMaterials { before, after })?;
            Ok(json!({ "slot": i, "material": id.0 }))
        }
        C::DeleteFromLibrary { name } => {
            let lib = &mut state.material_library;
            let n = lib.entries.len();
            lib.entries.retain(|e| e.name != name);
            if lib.entries.len() == n {
                return Err(format!("no material \"{name}\" in the library"));
            }
            lib.dirty = true;
            Ok(json!({ "count": lib.entries.len() }))
        }
        C::NewMaterialLibrary => {
            state.material_library = MaterialLibraryState::default();
            Ok(json!({ "count": 0 }))
        }
        C::OpenMaterialLibrary { path } => {
            let entries = read_file(&path)?;
            let n = entries.len();
            state.material_library = MaterialLibraryState { path: Some(path), entries, dirty: false };
            Ok(json!({ "count": n }))
        }
        C::MergeMaterialLibrary { path, names } => {
            let entries = read_file(&path)?;
            let lib = &mut state.material_library;
            let mut added = 0;
            for e in entries.into_iter().filter(|e| names.as_ref().is_none_or(|ns| ns.contains(&e.name))) {
                match lib.entries.iter_mut().find(|x| x.name == e.name) {
                    Some(x) => *x = e,
                    None => lib.entries.push(e),
                }
                added += 1;
            }
            lib.dirty |= added > 0;
            Ok(json!({ "merged": added, "count": lib.entries.len() }))
        }
        C::SaveMaterialLibrary { path } => {
            let path = path.or_else(|| state.material_library.path.clone()).ok_or("Save As: give the library a file name")?;
            let path = if std::path::Path::new(&path).extension().is_none() { format!("{path}.{LIBRARY_EXT}") } else { path };
            let f = LibraryFile { format: LIBRARY_FORMAT.into(), entries: state.material_library.entries.clone() };
            let text = serde_json::to_string_pretty(&f).map_err(|e| e.to_string())?;
            std::fs::write(&path, text).map_err(|e| format!("could not write {path}: {e}"))?;
            state.material_library.path = Some(path.clone());
            state.material_library.dirty = false;
            Ok(json!({ "path": path, "count": state.material_library.entries.len() }))
        }
        _ => unreachable!("routed by handles()"),
    }
}
