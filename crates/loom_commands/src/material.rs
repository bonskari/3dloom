//! Material Editor commands: sample slots, Standard material values, maps,
//! assignment. Every edit is one undo step (`Change::SetMaterials`).

use crate::command::Command as C;
use crate::execute::commit;
use crate::{AppState, Dialog};
use loom_scene::material::{next_material_id, BitmapMap, MaterialEditorSlots, MaterialLibrary, StandardMaterial, MAP_CHANNELS, SLOT_COUNT};
use loom_scene::{Change, NodeId};
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
fn ensure_in_library(lib: &mut MaterialLibrary, slots: &mut MaterialEditorSlots, slot: usize, material: StandardMaterial) -> loom_scene::MaterialId {
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

/// Edits a slot's material as one undo step.
fn edit(state: &mut AppState, slot: Option<u32>, label: &str, f: impl FnOnce(&mut StandardMaterial) -> Result<(), String>) -> Result<Value, String> {
    let i = slot_index(state, slot)?;
    let mut m = state.scene.slot_material(i);
    f(&mut m)?;
    let before = snapshot(state);
    let mut after = before.clone();
    let id = ensure_in_library(&mut after.0, &mut after.1, i, m);
    commit(state, label, Change::SetMaterials { before, after })?;
    Ok(json!({ "slot": i, "material": id.0 }))
}

pub fn execute(state: &mut AppState, cmd: C) -> Result<Value, String> {
    match cmd {
        C::MaterialEditorToggle => {
            state.dialog = if state.dialog == Some(Dialog::MaterialEditor) { None } else { Some(Dialog::MaterialEditor) };
            Ok(json!({ "open": state.dialog == Some(Dialog::MaterialEditor) }))
        }
        C::MaterialEditorSlot { slot } => {
            let i = slot_index(state, Some(slot))?;
            state.scene.material_editor.active = i;
            Ok(json!({ "slot": i }))
        }
        C::SetMaterialColor { slot, which, rgb } => edit(state, slot, "Material Color", |m| m.set_color(&which, rgb)),
        C::SetMaterialParameter { slot, name, value } => edit(state, slot, "Material Parameter", |m| m.set_parameter(&name, value)),
        C::SetMaterialMap { slot, map, path, amount } => edit(state, slot, "Material Map", |m| {
            let default = MAP_CHANNELS.iter().find(|(n, _)| *n == map).map(|(_, a)| *a).unwrap_or(100.0);
            let range = if map == "Bump" { -999.0..=999.0 } else { 0.0..=100.0 };
            let channel = m.map_slot_mut(&map)?;
            *channel = path.filter(|p| !p.trim().is_empty()).map(|path| BitmapMap { path, amount: amount.unwrap_or(default).clamp(*range.start(), *range.end()), enabled: true });
            Ok(())
        }),
        C::RenameMaterial { slot, name } => edit(state, slot, "Rename Material", |m| {
            m.name = name;
            Ok(())
        }),
        C::ResetMaterial { slot } => {
            let i = slot_index(state, slot)?;
            edit(state, Some(i as u32), "Reset Material", |m| {
                let name = m.name.clone();
                *m = StandardMaterial::slot_default(i);
                m.name = name;
                Ok(())
            })
        }
        C::AssignMaterialToSelection => {
            let ids: Vec<NodeId> = state.selection.iter().copied().collect();
            if ids.is_empty() {
                return Err("select the objects to assign the material to".into());
            }
            let i = state.scene.material_editor.active;
            let before = snapshot(state);
            let mut after = before.clone();
            let m = state.scene.slot_material(i);
            let id = ensure_in_library(&mut after.0, &mut after.1, i, m);
            let mut changes = vec![Change::SetMaterials { before, after }];
            for n in &ids {
                let old = state.scene.node(*n).ok_or("missing node")?.material;
                changes.push(Change::SetMaterial { id: *n, before: old, after: Some(id) });
            }
            commit(state, "Assign Material", Change::Group(changes))?;
            state.mesh_dirty = true;
            Ok(json!({ "material": id.0, "nodes": ids.iter().map(|n| n.0).collect::<Vec<_>>() }))
        }
        C::GetMaterialFromObject { node } => {
            let id = match node {
                Some(n) => NodeId(n),
                None => *state.selection.iter().next().ok_or("select an object")?,
            };
            let mat = state.scene.node(id).ok_or_else(|| format!("no node {}", id.0))?.material.ok_or("the object has no material")?;
            let i = state.scene.material_editor.active;
            let before = snapshot(state);
            let mut after = before.clone();
            after.1.slots[i] = Some(mat);
            commit(state, "Get Material", Change::SetMaterials { before, after })?;
            Ok(json!({ "slot": i, "material": mat.0 }))
        }
        _ => unreachable!("routed by handles()"),
    }
}
