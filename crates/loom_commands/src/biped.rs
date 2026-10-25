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

/// The Create Biped / Set Biped Structure arguments and the rollout
/// parameters they set, in the order they apply (Knuckles before Fingers,
/// which it sets).
pub const STRUCTURE_ARGS: [(&str, &str); 28] = [
    ("body_type", "Body Type"),
    ("height", "Height"),
    ("arms", "Arms"),
    ("knuckles", "Knuckles"),
    ("forefeet", "ForeFeet"),
    ("short_thumb", "Short Thumb"),
    ("neck_links", "Neck Links"),
    ("spine_links", "Spine Links"),
    ("leg_links", "Leg Links"),
    ("tail_links", "Tail Links"),
    ("ponytail1_links", "Ponytail1 Links"),
    ("ponytail2_links", "Ponytail2 Links"),
    ("fingers", "Fingers"),
    ("finger_links", "Finger Links"),
    ("toes", "Toes"),
    ("toe_links", "Toe Links"),
    ("prop1", "Prop1"),
    ("prop2", "Prop2"),
    ("prop3", "Prop3"),
    ("ankle_attach", "Ankle Attach"),
    ("triangle_pelvis", "Triangle Pelvis"),
    ("triangle_neck", "Triangle Neck"),
    ("twists", "Twists"),
    ("upper_arm_twists", "Upper Arm"),
    ("forearm_twists", "Forearm"),
    ("thigh_twists", "Thigh"),
    ("calf_twists", "Calf"),
    ("horse_link_twists", "Horse Link"),
];

/// Applies a command's structure arguments (absent ones keep their value).
fn apply_args(cmd: &Command, s: &mut BipedStructure) -> Result<(), String> {
    let v = serde_json::to_value(cmd).map_err(|e| e.to_string())?;
    let args = &v["args"];
    for (field, name) in STRUCTURE_ARGS {
        let a = &args[field];
        let value = match a {
            Value::Null => continue,
            Value::Bool(b) => *b as u32 as f64,
            Value::Number(n) => n.as_f64().unwrap_or(0.0),
            Value::String(t) if field == "body_type" => biped::BodyType::from_name(t).ok_or_else(|| format!("unknown Body Type {t:?} (Skeleton, Male, Female, Classic)"))?.index() as f64,
            other => return Err(format!("{field}: unexpected {other}")),
        };
        s.set(name, value)?;
    }
    Ok(())
}

/// The value of rollout parameter `name` as a command argument.
fn arg_value(s: &BipedStructure, name: &str) -> Value {
    match name {
        "Body Type" => json!(s.body_type.label()),
        "Height" => json!(s.height.get()),
        "Ankle Attach" => json!(s.ankle_attach.get()),
        _ => s.flag(name).map(|b| json!(b)).or_else(|| s.count(name).map(|c| json!(c))).unwrap_or(Value::Null),
    }
}

fn command_from(name: &str, args: Value) -> Command {
    serde_json::from_value(json!({ "cmd": name, "args": args })).expect("a valid biped command")
}

/// Create Biped with every structure value of `s` (but its height).
pub fn create_command(position: Vec3, height: Option<f32>, name: Option<String>, s: &BipedStructure) -> Command {
    let mut args = serde_json::Map::new();
    for (field, n) in STRUCTURE_ARGS {
        if n != "Height" {
            args.insert(field.to_owned(), arg_value(s, n));
        }
    }
    args.insert("position".into(), json!(position.to_array()));
    args.insert("height".into(), json!(height));
    args.insert("name".into(), json!(name));
    command_from("Create Biped", Value::Object(args))
}

/// Set Biped Structure on `id` with only the arguments in `args` (an object of
/// `STRUCTURE_ARGS` fields).
pub fn structure_command(id: NodeId, args: Value) -> Command {
    let mut a = args;
    a["id"] = json!(id.0);
    command_from("Set Biped Structure", a)
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
    let _ = o.set_parameter("Taper", spec.taper as f64);
    o
}

fn part_node(spec: &PartSpec, root_name: &str, layer: u32) -> Node {
    let mut n = Node::new(&format!("{root_name} {}", spec.suffix), part_object(spec));
    n.wire_color = spec.color;
    n.frozen = spec.frozen;
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

/// The structure with `f` applied, rebuilt as one undo entry `label`.
fn edit_structure(state: &mut AppState, root: NodeId, label: &str, f: impl FnOnce(&mut BipedStructure) -> Result<Value, String>) -> Result<Value, String> {
    let mut s = structure(state, root).ok_or("no biped")?.clone();
    let out = f(&mut s)?;
    rebuild(state, root, s, label)?;
    Ok(out)
}

/// Whether part `suffix` is one of `x`'s links or its nub.
fn part_of(x: &biped::Xtra, suffix: &str) -> bool {
    suffix.strip_prefix(x.name.as_str()).is_some_and(|r| r.is_empty() || r == "Nub" || r.parse::<u32>().is_ok())
}

/// A tail name that no tail or other part uses.
fn check_xtra_name(s: &BipedStructure, taken: &BTreeSet<String>, name: &str) -> Result<(), String> {
    if name.starts_with("L ") || name.starts_with("R ") {
        return Err(format!("{name:?}: L and R name the sides"));
    }
    let clash = |t: &String| t == name || (t.starts_with(name) && t[name.len()..].parse::<u32>().is_ok()) || t == &format!("{name}Nub");
    if s.xtras.iter().any(|x| x.name == name) || taken.iter().any(clash) {
        return Err(format!("the biped has a part named {name:?}"));
    }
    Ok(())
}

fn xtra_index(s: &BipedStructure, name: &str) -> Result<usize, String> {
    s.xtras.iter().position(|x| x.name == name).ok_or_else(|| format!("no Xtra named {name:?}"))
}

/// Xtras > Create Xtra: a new tail on the centre of mass, beside the pelvis.
pub fn create_xtra(state: &mut AppState, root: NodeId, name: Option<String>, links: Option<u32>) -> Result<Value, String> {
    let taken: BTreeSet<String> = parts(state, root).into_keys().collect();
    edit_structure(state, root, "Create Xtra", |s| {
        let name = name.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty()).unwrap_or_else(|| s.next_xtra_name());
        check_xtra_name(s, &taken, &name)?;
        s.xtras.push(biped::Xtra { name: name.clone(), links: links.unwrap_or(1).clamp(1, 25), parent: None, base: Vec3::new(0.05, 0.06, biped::COM_HEIGHT), dir: Vec3::new(0.0, 0.5, 0.85).normalize(), opposite: None });
        Ok(json!({ "name": name }))
    })
}

/// Xtras > Delete Xtra: tails parented to it go to the centre of mass where they are.
pub fn delete_xtra(state: &mut AppState, root: NodeId, name: &str) -> Result<Value, String> {
    edit_structure(state, root, "Delete Xtra", |s| {
        let i = xtra_index(s, name)?;
        let gone = s.xtras.remove(i);
        for x in &mut s.xtras {
            if x.parent.as_deref().is_some_and(|p| part_of(&gone, p)) {
                x.parent = None;
            }
            if x.opposite.as_deref() == Some(gone.name.as_str()) {
                x.opposite = None;
            }
        }
        Ok(json!({ "deleted": gone.name }))
    })
}

/// The part on the other side: "L Hand" <-> "R Hand".
fn mirror_name(n: &str) -> String {
    if let Some(r) = n.strip_prefix("L ") {
        format!("R {r}")
    } else if let Some(r) = n.strip_prefix("R ") {
        format!("L {r}")
    } else {
        n.to_owned()
    }
}

/// Xtras > Create Opposite Xtra: the mirror image of `name` across the biped.
pub fn create_opposite_xtra(state: &mut AppState, root: NodeId, name: &str) -> Result<Value, String> {
    let taken: BTreeSet<String> = parts(state, root).into_keys().collect();
    edit_structure(state, root, "Create Opposite Xtra", |s| {
        let i = xtra_index(s, name)?;
        if s.xtras[i].opposite.is_some() {
            return Err(format!("{name} already has an opposite Xtra"));
        }
        let o = s.xtras[i].clone();
        let new = format!("{}Opp", o.name);
        check_xtra_name(s, &taken, &new)?;
        let flip = Vec3::new(-1.0, 1.0, 1.0);
        let parent = o.parent.as_deref().map(|p| match s.xtras.iter().find(|x| part_of(x, p)).and_then(|x| x.opposite.clone().map(|op| (x.name.clone(), op))) {
            // A tail on a tail: the opposite tail's link.
            Some((src, op)) => format!("{op}{}", &p[src.len()..]),
            None => mirror_name(p),
        });
        s.xtras[i].opposite = Some(new.clone());
        s.xtras.push(biped::Xtra { name: new.clone(), links: o.links, parent, base: o.base * flip, dir: o.dir * flip, opposite: Some(o.name.clone()) });
        Ok(json!({ "name": new }))
    })
}

/// Xtras: rename `name`, set its Links, or give it a new parent (Pick
/// Parent; `reorient` moves it to the parent's end and along it).
pub fn set_xtra(state: &mut AppState, root: NodeId, name: &str, new_name: Option<String>, links: Option<u32>, parent: Option<u64>, reorient: bool) -> Result<Value, String> {
    let all = parts(state, root);
    let mut s = structure(state, root).ok_or("no biped")?.clone();
    let i = xtra_index(&s, name)?;
    // The picked parent as a part name, with its end and direction in biped space (heights from the feet).
    let mut picked: Option<(Option<String>, Vec3, Vec3)> = None;
    if let Some(p) = parent.map(NodeId) {
        let h = s.height.get();
        let lift = Vec3::Z * biped::COM_HEIGHT;
        if p == root {
            picked = Some((None, lift, Vec3::Z));
        } else {
            let (suffix, _) = all.iter().find(|(_, id)| **id == p).ok_or("pick a part of the same biped")?;
            if part_of(&s.xtras[i], suffix) {
                return Err("a tail cannot be its own parent".into());
            }
            let n = state.scene.node(p).expect("a part");
            let m = state.scene.world_matrix(root).inverse() * state.scene.world_matrix(p);
            let tip = m.transform_point3(n.object.bone_tip_local().unwrap_or(Vec3::ZERO));
            let dir = m.transform_vector3(Vec3::X).normalize_or(Vec3::Z);
            picked = Some((Some(suffix.clone()), tip / h + lift, dir));
        }
    }
    let mut changes = Vec::new();
    let root_name = state.scene.node(root).expect("a biped").name.clone();
    if let Some(new) = new_name.map(|n| n.trim().to_owned()).filter(|n| n != name) {
        if new.is_empty() {
            return Err("the Xtra name is empty".into());
        }
        let others: BTreeSet<String> = all.keys().filter(|t| !part_of(&s.xtras[i], t)).cloned().collect();
        check_xtra_name(&s, &others, &new)?;
        // The tail's nodes keep their identity under the new name.
        for (suffix, id) in all.iter().filter(|(t, _)| part_of(&s.xtras[i], t)) {
            let after = format!("{new}{}", &suffix[name.len()..]);
            changes.push(Change::SetName { id: *id, before: format!("{root_name} {suffix}"), after: format!("{root_name} {after}") });
        }
        let old = std::mem::replace(&mut s.xtras[i].name, new.clone());
        let renamed = s.xtras[i].clone();
        let was = biped::Xtra { name: old.clone(), ..renamed };
        for x in &mut s.xtras {
            if let Some(p) = x.parent.as_mut().filter(|p| part_of(&was, p)) {
                *p = format!("{new}{}", &p[old.len()..]);
            }
            if x.opposite.as_deref() == Some(old.as_str()) {
                x.opposite = Some(new.clone());
            }
        }
    }
    if let Some(l) = links {
        s.xtras[i].links = l.clamp(1, 25);
    }
    if let Some((p, base, dir)) = picked {
        s.xtras[i].parent = p;
        if reorient {
            s.xtras[i].base = base;
            s.xtras[i].dir = dir;
        }
    }
    let name = s.xtras[i].name.clone();
    state.undo.begin("Xtra");
    let r = if changes.is_empty() { Ok(()) } else { commit_static(state, "Xtra", Change::Group(changes)) }.and_then(|_| rebuild(state, root, s, "Xtra"));
    state.undo.end();
    r?;
    Ok(json!({ "name": name }))
}

/// A figure file: the structure and the figure pose of a biped, without its
/// animation. The reference's .fig format is binary and undocumented, so
/// 3dLoom writes its own JSON under the same extension.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct FigFile {
    pub format: String,
    pub version: u32,
    pub structure: BipedStructure,
    /// Each part's transform relative to its parent, by name after the root name.
    pub pose: Vec<(String, Transform)>,
}

pub const FIG_FORMAT: &str = "loom-biped-figure";

/// Biped rollout > Save File (Figure Mode): the structure and figure pose to `path`.
pub fn save_fig(state: &AppState, root: NodeId, path: &std::path::Path) -> Result<Value, String> {
    let mut s = structure(state, root).ok_or("no biped")?.clone();
    if !s.figure_mode {
        return Err("turn on Figure Mode to save a figure file".into());
    }
    s.figure_pose.clear();
    s.anim_pose.clear();
    let pose: Vec<(String, Transform)> = parts(state, root).into_iter().filter_map(|(suffix, id)| state.scene.node(id).map(|n| (suffix, Transform::from_matrix(n.transform.matrix())))).collect();
    let n = pose.len();
    let fig = FigFile { format: FIG_FORMAT.into(), version: 1, structure: s, pose };
    let text = serde_json::to_string_pretty(&fig).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(json!({ "path": path.to_string_lossy(), "parts": n }))
}

/// Biped rollout > Load File (Figure Mode): the file's structure and figure
/// pose replace the biped's; its name, place and animation stay. One undo entry.
pub fn load_fig(state: &mut AppState, root: NodeId, path: &std::path::Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let fig: FigFile = serde_json::from_str(&text).map_err(|e| format!("{}: not a figure file ({e})", path.display()))?;
    if fig.format != FIG_FORMAT {
        return Err(format!("{}: not a figure file", path.display()));
    }
    let cur = structure(state, root).ok_or("no biped")?.clone();
    if !cur.figure_mode {
        return Err("turn on Figure Mode to load a figure file".into());
    }
    let mut s = fig.structure;
    s.figure_mode = true;
    s.figure_pose = cur.figure_pose;
    s.anim_pose = cur.anim_pose;
    state.undo.begin("Load Figure");
    let r = rebuild(state, root, s, "Load Figure").and_then(|_| {
        let all = parts(state, root);
        let changes: Vec<Change> = fig
            .pose
            .iter()
            .filter_map(|(suffix, t)| {
                let n = state.scene.node(*all.get(suffix)?)?;
                (n.transform != *t).then(|| Change::SetTransform { id: n.id, before: n.transform.clone(), after: t.clone() })
            })
            .collect();
        if changes.is_empty() {
            Ok(())
        } else {
            commit_static(state, "Load Figure", Change::Group(changes))
        }
    });
    state.undo.end();
    r?;
    Ok(json!({ "path": path.to_string_lossy(), "parts": parts(state, root).len() }))
}

fn root_arg(state: &AppState, id: u64) -> Result<NodeId, String> {
    root_of(state, NodeId(id)).ok_or_else(|| format!("node {id} is not part of a biped"))
}

pub fn execute(state: &mut AppState, cmd: Command) -> Result<Value, String> {
    use Command as C;
    match &cmd {
        C::CreateBiped { position, height, name, .. } => {
            let h = height.unwrap_or_else(|| default_height(state));
            let mut s = BipedStructure::new(h);
            apply_args(&cmd, &mut s)?;
            s.set("Height", h as f64)?;
            create(state, Vec3::from(*position), name.clone(), s)
        }
        C::SetBipedStructure { id, .. } => {
            let root = root_arg(state, *id)?;
            let mut s = structure(state, root).expect("a root").clone();
            apply_args(&cmd, &mut s)?;
            rebuild(state, root, s, "Biped Structure")
        }
        C::BipedFigureMode { on } => {
            let root = selected_root(state)?;
            figure_mode(state, root, *on)
        }
        C::BipedRootName { id, name } => {
            let root = root_arg(state, *id)?;
            rename(state, root, name)
        }
        C::BipedCreateXtra { id, name, links } => {
            let root = root_arg(state, *id)?;
            create_xtra(state, root, name.clone(), *links)
        }
        C::BipedDeleteXtra { id, name } => {
            let root = root_arg(state, *id)?;
            delete_xtra(state, root, name)
        }
        C::BipedCreateOppositeXtra { id, name } => {
            let root = root_arg(state, *id)?;
            create_opposite_xtra(state, root, name)
        }
        C::BipedSetXtra { id, name, new_name, links, parent, reorient } => {
            let root = root_arg(state, *id)?;
            set_xtra(state, root, name, new_name.clone(), *links, *parent, reorient.unwrap_or(true))
        }
        C::BipedPickXtraParent { name, reorient } => {
            let root = selected_root(state)?;
            xtra_index(structure(state, root).expect("a root"), name)?;
            state.biped_xtra_pick = Some((root, name.clone(), reorient.unwrap_or(true)));
            state.pick_mode = Some(crate::state::PickMode::BipedXtraParent);
            state.status_prompt = format!("Pick the new parent of {name}");
            Ok(json!({}))
        }
        C::BipedSaveFile { path } => {
            let root = selected_root(state)?;
            match path {
                Some(p) => save_fig(state, root, std::path::Path::new(p)),
                None => {
                    state.dialog = Some(crate::viewport::Dialog::BipedSaveFig);
                    Ok(json!({ "dialog": "BipedSaveFig" }))
                }
            }
        }
        C::BipedLoadFile { path } => {
            let root = selected_root(state)?;
            match path {
                Some(p) => load_fig(state, root, std::path::Path::new(p)),
                None => {
                    state.dialog = Some(crate::viewport::Dialog::BipedLoadFig);
                    Ok(json!({ "dialog": "BipedLoadFig" }))
                }
            }
        }
        other => Err(format!("{} is not a biped command", other.name())),
    }
}

