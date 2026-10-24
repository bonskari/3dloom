//! Skin modifier commands: the "Skin" action group (bones, envelopes,
//! Weight Tool / Weight Table, selection tools, skin pose) and the Loom
//! calls behind the rollouts.

use crate::execute::commit;
use crate::state::{AppState, DialogPurpose, SelectionMode};
use crate::viewport::Dialog;
use crate::{Command, SelectOp};
use glam::Vec3;
use loom_mesh::select_ops::{self, SubLevel};
use loom_mesh::{HalfEdgeId, Mesh, VertexId};
use loom_scene::skin::{normalize, Envelope, SkinData};
use loom_scene::{Change, Modifier, NodeId, Transform};
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub struct Entry {
    pub id: NodeId,
    pub index: usize,
    pub data: SkinData,
    /// The mesh below the Skin (what the weights index).
    pub input: Mesh,
}

pub fn entry(state: &AppState) -> Result<Entry, String> {
    let (id, index) = state.skin_entry().or_else(|| skin_of_selected_bone(state)).ok_or("no Skin on the selection (add the modifier first)")?;
    let node = state.scene.node(id).ok_or("missing node")?;
    let Modifier::Skin(data) = &node.modifiers[index].modifier else { return Err("not a Skin entry".into()) };
    let input = match index.checked_sub(1) {
        Some(prev) => state.scene.evaluated(id, Some(prev)).value.mesh.clone(),
        None => node.object.display_output(),
    };
    Ok(Entry { id, index, data: (**data).clone(), input })
}

/// With a bone selected: the Skin that lists it (Set / Assume Skin Pose from a bone).
fn skin_of_selected_bone(state: &AppState) -> Option<(NodeId, usize)> {
    state.scene.nodes().find_map(|n| {
        n.modifiers.iter().position(|s| matches!(&s.modifier, Modifier::Skin(d) if d.bones.iter().any(|b| state.selection.contains(&b.node)))).map(|i| (n.id, i))
    })
}

/// Edits the Skin data as one undo entry.
fn edit(state: &mut AppState, label: &str, f: impl FnOnce(&mut SkinData, &Mesh, &AppState) -> Result<(), String>) -> Result<Value, String> {
    let e = entry(state)?;
    let mut data = e.data;
    data.resize(e.input.vertex_count());
    f(&mut data, &e.input, state)?;
    let node = state.scene.node(e.id).ok_or("missing node")?;
    let mut stack = node.modifiers.clone();
    stack[e.index].modifier = Modifier::Skin(Box::new(data));
    commit(state, label, Change::SetStack { id: e.id, before: node.modifiers.clone(), after: stack })?;
    Ok(status(state))
}

fn current_bone_index(state: &AppState, data: &SkinData) -> Result<usize, String> {
    let b = state.skin_bone.or_else(|| data.bones.first().map(|b| b.node)).ok_or("the Skin has no bones")?;
    data.bones.iter().position(|x| x.node == b).ok_or_else(|| "the current bone is not in the Skin's list".to_string())
}

fn selected_verts(state: &AppState, n: usize) -> Vec<usize> {
    selection(state).into_iter().map(|v| v as usize).filter(|v| *v < n).collect()
}

/// The selected vertices of the current Skin entry: empty when the selection
/// was made on another Skin, and only ids the mesh below the Skin has.
pub fn selection(state: &AppState) -> Vec<u32> {
    let Ok(e) = entry(state) else { return Vec::new() };
    if state.skin_selection_owner != Some((e.id, e.index)) {
        return Vec::new();
    }
    let n = e.input.vertex_count() as u32;
    state.skin_selection.iter().copied().filter(|v| *v < n).collect()
}

/// The weights a vertex has now (hand-set or from the envelopes).
fn current_weights(data: &SkinData, mesh: &Mesh, v: usize) -> Vec<(u32, f32)> {
    let p = data.mesh_bind.transform_point3(mesh.vertices()[v].position);
    data.vertex_weights(v, p)
}

fn weight_of(w: &[(u32, f32)], bone: u32) -> f32 {
    w.iter().find(|(b, _)| *b == bone).map(|x| x.1).unwrap_or(0.0)
}

/// Weight Tool: set / add / scale the current bone's weight on the selection.
fn weight_edit(state: &mut AppState, label: &str, f: impl Fn(f32, f32) -> f32) -> Result<Value, String> {
    let custom = state.skin_custom_weight;
    edit(state, label, |data, mesh, st| {
        let bone = current_bone_index(st, data)? as u32;
        let verts = selected_verts(st, mesh.vertex_count());
        if verts.is_empty() {
            return Err("select vertices first (Edit Envelopes)".into());
        }
        for v in verts {
            let cur = current_weights(data, mesh, v);
            let w = f(weight_of(&cur, bone), custom);
            data.set_weight(v, bone, w, &cur);
        }
        Ok(())
    })
}

fn set_selection(state: &mut AppState, ids: impl IntoIterator<Item = u32>, op: SelectOp) {
    let ids: BTreeSet<u32> = ids.into_iter().collect();
    let owner = entry(state).ok().map(|e| (e.id, e.index));
    if owner != state.skin_selection_owner {
        state.skin_selection.clear();
        state.skin_selection_owner = owner;
    }
    match op {
        SelectOp::Replace => state.skin_selection = ids,
        SelectOp::Add => state.skin_selection.extend(ids),
        SelectOp::Subtract => state.skin_selection.retain(|v| !ids.contains(v)),
    }
}

fn map_selection(state: &mut AppState, f: impl Fn(&Mesh, Vec<u32>) -> Vec<u32>) -> Result<Value, String> {
    let e = entry(state)?;
    let ids: Vec<u32> = selection(state);
    let new = f(&e.input, ids);
    set_selection(state, new, SelectOp::Replace);
    Ok(status(state))
}

/// Edges whose both ends are selected, or every edge at a lone vertex.
fn selected_edges(mesh: &Mesh, ids: &[u32]) -> Vec<HalfEdgeId> {
    let set: BTreeSet<u32> = ids.iter().copied().collect();
    let mut out: Vec<HalfEdgeId> = Vec::new();
    for v in ids {
        for he in select_ops::vertex_edges(mesh, VertexId(*v)) {
            let Some((a, b)) = mesh.edge_vertices(he) else { continue };
            if (set.contains(&a.0) && set.contains(&b.0)) || ids.len() == 1 {
                out.push(he);
            }
        }
    }
    out
}

fn edges_to_vertices(mesh: &Mesh, edges: &[HalfEdgeId]) -> Vec<u32> {
    let mut out = BTreeSet::new();
    for he in edges {
        if let Some((a, b)) = mesh.edge_vertices(*he) {
            out.insert(a.0);
            out.insert(b.0);
        }
    }
    out.into_iter().collect()
}

fn bone_relative(state: &mut AppState, pick: impl Fn(&AppState, &SkinData, usize) -> Option<usize>) -> Result<Value, String> {
    let e = entry(state)?;
    let cur = current_bone_index(state, &e.data)?;
    if let Some(i) = pick(state, &e.data, cur) {
        state.skin_bone = Some(e.data.bones[i].node);
    }
    Ok(status(state))
}

pub fn execute(state: &mut AppState, cmd: Command) -> Result<Value, String> {
    use Command as C;
    match cmd {
        C::SkinModifier => crate::execute(state, C::AddModifier { name: "Skin".into() }),
        C::SkinMorphModifier => crate::execute(state, C::AddModifier { name: "Skin Morph".into() }),
        C::SkinWrapModifier => crate::execute(state, C::AddModifier { name: "Skin Wrap".into() }),
        C::AddBonesSkin => {
            entry(state)?;
            state.dialog_purpose = DialogPurpose::SkinAddBones;
            state.dialog = Some(Dialog::SelectFromScene);
            Ok(json!({ "dialog": "SelectFromScene" }))
        }
        C::SkinAddBones { ids } => {
            let ids: Vec<NodeId> = ids.into_iter().map(NodeId).collect();
            for id in &ids {
                state.scene.node(*id).ok_or_else(|| format!("no node {}", id.0))?;
            }
            let r = edit(state, "Add Bones", |data, mesh, st| {
                if data.bones.is_empty() {
                    // First bind: the object's pose now is the skin pose.
                    let (id, _) = st.skin_entry().ok_or("no Skin")?;
                    data.mesh_bind = st.scene.object_matrix(id);
                }
                for id in &ids {
                    data.add_bone(&st.scene, *id);
                }
                data.refresh_envelope_weights(mesh);
                Ok(())
            })?;
            if state.skin_bone.is_none() {
                state.skin_bone = ids.first().copied();
            }
            Ok(r)
        }
        C::RemoveMultipleBonesSkin => {
            let e = entry(state)?;
            let i = current_bone_index(state, &e.data)?;
            crate::execute(state, C::SkinRemoveBone { id: e.data.bones[i].node.0 })
        }
        C::SkinRemoveBone { id } => {
            let id = NodeId(id);
            let r = edit(state, "Remove Bone", |data, mesh, _| {
                let i = data.bones.iter().position(|b| b.node == id).ok_or("the bone is not in the Skin")?;
                data.remove_bone(i);
                data.refresh_envelope_weights(mesh);
                Ok(())
            })?;
            if state.skin_bone == Some(id) {
                state.skin_bone = entry(state).ok().and_then(|e| e.data.bones.first().map(|b| b.node));
            }
            Ok(r)
        }
        C::SkinSelectBone { id } => {
            let e = entry(state)?;
            let id = NodeId(id);
            if !e.data.bones.iter().any(|b| b.node == id) {
                return Err("the bone is not in the Skin's list".into());
            }
            state.skin_bone = Some(id);
            state.skin_gizmo = None;
            Ok(status(state))
        }
        C::SkinSelectVertices { ids, op } => {
            let e = entry(state)?;
            let n = e.input.vertex_count() as u32;
            if let Some(bad) = ids.iter().find(|v| **v >= n) {
                return Err(format!("no vertex {bad} (the mesh has {n})"));
            }
            set_selection(state, ids, op);
            Ok(status(state))
        }
        C::SkinSetVertexWeight { weight, absolute } => weight_edit(state, "Set Weight", move |cur, _| if absolute { weight } else { cur + weight }),
        C::SkinSetCustomWeight { value } => {
            state.skin_custom_weight = value.clamp(0.0, 1.0);
            Ok(status(state))
        }
        C::SetWeightTo00Skin => weight_edit(state, "Set Weight", |_, _| 0.0),
        C::SetWeightTo010Skin => weight_edit(state, "Set Weight", |_, _| 0.1),
        C::SetWeightTo025Skin => weight_edit(state, "Set Weight", |_, _| 0.25),
        C::SetWeightTo05Skin => weight_edit(state, "Set Weight", |_, _| 0.5),
        C::SetWeightTo075Skin => weight_edit(state, "Set Weight", |_, _| 0.75),
        C::SetWeightTo090Skin => weight_edit(state, "Set Weight", |_, _| 0.9),
        C::SetWeightTo10Skin => weight_edit(state, "Set Weight", |_, _| 1.0),
        C::SetWeightCustomSkin => weight_edit(state, "Set Weight", |_, c| c),
        C::AddWeightSkin => weight_edit(state, "Add Weight", |w, _| w + 0.05),
        C::SubtractWeightSkin => weight_edit(state, "Subtract Weight", |w, _| w - 0.05),
        C::ScaleWeightUpSkin => weight_edit(state, "Scale Weight", |w, _| w * 1.1),
        C::ScaleWeightDownSkin => weight_edit(state, "Scale Weight", |w, _| w * 0.9),
        C::ScaleWeightCustomSkin => weight_edit(state, "Scale Weight", |w, c| w * c),
        C::BlendWeightsSkin => edit(state, "Blend Weights", |data, mesh, st| {
            let verts = selected_verts(st, mesh.vertex_count());
            if verts.is_empty() {
                return Err("select vertices first".into());
            }
            let before: Vec<Vec<(u32, f32)>> = (0..mesh.vertex_count()).map(|v| current_weights(data, mesh, v)).collect();
            for v in verts {
                let mut acc: std::collections::BTreeMap<u32, f32> = std::collections::BTreeMap::new();
                let mut count = 0.0;
                for nb in select_ops::vertex_neighbours(mesh, VertexId(v as u32)).into_iter().map(|x| x.0 as usize).chain(std::iter::once(v)) {
                    for (b, w) in &before[nb] {
                        *acc.entry(*b).or_insert(0.0) += *w;
                    }
                    count += 1.0;
                }
                let mut w: Vec<(u32, f32)> = acc.into_iter().map(|(b, x)| (b, x / count)).collect();
                normalize(&mut w);
                data.weights[v] = w;
                data.explicit[v] = true;
            }
            Ok(())
        }),
        C::SkinNormalize => edit(state, "Normalize", |data, mesh, st| {
            for v in selected_verts(st, mesh.vertex_count()) {
                let mut w = current_weights(data, mesh, v);
                normalize(&mut w);
                data.weights[v] = w;
                data.explicit[v] = true;
            }
            Ok(())
        }),
        C::SkinRigidToggle => edit(state, "Rigid", |data, mesh, st| {
            let verts = selected_verts(st, mesh.vertex_count());
            let on = !verts.iter().all(|v| data.rigid[*v]);
            for v in verts {
                data.rigid[v] = on;
                if !data.explicit[v] {
                    data.weights[v] = current_weights(data, mesh, v);
                    data.explicit[v] = true;
                }
            }
            Ok(())
        }),
        C::SkinBakeSelectedVerts => edit(state, "Bake Selected Verts", |data, mesh, st| {
            for v in selected_verts(st, mesh.vertex_count()) {
                data.weights[v] = current_weights(data, mesh, v);
                data.explicit[v] = true;
            }
            Ok(())
        }),
        C::SkinSetEnvelope { inner_start, inner_end, outer_start, outer_end, falloff, abs_effect, squash } => edit(state, "Envelope", |data, mesh, st| {
            let i = current_bone_index(st, data)?;
            let e = &mut data.bones[i].envelope;
            if let Some(v) = inner_start { e.inner[0] = v.max(0.0); }
            if let Some(v) = inner_end { e.inner[1] = v.max(0.0); }
            if let Some(v) = outer_start { e.outer[0] = v.max(0.0); }
            if let Some(v) = outer_end { e.outer[1] = v.max(0.0); }
            if let Some(v) = falloff { e.falloff = v.min(3); }
            if let Some(v) = abs_effect { e.abs_effect = v.clamp(0.0, 1.0); }
            if let Some(v) = squash { e.squash = v.max(0.0); }
            data.refresh_envelope_weights(mesh);
            Ok(())
        }),
        C::AddCrossSectionSkin => edit(state, "Add Cross Section", |data, mesh, st| {
            let i = current_bone_index(st, data)?;
            let b = &mut data.bones[i];
            let (inner, outer) = ((b.envelope.inner[0] + b.envelope.inner[1]) * 0.5, (b.envelope.outer[0] + b.envelope.outer[1]) * 0.5);
            b.cross_sections.push((0.5, inner, outer));
            data.refresh_envelope_weights(mesh);
            Ok(())
        }),
        C::RemoveCrossSectionSkin => edit(state, "Remove Cross Section", |data, mesh, st| {
            let i = current_bone_index(st, data)?;
            data.bones[i].cross_sections.pop().ok_or("the bone has no cross sections")?;
            data.refresh_envelope_weights(mesh);
            Ok(())
        }),
        C::SetSkinPose => edit(state, "Set Skin Pose", |data, _, st| {
            data.skin_pose = Some(data.bones.iter().filter_map(|b| st.scene.node(b.node).map(|n| (b.node, n.transform.clone()))).collect());
            Ok(())
        }),
        C::AssumeSkinPose => {
            let e = entry(state)?;
            let pose = e.data.skin_pose.clone().ok_or("no skin pose stored (Set Skin Pose first)")?;
            assume(state, &pose, "Assume Skin Pose")
        }
        C::SkinPoseModeOnOffToggle => {
            let e = entry(state)?;
            if let Some(live) = state.skin_pose_live.take() {
                assume(state, &live, "Skin Pose Mode Off")
            } else {
                let pose = e.data.skin_pose.clone().ok_or("no skin pose stored (Set Skin Pose first)")?;
                let live: Vec<(NodeId, Transform)> = pose.iter().filter_map(|(id, _)| state.scene.node(*id).map(|n| (*id, n.transform.clone()))).collect();
                let r = assume(state, &pose, "Skin Pose Mode On")?;
                state.skin_pose_live = Some(live);
                Ok(r)
            }
        }
        C::BackfaceCullVerticesSkin => {
            state.skin_backface_cull = !state.skin_backface_cull;
            Ok(status(state))
        }
        C::DrawCrossSectionsOnTopSkin => toggle_param(state, "Draw CrossSections On Top"),
        C::DrawEnvelopeOnTopSkin => toggle_param(state, "Draw Envelope On Top"),
        C::FilterCrossSectionsSkin => {
            state.skin_filter_cross_sections = !state.skin_filter_cross_sections;
            Ok(status(state))
        }
        C::FilterEnvelopesSkin => {
            state.skin_filter_envelopes = !state.skin_filter_envelopes;
            Ok(status(state))
        }
        C::FilterVerticesSkin => {
            state.skin_filter_vertices = !state.skin_filter_vertices;
            Ok(status(state))
        }
        C::ExcludeVertsSkin => edit(state, "Exclude Verts", |data, mesh, st| {
            let i = current_bone_index(st, data)?;
            for v in selected_verts(st, mesh.vertex_count()) {
                data.excluded[i].insert(v as u32);
                data.weights[v].retain(|(b, _)| *b as usize != i);
                normalize(&mut data.weights[v]);
            }
            Ok(())
        }),
        C::IncludeVertsSkin => edit(state, "Include Verts", |data, mesh, st| {
            let i = current_bone_index(st, data)?;
            for v in selected_verts(st, mesh.vertex_count()) {
                data.excluded[i].remove(&(v as u32));
            }
            data.refresh_envelope_weights(mesh);
            Ok(())
        }),
        C::SelectExcludedVertsSkin => {
            let e = entry(state)?;
            let i = current_bone_index(state, &e.data)?;
            let ids: Vec<u32> = e.data.excluded[i].iter().copied().collect();
            set_selection(state, ids, SelectOp::Replace);
            Ok(status(state))
        }
        C::GrowSelectionSkin | C::SkinGrow => map_selection(state, |m, ids| select_ops::grow(m, SubLevel::Vertex, &ids)),
        C::ShrinkSelectionSkin | C::SkinShrink => map_selection(state, |m, ids| select_ops::shrink(m, SubLevel::Vertex, &ids)),
        C::LoopSelectionSkin | C::SkinLoop => map_selection(state, |m, ids| {
            let edges = selected_edges(m, &ids);
            let mut out = edges_to_vertices(m, &select_ops::edge_loop(m, &edges));
            out.extend(ids);
            out
        }),
        C::RingSelectionSkin | C::SkinRing => map_selection(state, |m, ids| {
            let edges = selected_edges(m, &ids);
            let mut out = edges_to_vertices(m, &select_ops::edge_ring(m, &edges));
            out.extend(ids);
            out
        }),
        C::SkinElement => map_selection(state, |m, ids| {
            let faces: Vec<u32> = ids.iter().flat_map(|v| select_ops::vertex_edges(m, VertexId(*v)).into_iter().filter_map(|he| m.half_edges()[he.0 as usize].face.map(|f| f.0))).collect();
            let mut out: BTreeSet<u32> = ids.iter().copied().collect();
            for f in faces {
                for ef in select_ops::element_of(m, loom_mesh::FaceId(f)) {
                    out.extend(m.face_vertex_ids(ef).unwrap_or_default().into_iter().map(|v| v.0));
                }
            }
            out.into_iter().collect()
        }),
        C::SelectVerticesByBoneSkin => {
            let e = entry(state)?;
            let i = current_bone_index(state, &e.data)? as u32;
            let ids: Vec<u32> = (0..e.input.vertex_count()).filter(|v| weight_of(&current_weights(&e.data, &e.input, *v), i) > 1e-4).map(|v| v as u32).collect();
            set_selection(state, ids, SelectOp::Replace);
            Ok(status(state))
        }
        C::SelectNextBoneSkin => bone_relative(state, |_, d, cur| Some((cur + 1) % d.bones.len())),
        C::SelectPreviousBoneSkin => bone_relative(state, |_, d, cur| Some((cur + d.bones.len() - 1) % d.bones.len())),
        C::SelectParentBoneSkin => bone_relative(state, |st, d, cur| {
            let p = st.scene.node(d.bones[cur].node)?.parent?;
            d.bones.iter().position(|b| b.node == p)
        }),
        C::SelectChildBoneSkin => bone_relative(state, |st, d, cur| {
            let me = d.bones[cur].node;
            d.bones.iter().position(|b| st.scene.node(b.node).is_some_and(|n| n.parent == Some(me)))
        }),
        C::SelectNextSiblingBoneSkin => bone_relative(state, |st, d, cur| sibling(st, d, cur, 1)),
        C::SelectPreviousSiblingBoneSkin => bone_relative(state, |st, d, cur| sibling(st, d, cur, -1)),
        C::SelectStartPointSkin => {
            entry(state)?;
            state.skin_handle = Some(0);
            Ok(status(state))
        }
        C::SelectEndPointSkin => {
            entry(state)?;
            state.skin_handle = Some(1);
            Ok(status(state))
        }
        C::CopyEnvelopeSkin => {
            let e = entry(state)?;
            let i = current_bone_index(state, &e.data)?;
            state.skin_clipboard_envelope = Some(e.data.bones[i].envelope.clone());
            Ok(status(state))
        }
        C::PasteEnvelopeSkin => {
            let env = state.skin_clipboard_envelope.clone().ok_or("no envelope copied")?;
            edit(state, "Paste Envelope", |data, mesh, st| {
                let i = current_bone_index(st, data)?;
                paste_envelope(&mut data.bones[i].envelope, &env);
                data.refresh_envelope_weights(mesh);
                Ok(())
            })
        }
        C::PasteToAllEnvelopesSkin => {
            let env = state.skin_clipboard_envelope.clone().ok_or("no envelope copied")?;
            edit(state, "Paste To All Envelopes", |data, mesh, _| {
                for b in &mut data.bones {
                    paste_envelope(&mut b.envelope, &env);
                }
                data.refresh_envelope_weights(mesh);
                Ok(())
            })
        }
        C::CopyWeightsSkin => {
            let e = entry(state)?;
            let verts = selected_verts(state, e.input.vertex_count());
            if verts.is_empty() {
                return Err("select vertices first".into());
            }
            state.skin_clipboard_weights = verts
                .into_iter()
                .map(|v| {
                    let w = current_weights(&e.data, &e.input, v);
                    (e.input.vertices()[v].position, w.into_iter().map(|(b, x)| (e.data.bones[b as usize].node, x)).collect())
                })
                .collect();
            Ok(status(state))
        }
        C::PasteWeightsSkin | C::PasteWeightsByPosSkin => {
            let by_pos = matches!(cmd, C::PasteWeightsByPosSkin);
            let clip = state.skin_clipboard_weights.clone();
            if clip.is_empty() {
                return Err("no weights copied".into());
            }
            edit(state, "Paste Weights", |data, mesh, st| {
                let verts = selected_verts(st, mesh.vertex_count());
                for (k, v) in verts.iter().enumerate() {
                    let src = if by_pos {
                        let p = mesh.vertices()[*v].position;
                        clip.iter().min_by(|a, b| a.0.distance(p).total_cmp(&b.0.distance(p))).unwrap()
                    } else {
                        &clip[k % clip.len()]
                    };
                    let mut w: Vec<(u32, f32)> = src.1.iter().filter_map(|(node, x)| data.bones.iter().position(|b| b.node == *node).map(|i| (i as u32, *x))).collect();
                    normalize(&mut w);
                    data.weights[*v] = w;
                    data.explicit[*v] = true;
                }
                Ok(())
            })
        }
        C::RemoveZeroWeightsSkin => edit(state, "Remove Zero Weights", |data, mesh, _| {
            for v in 0..mesh.vertex_count() {
                if data.explicit[v] {
                    data.weights[v].retain(|(_, x)| *x > 0.0005);
                    normalize(&mut data.weights[v]);
                }
            }
            Ok(())
        }),
        C::ZoomToSelectedBoneSkin | C::ZoomToSelectedGizmoSkin => {
            let e = entry(state)?;
            let i = match (cmd, state.skin_gizmo) {
                (C::ZoomToSelectedGizmoSkin, Some(g)) => e.data.gizmos.get(g).map(|g| g.bone).ok_or("no gizmo selected")?,
                _ => current_bone_index(state, &e.data)?,
            };
            let bone = e.data.bones[i].node;
            let bounds = state.bounds_of(std::iter::once(bone));
            crate::execute::frame(state, bounds)
        }
        C::GizmoResetResetRotationPlaneSkin => edit(state, "Gizmo Reset", |data, _, st| {
            let g = st.skin_gizmo.ok_or("select a gizmo first")?;
            let gizmo = data.gizmos.get_mut(g).ok_or("no such gizmo")?;
            for k in &mut gizmo.keys {
                k.deltas.clear();
                k.bulge = 0.0;
            }
            Ok(())
        }),
        C::WeightTableDialogSkin => {
            entry(state)?;
            state.dialog = Some(Dialog::WeightTable);
            Ok(json!({ "dialog": "WeightTable" }))
        }
        C::WeightToolDialogSkin => {
            entry(state)?;
            state.dialog = Some(Dialog::WeightTool);
            Ok(json!({ "dialog": "WeightTool" }))
        }
        C::MirrorModeSkin => {
            let e = entry(state)?;
            crate::execute(state, C::SetModifierParameter { index: e.index as u32, name: "Mirror Mode".into(), value: if e.data.mirror.mode { 0.0 } else { 1.0 } })?;
            Ok(status(state))
        }
        C::PasteGreenToBlue | C::PasteBlueToGreen => {
            let green_to_blue = matches!(cmd, C::PasteGreenToBlue);
            edit(state, "Mirror Paste", |data, mesh, st| {
                let names = bone_names(st, data);
                let (plane, offset, threshold) = (data.mirror.plane, data.mirror.offset, data.mirror.threshold);
                loom_scene::skin::mirror_weights_named(data, mesh, &names, plane, offset, threshold, green_to_blue);
                Ok(())
            })
        }
        C::MirrorPaste => edit(state, "Mirror Paste", |data, mesh, st| {
            let names = bone_names(st, data);
            let (plane, offset, threshold) = (data.mirror.plane, data.mirror.offset, data.mirror.threshold);
            let verts = selected_verts(st, mesh.vertex_count());
            if verts.is_empty() {
                return Err("select vertices first".into());
            }
            // Only the selected vertices' mirrors change: paste both ways on a copy, keep the selected sources' targets.
            let a = plane.min(2) as usize;
            let pairs = loom_scene::skin::mirror_bone_pairs(data, &names, plane, offset, threshold);
            let positions: Vec<Vec3> = mesh.vertices().iter().map(|v| v.position).collect();
            let current: Vec<Vec<(u32, f32)>> = (0..mesh.vertex_count()).map(|i| data.vertex_weights(i, data.mesh_bind.transform_point3(positions[i]))).collect();
            for v in verts {
                let mut m = positions[v];
                m[a] = 2.0 * offset - m[a];
                let Some((t, _)) = (0..mesh.vertex_count()).filter(|t| *t != v).map(|t| (t, positions[t].distance(m))).filter(|(_, d)| *d <= threshold).min_by(|x, y| x.1.total_cmp(&y.1)) else { continue };
                let mut w: Vec<(u32, f32)> = current[v].iter().map(|(b, x)| (pairs[*b as usize] as u32, *x)).collect();
                normalize(&mut w);
                data.weights[t] = w;
                data.explicit[t] = true;
            }
            Ok(())
        }),
        C::SkinAddGizmo { kind } => {
            let kind = match kind.to_ascii_lowercase().as_str() {
                "joint" | "joint angle" | "joint angle deformer" => loom_scene::skin::GizmoKind::JointAngle,
                "bulge" | "bulge angle" | "bulge angle deformer" => loom_scene::skin::GizmoKind::BulgeAngle,
                other => return Err(format!("unknown gizmo kind {other:?} (joint, bulge)")),
            };
            let r = edit(state, "Add Gizmo", |data, _, st| {
                let bone = current_bone_index(st, data)?;
                let parent_node = st.scene.node(data.bones[bone].node).and_then(|n| n.parent).ok_or("the current bone has no parent bone in the Skin")?;
                let parent = data.bones.iter().position(|b| b.node == parent_node).ok_or("the current bone's parent is not in the Skin's list")?;
                let angle = loom_scene::skin::bone_angle(data, &|id| Some(st.scene.world_matrix(id)), bone, parent).unwrap_or(0.0);
                data.gizmos.push(loom_scene::skin::SkinGizmo { kind, bone, parent_bone: parent, keys: vec![loom_scene::skin::GizmoKey { angle_deg: angle, deltas: Vec::new(), bulge: 0.0 }], influence: 1.0, enabled: true });
                Ok(())
            })?;
            state.skin_gizmo = entry(state).ok().and_then(|e| e.data.gizmos.len().checked_sub(1));
            Ok(r)
        }
        C::SkinRemoveGizmo => {
            let g = state.skin_gizmo.ok_or("select a gizmo first")?;
            let r = edit(state, "Remove Gizmo", |data, _, _| {
                if g >= data.gizmos.len() {
                    return Err("no such gizmo".into());
                }
                data.gizmos.remove(g);
                Ok(())
            })?;
            state.skin_gizmo = None;
            Ok(r)
        }
        C::SkinSelectGizmo { index } => {
            let e = entry(state)?;
            if index as usize >= e.data.gizmos.len() {
                return Err(format!("no gizmo {index}"));
            }
            state.skin_gizmo = Some(index as usize);
            Ok(status(state))
        }
        C::SkinAddGizmoKey => edit(state, "Add Gizmo Key", |data, _, st| {
            let g = st.skin_gizmo.ok_or("select a gizmo first")?;
            let (bone, parent) = { let gz = data.gizmos.get(g).ok_or("no such gizmo")?; (gz.bone, gz.parent_bone) };
            let angle = loom_scene::skin::bone_angle(data, &|id| Some(st.scene.world_matrix(id)), bone, parent).unwrap_or(0.0);
            data.gizmos[g].keys.push(loom_scene::skin::GizmoKey { angle_deg: angle, deltas: Vec::new(), bulge: 0.0 });
            Ok(())
        }),
        C::SkinSetGizmoKey { key, bulge, angle } => edit(state, "Gizmo Key", |data, _, st| {
            let g = st.skin_gizmo.ok_or("select a gizmo first")?;
            let k = data.gizmos.get_mut(g).and_then(|gz| gz.keys.get_mut(key as usize)).ok_or("no such key")?;
            if let Some(b) = bulge { k.bulge = b; }
            if let Some(a) = angle { k.angle_deg = a; }
            Ok(())
        }),
        C::SkinSetGizmoDelta { key, vertex, delta } => edit(state, "Gizmo Delta", |data, _, st| {
            let g = st.skin_gizmo.ok_or("select a gizmo first")?;
            let k = data.gizmos.get_mut(g).and_then(|gz| gz.keys.get_mut(key as usize)).ok_or("no such key")?;
            let d = Vec3::from(delta);
            match k.deltas.iter_mut().find(|(v, _)| *v == vertex) {
                Some(entry) => entry.1 = d,
                None => k.deltas.push((vertex, d)),
            }
            Ok(())
        }),
        C::SkinSetGizmoOption { name, value } => edit(state, "Gizmo Option", |data, _, st| {
            let g = st.skin_gizmo.ok_or("select a gizmo first")?;
            let gz = data.gizmos.get_mut(g).ok_or("no such gizmo")?;
            match name.as_str() {
                "Influence" => gz.influence = value as f32,
                "Enabled" => gz.enabled = value != 0.0,
                other => return Err(format!("gizmo has no option {other:?}")),
            }
            Ok(())
        }),
        other => Err(format!("{} is not a Skin command", other.name())),
    }
}

fn bone_names(state: &AppState, data: &SkinData) -> Vec<String> {
    data.bones.iter().map(|b| state.scene.node(b.node).map(|n| n.name.clone()).unwrap_or_default()).collect()
}

fn paste_envelope(dst: &mut Envelope, src: &Envelope) {
    dst.inner = src.inner;
    dst.outer = src.outer;
    dst.falloff = src.falloff;
    dst.abs_effect = src.abs_effect;
    dst.squash = src.squash;
}

fn sibling(st: &AppState, d: &SkinData, cur: usize, dir: i32) -> Option<usize> {
    let parent = st.scene.node(d.bones[cur].node)?.parent;
    let sibs: Vec<usize> = (0..d.bones.len()).filter(|i| st.scene.node(d.bones[*i].node).is_some_and(|n| n.parent == parent)).collect();
    let k = sibs.iter().position(|i| *i == cur)?;
    let n = sibs.len() as i32;
    Some(sibs[((k as i32 + dir).rem_euclid(n)) as usize])
}

fn toggle_param(state: &mut AppState, name: &str) -> Result<Value, String> {
    let e = entry(state)?;
    let on = match name {
        "Draw CrossSections On Top" => e.data.display.cross_sections_on_top,
        _ => e.data.display.envelope_on_top,
    };
    crate::execute(state, Command::SetModifierParameter { index: e.index as u32, name: name.into(), value: if on { 0.0 } else { 1.0 } })?;
    Ok(status(state))
}

/// Moves the bones to a stored pose as one undo entry.
fn assume(state: &mut AppState, pose: &[(NodeId, Transform)], label: &str) -> Result<Value, String> {
    let mut changes = Vec::new();
    for (id, t) in pose {
        let Some(n) = state.scene.node(*id) else { continue };
        if n.transform != *t {
            changes.push(Change::SetTransform { id: *id, before: n.transform.clone(), after: t.clone() });
        }
    }
    if !changes.is_empty() {
        commit(state, label, Change::Group(changes))?;
    }
    Ok(status(state))
}

pub fn status(state: &AppState) -> Value {
    match entry(state) {
        Ok(e) => json!({
            "node": e.id.0,
            "index": e.index,
            "bones": e.data.bones.iter().map(|b| json!({ "node": b.node.0, "name": state.scene.node(b.node).map(|n| n.name.clone()).unwrap_or_default(), "inner": b.envelope.inner, "outer": b.envelope.outer, "falloff": b.envelope.falloff, "cross_sections": b.cross_sections.len() })).collect::<Vec<_>>(),
            "bone": state.skin_bone.map(|b| b.0),
            "selection": selection(state),
            "edit_envelopes": state.skin_active(),
            "vertices": e.data.weights.len(),
            "explicit": e.data.explicit.iter().filter(|x| **x).count(),
            "skin_pose": e.data.skin_pose.is_some(),
            "skin_pose_mode": state.skin_pose_live.is_some(),
            "gizmos": e.data.gizmos.iter().map(|g| json!({ "kind": format!("{:?}", g.kind), "bone": g.bone, "parent": g.parent_bone, "keys": g.keys.len() })).collect::<Vec<_>>(),
            "custom_weight": state.skin_custom_weight,
        }),
        Err(_) => Value::Null,
    }
}

/// Vertex weight colour for the current bone (Show Colored Vertices): blue 0 → red 1.
pub fn weight_color(w: f32) -> [u8; 3] {
    let w = w.clamp(0.0, 1.0);
    if w <= 0.0 {
        return [60, 90, 255];
    }
    if w < 0.5 {
        let t = w / 0.5;
        [(60.0 + 195.0 * t) as u8, (90.0 + 165.0 * t) as u8, (255.0 * (1.0 - t)) as u8]
    } else {
        let t = (w - 0.5) / 0.5;
        [255, (255.0 * (1.0 - t)) as u8, 0]
    }
}

/// Skin > Edit Envelopes is where the viewport clicks go.
pub fn picks_vertices(state: &AppState) -> bool {
    state.skin_active() && state.skin_filter_vertices && state.selection_mode == SelectionMode::Object
}

/// Per-vertex weight of the current bone, for the viewport colours.
pub fn current_bone_weights(state: &AppState) -> Option<(NodeId, Vec<f32>)> {
    let e = entry(state).ok()?;
    let i = current_bone_index(state, &e.data).ok()? as u32;
    Some((e.id, (0..e.input.vertex_count()).map(|v| weight_of(&current_weights(&e.data, &e.input, v), i)).collect()))
}

/// The current bone's envelope capsule in world space: (start, end, inner radii, outer radii, cross sections).
pub fn current_envelope_world(state: &AppState) -> Option<(Vec3, Vec3, [f32; 2], [f32; 2], Vec<(f32, f32, f32)>)> {
    let e = entry(state).ok()?;
    let i = current_bone_index(state, &e.data).ok()?;
    let b = &e.data.bones[i];
    let world = b.bind_inverse.inverse();
    Some((world.transform_point3(b.envelope.start), world.transform_point3(b.envelope.end), b.envelope.inner, b.envelope.outer, b.cross_sections.clone()))
}
