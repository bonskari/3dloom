//! Editable Mesh / Edit Mesh commands (triangles whose edges can be
//! invisible), the sub-object levels of Editable Mesh and Editable Patch,
//! their rollout settings, Convert To Editable Mesh / Patch and the Patch
//! Grids.

use crate::command::Command;
use crate::execute::{add_object, commit, detach_instance, replace_poly_change, selection_json};
use crate::state::{AppState, SelectionMode, SubObjectId, MESH_LEVELS, PATCH_LEVELS};
use glam::Vec3;
use loom_mesh::tri_mesh;
use loom_mesh::{FaceId, HalfEdgeId, Mesh};
use loom_scene::{Change, Modifier, Node, NodeId, ObjectKind, PolyObject};
use serde_json::{json, Value};

pub fn handles(c: &Command) -> bool {
    use Command as C;
    matches!(
        c,
        C::ConvertToEditableMesh
            | C::ConvertToEditablePatch
            | C::CreatePatchGrid { .. }
            | C::SubObjectLevel { .. }
            | C::MeshPatchSetting { .. }
            | C::MeshEdgeVisibility { .. }
            | C::MeshAutoEdge
            | C::MeshTurnEdge { .. }
            | C::MeshDivide
            | C::MeshExplode { .. }
            | C::MeshTessellate
            | C::MeshExtrude { .. }
            | C::MeshBevel { .. }
            | C::MeshFlipNormals
            | C::MeshUnifyNormals
    )
}

pub fn execute(state: &mut AppState, cmd: Command) -> Result<Value, String> {
    use Command as C;
    match cmd {
        C::ConvertToEditableMesh => convert(state, "Convert to Editable Mesh", |node, mesh, _| {
            let tri = tri_mesh::from_poly(&mesh)?;
            Ok(match &node.object {
                ObjectKind::EditableMesh(p) => ObjectKind::EditableMesh(p.with_mesh(tri)),
                _ => ObjectKind::EditableMesh(PolyObject::from_mesh(tri)),
            })
        }),
        C::ConvertToEditablePatch => convert(state, "Convert to Editable Patch", |node, mesh, patch| {
            let lattice = match patch {
                Some(p) => p,
                None => loom_mesh::patch::PatchMesh::from_mesh(&if mesh.has_invisible_edges() { tri_mesh::to_poly(&mesh)? } else { mesh }),
            };
            Ok(node.object.with_patch(lattice))
        }),
        C::CreatePatchGrid { quad, position, length, width } => {
            let object = if quad { ObjectKind::quad_patch_object(length, width) } else { ObjectKind::tri_patch_object(length, width) };
            let id = add_object(state, object, Vec3::from(position))?;
            state.selection_mode = SelectionMode::Object;
            Ok(json!({ "id": id.0, "name": state.scene.node(id).map(|n| n.name.clone()) }))
        }
        C::SubObjectLevel { level } => sub_object_level(state, level),
        C::MeshPatchSetting { name, value } => setting(state, &name, value),
        C::MeshEdgeVisibility { visible } => edit_edges(state, if visible { "Visible" } else { "Invisible" }, |m, es, _| Ok(tri_mesh::set_visibility(m, es, visible))),
        C::MeshAutoEdge => {
            let (t, mode) = (state.mesh_patch.auto_edge_threshold, state.mesh_patch.auto_edge_mode);
            edit_edges(state, "Auto Edge", |m, es, _| Ok(tri_mesh::auto_edge(m, es, t, mode)))
        }
        C::MeshTurnEdge { edges } => {
            let given: Vec<HalfEdgeId> = edges.into_iter().map(HalfEdgeId).collect();
            let r = edit_edges_or(state, "Turn", given, |m, es| {
                let mut cur = m.clone();
                for (a, b) in edge_positions(m, es) {
                    let e = edge_at(&cur, a, b).ok_or("Turn: the edge is gone")?;
                    cur = tri_mesh::turn_edge(&cur, e)?;
                }
                Ok(cur)
            });
            state.sub_selection.clear();
            r
        }
        C::MeshDivide => {
            let r = if state.selection_mode.is_edge_level() {
                edit_edges(state, "Divide", |m, es, _| {
                    let mut cur = m.clone();
                    for (a, b) in edge_positions(m, es) {
                        let e = edge_at(&cur, a, b).ok_or("Divide: the edge is gone")?;
                        cur = tri_mesh::divide_edge(&cur, e, 0.5)?.0;
                    }
                    Ok(cur)
                })
            } else {
                edit_faces(state, "Divide", |m, fs| {
                    let mut cur = m.clone();
                    for f in fs {
                        cur = tri_mesh::divide_face(&cur, *f)?;
                    }
                    Ok((cur, Vec::new()))
                })
            };
            state.sub_selection.clear();
            r
        }
        C::MeshExplode { name } => explode(state, name),
        C::MeshTessellate => {
            let (t, fc) = (state.mesh_patch.tension, state.mesh_patch.face_center);
            edit_faces(state, "Tessellate", |m, fs| tri_mesh::tessellate(m, fs, t, fc))
        }
        C::MeshExtrude { amount } => {
            state.mesh_patch.extrude = amount;
            edit_faces(state, "Extrude", |m, fs| Ok((tri_mesh::extrude(m, fs, amount)?, fs.to_vec())))
        }
        C::MeshBevel { height, outline } => {
            state.mesh_patch.extrude = height;
            state.mesh_patch.outline = outline;
            edit_faces(state, "Bevel", |m, fs| {
                let e = tri_mesh::extrude(m, fs, height)?;
                Ok((tri_mesh::outline(&e, fs, outline)?, fs.to_vec()))
            })
        }
        C::MeshFlipNormals => edit_faces(state, "Flip Normals", |m, fs| Ok((tri_mesh::flip(m, fs)?, fs.to_vec()))),
        C::MeshUnifyNormals => edit_faces(state, "Unify Normals", |m, fs| Ok((tri_mesh::unify(m, fs)?, fs.to_vec()))),
        other => Err(format!("not an Editable Mesh command: {}", other.name())),
    }
}

/// Convert To: each selected object (its whole stack collapsed) becomes
/// what `make` builds from the stack's mesh and patches.
fn convert(
    state: &mut AppState,
    label: &str,
    make: impl Fn(&Node, Mesh, Option<loom_mesh::patch::PatchMesh>) -> Result<ObjectKind, String>,
) -> Result<Value, String> {
    let ids: Vec<NodeId> = state
        .selection
        .iter()
        .copied()
        .filter(|id| state.scene.node(*id).is_some_and(|n| !matches!(n.object, ObjectKind::Camera { .. } | ObjectKind::Light(_) | ObjectKind::Group { .. } | ObjectKind::Dummy { .. } | ObjectKind::Point { .. } | ObjectKind::IkGoal { .. } | ObjectKind::Biped(_))))
        .collect();
    if ids.is_empty() {
        return Err(format!("{label}: select an object first"));
    }
    state.undo.begin(label);
    for id in &ids {
        if let Err(e) = detach_instance(state, *id) {
            state.undo.end();
            return Err(e);
        }
    }
    let mut changes = Vec::new();
    let mut err = None;
    for id in ids {
        let Some(node) = state.scene.node(id) else { continue };
        let (mesh, patch) = if node.modifiers.is_empty() {
            (node.object.display_output(), node.object.patch_mesh())
        } else {
            let e = state.scene.eval_mesh(id);
            (e.value.mesh.clone(), e.value.patch.clone())
        };
        match make(node, mesh, patch) {
            Ok(after) => {
                changes.push(Change::ReplaceObject { id, before: Box::new(node.object.clone()), after: Box::new(after) });
                if !node.modifiers.is_empty() {
                    changes.push(Change::SetStack { id, before: node.modifiers.clone(), after: Vec::new() });
                }
            }
            Err(e) => err = Some(e),
        }
    }
    let r = if changes.is_empty() { Err(err.unwrap_or_else(|| format!("{label}: nothing to convert"))) } else { commit(state, label, Change::Group(changes)) };
    state.undo.end();
    r?;
    state.sub_selection.clear();
    state.selection_mode = SelectionMode::Object;
    state.mesh_patch.polygon_level = false;
    state.mesh_patch.handle_level = false;
    Ok(json!({}))
}

/// The level list of the selected object: (mesh levels?, patch levels?).
fn level_kind(state: &AppState) -> Option<(NodeId, bool)> {
    state.selection.iter().find_map(|id| {
        let n = state.scene.node(*id)?;
        if n.edit_patch().is_some() {
            Some((*id, true))
        } else if n.edits_tri_mesh() {
            Some((*id, false))
        } else {
            None
        }
    })
}

/// An Editable Mesh / Patch (or an active Edit Mesh / Edit Patch) is selected.
pub fn mesh_or_patch_selected(state: &AppState) -> bool {
    level_kind(state).is_some()
}

/// The 1-based level of the selected Editable Mesh / Patch (0 = top).
pub fn current_level(state: &AppState) -> u32 {
    let patch = level_kind(state).is_some_and(|(_, p)| p);
    let m = &state.mesh_patch;
    match state.selection_mode {
        SelectionMode::Object => 0,
        SelectionMode::Vertex => if patch && m.handle_level { 2 } else { 1 },
        SelectionMode::Edge | SelectionMode::Border => if patch { 3 } else { 2 },
        SelectionMode::Face => if patch || m.polygon_level { 4 } else { 3 },
        SelectionMode::Element => 5,
    }
}

fn sub_object_level(state: &mut AppState, level: u32) -> Result<Value, String> {
    let Some(&first) = state.selection.iter().next() else { return Err("select an object first".into()) };
    let node = state.scene.node(first).ok_or("missing node")?;
    // An active stack entry with levels takes the level itself.
    if let Some(a) = node.active_entry {
        if !node.modifiers[a].modifier.sub_levels().is_empty() {
            return crate::execute(state, Command::ModifierSubobjectLevel { index: a as u32, level });
        }
    }
    let levels: Vec<(&'static str, SelectionMode)> = match &node.object {
        ObjectKind::EditableMesh(_) => MESH_LEVELS.to_vec(),
        ObjectKind::EditablePatch(_) => PATCH_LEVELS.to_vec(),
        other => crate::state::base_sub_levels(other),
    };
    let is_mesh = matches!(node.object, ObjectKind::EditableMesh(_));
    let is_patch = matches!(node.object, ObjectKind::EditablePatch(_));
    let mode = match level {
        0 => SelectionMode::Object,
        l => levels.get(l as usize - 1).map(|(_, m)| *m).ok_or_else(|| format!("{} has no sub-object level {l}", node.object.stack_name()))?,
    };
    crate::execute(state, Command::SelectionMode { mode })?;
    state.mesh_patch.polygon_level = is_mesh && level == 4;
    state.mesh_patch.handle_level = is_patch && level == 2;
    Ok(json!({ "level": level, "mode": state.level_label(state.selection_mode) }))
}

fn setting(state: &mut AppState, name: &str, value: f64) -> Result<Value, String> {
    let s = &mut state.mesh_patch;
    let v = value as f32;
    let b = value != 0.0;
    match name {
        "Ignore Visible Edges" => s.ignore_visible_edges = b,
        "Planar Thresh" => s.planar_thresh = v.clamp(0.0, 180.0),
        "Auto Edge Threshold" => s.auto_edge_threshold = v.clamp(0.0, 180.0),
        "Auto Edge Mode" => {
            s.auto_edge_mode = match value.round() as i64 {
                1 => tri_mesh::AutoEdgeMode::Set,
                2 => tri_mesh::AutoEdgeMode::Clear,
                _ => tri_mesh::AutoEdgeMode::SetAndClear,
            }
        }
        "Explode Angle" => s.explode_angle = v.clamp(0.0, 180.0),
        "Explode To Objects" => s.explode_to_objects = b,
        "Tension" => s.tension = v.clamp(-100.0, 100.0),
        "Face-Center" => s.face_center = b,
        "Extrude" => s.extrude = v,
        "Outline" => s.outline = v,
        "Patch Extrude" => s.patch_extrude = v,
        "Patch Outline" => s.patch_outline = v,
        "Patch Weld Threshold" => s.patch_weld_threshold = v.max(0.0),
        "Relax Value" => s.relax_amount = v.clamp(-1.0, 1.0),
        "Relax Iterations" => s.relax_iterations = (value.round().max(0.0) as u32).min(1000),
        "Keep Boundary Points Fixed" => s.relax_keep_boundary = b,
        _ => return Err(format!("no Editable Mesh / Patch setting {name:?}")),
    }
    Ok(json!({ "settings": serde_json::to_value(&state.mesh_patch).unwrap_or_default() }))
}

/// Selected nodes editing a triangle mesh, with their mesh.
fn tri_targets(state: &AppState) -> Vec<(NodeId, Mesh)> {
    state.selection.iter().filter_map(|id| state.scene.node(*id).filter(|n| n.edits_tri_mesh()).and_then(|n| n.edit_poly()).map(|p| (*id, p.mesh.clone()))).collect()
}

/// Commits new meshes for nodes editing a triangle mesh as one step.
fn commit_meshes(state: &mut AppState, label: &str, results: Vec<(NodeId, Mesh)>) -> Result<(), String> {
    let mut changes = Vec::new();
    for (id, mesh) in results {
        let node = state.scene.node(id).ok_or("missing node")?;
        let Some(p) = node.edit_poly() else { continue };
        changes.push(replace_poly_change(node, p.with_mesh(mesh)));
    }
    if changes.is_empty() {
        return Err(format!("{label}: select an Editable Mesh"));
    }
    commit(state, label, Change::Group(changes))
}

fn selected_of(state: &AppState, id: NodeId) -> (Vec<HalfEdgeId>, Vec<FaceId>) {
    let mut es = Vec::new();
    let mut fs = Vec::new();
    for (n, s) in state.sub_selection.iter() {
        if n != id {
            continue;
        }
        match s {
            SubObjectId::Edge(e) => es.push(e),
            SubObjectId::Face(f) => fs.push(f),
            SubObjectId::Vertex(_) => {}
        }
    }
    (es, fs)
}

fn edit_edges(state: &mut AppState, label: &str, mut f: impl FnMut(&Mesh, &[HalfEdgeId], NodeId) -> Result<Mesh, String>) -> Result<Value, String> {
    let mut results = Vec::new();
    for (id, mesh) in tri_targets(state) {
        let (es, _) = selected_of(state, id);
        if es.is_empty() {
            continue;
        }
        results.push((id, f(&mesh, &es, id)?));
    }
    if results.is_empty() {
        return Err(format!("{label}: select edges of an Editable Mesh first"));
    }
    commit_meshes(state, label, results)?;
    Ok(selection_json(state))
}

/// Like `edit_edges`, with explicit edge ids for the first target when given.
fn edit_edges_or(state: &mut AppState, label: &str, given: Vec<HalfEdgeId>, f: impl Fn(&Mesh, &[HalfEdgeId]) -> Result<Mesh, String>) -> Result<Value, String> {
    if given.is_empty() {
        return edit_edges(state, label, |m, es, _| f(m, es));
    }
    let Some((id, mesh)) = tri_targets(state).into_iter().next() else { return Err(format!("{label}: select an Editable Mesh")) };
    let out = f(&mesh, &given)?;
    commit_meshes(state, label, vec![(id, out)])?;
    Ok(selection_json(state))
}

/// A face operation on the selected faces; the returned faces end selected.
fn edit_faces(state: &mut AppState, label: &str, mut f: impl FnMut(&Mesh, &[FaceId]) -> Result<(Mesh, Vec<FaceId>), String>) -> Result<Value, String> {
    let mut results = Vec::new();
    let mut keep = Vec::new();
    for (id, mesh) in tri_targets(state) {
        let (_, fs) = selected_of(state, id);
        if fs.is_empty() {
            continue;
        }
        let (out, sel) = f(&mesh, &fs)?;
        results.push((id, out));
        keep.extend(sel.into_iter().map(|s| (id, s)));
    }
    if results.is_empty() {
        return Err(format!("{label}: select faces of an Editable Mesh first"));
    }
    commit_meshes(state, label, results)?;
    if !keep.is_empty() {
        state.sub_selection.clear();
        for (n, f) in keep {
            state.sub_selection.insert(n, SubObjectId::Face(f));
        }
    }
    Ok(selection_json(state))
}

fn edge_positions(m: &Mesh, es: &[HalfEdgeId]) -> Vec<(Vec3, Vec3)> {
    es.iter().filter_map(|e| m.edge_vertices(*e)).filter_map(|(a, b)| Some((m.vertex_position(a)?, m.vertex_position(b)?))).collect()
}

/// The half-edge from `a` to `b` (either direction), by positions.
fn edge_at(m: &Mesh, a: Vec3, b: Vec3) -> Option<HalfEdgeId> {
    (0..m.half_edge_count() as u32).map(HalfEdgeId).find(|h| {
        m.edge_vertices(*h).and_then(|(x, y)| Some((m.vertex_position(x)?, m.vertex_position(y)?))).is_some_and(|(x, y)| (x == a && y == b) || (x == b && y == a))
            && m.half_edges()[h.0 as usize].face.is_some()
    })
}

/// Explode: the selected faces (every face when none) split by angle into
/// new objects or elements.
fn explode(state: &mut AppState, name: Option<String>) -> Result<Value, String> {
    let angle = state.mesh_patch.explode_angle;
    let to_objects = state.mesh_patch.explode_to_objects;
    let mut changes = Vec::new();
    let mut taken: std::collections::BTreeSet<String> = state.scene.nodes().map(|n| n.name.clone()).collect();
    for (id, mesh) in tri_targets(state) {
        let (_, mut fs) = selected_of(state, id);
        if fs.is_empty() {
            fs = (0..mesh.face_count() as u32).map(FaceId).collect();
        }
        let node = state.scene.node(id).ok_or("missing node")?;
        let p = node.edit_poly().ok_or("not an Editable Mesh")?;
        if !to_objects {
            changes.push(replace_poly_change(node, p.with_mesh(tri_mesh::explode_to_elements(&mesh, &fs, angle)?)));
            continue;
        }
        let groups = tri_mesh::explode_groups(&mesh, &fs, angle);
        let (rest, _) = tri_mesh::split(&mesh, &fs)?;
        changes.push(replace_poly_change(node, p.with_mesh(rest)));
        let prefix = name.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| "Object".to_owned());
        for g in &groups {
            let (_, part) = tri_mesh::split(&mesh, g)?;
            let nm = crate::execute::unique_name(&prefix, &taken);
            taken.insert(nm.clone());
            let mut n = Node::new(&nm, ObjectKind::EditableMesh(PolyObject::from_mesh(part)));
            n.transform = node.transform.clone();
            n.object_offset = node.object_offset.clone();
            n.parent = node.parent;
            n.layer = node.layer;
            n.wire_color = node.wire_color;
            changes.push(Change::AddNode(n));
        }
    }
    if changes.is_empty() {
        return Err("Explode: select an Editable Mesh".into());
    }
    commit(state, "Explode", Change::Group(changes))?;
    state.sub_selection.clear();
    Ok(selection_json(state))
}

/// Polygon level picking: the faces a click on `face` selects.
pub fn polygon_pick(state: &AppState, node: &Node, mesh: &Mesh, face: FaceId) -> Option<Vec<FaceId>> {
    if !(state.mesh_patch.polygon_level && node.edits_tri_mesh()) {
        return None;
    }
    let planar = state.mesh_patch.ignore_visible_edges.then_some(state.mesh_patch.planar_thresh);
    Some(tri_mesh::polygon_of(mesh, face, planar))
}

/// Edit Mesh at a level: which selection mode and flag each level is.
pub fn modifier_level(modifier: &Modifier, level: u32) -> Option<(SelectionMode, bool, bool)> {
    let pick = |table: &[(&str, SelectionMode); 5]| table.get(level.checked_sub(1)? as usize).map(|(_, m)| *m);
    match modifier {
        Modifier::EditMesh { .. } => Some((if level == 0 { SelectionMode::Object } else { pick(&MESH_LEVELS)? }, level == 4, false)),
        Modifier::EditPatch { .. } => Some((if level == 0 { SelectionMode::Object } else { pick(&PATCH_LEVELS)? }, false, level == 2)),
        _ => None,
    }
}
