//! Editable Patch / Edit Patch commands. Sub-objects are picked on the
//! patch lattice (`Node::cage_mesh`): vertex ids below the vertex count are
//! the patch vertices, the ones above are the handles; faces are patches.

use crate::command::{Command, SelectOp};
use crate::execute::{commit, selection_json};
use crate::state::{AppState, SelectionMode, SubObjectId};
use loom_mesh::patch::PatchMesh;
use loom_mesh::{FaceId, HalfEdgeId, VertexId};
use loom_scene::{Change, Node, NodeId, ObjectKind, PatchObject};
use serde_json::Value;
use std::collections::BTreeSet;

pub fn handles(c: &Command) -> bool {
    use Command as C;
    matches!(c, C::PatchAddTri | C::PatchAddQuad | C::PatchWeld | C::PatchDelete | C::PatchExtrude { .. } | C::PatchRelax | C::PatchFlipNormals)
}

pub fn execute(state: &mut AppState, cmd: Command) -> Result<Value, String> {
    use Command as C;
    match cmd {
        C::PatchAddTri | C::PatchAddQuad => {
            let quad = matches!(cmd, C::PatchAddQuad);
            let r = edit(state, if quad { "Add Quad" } else { "Add Tri" }, |pm, sel| {
                if sel.edges.is_empty() {
                    return Err("select open edges first".into());
                }
                for e in &sel.edges {
                    pm.add_on_edge(*e, quad)?;
                }
                Ok(())
            });
            state.sub_selection.clear();
            r
        }
        C::PatchWeld => {
            let t = state.mesh_patch.patch_weld_threshold;
            let r = edit(state, "Weld", |pm, sel| {
                if pm.weld(&sel.verts, t) == 0 {
                    return Err("Weld: no selected vertices are within the threshold".into());
                }
                Ok(())
            });
            state.sub_selection.clear();
            r
        }
        C::PatchDelete => delete_selection(state),
        C::PatchExtrude { amount, outline } => {
            state.mesh_patch.patch_extrude = amount;
            state.mesh_patch.patch_outline = outline;
            edit(state, "Extrude", |pm, sel| {
                if sel.patches.is_empty() {
                    return Err("select patches first".into());
                }
                pm.extrude(&sel.patches, amount, outline)
            })
        }
        C::PatchRelax => {
            let (a, it, keep) = (state.mesh_patch.relax_amount, state.mesh_patch.relax_iterations, state.mesh_patch.relax_keep_boundary);
            edit(state, "Relax", |pm, sel| {
                let verts: Vec<u32> = if sel.verts.is_empty() { (0..pm.verts.len() as u32).collect() } else { sel.verts.clone() };
                pm.relax(&verts, a, it, keep);
                Ok(())
            })
        }
        C::PatchFlipNormals => edit(state, "Flip Normals", |pm, sel| {
            if sel.patches.is_empty() {
                return Err("select patches first".into());
            }
            pm.flip(&sel.patches);
            Ok(())
        }),
        other => Err(format!("not an Editable Patch command: {}", other.name())),
    }
}

/// The selection resolved on the lattice.
#[derive(Default)]
struct PatchSel {
    verts: Vec<u32>,
    edges: Vec<usize>,
    patches: Vec<usize>,
}

fn resolve(node: &Node, pm: &PatchMesh, ids: &BTreeSet<SubObjectId>) -> PatchSel {
    let cage = node.cage_mesh();
    let nv = pm.verts.len() as u32;
    let mut s = PatchSel::default();
    for id in ids {
        match id {
            SubObjectId::Vertex(v) if v.0 < nv => s.verts.push(v.0),
            SubObjectId::Vertex(_) => {}
            SubObjectId::Edge(e) => {
                if let Some((a, b)) = cage.edge_vertices(*e) {
                    if let Some((i, _)) = pm.edge_index(a.0, b.0) {
                        if !s.edges.contains(&i) {
                            s.edges.push(i);
                        }
                    }
                }
            }
            SubObjectId::Face(f) if (f.0 as usize) < pm.patches.len() => s.patches.push(f.0 as usize),
            SubObjectId::Face(_) => {}
        }
    }
    s
}

/// The change that stores an edited patch object: the active Edit Patch
/// entry's, else the base object's.
pub(crate) fn replace_patch_change(node: &Node, patch: PatchObject) -> Change {
    match node.editing_patch_entry() {
        Some(a) => {
            let mut after = node.modifiers.clone();
            if let Some(p) = after[a].modifier.edit_patch_mut() {
                *p = patch;
            }
            Change::SetStack { id: node.id, before: node.modifiers.clone(), after }
        }
        None => Change::ReplaceObject { id: node.id, before: Box::new(node.object.clone()), after: Box::new(ObjectKind::EditablePatch(patch)) },
    }
}

fn edit(state: &mut AppState, label: &str, mut f: impl FnMut(&mut PatchMesh, &PatchSel) -> Result<(), String>) -> Result<Value, String> {
    let ids: Vec<NodeId> = state.selection.iter().copied().filter(|id| state.scene.node(*id).is_some_and(|n| n.edit_patch().is_some())).collect();
    if ids.is_empty() {
        return Err(format!("{label}: select an Editable Patch"));
    }
    let mut changes = Vec::new();
    for id in ids {
        let node = state.scene.node(id).ok_or("missing node")?;
        let obj = node.edit_patch().ok_or("not a patch")?.clone();
        let sel_ids: BTreeSet<SubObjectId> = state.sub_selection.iter().filter(|(n, _)| *n == id).map(|(_, s)| s).collect();
        let sel = resolve(node, &obj.patch, &sel_ids);
        let mut pm = obj.patch.clone();
        f(&mut pm, &sel)?;
        changes.push(replace_patch_change(node, obj.with_patch(pm)));
    }
    commit(state, label, Change::Group(changes))?;
    Ok(selection_json(state))
}

/// Delete at a patch level: the patches of the selected vertices, edges or patches.
pub fn delete_selection(state: &mut AppState) -> Result<Value, String> {
    let r = edit(state, "Delete", |pm, sel| {
        let mut del: BTreeSet<usize> = sel.patches.iter().copied().collect();
        for e in &sel.edges {
            let [a, b] = pm.edges[*e].v;
            for (i, p) in pm.patches.iter().enumerate() {
                let n = p.v.len();
                if (0..n).any(|k| (p.v[k], p.v[(k + 1) % n]) == (a, b) || (p.v[k], p.v[(k + 1) % n]) == (b, a)) {
                    del.insert(i);
                }
            }
        }
        if !sel.verts.is_empty() {
            let vs: BTreeSet<u32> = sel.verts.iter().copied().collect();
            for (i, p) in pm.patches.iter().enumerate() {
                if p.v.iter().any(|v| vs.contains(v)) {
                    del.insert(i);
                }
            }
        }
        if del.is_empty() {
            return Err("select sub-objects to delete".into());
        }
        pm.delete_patches(&del.into_iter().collect::<Vec<_>>());
        Ok(())
    });
    state.sub_selection.clear();
    r
}

/// Selecting on a patch: Vertex level takes vertices, Handle level handles,
/// Edge edges, Patch patches, Element the patches joined to the clicked one.
pub fn select_sub(state: &mut AppState, node: NodeId, ids: &[u32], op: SelectOp) -> Result<Value, String> {
    let n = state.scene.node(node).ok_or("missing node")?;
    let pm = n.edit_patch().ok_or("not a patch")?.patch.clone();
    let cage = n.cage_mesh().into_owned();
    let nv = pm.verts.len() as u32;
    let handles = state.mesh_patch.handle_level;
    if op == SelectOp::Replace {
        state.sub_selection.clear();
    }
    for raw in ids {
        let picked: Vec<SubObjectId> = match state.selection_mode {
            SelectionMode::Vertex if handles => (*raw >= nv && (*raw as usize) < cage.vertex_count()).then_some(SubObjectId::Vertex(VertexId(*raw))).into_iter().collect(),
            SelectionMode::Vertex => (*raw < nv).then_some(SubObjectId::Vertex(VertexId(*raw))).into_iter().collect(),
            SelectionMode::Edge | SelectionMode::Border => cage.canonical_edge_id(HalfEdgeId(*raw)).map(SubObjectId::Edge).into_iter().collect(),
            SelectionMode::Face => ((*raw as usize) < pm.patches.len()).then_some(SubObjectId::Face(FaceId(*raw))).into_iter().collect(),
            SelectionMode::Element if (*raw as usize) < pm.patches.len() => pm.element_of(*raw as usize).into_iter().map(|p| SubObjectId::Face(FaceId(p as u32))).collect(),
            _ => Vec::new(),
        };
        for id in picked {
            if op == SelectOp::Subtract {
                state.sub_selection.remove(node, id);
            } else {
                state.sub_selection.insert(node, id);
            }
        }
    }
    Ok(selection_json(state))
}
