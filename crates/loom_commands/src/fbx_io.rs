//! FBX ↔ scene: `import` adds nodes (one undo entry), `export` writes the
//! given nodes with their hierarchy, in the scene's system unit.

use crate::execute::{commit, unique_name};
use crate::state::{AppState, SelectionMode};
use glam::Mat4;
use loom_fbx::{FbxBlendShape, FbxDocument, FbxKind, FbxNode, FbxSkin, WriteOpts};
use loom_scene::{Change, Modifier, ModifierSlot, Node, NodeId, ObjectKind, Transform, OBJECT_COLORS};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;

pub fn is_fbx(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("fbx"))
}

/// Metres per scene unit (Units Setup).
fn unit_metres(state: &AppState) -> f64 {
    state.units.system_unit.metres() * state.units.system_scale as f64
}

pub fn import(state: &mut AppState, path: &Path) -> Result<Vec<NodeId>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let doc = loom_fbx::read(&bytes, unit_metres(state)).map_err(|e| format!("{}: {e}", path.display()))?;
    // Empties without children are dropped; their children go to the grandparent.
    let has_children: Vec<bool> = (0..doc.nodes.len()).map(|i| doc.nodes.iter().any(|n| n.parent == Some(i))).collect();
    // Bones always stay (a skeleton's leaf joints have no children).
    let keep: Vec<bool> = doc.nodes.iter().enumerate().map(|(i, n)| !matches!(n.kind, FbxKind::Empty) || has_children[i]).collect();
    if !keep.iter().any(|k| *k) {
        return Err(format!("{}: nothing to import", path.display()));
    }
    let first = state.scene.next_id();
    let mut taken: std::collections::BTreeSet<String> = state.scene.nodes().map(|n| n.name.clone()).collect();
    // Ids are known up front (a skin may name bones that come later in the file).
    let id_of: BTreeMap<usize, NodeId> = (0..doc.nodes.len()).filter(|i| keep[*i]).enumerate().map(|(k, i)| (i, NodeId(first + k as u64))).collect();
    // A bone points at its child joint: FBX skeletons along +Y, Loom bones along +X.
    let mut bone_axis: Vec<u32> = vec![0; doc.nodes.len()];
    for (i, n) in doc.nodes.iter().enumerate() {
        if matches!(n.kind, FbxKind::Bone { .. }) {
            let child = doc.nodes.iter().find(|c| c.parent == Some(i) && c.translation.length() > 1e-4).map(|c| c.translation);
            bone_axis[i] = match child {
                Some(t) => loom_scene::node::bone_axis_of(t),
                None => n.parent.filter(|p| matches!(doc.nodes[*p].kind, FbxKind::Bone { .. })).map(|p| bone_axis[p]).unwrap_or(0),
            };
        }
    }
    let mut changes = Vec::new();
    let mut ids = Vec::new();
    for (i, n) in doc.nodes.iter().enumerate() {
        if !keep[i] {
            continue;
        }
        let base = if n.name.trim().is_empty() { "Object" } else { n.name.as_str() };
        let name = if taken.contains(base) { unique_name(base, &taken) } else { base.to_owned() };
        taken.insert(name.clone());
        let object = match &n.kind {
            FbxKind::Mesh { mesh, .. } => ObjectKind::EditablePoly(loom_scene::PolyObject::from_mesh(mesh.clone())),
            FbxKind::Camera { fov_deg } => ObjectKind::camera_object(*fov_deg),
            FbxKind::Light(l) => ObjectKind::Light(Box::new(light_from_fbx(l))),
            FbxKind::Bone { length, size } => {
                let mut b = ObjectKind::bone(length.max(0.001));
                let _ = b.set_parameter("Width", *size as f64);
                let _ = b.set_parameter("Height", *size as f64);
                let _ = b.set_parameter("Bone Axis", bone_axis[i] as f64);
                b
            }
            FbxKind::Empty => ObjectKind::Group { open: true },
        };
        let mut node = Node::new(&name, object);
        if matches!(n.kind, FbxKind::Bone { .. }) {
            node.wire_color = [180, 180, 100];
        }
        if matches!(n.kind, FbxKind::Light(_)) {
            node.wire_color = crate::light::LIGHT_COLOR;
        }
        if let FbxKind::Mesh { materials, .. } = &n.kind {
            node.materials = materials.clone();
        }
        // Constants: importing at a later frame with Auto Key on does not key.
        node.transform = Transform { position: loom_scene::Param::new(n.translation), rotation: loom_scene::Param::new(n.rotation_deg), scale: loom_scene::Param::new(n.scale) };
        // The file's take: a Linear key per frame on the components that move.
        if let Some((_, a)) = doc.anim.as_ref().and_then(|t| t.tracks.iter().find(|(ti, _)| *ti == i)) {
            node.transform = Transform { position: sampled(&a.frames, &a.translation), rotation: sampled(&a.frames, &a.rotation_deg), scale: sampled(&a.frames, &a.scale) };
        }
        // Parent: the nearest kept ancestor.
        let mut p = n.parent;
        while let Some(pi) = p {
            if keep[pi] {
                break;
            }
            p = doc.nodes[pi].parent;
        }
        node.parent = p.and_then(|pi| id_of.get(&pi).copied());
        node.wire_color = OBJECT_COLORS[((first as usize) + ids.len()) % OBJECT_COLORS.len()];
        node.layer = state.scene.current_layer;
        let id = NodeId(first + ids.len() as u64);
        node.id = id;
        // Morpher below Skin (FBX order); both built now that every id is known.
        if !n.blend_shapes.is_empty() {
            if let FbxKind::Mesh { mesh, .. } = &n.kind {
                let mut data = loom_scene::morph::MorpherData::default();
                for (k, bs) in n.blend_shapes.iter().enumerate() {
                    let mut offsets = vec![glam::Vec3::ZERO; mesh.vertex_count()];
                    for (v, o) in &bs.offsets {
                        if let Some(slot) = offsets.get_mut(*v as usize) {
                            *slot = *o;
                        }
                    }
                    let ch = data.channel(k);
                    ch.name = bs.name.clone();
                    ch.targets = vec![loom_scene::morph::MorphTarget { node: None, offsets, percent: 100.0 }];
                    ch.value.set(bs.weight);
                    ch.valid = true;
                }
                node.modifiers.push(ModifierSlot::new(Modifier::Morpher(Box::new(data))));
            }
        }
        if let Some(skin) = &n.skin {
            let mut bones: Vec<(NodeId, glam::Mat4, loom_scene::skin::Envelope)> = Vec::new();
            let mut remap: Vec<Option<u32>> = Vec::new();
            for (bi, g2b) in &skin.bones {
                match id_of.get(bi) {
                    Some(bid) => {
                        let (len, axis) = match doc.nodes[*bi].kind { FbxKind::Bone { length, .. } => (length.max(0.001), loom_scene::node::bone_axis_dir(bone_axis[*bi])), _ => (1.0, glam::Vec3::X) };
                        remap.push(Some(bones.len() as u32));
                        bones.push((*bid, *g2b, loom_scene::skin::Envelope { start: glam::Vec3::ZERO, end: axis * len, inner: [len * 0.25; 2], outer: [len * 0.5; 2], falloff: 0, abs_effect: 1.0, squash: 1.0 }));
                    }
                    None => remap.push(None),
                }
            }
            if !bones.is_empty() {
                let weights: Vec<Vec<(u32, f32)>> = skin.weights.iter().map(|w| w.iter().filter_map(|(b, x)| remap.get(*b as usize).copied().flatten().map(|nb| (nb, *x))).collect()).collect();
                node.modifiers.push(ModifierSlot::new(Modifier::Skin(Box::new(loom_scene::skin::SkinData::from_file(&bones, skin.mesh_bind, weights)))));
            }
        }
        ids.push(id);
        changes.push(Change::AddNode(node));
    }
    let label = format!("Import {}", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    commit(state, &label, Change::Group(changes))?;
    // A file with a take sets the active time segment and frame rate to it.
    if let Some(t) = doc.anim.as_ref().filter(|t| !t.tracks.is_empty()) {
        state.frame_range = (t.start, t.end.max(t.start + 1));
        if t.fps > 0.0 {
            state.fps = t.fps;
        }
        let f = state.current_frame.clamp(state.frame_range.0, state.frame_range.1);
        crate::execute::set_frame(state, f)?;
    }
    state.selection = ids.iter().copied().collect();
    state.sub_selection.clear();
    state.selection_mode = SelectionMode::Object;
    Ok(ids)
}

/// The bones a node's Skin references, and their ancestors (the skeleton travels with the mesh).
fn skin_bones_with_ancestors(state: &AppState, id: NodeId) -> Vec<NodeId> {
    let Some(node) = state.scene.node(id) else { return Vec::new() };
    let mut out = Vec::new();
    for slot in &node.modifiers {
        if let Modifier::Skin(d) = &slot.modifier {
            for b in &d.bones {
                out.push(b.node);
                out.extend(state.scene.ancestors(b.node));
            }
        }
    }
    out
}

pub fn export(state: &AppState, ids: &[NodeId], path: &Path) -> Result<Value, String> {
    let mut set: std::collections::BTreeSet<NodeId> = ids.iter().copied().collect();
    for id in ids {
        set.extend(skin_bones_with_ancestors(state, *id));
    }
    let ids: Vec<NodeId> = set.iter().copied().collect();
    let ids = &ids[..];
    // Parents before children.
    let mut order: Vec<NodeId> = Vec::new();
    let mut pending: Vec<NodeId> = ids.to_vec();
    while !pending.is_empty() {
        let before = pending.len();
        pending.retain(|id| {
            let parent_in = state.scene.node(*id).and_then(|n| n.parent).filter(|p| set.contains(p));
            if parent_in.is_none_or(|p| order.contains(&p)) {
                order.push(*id);
                false
            } else {
                true
            }
        });
        if pending.len() == before {
            order.extend(pending.drain(..));
        }
    }
    // Indices first (a skin names bones that may come later in the order), nodes second.
    let exported: Vec<NodeId> = order
        .iter()
        .copied()
        .filter(|id| {
            state.scene.node(*id).is_some_and(|n| match &n.object {
                ObjectKind::Camera { .. } | ObjectKind::Light(_) | ObjectKind::Group { .. } | ObjectKind::Bone { .. } | ObjectKind::Dummy { .. } | ObjectKind::Point { .. } | ObjectKind::Tape { .. } | ObjectKind::Protractor { .. } | ObjectKind::Compass { .. } | ObjectKind::Grid { .. } | ObjectKind::IkGoal { .. } | ObjectKind::Biped(_) => true,
                _ => state.scene.eval_mesh(*id).value.mesh.face_count() > 0 || n.object.display_output().face_count() > 0,
            })
        })
        .collect();
    let index_of: BTreeMap<NodeId, usize> = exported.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let mut nodes: Vec<FbxNode> = Vec::new();
    let exported_ids = exported.clone();
    for id in exported {
        let Some(node) = state.scene.node(id) else { continue };
        let mut skin: Option<FbxSkin> = None;
        let mut blend_shapes: Vec<FbxBlendShape> = Vec::new();
        let kind = match &node.object {
            ObjectKind::Camera { fov, .. } => FbxKind::Camera { fov_deg: fov.get() },
            ObjectKind::Light(l) => match light_to_fbx(l) {
                Some(light) => {
                    // A target light's aim comes from its LookAt constraint: write the evaluated rotation.
                    let parent = node.parent.map(|p| state.scene.world_matrix(p)).unwrap_or(Mat4::IDENTITY);
                    let local = Transform::from_matrix(parent.inverse() * state.scene.world_matrix(id));
                    nodes.push(FbxNode { name: node.name.clone(), parent: node.parent.filter(|p| set.contains(p)).and_then(|p| index_of.get(&p).copied()), translation: node.transform.position.get(), rotation_deg: local.rotation.get(), scale: node.transform.scale.get(), kind: FbxKind::Light(light), skin: None, blend_shapes: Vec::new() });
                    continue;
                }
                None => FbxKind::Empty,
            },
            ObjectKind::Group { .. } => FbxKind::Empty,
            ObjectKind::Bone { length, width, height, .. } => FbxKind::Bone { length: length.get(), size: width.get().max(height.get()) },
            ObjectKind::Dummy { .. } | ObjectKind::Point { .. } | ObjectKind::Tape { .. } | ObjectKind::Protractor { .. } | ObjectKind::Compass { .. } | ObjectKind::Grid { .. } | ObjectKind::IkGoal { .. } => FbxKind::Empty,
            // A biped's centre of mass is the skeleton's root joint.
            ObjectKind::Biped(b) => FbxKind::Bone { length: 0.0, size: loom_scene::biped::root_size(b.height.get()) },
            _ => {
                // A skinned node writes the mesh below its Skin (and Morpher) with the deformers as FBX deformers.
                let skin_index = node.modifiers.iter().position(|s| matches!(s.modifier, Modifier::Skin(_)));
                let morph_index = node.modifiers.iter().position(|s| matches!(s.modifier, Modifier::Morpher(_)));
                let below = [skin_index, morph_index].into_iter().flatten().min();
                let mut mesh = match below {
                    Some(0) => node.object.display_output(),
                    Some(i) => state.scene.evaluated(id, Some(i - 1)).value.mesh.clone(),
                    None => state.scene.eval_mesh(id).value.mesh.clone(),
                };
                if mesh.face_count() == 0 {
                    nodes.push(FbxNode { name: node.name.clone(), parent: node.parent.filter(|p| set.contains(p)).and_then(|p| index_of.get(&p).copied()), translation: node.transform.position.get(), rotation_deg: node.transform.rotation.get(), scale: node.transform.scale.get(), kind: FbxKind::Empty, skin: None, blend_shapes: Vec::new() });
                    continue;
                }
                if let Some(si) = skin_index {
                    if let Modifier::Skin(d) = &node.modifiers[si].modifier {
                        let bone_indices: Vec<Option<usize>> = d.bones.iter().map(|b| index_of.get(&b.node).copied()).collect();
                        // The written geometry has the object offset baked in, so the binding
                        // maps it back out (the node world at bind is mesh_bind · offset⁻¹).
                        let node_bind = d.mesh_bind * node.object_offset.matrix().inverse();
                        let bones: Vec<(usize, Mat4)> = d.bones.iter().zip(&bone_indices).filter_map(|(b, i)| i.map(|i| (i, b.bind_inverse * node_bind))).collect();
                        let remap: Vec<Option<u32>> = bone_indices.iter().scan(0u32, |k, i| Some(i.map(|_| { let v = *k; *k += 1; v }))).collect();
                        let weights: Vec<Vec<(u32, f32)>> = (0..mesh.vertex_count())
                            .map(|v| {
                                let p = d.mesh_bind.transform_point3(mesh.vertices()[v].position);
                                d.vertex_weights(v, p).into_iter().filter_map(|(b, x)| remap.get(b as usize).copied().flatten().map(|nb| (nb, x))).collect()
                            })
                            .collect();
                        skin = Some(FbxSkin { bones, mesh_bind: node_bind, weights });
                    }
                }
                if let Some(mi) = morph_index {
                    if let Modifier::Morpher(d) = &node.modifiers[mi].modifier {
                        for ch in d.channels.iter().filter(|c| c.valid && !c.targets.is_empty()) {
                            let full = ch.targets.iter().max_by(|a, b| a.percent.total_cmp(&b.percent)).unwrap();
                            if full.offsets.len() != mesh.vertex_count() {
                                continue;
                            }
                            let offsets: Vec<(u32, glam::Vec3)> = full.offsets.iter().enumerate().filter(|(_, o)| o.length_squared() > 1e-12).map(|(v, o)| (v as u32, *o)).collect();
                            blend_shapes.push(FbxBlendShape { name: ch.name.clone(), offsets, weight: ch.value.get() });
                        }
                    }
                }
                // The object offset is baked into the geometry so the node is the pivot.
                let off = node.object_offset.matrix();
                if off != Mat4::IDENTITY {
                    for v in mesh.vertices_mut() {
                        v.position = off.transform_point3(v.position);
                    }
                }
                FbxKind::Mesh { materials: loom_fbx::material_names(&mesh, &node.materials), mesh }
            }
        };
        let parent = node.parent.filter(|p| set.contains(p)).and_then(|p| index_of.get(&p).copied());
        let (translation, rotation_deg, scale) = if parent.is_some() {
            (node.transform.position.get(), node.transform.rotation.get(), node.transform.scale.get())
        } else {
            let t = Transform::from_matrix(state.scene.world_matrix(id));
            (t.position.get(), t.rotation.get(), t.scale.get())
        };
        nodes.push(FbxNode { name: node.name.clone(), parent, translation, rotation_deg, scale, kind, skin, blend_shapes });
    }
    if nodes.is_empty() {
        return Err("nothing to export: no geometry objects".into());
    }
    let count = nodes.len();
    let anim = bake_take(state, &exported_ids, &nodes);
    let doc = FbxDocument { nodes, unit_metres: unit_metres(state), anim };
    let bytes = loom_fbx::write(&doc, &WriteOpts { creator: format!("3dloom {}", env!("CARGO_PKG_VERSION")), ..Default::default() });
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(json!({ "path": path, "objects": count }))
}


/// Euler XYZ degrees of a rotation, each axis unwrapped toward `prev`.
fn euler_near(q: glam::Quat, prev: Option<glam::Vec3>) -> glam::Vec3 {
    let (x, y, z) = q.to_euler(glam::EulerRot::XYZ);
    let mut r = glam::Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees());
    if let Some(p) = prev {
        for i in 0..3 {
            r[i] += ((p[i] - r[i]) / 360.0).round() * 360.0;
        }
    }
    r
}

/// The active segment baked per frame: each exported node whose local
/// transform (relative to its exported parent) changes gets a track.
fn bake_take(state: &AppState, ids: &[NodeId], nodes: &[FbxNode]) -> Option<loom_fbx::FbxTake> {
    let (a, b) = state.frame_range;
    let mut tracks = Vec::new();
    for (i, id) in ids.iter().enumerate() {
        let parent = nodes.get(i).and_then(|n| n.parent).and_then(|p| ids.get(p)).copied();
        let mut anim = loom_fbx::FbxAnim::default();
        let mut prev = None;
        for f in a..=b {
            let local = state.scene.at_time(f as f32, || {
                let w = state.scene.world_matrix(*id);
                match parent {
                    Some(p) => state.scene.world_matrix(p).inverse() * w,
                    None => w,
                }
            });
            let (s, q, t) = local.to_scale_rotation_translation();
            let r = euler_near(q, prev);
            prev = Some(r);
            anim.frames.push(f as f32);
            anim.translation.push(t);
            anim.rotation_deg.push(r);
            anim.scale.push(s);
        }
        let moves = |v: &[glam::Vec3]| v.iter().any(|x| x.distance(v[0]) > 1e-5);
        if moves(&anim.translation) || moves(&anim.rotation_deg) || moves(&anim.scale) {
            tracks.push((i, anim));
        }
    }
    (!tracks.is_empty()).then(|| loom_fbx::FbxTake { name: "Take 001".into(), fps: state.fps, start: a, end: b, tracks })
}

/// Per-frame samples as a keyed Param: one Linear key per frame on each
/// component that changes; a component that holds stays a plain value.
fn sampled(frames: &[f32], values: &[glam::Vec3]) -> loom_scene::Param<glam::Vec3> {
    use loom_scene::anim::{Interp, TangentType};
    let first = values.first().copied().unwrap_or_default();
    let mut k = loom_scene::param::KeyedTracks::from_rest(loom_scene::param::ParamType::Vec3, first.to_array().to_vec());
    for (c, tr) in k.tracks.iter_mut().enumerate() {
        if values.iter().all(|v| (v[c] - first[c]).abs() < 1e-6) {
            continue;
        }
        tr.interp = Interp::Linear;
        for (f, v) in frames.iter().zip(values) {
            tr.set_key(*f, v[c], (TangentType::Linear, TangentType::Linear));
        }
    }
    loom_scene::Param { controller: loom_scene::param::Controller::Keys(k) }
}

/// An FBX light as a standard light (free: FBX carries no target).
fn light_from_fbx(l: &loom_fbx::FbxLight) -> loom_scene::light::LightObject {
    use loom_scene::light::{LightObject, LightType};
    let mut o = LightObject::new(match l.light_type {
        1 => LightType::Directional,
        2 => LightType::Spot,
        _ => LightType::Omni,
    });
    o.color.set(glam::Vec3::from(l.color).clamp(glam::Vec3::ZERO, glam::Vec3::ONE));
    o.multiplier.set(l.intensity);
    o.on.set(l.cast_light);
    o.shadows_on.set(l.cast_shadows);
    if o.light_type == LightType::Spot && l.outer_angle > 0.0 {
        let _ = o.set_parameter("Falloff", l.outer_angle as f64);
        let _ = o.set_parameter("Hotspot", l.inner_angle.min(l.outer_angle) as f64);
    }
    o.decay.set(l.decay.min(2));
    o.decay_start.set(l.decay_start.max(0.0));
    if let Some((a, b)) = l.near {
        o.near_use.set(true);
        o.near_start.set(a);
        o.near_end.set(b);
    }
    if let Some((a, b)) = l.far {
        o.far_use.set(true);
        o.far_start.set(a);
        o.far_end.set(b);
    }
    o
}

/// A standard light as an FBX light (a Skylight has no FBX equivalent).
fn light_to_fbx(l: &loom_scene::light::LightObject) -> Option<loom_fbx::FbxLight> {
    use loom_scene::light::LightType;
    let light_type = match l.light_type {
        LightType::Omni => 0,
        LightType::Directional => 1,
        LightType::Spot => 2,
        LightType::Skylight => return None,
    };
    Some(loom_fbx::FbxLight {
        light_type,
        color: l.color.get().to_array(),
        intensity: l.multiplier.get(),
        inner_angle: l.hotspot.get(),
        outer_angle: l.falloff.get(),
        cast_light: l.on.get(),
        cast_shadows: l.shadows_on.get(),
        decay: l.decay.get(),
        decay_start: l.decay_start.get(),
        near: l.near_use.get().then(|| (l.near_start.get(), l.near_end.get())),
        far: l.far_use.get().then(|| (l.far_start.get(), l.far_end.get())),
    })
}
