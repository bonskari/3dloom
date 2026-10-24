//! Utilities > Assign Vertex Colors: the scene lighting and the material's
//! diffuse colour baked into vertex colours, held by a VertexPaint modifier
//! on each selected object (a new one from the utility, the active one from
//! the modifier's Assign). Light Model: Lighting + Diffuse, Lighting Only or
//! Diffuse; Color by Face samples each face centre, Color by Vertex each
//! vertex; Shadows casts a ray to every light that has shadows on. Map
//! sampling (Mapping) and radiosity are not part of it.

use crate::execute::commit_change;
use crate::state::AppState;
use glam::Vec3;
use loom_mesh::{FaceId, Mesh};
use loom_scene::light::{scene_lights, srgb_to_linear, Emitter, LightSample};
use loom_scene::{Change, Modifier, ModifierSlot, NodeId, ObjectKind};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// Light Model list.
pub const LIGHT_MODELS: [&str; 3] = ["Lighting + Diffuse", "Lighting Only", "Diffuse"];

fn linear_to_srgb(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    if x <= 0.0031308 { x * 12.92 } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 }
}

/// Every other shadow-casting object's world triangles.
fn occluders(state: &AppState, light: &LightSample) -> Vec<(NodeId, [Vec3; 3])> {
    let mut out = Vec::new();
    for n in state.scene.nodes() {
        if n.hidden || matches!(n.object, ObjectKind::Camera { .. }) || n.object.light().is_some() || n.object.is_shape() || light.exclusion.excludes(n.id, false) {
            continue;
        }
        let world = state.scene.object_matrix(n.id);
        let m = &state.scene.eval_mesh(n.id).value.mesh;
        for f in 0..m.face_count() {
            let Some(p) = m.face_positions(FaceId(f as u32)) else { continue };
            for i in 1..p.len().saturating_sub(1) {
                out.push((n.id, [world.transform_point3(p[0]), world.transform_point3(p[i]), world.transform_point3(p[i + 1])]));
            }
        }
    }
    out
}

/// Linear light reaching world point `p` with normal `n` from every light.
fn irradiance(lights: &[(LightSample, Vec<(NodeId, [Vec3; 3])>)], id: NodeId, p: Vec3, n: Vec3, shadows: bool) -> Vec3 {
    let mut sum = Vec3::ZERO;
    for (l, occ) in lights {
        if l.exclusion.excludes(id, true) {
            continue;
        }
        if l.emitter == Emitter::Ambient {
            sum += l.color;
            continue;
        }
        let (to_light, dist) = match l.emitter {
            Emitter::Directional => (-l.direction, f32::INFINITY),
            _ => {
                let d = l.position - p;
                (d.normalize_or_zero(), d.length())
            }
        };
        let lambert = n.dot(to_light);
        if lambert <= 0.0 {
            continue;
        }
        let k = l.falloff(if dist.is_finite() { dist } else { 0.0 }, (-to_light).dot(l.direction));
        if k <= 0.0 {
            continue;
        }
        if shadows && l.shadows {
            let origin = p + n * 1e-3 * (1.0 + p.length());
            let blocked = occ.iter().any(|(_, [a, b, c])| ray_triangle(origin, to_light, *a, *b, *c).is_some_and(|t| t > 1e-4 && t < dist));
            if blocked {
                continue;
            }
        }
        sum += l.color * lambert * k;
    }
    sum
}

/// Möller–Trumbore, both sides.
fn ray_triangle(o: Vec3, d: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let (e1, e2) = (b - a, c - a);
    let h = d.cross(e2);
    let det = e1.dot(h);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - a;
    let u = s.dot(h) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    Some(e2.dot(q) * inv)
}

/// The colours (swatch space, 0..1) for every vertex of `id`'s current result.
pub fn vertex_colors(state: &AppState, id: NodeId, model: u32, by_vertex: bool, shadows: bool) -> Option<Vec<[f32; 3]>> {
    let node = state.scene.node(id)?;
    let mesh: Mesh = state.scene.eval_mesh(id).value.mesh.clone();
    let world = state.scene.object_matrix(id);
    let diffuse = match state.scene.node_material(id) {
        Some(m) => srgb_to_linear(m.diffuse.get()),
        None => srgb_to_linear(Vec3::new(node.wire_color[0] as f32, node.wire_color[1] as f32, node.wire_color[2] as f32) / 255.0),
    };
    // Lighting Only has no Shadows (the reference disables the toggle).
    let shadows = shadows && model != 1;
    let mut lights: Vec<LightSample> = scene_lights(&state.scene, |_| true);
    if lights.is_empty() {
        // No lights: the default lighting, a light from above.
        lights = loom_scene::light::default_lights(1, glam::Mat4::IDENTITY);
    }
    let with_occ: Vec<(LightSample, Vec<(NodeId, [Vec3; 3])>)> = lights.into_iter().map(|l| {
        let occ = if shadows && l.shadows && model != 2 { occluders(state, &l) } else { Vec::new() };
        (l, occ)
    }).collect();
    let shade = |p: Vec3, n: Vec3| -> Vec3 {
        let e = if model == 2 { Vec3::ONE } else { irradiance(&with_occ, id, p, n, shadows) };
        match model {
            1 => e,
            2 => diffuse,
            _ => diffuse * e,
        }
    };
    let normal_world = |n: Vec3| world.inverse().transpose().transform_vector3(n).normalize_or_zero();
    let mut out = vec![Vec3::ZERO; mesh.vertex_count()];
    if by_vertex {
        let mut normals = vec![Vec3::ZERO; mesh.vertex_count()];
        for f in 0..mesh.face_count() {
            let fid = FaceId(f as u32);
            let fn_ = mesh.face_normal(fid);
            for v in mesh.face_vertex_ids(fid).unwrap_or_default() {
                normals[v.0 as usize] += fn_;
            }
        }
        for (v, o) in out.iter_mut().enumerate() {
            let p = world.transform_point3(mesh.vertices()[v].position);
            *o = shade(p, normal_world(normals[v]));
        }
    } else {
        // Color by Face: each face centre's colour, averaged at the vertices.
        let mut count = vec![0.0f32; mesh.vertex_count()];
        for f in 0..mesh.face_count() {
            let fid = FaceId(f as u32);
            let vs = mesh.face_vertex_ids(fid).unwrap_or_default();
            if vs.is_empty() {
                continue;
            }
            let c = vs.iter().filter_map(|v| mesh.vertex_position(*v)).sum::<Vec3>() / vs.len() as f32;
            let col = shade(world.transform_point3(c), normal_world(mesh.face_normal(fid)));
            for v in vs {
                out[v.0 as usize] += col;
                count[v.0 as usize] += 1.0;
            }
        }
        for (o, c) in out.iter_mut().zip(count) {
            if c > 0.0 {
                *o /= c;
            }
        }
    }
    Some(out.into_iter().map(|c| [linear_to_srgb(c.x), linear_to_srgb(c.y), linear_to_srgb(c.z)]).collect())
}

fn write_layer(m: &mut Modifier, colors: &[[f32; 3]]) {
    if let Modifier::VertexPaint { paint, coverage, .. } = m {
        *paint = colors.iter().enumerate().map(|(i, c)| (i as u32, *c)).collect();
        *coverage = BTreeMap::new();
    }
}

/// Assign To Selected (`update` false: a new VertexPaint on top of each
/// selected object) or the modifier's Assign (`update` true: the active
/// VertexPaint entry is refilled).
pub fn assign(state: &mut AppState, model: u32, by_vertex: bool, shadows: bool, update: bool) -> Result<Value, String> {
    if model as usize >= LIGHT_MODELS.len() {
        return Err("Light Model is 0 Lighting + Diffuse, 1 Lighting Only or 2 Diffuse".into());
    }
    let ids: Vec<NodeId> = state.selection.iter().copied().filter(|id| state.scene.node(*id).is_some_and(|n| !matches!(n.object, ObjectKind::Camera { .. }) && n.object.light().is_none() && !n.object.is_shape())).collect();
    if ids.is_empty() {
        return Err("select the geometry to assign vertex colours to".into());
    }
    let mut changes = Vec::new();
    let mut assigned = 0;
    for id in &ids {
        let node = state.scene.node(*id).ok_or("missing node")?;
        let before = node.modifiers.clone();
        let mut after = before.clone();
        if update {
            let a = node.active_entry.filter(|a| matches!(before.get(*a).map(|s| &s.modifier), Some(Modifier::VertexPaint { .. }))).ok_or("Assign updates the active VertexPaint entry")?;
            // The colours of what enters the entry.
            let input = match a.checked_sub(1) {
                Some(prev) => state.scene.evaluated(*id, Some(prev)).value.mesh.vertex_count(),
                None => node.object.display_output().vertex_count(),
            };
            let mut cols = vertex_colors(state, *id, model, by_vertex, shadows).ok_or("missing node")?;
            cols.truncate(input);
            write_layer(&mut after[a].modifier, &cols);
        } else {
            let cols = vertex_colors(state, *id, model, by_vertex, shadows).ok_or("missing node")?;
            let mut m = Modifier::by_name("VertexPaint").ok_or("no VertexPaint")?;
            write_layer(&mut m, &cols);
            let first_wsm = after.iter().position(|s| s.modifier.is_wsm()).unwrap_or(after.len());
            after.insert(first_wsm, ModifierSlot::new(m));
        }
        changes.push(Change::SetStack { id: *id, before, after });
        assigned += 1;
    }
    commit_change(state, "Assign Vertex Colors", Change::Group(changes))?;
    if !update {
        for id in &ids {
            let top = state.scene.node(*id).and_then(|n| n.modifiers.iter().rposition(|s| matches!(s.modifier, Modifier::VertexPaint { .. })));
            if let Some(n) = state.scene.transform_mut(*id) {
                n.active_entry = top;
            }
        }
    }
    state.mesh_dirty = true;
    Ok(json!({ "assigned": assigned }))
}
