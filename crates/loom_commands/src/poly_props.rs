//! Editable Poly > Vertex / Polygon Properties, Subdivision Surface and Paint
//! Deformation rollouts.

use crate::execute::{edit_meshes, selected_faces, selected_vertices, selection_json, set_sub_selection};
use crate::state::{AppState, PaintMode};
use crate::{Command as C, SelectionMode, SubObjectId};
use loom_mesh::{FaceId, VertexId};
use loom_scene::{Change, NodeId, PaintDirection, PolyObject};
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub fn execute(state: &mut AppState, cmd: C) -> Result<Value, String> {
    match cmd {
        C::SetMaterialId { id } => {
            let faces = faces_for_props(state)?;
            let mat = id.max(1) - 1;
            edit_meshes(state, "Set Material ID", faces, |mesh, _, ids| {
                for f in ids {
                    if let Some(face) = mesh.faces_mut().get_mut(f.0 as usize) {
                        face.material_id = mat;
                    }
                }
                Ok(())
            })?;
            Ok(json!({}))
        }
        C::SelectByMaterialId { id, clear } => {
            let mat = id.max(1) - 1;
            select_faces_where(state, clear, |f| f.material_id == mat)
        }
        C::SetSmoothingGroups { bits, add } => {
            let faces = faces_for_props(state)?;
            edit_meshes(state, "Smoothing Groups", faces, |mesh, _, ids| {
                for f in ids {
                    if let Some(face) = mesh.faces_mut().get_mut(f.0 as usize) {
                        face.smoothing_groups = if add { face.smoothing_groups | bits } else { bits };
                    }
                }
                Ok(())
            })?;
            Ok(json!({}))
        }
        C::ClearSmoothingGroups => execute(state, C::SetSmoothingGroups { bits: 0, add: false }),
        C::SelectBySmoothingGroup { bits, clear } => select_faces_where(state, clear, |f| f.smoothing_groups & bits != 0),
        C::AutoSmoothThreshold { degrees } => {
            state.poly_settings.auto_smooth_threshold = degrees.clamp(0.0, 180.0);
            Ok(json!({ "threshold": state.poly_settings.auto_smooth_threshold }))
        }
        C::AutoSmooth => {
            let threshold = state.poly_settings.auto_smooth_threshold;
            let faces = faces_for_props(state)?;
            edit_meshes(state, "Auto Smooth", faces, |mesh, _, ids| {
                mesh.auto_smooth_faces(&ids, threshold);
                Ok(())
            })?;
            Ok(json!({}))
        }
        C::SetVertexColor { rgb } => set_vertex_attr(state, "Vertex Color", |v| v.color = to_unit(rgb)),
        C::SetVertexIllumination { rgb } => set_vertex_attr(state, "Vertex Illumination", |v| v.illum = to_unit(rgb)),
        C::SetVertexAlpha { alpha } => {
            let a = (alpha / 100.0).clamp(0.0, 1.0);
            set_vertex_attr(state, "Vertex Alpha", move |v| v.alpha = a)
        }
        C::SelectByVertexColor { rgb, range, illumination } => {
            let target = to_unit(rgb);
            let tol = to_unit(range);
            let polys = selected_polys(state);
            let mut out = Vec::new();
            for n in polys {
                let mesh = state.scene.node(n).ok_or("missing node")?.cage_mesh().into_owned();
                let ids: Vec<u32> = mesh
                    .vertices()
                    .iter()
                    .enumerate()
                    .filter(|(_, v)| {
                        let c = if illumination { v.illum } else { v.color };
                        (0..3).all(|i| (c[i] - target[i]).abs() <= tol[i] + 1e-6)
                    })
                    .map(|(i, _)| i as u32)
                    .collect();
                out.push((n, ids));
            }
            state.selection_mode = SelectionMode::Vertex;
            set_sub_selection(state, SelectionMode::Vertex, out);
            Ok(selection_json(state))
        }
        C::SubdivisionSurface { use_nurms, smooth_result, isoline_display, show_cage, iterations, smoothness, separate_by_smoothing, separate_by_materials } => {
            poly_change(state, "Subdivision Surface", |p| {
                if let Some(v) = use_nurms { p.subdiv.use_nurms.set(v); }
                if let Some(v) = smooth_result { p.subdiv.smooth_result = v; }
                if let Some(v) = isoline_display { p.subdiv.isoline_display = v; }
                if let Some(v) = show_cage { p.subdiv.show_cage = v; }
                if let Some(v) = iterations { p.subdiv.iterations.set(v.min(10)); }
                if let Some(v) = smoothness { p.subdiv.smoothness.set(v.clamp(0.0, 1.0)); }
                if let Some(v) = separate_by_smoothing { p.subdiv.separate_by_smoothing = v; }
                if let Some(v) = separate_by_materials { p.subdiv.separate_by_materials = v; }
            })?;
            Ok(json!({}))
        }
        C::PaintDeformPushPull => toggle_paint(state, PaintMode::PushPull),
        C::PaintDeformRelax => toggle_paint(state, PaintMode::Relax),
        C::PaintDeformRevert => toggle_paint(state, PaintMode::Revert),
        C::PaintDeformCommit => {
            poly_change(state, "Paint Deform Commit", |p| p.paint.original = None)?;
            state.paint_deform = None;
            Ok(json!({}))
        }
        C::PaintDeformCancel => {
            poly_change(state, "Paint Deform Cancel", |p| {
                if let Some(orig) = p.paint.original.take() {
                    if orig.len() == p.mesh.vertex_count() {
                        for (v, o) in p.mesh.vertices_mut().iter_mut().zip(orig) {
                            v.position = o;
                        }
                    }
                }
            })?;
            state.paint_deform = None;
            Ok(json!({}))
        }
        C::PaintDeformSettings { value, size, strength, direction } => {
            let dir = match direction.as_deref() {
                None => None,
                Some("OriginalNormals") => Some(PaintDirection::OriginalNormals),
                Some("DeformedNormals") => Some(PaintDirection::DeformedNormals),
                Some("X") => Some(PaintDirection::TransformAxis(0)),
                Some("Y") => Some(PaintDirection::TransformAxis(1)),
                Some("Z") => Some(PaintDirection::TransformAxis(2)),
                Some(other) => return Err(format!("unknown direction {other:?}")),
            };
            for id in selected_polys(state) {
                if let Some(p) = state.scene.node_mut(id).and_then(|n| n.edit_poly_mut()) {
                    if let Some(v) = value { p.paint.push_pull_value = v; }
                    if let Some(v) = size { p.paint.brush_size = v.max(0.01); }
                    if let Some(v) = strength { p.paint.brush_strength = v.clamp(0.0, 1.0); }
                    if let Some(d) = dir { p.paint.direction = d; }
                }
            }
            Ok(json!({}))
        }
        C::PaintDeformStroke { node, points, mode, invert } => paint_stroke(state, NodeId(node), points, mode, invert),
        other => Err(format!("not an Editable Poly command: {other:?}")),
    }
}

fn to_unit(rgb: [u8; 3]) -> [f32; 3] {
    [rgb[0] as f32 / 255.0, rgb[1] as f32 / 255.0, rgb[2] as f32 / 255.0]
}

fn selected_polys(state: &AppState) -> Vec<NodeId> {
    state.selection.iter().copied().filter(|i| state.scene.node(*i).is_some_and(|n| n.edit_poly().is_some())).collect()
}

/// Polygons the property buttons act on: the selected polygons, or every
/// polygon when nothing is selected at a polygon level (Auto Smooth
/// then applies to the whole object).
fn faces_for_props(state: &AppState) -> Result<Vec<(NodeId, Vec<FaceId>)>, String> {
    let sel = selected_faces(state);
    if !sel.is_empty() {
        return Ok(sel);
    }
    if state.selection_mode != SelectionMode::Object {
        return Err("select polygons first (at the top level the whole object is affected)".into());
    }
    let all: Vec<(NodeId, Vec<FaceId>)> = selected_polys(state)
        .into_iter()
        .filter_map(|n| {
            let m = state.scene.node(n)?.cage_mesh().into_owned();
            Some((n, (0..m.face_count() as u32).map(FaceId).collect()))
        })
        .collect();
    if all.is_empty() {
        return Err("select an Editable Poly".into());
    }
    Ok(all)
}

fn select_faces_where(state: &mut AppState, clear: bool, pred: impl Fn(&loom_mesh::Face) -> bool) -> Result<Value, String> {
    let polys = selected_polys(state);
    if polys.is_empty() {
        return Err("select an Editable Poly".into());
    }
    let mut keep: Vec<(NodeId, SubObjectId)> = if clear { Vec::new() } else { state.sub_selection.iter().collect() };
    for n in polys {
        let mesh = state.scene.node(n).ok_or("missing node")?.cage_mesh().into_owned();
        for (i, f) in mesh.faces().iter().enumerate() {
            if pred(f) && !f.hidden {
                keep.push((n, SubObjectId::Face(FaceId(i as u32))));
            }
        }
    }
    if !state.selection_mode.is_face_level() {
        state.selection_mode = SelectionMode::Face;
    }
    state.sub_selection.clear();
    for (n, id) in keep {
        if let SubObjectId::Face(_) = id {
            state.sub_selection.insert(n, id);
        }
    }
    Ok(selection_json(state))
}

/// Vertex colour edits apply to the selected vertices, or to the vertices
/// of the selected edges / polygons (Polygon Properties colours).
fn set_vertex_attr(state: &mut AppState, label: &str, f: impl Fn(&mut loom_mesh::Vertex) + Copy) -> Result<Value, String> {
    let verts = selected_vertices(state);
    if verts.is_empty() {
        return Err("select sub-objects to colour".into());
    }
    edit_meshes(state, label, verts, |mesh, _, ids| {
        for v in ids {
            if let Some(vert) = mesh.vertices_mut().get_mut(v.0 as usize) {
                f(vert);
            }
        }
        Ok(())
    })?;
    Ok(json!({}))
}

/// Undoable change of a poly's rollout settings (not the mesh).
fn poly_change(state: &mut AppState, label: &str, f: impl Fn(&mut PolyObject)) -> Result<(), String> {
    let mut changes = Vec::new();
    for id in selected_polys(state) {
        let node = state.scene.node(id).ok_or("missing node")?;
        let Some(mut p) = node.edit_poly().cloned() else { continue };
        f(&mut p);
        changes.push(crate::execute::replace_poly_change(node, p));
    }
    if changes.is_empty() {
        return Err("select an Editable Poly".into());
    }
    crate::execute::commit_change(state, label, Change::Group(changes))
}

fn toggle_paint(state: &mut AppState, mode: PaintMode) -> Result<Value, String> {
    state.paint_deform = if state.paint_deform == Some(mode) { None } else { Some(mode) };
    state.soft_paint = None;
    Ok(json!({ "paint_deform": state.paint_deform.map(|m| format!("{m:?}")) }))
}

/// One brush stroke. The first stroke after a Commit / Cancel remembers the
/// original positions for Revert / Cancel. Each stroke is one undo entry.
fn paint_stroke(state: &mut AppState, id: NodeId, points: Vec<[f32; 3]>, mode: PaintMode, invert: bool) -> Result<Value, String> {
    let node = state.scene.node(id).ok_or("missing node")?;
    let Some(mut p) = node.edit_poly().cloned() else { return Err("not an Editable Poly".into()) };
    let world = state.scene.object_matrix(id);
    let inv = world.inverse();
    let n = p.mesh.vertex_count();
    if p.paint.original.as_ref().is_none_or(|o| o.len() != n) {
        p.paint.original = Some(p.mesh.vertices().iter().map(|v| v.position).collect());
    }
    let original = p.paint.original.clone().unwrap();
    let radius = p.paint.brush_size.max(1e-3);
    let strength = p.paint.brush_strength.clamp(0.0, 1.0);
    let value = p.paint.push_pull_value * if invert { -1.0 } else { 1.0 };
    // Normals of the original surface for Original Normals.
    let orig_mesh = {
        let mut m = p.mesh.clone();
        for (v, o) in m.vertices_mut().iter_mut().zip(&original) {
            v.position = *o;
        }
        m
    };
    let axis = |i: u8| match i { 0 => glam::Vec3::X, 1 => glam::Vec3::Y, _ => glam::Vec3::Z };
    let mut touched: BTreeSet<usize> = BTreeSet::new();
    for pt in &points {
        let c = inv.transform_point3(glam::Vec3::from(*pt));
        let snapshot: Vec<glam::Vec3> = p.mesh.vertices().iter().map(|v| v.position).collect();
        for i in 0..n {
            let d = snapshot[i].distance(c);
            if d > radius {
                continue;
            }
            let k = strength * (1.0 - d / radius);
            let vid = VertexId(i as u32);
            let target = match mode {
                PaintMode::PushPull => {
                    let dir = match p.paint.direction {
                        PaintDirection::OriginalNormals => vertex_normal(&orig_mesh, vid),
                        PaintDirection::DeformedNormals => vertex_normal(&p.mesh, vid),
                        PaintDirection::TransformAxis(a) => axis(a),
                    };
                    snapshot[i] + dir * value * k * 0.1
                }
                PaintMode::Relax => {
                    let nb = loom_mesh::select_ops::vertex_neighbours(&p.mesh, vid);
                    if nb.is_empty() { snapshot[i] } else {
                        let avg = nb.iter().map(|v| snapshot[v.0 as usize]).sum::<glam::Vec3>() / nb.len() as f32;
                        snapshot[i] + (avg - snapshot[i]) * k
                    }
                }
                PaintMode::Revert => snapshot[i] + (original[i] - snapshot[i]) * k,
            };
            p.mesh.vertices_mut()[i].position = target;
            touched.insert(i);
        }
    }
    if touched.is_empty() {
        return Ok(json!({ "moved": 0 }));
    }
    state.paint_deform.get_or_insert(mode);
    let change = crate::execute::replace_poly_change(state.scene.node(id).ok_or("missing node")?, p);
    crate::execute::commit_change(state, "Paint Deform", change)?;
    Ok(json!({ "moved": touched.len() }))
}

fn vertex_normal(mesh: &loom_mesh::Mesh, v: VertexId) -> glam::Vec3 {
    let mut n = glam::Vec3::ZERO;
    for f in mesh.faces_around_vertex(v) {
        n += mesh.face_normal(f);
    }
    n.normalize_or_zero()
}
