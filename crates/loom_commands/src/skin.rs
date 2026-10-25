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
use loom_scene::skin::{normalize, Envelope, SkinData, SkinGizmo};
use loom_scene::{Change, Modifier, NodeId, Transform};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;

/// Skin state beyond the modifier's own data: selections, pick modes and buffers.
#[derive(Debug, Clone, Default)]
pub struct SkinUi {
    /// The selected cross section ring of the current envelope: (index in `SkinBone::sections`, outer ring).
    pub cross_section: Option<(usize, bool)>,
    /// Cross Sections > Add: the next click on the current bone places a cross section.
    pub adding_cross_section: bool,
    /// Select > Select Element: a picked vertex selects its whole element.
    pub select_element: bool,
    /// Gizmo Parameters > Edit Lattice, and the picked lattice points.
    pub edit_lattice: bool,
    pub lattice_selection: BTreeSet<u32>,
    /// Gizmos > Copy buffer.
    pub clipboard_gizmo: Option<SkinGizmo>,
    /// Mirror Parameters > Manual Update: the side colours as of the last Update.
    pub mirror_sides: Option<((NodeId, usize), Vec<i8>)>,
    /// Load Envelopes: the file whose bones are being matched.
    pub load: Option<EnvFile>,
    /// Weight Table: the vertex a double-click shows in pink.
    pub highlight: Option<u32>,
}

/// An envelope file (.env): 3dLoom's own JSON (the reference's binary
/// format is not documented): the bones' envelopes in their own frames,
/// cross sections and exclusions, and the vertices with their weights.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvFile {
    pub format: String,
    pub bones: Vec<EnvBone>,
    pub vertices: Vec<EnvVertex>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvBone {
    pub name: String,
    pub envelope: Envelope,
    pub cross_sections: Vec<(f32, f32, f32)>,
    pub excluded: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvVertex {
    /// Position in the skinned object's space (Load Vertices By Index off matches by it).
    pub position: [f32; 3],
    /// (index into `EnvFile::bones`, weight).
    pub weights: Vec<(u32, f32)>,
    pub explicit: bool,
    pub rigid: bool,
}

pub const ENV_FORMAT: &str = "3dloom-skin-envelopes-1";

/// The Skin commands outside the reference's Skin action group.
pub fn handles(c: &Command) -> bool {
    use Command as C;
    matches!(
        c,
        C::SkinSetEnvelopeMode { .. }
            | C::PasteToMultipleBonesSkin
            | C::SkinPasteToBones { .. }
            | C::SkinSetPainterCurve { .. }
            | C::SkinSetExclusion { .. }
            | C::SkinAddVertexSet { .. }
            | C::SkinSelectVertexSet { .. }
            | C::SkinRemoveVertexSet { .. }
            | C::SkinRigidHandlesToggle
            | C::SkinResetSelectedVerts
            | C::SkinResetSelectedBone
            | C::SkinResetAllBones
            | C::SkinSaveEnvelopes { .. }
            | C::SkinLoadEnvelopes { .. }
            | C::SkinApplyLoadedEnvelopes { .. }
            | C::SkinSelectCrossSection { .. }
            | C::SkinSetCrossSectionRadius { .. }
            | C::SkinAddCrossSectionAt { .. }
            | C::SkinMoveEnvelopePoint { .. }
            | C::SkinSelectElementToggle
            | C::PasteGreenToBlueBones
            | C::PasteBlueToGreenBones
            | C::SkinMirrorUpdate
            | C::SkinCopyGizmo
            | C::SkinPasteGizmo
            | C::SkinRenameGizmo { .. }
            | C::SkinGizmoAddFromStack
            | C::SkinGizmoAddFromNode { .. }
            | C::SkinGizmoDeleteTarget { .. }
            | C::SkinGizmoRenameTarget { .. }
            | C::SkinEditLatticeToggle
            | C::SkinSelectLatticePoints { .. }
            | C::SkinSetLatticeOffset { .. }
            | C::SkinAngleKeysCurvesDialog
            | C::SkinHighlightVertex { .. }
    )
}

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
        Some(prev) => {
            let ev = state.scene.evaluated(id, Some(prev));
            match &ev.value.patch {
                Some(p) => p.cage(),
                None => ev.value.mesh.clone(),
            }
        }
        None => match node.object.patch_mesh() {
            Some(p) => p.cage(),
            None => node.object.display_output(),
        },
    };
    Ok(Entry { id, index, data: (**data).clone(), input })
}

/// The patch lattice flowing into the Skin, when it deforms a patch.
pub fn input_patch(state: &AppState) -> Option<loom_mesh::patch::PatchMesh> {
    let (id, index) = state.skin_entry()?;
    match index.checked_sub(1) {
        Some(prev) => state.scene.evaluated(id, Some(prev)).value.patch.clone(),
        None => state.scene.node(id)?.object.patch_mesh(),
    }
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

/// Sets one bone's weight on a vertex. Lowering the only bone a vertex has
/// gives the rest to the other bones whose envelopes reach it, so the weight
/// can come down (it would normalise straight back to 1 otherwise).
fn set_bone_weight(data: &mut SkinData, mesh: &Mesh, v: usize, bone: u32, w: f32, current: &[(u32, f32)]) {
    let alone = current.iter().all(|(b, x)| *b == bone || *x <= 0.0);
    if alone && w < 1.0 {
        let p = data.mesh_bind.transform_point3(mesh.vertices()[v].position);
        let mut with: Vec<(u32, f32)> = current.to_vec();
        with.extend(envelope_reach(data, p, v as u32).into_iter().filter(|(b, _)| *b != bone));
        data.set_weight(v, bone, w, &with);
    } else {
        data.set_weight(v, bone, w, current);
    }
}

/// The raw envelope weight of every enabled bone not excluded from the vertex.
fn envelope_reach(data: &SkinData, p_world: Vec3, v: u32) -> Vec<(u32, f32)> {
    data.bones
        .iter()
        .enumerate()
        .filter(|(i, b)| b.enabled && !data.excluded.get(*i).is_some_and(|x| x.contains(&v)))
        .filter_map(|(i, b)| {
            let e = loom_scene::skin::envelope_weight(&b.envelope, b.bind_inverse.transform_point3(p_world));
            (e > 1e-6).then_some((i as u32, e))
        })
        .collect()
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
            set_bone_weight(data, mesh, v, bone, w, &cur);
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
    let bone = state.skin_bone;
    let r = run(state, cmd);
    if state.skin_bone != bone {
        // Show Colored Faces follows the current bone.
        state.mesh_dirty = true;
    }
    r
}

fn run(state: &mut AppState, cmd: Command) -> Result<Value, String> {
    use Command as C;
    match cmd {
        C::SkinModifier => crate::execute(state, C::AddModifier { name: "Skin".into() }),
        C::SkinMorphModifier => crate::execute(state, C::AddModifier { name: "Skin Morph".into() }),
        C::SkinWrapModifier => crate::execute(state, C::AddModifier { name: "Skin Wrap".into() }),
        C::SkinWrapPatchModifier => crate::execute(state, C::AddModifier { name: "Skin Wrap Patch".into() }),
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
            let mut ids = ids;
            if state.skin_ui.select_element {
                // Select Element: a picked vertex brings its whole element.
                ids = element_vertices(&e.input, &ids);
            }
            let m = &e.data.mirror;
            if m.mode && (m.projection == 1 || m.projection == 2) {
                // Display Projection Positive / Negative: only that side's vertices select.
                let a = m.plane.min(2) as usize;
                let positive = m.projection == 1;
                ids.retain(|v| {
                    let d = e.input.vertices()[*v as usize].position[a] - m.offset;
                    if positive { d > 0.0 } else { d < 0.0 }
                });
            }
            set_selection(state, ids, op);
            if op == SelectOp::Replace {
                // A new vertex pick replaces an end point / cross section pick.
                state.skin_handle = None;
                state.skin_ui.cross_section = None;
            }
            Ok(status(state))
        }
        C::SkinSetVertexWeight { weight, absolute } => weight_edit(state, "Set Weight", move |cur, _| if absolute { weight } else { cur + weight }),
        C::SkinSetCustomWeight { value } => {
            state.skin_custom_weight = value.clamp(0.0, 1.0);
            Ok(status(state))
        }
        C::SkinSetScaleWeight { value } => {
            state.skin_scale_weight = value.max(0.0);
            Ok(status(state))
        }
        C::SkinSetPasteTolerance { value } => {
            state.skin_paste_tolerance = value.max(0.0);
            Ok(status(state))
        }
        C::SkinSetVertexWeights { vertices, bone, weight } => edit(state, "Weight Table", |data, mesh, _| {
            let b = data.bones.iter().position(|x| x.node == NodeId(bone)).ok_or("the bone is not in the Skin's list")? as u32;
            let n = mesh.vertex_count();
            if let Some(bad) = vertices.iter().find(|v| **v as usize >= n) {
                return Err(format!("no vertex {bad} (the mesh has {n})"));
            }
            for v in vertices {
                let cur = current_weights(data, mesh, v as usize);
                set_bone_weight(data, mesh, v as usize, b, weight, &cur);
            }
            Ok(())
        }),
        C::SkinSelectBoneByName { name } => {
            let e = entry(state)?;
            let names = bone_names(state, &e.data);
            let i = names.iter().position(|n| name_matches(&name, n)).ok_or_else(|| format!("no bone matches {name:?}"))?;
            state.skin_bone = Some(e.data.bones[i].node);
            state.skin_gizmo = None;
            Ok(status(state))
        }
        C::SkinEnvelopeVisibility => edit(state, "Envelope Visibility", |data, _, st| {
            let i = current_bone_index(st, data)?;
            data.bones[i].envelope_visible = !data.bones[i].envelope_visible;
            Ok(())
        }),
        C::SkinPaintWeightsToggle => {
            let e = entry(state)?;
            if !state.skin_paint && !state.skin_active() {
                // Paint Weights works at the Envelope level, as Edit Envelopes does.
                crate::execute(state, C::ModifierSubobjectLevel { index: e.index as u32, level: 1 })?;
            }
            state.skin_paint = !state.skin_paint;
            Ok(status(state))
        }
        C::SkinPaintStroke { points, erase, pressure } => paint(state, &points, erase, pressure.as_deref()),
        C::SkinSetPainterOption { name, value } => {
            let p = &mut state.skin_painter;
            let on = value != 0.0;
            match name.as_str() {
                "Max Strength" => p.strength = (value as f32).clamp(0.0, 1.0),
                "Max Size" => p.size = (value as f32).max(1e-3),
                "Min Strength" => p.min_strength = (value as f32).clamp(0.0, 1.0),
                "Min Size" => p.min_size = (value as f32).max(0.0),
                "Enable Pressure Sensitivity" => p.pressure_on = on,
                "Pressure Affects" => p.pressure_affects = (value.max(0.0) as u32).min(3),
                "Additive" => p.additive = on,
                "Falloff" => {
                    // A falloff button sets the graph to its curve.
                    p.falloff = (value.max(0.0) as u32).min(4);
                    p.curve = Painter::preset_curve(p.falloff);
                }
                "Draw Ring" => p.draw_ring = on,
                "Mirror" => p.mirror = on,
                "Mirror Axis" => p.mirror_axis = (value.max(0.0) as u32).min(2),
                "Mirror Offset" => p.mirror_offset = value as f32,
                "Paint Blend Weights" => p.blend = on,
                other => return Err(format!("Painter Options has no {other:?}")),
            }
            Ok(status(state))
        }
        C::SkinPainterOptionsDialog => {
            entry(state)?;
            state.dialog = Some(Dialog::PainterOptions);
            Ok(json!({ "dialog": "PainterOptions" }))
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
        C::ScaleWeightUpSkin => weight_edit(state, "Scale Weight", |w, _| w * 1.05),
        C::ScaleWeightDownSkin => weight_edit(state, "Scale Weight", |w, _| w * 0.95),
        C::ScaleWeightCustomSkin => {
            let k = state.skin_scale_weight;
            weight_edit(state, "Scale Weight", move |w, _| w * k)
        }
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
            let animatable = data.animatable_envelopes;
            let e = &mut data.bones[i].envelope;
            if animatable && loom_scene::anim::animate() && e.keys.is_none() {
                // Animatable Envelopes under Auto Key: the radii become keyable and key at this frame.
                let p = |x: f32| loom_scene::Param::new(x);
                e.keys = Some(Box::new(loom_scene::skin::EnvelopeKeys { inner: [p(e.inner[0]), p(e.inner[1])], outer: [p(e.outer[0]), p(e.outer[1])] }));
            }
            if let Some(v) = inner_start { e.set_radius(0, false, v); }
            if let Some(v) = inner_end { e.set_radius(1, false, v); }
            if let Some(v) = outer_start { e.set_radius(0, true, v); }
            if let Some(v) = outer_end { e.set_radius(1, true, v); }
            if let Some(v) = falloff { e.falloff = v.min(3); }
            if let Some(v) = abs_effect { e.abs_effect = v.clamp(0.0, 1.0); }
            if let Some(v) = squash { e.squash = v.max(0.0); }
            data.refresh_envelope_weights(mesh);
            Ok(())
        }),
        C::AddCrossSectionSkin => {
            // Add, then a click on the bone in a viewport places the cross section.
            let e = entry(state)?;
            current_bone_index(state, &e.data)?;
            state.skin_ui.adding_cross_section = !state.skin_ui.adding_cross_section;
            if state.skin_ui.adding_cross_section {
                state.status_prompt = "Click on the bone to add a cross section".into();
            }
            Ok(status(state))
        }
        C::RemoveCrossSectionSkin => {
            let (k, _) = state.skin_ui.cross_section.ok_or("select a cross section first (Cross Sections filter on)")?;
            let r = edit(state, "Remove Cross Section", |data, mesh, st| {
                let i = current_bone_index(st, data)?;
                let n = data.bones[i].cross_sections.len() + 2;
                if k == 0 || k + 1 >= n {
                    return Err("only cross sections you added can be removed, not the envelope's ends".into());
                }
                data.bones[i].cross_sections.remove(k - 1);
                data.refresh_envelope_weights(mesh);
                Ok(())
            })?;
            state.skin_ui.cross_section = None;
            Ok(r)
        }
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
            state.skin_ui.cross_section = None;
            Ok(status(state))
        }
        C::SelectEndPointSkin => {
            entry(state)?;
            state.skin_handle = Some(1);
            state.skin_ui.cross_section = None;
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
            let tolerance = state.skin_paste_tolerance;
            if clip.is_empty() {
                return Err("no weights copied".into());
            }
            edit(state, "Paste Weights", |data, mesh, st| {
                let verts = selected_verts(st, mesh.vertex_count());
                for (k, v) in verts.iter().enumerate() {
                    let src = if by_pos {
                        // Paste-Pos: the nearest copied vertex within the tolerance; farther vertices keep theirs.
                        let p = mesh.vertices()[*v].position;
                        match clip.iter().filter(|c| c.0.distance(p) <= tolerance).min_by(|a, b| a.0.distance(p).total_cmp(&b.0.distance(p))) {
                            Some(c) => c,
                            None => continue,
                        }
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
            // Every vertex loses its weights below the Remove Zero Limit (they become hand-set).
            let limit = data.remove_zero_limit;
            for v in 0..mesh.vertex_count() {
                let w = current_weights(data, mesh, v);
                if w.iter().any(|(_, x)| *x < limit || *x <= 0.0) {
                    let mut kept: Vec<(u32, f32)> = w.into_iter().filter(|(_, x)| *x >= limit && *x > 0.0).collect();
                    normalize(&mut kept);
                    data.weights[v] = kept;
                    data.explicit[v] = true;
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
                "morph" | "morph angle" | "morph angle deformer" => loom_scene::skin::GizmoKind::MorphAngle,
                other => return Err(format!("unknown gizmo kind {other:?} (joint, bulge, morph)")),
            };
            let r = edit(state, "Add Gizmo", |data, mesh, st| {
                let bone = current_bone_index(st, data)?;
                let parent_node = st.scene.node(data.bones[bone].node).and_then(|n| n.parent).ok_or("the current bone has no parent bone in the Skin")?;
                let parent = data.bones.iter().position(|b| b.node == parent_node).ok_or("the current bone's parent is not in the Skin's list")?;
                let angle = loom_scene::skin::bone_angle(data, &|id| Some(st.scene.world_matrix(id)), bone, parent).unwrap_or(0.0);
                // The gizmo works on the selected vertices (with none selected, on its bones' vertices).
                let mut vertices: Vec<u32> = selected_verts(st, mesh.vertex_count()).into_iter().map(|v| v as u32).collect();
                vertices.sort_unstable();
                let morph = kind == loom_scene::skin::GizmoKind::MorphAngle;
                let mut g = SkinGizmo { kind, bone, parent_bone: parent, keys: vec![loom_scene::skin::GizmoKey { angle_deg: angle, name: if morph { "Base".into() } else { String::new() }, ..Default::default() }], vertices, ..Default::default() };
                g.name = format!("{} {:02}", kind.label(), data.gizmos.iter().filter(|x| x.kind == kind).count() + 1);
                if !morph {
                    // The lattice boxes the gizmo's vertices in its bone's bind frame.
                    let lb = g.lattice_bone();
                    let inv = data.bones[lb].bind_inverse;
                    let pts: Vec<Vec3> = (0..mesh.vertex_count())
                        .filter(|v| {
                            if !g.vertices.is_empty() {
                                return g.vertices.contains(&(*v as u32));
                            }
                            let w = current_weights(data, mesh, *v);
                            let on = |b: usize| w.iter().any(|(x, y)| *x as usize == b && *y > 0.0);
                            if kind == loom_scene::skin::GizmoKind::BulgeAngle { on(parent) } else { on(bone) || on(parent) }
                        })
                        .map(|v| inv.transform_point3(data.mesh_bind.transform_point3(mesh.vertices()[v].position)))
                        .collect();
                    g.lattice = loom_scene::skin::GizmoLattice::around(&pts);
                }
                data.gizmos.push(g);
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
            data.gizmos[g].keys.push(loom_scene::skin::GizmoKey { angle_deg: angle, deltas: Vec::new(), bulge: 0.0, ..Default::default() });
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
                "Twist" => gz.twist = value as f32,
                "Use Bounding Volume" => gz.bounding_volume = value != 0.0,
                other => return Err(format!("gizmo has no option {other:?}")),
            }
            Ok(())
        }),
        C::SkinSetEnvelopeMode { relative } => edit(state, if relative { "Envelope Relative" } else { "Envelope Absolute" }, |data, mesh, st| {
            let i = current_bone_index(st, data)?;
            data.bones[i].envelope.relative = relative;
            data.refresh_envelope_weights(mesh);
            Ok(())
        }),
        C::PasteToMultipleBonesSkin => {
            entry(state)?;
            state.skin_clipboard_envelope.as_ref().ok_or("no envelope copied (Copy first)")?;
            state.dialog = Some(Dialog::PasteToMultipleBones);
            Ok(json!({ "dialog": "PasteToMultipleBones" }))
        }
        C::SkinPasteToBones { ids } => {
            let env = state.skin_clipboard_envelope.clone().ok_or("no envelope copied (Copy first)")?;
            edit(state, "Paste To Multiple Bones", |data, mesh, _| {
                for id in &ids {
                    let i = data.bones.iter().position(|b| b.node == NodeId(*id)).ok_or_else(|| format!("node {id} is not in the Skin's list"))?;
                    paste_envelope(&mut data.bones[i].envelope, &env);
                }
                data.refresh_envelope_weights(mesh);
                Ok(())
            })
        }
        C::SkinSetPainterCurve { points } => {
            if points.len() < 2 {
                return Err("the falloff graph needs at least two points".into());
            }
            let mut pts: Vec<[f32; 2]> = points.iter().map(|p| [p[0].clamp(0.0, 1.0), p[1].clamp(0.0, 1.0)]).collect();
            pts.sort_by(|a, b| a[0].total_cmp(&b[0]));
            state.skin_painter.curve = pts;
            state.skin_painter.falloff = Painter::CUSTOM;
            Ok(status(state))
        }
        C::SkinSetExclusion { vertex, bone, excluded } => edit(state, if excluded { "Exclude Vertex" } else { "Include Vertex" }, |data, mesh, _| {
            let b = data.bones.iter().position(|x| x.node == NodeId(bone)).ok_or("the bone is not in the Skin's list")?;
            let v = vertex as usize;
            if v >= mesh.vertex_count() {
                return Err(format!("no vertex {vertex} (the mesh has {})", mesh.vertex_count()));
            }
            if excluded {
                data.excluded[b].insert(vertex);
                if data.explicit[v] {
                    data.weights[v].retain(|(x, _)| *x as usize != b);
                    normalize(&mut data.weights[v]);
                }
            } else {
                data.excluded[b].remove(&vertex);
            }
            data.refresh_envelope_weights(mesh);
            Ok(())
        }),
        C::SkinAddVertexSet { name } => {
            let name = name.trim().to_owned();
            if name.is_empty() {
                return Err("the vertex set needs a name".into());
            }
            let sel = selection(state);
            if sel.is_empty() {
                return Err("select vertices first".into());
            }
            edit(state, "Create Vertex Set", |data, _, _| {
                data.vertex_sets.retain(|(n, _)| *n != name);
                data.vertex_sets.push((name.clone(), sel.clone()));
                Ok(())
            })
        }
        C::SkinSelectVertexSet { name } => {
            let e = entry(state)?;
            let ids = e.data.vertex_sets.iter().find(|(n, _)| *n == name).map(|(_, v)| v.clone()).ok_or_else(|| format!("no vertex set {name:?}"))?;
            let n = e.input.vertex_count() as u32;
            set_selection(state, ids.into_iter().filter(|v| *v < n), SelectOp::Replace);
            Ok(status(state))
        }
        C::SkinRemoveVertexSet { name } => edit(state, "Delete Vertex Set", |data, _, _| {
            let before = data.vertex_sets.len();
            data.vertex_sets.retain(|(n, _)| *n != name);
            if data.vertex_sets.len() == before {
                return Err(format!("no vertex set {name:?}"));
            }
            Ok(())
        }),
        C::SkinRigidHandlesToggle => {
            let patch = input_patch(state).ok_or("Rigid Handles works on patch objects (the Skin's input has no patches)")?;
            let nv = patch.verts.len();
            edit(state, "Rigid Handles", |data, mesh, st| {
                let verts: Vec<usize> = selected_verts(st, mesh.vertex_count()).into_iter().filter(|v| *v < nv).collect();
                if verts.is_empty() {
                    return Err("select patch vertices first".into());
                }
                let on = !verts.iter().all(|v| data.rigid_handles[*v]);
                for v in verts {
                    data.rigid_handles[v] = on;
                }
                Ok(())
            })
        }
        C::SkinResetSelectedVerts => edit(state, "Reset Selected Verts", |data, mesh, st| {
            let verts = selected_verts(st, mesh.vertex_count());
            if verts.is_empty() {
                return Err("select vertices first".into());
            }
            for v in verts {
                data.explicit[v] = false;
            }
            data.refresh_envelope_weights(mesh);
            Ok(())
        }),
        C::SkinResetSelectedBone => edit(state, "Reset Selected Bone", |data, mesh, st| {
            let b = current_bone_index(st, data)? as u32;
            for v in 0..mesh.vertex_count() {
                if data.explicit[v] && weight_of(&current_weights(data, mesh, v), b) > 0.0 {
                    data.explicit[v] = false;
                }
            }
            data.refresh_envelope_weights(mesh);
            Ok(())
        }),
        C::SkinResetAllBones => edit(state, "Reset All Bones", |data, mesh, _| {
            data.explicit.iter_mut().for_each(|x| *x = false);
            data.refresh_envelope_weights(mesh);
            Ok(())
        }),
        C::SkinSaveEnvelopes { path } => {
            let Some(path) = path else {
                entry(state)?;
                state.dialog = Some(Dialog::SkinSaveEnvelopes);
                return Ok(json!({ "dialog": "SkinSaveEnvelopes" }));
            };
            let e = entry(state)?;
            let file = env_file(state, &e);
            let text = serde_json::to_string_pretty(&file).map_err(|x| x.to_string())?;
            std::fs::write(&path, text).map_err(|x| format!("{path}: {x}"))?;
            Ok(json!({ "path": path, "bones": file.bones.len(), "vertices": file.vertices.len() }))
        }
        C::SkinLoadEnvelopes { path } => {
            let Some(path) = path else {
                entry(state)?;
                state.dialog = Some(Dialog::SkinLoadEnvelopes);
                return Ok(json!({ "dialog": "SkinLoadEnvelopes" }));
            };
            entry(state)?;
            let text = std::fs::read_to_string(&path).map_err(|x| format!("{path}: {x}"))?;
            let file: EnvFile = serde_json::from_str(&text).map_err(|x| format!("{path}: not an envelope file ({x})"))?;
            let names: Vec<String> = file.bones.iter().map(|b| b.name.clone()).collect();
            state.skin_ui.load = Some(file);
            state.dialog = Some(Dialog::LoadEnvelopes);
            Ok(json!({ "dialog": "LoadEnvelopes", "incoming": names }))
        }
        C::SkinApplyLoadedEnvelopes { incoming, end_points, cross_sections, vertex_data, exclusions, by_index } => {
            let file = state.skin_ui.load.clone().ok_or("no envelope file loaded (Load first)")?;
            let r = edit(state, "Load Envelopes", |data, mesh, _| {
                apply_env(data, mesh, &file, &incoming, end_points, cross_sections, vertex_data, exclusions, by_index)?;
                Ok(())
            })?;
            state.skin_ui.load = None;
            Ok(r)
        }
        C::SkinSelectCrossSection { index, outer } => {
            let e = entry(state)?;
            let i = current_bone_index(state, &e.data)?;
            let n = e.data.bones[i].cross_sections.len() + 2;
            if index as usize >= n {
                return Err(format!("the envelope has {n} cross sections"));
            }
            state.skin_ui.cross_section = Some((index as usize, outer));
            state.skin_handle = None;
            Ok(status(state))
        }
        C::SkinSetCrossSectionRadius { radius } => {
            let (k, outer) = state.skin_ui.cross_section.ok_or("select a cross section first")?;
            edit(state, "Cross Section Radius", |data, mesh, st| {
                let i = current_bone_index(st, data)?;
                let b = &mut data.bones[i];
                let n = b.cross_sections.len() + 2;
                if k == 0 || k + 1 == n {
                    b.envelope.set_radius(if k == 0 { 0 } else { 1 }, outer, radius);
                } else if let Some(cs) = b.cross_sections.get_mut(k - 1) {
                    if outer { cs.2 = radius.max(0.0) } else { cs.1 = radius.max(0.0) }
                } else {
                    return Err("no such cross section".into());
                }
                data.refresh_envelope_weights(mesh);
                Ok(())
            })
        }
        C::SkinAddCrossSectionAt { u } => {
            let mut index = 0;
            let r = edit(state, "Add Cross Section", |data, mesh, st| {
                let i = current_bone_index(st, data)?;
                let b = &mut data.bones[i];
                let u = u.clamp(0.001, 0.999);
                let (inner, outer) = loom_scene::skin::radius_profile(&b.envelope, &b.cross_sections, u);
                let at = b.cross_sections.iter().position(|c| c.0 > u).unwrap_or(b.cross_sections.len());
                b.cross_sections.insert(at, (u, inner, outer));
                index = at + 1;
                data.refresh_envelope_weights(mesh);
                Ok(())
            })?;
            state.skin_ui.adding_cross_section = false;
            state.skin_ui.cross_section = Some((index, false));
            Ok(r)
        }
        C::SkinMoveEnvelopePoint { end, position } => edit(state, "Move Envelope", |data, mesh, st| {
            let i = current_bone_index(st, data)?;
            // The envelope is shown on the bone as it is now: back through the bone's current matrix.
            let now = st.scene.world_matrix(data.bones[i].node).inverse();
            let b = &mut data.bones[i];
            let local = now.transform_point3(Vec3::from(position));
            move_envelope_point(&mut b.envelope, end.min(1) as usize, local);
            data.refresh_envelope_weights(mesh);
            Ok(())
        }),
        C::SkinSelectElementToggle => {
            state.skin_ui.select_element = !state.skin_ui.select_element;
            Ok(status(state))
        }
        C::PasteGreenToBlueBones | C::PasteBlueToGreenBones => {
            let green_to_blue = matches!(cmd, C::PasteGreenToBlueBones);
            edit(state, "Mirror Paste Bones", |data, mesh, st| {
                let names = bone_names(st, data);
                if loom_scene::skin::mirror_envelopes(data, &names, green_to_blue) == 0 {
                    return Err("no bone on that side has a partner across the mirror plane".into());
                }
                data.refresh_envelope_weights(mesh);
                Ok(())
            })
        }
        C::SkinMirrorUpdate => {
            let e = entry(state)?;
            let sides = compute_mirror_sides(&e);
            state.skin_ui.mirror_sides = Some(((e.id, e.index), sides));
            state.mesh_dirty = true;
            Ok(status(state))
        }
        C::SkinCopyGizmo => {
            let e = entry(state)?;
            let g = state.skin_gizmo.and_then(|g| e.data.gizmos.get(g).cloned()).ok_or("select a gizmo first")?;
            state.skin_ui.clipboard_gizmo = Some(g);
            Ok(status(state))
        }
        C::SkinPasteGizmo => {
            let src = state.skin_ui.clipboard_gizmo.clone().ok_or("no gizmo copied")?;
            let g = state.skin_gizmo.ok_or("select a gizmo first")?;
            edit(state, "Paste Gizmo", |data, _, _| {
                let dst = data.gizmos.get_mut(g).ok_or("no such gizmo")?;
                if dst.kind != src.kind {
                    return Err(format!("a {} pastes only onto a {}", src.kind.label(), src.kind.label()));
                }
                dst.keys = src.keys.clone();
                dst.influence = src.influence;
                dst.twist = src.twist;
                dst.bounding_volume = src.bounding_volume;
                if dst.lattice.as_ref().map(|l| l.dims) != src.lattice.as_ref().map(|l| l.dims) {
                    // Lattice offsets belong to a lattice of the same size.
                    dst.lattice = src.lattice.clone();
                }
                Ok(())
            })
        }
        C::SkinRenameGizmo { name } => {
            let g = state.skin_gizmo.ok_or("select a gizmo first")?;
            edit(state, "Rename Gizmo", |data, _, _| {
                data.gizmos.get_mut(g).ok_or("no such gizmo")?.name = name.clone();
                Ok(())
            })
        }
        C::SkinGizmoAddFromStack | C::SkinGizmoAddFromNode { .. } => {
            let g = state.skin_gizmo.ok_or("select a Morph Angle gizmo first")?;
            let e = entry(state)?;
            let gizmo = e.data.gizmos.get(g).cloned().ok_or("no such gizmo")?;
            if gizmo.kind != loom_scene::skin::GizmoKind::MorphAngle {
                return Err("Add From Stack / Node works on a Morph Angle Deformer".into());
            }
            let world = state.scene.object_matrix(e.id);
            // The target in world space: the stack's top or another object.
            let target: Vec<Vec3> = match &cmd {
                C::SkinGizmoAddFromNode { id } => {
                    let id = NodeId(*id);
                    state.scene.node(id).ok_or_else(|| format!("no node {}", id.0))?;
                    let m = state.scene.object_matrix(id);
                    state.scene.eval_mesh(id).value.mesh.vertices().iter().map(|v| m.transform_point3(v.position)).collect()
                }
                _ => state.scene.eval_mesh(e.id).value.mesh.vertices().iter().map(|v| world.transform_point3(v.position)).collect(),
            };
            let skinned: Vec<Vec3> = loom_scene::skin::weighted_points(&state.scene.evaluated(e.id, Some(e.index)).value).into_iter().map(|p| world.transform_point3(p)).collect();
            if target.len() != skinned.len() {
                return Err(format!("the target has {} vertices, the skinned mesh {}", target.len(), skinned.len()));
            }
            let scene = &state.scene;
            let wm = |id: NodeId| Some(scene.world_matrix(id));
            let angle = loom_scene::skin::bone_angle(&e.data, &wm, gizmo.bone, gizmo.parent_bone).unwrap_or(0.0);
            let bone_world = scene.world_matrix(e.data.bones[gizmo.bone].node);
            // The gizmo's own share at this angle is in the skinned result: take it out first.
            let blend = loom_scene::skin::gizmos::key_blend(&gizmo.keys, angle);
            let mut own = vec![Vec3::ZERO; skinned.len()];
            for (w, k) in &blend {
                for (v, d) in &gizmo.keys[*k].deltas {
                    if let Some(o) = own.get_mut(*v as usize) {
                        *o += bone_world.transform_vector3(*d) * *w * gizmo.influence;
                    }
                }
            }
            let to_bone = bone_world.inverse();
            let members: Vec<usize> = if gizmo.vertices.is_empty() { (0..skinned.len()).collect() } else { gizmo.vertices.iter().map(|v| *v as usize).filter(|v| *v < skinned.len()).collect() };
            let influence = gizmo.influence.max(1e-6);
            let deltas: Vec<(u32, Vec3)> = members
                .into_iter()
                .filter_map(|v| {
                    let d = target[v] - (skinned[v] - own[v]);
                    (d.length_squared() > 1e-12).then(|| (v as u32, to_bone.transform_vector3(d) / influence))
                })
                .collect();
            let count = deltas.len();
            edit(state, "Add Morph Target", |data, _, _| {
                let gz = data.gizmos.get_mut(g).ok_or("no such gizmo")?;
                let n = gz.keys.len();
                match gz.keys.iter_mut().find(|k| (k.angle_deg - angle).abs() < 0.5) {
                    Some(k) => k.deltas = deltas.clone(),
                    None => gz.keys.push(loom_scene::skin::GizmoKey { angle_deg: angle, deltas: deltas.clone(), name: format!("Target {n:02}"), ..Default::default() }),
                }
                Ok(())
            })?;
            Ok(json!({ "angle": angle, "vertices": count }))
        }
        C::SkinGizmoDeleteTarget { key } => {
            let g = state.skin_gizmo.ok_or("select a gizmo first")?;
            edit(state, "Delete Morph Target", |data, _, _| {
                let gz = data.gizmos.get_mut(g).ok_or("no such gizmo")?;
                if key as usize >= gz.keys.len() {
                    return Err(format!("no target {key}"));
                }
                if gz.keys.len() == 1 {
                    return Err("the gizmo keeps at least one target".into());
                }
                gz.keys.remove(key as usize);
                Ok(())
            })
        }
        C::SkinGizmoRenameTarget { key, name } => {
            let g = state.skin_gizmo.ok_or("select a gizmo first")?;
            edit(state, "Rename Morph Target", |data, _, _| {
                let k = data.gizmos.get_mut(g).and_then(|gz| gz.keys.get_mut(key as usize)).ok_or_else(|| format!("no target {key}"))?;
                k.name = name.clone();
                Ok(())
            })
        }
        C::SkinEditLatticeToggle => {
            let e = entry(state)?;
            let g = state.skin_gizmo.and_then(|g| e.data.gizmos.get(g)).ok_or("select a gizmo first")?;
            if g.lattice.is_none() {
                return Err("this gizmo has no lattice (Joint Angle and Bulge Angle deformers do)".into());
            }
            state.skin_ui.edit_lattice = !state.skin_ui.edit_lattice;
            state.skin_ui.lattice_selection.clear();
            state.skin_handle = None;
            state.skin_ui.cross_section = None;
            Ok(status(state))
        }
        C::SkinSelectLatticePoints { indices, op } => {
            let e = entry(state)?;
            let lat = state.skin_gizmo.and_then(|g| e.data.gizmos.get(g)).and_then(|g| g.lattice.clone()).ok_or("the current gizmo has no lattice")?;
            let n = lat.count() as u32;
            if let Some(bad) = indices.iter().find(|i| **i >= n) {
                return Err(format!("no lattice point {bad} (the lattice has {n})"));
            }
            let sel = &mut state.skin_ui.lattice_selection;
            match op {
                SelectOp::Replace => *sel = indices.into_iter().collect(),
                SelectOp::Add => sel.extend(indices),
                SelectOp::Subtract => sel.retain(|i| !indices.contains(i)),
            }
            Ok(status(state))
        }
        C::SkinSetLatticeOffset { key, point, offset } => {
            let g = state.skin_gizmo.ok_or("select a gizmo first")?;
            edit(state, "Edit Lattice", |data, _, _| {
                let gz = data.gizmos.get_mut(g).ok_or("no such gizmo")?;
                let n = gz.lattice.as_ref().ok_or("the gizmo has no lattice")?.count();
                if point as usize >= n {
                    return Err(format!("no lattice point {point}"));
                }
                let k = gz.keys.get_mut(key as usize).ok_or_else(|| format!("no key {key}"))?;
                k.lattice.resize(n, Vec3::ZERO);
                k.lattice[point as usize] = Vec3::from(offset);
                Ok(())
            })
        }
        C::SkinAngleKeysCurvesDialog => {
            let e = entry(state)?;
            state.skin_gizmo.and_then(|g| e.data.gizmos.get(g)).and_then(|g| g.lattice.as_ref()).ok_or("the current gizmo has no lattice")?;
            state.dialog = Some(Dialog::AngleKeysCurves);
            Ok(json!({ "dialog": "AngleKeysCurves" }))
        }
        C::SkinHighlightVertex { vertex } => {
            state.skin_ui.highlight = vertex;
            Ok(status(state))
        }
        other => Err(format!("{} is not a Skin command", other.name())),
    }
}

/// The vertices of every element the given vertices belong to.
fn element_vertices(m: &Mesh, ids: &[u32]) -> Vec<u32> {
    let faces: Vec<u32> = ids.iter().flat_map(|v| select_ops::vertex_edges(m, VertexId(*v)).into_iter().filter_map(|he| m.half_edges()[he.0 as usize].face.map(|f| f.0))).collect();
    let mut out: BTreeSet<u32> = ids.iter().copied().collect();
    for f in faces {
        for ef in select_ops::element_of(m, loom_mesh::FaceId(f)) {
            out.extend(m.face_vertex_ids(ef).unwrap_or_default().into_iter().map(|v| v.0));
        }
    }
    out.into_iter().collect()
}

/// Moves an envelope's start (0) or end (1) point; a curved envelope's path end goes along.
fn move_envelope_point(e: &mut Envelope, end: usize, local: Vec3) {
    if end == 0 {
        e.start = local;
        if let Some(p) = e.path.first_mut() {
            *p = local;
        }
    } else {
        e.end = local;
        if let Some(p) = e.path.last_mut() {
            *p = local;
        }
    }
}

/// The current Skin as an envelope file.
fn env_file(state: &AppState, e: &Entry) -> EnvFile {
    let names = bone_names(state, &e.data);
    let bones = e.data.bones.iter().enumerate().map(|(i, b)| EnvBone { name: names[i].clone(), envelope: b.envelope.clone(), cross_sections: b.cross_sections.clone(), excluded: e.data.excluded.get(i).map(|x| x.iter().copied().collect()).unwrap_or_default() }).collect();
    let vertices = (0..e.input.vertex_count())
        .map(|v| EnvVertex { position: e.input.vertices()[v].position.to_array(), weights: current_weights(&e.data, &e.input, v), explicit: e.data.explicit.get(v).copied().unwrap_or(false), rigid: e.data.rigid.get(v).copied().unwrap_or(false) })
        .collect();
    EnvFile { format: ENV_FORMAT.into(), bones, vertices }
}

/// Load Envelopes > OK: `incoming[i]` is the file bone loaded onto current bone `i` (-1 none).
#[allow(clippy::too_many_arguments)]
pub fn apply_env(data: &mut SkinData, mesh: &Mesh, file: &EnvFile, incoming: &[i32], end_points: bool, cross_sections: bool, vertex_data: bool, exclusions: bool, by_index: bool) -> Result<(), String> {
    let mut to_current: Vec<Option<usize>> = vec![None; file.bones.len()];
    for (ci, inc) in incoming.iter().enumerate() {
        if *inc < 0 || ci >= data.bones.len() {
            continue;
        }
        let fb = file.bones.get(*inc as usize).ok_or_else(|| format!("the file has no bone {inc}"))?;
        to_current[*inc as usize] = Some(ci);
        let b = &mut data.bones[ci];
        let (start, end, path) = (b.envelope.start, b.envelope.end, b.envelope.path.clone());
        b.envelope = fb.envelope.clone();
        if !end_points {
            // The envelope's shape loads; its end points stay where they are.
            b.envelope.start = start;
            b.envelope.end = end;
            b.envelope.path = path;
        }
        if cross_sections {
            b.cross_sections = fb.cross_sections.clone();
        }
        if exclusions {
            data.excluded[ci] = fb.excluded.iter().copied().collect();
        }
    }
    if vertex_data {
        let n = mesh.vertex_count();
        data.resize(n);
        let nearest = |p: Vec3| file.vertices.iter().enumerate().min_by(|a, b| Vec3::from(a.1.position).distance(p).total_cmp(&Vec3::from(b.1.position).distance(p))).map(|(i, _)| i);
        for v in 0..n {
            let src = if by_index { (v < file.vertices.len()).then_some(v) } else { nearest(mesh.vertices()[v].position) };
            let Some(fv) = src.and_then(|i| file.vertices.get(i)) else { continue };
            if !fv.explicit {
                continue;
            }
            let mut w: Vec<(u32, f32)> = fv.weights.iter().filter_map(|(b, x)| to_current.get(*b as usize).copied().flatten().map(|c| (c as u32, *x))).collect();
            normalize(&mut w);
            if !w.is_empty() {
                data.weights[v] = w;
                data.explicit[v] = true;
                data.rigid[v] = fv.rigid;
            }
        }
    }
    data.refresh_envelope_weights(mesh);
    Ok(())
}

/// Mirror Mode side of every weighted vertex (of the mesh below the Skin, in its own space).
fn compute_mirror_sides(e: &Entry) -> Vec<i8> {
    let m = &e.data.mirror;
    let pts: Vec<Vec3> = e.input.vertices().iter().map(|v| v.position).collect();
    loom_scene::skin::mirror_sides(&pts, m.plane, m.offset, m.threshold)
}

/// Mirror Mode on: each vertex's side (-1 green, 1 blue, 0 red); with Manual
/// Update the sides as of the last Update.
pub fn mirror_sides(state: &AppState) -> Option<Vec<i8>> {
    let e = entry(state).ok()?;
    if !e.data.mirror.mode {
        return None;
    }
    if e.data.mirror.manual_update {
        if let Some((owner, sides)) = &state.skin_ui.mirror_sides {
            if *owner == (e.id, e.index) && sides.len() == e.input.vertex_count() {
                return Some(sides.clone());
            }
        }
    }
    Some(compute_mirror_sides(&e))
}

/// Mirror Mode: the vertex across the plane from each selected vertex (Display Projection Default).
pub fn mirror_partners(state: &AppState) -> BTreeSet<u32> {
    let Ok(e) = entry(state) else { return BTreeSet::new() };
    let m = &e.data.mirror;
    if !m.mode || m.projection != 0 {
        return BTreeSet::new();
    }
    let a = m.plane.min(2) as usize;
    let pts: Vec<Vec3> = e.input.vertices().iter().map(|v| v.position).collect();
    selection(state)
        .into_iter()
        .filter_map(|v| {
            let mut q = pts[v as usize];
            q[a] = 2.0 * m.offset - q[a];
            pts.iter().enumerate().filter(|(i, _)| *i != v as usize).map(|(i, p)| (i, p.distance(q))).filter(|(_, d)| *d <= m.threshold).min_by(|x, y| x.1.total_cmp(&y.1)).map(|(i, _)| i as u32)
        })
        .collect()
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
            "bones": e.data.bones.iter().map(|b| json!({ "node": b.node.0, "name": state.scene.node(b.node).map(|n| n.name.clone()).unwrap_or_default(), "inner": b.envelope.radii().0, "outer": b.envelope.radii().1, "falloff": b.envelope.falloff, "relative": b.envelope.relative, "cross_sections": b.cross_sections.len() })).collect::<Vec<_>>(),
            "cross_section": state.skin_ui.cross_section.map(|(k, outer)| json!({ "index": k, "outer": outer })),
            "adding_cross_section": state.skin_ui.adding_cross_section,
            "select_element": state.skin_ui.select_element,
            "edit_lattice": state.skin_ui.edit_lattice,
            "lattice_selection": state.skin_ui.lattice_selection.iter().collect::<Vec<_>>(),
            "vertex_sets": e.data.vertex_sets.iter().map(|(n, v)| json!({ "name": n, "count": v.len() })).collect::<Vec<_>>(),
            "highlight": state.skin_ui.highlight,
            "bone": state.skin_bone.map(|b| b.0),
            "selection": selection(state),
            "edit_envelopes": state.skin_active(),
            "vertices": e.data.weights.len(),
            "explicit": e.data.explicit.iter().filter(|x| **x).count(),
            "skin_pose": e.data.skin_pose.is_some(),
            "skin_pose_mode": state.skin_pose_live.is_some(),
            "gizmos": e.data.gizmos.iter().map(|g| json!({ "kind": format!("{:?}", g.kind), "name": g.name, "bone": g.bone, "parent": g.parent_bone, "keys": g.keys.len(), "vertices": g.vertices.len(), "twist": g.twist, "bounding_volume": g.bounding_volume, "lattice": g.lattice.as_ref().map(|l| l.count()) })).collect::<Vec<_>>(),
            "custom_weight": state.skin_custom_weight,
            "scale_weight": state.skin_scale_weight,
            "paste_tolerance": state.skin_paste_tolerance,
            "paint_weights": state.skin_paint,
            "painter": { "max_strength": state.skin_painter.strength, "max_size": state.skin_painter.size, "min_strength": state.skin_painter.min_strength, "min_size": state.skin_painter.min_size, "pressure": state.skin_painter.pressure_on, "pressure_affects": state.skin_painter.pressure_affects, "curve": state.skin_painter.curve, "additive": state.skin_painter.additive, "falloff": state.skin_painter.falloff, "draw_ring": state.skin_painter.draw_ring, "mirror": state.skin_painter.mirror, "mirror_axis": state.skin_painter.mirror_axis, "mirror_offset": state.skin_painter.mirror_offset, "paint_blend_weights": state.skin_painter.blend },
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

/// Skin > Edit Envelopes owns the viewport clicks: vertices (with Select
/// Vertices on) and bones; a click on nothing clears the vertex selection.
pub fn picks_vertices(state: &AppState) -> bool {
    state.skin_active() && state.selection_mode == SelectionMode::Object
}

/// Paint Weights is on and Edit Envelopes active.
pub fn painting(state: &AppState) -> bool {
    state.skin_paint && state.skin_active()
}

/// Painter Options (Min / Max Strength and Size, the falloff graph,
/// pressure, mirror) and Paint Blend Weights.
#[derive(Debug, Clone, PartialEq)]
pub struct Painter {
    /// Max. Strength.
    pub strength: f32,
    /// Max. Size: brush radius in world units.
    pub size: f32,
    pub additive: bool,
    /// 0 Linear, 1 Smooth, 2 Slow, 3 Fast, 4 Flat, 5 the graph edited by hand (`curve`).
    pub falloff: u32,
    pub draw_ring: bool,
    pub mirror: bool,
    pub mirror_axis: u32,
    pub mirror_offset: f32,
    pub blend: bool,
    /// Min. Strength / Min. Size: what a light pen pressure gives.
    pub min_strength: f32,
    pub min_size: f32,
    /// Enable Pressure Sensitivity and Pressure Affects: 0 None, 1 Strength, 2 Size, 3 Size and Strength.
    pub pressure_on: bool,
    pub pressure_affects: u32,
    /// The falloff graph: (distance from the centre 0..1, strength 0..1), sorted.
    pub curve: Vec<[f32; 2]>,
}

impl Default for Painter {
    fn default() -> Self {
        Self { strength: 1.0, size: 10.0, additive: true, falloff: 1, draw_ring: true, mirror: false, mirror_axis: 0, mirror_offset: 0.0, blend: true, min_strength: 0.0, min_size: 0.0, pressure_on: false, pressure_affects: 1, curve: Self::preset_curve(1) }
    }
}

impl Painter {
    /// The falloff graph edited by hand.
    pub const CUSTOM: u32 = 5;

    /// A falloff button's curve formula at `t` = distance / radius.
    fn preset_at(falloff: u32, t: f32) -> f32 {
        match falloff {
            0 => 1.0 - t,
            2 => 1.0 - t * t * t,
            3 => (1.0 - t) * (1.0 - t),
            4 => 1.0,
            _ => 1.0 - t * t * (3.0 - 2.0 * t),
        }
    }

    /// The graph a falloff button sets: its curve sampled at nine points.
    pub fn preset_curve(falloff: u32) -> Vec<[f32; 2]> {
        (0..=8).map(|i| i as f32 / 8.0).map(|t| [t, Self::preset_at(falloff, t)]).collect()
    }

    /// Brush weight at `t` = distance / radius (0 centre, 1 rim).
    pub fn falloff_at(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        if self.falloff != Self::CUSTOM || self.curve.len() < 2 {
            return Self::preset_at(self.falloff, t);
        }
        let c = &self.curve;
        if t <= c[0][0] {
            return c[0][1];
        }
        for w in c.windows(2) {
            if t <= w[1][0] {
                let k = if w[1][0] - w[0][0] < 1e-6 { 0.0 } else { (t - w[0][0]) / (w[1][0] - w[0][0]) };
                return w[0][1] + (w[1][1] - w[0][1]) * k;
            }
        }
        c[c.len() - 1][1]
    }

    /// Strength and radius of a dab at pen `pressure` (0..1): with Enable
    /// Pressure Sensitivity the affected values run from Min to Max.
    pub fn dab(&self, pressure: f32) -> (f32, f32) {
        let p = pressure.clamp(0.0, 1.0);
        if !self.pressure_on {
            return (self.strength, self.size);
        }
        let s = if matches!(self.pressure_affects, 1 | 3) { self.min_strength + (self.strength - self.min_strength) * p } else { self.strength };
        let r = if matches!(self.pressure_affects, 2 | 3) { self.min_size + (self.size - self.min_size) * p } else { self.size };
        (s, r)
    }
}

/// World positions of the Skin's output vertices (what the viewport shows;
/// on a patch its vertices then handles).
pub fn vertex_world_positions(state: &AppState) -> Option<(NodeId, Vec<Vec3>)> {
    let (id, index) = state.skin_entry()?;
    let world = state.scene.object_matrix(id);
    Some((id, loom_scene::skin::weighted_points(&state.scene.evaluated(id, Some(index)).value).into_iter().map(|p| world.transform_point3(p)).collect()))
}

/// Area-weighted vertex normals of the Skin's output in world space (Backface Cull Vertices).
pub fn vertex_world_normals(state: &AppState) -> Option<Vec<Vec3>> {
    let (id, index) = state.skin_entry()?;
    let world = state.scene.object_matrix(id);
    let ev = state.scene.evaluated(id, Some(index));
    let cage = ev.value.patch.as_ref().map(|p| p.cage());
    let mesh = cage.as_ref().unwrap_or(&ev.value.mesh);
    let pos: Vec<Vec3> = mesh.vertices().iter().map(|v| world.transform_point3(v.position)).collect();
    let mut n = vec![Vec3::ZERO; pos.len()];
    for f in 0..mesh.face_count() {
        let ids = mesh.face_vertex_ids(loom_mesh::FaceId(f as u32)).unwrap_or_default();
        for k in 1..ids.len().saturating_sub(1) {
            let (a, b, c) = (ids[0].0 as usize, ids[k].0 as usize, ids[k + 1].0 as usize);
            let face = (pos[b] - pos[a]).cross(pos[c] - pos[a]);
            for v in [a, b, c] {
                n[v] += face;
            }
        }
    }
    Some(n.into_iter().map(|x| x.normalize_or_zero()).collect())
}

/// Paint Weights: a dab at each world point (and its mirror with Painter
/// Options > Mirror), its strength and size from the pen pressure.
fn paint(state: &mut AppState, points: &[[f32; 3]], erase: bool, pressure: Option<&[f32]>) -> Result<Value, String> {
    let (_, world) = vertex_world_positions(state).ok_or("no Skin on the selection (add the modifier first)")?;
    let p = state.skin_painter.clone();
    // (centre, strength, radius) per dab.
    let mut dabs: Vec<(Vec3, f32, f32)> = points
        .iter()
        .enumerate()
        .map(|(i, x)| {
            let (s, r) = p.dab(pressure.and_then(|pr| pr.get(i)).copied().unwrap_or(1.0));
            (Vec3::from(*x), s, r)
        })
        .collect();
    if p.mirror {
        let a = p.mirror_axis.min(2) as usize;
        let mirrored: Vec<(Vec3, f32, f32)> = dabs
            .iter()
            .map(|(c, s, r)| {
                let mut m = *c;
                m[a] = 2.0 * p.mirror_offset - m[a];
                (m, *s, *r)
            })
            .collect();
        dabs.extend(mirrored);
    }
    // Per vertex the strongest dab: (strength × brush weight, brush weight, dab strength).
    let mut hit: std::collections::BTreeMap<usize, (f32, f32, f32)> = std::collections::BTreeMap::new();
    for (c, s, r) in &dabs {
        if *r <= 0.0 {
            continue;
        }
        for (v, q) in world.iter().enumerate() {
            let d = q.distance(*c);
            if d <= *r {
                let f = p.falloff_at(d / r);
                let e = hit.entry(v).or_insert((0.0, 0.0, 0.0));
                if s * f > e.0 {
                    *e = (s * f, f, *s);
                }
            }
        }
    }
    if hit.is_empty() {
        return Ok(json!({ "painted": 0 }));
    }
    let painted = hit.len();
    edit(state, "Paint Weights", |data, mesh, st| {
        let bone = current_bone_index(st, data)?;
        let b = bone as u32;
        let n = mesh.vertex_count();
        let before: Vec<f32> = (0..n).map(|v| weight_of(&current_weights(data, mesh, v), b)).collect();
        let mut after = before.clone();
        let paintable = |v: usize| v < n && !data.excluded[bone].contains(&(v as u32));
        for (v, (amount, f, s)) in hit.iter().filter(|(v, _)| paintable(**v)) {
            let cur = before[*v];
            let w = if p.additive {
                if erase { cur - amount } else { cur + amount }
            } else {
                // Not additive: the stroke moves the weight towards the dab's strength (or 0).
                let target = if erase { 0.0 } else { *s };
                cur + (target - cur) * f
            };
            after[*v] = w.clamp(0.0, 1.0);
        }
        if p.blend {
            // Paint Blend Weights: each painted vertex moves towards its neighbourhood's average, by the brush weight.
            let painted = after.clone();
            for (v, (_, f, _)) in hit.iter().filter(|(v, _)| paintable(**v)) {
                let ring: Vec<usize> = select_ops::vertex_neighbours(mesh, VertexId(*v as u32)).into_iter().map(|x| x.0 as usize).chain(std::iter::once(*v)).collect();
                let avg = ring.iter().map(|i| painted[*i]).sum::<f32>() / ring.len() as f32;
                after[*v] = painted[*v] + (avg - painted[*v]) * f * 0.5;
            }
        }
        let changed: Vec<usize> = hit.keys().copied().filter(|v| paintable(*v) && (after[*v] - before[*v]).abs() > 1e-6).collect();
        for v in changed {
            let cur = current_weights(data, mesh, v);
            set_bone_weight(data, mesh, v, b, after[v], &cur);
        }
        Ok(())
    })?;
    Ok(json!({ "painted": painted }))
}

/// Bone name field: case-insensitive prefix, `*` matches any run of characters.
fn name_matches(pattern: &str, name: &str) -> bool {
    let (p, n) = (pattern.to_lowercase(), name.to_lowercase());
    let mut rest = n.as_str();
    for (k, part) in p.split('*').enumerate() {
        if k == 0 {
            match rest.strip_prefix(part) {
                Some(r) => rest = r,
                None => return false,
            }
        } else if let Some(i) = rest.find(part) {
            rest = &rest[i + part.len()..];
        } else {
            return false;
        }
    }
    true
}

/// Per-vertex weight of the current bone, for the viewport colours.
pub fn current_bone_weights(state: &AppState) -> Option<(NodeId, Vec<f32>)> {
    let e = entry(state).ok()?;
    let i = current_bone_index(state, &e.data).ok()? as u32;
    Some((e.id, (0..e.input.vertex_count()).map(|v| weight_of(&current_weights(&e.data, &e.input, v), i)).collect()))
}

/// An envelope capsule in world space.
pub struct EnvelopeWorld {
    /// The current bone's (drawn with its handles and cross sections).
    pub current: bool,
    pub start: Vec3,
    pub end: Vec3,
    pub inner: [f32; 2],
    pub outer: [f32; 2],
    pub cross_sections: Vec<(f32, f32, f32)>,
    /// A curved (spline) envelope's axis in world space; empty = straight.
    pub path: Vec<Vec3>,
    /// The bone's index in the Skin's list.
    pub bone: usize,
}

impl EnvelopeWorld {
    /// The axis point at `u` (0 start … 1 end, by length).
    pub fn point(&self, u: f32) -> Vec3 {
        if self.path.len() < 2 {
            return self.start.lerp(self.end, u);
        }
        let e = Envelope { path: self.path.clone(), start: self.start, end: self.end, ..Envelope::default() };
        e.axis_point(u)
    }

    /// The axis direction at `u`.
    pub fn direction(&self, u: f32) -> Vec3 {
        let e = Envelope { path: self.path.clone(), start: self.start, end: self.end, ..Envelope::default() };
        e.axis_direction(u)
    }

    /// Inner and outer radius at `u` (cross sections between the ends).
    pub fn radii_at(&self, u: f32) -> (f32, f32) {
        let e = Envelope { inner: self.inner, outer: self.outer, ..Envelope::default() };
        loom_scene::skin::radius_profile(&e, &self.cross_sections, u)
    }

    /// The cross sections with the ends: (u, inner, outer).
    pub fn sections(&self) -> Vec<(f32, f32, f32)> {
        let mut out = vec![(0.0, self.inner[0], self.outer[0])];
        out.extend(self.cross_sections.iter().copied());
        out.push((1.0, self.inner[1], self.outer[1]));
        out
    }
}

/// Color All Weights: a colour per bone (golden-angle hues).
pub fn bone_color(i: usize) -> [u8; 3] {
    let h = (i as f32 * 0.618_034).fract() * 6.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    let (r, g, b) = match h as u32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    [(r * 230.0 + 20.0) as u8, (g * 230.0 + 20.0) as u8, (b * 230.0 + 20.0) as u8]
}

/// The colour of each weighted point at the Envelope level: the current
/// bone's weight (blue 0 → red 1), or with Color All Weights every bone's
/// colour blended by its weight.
pub fn point_colors(state: &AppState) -> Option<(NodeId, Vec<[u8; 3]>)> {
    let e = entry(state).ok()?;
    if e.data.display.color_all_weights {
        let colors = (0..e.input.vertex_count())
            .map(|v| {
                let mut c = [0.0f32; 3];
                for (b, w) in current_weights(&e.data, &e.input, v) {
                    let bc = bone_color(b as usize);
                    for k in 0..3 {
                        c[k] += bc[k] as f32 * w;
                    }
                }
                [c[0].min(255.0) as u8, c[1].min(255.0) as u8, c[2].min(255.0) as u8]
            })
            .collect();
        return Some((e.id, colors));
    }
    let (id, w) = current_bone_weights(state)?;
    Some((id, w.into_iter().map(weight_color).collect()))
}

/// Display > Show Colored Faces at the Envelope level: the skinned node and
/// a colour per vertex of the mesh the Skin puts out (a patch's
/// tessellation takes its lattice's colours across the surface).
pub fn colored_faces(state: &AppState) -> Option<(NodeId, Vec<[u8; 3]>)> {
    if !state.skin_active() || !entry(state).ok()?.data.display.colored_faces {
        return None;
    }
    let (id, colors) = point_colors(state)?;
    let (_, index) = state.skin_entry()?;
    let ev = state.scene.evaluated(id, Some(index));
    match (&ev.value.patch, ev.value.patch_steps) {
        (Some(p), steps) => {
            // Tessellation is linear in the lattice points: tessellate the colours.
            let nv = p.verts.len();
            let mut q = p.clone();
            for (i, c) in colors.iter().enumerate() {
                let x = Vec3::new(c[0] as f32, c[1] as f32, c[2] as f32);
                if i < nv {
                    q.verts[i] = x;
                } else if let Some(h) = q.vecs.get_mut(i - nv) {
                    *h = x;
                }
            }
            let (steps, interior) = steps.unwrap_or((5, false));
            let t = q.tessellate(steps, interior);
            Some((id, t.vertices().iter().map(|v| [v.position.x.clamp(0.0, 255.0) as u8, v.position.y.clamp(0.0, 255.0) as u8, v.position.z.clamp(0.0, 255.0) as u8]).collect()))
        }
        (None, _) => Some((id, colors)),
    }
}

/// Zoom Extents Selected / Orbit SubObject at the Envelope level: the
/// selected vertices, else the current envelope.
pub fn focus_bounds(state: &AppState) -> Option<(Vec3, Vec3)> {
    if !state.skin_active() {
        return None;
    }
    let sel = selection(state);
    let grow = |(lo, hi): (Vec3, Vec3), p: Vec3| (lo.min(p), hi.max(p));
    let empty = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    if !sel.is_empty() {
        let (_, world) = vertex_world_positions(state)?;
        let b = sel.iter().filter_map(|v| world.get(*v as usize)).fold(empty, |acc, p| grow(acc, *p));
        return b.0.is_finite().then_some(b);
    }
    let env = shown_envelopes_world(state).into_iter().find(|e| e.current)?;
    let r = env.outer[0].max(env.outer[1]);
    let b = [env.start, env.end].into_iter().fold(empty, |acc, p| grow(grow(acc, p - Vec3::splat(r)), p + Vec3::splat(r)));
    Some(b)
}

/// The world position of the current gizmo's lattice points now (rest + the offsets at the bones' angle).
pub fn lattice_world_points(state: &AppState) -> Option<Vec<Vec3>> {
    let e = entry(state).ok()?;
    let g = e.data.gizmos.get(state.skin_gizmo?)?;
    lattice_points_of(state, &e.data, g)
}

/// A gizmo's lattice points in world space now.
pub fn lattice_points_of(state: &AppState, data: &SkinData, g: &SkinGizmo) -> Option<Vec<Vec3>> {
    let lat = g.lattice.as_ref()?;
    let scene = &state.scene;
    let wm = |id: NodeId| Some(scene.world_matrix(id));
    let angle = loom_scene::skin::bone_angle(data, &wm, g.bone, g.parent_bone).unwrap_or(0.0);
    let blend = loom_scene::skin::gizmos::key_blend(&g.keys, angle);
    let offsets = loom_scene::skin::gizmos::lattice_offsets(g, &blend).unwrap_or_else(|| vec![Vec3::ZERO; lat.count()]);
    let frame = lat.frame(g.twist);
    let bw = scene.world_matrix(data.bones.get(g.lattice_bone())?.node);
    Some(lat.rest_points(g.twist).into_iter().zip(offsets).map(|(r, o)| bw.transform_point3(r + frame.transform_vector3(o))).collect())
}

/// Where the transform gizmo sits at the Envelope level: the selected
/// envelope end point, the selected lattice points, else the focus.
pub fn pivot(state: &AppState) -> Option<Vec3> {
    if !state.skin_active() {
        return None;
    }
    if let Some(h) = state.skin_handle {
        let env = shown_envelopes_world(state).into_iter().find(|e| e.current)?;
        return Some(if h == 0 { env.start } else { env.end });
    }
    if state.skin_ui.edit_lattice && !state.skin_ui.lattice_selection.is_empty() {
        let pts = lattice_world_points(state)?;
        let sel: Vec<Vec3> = state.skin_ui.lattice_selection.iter().filter_map(|i| pts.get(*i as usize).copied()).collect();
        return (!sel.is_empty()).then(|| sel.iter().copied().sum::<Vec3>() / sel.len() as f32);
    }
    focus_bounds(state).map(|(a, b)| (a + b) * 0.5)
}

/// The transform tools at the Envelope level: they move the selected
/// envelope end point or the selected lattice points (offsets keyed at
/// the bones' current angle), never the skinned object. `m` maps world to world.
pub fn transform(state: &mut AppState, label: &str, m: glam::Mat4) -> Option<Result<Value, String>> {
    if !state.skin_active() {
        return None;
    }
    if let Some(h) = state.skin_handle {
        let p = pivot(state)?;
        let to = m.transform_point3(p);
        return Some(execute(state, Command::SkinMoveEnvelopePoint { end: h as u32, position: to.to_array() }));
    }
    if state.skin_ui.edit_lattice && !state.skin_ui.lattice_selection.is_empty() {
        let g = state.skin_gizmo?;
        let sel: Vec<usize> = state.skin_ui.lattice_selection.iter().map(|i| *i as usize).collect();
        let pts = lattice_world_points(state)?;
        return Some(edit(state, label, |data, _, st| {
            let wm = |id: NodeId| Some(st.scene.world_matrix(id));
            let gz = data.gizmos.get(g).ok_or("no such gizmo")?.clone();
            let lat = gz.lattice.clone().ok_or("the gizmo has no lattice")?;
            let angle = loom_scene::skin::bone_angle(data, &wm, gz.bone, gz.parent_bone).unwrap_or(0.0);
            let blend = loom_scene::skin::gizmos::key_blend(&gz.keys, angle);
            let now = loom_scene::skin::gizmos::lattice_offsets(&gz, &blend).unwrap_or_else(|| vec![Vec3::ZERO; lat.count()]);
            let bw = st.scene.world_matrix(data.bones[gz.lattice_bone()].node);
            let to_box = lat.frame(gz.twist).inverse();
            let gzm = data.gizmos.get_mut(g).ok_or("no such gizmo")?;
            // Offsets key at the bones' current angle: the key there, or a new one holding the blend.
            let k = match gzm.keys.iter().position(|k| (k.angle_deg - angle).abs() < 0.5) {
                Some(k) => k,
                None => {
                    let bulge = blend.iter().map(|(w, k)| w * gzm.keys[*k].bulge).sum();
                    gzm.keys.push(loom_scene::skin::GizmoKey { angle_deg: angle, bulge, ..Default::default() });
                    gzm.keys.len() - 1
                }
            };
            let key = &mut gzm.keys[k];
            if key.lattice.len() != now.len() {
                key.lattice = now.clone();
            }
            for i in sel.iter().copied().filter(|i| *i < pts.len()) {
                let moved = m.transform_point3(pts[i]);
                key.lattice[i] += to_box.transform_vector3(bw.inverse().transform_vector3(moved - pts[i]));
            }
            Ok(())
        }));
    }
    // Nothing of the Skin's picked: the object stays where it is at this level.
    Some(Ok(json!({})))
}

/// The envelopes the viewport shows: the current bone's, the ones with
/// Envelope Visibility on, or all with Display > Show All Envelopes.
pub fn shown_envelopes_world(state: &AppState) -> Vec<EnvelopeWorld> {
    let Ok(e) = entry(state) else { return Vec::new() };
    let cur = current_bone_index(state, &e.data).ok();
    e.data
        .bones
        .iter()
        .enumerate()
        .filter(|(i, b)| Some(*i) == cur || b.envelope_visible || e.data.display.all_envelopes)
        .map(|(i, b)| {
            // The envelope moves with its bone (the bind-pose envelope carried by the bone's current matrix).
            let world = state.scene.world_matrix(b.node);
            let (inner, outer) = b.envelope.radii();
            EnvelopeWorld {
                current: Some(i) == cur,
                start: world.transform_point3(b.envelope.start),
                end: world.transform_point3(b.envelope.end),
                inner,
                outer,
                cross_sections: b.cross_sections.clone(),
                path: b.envelope.path.iter().map(|p| world.transform_point3(*p)).collect(),
                bone: i,
            }
        })
        .collect()
}
