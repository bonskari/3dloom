//! Animation > Animation Layers: the Layer controller commands on the
//! selected objects (each one undo step).

use crate::execute::commit;
use crate::state::AppState;
use crate::Command as C;
use loom_scene::anim_layers::{AnimLayer, AnimLayers};
use loom_scene::{Change, NodeId};
use serde_json::{json, Value};

pub fn handles(c: &C) -> bool {
    matches!(
        c,
        C::AnimationLayers
            | C::EnableAnimLayers { .. }
            | C::AddAnimLayer { .. }
            | C::DeleteAnimLayer
            | C::SetActiveAnimLayer { .. }
            | C::SetAnimLayerProperty { .. }
            | C::CopyAnimLayer
            | C::PasteActiveAnimLayer
            | C::PasteNewAnimLayer { .. }
            | C::CollapseAnimLayer
            | C::DisableAnimLayers
    )
}

fn selected(state: &AppState) -> Result<Vec<NodeId>, String> {
    let ids: Vec<NodeId> = state.selection.iter().copied().collect();
    if ids.is_empty() {
        return Err("select objects first".into());
    }
    Ok(ids)
}

/// Edits the Layer controller of every selected layer-enabled object.
fn edit(state: &mut AppState, label: &str, f: impl Fn(&mut AnimLayers) -> Result<(), String>) -> Result<Value, String> {
    let mut changes = Vec::new();
    for id in selected(state)? {
        let Some(n) = state.scene.node(id) else { continue };
        let Some(al) = &n.anim_layers else { continue };
        let before = n.props();
        let mut after = before.clone();
        let mut l = al.clone();
        f(&mut l)?;
        after.anim_layers = Some(l);
        if after != before {
            changes.push(Change::SetNodeProps { id, before: Box::new(before), after: Box::new(after) });
        }
    }
    if changes.is_empty() {
        return Err("select objects with animation layers (Enable Anim Layers first)".into());
    }
    commit(state, label, Change::Group(changes))?;
    Ok(status(state))
}

/// The first selected object's layers, for the toolbar.
pub fn status(state: &AppState) -> Value {
    let al = state.selection.iter().find_map(|id| state.scene.node(*id).and_then(|n| n.anim_layers.clone()));
    match al {
        Some(al) => json!({ "layers": al.names(), "active": al.active }),
        None => json!({ "layers": [] }),
    }
}

pub fn execute(state: &mut AppState, cmd: C) -> Result<Value, String> {
    match cmd {
        C::AnimationLayers => {
            state.dialog = Some(crate::viewport::Dialog::AnimationLayers);
            Ok(status(state))
        }
        C::EnableAnimLayers { position, rotation, scale } => {
            if !(position || rotation || scale) {
                return Err("choose at least one track".into());
            }
            let mut changes = Vec::new();
            for id in selected(state)? {
                let Some(n) = state.scene.node(id) else { continue };
                let before = n.props();
                let mut after = before.clone();
                match after.anim_layers.as_mut() {
                    Some(al) => {
                        al.position |= position;
                        al.rotation |= rotation;
                        al.scale |= scale;
                    }
                    None => after.anim_layers = Some(AnimLayers::new(position, rotation, scale)),
                }
                if after != before {
                    changes.push(Change::SetNodeProps { id, before: Box::new(before), after: Box::new(after) });
                }
            }
            if !changes.is_empty() {
                commit(state, "Enable Anim Layers", Change::Group(changes))?;
            }
            Ok(status(state))
        }
        C::AddAnimLayer { name } => edit(state, "Add Anim Layer", |al| {
            let n = name.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| format!("AnimLayer{:02}", al.layers.len() + 1));
            al.layers.push(AnimLayer::new(&n));
            al.active = al.layers.len();
            Ok(())
        }),
        C::DeleteAnimLayer => edit(state, "Delete Anim Layer", |al| {
            let a = al.active.checked_sub(1).ok_or("the Base Layer cannot be deleted (Disable Anim Layers removes the controller)")?;
            al.layers.remove(a);
            al.active = a;
            Ok(())
        }),
        C::SetActiveAnimLayer { index } => edit(state, "Active Anim Layer", |al| {
            if index as usize > al.layers.len() {
                return Err(format!("no layer {index}"));
            }
            al.active = index as usize;
            Ok(())
        }),
        C::SetAnimLayerProperty { index, name, value } => edit(state, "Anim Layer", |al| {
            let i = index as usize;
            let on = value != 0.0;
            if i == 0 {
                return match name.to_ascii_lowercase().as_str() {
                    "locked" | "lock" => {
                        al.base_locked = on;
                        Ok(())
                    }
                    _ => Err(format!("the Base Layer has no {name:?} (Locked only)")),
                };
            }
            let l = al.layers.get_mut(i - 1).ok_or_else(|| format!("no layer {index}"))?;
            match name.to_ascii_lowercase().as_str() {
                "on" => l.on = on,
                "locked" | "lock" => l.locked = on,
                "output" => l.output = on,
                "weight" => l.weight.set(value as f32),
                "name" => return Err("rename with Anim Layer Name".into()),
                _ => return Err(format!("no layer property {name:?} (On, Locked, Output, Weight)")),
            }
            Ok(())
        }),
        C::CopyAnimLayer => {
            let id = *state.selection.iter().next().ok_or("select an object first")?;
            let al = state.scene.node(id).and_then(|n| n.anim_layers.clone()).ok_or("the object has no animation layers")?;
            let a = al.active.checked_sub(1).ok_or("copy a layer above the Base Layer")?;
            state.anim_layer_clipboard = Some(al.layers[a].clone());
            Ok(json!({ "copied": al.layers[a].name }))
        }
        C::PasteActiveAnimLayer => {
            let clip = state.anim_layer_clipboard.clone().ok_or("copy a layer first")?;
            edit(state, "Paste Active Anim Layer", move |al| {
                let a = al.active.checked_sub(1).ok_or("the Base Layer takes no pasted layer")?;
                let name = al.layers[a].name.clone();
                al.layers[a] = AnimLayer { name, ..clip.clone() };
                Ok(())
            })
        }
        C::PasteNewAnimLayer { name } => {
            let clip = state.anim_layer_clipboard.clone().ok_or("copy a layer first")?;
            edit(state, "Paste New Anim Layer", move |al| {
                let n = name.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| format!("{} copy", clip.name));
                al.layers.push(AnimLayer { name: n, ..clip.clone() });
                al.active = al.layers.len();
                Ok(())
            })
        }
        C::CollapseAnimLayer => collapse(state),
        C::DisableAnimLayers => {
            let mut changes = Vec::new();
            for id in selected(state)? {
                let Some(n) = state.scene.node(id) else { continue };
                let Some(al) = &n.anim_layers else { continue };
                if !al.layers.is_empty() {
                    return Err(format!("{}: delete or collapse its layers first", n.name));
                }
                let before = n.props();
                let mut after = before.clone();
                after.anim_layers = None;
                changes.push(Change::SetNodeProps { id, before: Box::new(before), after: Box::new(after) });
            }
            if changes.is_empty() {
                return Err("select objects with animation layers".into());
            }
            commit(state, "Disable Anim Layers", Change::Group(changes))?;
            Ok(status(state))
        }
        _ => Err("not an Animation Layers command".into()),
    }
}

/// Collapse: the active layer merges into the one below it (into the Base
/// Layer's own tracks from layer 1). Unanimated offsets add once; animated
/// ones are keyed on every frame of the active time segment.
fn collapse(state: &mut AppState) -> Result<Value, String> {
    let (start, end) = state.frame_range;
    let mut changes = Vec::new();
    for id in selected(state)? {
        let Some(n) = state.scene.node(id) else { continue };
        let Some(al) = n.anim_layers.clone() else { continue };
        let Some(a) = al.active.checked_sub(1) else { return Err("the Base Layer has nothing below it to collapse into".into()) };
        let top = al.layers[a].clone();
        let animated = top.position.is_animated() || top.rotation.is_animated() || top.scale.is_animated() || top.weight.is_animated();
        let frames: Vec<i32> = if animated { (start..=end).collect() } else { vec![state.current_frame] };
        // Sample the merged values first, then write them.
        let sample = |f: i32| {
            let t = f as f32;
            let w = if top.on { top.weight.get_at(t) / 100.0 } else { 0.0 };
            (top.position.get_at(t) * w, top.rotation.get_at(t) * w, top.scale.get_at(t) * w)
        };
        let offsets: Vec<(i32, (glam::Vec3, glam::Vec3, glam::Vec3))> = frames.iter().map(|f| (*f, sample(*f))).collect();
        let before_props = n.props();
        let mut props = before_props.clone();
        let mut transform = n.transform.clone();
        {
            let al2 = props.anim_layers.as_mut().expect("has layers");
            for (f, (dp, dr, ds)) in &offsets {
                let write = |p: &mut loom_scene::Param<glam::Vec3>, d: glam::Vec3, on: bool| {
                    if !on || (d == glam::Vec3::ZERO && !animated) {
                        return;
                    }
                    let t = *f as f32;
                    let cur = p.get_at(t);
                    loom_scene::anim::with(t, animated, || {
                        p.set(cur + d);
                        if animated {
                            p.key_now();
                        }
                    });
                };
                if a == 0 {
                    write(&mut transform.position, *dp, al.position);
                    write(&mut transform.rotation, *dr, al.rotation);
                    write(&mut transform.scale, *ds, al.scale);
                } else {
                    let below = &mut al2.layers[a - 1];
                    let w = (below.weight.get() / 100.0).max(1e-4);
                    write(&mut below.position, *dp / w, al.position);
                    write(&mut below.rotation, *dr / w, al.rotation);
                    write(&mut below.scale, *ds / w, al.scale);
                }
            }
            al2.layers.remove(a);
            al2.active = a;
        }
        if transform != n.transform {
            changes.push(Change::SetTransform { id, before: n.transform.clone(), after: transform });
        }
        changes.push(Change::SetNodeProps { id, before: Box::new(before_props), after: Box::new(props) });
    }
    if changes.is_empty() {
        return Err("select objects with animation layers".into());
    }
    commit(state, "Collapse Anim Layer", Change::Group(changes))?;
    Ok(status(state))
}
