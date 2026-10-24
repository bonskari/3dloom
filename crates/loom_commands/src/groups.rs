//! Group menu (Group, Ungroup, Open, Close, Attach, Detach, Explode) and the
//! Layer Manager commands.

use crate::execute::{commit_change, commit_static, selection_json};
use crate::state::{AppState, PickMode};
use crate::viewport::Dialog;
use crate::Command as C;
use loom_scene::{Change, Layer, Node, NodeId, ObjectKind, Transform};
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub fn execute(state: &mut AppState, cmd: C) -> Result<Value, String> {
    match cmd {
        C::Group { name } => group(state, name),
        C::Ungroup => ungroup(state, false),
        C::GroupExplode => ungroup(state, true),
        C::GroupOpen => set_open(state, true),
        C::GroupClose => set_open(state, false),
        C::GroupAttach => {
            state.pick_mode = if state.pick_mode == Some(PickMode::GroupAttach) { None } else { Some(PickMode::GroupAttach) };
            Ok(json!({ "pick": state.pick_mode.map(|p| format!("{p:?}")) }))
        }
        C::GroupDetach => detach_from_group(state),
        C::LayerManager => {
            state.dialog = if state.dialog == Some(Dialog::LayerManager) { None } else { Some(Dialog::LayerManager) };
            Ok(json!({ "open": state.dialog.is_some() }))
        }
        C::CreateLayer { name } => {
            let id = state.scene.next_layer_id();
            let name = name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| {
                let mut n = 1;
                loop {
                    let candidate = format!("Layer{n:03}");
                    if !state.scene.layers.iter().any(|l| l.name == candidate) {
                        break candidate;
                    }
                    n += 1;
                }
            });
            let mut layers = state.scene.layers.clone();
            layers.push(Layer::new(id, name));
            let mut changes = vec![layers_change(state, layers, id)];
            for n in state.selection.clone() {
                if let Some(node) = state.scene.node(n) {
                    changes.push(Change::SetLayer { id: n, before: node.layer, after: id });
                }
            }
            commit_change(state, "Create Layer", Change::Group(changes))?;
            Ok(json!({ "layer": id }))
        }
        C::DeleteLayer { layer } => {
            if layer == 0 {
                return Err("layer 0 (default) cannot be deleted".into());
            }
            state.scene.layer(layer).ok_or_else(|| format!("no layer {layer}"))?;
            if state.scene.nodes().any(|n| n.layer == layer) {
                return Err("the layer still has objects; move them first".into());
            }
            let layers: Vec<Layer> = state.scene.layers.iter().filter(|l| l.id != layer).cloned().collect();
            let current = if state.scene.current_layer == layer { 0 } else { state.scene.current_layer };
            commit_change(state, "Delete Layer", layers_change(state, layers, current))?;
            Ok(json!({}))
        }
        C::RenameLayer { layer, name } => {
            let mut layers = state.scene.layers.clone();
            let l = layers.iter_mut().find(|l| l.id == layer).ok_or_else(|| format!("no layer {layer}"))?;
            l.name = name;
            let cur = state.scene.current_layer;
            commit_change(state, "Rename Layer", layers_change(state, layers, cur))?;
            Ok(json!({}))
        }
        C::SetCurrentLayer { layer } => {
            state.scene.layer(layer).ok_or_else(|| format!("no layer {layer}"))?;
            let layers = state.scene.layers.clone();
            commit_change(state, "Current Layer", layers_change(state, layers, layer))?;
            Ok(json!({ "current": layer }))
        }
        C::SetLayerHidden { layer, on } => layer_flag(state, layer, |l| l.hidden = on, "Hide Layer"),
        C::SetLayerFrozen { layer, on } => layer_flag(state, layer, |l| l.frozen = on, "Freeze Layer"),
        C::MoveToLayer { layer } => move_to_layer(state, layer),
        C::AddSelectionToCurrentLayer => {
            let cur = state.scene.current_layer;
            move_to_layer(state, cur)
        }
        C::SelectLayerObjects { layer } => {
            state.scene.layer(layer).ok_or_else(|| format!("no layer {layer}"))?;
            let ids: Vec<u64> = state.scene.nodes().filter(|n| n.layer == layer).map(|n| n.id.0).collect();
            crate::execute(state, C::SelectNodes { ids, op: crate::SelectOp::Replace })
        }
        C::SetCurrentLayerToSelection => {
            let layers: BTreeSet<u32> = state.selection.iter().filter_map(|id| state.scene.node(*id)).map(|n| n.layer).collect();
            match layers.len() {
                0 => Err("select objects first".into()),
                1 => crate::execute(state, C::SetCurrentLayer { layer: *layers.iter().next().expect("one layer") }),
                _ => Err("the selected objects are on different layers".into()),
            }
        }
        C::SetLayerColor { layer, rgb } => layer_flag(state, layer, |l| l.color = rgb, "Layer Color"),
        C::SetLayerRenderable { layer, on } => layer_flag(state, layer, |l| l.renderable = on, "Layer Render"),
        C::SetAllLayersHidden { on } => all_layers(state, |l| l.hidden = on, if on { "Hide All Layers" } else { "Unhide All Layers" }),
        C::SetAllLayersFrozen { on } => all_layers(state, |l| l.frozen = on, if on { "Freeze All Layers" } else { "Unfreeze All Layers" }),
        C::SetColorByLayer { on } => {
            let mut changes = Vec::new();
            for id in &state.selection {
                let Some(n) = state.scene.node(*id) else { continue };
                let before = n.props();
                let mut after = before.clone();
                after.display.color_by_layer = on;
                if after != before {
                    changes.push(Change::SetNodeProps { id: *id, before: Box::new(before), after: Box::new(after) });
                }
            }
            if !changes.is_empty() {
                commit_change(state, if on { "Color By Layer" } else { "Color By Object" }, Change::Group(changes))?;
            }
            Ok(json!({}))
        }
        other => Err(format!("not a group / layer command: {other:?}")),
    }
}

fn layers_change(state: &AppState, layers: Vec<Layer>, current: u32) -> Change {
    Change::SetLayers { before: (state.scene.layers.clone(), state.scene.current_layer), after: (layers, current) }
}

fn layer_flag(state: &mut AppState, layer: u32, f: impl Fn(&mut Layer), label: &str) -> Result<Value, String> {
    let mut layers = state.scene.layers.clone();
    let l = layers.iter_mut().find(|l| l.id == layer).ok_or_else(|| format!("no layer {layer}"))?;
    f(l);
    let cur = state.scene.current_layer;
    commit_change(state, label, layers_change(state, layers, cur))?;
    let keep: BTreeSet<NodeId> = state.selection.iter().copied().filter(|id| state.scene.node(*id).is_some_and(|n| state.is_selectable(n))).collect();
    state.selection = keep;
    Ok(json!({}))
}

fn all_layers(state: &mut AppState, f: impl Fn(&mut Layer), label: &str) -> Result<Value, String> {
    let mut layers = state.scene.layers.clone();
    layers.iter_mut().for_each(f);
    if layers == state.scene.layers {
        return Ok(json!({}));
    }
    let cur = state.scene.current_layer;
    commit_change(state, label, layers_change(state, layers, cur))?;
    let keep: BTreeSet<NodeId> = state.selection.iter().copied().filter(|id| state.scene.node(*id).is_some_and(|n| state.is_selectable(n))).collect();
    state.selection = keep;
    Ok(json!({}))
}

fn move_to_layer(state: &mut AppState, layer: u32) -> Result<Value, String> {
    state.scene.layer(layer).ok_or_else(|| format!("no layer {layer}"))?;
    let changes: Vec<Change> = state
        .selection
        .iter()
        .filter_map(|id| state.scene.node(*id).map(|n| Change::SetLayer { id: *id, before: n.layer, after: layer }))
        .filter(|c| !matches!(c, Change::SetLayer { before, after, .. } if before == after))
        .collect();
    if changes.is_empty() {
        return Ok(json!({ "moved": 0 }));
    }
    let n = changes.len();
    commit_change(state, "Move To Layer", Change::Group(changes))?;
    Ok(json!({ "moved": n }))
}

/// The selected nodes without a selected ancestor (the roots of the selection).
fn selection_roots(state: &AppState) -> Vec<NodeId> {
    let sel = &state.selection;
    sel.iter()
        .copied()
        .filter(|id| {
            let mut cur = state.scene.node(*id).and_then(|n| n.parent);
            while let Some(p) = cur {
                if sel.contains(&p) {
                    return false;
                }
                cur = state.scene.node(p).and_then(|n| n.parent);
            }
            true
        })
        .collect()
}

/// Reparents `id` under `parent` keeping its world transform, as changes.
fn reparent_changes(state: &AppState, id: NodeId, parent: Option<NodeId>, parent_world: glam::Mat4) -> Vec<Change> {
    reparent_in(&state.scene, id, parent, parent_world)
}

fn reparent_in(scene: &loom_scene::Scene, id: NodeId, parent: Option<NodeId>, parent_world: glam::Mat4) -> Vec<Change> {
    let Some(node) = scene.node(id) else { return Vec::new() };
    let world = scene.world_matrix(id);
    let frame = loom_scene::Scene::frame_from(parent_world, node.inherit);
    let local = Transform::from_matrix(frame.inverse() * world);
    vec![
        Change::SetTransform { id, before: node.transform.clone(), after: local },
        Change::SetParent { id, before: node.parent, after: parent },
    ]
}

fn group(state: &mut AppState, name: Option<String>) -> Result<Value, String> {
    let members = selection_roots(state);
    if members.is_empty() {
        return Err("select objects to group".into());
    }
    let (lo, hi) = state.bounds_of(members.iter().copied().chain(members.iter().flat_map(|m| state.scene.descendants(*m)))).unwrap_or((glam::Vec3::ZERO, glam::Vec3::ZERO));
    let centre = (lo + hi) * 0.5;
    // The head joins the members' common parent (if they share one).
    let parents: BTreeSet<Option<NodeId>> = members.iter().filter_map(|m| state.scene.node(*m).map(|n| n.parent)).collect();
    let parent = if parents.len() == 1 { parents.into_iter().next().flatten() } else { None };
    let name = name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| state.scene.next_name("Group"));
    let mut head = Node::new(&name, ObjectKind::Group { open: false });
    head.id = NodeId(state.scene.next_id());
    head.parent = parent;
    head.layer = state.scene.current_layer;
    let parent_world = parent.map(|p| state.scene.world_matrix(p)).unwrap_or(glam::Mat4::IDENTITY);
    head.transform = Transform::from_matrix(parent_world.inverse() * glam::Mat4::from_translation(centre));
    let head_id = head.id;
    let head_world = glam::Mat4::from_translation(centre);
    let mut changes = vec![Change::AddNode(head)];
    for m in &members {
        changes.extend(reparent_changes(state, *m, Some(head_id), head_world));
    }
    commit_static(state, "Group", Change::Group(changes))?;
    state.selection = BTreeSet::from([head_id]);
    state.sub_selection.clear();
    Ok(json!({ "group": head_id.0 }))
}

/// Group heads the selection refers to: selected heads, plus the (open)
/// groups of selected members.
fn selected_groups(state: &AppState) -> Vec<NodeId> {
    let mut out: BTreeSet<NodeId> = BTreeSet::new();
    for id in &state.selection {
        let Some(n) = state.scene.node(*id) else { continue };
        if matches!(n.object, ObjectKind::Group { .. }) {
            out.insert(*id);
        } else if let Some(p) = n.parent {
            if state.scene.node(p).is_some_and(|pn| matches!(pn.object, ObjectKind::Group { .. })) {
                out.insert(p);
            }
        }
    }
    out.into_iter().collect()
}

fn ungroup(state: &mut AppState, explode: bool) -> Result<Value, String> {
    let mut heads: Vec<NodeId> = state.selection.iter().copied().filter(|id| state.scene.node(*id).is_some_and(|n| matches!(n.object, ObjectKind::Group { .. }))).collect();
    if heads.is_empty() {
        return Err("select a group".into());
    }
    if explode {
        let nested: Vec<NodeId> = heads.iter().flat_map(|h| state.scene.descendants(*h)).filter(|d| state.scene.node(*d).is_some_and(|n| matches!(n.object, ObjectKind::Group { .. }))).collect();
        heads.extend(nested);
    }
    // Outer groups first so inner heads still exist when their turn comes.
    heads.sort_by_key(|h| state.scene.depth(*h));
    let mut changes = Vec::new();
    let mut members_out: BTreeSet<NodeId> = BTreeSet::new();
    let mut sim = state.scene.clone();
    for h in heads {
        let Some(head) = sim.node(h) else { continue };
        let gp = head.parent;
        let gp_world = gp.map(|p| sim.world_matrix(p)).unwrap_or(glam::Mat4::IDENTITY);
        for c in sim.children(h) {
            let cs = reparent_in(&sim, c, gp, gp_world);
            for ch in &cs {
                let _ = ch.apply(&mut sim);
            }
            changes.extend(cs);
            members_out.insert(c);
        }
        let _ = Change::RemoveNode(h).apply(&mut sim);
        changes.push(Change::RemoveNode(h));
    }
    commit_static(state, if explode { "Explode" } else { "Ungroup" }, Change::Group(changes))?;
    state.selection = members_out.into_iter().filter(|m| state.scene.node(*m).is_some()).collect();
    Ok(selection_json(state))
}

fn set_open(state: &mut AppState, open: bool) -> Result<Value, String> {
    let heads = selected_groups(state);
    if heads.is_empty() {
        return Err("select a group".into());
    }
    let mut changes = Vec::new();
    for h in &heads {
        let Some(n) = state.scene.node(*h) else { continue };
        let ObjectKind::Group { open: was } = n.object else { continue };
        if was != open {
            changes.push(Change::ReplaceObject { id: *h, before: Box::new(n.object.clone()), after: Box::new(ObjectKind::Group { open }) });
        }
    }
    if !changes.is_empty() {
        commit_change(state, if open { "Open Group" } else { "Close Group" }, Change::Group(changes))?;
    }
    if !open {
        // Closing selects the heads (members are no longer individually selectable).
        state.selection = heads.into_iter().collect();
        state.sub_selection.clear();
    }
    Ok(selection_json(state))
}

/// Attach to Group: the selected objects join the clicked group (or the
/// clicked member's group).
pub fn attach_to_group(state: &mut AppState, clicked: NodeId) -> Result<Value, String> {
    let head = {
        let n = state.scene.node(clicked).ok_or("missing node")?;
        if matches!(n.object, ObjectKind::Group { .. }) {
            clicked
        } else {
            state.scene.closed_group_of(clicked).or_else(|| n.parent.filter(|p| state.scene.node(*p).is_some_and(|pn| matches!(pn.object, ObjectKind::Group { .. })))).ok_or("click a group")?
        }
    };
    let members: Vec<NodeId> = selection_roots(state).into_iter().filter(|m| *m != head && !state.scene.is_descendant(head, *m)).collect();
    if members.is_empty() {
        return Err("select objects to attach first".into());
    }
    let head_world = state.scene.world_matrix(head);
    let mut changes = Vec::new();
    for m in &members {
        changes.extend(reparent_changes(state, *m, Some(head), head_world));
    }
    commit_static(state, "Attach to Group", Change::Group(changes))?;
    state.pick_mode = None;
    state.selection = BTreeSet::from([head]);
    Ok(selection_json(state))
}

fn detach_from_group(state: &mut AppState) -> Result<Value, String> {
    let members: Vec<NodeId> = state
        .selection
        .iter()
        .copied()
        .filter(|id| state.scene.node(*id).and_then(|n| n.parent).and_then(|p| state.scene.node(p)).is_some_and(|pn| matches!(pn.object, ObjectKind::Group { .. })))
        .collect();
    if members.is_empty() {
        return Err("select members of an open group".into());
    }
    let mut changes = Vec::new();
    for m in &members {
        let head = state.scene.node(*m).and_then(|n| n.parent).unwrap();
        let gp = state.scene.node(head).and_then(|n| n.parent);
        let gp_world = gp.map(|p| state.scene.world_matrix(p)).unwrap_or(glam::Mat4::IDENTITY);
        changes.extend(reparent_changes(state, *m, gp, gp_world));
    }
    commit_static(state, "Detach from Group", Change::Group(changes))?;
    Ok(selection_json(state))
}
