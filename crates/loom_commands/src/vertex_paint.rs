//! VertexPaint dialog commands: Pick Color From Object, face selection,
//! Paint All and New Layer. The brush itself (`VertexPaintStroke`) is in
//! `stack.rs`.

use crate::command::{Command, SelectOp};
use crate::execute::commit_change;
use crate::state::AppState;
use glam::Vec3;
use loom_mesh::{FaceId, Mesh};
use loom_scene::{Change, Modifier, ModifierSlot, NodeId};
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub fn handles(c: &Command) -> bool {
    use Command as C;
    matches!(c, C::VertexPaintPickColorToggle | C::VertexPaintFaceSelectToggle | C::VertexPaintSelectFaces { .. } | C::VertexPaintPaintAll | C::VertexPaintPickColor { .. } | C::VertexPaintNewLayer { .. })
}

/// The selected node's active VertexPaint entry.
pub fn entry(state: &AppState) -> Option<(NodeId, usize)> {
    state.selection.iter().find_map(|id| {
        let n = state.scene.node(*id)?;
        let a = n.active_entry?;
        matches!(n.modifiers.get(a)?.modifier, Modifier::VertexPaint { .. }).then_some((*id, a))
    })
}

/// The mesh entering stack entry `index` of `id`.
pub fn input_mesh(state: &AppState, id: NodeId, index: usize) -> Mesh {
    match index.checked_sub(1) {
        Some(prev) => state.scene.evaluated(id, Some(prev)).value.mesh.clone(),
        None => state.scene.node(id).map(|n| n.object.display_output()).unwrap_or_default(),
    }
}

/// The input vertices the selected faces use (None: no face selection, every vertex).
pub fn face_vertices(input: &Mesh, faces: &BTreeSet<u32>) -> Option<BTreeSet<u32>> {
    if faces.is_empty() {
        return None;
    }
    Some(faces.iter().filter_map(|f| input.face_vertex_ids(FaceId(*f))).flatten().map(|v| v.0).collect())
}

/// Composites `target` at `k` onto a layer vertex's colour and coverage.
pub fn composite(paint: &mut std::collections::BTreeMap<u32, [f32; 3]>, coverage: &mut std::collections::BTreeMap<u32, f32>, v: u32, target: [f32; 3], k: f32) {
    let (c, a) = match paint.get(&v) {
        Some(c) => (*c, coverage.get(&v).copied().unwrap_or(1.0)),
        None => (target, 0.0),
    };
    let a2 = a + k * (1.0 - a);
    let mix = |c: f32, t: f32| if a2 > 0.0 { (c * a * (1.0 - k) + t * k) / a2 } else { t };
    paint.insert(v, [mix(c[0], target[0]), mix(c[1], target[1]), mix(c[2], target[2])]);
    coverage.insert(v, a2);
}

fn edit(state: &mut AppState, label: &str, f: impl FnOnce(&mut Modifier, &Mesh) -> Result<(), String>) -> Result<Value, String> {
    let (id, index) = entry(state).ok_or("select an object with an active VertexPaint modifier")?;
    let input = input_mesh(state, id, index);
    let before = state.scene.node(id).ok_or("missing node")?.modifiers.clone();
    let mut after = before.clone();
    f(&mut after[index].modifier, &input)?;
    if after != before {
        commit_change(state, label, Change::SetStack { id, before, after })?;
    }
    Ok(state_json(state))
}

/// The active entry's face selection, colour and layer settings.
pub fn state_json(state: &AppState) -> Value {
    let Some((id, index)) = entry(state) else { return Value::Null };
    let Some(Modifier::VertexPaint { faces, color, layer_opacity, mode, .. }) = state.scene.node(id).and_then(|n| n.modifiers.get(index)).map(|s| &s.modifier) else { return Value::Null };
    json!({
        "node": id.0,
        "index": index,
        "faces": faces.iter().collect::<Vec<_>>(),
        "color": [color[0].get(), color[1].get(), color[2].get()],
        "layer_opacity": layer_opacity.get(),
        "mode": loom_scene::modifier::VERTEX_PAINT_MODES.get(mode.get() as usize),
        "brush": state.vertex_paint_brush,
        "pick_color": state.vertex_paint_pick,
        "face_select": state.vertex_paint_faces,
    })
}

/// The channel colour at `point` (object space) of the result the entry
/// shows: the nearest face's corners blended by inverse distance.
fn colour_at(mesh: &Mesh, channel: u32, point: Vec3) -> Option<[f32; 3]> {
    let value = |v: usize| -> [f32; 3] {
        let x = &mesh.vertices()[v];
        match channel {
            0 => x.color,
            1 => x.illum,
            _ => [x.alpha; 3],
        }
    };
    let mut best: Option<(f32, Vec<usize>)> = None;
    for f in 0..mesh.face_count() {
        let Some(vs) = mesh.face_vertex_ids(FaceId(f as u32)) else { continue };
        let pts: Vec<Vec3> = vs.iter().filter_map(|v| mesh.vertex_position(*v)).collect();
        if pts.len() < 3 {
            continue;
        }
        let d = (1..pts.len() - 1).map(|i| triangle_distance(point, pts[0], pts[i], pts[i + 1])).fold(f32::INFINITY, f32::min);
        if best.as_ref().is_none_or(|(bd, _)| d < *bd) {
            best = Some((d, vs.iter().map(|v| v.0 as usize).collect()));
        }
    }
    let (_, vs) = best?;
    let mut sum = [0.0f32; 3];
    let mut wsum = 0.0;
    for v in vs {
        let d = mesh.vertices()[v].position.distance(point);
        if d < 1e-5 {
            return Some(value(v));
        }
        let w = 1.0 / d;
        let c = value(v);
        for k in 0..3 {
            sum[k] += c[k] * w;
        }
        wsum += w;
    }
    Some(sum.map(|s| s / wsum))
}

/// Distance from `p` to the triangle `a b c`.
fn triangle_distance(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> f32 {
    let n = (b - a).cross(c - a);
    if n.length_squared() < 1e-12 {
        return p.distance(a).min(p.distance(b)).min(p.distance(c));
    }
    let n = n.normalize();
    let q = p - n * (p - a).dot(n);
    let inside = [(a, b), (b, c), (c, a)].iter().all(|(u, v)| (*v - *u).cross(q - *u).dot(n) >= -1e-6);
    if inside {
        return (p - q).length();
    }
    let seg = |u: Vec3, v: Vec3| {
        let t = ((p - u).dot(v - u) / (v - u).length_squared().max(1e-12)).clamp(0.0, 1.0);
        p.distance(u + (v - u) * t)
    };
    seg(a, b).min(seg(b, c)).min(seg(c, a))
}

pub fn execute(state: &mut AppState, cmd: Command) -> Result<Value, String> {
    use Command as C;
    match cmd {
        C::VertexPaintPickColorToggle => {
            state.vertex_paint_pick = !state.vertex_paint_pick;
            if state.vertex_paint_pick {
                state.vertex_paint_brush = false;
                state.vertex_paint_faces = false;
            }
            Ok(state_json(state))
        }
        C::VertexPaintFaceSelectToggle => {
            state.vertex_paint_faces = !state.vertex_paint_faces;
            if state.vertex_paint_faces {
                state.vertex_paint_brush = false;
                state.vertex_paint_pick = false;
            }
            Ok(state_json(state))
        }
        C::VertexPaintSelectFaces { ids, op } => edit(state, "Select Faces", |m, input| {
            let Modifier::VertexPaint { faces, .. } = m else { unreachable!() };
            let ids = ids.into_iter().filter(|f| (*f as usize) < input.face_count());
            match op {
                SelectOp::Replace => *faces = ids.collect(),
                SelectOp::Add => faces.extend(ids),
                SelectOp::Subtract => {
                    let gone: BTreeSet<u32> = ids.collect();
                    faces.retain(|f| !gone.contains(f));
                }
            }
            Ok(())
        }),
        C::VertexPaintPaintAll => edit(state, "Paint All", |m, input| {
            let Modifier::VertexPaint { faces, color, opacity, paint, coverage, .. } = m else { unreachable!() };
            let target = [color[0].get(), color[1].get(), color[2].get()];
            let k = opacity.get().clamp(0.0, 1.0);
            let only = face_vertices(input, faces);
            for v in 0..input.vertex_count() as u32 {
                if only.as_ref().is_none_or(|o| o.contains(&v)) {
                    composite(paint, coverage, v, target, k);
                }
            }
            Ok(())
        }),
        C::VertexPaintPickColor { node, point } => {
            let id = NodeId(node);
            let (eid, index) = entry(state).filter(|(e, _)| *e == id).ok_or("the node has no active VertexPaint entry")?;
            let Some(Modifier::VertexPaint { channel, .. }) = state.scene.node(eid).map(|n| &n.modifiers[index].modifier) else { unreachable!() };
            let shown = state.scene.evaluated(id, Some(index)).value.mesh.clone();
            let c = colour_at(&shown, channel.get(), Vec3::from(point)).ok_or("no surface to pick a colour from")?;
            edit(state, "Pick Color", |m, _| {
                let Modifier::VertexPaint { color, .. } = m else { unreachable!() };
                for k in 0..3 {
                    color[k].set(c[k].clamp(0.0, 1.0));
                }
                Ok(())
            })
        }
        C::VertexPaintNewLayer { name, mode } => {
            let (id, index) = entry(state).ok_or("select an object with an active VertexPaint modifier")?;
            let before = state.scene.node(id).ok_or("missing node")?.modifiers.clone();
            let mut after = before.clone();
            let Modifier::VertexPaint { channel, brush_size, opacity, color, .. } = &before[index].modifier else { unreachable!() };
            let mut layer = Modifier::by_name("VertexPaint").ok_or("no VertexPaint")?;
            if let Modifier::VertexPaint { channel: c, brush_size: b, opacity: o, color: col, mode: m, .. } = &mut layer {
                *c = channel.clone();
                *b = brush_size.clone();
                *o = opacity.clone();
                *col = color.clone();
                m.set(mode.min(loom_scene::modifier::VERTEX_PAINT_MODES.len() as u32 - 1));
            }
            let mut slot = ModifierSlot::new(layer);
            if !name.trim().is_empty() {
                slot.name = name.trim().to_owned();
            }
            after.insert(index + 1, slot);
            commit_change(state, "New Layer", Change::SetStack { id, before, after })?;
            if let Some(n) = state.scene.transform_mut(id) {
                n.active_entry = Some(index + 1);
            }
            state.mesh_dirty = true;
            Ok(state_json(state))
        }
        _ => Err("not a VertexPaint command".into()),
    }
}
