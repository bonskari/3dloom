//! Editable Poly command handlers (the chart's "Editable Poly" group plus
//! the Loom helpers around it). `execute` delegates the `C::<poly>` arms here.

use crate::command::Command as C;
use crate::execute::{edit_meshes, selected_ids_raw, selection_json, set_sub_selection, sub_id};
use crate::state::{AppState, SelectionMode, SubObjectId};
use loom_mesh::select_ops::{self, SubLevel};
use loom_mesh::{FaceId, HalfEdgeId, VertexId};
use loom_scene::NodeId;
use serde_json::{json, Value};

pub fn level_of(mode: SelectionMode) -> Option<SubLevel> {
    Some(match mode {
        SelectionMode::Object => return None,
        SelectionMode::Vertex => SubLevel::Vertex,
        SelectionMode::Edge => SubLevel::Edge,
        SelectionMode::Border => SubLevel::Border,
        SelectionMode::Face => SubLevel::Face,
        SelectionMode::Element => SubLevel::Element,
    })
}

/// A level key toggles its level; the same key again returns to the top.
fn set_level(state: &mut AppState, mode: SelectionMode) -> Result<Value, String> {
    let mode = if state.selection_mode == mode { SelectionMode::Object } else { mode };
    crate::execute(state, C::SelectionMode { mode })
}

/// Replaces the sub-selection of every node with `f(mesh, current ids)`.
fn map_selection(state: &mut AppState, f: impl Fn(&loom_mesh::Mesh, SubLevel, Vec<u32>) -> Vec<u32>) -> Result<Value, String> {
    let mode = state.selection_mode;
    let Some(level) = level_of(mode) else { return Err("not in a sub-object level".into()) };
    let per_node: Vec<(NodeId, Vec<u32>)> = selected_ids_raw(state);
    let mut out: Vec<(NodeId, Vec<u32>)> = Vec::new();
    for (n, ids) in per_node {
        let mesh = state.scene.node(n).ok_or("missing node")?.cage_mesh().into_owned();
        let new = f(&mesh, level, ids);
        out.push((n, new));
    }
    set_sub_selection(state, mode, out);
    Ok(selection_json(state))
}

fn edge_step(state: &mut AppState, ring: bool, up: bool, op: StepOp) -> Result<Value, String> {
    let mode = state.selection_mode;
    if !mode.is_edge_level() {
        return Err("Edge or Border level required".into());
    }
    map_selection(state, |mesh, _, ids| {
        let edges: Vec<HalfEdgeId> = ids.iter().map(|i| HalfEdgeId(*i)).collect();
        let stepped: Vec<u32> = if ring { select_ops::ring_step(mesh, &edges, up) } else { select_ops::loop_step(mesh, &edges, up) }.into_iter().map(|e| e.0).collect();
        match op {
            StepOp::Replace => stepped,
            StepOp::Add => {
                let mut all = ids.clone();
                all.extend(stepped);
                all.sort();
                all.dedup();
                all
            }
            StepOp::Subtract => ids.into_iter().filter(|i| !stepped.contains(i)).collect(),
        }
    })
}

#[derive(Clone, Copy)]
enum StepOp {
    Replace,
    Add,
    Subtract,
}

/// Hide / Hide Unselected / Unhide All at the Vertex and Polygon levels.
fn hide(state: &mut AppState, which: Hide) -> Result<Value, String> {
    let mode = state.selection_mode;
    let targets: Vec<NodeId> = state.selection.iter().copied().collect();
    let per_node: std::collections::BTreeMap<NodeId, Vec<u32>> = selected_ids_raw(state).into_iter().collect();
    let items: Vec<(NodeId, Vec<u32>)> = targets.iter().map(|n| (*n, per_node.get(n).cloned().unwrap_or_default())).collect();
    edit_meshes(state, match which { Hide::Selected => "Hide", Hide::Unselected => "Hide Unselected", Hide::None => "Unhide All" }, items, |mesh, _, ids| {
        match (which, mode) {
            (Hide::None, _) => {
                for v in mesh.vertices_mut() {
                    v.hidden = false;
                }
                for f in mesh.faces_mut() {
                    f.hidden = false;
                }
            }
            (w, SelectionMode::Vertex) => {
                for (i, v) in mesh.vertices_mut().iter_mut().enumerate() {
                    let sel = ids.contains(&(i as u32));
                    if (w == Hide::Selected) == sel {
                        v.hidden = true;
                    }
                }
            }
            (w, SelectionMode::Face | SelectionMode::Element) => {
                for (i, f) in mesh.faces_mut().iter_mut().enumerate() {
                    let sel = ids.contains(&(i as u32));
                    if (w == Hide::Selected) == sel {
                        f.hidden = true;
                    }
                }
            }
            _ => return Err("Hide works at the Vertex and Polygon levels".into()),
        }
        Ok(())
    })?;
    if which != Hide::None {
        state.sub_selection.clear();
    }
    Ok(selection_json(state))
}

/// Main UI "Hide Unselected" / "Unhide All" at a sub-object level.
pub fn hide_unselected(state: &mut AppState) -> Result<Value, String> {
    hide(state, Hide::Unselected)
}

pub fn unhide_all(state: &mut AppState) -> Result<Value, String> {
    hide(state, Hide::None)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Hide {
    Selected,
    Unselected,
    None,
}

pub fn execute(state: &mut AppState, cmd: C) -> Result<Value, String> {
    match cmd {
        C::VertexLevel => set_level(state, SelectionMode::Vertex),
        C::EdgeLevel => set_level(state, SelectionMode::Edge),
        C::BorderLevel => set_level(state, SelectionMode::Border),
        C::FaceLevel => set_level(state, SelectionMode::Face),
        C::ElementLevel => set_level(state, SelectionMode::Element),
        C::ObjectLevel => crate::execute(state, C::SelectionMode { mode: SelectionMode::Object }),
        C::GrowSelection => map_selection(state, |m, l, ids| select_ops::grow(m, l, &ids)),
        C::ShrinkSelection => map_selection(state, |m, l, ids| select_ops::shrink(m, l, &ids)),
        C::SelectEdgeRing => crate::execute(state, C::EdgeRing),
        C::SelectEdgeLoop => crate::execute(state, C::EdgeLoop),
        C::SelectEdgeRingUp => edge_step(state, true, true, StepOp::Replace),
        C::SelectEdgeRingDown => edge_step(state, true, false, StepOp::Replace),
        C::SelectEdgeRingUpAdd => edge_step(state, true, true, StepOp::Add),
        C::SelectEdgeRingDownAdd => edge_step(state, true, false, StepOp::Add),
        C::SelectEdgeRingUpSubtract => edge_step(state, true, true, StepOp::Subtract),
        C::SelectEdgeRingDownSubtract => edge_step(state, true, false, StepOp::Subtract),
        C::SelectEdgeLoopUp => edge_step(state, false, true, StepOp::Replace),
        C::SelectEdgeLoopDown => edge_step(state, false, false, StepOp::Replace),
        C::SelectEdgeLoopUpAdd => edge_step(state, false, true, StepOp::Add),
        C::SelectEdgeLoopDownAdd => edge_step(state, false, false, StepOp::Add),
        C::SelectEdgeLoopUpSubtract => edge_step(state, false, true, StepOp::Subtract),
        C::SelectEdgeLoopDownSubtract => edge_step(state, false, false, StepOp::Subtract),
        C::SelectByVertex => {
            state.select_by_vertex = !state.select_by_vertex;
            Ok(json!({ "by_vertex": state.select_by_vertex }))
        }
        C::IgnoreBackfacingInSelections => {
            state.ignore_backfacing = !state.ignore_backfacing;
            Ok(json!({ "ignore_backfacing": state.ignore_backfacing }))
        }
        C::PreviewSelectMode => {
            state.preview_selection = state.preview_selection.next();
            Ok(json!({ "preview": format!("{:?}", state.preview_selection) }))
        }
        C::Hide => hide(state, Hide::Selected),
        C::AffectBackfacing => {
            let ids: Vec<NodeId> = state.selection.iter().copied().filter(|i| state.scene.node(*i).is_some_and(|n| n.edit_poly().is_some())).collect();
            let mut on = None;
            for id in ids {
                if let Some(p) = state.scene.node_mut(id).and_then(|n| n.edit_poly_mut()) {
                    p.soft.affect_backfacing = !p.soft.affect_backfacing;
                    on = Some(p.soft.affect_backfacing);
                }
            }
            Ok(json!({ "affect_backfacing": on }))
        }
        C::ConvertSelection { to, contained } => {
            let from = state.selection_mode;
            let Some(from_level) = level_of(from) else { return Err("not in a sub-object level".into()) };
            let Some(to_level) = level_of(to) else { return crate::execute(state, C::SelectionMode { mode: to }) };
            let per_node = selected_ids_raw(state);
            let mut out = Vec::new();
            for (n, ids) in per_node {
                let mesh = state.scene.node(n).ok_or("missing node")?.cage_mesh().into_owned();
                out.push((n, select_ops::convert_selection(&mesh, from_level, &ids, to_level, contained)));
            }
            state.selection_mode = to;
            set_sub_selection(state, to, out);
            Ok(selection_json(state))
        }
        C::UseSoftSelection => {
            if let Some(on) = spline_soft(state, |s| { s.on = !s.on; }) {
                return Ok(json!({ "on": on }));
            }
            soft_toggle(state, "Use Soft Selection", |p| { p.soft.on = !p.soft.on; p.soft.on })
        }
        C::LockSoftSelection => {
            // Locking snapshots the current weights so reselecting keeps them.
            let ids = selected_polys(state);
            let weights: Vec<(NodeId, Vec<f32>)> = ids.iter().map(|i| (*i, state.soft_weights(*i))).collect();
            let mut on = None;
            for (id, w) in weights {
                if let Some(p) = state.scene.node_mut(id).and_then(|n| n.edit_poly_mut()) {
                    p.soft.lock = !p.soft.lock;
                    p.soft.painted = if p.soft.lock { Some(w) } else { None };
                    on = Some(p.soft.lock);
                }
            }
            Ok(json!({ "lock": on }))
        }
        C::EditSoftSelectionMode | C::PaintSoftSelection => {
            state.soft_paint = if state.soft_paint == Some(crate::state::SoftPaint::Paint) { None } else { Some(crate::state::SoftPaint::Paint) };
            Ok(json!({ "paint": state.soft_paint.map(|p| format!("{p:?}")) }))
        }
        C::PaintSoftSelectionBlur => {
            state.soft_paint = if state.soft_paint == Some(crate::state::SoftPaint::Blur) { None } else { Some(crate::state::SoftPaint::Blur) };
            Ok(json!({ "paint": state.soft_paint.map(|p| format!("{p:?}")) }))
        }
        C::SoftSelectionRevert => {
            for id in selected_polys(state) {
                if let Some(p) = state.scene.node_mut(id).and_then(|n| n.edit_poly_mut()) {
                    let n = p.mesh.vertex_count();
                    p.soft.painted = Some(vec![0.0; n]);
                }
            }
            Ok(json!({}))
        }
        C::ShadedFaceToggle => {
            state.soft_shaded_faces = !state.soft_shaded_faces;
            Ok(json!({ "shaded_faces": state.soft_shaded_faces }))
        }
        C::SoftSelectionSettings { falloff, pinch, bubble, edge_distance, affect_backfacing } => {
            spline_soft(state, |s| {
                if let Some(f) = falloff { s.falloff.set(f.max(0.0)); }
                if let Some(v) = pinch { s.pinch.set(v.clamp(-1.0, 1.0)); }
                if let Some(v) = bubble { s.bubble.set(v.clamp(-1.0, 1.0)); }
                if let Some(e) = edge_distance { s.edge_distance = (e > 0).then_some(e); }
            });
            for id in selected_polys(state) {
                if let Some(p) = state.scene.node_mut(id).and_then(|n| n.edit_poly_mut()) {
                    if let Some(f) = falloff { p.soft.falloff.set(f.max(0.0)); }
                    if let Some(v) = pinch { p.soft.pinch.set(v.clamp(-1.0, 1.0)); }
                    if let Some(v) = bubble { p.soft.bubble.set(v.clamp(-1.0, 1.0)); }
                    if let Some(e) = edge_distance { p.soft.edge_distance = (e > 0).then_some(e); }
                    if let Some(b) = affect_backfacing { p.soft.affect_backfacing = b; }
                }
            }
            Ok(json!({}))
        }
        C::PaintSoftSelectionStroke { node, points, value, size, strength } => {
            let id = NodeId(node);
            let mode = state.soft_paint.unwrap_or(crate::state::SoftPaint::Paint);
            let base = state.soft_weights(id);
            let world = state.scene.object_matrix(id);
            let Some(p) = state.scene.node_mut(id).and_then(|n| n.edit_poly_mut()) else { return Err("not an Editable Poly".into()) };
            let n = p.mesh.vertex_count();
            let mut w = if base.len() == n { base } else { vec![0.0; n] };
            let radius = size.max(1e-3);
            let strength = strength.clamp(0.0, 1.0);
            let positions: Vec<glam::Vec3> = p.mesh.vertices().iter().map(|v| world.transform_point3(v.position)).collect();
            for pt in &points {
                let c = glam::Vec3::from(*pt);
                for (i, pos) in positions.iter().enumerate() {
                    let d = pos.distance(c);
                    if d > radius {
                        continue;
                    }
                    // Soft brush: full at the centre, zero at the edge.
                    let k = strength * (1.0 - d / radius);
                    w[i] = match mode {
                        crate::state::SoftPaint::Paint => w[i].max(value * k).min(1.0),
                        crate::state::SoftPaint::Revert => w[i] * (1.0 - k),
                        crate::state::SoftPaint::Blur => {
                            let nb = loom_mesh::select_ops::vertex_neighbours(&p.mesh, VertexId(i as u32));
                            if nb.is_empty() { w[i] } else {
                                let avg: f32 = nb.iter().map(|v| w[v.0 as usize]).sum::<f32>() / nb.len() as f32;
                                w[i] + (avg - w[i]) * k
                            }
                        }
                    };
                }
            }
            p.soft.painted = Some(w);
            p.soft.on = true;
            Ok(json!({}))
        }
        C::NamedSubSelectionCopy { name } => {
            let items: Vec<(NodeId, SubObjectId)> = state.sub_selection.iter().collect();
            state.named_sub_selections.insert(name, items);
            Ok(json!({}))
        }
        C::NamedSubSelectionPaste { name } => {
            let items = state.named_sub_selections.get(&name).cloned().ok_or_else(|| format!("no named selection {name:?}"))?;
            state.sub_selection.clear();
            let mode = state.selection_mode;
            for (n, id) in items {
                // Only ids that still exist at the current level (the topology may have changed).
                let Some(node) = state.scene.node(n) else { continue };
                let mesh = node.cage_mesh();
                let valid = match (mode, id) {
                    (SelectionMode::Vertex, SubObjectId::Vertex(v)) => (v.0 as usize) < mesh.vertex_count(),
                    (SelectionMode::Edge | SelectionMode::Border, SubObjectId::Edge(e)) => mesh.canonical_edge_id(e) == Some(e),
                    (SelectionMode::Face | SelectionMode::Element, SubObjectId::Face(f)) => (f.0 as usize) < mesh.face_count(),
                    _ => false,
                };
                if valid && !crate::execute::is_hidden(&mesh, id) {
                    state.sub_selection.insert(n, id);
                }
            }
            Ok(selection_json(state))
        }
        other => crate::poly_edit::execute(state, other),
    }
}

/// Selected nodes that are Editable Polys.
fn selected_polys(state: &AppState) -> Vec<NodeId> {
    state.selection.iter().copied().filter(|i| state.scene.node(*i).is_some_and(|n| n.edit_poly().is_some())).collect()
}

/// Flips a per-object soft-selection flag on every selected poly.
/// Soft Selection on the selected splines (Editable Spline / Edit Spline):
/// the new On state, or None when no spline is selected.
fn spline_soft(state: &mut AppState, f: impl Fn(&mut loom_scene::SoftSelection)) -> Option<bool> {
    let ids: Vec<NodeId> = state.selection.iter().copied().collect();
    let mut on = None;
    for id in ids {
        if let Some(so) = state.scene.node_mut(id).and_then(|n| n.edit_spline_mut()) {
            f(&mut so.soft);
            on = Some(so.soft.on);
        }
    }
    on
}

fn soft_toggle(state: &mut AppState, _label: &str, f: impl Fn(&mut loom_scene::PolyObject) -> bool) -> Result<Value, String> {
    let mut on = None;
    for id in selected_polys(state) {
        if let Some(p) = state.scene.node_mut(id).and_then(|n| n.edit_poly_mut()) {
            on = Some(f(p));
        }
    }
    Ok(json!({ "on": on }))
}

/// Raw sub-object ids per node for the current level.
pub fn raw_ids(level: SelectionMode, ids: &[SubObjectId]) -> Vec<u32> {
    ids.iter()
        .filter_map(|id| match (level, id) {
            (SelectionMode::Vertex, SubObjectId::Vertex(v)) => Some(v.0),
            (SelectionMode::Edge | SelectionMode::Border, SubObjectId::Edge(e)) => Some(e.0),
            (SelectionMode::Face | SelectionMode::Element, SubObjectId::Face(f)) => Some(f.0),
            _ => None,
        })
        .collect()
}

#[allow(dead_code)]
fn _ids(_: VertexId, _: FaceId) {}

pub use crate::execute::is_border_edge;
pub fn sub(mode: SelectionMode, raw: u32) -> SubObjectId {
    sub_id(mode, raw)
}
