//! Editable Spline command handlers: the chart's "(Spline)" actions and the
//! Loom variants with arguments, working on the selected shapes' knots /
//! segments / splines (SelectionMode Vertex / Edge / Face).

use crate::execute::{commit_change, selection_json};
use crate::state::{AppState, PickMode, SelectionMode, SubObjectId};
use crate::{Command as C, SelectOp};
use glam::{Mat4, Quat, Vec3};
use loom_scene::{Change, Node, NodeId, ObjectKind};
use loom_shape::{BoolOp, Knot, KnotKind, Shape, Spline};
use serde_json::{json, Value};
use std::collections::BTreeSet;

/// Geometry rollout spinners and radios (kept per session).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SplineSettings {
    pub weld_threshold: f32,
    pub fillet_amount: f32,
    pub chamfer_amount: f32,
    pub outline_amount: f32,
    pub outline_center: bool,
    pub divide_count: u32,
    /// 0 Union, 1 Subtract, 2 Intersect.
    pub boolean_op: u32,
    /// 0 horizontal, 1 vertical, 2 both.
    pub mirror_axis: u32,
    pub mirror_copy: bool,
    pub mirror_about_pivot: bool,
    /// New Vertex Type: 0 Linear, 1 Smooth, 2 Bezier, 3 Bezier Corner.
    pub new_vertex_type: u32,
    pub refine_connect: bool,
    pub refine_linear: bool,
    pub end_point_auto_weld: bool,
    pub auto_weld_threshold: f32,
    pub detach_same_shape: bool,
    pub detach_reorient: bool,
    pub detach_copy: bool,
    pub attach_reorient: bool,
    pub explode_to_objects: bool,
    pub lock_handles: bool,
    pub area_selection: bool,
    pub area_threshold: f32,
    pub segment_end: bool,
    pub show_vertex_numbers: bool,
    pub show_selected_segs: bool,
    pub create_shape_smooth: bool,
    pub extrude_segments: u32,
    pub extrude_taper: f32,
    pub extrude_twist: f32,
    pub extrude_align: bool,
    /// Extrude Along Spline Taper Curve: bulges (+) or pinches (−) the middle of the taper.
    pub extrude_taper_curve: f32,
    /// Extrude Along Spline Rotation (degrees about the face normal, with Align on).
    pub extrude_rotation: f32,
    /// The spline picked in the open Extrude Along Spline dialog (its preview path).
    pub extrude_path: Option<NodeId>,
    /// Refine on an existing vertex with "Do not show this message again":
    /// Some(true) = Connect Only, Some(false) = Refine; None = ask.
    pub refine_vertex_choice: Option<bool>,
    /// Cross Insert threshold.
    pub cross_threshold: f32,
    /// Cross Section: 0 Linear, 1 Smooth, 2 Bezier, 3 Bezier Corner.
    pub cross_section_kind: u32,
    /// Tangent > Paste Length.
    pub paste_length: bool,
    /// Trim/Extend modifier Operation: 0 Auto, 1 Trim Only, 2 Extend Only.
    pub trim_extend_op: u32,
}

impl Default for SplineSettings {
    fn default() -> Self {
        Self {
            weld_threshold: 0.1, fillet_amount: 0.0, chamfer_amount: 0.0, outline_amount: 0.0, outline_center: false, divide_count: 1, boolean_op: 0,
            mirror_axis: 0, mirror_copy: false, mirror_about_pivot: false, new_vertex_type: 0, refine_connect: false, refine_linear: false,
            end_point_auto_weld: true, auto_weld_threshold: 6.0, detach_same_shape: false, detach_reorient: false, detach_copy: false, attach_reorient: false,
            explode_to_objects: false, lock_handles: false, area_selection: false, area_threshold: 0.1, segment_end: false, show_vertex_numbers: false,
            show_selected_segs: true, create_shape_smooth: false, extrude_segments: 6, extrude_taper: 0.0, extrude_twist: 0.0, extrude_align: true,
            extrude_taper_curve: 0.0, extrude_rotation: 0.0, extrude_path: None, refine_vertex_choice: None,
            cross_threshold: 0.1, cross_section_kind: 2, paste_length: false, trim_extend_op: 0,
        }
    }
}

/// Selected Editable Spline nodes.
pub fn selected_splines(state: &AppState) -> Vec<NodeId> {
    state.selection.iter().copied().filter(|i| state.scene.node(*i).is_some_and(|n| n.edit_spline().is_some())).collect()
}

/// Flat sub-object indices of the current level, per spline node.
fn selected_flat(state: &AppState, id: NodeId) -> Vec<usize> {
    state.sub_selection.by_node().find(|(n, _)| *n == id).map(|(_, ids)| {
        let mut v: Vec<usize> = ids.iter().map(|s| match s {
            SubObjectId::Vertex(v) => v.0 as usize,
            SubObjectId::Edge(e) => e.0 as usize,
            SubObjectId::Face(f) => f.0 as usize,
        }).collect();
        v.sort();
        v
    }).unwrap_or_default()
}

/// Flat knot indices the current selection covers (vertices, segments' ends, splines' knots).
pub fn selected_knots(state: &AppState, id: NodeId, shape: &Shape) -> Vec<usize> {
    let flat = selected_flat(state, id);
    let mut out: BTreeSet<usize> = BTreeSet::new();
    match state.selection_mode {
        SelectionMode::Vertex => out.extend(flat),
        SelectionMode::Edge | SelectionMode::Border => {
            for f in flat {
                if let Some((a, b)) = shape.segment_knots(f) {
                    out.insert(a);
                    out.insert(b);
                }
            }
        }
        SelectionMode::Face | SelectionMode::Element => {
            for si in flat {
                if let Some(s) = shape.splines.get(si) {
                    for k in 0..s.knots.len() {
                        out.insert(shape.knot_index(si, k));
                    }
                }
            }
        }
        SelectionMode::Object => {}
    }
    out.into_iter().collect()
}

/// Applies `f` to every selected spline node's shape as one undo entry.
fn edit_shapes(state: &mut AppState, label: &str, mut f: impl FnMut(&mut Shape, &Node, &[usize]) -> Result<(), String>) -> Result<Value, String> {
    let ids = selected_splines(state);
    if ids.is_empty() {
        return Err("select an Editable Spline first".into());
    }
    let mut changes = Vec::new();
    for id in ids {
        let node = state.scene.node(id).ok_or("missing node")?;
        let mut shape = node.edit_shape().ok_or("not a shape")?;
        let flat = selected_flat(state, id);
        f(&mut shape, node, &flat)?;
        shape.apply_binds();
        changes.push(crate::execute::replace_shape_change(node, shape));
    }
    commit_change(state, label, Change::Group(changes))?;
    Ok(selection_json(state))
}

/// `edit_shapes` for operations that renumber knots / segments / splines:
/// the stale sub-selection is dropped afterwards.
fn edit_shapes_renumbering(state: &mut AppState, label: &str, mut f: impl FnMut(&mut Shape, &Node, &[usize]) -> Result<(), String>) -> Result<Value, String> {
    // Binds name flat indices, which renumbering invalidates.
    let r = edit_shapes(state, label, |shape, node, flat| {
        shape.binds.clear();
        f(shape, node, flat)
    })?;
    state.sub_selection.clear();
    // Spline indices moved: hidden flags would point at the wrong splines, so they clear.
    for id in selected_splines(state) {
        if let Some(so) = state.scene.node_mut(id).and_then(|n| n.edit_spline_mut()) {
            so.hidden.clear();
        }
    }
    Ok(r)
}

fn bool_op(name: &str) -> Result<BoolOp, String> {
    Ok(match name.to_ascii_lowercase().as_str() {
        "union" => BoolOp::Union,
        "subtract" | "subtraction" => BoolOp::Subtraction,
        "intersect" | "intersection" => BoolOp::Intersection,
        other => return Err(format!("unknown boolean {other:?} (Union, Subtract, Intersect)")),
    })
}

fn knot_kind(name: &str) -> Result<KnotKind, String> {
    Ok(match name.to_ascii_lowercase().replace(' ', "").as_str() {
        "corner" | "linear" => KnotKind::Corner,
        "smooth" => KnotKind::Smooth,
        "bezier" => KnotKind::Bezier,
        "beziercorner" => KnotKind::BezierCorner,
        other => return Err(format!("unknown vertex type {other:?}")),
    })
}

/// Per-knot operations at the Vertex level (indices descending so inserts stay valid).
fn per_selected_knot(shape: &mut Shape, flat: &[usize], mut f: impl FnMut(&mut Spline, usize)) {
    let mut targets: Vec<(usize, usize)> = flat.iter().filter_map(|i| shape.knot_at(*i)).collect();
    targets.sort_by(|a, b| b.cmp(a));
    for (si, k) in targets {
        f(&mut shape.splines[si], k);
    }
}

/// The world point brought into a shape node's object space.
fn local_point(state: &AppState, id: NodeId, p: [f32; 3]) -> Vec3 {
    state.scene.object_matrix(id).inverse().transform_point3(Vec3::from(p))
}

/// Bind pick: the selected end vertices bind to the segment nearest `point`.
pub fn bind_at(state: &mut AppState, point: Vec3) -> Result<Value, String> {
    let id = *selected_splines(state).first().ok_or("select an Editable Spline first")?;
    let shape = state.scene.node(id).and_then(|n| n.edit_shape()).ok_or("not a shape")?;
    let local = state.scene.object_matrix(id).inverse().transform_point3(point);
    let (si, param, _) = loom_shape::trim::nearest(&shape, local).ok_or("no segment there")?;
    let seg = shape.segment_index(si, (param.floor() as usize).min(shape.splines[si].segment_count().saturating_sub(1)));
    let knots: Vec<usize> = selected_flat(state, id);
    if knots.is_empty() {
        return Err("select the end vertices to bind first".into());
    }
    state.pick_mode = None;
    for k in knots {
        execute(state, C::SplineBind { knot: k as u32, segment: seg as u32 })?;
    }
    Ok(selection_json(state))
}

pub fn execute(state: &mut AppState, cmd: C) -> Result<Value, String> {
    let ss = state.spline_settings;
    match cmd {
        C::BindSpline | C::TrimSpline | C::ExtendSpline | C::CrossInsertSpline => {
            if selected_splines(state).is_empty() {
                return Err("select an Editable Spline first".into());
            }
            let mode = match cmd {
                C::BindSpline => PickMode::SplineBind,
                C::TrimSpline => PickMode::SplineTrim,
                C::ExtendSpline => PickMode::SplineExtend,
                _ => PickMode::SplineCrossInsert,
            };
            state.pick_mode = if state.pick_mode == Some(mode) { None } else { Some(mode) };
            Ok(json!({ "pick": state.pick_mode.map(|m| format!("{m:?}")) }))
        }
        C::UnbindSpline => {
            let ids = selected_splines(state);
            let flats: Vec<(NodeId, Vec<usize>)> = ids.iter().map(|id| (*id, selected_flat(state, *id))).collect();
            edit_shapes(state, "Unbind", |shape, node, _| {
                let sel = flats.iter().find(|(i, _)| *i == node.id).map(|(_, f)| f.clone()).unwrap_or_default();
                shape.binds.retain(|(k, _)| !sel.contains(k));
                Ok(())
            })
        }
        C::SplineTrim { point } => {
            let id = *selected_splines(state).first().ok_or("select an Editable Spline first")?;
            let p = local_point(state, id, point);
            // Tried on a copy first: a miss changes nothing and records nothing.
            let mut probe = state.scene.node(id).and_then(|n| n.edit_shape()).ok_or("not a shape")?;
            if !loom_shape::trim::trim(&mut probe, p) {
                return Err("nothing to trim to: the spline crosses nothing there".into());
            }
            edit_shapes_renumbering(state, "Trim", |shape, node, _| {
                if node.id == id {
                    loom_shape::trim::trim(shape, p);
                }
                Ok(())
            })
        }
        C::SplineExtend { point } => {
            let id = *selected_splines(state).first().ok_or("select an Editable Spline first")?;
            let p = local_point(state, id, point);
            let mut probe = state.scene.node(id).and_then(|n| n.edit_shape()).ok_or("not a shape")?;
            if !loom_shape::trim::extend(&mut probe, p) {
                return Err("nothing to extend to from that end".into());
            }
            edit_shapes(state, "Extend", |shape, node, _| {
                if node.id == id {
                    loom_shape::trim::extend(shape, p);
                }
                Ok(())
            })
        }
        C::SplineCrossInsert { threshold, point } => {
            let mats: Vec<(NodeId, glam::Mat4)> = selected_splines(state).into_iter().map(|id| (id, state.scene.object_matrix(id).inverse())).collect();
            edit_shapes_renumbering(state, "Cross Insert", |shape, node, _| {
                let near = point.and_then(|p| mats.iter().find(|(i, _)| *i == node.id).map(|(_, m)| m.transform_point3(Vec3::from(p))));
                if loom_shape::trim::cross_insert(shape, threshold, near) == 0 {
                    return Err("no splines cross within the threshold".into());
                }
                Ok(())
            })
        }
        C::SplineBind { knot, segment } => edit_shapes(state, "Bind", |shape, _, _| {
            let (ks, ki) = shape.knot_at(knot as usize).ok_or("no such vertex")?;
            let (ss_, _) = shape.segment_at(segment as usize).ok_or("no such segment")?;
            let sp = &shape.splines[ks];
            if sp.closed || (ki != 0 && ki + 1 != sp.knots.len()) {
                return Err("only an open spline's end vertex binds".into());
            }
            if ks == ss_ {
                return Err("a vertex binds to another spline's segment".into());
            }
            shape.binds.retain(|(k, _)| *k != knot as usize);
            shape.binds.push((knot as usize, segment as usize));
            Ok(())
        }),
        C::SplineCrossSection { splines, kind } => edit_shapes(state, "Cross Section", |shape, _, _| {
            let k = [KnotKind::Corner, KnotKind::Smooth, KnotKind::Bezier, KnotKind::BezierCorner][(kind as usize).min(3)];
            let list: Vec<usize> = splines.iter().map(|s| *s as usize).collect();
            let extra = loom_shape::trim::cross_section(shape, &list, k);
            if extra.is_empty() {
                return Err("Cross Section needs two or more splines".into());
            }
            shape.splines.extend(extra);
            Ok(())
        }),
        C::SplineTangentCopy { knot, side_in } => {
            let id = *selected_splines(state).first().ok_or("select an Editable Spline first")?;
            let shape = state.scene.node(id).and_then(|n| n.edit_shape()).ok_or("not a shape")?;
            let (si, ki) = shape.knot_at(knot as usize).ok_or("no such vertex")?;
            let sp = &shape.splines[si];
            let h = if side_in { sp.in_handle(ki) } else { sp.out_handle(ki) };
            state.tangent_clipboard = Some(h);
            Ok(json!({ "handle": h.to_array() }))
        }
        C::SplineTangentPaste { knot, side_in, length } => {
            let h = state.tangent_clipboard.ok_or("copy a tangent first")?;
            edit_shapes(state, "Tangent Paste", |shape, _, _| {
                let (si, ki) = shape.knot_at(knot as usize).ok_or("no such vertex")?;
                let sp = &mut shape.splines[si];
                let (in_h, out_h) = (sp.in_handle(ki), sp.out_handle(ki));
                let k = &mut sp.knots[ki];
                if !matches!(k.kind, KnotKind::Bezier | KnotKind::BezierCorner) {
                    k.in_vec = in_h;
                    k.out_vec = out_h;
                    k.kind = KnotKind::Bezier;
                }
                let target = if side_in { &mut k.in_vec } else { &mut k.out_vec };
                // Paste Length off: only the angle; the handle keeps its length.
                let keep = target.length();
                *target = if length || keep < 1e-6 { h } else { h.normalize_or_zero() * keep };
                // A Bezier (not Corner) vertex keeps its handles opposite.
                if k.kind == KnotKind::Bezier {
                    if side_in {
                        k.out_vec = -k.in_vec.normalize_or_zero() * k.out_vec.length();
                    } else {
                        k.in_vec = -k.out_vec.normalize_or_zero() * k.in_vec.length();
                    }
                }
                Ok(())
            })
        }
        C::FilletChamferApply { fillet } => fillet_chamfer_apply(state, fillet),
        C::TrimExtendPick { point, extend } => trim_extend_pick(state, point, extend),
        C::SplineRefine { node, segment, t } => {
            let id = NodeId(node);
            let n = state.scene.node(id).ok_or("missing node")?;
            let mut shape = n.edit_shape().ok_or("not a shape")?;
            let (si, seg) = shape.segment_at(segment as usize).ok_or("no such segment")?;
            let linear = state.spline_settings.refine_linear;
            let new = shape.splines[si].refine(seg, t.clamp(0.0, 1.0)).ok_or("refine failed")?;
            if linear {
                shape.splines[si].knots[new].kind = KnotKind::Corner;
            }
            let flat = shape.knot_index(si, new);
            if state.spline_settings.refine_connect {
                let p = shape.splines[si].knots[new].pos;
                refine_connect(state, id, &mut shape, p);
            }
            let n = state.scene.node(id).ok_or("missing node")?;
            commit_change(state, "Refine", crate::execute::replace_shape_change(n, shape))?;
            if state.selection_mode == SelectionMode::Vertex {
                state.sub_selection.clear();
                state.sub_selection.insert(id, SubObjectId::Vertex(loom_mesh::VertexId(flat as u32)));
            }
            Ok(json!({ "knot": flat }))
        }
        C::SplineRefineAtVertex { node, knot, segment, t } => {
            if !state.spline_settings.refine_connect {
                return execute(state, C::SplineRefine { node, segment, t });
            }
            match state.spline_settings.refine_vertex_choice {
                Some(true) => connect_only(state, NodeId(node), knot),
                Some(false) => execute(state, C::SplineRefine { node, segment, t }),
                None => {
                    state.refine_pending = Some((NodeId(node), knot, segment, t));
                    state.dialog = Some(crate::Dialog::RefineVertex);
                    Ok(json!({ "dialog": "RefineVertex" }))
                }
            }
        }
        C::SplineRefineReply { connect_only: only, dont_show } => {
            let (id, knot, segment, t) = state.refine_pending.take().ok_or("no Refine prompt is open")?;
            if state.dialog == Some(crate::Dialog::RefineVertex) {
                state.dialog = None;
            }
            if dont_show {
                state.spline_settings.refine_vertex_choice = Some(only);
            }
            if only {
                connect_only(state, id, knot)
            } else {
                execute(state, C::SplineRefine { node: id.0, segment, t })
            }
        }
        C::SplineFilletMode { chamfer } => {
            if selected_splines(state).is_empty() {
                return Err("select an Editable Spline first".into());
            }
            let mode = if chamfer { PickMode::SplineChamfer } else { PickMode::SplineFillet };
            state.pick_mode = if state.pick_mode == Some(mode) { None } else { Some(mode) };
            Ok(json!({ "pick": state.pick_mode.map(|m| format!("{m:?}")) }))
        }
        C::RefineSpline => {
            // Each Refine session connects its own vertices.
            state.refine_chain = None;
            state.pick_mode = if state.pick_mode == Some(PickMode::SplineRefine) { None } else { Some(PickMode::SplineRefine) };
            Ok(json!({ "pick": state.pick_mode.map(|m| format!("{m:?}")) }))
        }
        C::RefineConnectSpline => {
            state.spline_settings.refine_connect = !state.spline_settings.refine_connect;
            Ok(json!({ "connect": state.spline_settings.refine_connect }))
        }
        C::InsertSelectionSpline => {
            state.pick_mode = if state.pick_mode == Some(PickMode::SplineInsert) { None } else { Some(PickMode::SplineInsert) };
            Ok(json!({ "pick": state.pick_mode.map(|m| format!("{m:?}")) }))
        }
        C::SplineFillet { amount } => fillet_chamfer(state, amount, true),
        C::FilletSpline => fillet_chamfer(state, ss.fillet_amount, true),
        C::SplineChamfer { amount } => fillet_chamfer(state, amount, false),
        C::ChamferVertexSpline => fillet_chamfer(state, ss.chamfer_amount, false),
        C::SetVertexType { kind } => {
            let kind = knot_kind(&kind)?;
            edit_shapes(state, "Vertex Type", |shape, _, flat| {
                let knots = flat.to_vec();
                per_selected_knot(shape, &knots, |s, k| {
                    let (in_h, out_h) = (s.in_handle(k), s.out_handle(k));
                    let knot = &mut s.knots[k];
                    if matches!(kind, KnotKind::Bezier | KnotKind::BezierCorner) && knot.kind != KnotKind::Bezier && knot.kind != KnotKind::BezierCorner {
                        // Entering Bezier keeps the current curve as handles.
                        knot.in_vec = in_h;
                        knot.out_vec = out_h;
                    }
                    knot.kind = kind;
                });
                Ok(())
            })
        }
        C::SetKnotHandle { node, knot, side_in, point } => {
            let id = NodeId(node);
            let n = state.scene.node(id).ok_or("missing node")?;
            let mut shape = n.edit_shape().ok_or("not a shape")?;
            let (si, k) = shape.knot_at(knot as usize).ok_or("no such knot")?;
            let local = state.scene.object_matrix(id).inverse().transform_point3(Vec3::from(point));
            let lock = state.spline_settings.lock_handles;
            let selected = selected_knots(state, id, &shape);
            let delta = {
                let kn = &shape.splines[si].knots[k];
                local - kn.pos - if side_in { kn.in_vec } else { kn.out_vec }
            };
            let targets: Vec<(usize, usize)> = if lock && selected.contains(&(knot as usize)) { selected.iter().filter_map(|f| shape.knot_at(*f)).collect() } else { vec![(si, k)] };
            for (si, k) in targets {
                let kn = &mut shape.splines[si].knots[k];
                if kn.kind != KnotKind::Bezier && kn.kind != KnotKind::BezierCorner {
                    continue;
                }
                let (h, other) = if side_in { (&mut kn.in_vec, &mut kn.out_vec) } else { (&mut kn.out_vec, &mut kn.in_vec) };
                *h += delta;
                if kn.kind == KnotKind::Bezier {
                    // Bezier: the opposite handle stays collinear, keeping its length.
                    let len = other.length();
                    *other = -h.normalize_or_zero() * len;
                }
            }
            commit_change(state, "Move Handle", crate::execute::replace_shape_change(n, shape))?;
            Ok(json!({}))
        }
        C::SplineOutline { amount, center } => outline(state, amount, center),
        C::OutlineSpline => outline(state, ss.outline_amount, ss.outline_center),
        C::SplineBoolean { op, other } => boolean(state, bool_op(&op)?, Some(other as usize)),
        C::SubtractSpline => boolean(state, BoolOp::Subtraction, None),
        C::SplineMirror { axis, copy, about_pivot } => mirror(state, axis, copy, about_pivot),
        C::MirrorHorizontallySpline => mirror(state, 0, ss.mirror_copy, ss.mirror_about_pivot),
        C::MirrorVerticallySpline => mirror(state, 1, ss.mirror_copy, ss.mirror_about_pivot),
        C::MirrorBothSpline => mirror(state, 2, ss.mirror_copy, ss.mirror_about_pivot),
        C::SplineDivide { n } => divide(state, n),
        C::DivideSpline => divide(state, ss.divide_count),
        C::SplineWeld { threshold } => edit_shapes_renumbering(state, "Weld", |shape, _, flat| {
            // Selected end knots within the threshold weld (same spline closes, two splines join),
            // and any two selected knots of one spline within the threshold merge.
            let knots = flat.to_vec();
            let selected: BTreeSet<usize> = knots.iter().copied().collect();
            let mut merged = Vec::new();
            for si in 0..shape.splines.len() {
                let n = shape.splines[si].knots.len();
                let mut k = 0;
                while k + 1 < shape.splines[si].knots.len() {
                    let (a, b) = (shape.knot_index(si, k), shape.knot_index(si, k + 1));
                    let (pa, pb) = (shape.splines[si].knots[k].pos, shape.splines[si].knots[k + 1].pos);
                    if selected.contains(&a) && selected.contains(&b) && pa.distance(pb) <= threshold {
                        let mid = (pa + pb) * 0.5;
                        shape.splines[si].knots[k].pos = mid;
                        let next = shape.splines[si].knots.remove(k + 1);
                        shape.splines[si].knots[k].out_vec = next.out_vec;
                        merged.push(a);
                    } else {
                        k += 1;
                    }
                }
                let _ = n;
            }
            loom_shape::outline::weld(shape, threshold);
            Ok(())
        }),
        C::WeldVerticesSpline => execute(state, C::SplineWeld { threshold: ss.weld_threshold }),
        C::SplineFuse | C::FuseVerticesSpline => edit_shapes(state, "Fuse", |shape, _, flat| {
            shape.fuse(flat);
            Ok(())
        }),
        C::SplineConnect { from, to } => edit_shapes(state, "Connect", |shape, _, _| {
            let (a, b) = (shape.knot_at(from as usize).ok_or("no such knot")?, shape.knot_at(to as usize).ok_or("no such knot")?);
            if !loom_shape::outline::connect(shape, a, b) {
                return Err("Connect needs two end vertices".into());
            }
            Ok(())
        }),
        C::ConnectSpline => {
            let id = selected_splines(state).first().copied().ok_or("select an Editable Spline first")?;
            let flat = selected_flat(state, id);
            if flat.len() != 2 || state.selection_mode != SelectionMode::Vertex {
                return Err("select two end vertices to connect".into());
            }
            execute(state, C::SplineConnect { from: flat[0] as u32, to: flat[1] as u32 })
        }
        C::BreakVerticesSpline => edit_shapes_renumbering(state, "Break", |shape, _, flat| {
            let knots = flat.to_vec();
            let mut targets: Vec<(usize, usize)> = knots.iter().filter_map(|i| shape.knot_at(*i)).collect();
            targets.sort_by(|a, b| b.cmp(a));
            for (si, k) in targets {
                if let Some(tail) = shape.splines[si].break_at(k) {
                    shape.splines.push(tail);
                }
            }
            Ok(())
        }),
        C::CloseSpline => edit_shapes(state, "Close", |shape, _, flat| {
            let splines: BTreeSet<usize> = flat.iter().filter_map(|f| shape.knot_at(*f).map(|(si, _)| si).or_else(|| shape.segment_at(*f).map(|(si, _)| si)).or(Some(*f))).collect();
            for si in splines {
                if let Some(s) = shape.splines.get_mut(si) {
                    s.close();
                }
            }
            Ok(())
        }),
        C::ReverseSpline => for_selected_splines(state, "Reverse", |s| s.reverse()),
        C::MakeFirstSpline => edit_shapes(state, "Make First", |shape, _, flat| {
            if let Some((si, k)) = flat.first().and_then(|f| shape.knot_at(*f)) {
                shape.splines[si].make_first(k);
            }
            Ok(())
        }),
        C::CycleVerticesSpline => {
            // Cycle: the selection moves to the next knot of its spline.
            let id = selected_splines(state).first().copied().ok_or("select an Editable Spline first")?;
            let shape = state.scene.node(id).ok_or("missing node")?.edit_shape().ok_or("not a shape")?;
            let flat = selected_flat(state, id);
            let Some((si, k)) = flat.first().and_then(|f| shape.knot_at(*f)) else { return Err("select a vertex first".into()) };
            let next = shape.knot_index(si, (k + 1) % shape.splines[si].knots.len());
            state.sub_selection.clear();
            state.sub_selection.insert(id, SubObjectId::Vertex(loom_mesh::VertexId(next as u32)));
            Ok(selection_json(state))
        }
        C::DeleteVertexSpline | C::DeleteSegmentSpline | C::DeleteSplineSpline => delete_selection(state),
        C::HideSpline => {
            let mode = state.selection_mode;
            let mut changes = Vec::new();
            for id in selected_splines(state) {
                let flat = selected_flat(state, id);
                let node = state.scene.node(id).ok_or("missing node")?;
                let Some(so) = node.edit_spline() else { continue };
                let shape = &so.shape;
                let splines: BTreeSet<usize> = match mode {
                    SelectionMode::Vertex => flat.iter().filter_map(|f| shape.knot_at(*f).map(|(si, _)| si)).collect(),
                    SelectionMode::Edge => flat.iter().filter_map(|f| shape.segment_at(*f).map(|(si, _)| si)).collect(),
                    _ => flat.into_iter().collect(),
                };
                let mut hidden = so.hidden.clone();
                hidden.extend(splines);
                changes.push(hidden_change(node, hidden));
            }
            commit_change(state, "Hide", Change::Group(changes))?;
            state.sub_selection.clear();
            Ok(json!({}))
        }
        C::UnhideAllSpline => {
            let mut changes = Vec::new();
            for id in selected_splines(state) {
                let node = state.scene.node(id).ok_or("missing node")?;
                if node.edit_spline().is_some_and(|so| !so.hidden.is_empty()) {
                    changes.push(hidden_change(node, BTreeSet::new()));
                }
            }
            commit_change(state, "Unhide All", Change::Group(changes))?;
            Ok(json!({}))
        }
        C::SplineAttach { node, reorient } => attach(state, NodeId(node), reorient),
        C::AttachSpline => {
            state.pick_mode = if state.pick_mode == Some(PickMode::SplineAttach) { None } else { Some(PickMode::SplineAttach) };
            Ok(json!({ "pick": state.pick_mode.map(|m| format!("{m:?}")) }))
        }
        C::AttachMultipleSpline => {
            // Attach Multiple: every other selected shape joins the first.
            let ids = selected_splines(state);
            let target = ids.first().copied().ok_or("select an Editable Spline first")?;
            let others: Vec<NodeId> = state.selection.iter().copied().filter(|i| *i != target && state.scene.node(*i).is_some_and(|n| n.object.is_shape())).collect();
            if others.is_empty() {
                return Err("select the shapes to attach along with the target".into());
            }
            state.undo.begin("Attach Multiple");
            for o in others {
                state.selection = BTreeSet::from([target]);
                if let Err(e) = attach(state, o, ss.attach_reorient) {
                    state.undo.end();
                    return Err(e);
                }
            }
            state.undo.end();
            Ok(json!({}))
        }
        C::SplineDetach { same_shape, reorient, copy } => detach(state, same_shape, reorient, copy),
        C::DetachSegmentSpline | C::DetachSplineSpline => detach(state, ss.detach_same_shape, ss.detach_reorient, ss.detach_copy),
        C::SplineExplode { to_objects } => explode(state, to_objects),
        C::ExplodeSpline => explode(state, ss.explode_to_objects),
        C::CreateLineSpline => {
            // Create Line: the Line tool draws into this shape (the UI attaches the new line).
            let id = selected_splines(state).first().copied().ok_or("select an Editable Spline first")?;
            state.spline_create_line_target = Some(id);
            Ok(json!({ "target": id.0 }))
        }
        C::SplineSettings { name, value } => {
            let s = &mut state.spline_settings;
            let b = value != 0.0;
            let f = value as f32;
            let u = value.max(0.0) as u32;
            match name.as_str() {
                "Weld Threshold" => s.weld_threshold = f.max(0.0),
                "Fillet" => s.fillet_amount = f.max(0.0),
                "Chamfer" => s.chamfer_amount = f.max(0.0),
                "Outline" => s.outline_amount = f,
                "Outline Center" => s.outline_center = b,
                "Divisions" => s.divide_count = u.max(1),
                "Boolean" => s.boolean_op = u.min(2),
                "Mirror Axis" => s.mirror_axis = u.min(2),
                "Mirror Copy" => s.mirror_copy = b,
                "Mirror About Pivot" => s.mirror_about_pivot = b,
                "New Vertex Type" => s.new_vertex_type = u.min(3),
                "Refine Connect" => s.refine_connect = b,
                "Refine Linear" => s.refine_linear = b,
                "End Point Auto-Welding" => s.end_point_auto_weld = b,
                "Auto-Weld Threshold" => s.auto_weld_threshold = f.max(0.0),
                "Detach Same Shape" => s.detach_same_shape = b,
                "Detach Reorient" => s.detach_reorient = b,
                "Detach Copy" => s.detach_copy = b,
                "Attach Reorient" => s.attach_reorient = b,
                "Explode To Objects" => s.explode_to_objects = b,
                "Cross Insert Threshold" => s.cross_threshold = f.max(0.0),
                "Cross Section Type" => s.cross_section_kind = (f as u32).min(3),
                "Paste Length" => s.paste_length = b,
                "Trim/Extend Operation" => s.trim_extend_op = (f as u32).min(2),
                "Lock Handles" => s.lock_handles = b,
                "Area Selection" => s.area_selection = b,
                "Area Threshold" => s.area_threshold = f.max(0.0),
                "Segment End" => s.segment_end = b,
                "Show Vertex Numbers" => s.show_vertex_numbers = b,
                "Show Selected Segs" => s.show_selected_segs = b,
                "Create Shape Smooth" => s.create_shape_smooth = b,
                "Extrude Segments" => s.extrude_segments = u.max(1),
                "Extrude Taper" => s.extrude_taper = f,
                "Extrude Twist" => s.extrude_twist = f,
                "Extrude Align" => s.extrude_align = b,
                "Extrude Taper Curve" => s.extrude_taper_curve = f,
                "Extrude Rotation" => s.extrude_rotation = f,
                other => return Err(format!("unknown spline setting {other:?}")),
            }
            Ok(json!({}))
        }
        C::CreateShapeFromEdges { name, smooth } => create_shape_from_edges(state, name, smooth),
        C::ExtrudeAlongSpline { spline, segments, taper, taper_curve, twist, rotation, align } => extrude_along_spline(state, NodeId(spline), segments, [taper, taper_curve], twist, rotation, align),
        other => Err(format!("not an Editable Spline command: {other:?}")),
    }
}

/// Refine with Connect: `p` (object space) joins the previous connected point
/// of this node by a segment, all of a session's points in one spline (the
/// first point only starts it). Returns true when the shape changed.
fn refine_connect(state: &mut AppState, id: NodeId, shape: &mut Shape, p: Vec3) -> bool {
    let knot = if state.spline_settings.refine_linear { Knot::corner(p) } else { Knot::smooth(p) };
    let (next, changed) = match state.refine_chain.filter(|(n, _, _)| *n == id) {
        // An undo can take the chain's end away: then a new chain starts here.
        Some((_, Some(ci), last)) => match shape.splines.get_mut(ci).filter(|c| !c.closed && c.knots.last().is_some_and(|k| k.pos.distance(last) < 1e-4)) {
            Some(c) => {
                c.knots.push(knot);
                (Some(ci), true)
            }
            None => (None, false),
        },
        Some((_, None, last)) => {
            let first = if state.spline_settings.refine_linear { Knot::corner(last) } else { Knot::smooth(last) };
            shape.splines.push(Spline::new(vec![first, knot], false));
            (Some(shape.splines.len() - 1), true)
        }
        None => (None, false),
    };
    state.refine_chain = Some((id, next, p));
    changed
}

/// Connect Only: the existing knot joins the Refine connection without a new vertex.
fn connect_only(state: &mut AppState, id: NodeId, knot: u32) -> Result<Value, String> {
    let n = state.scene.node(id).ok_or("missing node")?;
    let mut shape = n.edit_shape().ok_or("not a shape")?;
    let (si, k) = shape.knot_at(knot as usize).ok_or("no such vertex")?;
    let p = shape.splines[si].knots[k].pos;
    if refine_connect(state, id, &mut shape, p) {
        let n = state.scene.node(id).ok_or("missing node")?;
        commit_change(state, "Refine", crate::execute::replace_shape_change(n, shape))?;
    }
    Ok(json!({ "knot": knot }))
}

fn for_selected_splines(state: &mut AppState, label: &str, f: impl Fn(&mut Spline)) -> Result<Value, String> {
    let mode = state.selection_mode;
    edit_shapes(state, label, |shape, _, flat| {
        let splines: BTreeSet<usize> = match mode {
            SelectionMode::Vertex => flat.iter().filter_map(|i| shape.knot_at(*i).map(|(si, _)| si)).collect(),
            SelectionMode::Edge | SelectionMode::Border => flat.iter().filter_map(|i| shape.segment_at(*i).map(|(si, _)| si)).collect(),
            _ => flat.iter().copied().collect(),
        };
        for si in splines {
            if let Some(s) = shape.splines.get_mut(si) {
                f(s);
            }
        }
        Ok(())
    })
}

fn fillet_chamfer(state: &mut AppState, amount: f32, fillet: bool) -> Result<Value, String> {
    if state.selection_mode != SelectionMode::Vertex {
        return Err("Fillet / Chamfer work at the Vertex level".into());
    }
    let mut new_sel: Vec<(NodeId, usize)> = Vec::new();
    let r = edit_shapes(state, if fillet { "Fillet" } else { "Chamfer" }, |shape, node, flat| {
        let mut targets: Vec<(usize, usize)> = flat.iter().filter_map(|i| shape.knot_at(*i)).collect();
        targets.sort_by(|a, b| b.cmp(a));
        let mut made: Vec<(usize, usize)> = Vec::new();
        for (si, k) in targets {
            if let Some((a, b)) = loom_shape::outline::fillet_or_chamfer(&mut shape.splines[si], k, amount, fillet) {
                // Earlier (higher-index) results shift by one per insert before them.
                for m in made.iter_mut().filter(|(msi, mk)| *msi == si && *mk > k) {
                    m.1 += 1;
                }
                made.push((si, a));
                made.push((si, b));
            }
        }
        for (si, k) in made {
            new_sel.push((node.id, shape.knot_index(si, k)));
        }
        Ok(())
    })?;
    state.sub_selection.clear();
    for (n, k) in new_sel {
        state.sub_selection.insert(n, SubObjectId::Vertex(loom_mesh::VertexId(k as u32)));
    }
    Ok(r)
}

fn outline(state: &mut AppState, amount: f32, center: bool) -> Result<Value, String> {
    if amount.abs() < 1e-6 {
        return Err("Outline needs an amount".into());
    }
    let mode = state.selection_mode;
    edit_shapes_renumbering(state, "Outline", |shape, _, flat| {
        let splines: Vec<usize> = match mode {
            SelectionMode::Face | SelectionMode::Element => flat.to_vec(),
            SelectionMode::Edge => flat.iter().filter_map(|i| shape.segment_at(*i).map(|(si, _)| si)).collect::<BTreeSet<_>>().into_iter().collect(),
            _ => (0..shape.splines.len()).collect(),
        };
        for si in splines {
            loom_shape::outline::outline(shape, si, amount, center);
        }
        Ok(())
    })
}

fn boolean(state: &mut AppState, op: BoolOp, other: Option<usize>) -> Result<Value, String> {
    let steps = 6;
    edit_shapes_renumbering(state, "Boolean", |shape, _, flat| {
        let a = *flat.first().ok_or("select the first spline")?;
        let b = other.or_else(|| flat.get(1).copied()).ok_or("Boolean needs a second closed spline (pick it or select two)")?;
        if a == b || a >= shape.splines.len() || b >= shape.splines.len() {
            return Err("Boolean needs two different splines of this shape".into());
        }
        if !shape.splines[a].closed || !shape.splines[b].closed {
            return Err("Boolean works on closed splines".into());
        }
        let result = loom_shape::boolean(&shape.splines[a], &shape.splines[b], op, steps.max(shape.steps));
        let (hi, lo) = (a.max(b), a.min(b));
        shape.splines.remove(hi);
        shape.splines.remove(lo);
        shape.splines.extend(result);
        Ok(())
    })
}

fn mirror(state: &mut AppState, axis: u32, copy: bool, about_pivot: bool) -> Result<Value, String> {
    let mode = state.selection_mode;
    edit_shapes(state, "Mirror", |shape, _, flat| {
        let splines: Vec<usize> = match mode {
            SelectionMode::Face | SelectionMode::Element => flat.to_vec(),
            _ => (0..shape.splines.len()).collect(),
        };
        for si in splines {
            let mut m = shape.splines[si].clone();
            // About Pivot mirrors about the object origin, else about the spline's own centre.
            let centre = if about_pivot { Vec3::ZERO } else { m.knots.iter().map(|k| k.pos).sum::<Vec3>() / m.knots.len().max(1) as f32 };
            m.translate(-centre);
            m.mirror(axis as u8);
            m.translate(centre);
            if copy {
                shape.splines.push(m);
            } else {
                shape.splines[si] = m;
            }
        }
        Ok(())
    })
}

/// The change that sets the hidden splines of the edited spline object.
fn hidden_change(node: &Node, hidden: BTreeSet<usize>) -> Change {
    match node.editing_spline_entry() {
        Some(a) => {
            let mut after = node.modifiers.clone();
            if let Some(so) = after[a].modifier.edit_spline_mut() {
                so.hidden = hidden;
            }
            Change::SetStack { id: node.id, before: node.modifiers.clone(), after }
        }
        None => {
            let mut after = node.object.clone();
            if let Some(so) = after.spline_mut() {
                so.hidden = hidden;
            }
            Change::ReplaceObject { id: node.id, before: Box::new(node.object.clone()), after: Box::new(after) }
        }
    }
}

/// Line End: a line with fewer than two knots is dropped; a finished line's
/// pivot moves to the centre of its bounds (the knots stay where they were drawn).
pub fn line_end(state: &mut AppState, id: NodeId) -> Result<Value, String> {
    let node = state.scene.node(id).ok_or("missing node")?;
    let knots = node.object.spline().map(|s| s.shape.knot_count()).unwrap_or(0);
    if knots < 2 {
        commit_change(state, "Line", Change::RemoveNode(id))?;
        state.selection.remove(&id);
        return Ok(json!({ "removed": true }));
    }
    if let Some(so) = node.object.spline() {
        let (lo, hi) = so.shape.aabb();
        let c = (lo + hi) * 0.5;
        if c.length() > 1e-6 {
            let mut shape = so.shape.clone();
            for s in &mut shape.splines {
                s.translate(-c);
            }
            let before = node.transform.clone();
            let mut after = before.clone();
            after.position.set(before.position.get() + before.matrix().transform_vector3(node.object_offset.matrix().transform_vector3(c)));
            let changes = vec![
                Change::ReplaceObject { id, before: Box::new(node.object.clone()), after: Box::new(node.object.with_shape(shape)) },
                Change::SetTransform { id, before, after },
            ];
            commit_change(state, "Line", Change::Group(changes))?;
        }
    }
    Ok(json!({ "removed": false, "knots": knots }))
}

fn divide(state: &mut AppState, n: u32) -> Result<Value, String> {
    if state.selection_mode != SelectionMode::Edge {
        return Err("Divide works at the Segment level".into());
    }
    edit_shapes_renumbering(state, "Divide", |shape, _, flat| {
        let mut targets: Vec<(usize, usize)> = flat.iter().filter_map(|i| shape.segment_at(*i)).collect();
        targets.sort_by(|a, b| b.cmp(a));
        for (si, seg) in targets {
            shape.splines[si].divide(seg, n.max(1));
        }
        Ok(())
    })
}

/// Delete at a spline level: knots, segments or whole splines.
pub fn delete_selection(state: &mut AppState) -> Result<Value, String> {
    let mode = state.selection_mode;
    edit_shapes_renumbering(state, "Delete", |shape, _, flat| {
        match mode {
            SelectionMode::Vertex => shape.delete_knots(flat),
            SelectionMode::Edge | SelectionMode::Border => shape.delete_segments(flat),
            _ => {
                let mut gone: Vec<usize> = flat.to_vec();
                gone.sort();
                for si in gone.into_iter().rev() {
                    if si < shape.splines.len() {
                        shape.splines.remove(si);
                    }
                }
            }
        }
        Ok(())
    })
}

/// Attach another shape: its splines join the selected one in this object's space.
fn attach(state: &mut AppState, other: NodeId, reorient: bool) -> Result<Value, String> {
    let target = selected_splines(state).first().copied().ok_or("select an Editable Spline first")?;
    if other == target {
        return Err("cannot attach a shape to itself".into());
    }
    let onode = state.scene.node(other).ok_or("missing node")?;
    let oshape = onode.edit_shape().ok_or("the picked object is not a shape")?;
    let tnode = state.scene.node(target).ok_or("missing node")?;
    let mut shape = tnode.edit_shape().ok_or("not a shape")?;
    let to_local = if reorient { Mat4::IDENTITY } else { state.scene.object_matrix(target).inverse() * state.scene.object_matrix(other) };
    for mut s in oshape.splines {
        for k in &mut s.knots {
            k.pos = to_local.transform_point3(k.pos);
            k.in_vec = to_local.transform_vector3(k.in_vec);
            k.out_vec = to_local.transform_vector3(k.out_vec);
        }
        shape.splines.push(s);
    }
    let changes = vec![
        crate::execute::replace_shape_change(tnode, shape),
        Change::RemoveNode(other),
    ];
    commit_change(state, "Attach", Change::Group(changes))?;
    state.selection = BTreeSet::from([target]);
    state.pick_mode = None;
    Ok(json!({}))
}

/// Detach the selected segments / splines: to a new shape (or the same one), optionally as a copy.
fn detach(state: &mut AppState, same_shape: bool, reorient: bool, copy: bool) -> Result<Value, String> {
    let id = selected_splines(state).first().copied().ok_or("select an Editable Spline first")?;
    let node = state.scene.node(id).ok_or("missing node")?;
    let shape = node.edit_shape().ok_or("not a shape")?;
    let flat = selected_flat(state, id);
    let mode = state.selection_mode;
    let mut taken = Shape::from_splines(Vec::new());
    taken.steps = shape.steps;
    taken.optimize = shape.optimize;
    taken.adaptive = shape.adaptive;
    taken.render = shape.render.clone();
    let mut rest = shape.clone();
    match mode {
        SelectionMode::Face | SelectionMode::Element => {
            let mut idx: Vec<usize> = flat.iter().copied().filter(|i| *i < shape.splines.len()).collect();
            idx.sort();
            for si in idx.iter().rev() {
                taken.splines.insert(0, shape.splines[*si].clone());
                if !copy {
                    rest.splines.remove(*si);
                }
            }
        }
        SelectionMode::Edge | SelectionMode::Border => {
            // Each selected segment becomes a two-knot spline.
            for f in &flat {
                if let Some((si, seg)) = shape.segment_at(*f) {
                    let b = shape.splines[si].segment(seg).unwrap();
                    taken.splines.push(Spline::new(vec![Knot::bezier(b[0], Vec3::ZERO, b[1] - b[0]), Knot::bezier(b[3], b[2] - b[3], Vec3::ZERO)], false));
                }
            }
            if !copy {
                rest.delete_segments(&flat);
            }
        }
        _ => return Err("Detach works at the Segment or Spline level".into()),
    }
    if taken.splines.is_empty() {
        return Err("select segments or splines to detach".into());
    }
    let kept = if same_shape { let mut r = rest.clone(); r.splines.extend(taken.splines.clone()); r } else { rest.clone() };
    let mut changes = vec![crate::execute::replace_shape_change(node, kept)];
    let mut new_id = None;
    if !same_shape {
        let mut new_node = Node::new(&state.scene.next_name("Shape"), ObjectKind::EditableSpline(loom_scene::ShapeObject::from_shape(taken)));
        new_node.wire_color = node.wire_color;
        if !reorient {
            new_node.transform = node.transform.clone();
        }
        let nid = NodeId(state.scene.next_id());
        new_id = Some(nid);
        changes.push(Change::AddNode(new_node));
    }
    commit_change(state, "Detach", Change::Group(changes))?;
    state.sub_selection.clear();
    Ok(json!({ "new": new_id.map(|n| n.0) }))
}

/// Explode: every segment becomes its own spline (or its own shape object).
fn explode(state: &mut AppState, to_objects: bool) -> Result<Value, String> {
    let id = selected_splines(state).first().copied().ok_or("select an Editable Spline first")?;
    let node = state.scene.node(id).ok_or("missing node")?;
    let shape = node.edit_shape().ok_or("not a shape")?;
    let flat = selected_flat(state, id);
    let targets: Vec<usize> = if matches!(state.selection_mode, SelectionMode::Face | SelectionMode::Element) && !flat.is_empty() { flat } else { (0..shape.splines.len()).collect() };
    let mut pieces = Shape::from_splines(targets.iter().filter_map(|si| shape.splines.get(*si).cloned()).collect());
    pieces.explode();
    let mut rest = shape.clone();
    let mut sorted = targets.clone();
    sorted.sort();
    for si in sorted.into_iter().rev() {
        if si < rest.splines.len() {
            rest.splines.remove(si);
        }
    }
    let mut changes = Vec::new();
    if to_objects {
        changes.push(crate::execute::replace_shape_change(node, rest));
        for (next, s) in (state.scene.next_id()..).zip(pieces.splines) {
            let mut piece = shape.clone();
            piece.splines = vec![s];
            let mut n = Node::new(&format!("{}_{:03}", node.name, next), ObjectKind::EditableSpline(loom_scene::ShapeObject::from_shape(piece)));
            n.transform = node.transform.clone();
            n.wire_color = node.wire_color;
            changes.push(Change::AddNode(n));
        }
    } else {
        rest.splines.extend(pieces.splines);
        changes.push(crate::execute::replace_shape_change(node, rest));
    }
    commit_change(state, "Explode", Change::Group(changes))?;
    state.sub_selection.clear();
    Ok(json!({}))
}

/// Create Shape From Selection: the selected poly edges chain into splines
/// of a new Editable Spline (Linear = corner knots, Smooth = smooth knots).
fn create_shape_from_edges(state: &mut AppState, name: Option<String>, smooth: bool) -> Result<Value, String> {
    let edges = crate::execute::selected_edges(state);
    if edges.is_empty() {
        return Err("select edges first".into());
    }
    let mut splines = Vec::new();
    let mut world_of: Option<Mat4> = None;
    for (id, es) in edges {
        let node = state.scene.node(id).ok_or("missing node")?;
        let mesh = node.cage_mesh().into_owned();
        let world = state.scene.object_matrix(id);
        world_of.get_or_insert(world);
        // Chain edges by shared vertices.
        let mut pairs: Vec<(u32, u32)> = es.iter().filter_map(|e| mesh.edge_vertices(*e)).map(|(a, b)| (a.0, b.0)).collect();
        while let Some(first) = pairs.pop() {
            let mut chain = vec![first.0, first.1];
            let mut closed = false;
            loop {
                let tail = *chain.last().unwrap();
                if let Some(i) = pairs.iter().position(|(a, b)| *a == tail || *b == tail) {
                    let (a, b) = pairs.remove(i);
                    chain.push(if a == tail { b } else { a });
                    continue;
                }
                let head = chain[0];
                if let Some(i) = pairs.iter().position(|(a, b)| *a == head || *b == head) {
                    let (a, b) = pairs.remove(i);
                    chain.insert(0, if a == head { b } else { a });
                    continue;
                }
                break;
            }
            if chain.len() > 2 && chain.first() == chain.last() {
                chain.pop();
                closed = true;
            }
            let knots: Vec<Knot> = chain.iter().map(|v| {
                let p = world.transform_point3(mesh.vertex_position(loom_mesh::VertexId(*v)).unwrap_or_default());
                if smooth { Knot::smooth(p) } else { Knot::corner(p) }
            }).collect();
            splines.push(Spline::new(knots, closed));
        }
    }
    let shape = Shape::from_splines(splines);
    let name = name.unwrap_or_else(|| state.scene.next_name("Shape"));
    let new_node = Node::new(&name, ObjectKind::EditableSpline(loom_scene::ShapeObject::from_shape(shape)));
    let nid = NodeId(state.scene.next_id());
    commit_change(state, "Create Shape", Change::AddNode(new_node))?;
    Ok(json!({ "id": nid.0, "name": name }))
}

/// Extrude Along Spline: the selected polygons sweep along the picked
/// shape's first spline (`segments` steps; `taper` = [amount, curve] scales
/// by `1 + amount·t + curve·t·(1−t)` like the Taper modifier; twist; `align`
/// turns the path so it starts along the face normal, then `rotation`
/// degrees about it).
fn extrude_along_spline(state: &mut AppState, spline: NodeId, segments: u32, taper: [f32; 2], twist: f32, rotation: f32, align: bool) -> Result<Value, String> {
    let snode = state.scene.node(spline).ok_or("missing spline node")?;
    let shape = snode.edit_shape().ok_or("the picked object is not a shape")?;
    let path = shape.splines.first().ok_or("the shape has no spline")?;
    if path.segment_count() == 0 {
        return Err("the path needs at least two knots".into());
    }
    let spline_world = state.scene.object_matrix(spline);
    let segs = segments.max(1) as usize;
    // Points along the path at equal parameter steps (world space).
    let pts: Vec<Vec3> = (0..=segs).map(|i| {
        let u = i as f32 / segs as f32 * path.segment_count() as f32;
        let si = (u.floor() as usize).min(path.segment_count().saturating_sub(1));
        let t = (u - si as f32).clamp(0.0, 1.0);
        let b = path.segment(si).unwrap();
        spline_world.transform_point3(loom_shape::interp::point_at(&b, t))
    }).collect();
    let tangents: Vec<Vec3> = (0..=segs).map(|i| {
        let prev = pts[i.saturating_sub(1)];
        let next = pts[(i + 1).min(segs)];
        (next - prev).normalize_or_zero()
    }).collect();
    let faces = crate::execute::selected_faces(state);
    if faces.is_empty() {
        return Err("select polygons first".into());
    }
    let mut count = 0;
    let matrices: std::collections::BTreeMap<NodeId, Mat4> = faces.iter().map(|(id, _)| (*id, state.scene.object_matrix(*id))).collect();
    crate::execute::edit_meshes(state, "Extrude Along Spline", faces, |mesh, id, fs| {
        let inv = matrices.get(&id).copied().unwrap_or(Mat4::IDENTITY).inverse();
        let snapshot = mesh.clone();
        let ring_pos = |group: &BTreeSet<loom_mesh::FaceId>, _v: loom_mesh::VertexId, p: Vec3, t: f32| -> Vec3 {
            let (n, c) = loom_mesh::poly_ops::group_frame(&snapshot, group);
            let k = ((t * segs as f32).round() as usize).min(segs);
            // Path frame k relative to its start, in this object's space.
            let start = inv.transform_point3(pts[0]);
            let pk = inv.transform_point3(pts[k]);
            let t0 = inv.transform_vector3(tangents[0]).normalize_or_zero();
            let tk = inv.transform_vector3(tangents[k]).normalize_or_zero();
            let base_rot = if align { Quat::from_axis_angle(n, rotation.to_radians()) * Quat::from_rotation_arc(t0, n) } else { Quat::IDENTITY };
            let turn = Quat::from_rotation_arc(t0, tk);
            let axis = base_rot * tk;
            let spin = Quat::from_axis_angle(axis.normalize_or_zero(), (twist * t).to_radians());
            let scale = 1.0 + taper[0] * t + taper[1] * t * (1.0 - t);
            let offset = base_rot * (pk - start);
            c + offset + spin * (base_rot * (turn * (base_rot.inverse() * ((p - c) * scale))))
        };
        let caps = loom_mesh::poly_ops::sweep_faces(mesh, &fs, segs as u32, &ring_pos)?;
        count += caps.len();
        Ok(())
    })?;
    state.pick_mode = None;
    Ok(json!({ "extruded": count }))
}

/// Select Sub for spline nodes: ids are flat knot / segment / spline indices.
pub fn select_sub(state: &mut AppState, node: NodeId, ids: &[u32], op: SelectOp) -> Result<Value, String> {
    let shape = state.scene.node(node).and_then(|n| n.edit_shape()).ok_or("not a shape")?;
    let mode = state.selection_mode;
    if op == SelectOp::Replace {
        state.sub_selection.clear();
    }
    let hidden = state.scene.node(node).and_then(|n| n.edit_spline().map(|s| s.hidden.clone())).unwrap_or_default();
    // Area Selection: a vertex pick takes every knot within the threshold with it.
    let mut ids: Vec<u32> = ids.to_vec();
    if mode == SelectionMode::Vertex && state.spline_settings.area_selection {
        let pts = shape.knot_positions();
        let r = state.spline_settings.area_threshold;
        let extra: Vec<u32> = ids.iter().flat_map(|i| {
            let c = pts.get(*i as usize).copied();
            pts.iter().enumerate().filter(move |(_, p)| c.is_some_and(|c| c.distance(**p) <= r)).map(|(j, _)| j as u32)
        }).collect();
        ids.extend(extra);
        ids.sort();
        ids.dedup();
    }
    for raw in &ids {
        let valid = match mode {
            SelectionMode::Vertex => shape.knot_at(*raw as usize).is_some_and(|(si, _)| !hidden.contains(&si)),
            SelectionMode::Edge | SelectionMode::Border => shape.segment_at(*raw as usize).is_some_and(|(si, _)| !hidden.contains(&si)),
            _ => (*raw as usize) < shape.splines.len() && !hidden.contains(&(*raw as usize)),
        };
        if !valid {
            continue;
        }
        let id = crate::execute::sub_id(mode, *raw);
        if op == SelectOp::Subtract {
            state.sub_selection.remove(node, id);
        } else {
            state.sub_selection.insert(node, id);
        }
    }
    Ok(selection_json(state))
}

/// The selected node's entry of a modifier kind: the active one, else the topmost.
fn modifier_entry(state: &AppState, is: impl Fn(&loom_scene::Modifier) -> bool) -> Option<(NodeId, usize)> {
    state.selection.iter().find_map(|id| {
        let n = state.scene.node(*id)?;
        let i = n.active_entry.filter(|a| n.modifiers.get(*a).is_some_and(|s| is(&s.modifier))).or_else(|| n.modifiers.iter().rposition(|s| is(&s.modifier)))?;
        Some((*id, i))
    })
}

fn input_shape(state: &AppState, id: NodeId, index: usize) -> Option<Shape> {
    match index {
        0 => state.scene.node(id)?.object.shape(),
        i => state.scene.evaluated(id, Some(i - 1)).value.shape.clone(),
    }
}

fn fillet_chamfer_apply(state: &mut AppState, fillet: bool) -> Result<Value, String> {
    let (id, index) = modifier_entry(state, |m| matches!(m, loom_scene::Modifier::FilletChamfer(_))).ok_or("select an object with a Fillet/Chamfer modifier")?;
    let n = state.scene.node(id).unwrap();
    let before = n.modifiers.clone();
    let mut after = before.clone();
    let input = input_shape(state, id, index).ok_or("Fillet/Chamfer needs a shape below it")?;
    let loom_scene::Modifier::FilletChamfer(d) = &mut after[index].modifier else { unreachable!() };
    if d.ids.is_empty() {
        return Err("select vertices at the Vertex level first".into());
    }
    let amount = if fillet { d.radius } else { d.distance };
    if amount <= 0.0 {
        return Err(if fillet { "set a Radius first".into() } else { "set a Distance first".to_string() });
    }
    // Operations name input vertices; the count guards against topology changes below.
    if d.ops.is_empty() {
        d.knots = input.knot_count();
    }
    d.ops.push(loom_scene::modifier::FilletOp { knots: d.ids.clone(), fillet, amount });
    d.ids.clear();
    commit_change(state, if fillet { "Fillet" } else { "Chamfer" }, Change::SetStack { id, before, after })?;
    Ok(json!({ "applied": true }))
}

/// Pick Locations click: Trim Only / Extend Only, or Auto (trim where the
/// spline crosses something there, else extend), decided on the input shape.
pub fn trim_extend_click(state: &mut AppState, point: [f32; 3]) -> Result<Value, String> {
    let extend = match state.spline_settings.trim_extend_op {
        1 => false,
        2 => true,
        _ => {
            let (id, index) = modifier_entry(state, |m| matches!(m, loom_scene::Modifier::TrimExtend { .. })).ok_or("select an object with a Trim/Extend modifier")?;
            let mut probe = state.scene.evaluated(id, Some(index)).value.shape.clone().ok_or("Trim/Extend needs a shape below it")?;
            !loom_shape::trim::trim(&mut probe, local_point(state, id, point))
        }
    };
    trim_extend_pick(state, point, extend)
}

fn trim_extend_pick(state: &mut AppState, point: [f32; 3], extend: bool) -> Result<Value, String> {
    let (id, index) = modifier_entry(state, |m| matches!(m, loom_scene::Modifier::TrimExtend { .. })).ok_or("select an object with a Trim/Extend modifier")?;
    let local = local_point(state, id, point);
    let n = state.scene.node(id).unwrap();
    let before = n.modifiers.clone();
    let mut after = before.clone();
    if let loom_scene::Modifier::TrimExtend { picks } = &mut after[index].modifier {
        picks.push((local, extend));
    }
    commit_change(state, if extend { "Extend" } else { "Trim" }, Change::SetStack { id, before, after })?;
    Ok(json!({ "picks": 1 }))
}
