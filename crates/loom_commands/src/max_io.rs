//! .max scene files (loom_max reads them) as a Loom document: nodes keep
//! their names, hierarchy, transforms, pivot offsets, wire colours and
//! layers; Editable Poly / Editable Mesh become Editable Poly, Box /
//! Cylinder / Plane stay parametric, splines become Editable Splines, Free
//! cameras and Omni / Target Direct lights become cameras and lights (aimed
//! at their targets), Dummies stay Dummies and anything else becomes a
//! Dummy so its children keep their place. The modifiers Loom has are
//! rebuilt on the stack with their parameters, gizmos, FFD lattices and
//! Unwrap UVW texture vertices; Standard and Multi/Sub-Object materials come
//! with their maps. File > Import and File > Merge both read it this way.

use crate::state::{AppState, SelectionMode};
use loom_max::{MaxKind, MaxMaterial};
use loom_scene::material::{MaterialId, MaterialKind, StandardMaterial, SubMaterial};
use loom_scene::{Modifier, ModifierSlot, Node, NodeId, ObjectKind, Transform};
use serde_json::{json, Value};
use std::path::Path;

pub fn is_max(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("max"))
}

/// A map file the file names by an absolute path on another machine: the
/// same name next to the .max file when the path itself is not there.
fn resolve_map(path: &str, max: &Path) -> Option<String> {
    let direct = Path::new(path);
    if direct.is_file() {
        return Some(path.to_owned());
    }
    let name = Path::new(&path.replace('\\', "/")).file_name()?.to_owned();
    let near = max.parent().unwrap_or(Path::new(".")).join(name);
    near.is_file().then(|| near.to_string_lossy().into_owned())
}

fn material(m: &MaxMaterial, ids: &[MaterialId], max: &Path, w: &mut Vec<String>) -> StandardMaterial {
    let v = |c: [f32; 3]| glam::Vec3::from(c).clamp(glam::Vec3::ZERO, glam::Vec3::ONE);
    match m {
        MaxMaterial::Standard(s) => {
            let mut out = StandardMaterial::new(s.name.clone());
            out.ambient.set(v(s.ambient));
            out.diffuse.set(v(s.diffuse));
            out.specular.set(v(s.specular));
            out.ambient_locked = s.ambient == s.diffuse;
            if let Some(x) = s.specular_level {
                out.specular_level.set(x.clamp(0.0, 999.0));
            }
            if let Some(x) = s.glossiness {
                out.glossiness.set(x.clamp(0.0, 100.0));
            }
            if let Some(x) = s.soften {
                out.soften.set(x.clamp(0.0, 1.0));
            }
            if let Some(x) = s.opacity {
                out.opacity.set(x.clamp(0.0, 100.0));
            }
            for (ch, path) in &s.maps {
                let Some((slot, amount)) = loom_scene::material::MAP_CHANNELS.get(*ch) else {
                    w.push(format!("{}: map channel {ch} is not one Loom has", s.name));
                    continue;
                };
                let file = resolve_map(path, max).unwrap_or_else(|| {
                    w.push(format!("{}: map file {path} was not found", s.name));
                    path.clone()
                });
                if let Ok(slot) = out.map_slot_mut(slot) {
                    *slot = Some(loom_scene::material::Map::bitmap(file, *amount));
                }
            }
            out.show_map_in_viewport = out.diffuse_map.is_some();
            out
        }
        MaxMaterial::MultiSub { name, subs } => {
            let mut out = StandardMaterial::new(name.clone());
            out.kind = MaterialKind::MultiSub(subs.iter().map(|s| SubMaterial { id: s.id, name: String::new(), material: s.material.and_then(|i| ids.get(i).copied()), enabled: s.enabled }).collect());
            out
        }
    }
}

fn object(kind: &MaxKind) -> Result<ObjectKind, String> {
    Ok(match kind {
        MaxKind::Mesh(m) => ObjectKind::EditablePoly(loom_scene::PolyObject::from_mesh(m.clone())),
        MaxKind::Box { length, width, height, length_segs, width_segs, height_segs } => {
            let mut o = ObjectKind::box_object(*length, *width, *height);
            for (p, v) in [("Length Segs", *length_segs), ("Width Segs", *width_segs), ("Height Segs", *height_segs)] {
                o.set_parameter(p, v as f64)?;
            }
            o
        }
        MaxKind::Cylinder { radius, height, height_segs, cap_segs, sides, smooth, slice_on, slice_from, slice_to } => {
            let mut o = ObjectKind::cylinder_object(*radius, *height);
            for (p, v) in [("Height Segments", *height_segs as f64), ("Cap Segments", *cap_segs as f64), ("Sides", *sides as f64), ("Slice On", *slice_on as u8 as f64), ("Slice From", *slice_from as f64), ("Slice To", *slice_to as f64)] {
                o.set_parameter(p, v)?;
            }
            let _ = smooth;
            o
        }
        MaxKind::Plane { length, width, length_segs, width_segs } => {
            let mut o = ObjectKind::plane_object(*length, *width);
            o.set_parameter("Length Segs", *length_segs as f64)?;
            o.set_parameter("Width Segs", *width_segs as f64)?;
            o
        }
        MaxKind::Shape(s) => {
            use loom_shape::{Knot, KnotKind, Spline};
            let splines = s
                .splines
                .iter()
                .map(|sp| {
                    let knots = sp
                        .knots
                        .iter()
                        .map(|k| Knot {
                            pos: k.pos,
                            in_vec: k.in_handle - k.pos,
                            out_vec: k.out_handle - k.pos,
                            kind: match k.kind {
                                loom_max::KnotType::Smooth => KnotKind::Smooth,
                                loom_max::KnotType::Corner => KnotKind::Corner,
                                loom_max::KnotType::Bezier => KnotKind::Bezier,
                                loom_max::KnotType::BezierCorner => KnotKind::BezierCorner,
                            },
                        })
                        .collect();
                    Spline::new(knots, sp.closed)
                })
                .collect();
            let shape = loom_shape::Shape { splines, steps: s.steps, ..Default::default() };
            ObjectKind::EditableSpline(loom_scene::ShapeObject { shape, soft: Default::default(), is_line: s.line, hidden: Default::default() })
        }
        MaxKind::Camera { params } => {
            let mut o = ObjectKind::camera_object(45.0);
            for (p, v) in params {
                o.set_parameter(p, *v)?;
            }
            o
        }
        MaxKind::Light { kind, params } => {
            let t = if *kind == "omni" { loom_scene::light::LightType::Omni } else { loom_scene::light::LightType::Directional };
            let mut l = loom_scene::light::LightObject::new(t);
            for (p, v) in params {
                l.set_parameter(p, *v)?;
            }
            ObjectKind::Light(Box::new(l))
        }
        MaxKind::Target => ObjectKind::dummy(crate::camera::TARGET_SIZE),
        MaxKind::Dummy { size } => ObjectKind::dummy(size.max_element().max(0.001)),
        MaxKind::Other(_) => ObjectKind::dummy(10.0),
    })
}

fn modifier(m: &loom_max::MaxModifier, node: &str, w: &mut Vec<String>) -> Option<ModifierSlot> {
    let Some(mut md) = Modifier::by_name(m.class) else {
        w.push(format!("{node}: {} is not a Loom modifier", m.class));
        return None;
    };
    for (p, v) in &m.params {
        if let Err(e) = md.set_parameter(p, *v) {
            w.push(format!("{node}: {} {p}: {e}", m.class));
        }
    }
    if let Some(g) = m.gizmo {
        match md.gizmo_mut() {
            Some(gz) => *gz = Transform::from_matrix(g),
            None => w.push(format!("{node}: the {} gizmo (axis) is not imported", m.class)),
        }
    }
    if let (Some(c), Some(d)) = (m.center, md.deformer_mut()) {
        d.center.set(c);
    }
    if let Some(l) = &m.lattice {
        // Loom keeps the lattice volume in object space and the control points in
        // gizmo space; the file keeps both in gizmo space. They agree while the
        // gizmo only moves.
        let g = m.gizmo.unwrap_or(glam::Mat4::IDENTITY);
        if (glam::Mat3::from_mat4(g) - glam::Mat3::IDENTITY).to_cols_array().iter().any(|x| x.abs() > 1e-5) {
            w.push(format!("{node}: {} lattice is turned or scaled against the object; not imported", m.class));
            return None;
        }
        md.fit_ffd(g.transform_point3(l.lo), g.transform_point3(l.hi));
        if let Modifier::FFD { points, dims, .. } = &mut md {
            if *dims == l.dims && points.len() == l.points.len() {
                let size = l.hi - l.lo;
                *points = l.points.iter().map(|p| l.lo + *p * size).collect();
            }
        }
    }
    let mut slot = ModifierSlot::new(md);
    if !m.name.is_empty() {
        slot.name = m.name.clone();
    }
    Some(slot)
}

/// Unwrap UVW's texture vertices onto its input: faces matched by index
/// with the same geometry vertices in the same turn.
fn unwrap_edits(input: &loom_mesh::Mesh, uvs: &loom_max::UnwrapUvs, data: &mut loom_scene::modifier::UnwrapUvw) -> Result<(), String> {
    if input.face_count() != uvs.faces.len() {
        return Err(format!("its input has {} polygons, the file {}", input.face_count(), uvs.faces.len()));
    }
    data.init(input);
    let mut corners = vec![0u32; input.half_edge_count()];
    for (f, (tv, gv)) in uvs.faces.iter().enumerate() {
        let hes = input.face_corners(loom_mesh::FaceId(f as u32)).unwrap_or_default();
        let ids: Vec<u32> = hes.iter().map(|h| input.half_edges()[h.0 as usize].origin.0).collect();
        if ids.len() != gv.len() {
            return Err(format!("polygon {f} has {} corners in its input, {} in the file", ids.len(), gv.len()));
        }
        let turn = (0..ids.len()).find(|s| (0..ids.len()).all(|k| ids[k] == gv[(k + s) % ids.len()])).ok_or_else(|| format!("polygon {f} has other vertices in its input"))?;
        for (k, h) in hes.iter().enumerate() {
            corners[h.0 as usize] = tv[(k + turn) % ids.len()];
        }
    }
    data.edits = Some(loom_mesh::half_edge::MapChannel { verts: uvs.verts.clone(), corners });
    Ok(())
}

/// Reads a .max file as a Loom document (fresh ids from 1, its materials
/// and layers) and the warnings for what it leaves out.
pub fn document(path: &Path) -> Result<(crate::file::Document, Vec<String>), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let doc = loom_max::read(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut w = doc.warnings.clone();
    let mut scene = loom_scene::Scene::new();
    // Materials first, so Multi/Sub-Object rows find their sub-materials' ids.
    let ids: Vec<MaterialId> = (0..doc.materials.len()).map(|i| MaterialId(i as u32 + 1)).collect();
    for (i, m) in doc.materials.iter().enumerate() {
        scene.materials.insert(ids[i], material(m, &ids, path, &mut w));
    }
    // Layers by name: "0" is the scene's own layer 0.
    let mut layer_ids = Vec::with_capacity(doc.layers.len());
    for l in &doc.layers {
        let id = match scene.layers.iter().find(|x| x.name == l.name) {
            Some(x) => x.id,
            None => {
                let id = scene.layers.iter().map(|x| x.id + 1).max().unwrap_or(1);
                let mut nl = loom_scene::Layer::new(id, l.name.clone());
                nl.color = l.color;
                scene.layers.push(nl);
                id
            }
        };
        layer_ids.push(id);
    }
    // Parents before children (Scene::add_node drops an unknown parent).
    let depth = |mut i: usize| {
        let mut d = 0;
        while let Some(p) = doc.nodes[i].parent {
            i = p;
            d += 1;
        }
        d
    };
    let mut order: Vec<usize> = (0..doc.nodes.len()).collect();
    order.sort_by_key(|i| (depth(*i), *i));
    let nid = |i: usize| NodeId(i as u64 + 1);
    for i in order {
        let n = &doc.nodes[i];
        let name = if n.name.trim().is_empty() { "Object".to_owned() } else { n.name.clone() };
        let mut node = Node::new(&name, object(&n.kind).map_err(|e| format!("{name}: {e}"))?);
        node.id = nid(i);
        node.parent = n.parent.map(nid);
        node.transform = Transform::from_matrix(n.local);
        node.object_offset = Transform::from_matrix(n.offset);
        node.wire_color = n.wire_color;
        node.layer = n.layer.and_then(|l| layer_ids.get(l).copied()).unwrap_or(0);
        node.material = n.material.and_then(|m| ids.get(m).copied());
        if let Some(t) = n.look_at {
            node.constraints.rotation = Some(crate::camera::look_at_target(nid(t)));
            match &mut node.object {
                ObjectKind::Light(l) => l.target = Some(nid(t)),
                ObjectKind::Camera { target, .. } => *target = Some(nid(t)),
                _ => {}
            }
        }
        let mut unwraps = Vec::new();
        for m in &n.modifiers {
            if let Some(slot) = modifier(m, &name, &mut w) {
                if let Some(u) = &m.uvs {
                    unwraps.push((node.modifiers.len(), u));
                }
                node.modifiers.push(slot);
            }
        }
        let id = scene.add_node(node);
        for (k, uvs) in unwraps {
            let input = match k.checked_sub(1) {
                Some(prev) => scene.evaluated(id, Some(prev)).value.mesh.clone(),
                None => scene.node(id).map(|n| n.object.display_output()).unwrap_or_default(),
            };
            let Some(Modifier::UnwrapUVW(data)) = scene.node(id).map(|n| n.modifiers[k].modifier.clone()) else { continue };
            let mut data = data;
            match unwrap_edits(&input, uvs, &mut data) {
                Ok(()) => {
                    if let Some(n) = scene.node_mut(id) {
                        n.modifiers[k].modifier = Modifier::UnwrapUVW(data);
                    }
                    scene.touch(id);
                }
                Err(e) => w.push(format!("{name}: Unwrap UVW texture vertices not applied ({e})")),
            }
        }
    }
    let settings = crate::file::SceneSettings::default();
    Ok((crate::file::Document { format: crate::file::FORMAT, app: format!("3dloom {}", env!("CARGO_PKG_VERSION")), scene, settings }, w))
}

/// File > Import: every node of the file, one undo step; the result lists
/// the new ids in file order.
pub fn import(state: &mut AppState, path: &Path) -> Result<Value, String> {
    let (doc, warnings) = document(path)?;
    let label = format!("Import {}", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    let map = crate::file::merge_map(state, &doc, None, &[], &label)?;
    let mut ids: Vec<NodeId> = Vec::with_capacity(map.len());
    let mut src: Vec<NodeId> = map.keys().copied().collect();
    src.sort();
    for s in src {
        ids.push(map[&s]);
    }
    state.selection = ids.iter().copied().collect();
    state.sub_selection.clear();
    state.selection_mode = SelectionMode::Object;
    state.import_report = warnings.clone();
    Ok(json!({ "nodes": ids.iter().map(|i| i.0).collect::<Vec<_>>(), "warnings": warnings }))
}
