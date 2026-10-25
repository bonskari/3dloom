//! Schematic View windows (Graph Editors > New Schematic View): the node
//! graph of the scene (objects, their base objects, modifiers, materials and
//! transform controllers), its Hierarchy / References layouts, free node
//! positions, and the Connect / Unlink / Delete tools.
//!
//! Entities are named by key: `obj:<node>`, `base:<node>`, `mod:<node>:<index>`
//! (stack index, bottom 0), `mat:<material>`, `xf:<node>` (the transform) and
//! `ctl:<node>:<Position|Rotation|Scale>`.

use crate::{AppState, Command, SelectOp};
use loom_scene::tracks::{self, TrackOwner};
use loom_scene::{Change, NodeId};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

/// Hierarchy Mode (children indented under their parent) or References Mode
/// (objects side by side, what they use hanging below).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SchMode {
    #[default]
    Hierarchy,
    References,
}

/// One Schematic View's setup (saved with the scene).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SchematicView {
    pub id: u32,
    pub name: String,
    pub open: bool,
    pub mode: SchMode,
    pub always_arrange: bool,
    pub sync_selection: bool,
    pub move_children: bool,
    pub show_grid: bool,
    /// Display floater > Entities.
    pub base_objects: bool,
    pub modifiers: bool,
    pub materials: bool,
    pub controllers: bool,
    /// Free entities (moved by hand) at their window positions.
    pub free: BTreeMap<String, [f32; 2]>,
    /// Collapsed entities: their children are not shown.
    pub collapsed: BTreeSet<String>,
    /// Window navigation: the graph point at the canvas's top-left and the zoom.
    pub pan: [f32; 2],
    pub zoom: f32,
    /// Selected entities (objects too when Sync Selection is off).
    #[serde(skip)]
    pub selected: BTreeSet<String>,
}

impl Default for SchematicView {
    fn default() -> Self {
        SchematicView {
            id: 0,
            name: String::new(),
            open: false,
            mode: SchMode::Hierarchy,
            always_arrange: false,
            sync_selection: true,
            move_children: false,
            show_grid: true,
            base_objects: false,
            modifiers: true,
            materials: true,
            controllers: false,
            free: BTreeMap::new(),
            collapsed: BTreeSet::new(),
            pan: [-20.0, -20.0],
            zoom: 1.0,
            selected: BTreeSet::new(),
        }
    }
}

/// Every Schematic View of the scene.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Schematics {
    pub list: Vec<SchematicView>,
    pub last: Option<String>,
    pub next_id: u32,
}

impl Schematics {
    pub fn index(&self, name: Option<&str>) -> Option<usize> {
        let name = name.or(self.last.as_deref())?;
        self.list.iter().position(|v| v.name == name)
    }

    pub fn get(&self, name: Option<&str>) -> Option<&SchematicView> {
        self.index(name).map(|i| &self.list[i])
    }

    fn get_mut(&mut self, name: Option<&str>) -> Result<&mut SchematicView, String> {
        let i = self.index(name).ok_or_else(|| match name {
            Some(n) => format!("no Schematic View `{n}`"),
            None => "no Schematic View".to_owned(),
        })?;
        Ok(&mut self.list[i])
    }

    pub fn create(&mut self) -> &SchematicView {
        let mut n = 1;
        while self.list.iter().any(|v| v.name == format!("Schematic View {n}")) {
            n += 1;
        }
        self.next_id += 1;
        let v = SchematicView { id: self.next_id, name: format!("Schematic View {n}"), open: true, ..Default::default() };
        self.last = Some(v.name.clone());
        self.list.push(v);
        self.list.last().expect("just pushed")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityKind {
    Object,
    BaseObject,
    Modifier(usize),
    Material(u32),
    Transform,
    Controller,
}

/// A box of the graph.
#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    pub key: String,
    pub label: String,
    pub kind: EntityKind,
    pub node: NodeId,
    /// The entity it hangs under in the layout (its parent in the tree).
    pub owner: Option<String>,
    /// Red border: the entity is animated.
    pub animated: bool,
    /// Bold label: an instance (base object) or shared (material, modifier).
    pub instanced: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireKind {
    /// Parent object to child object.
    Hierarchy,
    /// Object to what it uses (modifier, material, base object, transform, controller).
    Uses,
    /// An object's link to a shared material or to the object it instances.
    Shared,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Wire {
    pub from: String,
    pub to: String,
    pub kind: WireKind,
}

pub fn obj_key(id: NodeId) -> String {
    format!("obj:{}", id.0)
}

/// The node an `obj:` key names.
pub fn key_node(key: &str) -> Option<NodeId> {
    key.strip_prefix("obj:").and_then(|n| n.parse().ok()).map(NodeId)
}

/// The graph the view shows (collapsed branches included; `visible` leaves them out).
pub fn graph(state: &AppState, v: &SchematicView) -> (Vec<Entity>, Vec<Wire>) {
    let s = &state.scene;
    let mut ents: Vec<Entity> = Vec::new();
    let mut wires: Vec<Wire> = Vec::new();
    let mut materials: BTreeMap<u32, String> = BTreeMap::new();
    let mut shared_mats: BTreeMap<u32, usize> = BTreeMap::new();
    for n in s.nodes() {
        if let Some(m) = n.material {
            *shared_mats.entry(m.0).or_default() += 1;
        }
    }
    let instanced: BTreeSet<NodeId> = s.nodes().filter_map(|n| n.instance_of.map(|src| [n.id, src])).flatten().collect();
    let children = crate::explorer::children_of(s);
    #[allow(clippy::too_many_arguments)]
    fn visit(state: &AppState, v: &SchematicView, id: NodeId, parent: Option<NodeId>, children: &BTreeMap<Option<NodeId>, Vec<NodeId>>, instanced: &BTreeSet<NodeId>, shared_mats: &BTreeMap<u32, usize>, materials: &mut BTreeMap<u32, String>, ents: &mut Vec<Entity>, wires: &mut Vec<Wire>) {
        let s = &state.scene;
        let Some(n) = s.node(id) else { return };
        let key = obj_key(id);
        let owner = match (v.mode, parent) {
            (SchMode::Hierarchy, Some(p)) => Some(obj_key(p)),
            _ => None,
        };
        let node_tracks = tracks::node_tracks(n);
        let xf: Vec<(String, bool, String)> = node_tracks
            .iter()
            .filter(|t| t.owner == TrackOwner::Transform)
            .filter_map(|t| tracks::read_node(n, t).map(|p| (t.name.clone(), p.is_animated(), crate::track_view::controller_name(n, t, &p))))
            .collect();
        let animated = node_tracks.iter().any(|t| tracks::read_node(n, t).is_some_and(|p| p.is_animated()));
        ents.push(Entity { key: key.clone(), label: n.name.clone(), kind: EntityKind::Object, node: id, owner, animated, instanced: false });
        if let Some(p) = parent {
            wires.push(Wire { from: obj_key(p), to: key.clone(), kind: WireKind::Hierarchy });
        }
        if let Some(src) = n.instance_of {
            wires.push(Wire { from: obj_key(src), to: key.clone(), kind: WireKind::Shared });
        }
        if v.controllers {
            let xk = format!("xf:{}", id.0);
            ents.push(Entity { key: xk.clone(), label: "Transform".into(), kind: EntityKind::Transform, node: id, owner: Some(key.clone()), animated: xf.iter().any(|x| x.1), instanced: false });
            wires.push(Wire { from: key.clone(), to: xk.clone(), kind: WireKind::Uses });
            for (track, anim, ctl) in &xf {
                let ck = format!("ctl:{}:{track}", id.0);
                ents.push(Entity { key: ck.clone(), label: format!("{track}: {ctl}"), kind: EntityKind::Controller, node: id, owner: Some(xk.clone()), animated: *anim, instanced: false });
                wires.push(Wire { from: xk.clone(), to: ck, kind: WireKind::Uses });
            }
        }
        if v.modifiers {
            // Top of the stack first, as the stack reads.
            for (i, m) in n.modifiers.iter().enumerate().rev() {
                let mk = format!("mod:{}:{i}", id.0);
                let anim = node_tracks.iter().any(|t| t.owner == TrackOwner::Modifier(i) && tracks::read_node(n, t).is_some_and(|p| p.is_animated()));
                ents.push(Entity { key: mk.clone(), label: m.name.clone(), kind: EntityKind::Modifier(i), node: id, owner: Some(key.clone()), animated: anim, instanced: m.instance_of.is_some() });
                wires.push(Wire { from: key.clone(), to: mk, kind: WireKind::Uses });
            }
        }
        if v.base_objects {
            let bk = format!("base:{}", id.0);
            let anim = node_tracks.iter().any(|t| t.owner == TrackOwner::Object && tracks::read_node(n, t).is_some_and(|p| p.is_animated()));
            ents.push(Entity { key: bk.clone(), label: n.object.base_name().into(), kind: EntityKind::BaseObject, node: id, owner: Some(key.clone()), animated: anim, instanced: instanced.contains(&id) });
            wires.push(Wire { from: key.clone(), to: bk, kind: WireKind::Uses });
        }
        if v.materials {
            if let Some(m) = n.material {
                let mk = format!("mat:{}", m.0);
                if let std::collections::btree_map::Entry::Vacant(slot) = materials.entry(m.0) {
                    if let Some(mat) = s.materials.get(&m) {
                        slot.insert(mat.name.clone());
                        ents.push(Entity { key: mk.clone(), label: mat.name.clone(), kind: EntityKind::Material(m.0), node: id, owner: Some(key.clone()), animated: false, instanced: shared_mats.get(&m.0).is_some_and(|c| *c > 1) });
                        wires.push(Wire { from: key.clone(), to: mk, kind: WireKind::Uses });
                    }
                } else {
                    // A material another object showed first: wired to it.
                    wires.push(Wire { from: key.clone(), to: mk, kind: WireKind::Shared });
                }
            }
        }
        for c in children.get(&Some(id)).map(|v| v.as_slice()).unwrap_or(&[]) {
            visit(state, v, *c, Some(id), children, instanced, shared_mats, materials, ents, wires);
        }
    }
    for r in children.get(&None).map(|v| v.as_slice()).unwrap_or(&[]) {
        visit(state, v, *r, None, &children, &instanced, &shared_mats, &mut materials, &mut ents, &mut wires);
    }
    (ents, wires)
}

/// The entities not inside a collapsed one.
pub fn visible(v: &SchematicView, ents: &[Entity]) -> Vec<Entity> {
    let by_key: BTreeMap<&str, &Entity> = ents.iter().map(|e| (e.key.as_str(), e)).collect();
    ents.iter()
        .filter(|e| {
            let mut cur = e.owner.as_deref();
            while let Some(o) = cur {
                if v.collapsed.contains(o) {
                    return false;
                }
                cur = by_key.get(o).and_then(|p| p.owner.as_deref());
            }
            true
        })
        .cloned()
        .collect()
}

/// Entity box size and spacing in graph units.
pub const BOX_W: f32 = 110.0;
pub const BOX_H: f32 = 20.0;
const ROW: f32 = 28.0;
const INDENT: f32 = 24.0;
const COL: f32 = 130.0;

/// The arranged positions (top-left corners) of the visible entities.
pub fn arrange(v: &SchematicView, ents: &[Entity]) -> BTreeMap<String, [f32; 2]> {
    let depth = |e: &Entity| -> usize {
        let by_key: BTreeMap<&str, &Entity> = ents.iter().map(|e| (e.key.as_str(), e)).collect();
        let mut d = 0;
        let mut cur = e.owner.as_deref();
        while let Some(o) = cur {
            d += 1;
            cur = by_key.get(o).and_then(|p| p.owner.as_deref());
        }
        d
    };
    let mut out = BTreeMap::new();
    match v.mode {
        SchMode::Hierarchy => {
            // An indented list in tree order.
            for (row, e) in ents.iter().enumerate() {
                out.insert(e.key.clone(), [depth(e) as f32 * INDENT, row as f32 * ROW]);
            }
        }
        SchMode::References => {
            // Each top-level object a column, what it uses stacked below.
            let mut col = -1.0f32;
            let mut row = 0.0f32;
            for e in ents {
                if e.owner.is_none() {
                    col += 1.0;
                    row = 0.0;
                } else {
                    row += 1.0;
                }
                out.insert(e.key.clone(), [col * COL + (depth(e).min(3) as f32) * 6.0, row * ROW]);
            }
        }
    }
    out
}

/// Where each visible entity is drawn: free ones where they were left
/// (unless Always Arrange), the rest arranged.
pub fn positions(v: &SchematicView, ents: &[Entity]) -> BTreeMap<String, [f32; 2]> {
    let mut p = arrange(v, ents);
    if !v.always_arrange {
        for (k, at) in p.iter_mut() {
            if let Some(f) = v.free.get(k) {
                *at = *f;
            }
        }
    }
    p
}

/// The selected entities: with Sync Selection the selected objects are the scene selection.
pub fn selected(state: &AppState, v: &SchematicView) -> BTreeSet<String> {
    if !v.sync_selection {
        return v.selected.clone();
    }
    let mut out: BTreeSet<String> = v.selected.iter().filter(|k| !k.starts_with("obj:")).cloned().collect();
    out.extend(state.selection.iter().map(|id| obj_key(*id)));
    out
}

/// All entities under `keys` in the layout tree (not the keys themselves).
pub fn descendants(ents: &[Entity], keys: &BTreeSet<String>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut grew = true;
    while grew {
        grew = false;
        for e in ents {
            if let Some(o) = &e.owner {
                if (keys.contains(o) || out.contains(o)) && !keys.contains(&e.key) && out.insert(e.key.clone()) {
                    grew = true;
                }
            }
        }
    }
    out
}

pub const OPTIONS: &[&str] = &[
    "Hierarchy Mode", "References Mode", "Always Arrange", "Sync Selection", "Move Children", "Show Grid",
    "Base Objects", "Modifier Stack", "Materials", "Controllers", "Free All", "Free Selected", "Arrange Selected",
];

pub fn handles(c: &Command) -> bool {
    use Command as C;
    matches!(
        c,
        C::NewSchematicView
            | C::OpenSchematicView { .. }
            | C::CloseSchematicView { .. }
            | C::DeleteSchematicView { .. }
            | C::RenameSchematicView { .. }
            | C::SchematicOption { .. }
            | C::SchematicSelect { .. }
            | C::SchematicConnect { .. }
            | C::SchematicUnlinkSelected { .. }
            | C::SchematicDeleteSelected { .. }
    )
}

/// The selected entities of the view that are objects.
fn selected_objects(state: &AppState, view: Option<&str>) -> Result<Vec<NodeId>, String> {
    let v = state.schematics.get(view).ok_or("no Schematic View")?;
    Ok(selected(state, v).iter().filter_map(|k| key_node(k)).filter(|id| state.scene.node(*id).is_some()).collect())
}

pub fn execute(state: &mut AppState, cmd: Command) -> Result<Value, String> {
    use Command as C;
    match cmd {
        C::NewSchematicView => {
            let v = state.schematics.create();
            Ok(json!({ "name": v.name }))
        }
        C::OpenSchematicView { name } => {
            if name.is_none() && state.schematics.index(None).is_none() {
                let v = state.schematics.create();
                return Ok(json!({ "name": v.name }));
            }
            let v = state.schematics.get_mut(name.as_deref())?;
            v.open = true;
            let n = v.name.clone();
            state.schematics.last = Some(n.clone());
            Ok(json!({ "name": n }))
        }
        C::CloseSchematicView { name } => {
            state.schematics.get_mut(name.as_deref())?.open = false;
            Ok(json!({}))
        }
        C::DeleteSchematicView { name } => {
            let i = state.schematics.index(Some(&name)).ok_or_else(|| format!("no Schematic View `{name}`"))?;
            state.schematics.list.remove(i);
            if state.schematics.last.as_deref() == Some(name.as_str()) {
                state.schematics.last = state.schematics.list.last().map(|v| v.name.clone());
            }
            Ok(json!({}))
        }
        C::RenameSchematicView { view, name } => {
            let name = name.trim().to_owned();
            if name.is_empty() {
                return Err("a Schematic View needs a name".into());
            }
            let sc = &mut state.schematics;
            let i = sc.index(view.as_deref()).ok_or("no Schematic View")?;
            if sc.list.iter().enumerate().any(|(j, v)| j != i && v.name == name) {
                return Err(format!("a Schematic View is already called `{name}`"));
            }
            let old = std::mem::replace(&mut sc.list[i].name, name.clone());
            if sc.last.as_deref() == Some(old.as_str()) || view.is_none() {
                sc.last = Some(name);
            }
            Ok(json!({}))
        }
        C::SchematicOption { view, option, on } => {
            let i = state.schematics.index(view.as_deref()).ok_or("no Schematic View")?;
            let sel = selected(state, &state.schematics.list[i]);
            let (ents, _) = graph(state, &state.schematics.list[i]);
            let ents = visible(&state.schematics.list[i], &ents);
            let v = &mut state.schematics.list[i];
            match option.as_str() {
                "Hierarchy Mode" => v.mode = if on { SchMode::Hierarchy } else { SchMode::References },
                "References Mode" => v.mode = if on { SchMode::References } else { SchMode::Hierarchy },
                "Always Arrange" => v.always_arrange = on,
                "Sync Selection" => {
                    if v.sync_selection && !on {
                        v.selected = sel;
                    }
                    v.sync_selection = on;
                }
                "Move Children" => v.move_children = on,
                "Show Grid" => v.show_grid = on,
                "Base Objects" => v.base_objects = on,
                "Modifier Stack" => v.modifiers = on,
                "Materials" => v.materials = on,
                "Controllers" => v.controllers = on,
                "Free All" => {
                    // Every entity keeps its place and becomes free.
                    v.free = positions(v, &ents);
                }
                "Free Selected" => {
                    let at = positions(v, &ents);
                    for k in &sel {
                        if let Some(p) = at.get(k) {
                            v.free.insert(k.clone(), *p);
                        }
                    }
                }
                "Arrange Selected" => {
                    for k in &sel {
                        v.free.remove(k);
                    }
                }
                o => return Err(format!("no Schematic View option `{o}`")),
            }
            Ok(json!({}))
        }
        C::SchematicSelect { view, keys, op } => {
            let i = state.schematics.index(view.as_deref()).ok_or("no Schematic View")?;
            let mut sel = selected(state, &state.schematics.list[i]);
            match op {
                SelectOp::Replace => sel = keys.iter().cloned().collect(),
                SelectOp::Add => sel.extend(keys.iter().cloned()),
                SelectOp::Subtract => sel.retain(|k| !keys.contains(k)),
            }
            let sync = state.schematics.list[i].sync_selection;
            state.schematics.list[i].selected = sel.clone();
            if sync {
                let ids: Vec<u64> = sel.iter().filter_map(|k| key_node(k)).filter(|id| state.scene.node(*id).is_some()).map(|n| n.0).collect();
                crate::execute::execute(state, Command::SelectNodes { ids, op: SelectOp::Replace })?;
            }
            Ok(json!({ "selected": sel }))
        }
        C::SchematicConnect { from, to } => {
            let target = key_node(&to).filter(|id| state.scene.node(*id).is_some()).ok_or_else(|| format!("connect to an object, not `{to}`"))?;
            if let Some(child) = key_node(&from) {
                crate::execute::execute(state, Command::SetParent { id: child.0, parent: Some(target.0) })
            } else if let Some(m) = from.strip_prefix("mat:").and_then(|m| m.parse::<u32>().ok()) {
                let m = loom_scene::material::MaterialId(m);
                if !state.scene.materials.contains_key(&m) {
                    return Err(format!("no material {}", m.0));
                }
                let before = state.scene.node(target).and_then(|n| n.material);
                crate::execute::commit(state, "Assign Material", Change::SetMaterial { id: target, before, after: Some(m) })?;
                Ok(json!({}))
            } else {
                Err(format!("`{from}` cannot be connected to an object"))
            }
        }
        C::SchematicUnlinkSelected { view } => {
            let ids = selected_objects(state, view.as_deref())?;
            let mut changes = Vec::new();
            for id in ids {
                if state.scene.node(id).and_then(|n| n.parent).is_some() {
                    changes.push(crate::execute::link_change(state, id, None)?);
                }
            }
            if !changes.is_empty() {
                crate::execute::commit_static(state, "Unlink", Change::Group(changes))?;
            }
            Ok(json!({}))
        }
        C::SchematicDeleteSelected { view } => {
            let ids = selected_objects(state, view.as_deref())?;
            crate::execute::execute(state, Command::DeleteNodes { ids: ids.into_iter().map(|n| n.0).collect() })
        }
        _ => Err("not a Schematic View command".into()),
    }
}
