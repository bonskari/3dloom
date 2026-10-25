//! Material Editor commands: sample slots, material types (Standard,
//! Multi/Sub-Object, Blend), maps, navigation through a material's levels,
//! assignment to objects and faces. Every edit is one undo step
//! (`Change::SetMaterials`); moving between levels is not an edit.

use crate::command::Command as C;
use crate::execute::{commit, edit_meshes, selected_faces};
use crate::AppState;
use loom_scene::material::{
    default_amount, map_at, map_at_mut, map_slot_at_mut, next_material_id, resolve_level, Blend, Level, Map, MapType, MaterialEditorSlots, MaterialKind, MaterialLibrary, NavStep, SampleType, StandardMaterial,
    SubMaterial, SLOT_COUNT,
};
use loom_scene::{Change, MaterialId, NodeId, Param};
use serde_json::{json, Value};

pub fn handles(c: &C) -> bool {
    matches!(
        c,
        C::MaterialEditorToggle
            | C::MaterialEditorSlot { .. }
            | C::SetMaterialColor { .. }
            | C::SetMaterialParameter { .. }
            | C::SetMaterialMap { .. }
            | C::RenameMaterial { .. }
            | C::AssignMaterialToSelection
            | C::GetMaterialFromObject { .. }
            | C::ResetMaterial { .. }
            | C::SetMaterialType { .. }
            | C::MultiSubSetNumber { .. }
            | C::MultiSubAdd
            | C::MultiSubDelete { .. }
            | C::MultiSubSetEntry { .. }
            | C::MaterialEditorGoToSub { .. }
            | C::MaterialEditorGoToMap { .. }
            | C::NewMap { .. }
            | C::SetMapType { .. }
            | C::MaterialEditorGoToParent
            | C::MaterialEditorGoForwardToSibling
            | C::MaterialEditorNavigate { .. }
            | C::SetMapParameter { .. }
            | C::SetMapColor { .. }
            | C::SwapMapColors
            | C::SetMapBitmap { .. }
            | C::ReloadBitmaps
            | C::SetMaterialMapEnabled { .. }
            | C::SetMaterialMapAmount { .. }
            | C::CopyMap { .. }
            | C::CopyMaterialToSlot { .. }
            | C::AssignMaterialToNode { .. }
            | C::AssignMaterialToFaces { .. }
            | C::GetMaterial { .. }
            | C::DropBitmap { .. }
            | C::SetMaterialEditorOption { .. }
            | C::SetSampleType { .. }
            | C::PickMaterialFromObject
            | C::MaterialMapNavigator
    )
}

fn slot_index(state: &AppState, slot: Option<u32>) -> Result<usize, String> {
    let i = slot.map(|s| s as usize).unwrap_or(state.scene.material_editor.active);
    if i >= SLOT_COUNT {
        return Err(format!("no sample slot {i} (0-{})", SLOT_COUNT - 1));
    }
    Ok(i)
}

fn snapshot(state: &AppState) -> Box<(MaterialLibrary, MaterialEditorSlots)> {
    Box::new((state.scene.materials.clone(), state.scene.material_editor.clone()))
}

/// The slot's material put in the library (a default slot gets an id now).
fn ensure_in_library(lib: &mut MaterialLibrary, slots: &mut MaterialEditorSlots, slot: usize, material: StandardMaterial) -> MaterialId {
    match slots.slots[slot] {
        Some(id) => {
            lib.insert(id, material);
            id
        }
        None => {
            let id = next_material_id(lib);
            lib.insert(id, material);
            slots.slots[slot] = Some(id);
            id
        }
    }
}

/// The slot's top material id, put in the library first if the slot is untouched.
fn top_id(lib: &mut MaterialLibrary, slots: &mut MaterialEditorSlots, slot: usize) -> MaterialId {
    match slots.slots[slot].filter(|id| lib.contains_key(id)) {
        Some(id) => id,
        None => {
            slots.slots[slot] = None;
            ensure_in_library(lib, slots, slot, StandardMaterial::slot_default(slot))
        }
    }
}

/// A new Standard material in the library, named as the reference names them ("Material #12").
fn new_material(lib: &mut MaterialLibrary) -> MaterialId {
    let id = next_material_id(lib);
    lib.insert(id, StandardMaterial::new(format!("Material #{}", id.0)));
    id
}

/// The level the editor shows: (material, map steps below it). Steps that no
/// longer exist (after an undo) are dropped.
fn current(lib: &mut MaterialLibrary, slots: &mut MaterialEditorSlots) -> (MaterialId, Vec<String>) {
    let active = slots.active;
    let top = top_id(lib, slots, active);
    let (level, n) = resolve_level(lib, top, &slots.path);
    slots.path.truncate(n);
    match level {
        Level::Material(id) => (id, Vec::new()),
        Level::Map { owner, maps } => (owner, maps),
    }
}

/// Runs `f` on a copy of the library and slots at the current level and
/// commits the result as one undo step (nothing when nothing changed).
fn edit_level(state: &mut AppState, label: &str, f: impl FnOnce(&mut MaterialLibrary, &mut MaterialEditorSlots, MaterialId, &[String]) -> Result<Value, String>) -> Result<Value, String> {
    let before = snapshot(state);
    let mut after = before.clone();
    let (lib, slots) = (&mut after.0, &mut after.1);
    let (id, maps) = current(lib, slots);
    let v = f(lib, slots, id, &maps)?;
    if after != before {
        commit(state, label, Change::SetMaterials { before, after })?;
    }
    Ok(v)
}

/// Edits a material: slot omitted (or the active slot) = the editor's
/// current material level, another slot = that slot's top material.
fn edit(state: &mut AppState, slot: Option<u32>, label: &str, f: impl FnOnce(&mut StandardMaterial) -> Result<(), String>) -> Result<Value, String> {
    let i = slot_index(state, slot)?;
    if i == state.scene.material_editor.active {
        return edit_level(state, label, |lib, _, id, _| {
            f(lib.get_mut(&id).ok_or("missing material")?)?;
            Ok(json!({ "slot": i, "material": id.0 }))
        });
    }
    let mut m = state.scene.slot_material(i);
    f(&mut m)?;
    let before = snapshot(state);
    let mut after = before.clone();
    let id = ensure_in_library(&mut after.0, &mut after.1, i, m);
    commit(state, label, Change::SetMaterials { before, after })?;
    Ok(json!({ "slot": i, "material": id.0 }))
}

/// Edits the map at the current level.
fn edit_map(state: &mut AppState, label: &str, f: impl FnOnce(&mut Map) -> Result<(), String>) -> Result<Value, String> {
    edit_level(state, label, |lib, _, id, maps| {
        if maps.is_empty() {
            return Err("the Material Editor is not at a map level".into());
        }
        let m = lib.get_mut(&id).ok_or("missing material")?;
        f(map_at_mut(m, maps).ok_or("no map at this level")?)?;
        Ok(json!({ "material": id.0, "map": maps }))
    })
}

/// "Map #N": one more than the maps the library holds.
fn next_map_name(lib: &MaterialLibrary) -> String {
    fn count(m: &Map) -> usize {
        1 + m.components().map(count).sum::<usize>()
    }
    let n: usize = lib.values().map(|m| m.map_slots().iter().filter_map(|s| m.map(s)).map(count).sum::<usize>()).sum();
    format!("Map #{}", n + 1)
}

/// A level's children in sibling order: sub-materials, then the maps present.
fn children(lib: &MaterialLibrary, id: MaterialId, maps: &[String]) -> Vec<NavStep> {
    let Some(m) = lib.get(&id) else { return Vec::new() };
    if maps.is_empty() {
        let mut out: Vec<NavStep> = m.children().iter().enumerate().filter(|(_, c)| c.is_some()).map(|(i, _)| NavStep::Sub(i as u32)).collect();
        out.extend(m.map_slots().into_iter().filter(|s| m.map(s).is_some()).map(|s| NavStep::Map(s.to_owned())));
        return out;
    }
    let Some(map) = map_at(m, maps) else { return Vec::new() };
    map.slot_names().into_iter().enumerate().filter(|(i, _)| map.sub_map(*i).is_some()).map(|(_, s)| NavStep::Map(s)).collect()
}

/// Changes the material at the current level to `kind`.
fn set_type(lib: &mut MaterialLibrary, id: MaterialId, kind: &str, keep_old: bool) -> Result<(), String> {
    let old = lib.get(&id).ok_or("missing material")?.clone();
    if old.kind.label() == kind {
        return Ok(());
    }
    let mut first = None;
    if keep_old && kind != "Standard" {
        let sub = next_material_id(lib);
        lib.insert(sub, old.clone());
        first = Some(sub);
    }
    let mut new = StandardMaterial::new(old.name.clone());
    new.kind = match kind {
        "Standard" => MaterialKind::Standard,
        "Multi/Sub-Object" => {
            // Set Number defaults to 10.
            let mut list = Vec::new();
            for n in 1..=10u16 {
                let material = match (n, first) {
                    (1, Some(f)) => f,
                    _ => new_material(lib),
                };
                list.push(SubMaterial { id: n, name: String::new(), material: Some(material), enabled: true });
            }
            MaterialKind::MultiSub(list)
        }
        "Blend" => {
            let a = first.unwrap_or_else(|| new_material(lib));
            let b = new_material(lib);
            MaterialKind::Blend(Box::new(Blend { materials: [Some(a), Some(b)], enabled: [true; 2], mask: None, mix_amount: Param::new(0.0), interactive: 0, use_curve: false, upper: 0.75, lower: 0.25 }))
        }
        "Shell" => {
            // Original Material (the old one when kept), Baked Material; viewports show the baked one.
            let a = first.unwrap_or_else(|| new_material(lib));
            let b = new_material(lib);
            MaterialKind::Shell(Box::new(loom_scene::material::Shell { materials: [Some(a), Some(b)], viewport: 1, render: 1 }))
        }
        other => return Err(format!("no material type \"{other}\" ({})", MaterialKind::TYPES.join(", "))),
    };
    lib.insert(id, new);
    Ok(())
}

fn multi_list(lib: &mut MaterialLibrary, id: MaterialId) -> Result<&mut Vec<SubMaterial>, String> {
    match &mut lib.get_mut(&id).ok_or("missing material")?.kind {
        MaterialKind::MultiSub(list) => Ok(list),
        _ => Err("the material at this level is not a Multi/Sub-Object material".into()),
    }
}

fn add_sub(lib: &mut MaterialLibrary, id: MaterialId) -> Result<u16, String> {
    let material = new_material(lib);
    let list = multi_list(lib, id)?;
    let next = list.iter().map(|s| s.id).max().unwrap_or(0) + 1;
    list.push(SubMaterial { id: next, name: String::new(), material: Some(material), enabled: true });
    Ok(next)
}

/// How many nodes use material `id` directly.
fn users(state: &AppState, id: MaterialId) -> usize {
    state.scene.nodes().filter(|n| n.material == Some(id)).count()
}

/// Drag-and-drop of material `dropped` onto the selected polygons of `node`
/// (Drag and Drop Sub-Object Material Assignment): returns the node's new
/// material, the ID (1-based) the faces get and whether the other faces keep their IDs.
fn drop_on_faces(state: &AppState, lib: &mut MaterialLibrary, node: NodeId, faces: &[loom_mesh::FaceId], dropped: MaterialId) -> Result<(MaterialId, u16, bool), String> {
    let n = state.scene.node(node).ok_or("missing node")?;
    let mesh = n.cage_mesh();
    let selected: std::collections::BTreeSet<usize> = faces.iter().map(|f| f.0 as usize).collect();
    let ids_of = |pick: bool| -> std::collections::BTreeSet<u16> { mesh.faces().iter().enumerate().filter(|(i, _)| selected.contains(i) == pick).map(|(_, f)| f.material_id + 1).collect() };
    match n.material.and_then(|m| lib.get(&m).map(|mm| (m, mm.kind.clone()))) {
        None => {
            // No material: a new Multi/Sub with the dropped one at the faces' ID; existing IDs stay.
            let face_ids = ids_of(true);
            let id = if face_ids.len() == 1 { *face_ids.iter().next().expect("one") } else { 1 };
            let multi = next_material_id(lib);
            let mut m = StandardMaterial::new(format!("Material #{}", multi.0));
            m.kind = MaterialKind::MultiSub(vec![SubMaterial { id, name: String::new(), material: Some(dropped), enabled: true }]);
            lib.insert(multi, m);
            Ok((multi, id, true))
        }
        Some((old, MaterialKind::MultiSub(_))) => {
            // Shared by other objects: work on a copy.
            let multi = if users(state, old) > 1 {
                let copy = next_material_id(lib);
                let m = lib[&old].clone();
                lib.insert(copy, m);
                copy
            } else {
                old
            };
            let list = multi_list(lib, multi)?;
            if let Some(s) = list.iter().find(|s| s.material == Some(dropped)) {
                return Ok((multi, s.id, true));
            }
            let face_ids = ids_of(true);
            let others = ids_of(false);
            if face_ids.len() == 1 {
                let id = *face_ids.iter().next().expect("one");
                if !others.contains(&id) {
                    // The faces alone use this ID: the dropped material replaces its sub-material.
                    match list.iter_mut().find(|s| s.id == id) {
                        Some(s) => s.material = Some(dropped),
                        None => list.push(SubMaterial { id, name: String::new(), material: Some(dropped), enabled: true }),
                    }
                    return Ok((multi, id, false));
                }
            }
            let used: u16 = list.iter().map(|s| s.id).chain(face_ids).chain(others).max().unwrap_or(0);
            list.push(SubMaterial { id: used + 1, name: String::new(), material: Some(dropped), enabled: true });
            Ok((multi, used + 1, true))
        }
        Some((old, _)) => {
            // Another material: it becomes ID 1 (unselected faces), the dropped one ID 2.
            let multi = next_material_id(lib);
            let mut m = StandardMaterial::new(format!("Material #{}", multi.0));
            m.kind = MaterialKind::MultiSub(vec![
                SubMaterial { id: 1, name: String::new(), material: Some(old), enabled: true },
                SubMaterial { id: 2, name: String::new(), material: Some(dropped), enabled: true },
            ]);
            lib.insert(multi, m);
            Ok((multi, 2, false))
        }
    }
}

pub fn execute(state: &mut AppState, cmd: C) -> Result<Value, String> {
    match cmd {
        C::MaterialEditorToggle => {
            state.material_editor_open = !state.material_editor_open;
            Ok(json!({ "open": state.material_editor_open }))
        }
        C::MaterialMapNavigator => {
            state.material_navigator_open = !state.material_navigator_open;
            Ok(json!({ "open": state.material_navigator_open }))
        }
        C::MaterialEditorSlot { slot } => {
            let i = slot_index(state, Some(slot))?;
            if state.scene.material_editor.active != i {
                state.scene.material_editor.path.clear();
            }
            state.scene.material_editor.active = i;
            Ok(json!({ "slot": i }))
        }
        C::SetMaterialColor { slot, which, rgb } => edit(state, slot, "Material Color", |m| m.set_color(&which, rgb)),
        C::SetMaterialParameter { slot, name, value } => edit(state, slot, "Material Parameter", |m| m.set_parameter(&name, value)),
        C::SetMaterialMap { slot, map, path, amount } => edit(state, slot, "Material Map", |m| {
            let range = if map == "Bump" { -999.0..=999.0 } else { 0.0..=100.0 };
            let channel = m.map_slot_mut(&map)?;
            match path.filter(|p| !p.trim().is_empty()) {
                None => *channel = None,
                Some(path) => {
                    let amount = amount.map(|a| a.clamp(*range.start(), *range.end()));
                    match channel {
                        // The same bitmap: only the Amount changes, its other settings stay.
                        Some(b) if b.kind == MapType::Bitmap && b.path == path => {
                            if let Some(a) = amount {
                                b.amount = a;
                            }
                        }
                        _ => *channel = Some(Map::bitmap(path, amount.unwrap_or(default_amount(&map)))),
                    }
                }
            }
            Ok(())
        }),
        C::SetMaterialMapAmount { map, amount } => edit(state, None, "Map Amount", |m| {
            let range = if map == "Bump" { -999.0..=999.0 } else { 0.0..=100.0 };
            let b = m.map_slot_mut(&map)?.as_mut().ok_or_else(|| format!("no map in {map}"))?;
            b.amount = amount.clamp(*range.start(), *range.end());
            Ok(())
        }),
        C::SetMaterialMapEnabled { map, enabled } => edit(state, None, "Material Map", |m| {
            let b = m.map_slot_mut(&map)?.as_mut().ok_or_else(|| format!("no map in {map}"))?;
            b.enabled = enabled;
            Ok(())
        }),
        C::RenameMaterial { slot, name } => {
            let at_map = slot_index(state, slot)? == state.scene.material_editor.active && !state.scene.material_editor.path.iter().all(|s| matches!(s, NavStep::Sub(_)));
            if at_map {
                return edit_map(state, "Rename Map", |m| {
                    m.name = name;
                    Ok(())
                });
            }
            edit(state, slot, "Rename Material", |m| {
                m.name = name;
                Ok(())
            })
        }
        C::ResetMaterial { slot } => {
            let i = slot_index(state, slot)?;
            let top = state.scene.material_editor.path.is_empty() || i != state.scene.material_editor.active;
            edit(state, Some(i as u32), "Reset Material", |m| {
                let name = m.name.clone();
                *m = if top { StandardMaterial::slot_default(i) } else { StandardMaterial::new("") };
                m.name = name;
                Ok(())
            })
        }
        C::SetMaterialType { kind, keep_old } => edit_level(state, "Material Type", |lib, _, id, maps| {
            if !maps.is_empty() {
                return Err("at a map level the Type button chooses a map type (Set Map Type)".into());
            }
            set_type(lib, id, &kind, keep_old.unwrap_or(false))?;
            Ok(json!({ "material": id.0, "type": kind }))
        }),
        C::MultiSubSetNumber { number } => edit_level(state, "Set Number", |lib, _, id, _| {
            let number = number.clamp(1, 1000) as usize;
            let len = multi_list(lib, id)?.len();
            if number < len {
                multi_list(lib, id)?.truncate(number);
            }
            for _ in len..number {
                add_sub(lib, id)?;
            }
            Ok(json!({ "number": number }))
        }),
        C::MultiSubAdd => edit_level(state, "Add Sub-Material", |lib, _, id, _| Ok(json!({ "id": add_sub(lib, id)? }))),
        C::MultiSubDelete { index } => edit_level(state, "Delete Sub-Material", |lib, _, id, _| {
            let list = multi_list(lib, id)?;
            if index as usize >= list.len() {
                return Err(format!("no sub-material row {index}"));
            }
            list.remove(index as usize);
            Ok(json!({ "number": list.len() }))
        }),
        C::MultiSubSetEntry { index, id: mat_id, name, enabled, color } => edit_level(state, "Sub-Material", |lib, _, id, _| {
            let list = multi_list(lib, id)?;
            let row = list.get_mut(index as usize).ok_or_else(|| format!("no sub-material row {index}"))?;
            if let Some(v) = mat_id {
                row.id = v.clamp(1, 65535) as u16;
            }
            if let Some(v) = name {
                row.name = v;
            }
            if let Some(v) = enabled {
                row.enabled = v;
            }
            let sub = row.material;
            let duplicate = list.iter().filter(|s| list.iter().filter(|o| o.id == s.id).count() > 1).count() > 0;
            if let Some(rgb) = color {
                let sub = match sub {
                    Some(s) => s,
                    None => {
                        let s = new_material(lib);
                        multi_list(lib, id)?[index as usize].material = Some(s);
                        s
                    }
                };
                lib.get_mut(&sub).ok_or("missing sub-material")?.set_color("Diffuse", rgb)?;
            }
            // The reference warns about two rows with one ID; it is allowed.
            Ok(json!({ "duplicate_ids": duplicate }))
        }),
        C::MaterialEditorGoToSub { index } => edit_level(state, "Sub-Material", |lib, slots, id, maps| {
            if !maps.is_empty() {
                return Err("a map has no sub-materials".into());
            }
            let children = lib.get(&id).ok_or("missing material")?.children();
            if index as usize >= children.len() {
                return Err(format!("no sub-material {index} at this level"));
            }
            if children[index as usize].is_none() {
                let sub = new_material(lib);
                match &mut lib.get_mut(&id).ok_or("missing material")?.kind {
                    MaterialKind::MultiSub(list) => list[index as usize].material = Some(sub),
                    MaterialKind::Blend(b) => b.materials[index as usize] = Some(sub),
                    MaterialKind::Shell(sh) => sh.materials[index as usize] = Some(sub),
                    MaterialKind::Standard => unreachable!("a Standard material has no children"),
                }
            }
            slots.path.push(NavStep::Sub(index));
            Ok(json!({ "path": slots.path.len() }))
        }),
        C::MaterialEditorGoToMap { map } => {
            let ed = &state.scene.material_editor;
            let top = ed.slots[ed.active].ok_or("this slot's material has no maps")?;
            let mut path = ed.path.clone();
            path.push(NavStep::Map(map.clone()));
            let (_, n) = resolve_level(&state.scene.materials, top, &path);
            if n != path.len() {
                return Err(format!("no map in {map} at this level"));
            }
            state.scene.material_editor.path = path;
            Ok(json!({ "map": map }))
        }
        C::NewMap { map, kind } => {
            let ty = MapType::from_label(&kind).ok_or_else(|| format!("no map type \"{kind}\" ({})", MapType::ALL.map(|t| t.label()).join(", ")))?;
            edit_level(state, "New Map", |lib, slots, id, maps| {
                let mut new = Map::new(ty, if maps.is_empty() { default_amount(&map) } else { 100.0 });
                new.name = next_map_name(lib);
                let m = lib.get_mut(&id).ok_or("missing material")?;
                if maps.is_empty() {
                    *m.map_slot_mut(&map)? = Some(new);
                } else {
                    *map_slot_at_mut(m, maps, &map)? = Some(Box::new(new));
                }
                slots.path.push(NavStep::Map(map.clone()));
                Ok(json!({ "map": map, "type": ty.label() }))
            })
        }
        C::SetMapType { kind, keep_old } => {
            let ty = MapType::from_label(&kind).ok_or_else(|| format!("no map type \"{kind}\""))?;
            edit_map(state, "Map Type", |m| {
                if m.kind == ty {
                    return Ok(());
                }
                let old = m.clone();
                let mut new = Map::new(ty, old.amount);
                new.enabled = old.enabled;
                new.name = old.name.clone();
                if keep_old.unwrap_or(false) {
                    if let Some(i) = new.keep_old_slot() {
                        *new.sub_map_slot_mut(i) = Some(Box::new(old));
                    }
                }
                *m = new;
                Ok(())
            })
        }
        C::MaterialEditorGoToParent => {
            let ed = &mut state.scene.material_editor;
            if ed.path.pop().is_none() {
                return Err("already at the top level".into());
            }
            Ok(json!({ "path": ed.path.len() }))
        }
        C::MaterialEditorGoForwardToSibling => {
            let ed = &state.scene.material_editor;
            let Some(last) = ed.path.last().cloned() else { return Err("Go Forward to Sibling works below the top level".into()) };
            let top = ed.slots[ed.active].ok_or("no material in this slot")?;
            let parent_path = &ed.path[..ed.path.len() - 1];
            let (level, _) = resolve_level(&state.scene.materials, top, parent_path);
            let (id, maps) = match level {
                Level::Material(id) => (id, Vec::new()),
                Level::Map { owner, maps } => (owner, maps),
            };
            let sibs = children(&state.scene.materials, id, &maps);
            if sibs.len() < 2 {
                return Err("no sibling at this level".into());
            }
            let i = sibs.iter().position(|s| *s == last).unwrap_or(0);
            let next = sibs[(i + 1) % sibs.len()].clone();
            let ed = &mut state.scene.material_editor;
            *ed.path.last_mut().expect("checked") = next;
            Ok(json!({ "path": ed.path.len() }))
        }
        C::MaterialEditorNavigate { path } => {
            let steps: Vec<NavStep> = path.iter().map(|s| s.parse::<u32>().map(NavStep::Sub).unwrap_or_else(|_| NavStep::Map(s.clone()))).collect();
            let ed = &state.scene.material_editor;
            if steps.is_empty() {
                state.scene.material_editor.path.clear();
                return Ok(json!({ "path": 0 }));
            }
            let top = ed.slots[ed.active].ok_or("no material in this slot")?;
            let (_, n) = resolve_level(&state.scene.materials, top, &steps);
            if n != steps.len() {
                return Err(format!("no level {path:?} in this material"));
            }
            state.scene.material_editor.path = steps;
            Ok(json!({ "path": n }))
        }
        C::SetMapParameter { name, value } => edit_map(state, "Map Parameter", |m| m.set_parameter(&name, value)),
        C::SetMapColor { which, rgb } => edit_map(state, "Map Color", |m| m.set_color(&which, rgb)),
        C::SwapMapColors => edit_map(state, "Swap", |m| {
            m.swap();
            Ok(())
        }),
        C::SetMapBitmap { path } => edit_map(state, "Bitmap", |m| {
            if m.kind != MapType::Bitmap {
                return Err("the map at this level is not a Bitmap".into());
            }
            m.path = path;
            Ok(())
        }),
        C::ReloadBitmaps => {
            state.bitmap_generation += 1;
            state.mesh_dirty = true;
            Ok(json!({ "generation": state.bitmap_generation }))
        }
        C::CopyMap { from, to, swap } => edit_level(state, if swap { "Swap Maps" } else { "Copy Map" }, |lib, _, id, maps| {
            let m = lib.get_mut(&id).ok_or("missing material")?;
            if maps.is_empty() {
                let a = m.map_slot_mut(&from)?.clone();
                let b = m.map_slot_mut(&to)?.clone();
                if a.is_none() {
                    return Err(format!("no map in {from}"));
                }
                *m.map_slot_mut(&to)? = a;
                if swap {
                    *m.map_slot_mut(&from)? = b;
                }
            } else {
                let a = map_slot_at_mut(m, maps, &from)?.clone();
                let b = map_slot_at_mut(m, maps, &to)?.clone();
                if a.is_none() {
                    return Err(format!("no map in {from}"));
                }
                *map_slot_at_mut(m, maps, &to)? = a;
                if swap {
                    *map_slot_at_mut(m, maps, &from)? = b;
                }
            }
            Ok(json!({}))
        }),
        C::CopyMaterialToSlot { from, to } => {
            let (f, t) = (slot_index(state, Some(from))?, slot_index(state, Some(to))?);
            if f == t {
                return Ok(json!({}));
            }
            let before = snapshot(state);
            let mut after = before.clone();
            let mut m = state.scene.slot_material(f);
            // A dragged copy is a new material; a compound one copies its sub-materials too.
            fn deep_copy(lib: &mut MaterialLibrary, m: &mut StandardMaterial) {
                let copy_child = |lib: &mut MaterialLibrary, c: &mut Option<MaterialId>| {
                    if let Some(src) = c.and_then(|c| lib.get(&c).cloned()) {
                        let mut sub = src;
                        deep_copy(lib, &mut sub);
                        let id = next_material_id(lib);
                        lib.insert(id, sub);
                        *c = Some(id);
                    }
                };
                match &mut m.kind {
                    MaterialKind::Standard => {}
                    MaterialKind::MultiSub(list) => {
                        for s in list.iter_mut() {
                            copy_child(lib, &mut s.material);
                        }
                    }
                    MaterialKind::Blend(b) => {
                        for c in b.materials.iter_mut() {
                            copy_child(lib, c);
                        }
                    }
                    MaterialKind::Shell(sh) => {
                        for c in sh.materials.iter_mut() {
                            copy_child(lib, c);
                        }
                    }
                }
            }
            deep_copy(&mut after.0, &mut m);
            let id = next_material_id(&after.0);
            after.0.insert(id, m);
            after.1.slots[t] = Some(id);
            commit(state, "Copy Material", Change::SetMaterials { before, after })?;
            Ok(json!({ "slot": t, "material": id.0 }))
        }
        C::AssignMaterialToSelection | C::AssignMaterialToNode { .. } => {
            let (ids, slot) = match cmd {
                C::AssignMaterialToNode { node, slot } => {
                    state.scene.node(NodeId(node)).ok_or_else(|| format!("no node {node}"))?;
                    (vec![NodeId(node)], slot_index(state, slot)?)
                }
                _ => (state.selection.iter().copied().collect::<Vec<_>>(), state.scene.material_editor.active),
            };
            if ids.is_empty() {
                return Err("select the objects to assign the material to".into());
            }
            let before = snapshot(state);
            let mut after = before.clone();
            let id = top_id(&mut after.0, &mut after.1, slot);
            let mut changes = vec![Change::SetMaterials { before, after }];
            for n in &ids {
                let old = state.scene.node(*n).ok_or("missing node")?.material;
                changes.push(Change::SetMaterial { id: *n, before: old, after: Some(id) });
            }
            commit(state, "Assign Material", Change::Group(changes))?;
            state.mesh_dirty = true;
            Ok(json!({ "material": id.0, "nodes": ids.iter().map(|n| n.0).collect::<Vec<_>>() }))
        }
        C::AssignMaterialToFaces { slot } => {
            let slot = slot_index(state, slot)?;
            let faces = selected_faces(state);
            let Some((node, faces)) = faces.into_iter().find(|(_, f)| !f.is_empty()) else { return Err("select the polygons to drop the material on".into()) };
            let before = snapshot(state);
            let mut after = before.clone();
            let dropped = top_id(&mut after.0, &mut after.1, slot);
            let (multi, face_id, keep_ids) = drop_on_faces(state, &mut after.0, node, &faces, dropped)?;
            let old_material = state.scene.node(node).ok_or("missing node")?.material;
            state.undo.begin("Assign Material");
            let r = (|| {
                commit(state, "Assign Material", Change::Group(vec![Change::SetMaterials { before, after }, Change::SetMaterial { id: node, before: old_material, after: Some(multi) }]))?;
                let selected: std::collections::BTreeSet<u32> = faces.iter().map(|f| f.0).collect();
                edit_meshes(state, "Assign Material", vec![(node, vec![()])], |mesh, _, _| {
                    for (i, f) in mesh.faces_mut().iter_mut().enumerate() {
                        if selected.contains(&(i as u32)) {
                            f.material_id = face_id - 1;
                        } else if !keep_ids {
                            // An old material became ID 1: the other faces show it.
                            f.material_id = 0;
                        }
                    }
                    Ok(())
                })
            })();
            state.undo.end();
            r?;
            Ok(json!({ "material": multi.0, "id": face_id }))
        }
        C::DropBitmap { path, node } => {
            let node = match node {
                Some(n) => NodeId(n),
                None => *state.selection.iter().next().ok_or("drop the bitmap on an object")?,
            };
            state.scene.node(node).ok_or_else(|| format!("no node {}", node.0))?;
            let ed = &state.scene.material_editor;
            let slot = (0..SLOT_COUNT).find(|i| ed.slots[*i].is_none()).unwrap_or(ed.active);
            let name = std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| path.clone());
            let on_faces = selected_faces(state).iter().any(|(n, f)| *n == node && !f.is_empty());
            state.undo.begin("Drop Bitmap");
            let r = (|| {
                let before = snapshot(state);
                let mut after = before.clone();
                let mut m = StandardMaterial::new(name);
                m.diffuse_map = Some(Map::bitmap(path, default_amount("Diffuse Color")));
                m.show_map_in_viewport = true;
                let id = next_material_id(&after.0);
                after.0.insert(id, m);
                after.1.slots[slot] = Some(id);
                commit(state, "Drop Bitmap", Change::SetMaterials { before, after })?;
                if on_faces {
                    execute(state, C::AssignMaterialToFaces { slot: Some(slot as u32) })
                } else {
                    execute(state, C::AssignMaterialToNode { node: node.0, slot: Some(slot as u32) })
                }
            })();
            state.undo.end();
            let v = r?;
            Ok(json!({ "slot": slot, "assigned": v }))
        }
        C::GetMaterial { material } => {
            let id = MaterialId(material);
            if !state.scene.materials.contains_key(&id) {
                return Err(format!("no material {material}"));
            }
            let before = snapshot(state);
            let mut after = before.clone();
            let i = after.1.active;
            after.1.slots[i] = Some(id);
            after.1.path.clear();
            commit(state, "Get Material", Change::SetMaterials { before, after })?;
            Ok(json!({ "slot": i, "material": material }))
        }
        C::GetMaterialFromObject { node } => {
            let id = match node {
                Some(n) => NodeId(n),
                None => *state.selection.iter().next().ok_or("select an object")?,
            };
            let n = state.scene.node(id).ok_or_else(|| format!("no node {}", id.0))?;
            let mut mat = n.material.ok_or("the object has no material")?;
            // Over selected polygons that all use one sub-material: that sub-material.
            if let Some(MaterialKind::MultiSub(list)) = state.scene.materials.get(&mat).map(|m| &m.kind) {
                let faces: Vec<loom_mesh::FaceId> = selected_faces(state).into_iter().filter(|(nn, _)| *nn == id).flat_map(|(_, f)| f).collect();
                let mesh = n.cage_mesh();
                let ids: std::collections::BTreeSet<u16> = faces.iter().filter_map(|f| mesh.faces().get(f.0 as usize)).map(|f| f.material_id + 1).collect();
                if ids.len() == 1 {
                    let fid = *ids.iter().next().expect("one");
                    if let Some(sub) = list.iter().find(|s| s.id == fid).and_then(|s| s.material) {
                        mat = sub;
                    }
                }
            }
            let i = state.scene.material_editor.active;
            if state.scene.material_editor.slots[i] == Some(mat) {
                // Already in the active slot: the eyedropper has no effect.
                return Ok(json!({ "slot": i, "material": mat.0 }));
            }
            let before = snapshot(state);
            let mut after = before.clone();
            after.1.slots[i] = Some(mat);
            after.1.path.clear();
            commit(state, "Get Material", Change::SetMaterials { before, after })?;
            Ok(json!({ "slot": i, "material": mat.0 }))
        }
        C::PickMaterialFromObject => {
            state.pick_mode = Some(crate::state::PickMode::PickMaterial);
            Ok(json!({}))
        }
        C::SetMaterialEditorOption { name, value } => {
            state.scene.material_editor.options.set(&name, value)?;
            Ok(json!({ "option": name, "value": value }))
        }
        C::SetSampleType { kind } => {
            let t = match kind.as_str() {
                "Sphere" => SampleType::Sphere,
                "Cylinder" => SampleType::Cylinder,
                "Cube" => SampleType::Cube,
                other => return Err(format!("no sample type \"{other}\" (Sphere, Cylinder, Cube)")),
            };
            let ed = &mut state.scene.material_editor;
            ed.sample_types.insert(ed.active as u32, t);
            Ok(json!({ "slot": ed.active, "type": kind }))
        }
        _ => unreachable!("routed by handles()"),
    }
}
