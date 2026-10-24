//! Systems > Biped: creating a biped, the Structure rollout (rebuilding the
//! parts with stable names), Figure Mode and the root name.

use crate::execute::commit_static;
use crate::state::{AppState, SelectionMode};
use crate::Command;
use glam::{Mat4, Vec3};
use loom_scene::biped::{self, BipedStructure, PartSpec};
use loom_scene::{Change, Node, NodeId, ObjectKind, Transform};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Structure values a command may set (None keeps the current one).
#[derive(Debug, Clone, Default)]
pub struct StructureArgs {
    pub height: Option<f32>,
    pub arms: Option<bool>,
    pub counts: Vec<(&'static str, Option<u32>)>,
}

impl StructureArgs {
    fn apply(&self, s: &mut BipedStructure) -> Result<(), String> {
        if let Some(h) = self.height {
            s.set("Height", h as f64)?;
        }
        if let Some(a) = self.arms {
            s.set("Arms", a as u32 as f64)?;
        }
        for (name, v) in &self.counts {
            if let Some(v) = v {
                s.set(name, *v as f64)?;
            }
        }
        Ok(())
    }
}

/// The biped root (centre of mass) `id` belongs to: itself or an ancestor.
pub fn root_of(state: &AppState, id: NodeId) -> Option<NodeId> {
    std::iter::once(id).chain(state.scene.ancestors(id)).find(|n| state.scene.node(*n).is_some_and(|n| matches!(n.object, ObjectKind::Biped(_))))
}

pub fn structure(state: &AppState, root: NodeId) -> Option<&BipedStructure> {
    match &state.scene.node(root)?.object {
        ObjectKind::Biped(b) => Some(b),
        _ => None,
    }
}

/// The biped's parts by their name after the root name ("Pelvis", "L Hand", ...).
pub fn parts(state: &AppState, root: NodeId) -> BTreeMap<String, NodeId> {
    let Some(r) = state.scene.node(root) else { return BTreeMap::new() };
    let prefix = format!("{} ", r.name);
    state
        .scene
        .descendants(root)
        .into_iter()
        .filter_map(|id| {
            let n = state.scene.node(id)?;
            let suffix = n.name.strip_prefix(&prefix)?;
            n.object.is_bone().then(|| (suffix.to_owned(), id))
        })
        .collect()
}

fn part_object(spec: &PartSpec) -> ObjectKind {
    let mut o = ObjectKind::bone(spec.length);
    let _ = o.set_parameter("Width", spec.width as f64);
    let _ = o.set_parameter("Height", spec.depth as f64);
    let _ = o.set_parameter("Taper", 0.0);
    o
}

fn part_node(spec: &PartSpec, root_name: &str, layer: u32) -> Node {
    let mut n = Node::new(&format!("{root_name} {}", spec.suffix), part_object(spec));
    n.wire_color = spec.color;
    n.layer = layer;
    n.object_offset.position.set(spec.offset);
    n
}

/// A new biped standing at `feet`, the centre of mass selected.
pub fn create(state: &mut AppState, feet: Vec3, name: Option<String>, mut s: BipedStructure) -> Result<Value, String> {
    s.figure_mode = true;
    let name = match name.filter(|n| !n.trim().is_empty()) {
        Some(n) if state.scene.nodes().any(|x| x.name == n) => return Err(format!("a node named {n:?} exists")),
        Some(n) => n,
        None => state.scene.next_name(biped::ROOT_BASE),
    };
    let h = s.height.get();
    let specs = biped::layout(&s);
    let frames = biped::frames(&specs);
    let first = state.scene.next_id();
    let root_id = NodeId(first);
    let mut root = Node::new(&name, ObjectKind::Biped(Box::new(s)));
    root.id = root_id;
    root.wire_color = biped::ROOT_COLOR;
    root.layer = state.scene.current_layer;
    root.transform.position.set(feet + Vec3::Z * (biped::COM_HEIGHT * h));
    let mut changes = vec![Change::AddNode(root)];
    let mut ids: HashMap<String, NodeId> = HashMap::new();
    for (i, spec) in specs.iter().enumerate() {
        let id = NodeId(first + 1 + i as u64);
        let mut n = part_node(spec, &name, state.scene.current_layer);
        n.id = id;
        let parent_frame = spec.parent.as_ref().map_or(Mat4::IDENTITY, |p| frames[p]);
        n.parent = Some(spec.parent.as_ref().map_or(root_id, |p| ids[p]));
        n.transform = Transform::from_matrix(spec.local(parent_frame));
        ids.insert(spec.suffix.clone(), id);
        changes.push(Change::AddNode(n));
    }
    commit_static(state, "Create Biped", Change::Group(changes))?;
    // Only the centre of mass is selected after creation, as in the reference.
    state.selection = BTreeSet::from([root_id]);
    state.sub_selection.clear();
    state.selection_mode = SelectionMode::Object;
    Ok(json!({ "root": root_id.0, "name": name, "parts": specs.len() }))
}

/// The Structure rollout: rebuilds the parts for `after`. Parts keep their
/// names (and nodes); a group whose link counts change is laid out again,
/// added parts are new nodes and missing ones are deleted. A height change
/// scales the pose about the centre of mass with the feet kept in place.
pub fn rebuild(state: &mut AppState, root: NodeId, after: BipedStructure, label: &str) -> Result<Value, String> {
    let node = state.scene.node(root).ok_or("no biped")?;
    let before = match &node.object {
        ObjectKind::Biped(b) => (**b).clone(),
        _ => return Err(format!("{} is not a biped", node.name)),
    };
    if !before.figure_mode {
        return Err("turn on Figure Mode to change the biped's structure".into());
    }
    let root_name = node.name.clone();
    let layer = node.layer;
    let root_transform = node.transform.clone();
    let existing = parts(state, root);
    let specs = biped::layout(&after);
    let frames = biped::frames(&specs);
    let k = after.height.get() / before.height.get();
    let mut changes = Vec::new();
    let mut next = state.scene.next_id();
    let mut ids: HashMap<String, NodeId> = HashMap::new();
    let mut added = 0;
    for spec in &specs {
        let parent_id = spec.parent.as_ref().map_or(root, |p| ids[p]);
        let parent_frame = spec.parent.as_ref().map_or(Mat4::IDENTITY, |p| frames[p]);
        let default_local = Transform::from_matrix(spec.local(parent_frame));
        let Some(id) = existing.get(&spec.suffix).copied() else {
            let mut n = part_node(spec, &root_name, layer);
            n.id = NodeId(next);
            next += 1;
            added += 1;
            n.parent = Some(parent_id);
            n.transform = default_local;
            ids.insert(spec.suffix.clone(), n.id);
            changes.push(Change::AddNode(n));
            continue;
        };
        ids.insert(spec.suffix.clone(), id);
        let n = state.scene.node(id).expect("listed part");
        let relayout = after.group_changed(&before, spec.group) || n.parent != Some(parent_id);
        if n.parent != Some(parent_id) {
            changes.push(Change::SetParent { id, before: n.parent, after: Some(parent_id) });
        }
        let transform = if relayout {
            Some(default_local)
        } else if (k - 1.0).abs() > 1e-6 {
            let mut t = n.transform.clone();
            t.position.set(t.position.get() * k);
            Some(t)
        } else {
            None
        };
        if let Some(t) = transform {
            changes.push(Change::SetTransform { id, before: n.transform.clone(), after: t });
        }
        if relayout || (k - 1.0).abs() > 1e-6 {
            let object = part_object(spec);
            if object != n.object {
                changes.push(Change::ReplaceObject { id, before: Box::new(n.object.clone()), after: Box::new(object) });
            }
            let mut props = n.props();
            props.object_offset.position.set(spec.offset);
            if props.object_offset != n.object_offset {
                changes.push(Change::SetNodeProps { id, before: Box::new(n.props()), after: Box::new(props) });
            }
        }
    }
    // Parts the new structure has no place for go, deepest first.
    let mut gone: Vec<NodeId> = existing.iter().filter(|(s, _)| !ids.contains_key(*s)).map(|(_, id)| *id).collect();
    gone.sort_by_key(|id| std::cmp::Reverse(state.scene.depth(*id)));
    let removed = gone.len();
    changes.extend(gone.into_iter().map(Change::RemoveNode));
    // The feet stay where they are when the height changes.
    if (k - 1.0).abs() > 1e-6 {
        let up = root_transform.rotation_quat() * Vec3::Z;
        let mut t = root_transform.clone();
        t.position.set(t.position.get() + up * (biped::COM_HEIGHT * (after.height.get() - before.height.get())));
        changes.push(Change::SetTransform { id: root, before: root_transform, after: t });
    }
    let node = state.scene.node(root).expect("checked");
    changes.push(Change::ReplaceObject { id: root, before: Box::new(node.object.clone()), after: Box::new(ObjectKind::Biped(Box::new(after))) });
    commit_static(state, label, Change::Group(changes))?;
    Ok(json!({ "root": root.0, "parts": specs.len(), "added": added, "removed": removed }))
}

/// The live transforms of the root ("") and every part, by name after the root name.
fn pose(state: &AppState, root: NodeId) -> Vec<(String, Transform)> {
    let mut out = vec![(String::new(), state.scene.node(root).map(|n| n.transform.clone()).unwrap_or_default())];
    for (s, id) in parts(state, root) {
        if let Some(n) = state.scene.node(id) {
            out.push((s, n.transform.clone()));
        }
    }
    out
}

/// Figure Mode: on shows the stored figure pose and keeps the animated pose;
/// off stores the figure pose and brings the animated pose back.
pub fn figure_mode(state: &mut AppState, root: NodeId, on: Option<bool>) -> Result<Value, String> {
    let mut s = structure(state, root).ok_or("no biped")?.clone();
    let on = on.unwrap_or(!s.figure_mode);
    if on == s.figure_mode {
        return Ok(json!({ "figure_mode": on }));
    }
    let live = pose(state, root);
    let apply = if on { std::mem::take(&mut s.figure_pose) } else { std::mem::take(&mut s.anim_pose) };
    if on {
        s.anim_pose = live;
    } else {
        s.figure_pose = live;
    }
    s.figure_mode = on;
    let all = parts(state, root);
    let mut changes = Vec::new();
    for (suffix, t) in apply {
        let id = if suffix.is_empty() { Some(root) } else { all.get(&suffix).copied() };
        if let Some(n) = id.and_then(|id| state.scene.node(id)) {
            if n.transform != t {
                changes.push(Change::SetTransform { id: n.id, before: n.transform.clone(), after: t });
            }
        }
    }
    let node = state.scene.node(root).expect("checked");
    changes.push(Change::ReplaceObject { id: root, before: Box::new(node.object.clone()), after: Box::new(ObjectKind::Biped(Box::new(s))) });
    // The stored poses come back exactly, keys and all.
    state.undo.commit_exact(if on { "Figure Mode On" } else { "Figure Mode Off" }, Change::Group(changes), &mut state.scene)?;
    state.mesh_dirty = true;
    Ok(json!({ "figure_mode": on }))
}

/// Root Name: the centre of mass and every part take the new name.
pub fn rename(state: &mut AppState, root: NodeId, name: &str) -> Result<Value, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("the root name is empty".into());
    }
    let old = state.scene.node(root).ok_or("no biped")?.name.clone();
    if old == name {
        return Ok(json!({ "name": name }));
    }
    let all = parts(state, root);
    let mut changes = vec![Change::SetName { id: root, before: old.clone(), after: name.to_owned() }];
    for (suffix, id) in &all {
        changes.push(Change::SetName { id: *id, before: format!("{old} {suffix}"), after: format!("{name} {suffix}") });
    }
    let renamed: BTreeSet<NodeId> = all.values().copied().chain([root]).collect();
    let new_names: BTreeSet<String> = all.keys().map(|s| format!("{name} {s}")).chain([name.to_owned()]).collect();
    if let Some(clash) = state.scene.nodes().find(|n| !renamed.contains(&n.id) && new_names.contains(&n.name)) {
        return Err(format!("a node named {:?} exists", clash.name));
    }
    commit_static(state, "Root Name", Change::Group(changes))?;
    Ok(json!({ "name": name }))
}

fn selected_root(state: &AppState) -> Result<NodeId, String> {
    state.selection.iter().find_map(|id| root_of(state, *id)).ok_or_else(|| "select a biped".into())
}

/// The default height of a new biped in scene units (1.8 m).
pub fn default_height(state: &AppState) -> f32 {
    (biped::DEFAULT_HEIGHT_METRES as f64 / (state.units.system_unit.metres() * state.units.system_scale as f64)) as f32
}

pub fn execute(state: &mut AppState, cmd: Command) -> Result<Value, String> {
    use Command as C;
    match cmd {
        C::CreateBiped { position, height, name, arms, neck_links, spine_links, leg_links, tail_links, fingers, finger_links, toes, toe_links } => {
            let h = height.unwrap_or_else(|| default_height(state));
            let mut s = BipedStructure::new(h);
            let args = StructureArgs { height: None, arms, counts: counts(neck_links, spine_links, leg_links, tail_links, fingers, finger_links, toes, toe_links) };
            args.apply(&mut s)?;
            create(state, Vec3::from(position), name, s)
        }
        C::SetBipedStructure { id, height, arms, neck_links, spine_links, leg_links, tail_links, fingers, finger_links, toes, toe_links } => {
            let root = root_of(state, NodeId(id)).ok_or_else(|| format!("node {id} is not part of a biped"))?;
            let mut s = structure(state, root).expect("a root").clone();
            let args = StructureArgs { height, arms, counts: counts(neck_links, spine_links, leg_links, tail_links, fingers, finger_links, toes, toe_links) };
            args.apply(&mut s)?;
            rebuild(state, root, s, "Biped Structure")
        }
        C::BipedFigureMode { on } => {
            let root = selected_root(state)?;
            figure_mode(state, root, on)
        }
        C::BipedRootName { id, name } => {
            let root = root_of(state, NodeId(id)).ok_or_else(|| format!("node {id} is not part of a biped"))?;
            rename(state, root, &name)
        }
        other => Err(format!("{} is not a biped command", other.name())),
    }
}

#[allow(clippy::too_many_arguments)]
fn counts(neck: Option<u32>, spine: Option<u32>, leg: Option<u32>, tail: Option<u32>, fingers: Option<u32>, finger_links: Option<u32>, toes: Option<u32>, toe_links: Option<u32>) -> Vec<(&'static str, Option<u32>)> {
    vec![
        ("Neck Links", neck),
        ("Spine Links", spine),
        ("Leg Links", leg),
        ("Tail Links", tail),
        ("Fingers", fingers),
        ("Finger Links", finger_links),
        ("Toes", toes),
        ("Toe Links", toe_links),
    ]
}
