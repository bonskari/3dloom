//! Morpher and Skin Morph commands.

use crate::execute::commit;
use crate::state::AppState;
use crate::Command;
use glam::Vec3;
use loom_mesh::Mesh;
use loom_scene::morph::{MorphTarget, MorpherData, SkinMorphData, SkinWrapData, SkinWrapPatchData};
use loom_scene::ModifierSlot;
use loom_scene::{Change, Modifier, Node, NodeId, ObjectKind};
use serde_json::{json, Value};
use std::collections::BTreeSet;

fn find_entry(state: &AppState, is: fn(&Modifier) -> bool) -> Option<(NodeId, usize)> {
    state.selection.iter().find_map(|id| {
        let n = state.scene.node(*id)?;
        if let Some(a) = n.active_entry {
            if n.modifiers.get(a).is_some_and(|s| is(&s.modifier)) {
                return Some((*id, a));
            }
        }
        n.modifiers.iter().rposition(|s| is(&s.modifier)).map(|i| (*id, i))
    })
}

pub fn morpher_entry(state: &AppState) -> Result<(NodeId, usize, MorpherData), String> {
    let (id, index) = find_entry(state, |m| matches!(m, Modifier::Morpher(_))).ok_or("no Morpher on the selection (add the modifier first)")?;
    let Modifier::Morpher(d) = &state.scene.node(id).ok_or("missing node")?.modifiers[index].modifier else { unreachable!() };
    Ok((id, index, (**d).clone()))
}

pub fn skin_wrap_entry(state: &AppState) -> Result<(NodeId, usize, SkinWrapData), String> {
    let (id, index) = find_entry(state, |m| matches!(m, Modifier::SkinWrap(_))).ok_or("no Skin Wrap on the selection (add the modifier first)")?;
    let Modifier::SkinWrap(d) = &state.scene.node(id).ok_or("missing node")?.modifiers[index].modifier else { unreachable!() };
    Ok((id, index, (**d).clone()))
}

fn edit_skin_wrap(state: &mut AppState, label: &str, f: impl FnOnce(&mut SkinWrapData, NodeId, usize, &AppState) -> Result<(), String>) -> Result<Value, String> {
    let (id, index, mut data) = skin_wrap_entry(state)?;
    f(&mut data, id, index, state)?;
    put(state, id, index, Modifier::SkinWrap(Box::new(data)), label)?;
    Ok(skin_wrap_status(state))
}

pub fn skin_morph_entry(state: &AppState) -> Result<(NodeId, usize, SkinMorphData), String> {
    let (id, index) = find_entry(state, |m| matches!(m, Modifier::SkinMorph(_))).ok_or("no Skin Morph on the selection (add the modifier first)")?;
    let Modifier::SkinMorph(d) = &state.scene.node(id).ok_or("missing node")?.modifiers[index].modifier else { unreachable!() };
    Ok((id, index, (**d).clone()))
}

fn put(state: &mut AppState, id: NodeId, index: usize, modifier: Modifier, label: &str) -> Result<(), String> {
    let node = state.scene.node(id).ok_or("missing node")?;
    let mut stack = node.modifiers.clone();
    stack[index].modifier = modifier;
    commit(state, label, Change::SetStack { id, before: node.modifiers.clone(), after: stack })
}

fn edit_morpher(state: &mut AppState, label: &str, f: impl FnOnce(&mut MorpherData, NodeId, usize, &AppState) -> Result<(), String>) -> Result<Value, String> {
    let (id, index, mut data) = morpher_entry(state)?;
    f(&mut data, id, index, state)?;
    put(state, id, index, Modifier::Morpher(Box::new(data)), label)?;
    Ok(morpher_status(state))
}

fn edit_skin_morph(state: &mut AppState, label: &str, f: impl FnOnce(&mut SkinMorphData, NodeId, usize, &AppState) -> Result<(), String>) -> Result<Value, String> {
    let (id, index, mut data) = skin_morph_entry(state)?;
    f(&mut data, id, index, state)?;
    put(state, id, index, Modifier::SkinMorph(Box::new(data)), label)?;
    Ok(skin_morph_status(state))
}

/// The mesh flowing into a stack entry.
fn input_mesh(state: &AppState, id: NodeId, index: usize) -> Mesh {
    match index.checked_sub(1) {
        Some(prev) => state.scene.evaluated(id, Some(prev)).value.mesh.clone(),
        None => state.scene.node(id).map(|n| n.object.display_output()).unwrap_or_default(),
    }
}

pub fn execute(state: &mut AppState, cmd: Command) -> Result<Value, String> {
    use Command as C;
    match cmd {
        C::MorpherModifier => crate::execute(state, C::AddModifier { name: "Morpher".into() }),
        C::MorpherPickTarget { channel, node } => {
            let target = NodeId(node);
            state.scene.node(target).ok_or_else(|| format!("no node {node}"))?;
            edit_morpher(state, "Pick Target", |d, id, index, st| {
                if target == id {
                    return Err("pick another object".into());
                }
                d.pick_target(&st.scene, id, Some(index), channel as usize, target);
                Ok(())
            })
        }
        C::MorpherCaptureCurrentState { channel } => edit_morpher(state, "Capture Current State", |d, id, index, st| {
            let base = input_mesh(st, id, index);
            let current = st.scene.eval_mesh(id).value.mesh.clone();
            let offsets = MorpherData::offsets_between(&base, &current).ok_or("the stack output has a different vertex count")?;
            let ch = d.channel(channel as usize);
            ch.name = format!("Capture {}", channel + 1);
            ch.valid = true;
            ch.targets = vec![MorphTarget { node: None, offsets, percent: 100.0 }];
            Ok(())
        }),
        C::MorpherSetChannel { channel, name, value } => edit_morpher(state, "Morpher Channel", |d, _, _, _| {
            let ch = d.channel(channel as usize);
            match name.as_str() {
                "Value" => ch.value.set(value as f32),
                "Active" => ch.active = value != 0.0,
                "Use Limits" => ch.use_limits = value != 0.0,
                "Min" => ch.min = value as f32,
                "Max" => ch.max = value as f32,
                "Tension" => ch.tension = (value as f32).clamp(0.0, 1.0),
                other => return Err(format!("Morpher channel has no parameter {other:?}")),
            }
            Ok(())
        }),
        C::MorpherRenameChannel { channel, name } => edit_morpher(state, "Rename Channel", |d, _, _, _| {
            d.channel(channel as usize).name = name;
            Ok(())
        }),
        C::MorpherDeleteChannel { channel } => edit_morpher(state, "Delete Channel", |d, _, _, _| {
            *d.channel(channel as usize) = Default::default();
            Ok(())
        }),
        C::MorpherAddProgressiveTarget { channel, node } => {
            let target = NodeId(node);
            state.scene.node(target).ok_or_else(|| format!("no node {node}"))?;
            edit_morpher(state, "Add Progressive Target", |d, id, index, st| {
                let base = input_mesh(st, id, index);
                let offsets = MorpherData::offsets_between(&base, &st.scene.eval_mesh(target).value.mesh).ok_or("the target has a different vertex count")?;
                let ch = d.channel(channel as usize);
                if ch.targets.is_empty() {
                    ch.name = st.scene.node(target).map(|n| n.name.clone()).unwrap_or_default();
                }
                ch.targets.push(MorphTarget { node: Some(target), offsets, percent: 100.0 });
                // Evenly spaced until the user sets the percents.
                let n = ch.targets.len() as f32;
                for (i, t) in ch.targets.iter_mut().enumerate() {
                    t.percent = 100.0 * (i as f32 + 1.0) / n;
                }
                ch.valid = true;
                Ok(())
            })
        }
        C::MorpherSetProgressivePercent { channel, target, percent } => edit_morpher(state, "Progressive Percent", |d, _, _, _| {
            let ch = d.channel(channel as usize);
            let t = ch.targets.get_mut(target as usize).ok_or("no such target")?;
            t.percent = percent.clamp(0.0, 100.0);
            Ok(())
        }),
        C::MorpherReload => edit_morpher(state, "Reload", |d, id, index, st| {
            d.reload(&st.scene, id, Some(index));
            Ok(())
        }),
        C::MorpherExtractTarget { channel } => {
            let (id, index, data) = morpher_entry(state)?;
            let ch = data.channel_ref(channel as usize).filter(|c| !c.targets.is_empty()).ok_or("the channel has no target")?;
            let full = ch.targets.iter().max_by(|a, b| a.percent.total_cmp(&b.percent)).unwrap();
            let mut mesh = input_mesh(state, id, index);
            if full.offsets.len() != mesh.vertex_count() {
                return Err("the target has a different vertex count".into());
            }
            for (v, o) in mesh.vertices_mut().iter_mut().zip(&full.offsets) {
                v.position += *o;
            }
            let base = state.scene.node(id).ok_or("missing node")?;
            let taken: BTreeSet<String> = state.scene.nodes().map(|n| n.name.clone()).collect();
            let want = format!("{}_morph", ch.name);
            let name = if taken.contains(&want) { crate::execute::unique_name(&want, &taken) } else { want };
            let mut node = Node::new(&name, ObjectKind::EditablePoly(loom_scene::PolyObject::from_mesh(mesh)));
            node.transform = base.transform.clone();
            node.transform.position.set(base.transform.position.get() + Vec3::X * 20.0);
            node.wire_color = base.wire_color;
            let new_id = NodeId(state.scene.next_id());
            commit(state, "Extract Target", Change::AddNode(node))?;
            state.selection = BTreeSet::from([new_id]);
            Ok(json!({ "node": new_id.0 }))
        }
        C::MorpherSetGlobal { name, value } => edit_morpher(state, "Morpher Global", |d, _, _, _| {
            match name.as_str() {
                "Use Vertex Selection" => d.use_vertex_selection = value != 0.0,
                "Spinner Min" => d.spinner_min = value as f32,
                "Spinner Max" => d.spinner_max = value as f32,
                other => return Err(format!("Morpher has no global {other:?}")),
            }
            Ok(())
        }),
        C::SkinMorphAddBones { ids } => edit_skin_morph(state, "Add Bones", |d, _, _, st| {
            for id in ids {
                st.scene.node(NodeId(id)).ok_or_else(|| format!("no node {id}"))?;
                d.add_bone(&st.scene, NodeId(id));
            }
            Ok(())
        }),
        C::SkinMorphRemoveBone { id } => edit_skin_morph(state, "Remove Bone", |d, _, _, _| {
            let i = d.bones.iter().position(|b| b.node == NodeId(id)).ok_or("the bone is not in the list")?;
            d.bones.remove(i);
            d.morphs.retain(|m| m.bone != i);
            d.editing = None;
            for m in &mut d.morphs {
                if m.bone > i {
                    m.bone -= 1;
                }
            }
            Ok(())
        }),
        C::SkinMorphCreateMorph { bone } => {
            let r = edit_skin_morph(state, "Create Morph", |d, _, _, st| {
                let i = d.bones.iter().position(|b| b.node == NodeId(bone)).ok_or("the bone is not in the Skin Morph's list (Add Bones first)")?;
                d.create_morph(&st.scene, i);
                Ok(())
            })?;
            state.skin_morph_bone = Some(NodeId(bone));
            let idx = skin_morph_entry(state)?.2.morphs.len() - 1;
            state.skin_morph_current = Some(idx);
            Ok(json!({ "morph": idx, "status": r }))
        }
        C::SkinMorphSetVertexDelta { morph, vertex, delta } => edit_skin_morph(state, "Morph Delta", |d, _, _, _| {
            let m = d.morphs.get_mut(morph as usize).ok_or("no such morph")?;
            let v = Vec3::from(delta);
            match m.deltas.iter_mut().find(|(x, _)| *x == vertex) {
                Some(e) => e.1 = v,
                None => m.deltas.push((vertex, v)),
            }
            Ok(())
        }),
        C::SkinMorphSetMorph { morph, name, value } => edit_skin_morph(state, "Morph Property", |d, _, _, _| {
            let m = d.morphs.get_mut(morph as usize).ok_or("no such morph")?;
            match name.as_str() {
                "Enabled" => m.enabled = value != 0.0,
                "Angle" => m.angle_deg = value as f32,
                "Influence Angle" => m.influence_deg = (value as f32).max(0.001),
                "Falloff" => m.falloff = (value.max(0.0) as u32).min(loom_scene::morph::FALLOFF_CUSTOM),
                "Reload Only Selected Verts" => m.reload_selected = value != 0.0,
                other => return Err(format!("morph has no property {other:?}")),
            }
            Ok(())
        }),
        C::SkinMorphRenameMorph { morph, name } => edit_skin_morph(state, "Rename Morph", |d, _, _, _| {
            d.morphs.get_mut(morph as usize).ok_or("no such morph")?.name = name;
            Ok(())
        }),
        C::SkinMorphDeleteMorph { morph } => {
            let r = edit_skin_morph(state, "Delete Morph", |d, _, _, _| {
                if morph as usize >= d.morphs.len() {
                    return Err("no such morph".into());
                }
                d.morphs.remove(morph as usize);
                d.editing = None;
                Ok(())
            })?;
            state.skin_morph_current = None;
            Ok(r)
        }
        C::SkinMorphReload { morph } => edit_skin_morph(state, "Reset Orientation", |d, _, _, st| {
            let bone = d.morphs.get(morph as usize).ok_or("no such morph")?.bone;
            let world = |id: NodeId| Some(st.scene.world_matrix(id));
            d.morphs[morph as usize].angle_deg = d.bone_angle(bone, &world).unwrap_or(0.0);
            d.morphs[morph as usize].pose = d.local_rotation(bone, &world);
            Ok(())
        }),
        C::SkinWrapAddBase { id } => {
            let base = NodeId(id);
            state.scene.node(base).ok_or_else(|| format!("no node {id}"))?;
            edit_skin_wrap(state, "Add Base", |d, wrapped, index, st| {
                if base == wrapped {
                    return Err("pick another object".into());
                }
                if !d.bases.contains(&base) {
                    d.bases.push(base);
                }
                d.rebind(&st.scene, wrapped, index);
                Ok(())
            })
        }
        C::SkinWrapRemoveBase { id } => edit_skin_wrap(state, "Remove Base", |d, wrapped, index, st| {
            d.bases.retain(|b| *b != NodeId(id));
            d.rebind(&st.scene, wrapped, index);
            Ok(())
        }),
        C::SkinWrapSetOption { name, value } => edit_skin_wrap(state, "Skin Wrap Option", |d, wrapped, index, st| {
            let on = value != 0.0;
            let mut rebind = true;
            match name.as_str() {
                "Deformation Engine" => d.engine = (value.max(0.0) as u32).min(1),
                "Falloff" => d.falloff = (value as f32).clamp(0.001, 10.0),
                "Distance Influence" => d.distance_influence = (value as f32).clamp(0.001, 10.0),
                "Face Limit" => d.face_limit = value.clamp(0.0, 30.0).round() as u32,
                "Threshold" => d.threshold = (value as f32).max(0.0),
                "Weight All Points" => d.weight_all_points = on,
                "Blend To Base Mesh" => d.blend_to_base = on,
                "Blend Distance" => d.blend_distance = (value as f32).max(0.001),
                other => {
                    rebind = false;
                    match other {
                        "Show Mirror Data" => d.mirror.show = on,
                        "Mirror Plane" => d.mirror.axis = (value.max(0.0) as u32).min(2),
                        "Mirror Offset" => d.mirror.offset = value as f32,
                        "Mirror Threshold" => d.mirror.threshold = (value as f32).max(0.0),
                        "Display Loops" => d.display.loops = on,
                        "Display Axis" => d.display.axis = on,
                        "Display Face Limit" => d.display.face_limit = on,
                        "Display Unassigned Points" => d.display.unassigned = on,
                        "Display Control Verts" => d.display.control_verts = on,
                        other => return Err(format!("Skin Wrap has no option {other:?}")),
                    }
                }
            }
            if rebind {
                d.rebind(&st.scene, wrapped, index);
            }
            Ok(())
        }),
        C::SkinWrapReset => edit_skin_wrap(state, "Skin Wrap Reset", |d, wrapped, index, st| {
            for l in d.locals.iter_mut().flatten() {
                *l = Default::default();
            }
            d.rebind(&st.scene, wrapped, index);
            Ok(())
        }),
        C::SkinWrapConvertToSkin => {
            let (wrapped, index, data) = skin_wrap_entry(state)?;
            // The first base with a Skin lends its bones; weights blend from the bound faces' vertices.
            let (base, base_skin) = data
                .bases
                .iter()
                .find_map(|b| state.scene.node(*b)?.modifiers.iter().find_map(|s| match &s.modifier { Modifier::Skin(d) => Some((*b, (**d).clone())), _ => None }))
                .ok_or("no base object has a Skin modifier")?;
            let bones: Vec<NodeId> = base_skin.bones.iter().map(|b| b.node).collect();
            let below = input_mesh(state, wrapped, index);
            let mut skin = loom_scene::skin::SkinData::bind_mesh(&state.scene, wrapped, &bones, &below);
            let base_mesh = state.scene.eval_mesh(base).value.mesh.clone();
            let base_input = match state.scene.node(base).and_then(|n| n.modifiers.iter().position(|s| matches!(s.modifier, Modifier::Skin(_)))) {
                Some(i) => input_mesh(state, base, i),
                None => base_mesh.clone(),
            };
            let base_weights = |v: usize| -> Vec<(u32, f32)> {
                let p = base_skin.mesh_bind.transform_point3(base_input.vertices().get(v).map(|x| x.position).unwrap_or_default());
                base_skin.vertex_weights(v, p)
            };
            let n = skin.weights.len();
            for (i, binds) in data.binds.iter().enumerate().take(n) {
                let mut acc: std::collections::BTreeMap<u32, f32> = std::collections::BTreeMap::new();
                for b in binds.iter().filter(|b| data.bases.get(b.base as usize) == Some(&base)) {
                    let verts: Vec<usize> = if data.engine == 0 { base_mesh.face_vertex_ids(loom_mesh::FaceId(b.target)).unwrap_or_default().into_iter().map(|v| v.0 as usize).collect() } else { vec![b.target as usize] };
                    if verts.is_empty() {
                        continue;
                    }
                    for v in &verts {
                        for (bone, w) in base_weights(*v) {
                            *acc.entry(bone).or_insert(0.0) += w * b.weight / verts.len() as f32;
                        }
                    }
                }
                if !acc.is_empty() {
                    let mut w: Vec<(u32, f32)> = acc.into_iter().collect();
                    loom_scene::skin::normalize(&mut w);
                    skin.weights[i] = w;
                    skin.explicit[i] = true;
                }
            }
            let node = state.scene.node(wrapped).ok_or("missing node")?;
            let mut stack = node.modifiers.clone();
            stack[index].enabled = false;
            stack.insert(index + 1, ModifierSlot::new(Modifier::Skin(Box::new(skin))));
            commit(state, "Convert To Skin", Change::SetStack { id: wrapped, before: node.modifiers.clone(), after: stack })?;
            if let Some(n) = state.scene.transform_mut(wrapped) {
                n.active_entry = Some(index + 1);
            }
            Ok(skin_wrap_status(state))
        }
        other => tools(state, other),
    }
}

pub fn skin_wrap_status(state: &AppState) -> Value {
    match skin_wrap_entry(state) {
        Ok((id, index, d)) => json!({
            "node": id.0,
            "index": index,
            "bases": d.bases.iter().map(|b| json!({ "node": b.0, "name": state.scene.node(*b).map(|n| n.name.clone()).unwrap_or_default() })).collect::<Vec<_>>(),
            "engine": d.engine,
            "falloff": d.falloff,
            "distance_influence": d.distance_influence,
            "face_limit": d.face_limit,
            "threshold": d.threshold,
            "weight_all_points": d.weight_all_points,
            "blend_to_base": d.blend_to_base,
            "blend_distance": d.blend_distance,
            "selection": d.selection.iter().map(|(b, v)| [*b, *v]).collect::<Vec<_>>(),
            "selected_locals": d.selection.iter().map(|(b, v)| { let l = d.local(*b, *v); json!({ "strength": l.strength, "scale": l.scale, "x": l.x, "y": l.y, "z": l.z }) }).collect::<Vec<_>>(),
            "mirror": { "show": d.mirror.show, "plane": d.mirror.axis, "offset": d.mirror.offset, "threshold": d.mirror.threshold },
            "display": { "loops": d.display.loops, "axis": d.display.axis, "face_limit": d.display.face_limit, "unassigned": d.display.unassigned, "control_verts": d.display.control_verts },
            "unassigned": d.closest.iter().filter(|c| c.is_none()).count(),
            "unweighted": d.closest.iter().zip(&d.binds).filter(|(c, b)| c.is_some() && b.is_empty()).count(),
            "bound": d.binds.iter().filter(|b| !b.is_empty()).count(),
            "vertices": d.binds.len(),
        }),
        Err(_) => Value::Null,
    }
}

pub fn morpher_status(state: &AppState) -> Value {
    match morpher_entry(state) {
        Ok((id, index, d)) => json!({
            "node": id.0,
            "index": index,
            "channels": d.channels.iter().map(|c| json!({ "name": c.name, "value": c.value.get(), "active": c.active, "valid": c.valid, "targets": c.targets.iter().map(|t| json!({ "node": t.node.map(|n| n.0), "percent": t.percent })).collect::<Vec<_>>(), "use_limits": c.use_limits, "min": c.min, "max": c.max })).collect::<Vec<_>>(),
            "use_vertex_selection": d.use_vertex_selection,
        }),
        Err(_) => Value::Null,
    }
}

pub fn skin_morph_status(state: &AppState) -> Value {
    match skin_morph_entry(state) {
        Ok((id, index, d)) => json!({
            "node": id.0,
            "index": index,
            "bones": d.bones.iter().map(|b| b.node.0).collect::<Vec<_>>(),
            "joint_types": d.bones.iter().map(|b| if b.planar { "Planar" } else { "Ball" }).collect::<Vec<_>>(),
            "morphs": d.morphs.iter().enumerate().map(|(i, m)| json!({ "name": m.name, "bone": d.bones.get(m.bone).map(|b| b.node.0), "angle": m.angle_deg, "influence": m.influence_deg, "falloff": m.falloff, "enabled": m.enabled, "deltas": m.deltas.len(), "current_angle": d.bone_angle(m.bone, &|id| Some(state.scene.world_matrix(id))), "weight": d.morph_weight(i, &|id| Some(state.scene.world_matrix(id))), "external": m.external.map(|n| n.0), "reload_selected": m.reload_selected, "curve": m.curve })).collect::<Vec<_>>(),
            "current": state.skin_morph_current,
            "current_bone": state.skin_morph_bone.map(|b| b.0),
            "selection": d.selection,
            "editing": d.editing,
            "soft": { "on": d.soft.on, "radius": d.soft.radius, "edge_limit_on": d.soft.edge_limit_on, "edge_limit": d.soft.edge_limit, "curve": d.soft.curve },
            "mirror": { "plane": d.mirror.axis, "offset": d.mirror.offset, "threshold": d.mirror.threshold, "show_plane": d.mirror.show_plane, "preview_bone": d.mirror.preview_bone, "preview_vertices": d.mirror.preview_vertices },
            "options": { "beginner_mode": d.options.beginner_mode, "show_driver_matrix": d.options.show_driver_matrix, "show_morph_matrix": d.options.show_morph_matrix, "show_current_angle": d.options.show_current_angle, "show_edges": d.options.show_edges, "matrix_size": d.options.matrix_size, "bone_size": d.options.bone_size },
        }),
        Err(_) => Value::Null,
    }
}

fn select_ids<T: Ord + Copy>(current: &mut Vec<T>, ids: &[T], op: crate::SelectOp) {
    let mut set: BTreeSet<T> = match op {
        crate::SelectOp::Replace => BTreeSet::new(),
        _ => current.iter().copied().collect(),
    };
    match op {
        crate::SelectOp::Subtract => ids.iter().for_each(|i| {
            set.remove(i);
        }),
        _ => set.extend(ids.iter().copied()),
    }
    *current = set.into_iter().collect();
}

/// The Skin Morph entry when its Points level is on.
pub fn points_level(state: &AppState) -> Option<(NodeId, usize)> {
    let (id, index) = find_entry(state, |m| matches!(m, Modifier::SkinMorph(_)))?;
    let n = state.scene.node(id)?;
    (n.active_entry == Some(index) && n.modifiers.get(index)?.sub_level == 1).then_some((id, index))
}

/// The Skin Wrap entry when its Control Vertices level is on.
pub fn control_vertex_level(state: &AppState) -> Option<(NodeId, usize)> {
    let (id, index) = find_entry(state, |m| matches!(m, Modifier::SkinWrap(_)))?;
    let n = state.scene.node(id)?;
    (n.active_entry == Some(index) && n.modifiers.get(index)?.sub_level == 1).then_some((id, index))
}

/// World positions of the Skin Morph entry's output vertices (what the Points level shows).
pub fn skin_morph_points(state: &AppState) -> Option<(NodeId, usize, Vec<Vec3>)> {
    let (id, index, _) = skin_morph_entry(state).ok()?;
    let world = state.scene.object_matrix(id);
    let mesh = state.scene.evaluated(id, Some(index)).value.mesh.clone();
    Some((id, index, mesh.vertices().iter().map(|v| world.transform_point3(v.position)).collect()))
}

/// World centre of the selected points at the Points level (the gizmo's place).
pub fn points_center(state: &AppState) -> Option<Vec3> {
    points_level(state)?;
    let (_, _, d) = skin_morph_entry(state).ok()?;
    let (_, _, pts) = skin_morph_points(state)?;
    let sel: Vec<Vec3> = d.selection.iter().filter_map(|v| pts.get(*v as usize).copied()).collect();
    (!sel.is_empty()).then(|| sel.iter().sum::<Vec3>() / sel.len() as f32)
}

/// The transform tools at the Points level: in Edit mode the selected
/// points (soft selection included) move into the edited morph's deltas;
/// with Beginner Mode off, into the highlighted bone's morph at 100% (a new
/// one when it has none). Otherwise nothing moves.
pub fn transform_points(state: &mut AppState, label: &str, m: glam::Mat4) -> Result<Value, String> {
    let (id, index, mut d) = skin_morph_entry(state)?;
    if d.selection.is_empty() {
        return Ok(skin_morph_status(state));
    }
    let mut created = false;
    let target = {
        let world = |n: NodeId| Some(state.scene.world_matrix(n));
        match d.editing {
            Some(i) => i,
            None if !d.options.beginner_mode => {
                let bone = state.skin_morph_bone.and_then(|b| d.bones.iter().position(|x| x.node == b)).ok_or("highlight a bone in the list first")?;
                match (0..d.morphs.len()).find(|i| d.morphs[*i].bone == bone && d.morph_weight(*i, &world) >= 0.999) {
                    Some(i) => i,
                    None => {
                        created = true;
                        d.create_morph(&state.scene, bone)
                    }
                }
            }
            None => return Ok(skin_morph_status(state)),
        }
    };
    let world = |n: NodeId| Some(state.scene.world_matrix(n));
    let weight = d.morph_weight(target, &world).max(1e-3);
    let Some(bw) = d.bones.get(d.morphs[target].bone).and_then(|b| world(b.node)) else { return Err("the morph's bone is gone".into()) };
    let out = state.scene.evaluated(id, Some(index)).value.mesh.clone();
    let obj = state.scene.object_matrix(id);
    let soft = d.soft_weights(&out, obj);
    let inv = bw.inverse();
    let morph = &mut d.morphs[target];
    for (v, s) in soft.iter().enumerate().filter(|(_, s)| **s > 0.0) {
        let p = obj.transform_point3(out.vertices()[v].position);
        let delta = inv.transform_vector3((m.transform_point3(p) - p) * *s) / weight;
        match morph.deltas.iter_mut().find(|(x, _)| *x as usize == v) {
            Some(e) => e.1 += delta,
            None => morph.deltas.push((v as u32, delta)),
        }
    }
    put(state, id, index, Modifier::SkinMorph(Box::new(d)), label)?;
    if created || state.skin_morph_current.is_none() {
        state.skin_morph_current = Some(target);
    }
    Ok(skin_morph_status(state))
}

/// World positions of the control vertices: ((control object, vertex), position).
pub fn control_vertices(state: &AppState) -> Vec<((u32, u32), Vec3)> {
    let Ok((_, _, d)) = skin_wrap_entry(state) else { return Vec::new() };
    let mut out = Vec::new();
    for (b, node) in d.bases.iter().enumerate() {
        let world = state.scene.object_matrix(*node);
        for (v, vx) in state.scene.eval_mesh(*node).value.mesh.vertices().iter().enumerate() {
            out.push(((b as u32, v as u32), world.transform_point3(vx.position)));
        }
    }
    out
}

pub fn skin_wrap_patch_entry(state: &AppState) -> Result<(NodeId, usize, SkinWrapPatchData), String> {
    let (id, index) = find_entry(state, |m| matches!(m, Modifier::SkinWrapPatch(_))).ok_or("no Skin Wrap Patch on the selection (add the modifier first)")?;
    let Modifier::SkinWrapPatch(d) = &state.scene.node(id).ok_or("missing node")?.modifiers[index].modifier else { unreachable!() };
    Ok((id, index, (**d).clone()))
}

fn edit_skin_wrap_patch(state: &mut AppState, label: &str, f: impl FnOnce(&mut SkinWrapPatchData, NodeId, usize, &AppState) -> Result<(), String>) -> Result<Value, String> {
    let (id, index, mut data) = skin_wrap_patch_entry(state)?;
    f(&mut data, id, index, state)?;
    put(state, id, index, Modifier::SkinWrapPatch(Box::new(data)), label)?;
    Ok(skin_wrap_patch_status(state))
}

/// Pick Patch: the picked node must output a patch.
pub fn check_patch(state: &AppState, node: NodeId) -> Result<(), String> {
    state.scene.node(node).ok_or_else(|| format!("no node {}", node.0))?;
    if state.scene.evaluated(node, None).value.patch.is_none() {
        return Err("Pick Patch: pick a patch object".into());
    }
    Ok(())
}

/// The rotation from bind to the morph pose of bone `from`, mirrored across
/// the mirror plane onto bone `to` (in the parents' bind frames).
fn mirrored_pose(d: &SkinMorphData, from: usize, to: usize, pose: glam::Quat) -> glam::Quat {
    let rot = |m: glam::Mat4| m.to_scale_rotation_translation().1.normalize();
    let (ps, pt) = (rot(d.bones[from].parent_bind_world), rot(d.bones[to].parent_bind_world));
    let delta_world = ps * (pose * d.bind_rotation(from).inverse()) * ps.inverse();
    let (axis, angle) = delta_world.to_axis_angle();
    // A rotation axis is a pseudovector: its mirror image flips sign.
    let axis = -d.mirror.reflect_vector(axis);
    let delta_t = pt.inverse() * glam::Quat::from_axis_angle(axis.normalize_or_zero(), angle) * pt;
    (delta_t * d.bind_rotation(to)).normalize()
}

/// The Skin Morph, Skin Wrap and Skin Wrap Patch tools beyond the basic list editing.
pub(crate) fn tools(state: &mut AppState, cmd: Command) -> Result<Value, String> {
    use Command as C;
    match cmd {
        C::SkinMorphSelect { bone, morph } => {
            let (_, _, d) = skin_morph_entry(state)?;
            if let Some(b) = bone {
                d.bones.iter().find(|x| x.node == NodeId(b)).ok_or("the bone is not in the Skin Morph's list")?;
            }
            if let Some(m) = morph {
                let t = d.morphs.get(m as usize).ok_or("no such morph")?;
                state.skin_morph_bone = d.bones.get(t.bone).map(|b| b.node);
            } else {
                state.skin_morph_bone = bone.map(NodeId);
            }
            state.skin_morph_current = morph.map(|m| m as usize);
            Ok(skin_morph_status(state))
        }
        C::SkinMorphPickBone => {
            skin_morph_entry(state)?;
            state.pick_mode = if state.pick_mode == Some(crate::PickMode::SkinMorphBone) { None } else { Some(crate::PickMode::SkinMorphBone) };
            Ok(skin_morph_status(state))
        }
        C::SkinMorphPickExternal => {
            skin_morph_entry(state)?;
            state.skin_morph_current.ok_or("highlight a morph first")?;
            state.pick_mode = if state.pick_mode == Some(crate::PickMode::SkinMorphExternal) { None } else { Some(crate::PickMode::SkinMorphExternal) };
            Ok(skin_morph_status(state))
        }
        C::SkinMorphSelectPoints { ids, op } => edit_skin_morph(state, "Select Points", |d, id, index, st| {
            let n = input_mesh(st, id, index).vertex_count() as u32;
            if let Some(bad) = ids.iter().find(|v| **v >= n) {
                return Err(format!("no vertex {bad} (the mesh has {n})"));
            }
            select_ids(&mut d.selection, &ids, op);
            Ok(())
        }),
        C::SkinMorphSelectionTool { tool } => edit_skin_morph(state, &tool.clone(), |d, id, index, st| {
            use loom_mesh::select_ops::{self as so, SubLevel};
            let mesh = input_mesh(st, id, index);
            // The edges with both ends selected.
            let selected: BTreeSet<u32> = d.selection.iter().copied().collect();
            let seeds: Vec<loom_mesh::HalfEdgeId> = d
                .selection
                .iter()
                .flat_map(|v| so::vertex_edges(&mesh, loom_mesh::VertexId(*v)))
                .filter(|e| mesh.edge_vertices(*e).is_some_and(|(a, b)| selected.contains(&a.0) && selected.contains(&b.0)))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            d.selection = match tool.as_str() {
                "Grow" => so::grow(&mesh, SubLevel::Vertex, &d.selection),
                "Shrink" => so::shrink(&mesh, SubLevel::Vertex, &d.selection),
                "Loop" | "Ring" => {
                    if seeds.is_empty() {
                        return Err(format!("{tool} needs two selected vertices on one edge"));
                    }
                    let edges = if tool == "Loop" { so::edge_loop(&mesh, &seeds) } else { so::edge_ring(&mesh, &seeds) };
                    let mut out: BTreeSet<u32> = d.selection.iter().copied().collect();
                    for e in edges {
                        if let Some((a, b)) = mesh.edge_vertices(e) {
                            out.insert(a.0);
                            out.insert(b.0);
                        }
                    }
                    out.into_iter().collect()
                }
                other => return Err(format!("no selection tool {other:?} (Ring, Loop, Shrink, Grow)")),
            };
            Ok(())
        }),
        C::SkinMorphSetOption { name, value } => edit_skin_morph(state, "Skin Morph Option", |d, _, _, _| {
            let on = value != 0.0;
            let v = value as f32;
            match name.as_str() {
                "Use Soft Selection" => d.soft.on = on,
                "Radius" => d.soft.radius = v.max(0.0),
                "Use Edge Limit" => d.soft.edge_limit_on = on,
                "Edge Limit" => d.soft.edge_limit = value.clamp(0.0, 1000.0).round() as u32,
                "Mirror Plane" => d.mirror.axis = (value.max(0.0) as u32).min(2),
                "Mirror Offset" => d.mirror.offset = v,
                "Mirror Threshold" => d.mirror.threshold = v.max(0.0),
                "Show Mirror Plane" => d.mirror.show_plane = on,
                "Preview Bone" => d.mirror.preview_bone = on,
                "Preview Vertices" => d.mirror.preview_vertices = on,
                "Beginner Mode" => d.options.beginner_mode = on,
                "Show Driver Bone Matrix" => d.options.show_driver_matrix = on,
                "Show Morph Bone Matrix" => d.options.show_morph_matrix = on,
                "Show Current Angle" => d.options.show_current_angle = on,
                "Show Edges" => d.options.show_edges = on,
                "Matrix Size" => d.options.matrix_size = v.max(0.0),
                "Bone Size" => d.options.bone_size = v.max(0.0),
                other => return Err(format!("Skin Morph has no option {other:?}")),
            }
            Ok(())
        }),
        C::SkinMorphSetCurve { graph, morph, points } => edit_skin_morph(state, "Graph", |d, _, _, _| {
            let mut pts: Vec<[f32; 2]> = points.iter().map(|p| [p[0].clamp(0.0, 1.0), p[1]]).collect();
            pts.sort_by(|a, b| a[0].total_cmp(&b[0]));
            if pts.len() < 2 {
                return Err("a graph needs at least two points".into());
            }
            match graph.as_str() {
                "Soft Selection" => d.soft.curve = pts,
                "Falloff" => d.morphs.get_mut(morph.ok_or("the falloff graph needs a morph")? as usize).ok_or("no such morph")?.curve = pts,
                other => return Err(format!("no graph {other:?} (Soft Selection, Falloff)")),
            }
            Ok(())
        }),
        C::SkinMorphResetGraph => edit_skin_morph(state, "Reset Graph", |d, _, _, _| {
            d.soft.curve = loom_scene::morph::default_curve();
            Ok(())
        }),
        C::SkinMorphEdit { morph } => {
            let (id, index, mut d) = skin_morph_entry(state)?;
            let m = morph as usize;
            d.morphs.get(m).ok_or("no such morph")?;
            let mut changes = Vec::new();
            // Leaving Edit (or switching morphs) gives the bone its own transform back.
            if let Some((bone, t)) = d.edit_restore.take() {
                if let Some(n) = state.scene.node(bone) {
                    changes.push(Change::SetTransform { id: bone, before: n.transform.clone(), after: t });
                }
            }
            let entering = d.editing != Some(m);
            d.editing = entering.then_some(m);
            if entering {
                let (pose, bone_index) = (d.morphs[m].pose, d.morphs[m].bone);
                let bone = d.bones[bone_index].node;
                let world = |n: NodeId| Some(state.scene.world_matrix(n));
                if let (Some(pose), Some(cur), Some(node)) = (pose, d.local_rotation(bone_index, &world), state.scene.node(bone)) {
                    if cur.angle_between(pose) > 1e-4 {
                        // Pose the bone at the morph: its rotation in the parent's frame becomes the pose.
                        let (s, _, t) = node.transform.matrix().to_scale_rotation_translation();
                        let posed = loom_scene::Transform::from_matrix_near(glam::Mat4::from_scale_rotation_translation(s, pose, t), &node.transform, Vec3::ZERO);
                        let after = loom_scene::Transform { position: node.transform.position.clone(), scale: node.transform.scale.clone(), rotation: posed.rotation };
                        d.edit_restore = Some((bone, node.transform.clone()));
                        changes.push(Change::SetTransform { id: bone, before: node.transform.clone(), after });
                    }
                }
            }
            let node = state.scene.node(id).ok_or("missing node")?;
            let mut stack = node.modifiers.clone();
            stack[index].modifier = Modifier::SkinMorph(Box::new(d));
            if entering {
                // Edit turns the Points level on so the transform tools shape the morph.
                stack[index].sub_level = 1;
            }
            changes.push(Change::SetStack { id, before: node.modifiers.clone(), after: stack });
            commit(state, "Edit Morph", Change::Group(changes))?;
            if entering {
                if let Some(n) = state.scene.transform_mut(id) {
                    n.active_entry = Some(index);
                }
                state.skin_morph_current = Some(m);
            }
            Ok(skin_morph_status(state))
        }
        C::SkinMorphClearVerts { morph } | C::SkinMorphRemoveVerts { morph } => {
            let clear = matches!(cmd, C::SkinMorphClearVerts { .. });
            edit_skin_morph(state, if clear { "Clear Verts" } else { "Remove Verts" }, |d, _, _, _| {
                let sel: BTreeSet<u32> = d.selection.iter().copied().collect();
                let m = d.morphs.get_mut(morph as usize).ok_or("no such morph")?;
                if clear {
                    for (v, delta) in m.deltas.iter_mut() {
                        if sel.contains(v) {
                            *delta = Vec3::ZERO;
                        }
                    }
                } else {
                    m.deltas.retain(|(v, _)| !sel.contains(v));
                }
                Ok(())
            })
        }
        C::SkinMorphSetJointType { bone, planar } => edit_skin_morph(state, "Joint Type", |d, _, _, _| {
            d.bones.iter_mut().find(|b| b.node == NodeId(bone)).ok_or("the bone is not in the Skin Morph's list")?.planar = planar;
            Ok(())
        }),
        C::SkinMorphSetExternal { morph, node } => {
            if let Some(n) = node {
                state.scene.node(NodeId(n)).ok_or_else(|| format!("no node {n}"))?;
            }
            edit_skin_morph(state, "External Mesh", |d, id, _, _| {
                if node == Some(id.0) {
                    return Err("pick another object".into());
                }
                d.morphs.get_mut(morph as usize).ok_or("no such morph")?.external = node.map(NodeId);
                Ok(())
            })
        }
        C::SkinMorphReloadTarget { morph } => edit_skin_morph(state, "Reload Target", |d, id, _, st| {
            let m = d.morphs.get(morph as usize).ok_or("no such morph")?;
            let target = m.external.ok_or("the morph has no External Mesh")?;
            let tmesh = st.scene.eval_mesh(target).value.mesh.clone();
            let base = st.scene.node(id).ok_or("missing node")?.object.display_output();
            if tmesh.vertex_count() != base.vertex_count() {
                return Err(format!("the External Mesh has {} vertices, the object {}", tmesh.vertex_count(), base.vertex_count()));
            }
            let to_bone = d.bones[m.bone].bind_world.inverse() * st.scene.object_matrix(id);
            let only: Option<BTreeSet<u32>> = m.reload_selected.then(|| d.selection.iter().copied().collect());
            let m = &mut d.morphs[morph as usize];
            for (v, (a, b)) in base.vertices().iter().zip(tmesh.vertices()).enumerate() {
                let v = v as u32;
                if only.as_ref().is_some_and(|s| !s.contains(&v)) {
                    continue;
                }
                let delta = to_bone.transform_vector3(b.position - a.position);
                m.deltas.retain(|(x, _)| *x != v);
                if delta.length() > 1e-6 {
                    m.deltas.push((v, delta));
                }
            }
            m.deltas.sort_by_key(|(v, _)| *v);
            Ok(())
        }),
        C::SkinMorphPasteMirror { bone } => edit_skin_morph(state, "Paste Mirror", |d, id, _, st| {
            let from = d.bones.iter().position(|b| b.node == NodeId(bone)).ok_or("the bone is not in the Skin Morph's list")?;
            let to = mirror_bone(d, from, &|n| st.scene.world_matrix(n)).ok_or("no bone in the list on the other side of the mirror plane (within Mirror Threshold)")?;
            let base = st.scene.node(id).ok_or("missing node")?.object.display_output();
            let world = st.scene.object_matrix(id);
            let pos: Vec<Vec3> = base.vertices().iter().map(|v| world.transform_point3(v.position)).collect();
            let (sw, tw) = (d.bones[from].bind_world, d.bones[to].bind_world.inverse());
            let dm: &SkinMorphData = d;
            let copies: Vec<_> = dm
                .morphs
                .iter()
                .filter(|m| m.bone == from)
                .map(|m| {
                    let mut c = m.clone();
                    c.bone = to;
                    c.name = format!("{} Mirror", m.name);
                    c.pose = m.pose.map(|p| mirrored_pose(dm, from, to, p));
                    c.deltas = m.deltas.iter().filter_map(|(v, delta)| Some((mirror_vertex(&dm.mirror, &pos, *v)?, tw.transform_vector3(dm.mirror.reflect_vector(sw.transform_vector3(*delta)))))).collect();
                    c
                })
                .collect();
            if copies.is_empty() {
                return Err("the bone has no morphs to paste".into());
            }
            d.morphs.extend(copies);
            Ok(())
        }),
        C::SkinWrapPickControl => {
            skin_wrap_entry(state)?;
            state.pick_mode = if state.pick_mode == Some(crate::PickMode::SkinWrapControl) { None } else { Some(crate::PickMode::SkinWrapControl) };
            Ok(skin_wrap_status(state))
        }
        C::SkinWrapSelectControlVerts { ids, op } => edit_skin_wrap(state, "Select Control Verts", |d, _, _, _| {
            let ids: Vec<(u32, u32)> = ids.iter().map(|p| (p[0], p[1])).collect();
            if let Some(bad) = ids.iter().find(|(b, v)| d.base_counts.get(*b as usize).is_none_or(|(_, n)| *v as usize >= *n)) {
                return Err(format!("no control vertex {bad:?}"));
            }
            select_ids(&mut d.selection, &ids, op);
            Ok(())
        }),
        C::SkinWrapSetLocal { name, value } => edit_skin_wrap(state, "Local Parameter", |d, wrapped, index, st| {
            if d.selection.is_empty() {
                return Err("select control vertices first (Control Vertices level)".into());
            }
            let v = value as f32;
            for (b, i) in d.selection.clone() {
                let Some(l) = d.locals.get_mut(b as usize).and_then(|l| l.get_mut(i as usize)) else { continue };
                match name.as_str() {
                    "Local Strength" => l.strength = v,
                    "Local Scale Mult" => l.scale = v.max(0.001),
                    "Local X" => l.x = v.max(0.001),
                    "Local Y" => l.y = v.max(0.001),
                    "Local Z" => l.z = v.max(0.001),
                    other => return Err(format!("Skin Wrap has no local parameter {other:?}")),
                }
            }
            d.rebind(&st.scene, wrapped, index);
            Ok(())
        }),
        C::SkinWrapMirrorSelected => edit_skin_wrap(state, "Mirror Selected", |d, wrapped, index, st| {
            let geos = d.control_geometry(&st.scene);
            let mut copies = Vec::new();
            for (b, v) in &d.selection {
                let Some(p) = geos.get(*b as usize).and_then(|g| g.pos.get(*v as usize)) else { continue };
                let q = d.mirror.reflect_point(*p);
                let from = d.local(*b, *v);
                for (b2, g) in geos.iter().enumerate() {
                    for (v2, p2) in g.pos.iter().enumerate() {
                        if (b2 as u32, v2 as u32) != (*b, *v) && p2.distance(q) <= d.mirror.threshold {
                            copies.push((b2, v2, from));
                        }
                    }
                }
            }
            if copies.is_empty() {
                return Err("no control vertex near the mirror image of the selection (Mirror Threshold)".into());
            }
            for (b, v, l) in copies {
                if let Some(slot) = d.locals.get_mut(b).and_then(|x| x.get_mut(v)) {
                    *slot = l;
                }
            }
            d.rebind(&st.scene, wrapped, index);
            Ok(())
        }),
        C::SkinWrapBakeControlVerts => {
            let (_, _, d) = skin_wrap_entry(state)?;
            let mut changes = Vec::new();
            for (i, b) in d.bases.iter().enumerate() {
                let Some(n) = state.scene.node(*b) else { continue };
                let mut after = n.props();
                after.skin_wrap_bake = Some(Box::new(loom_scene::morph::SkinWrapBake { locals: d.locals.get(i).cloned().unwrap_or_default(), falloff: d.falloff, distance_influence: d.distance_influence, face_limit: d.face_limit }));
                changes.push(Change::SetNodeProps { id: *b, before: Box::new(n.props()), after: Box::new(after) });
            }
            if changes.is_empty() {
                return Err("no control objects to bake onto".into());
            }
            commit(state, "Bake Control Verts", Change::Group(changes))?;
            Ok(skin_wrap_status(state))
        }
        C::SkinWrapRetrieveControlVerts => edit_skin_wrap(state, "Retrieve Control Verts", |d, wrapped, index, st| {
            let mut found = false;
            for (i, b) in d.bases.clone().iter().enumerate() {
                let Some(bake) = st.scene.node(*b).and_then(|n| n.skin_wrap_bake.clone()) else { continue };
                if !found {
                    (d.falloff, d.distance_influence, d.face_limit) = (bake.falloff, bake.distance_influence, bake.face_limit);
                    found = true;
                }
                let n = st.scene.eval_mesh(*b).value.mesh.vertex_count();
                if bake.locals.len() == n {
                    d.locals.resize(d.bases.len(), Vec::new());
                    d.locals[i] = bake.locals.clone();
                }
            }
            if !found {
                return Err("no control object has baked control vertex data".into());
            }
            d.rebind(&st.scene, wrapped, index);
            Ok(())
        }),
        C::SkinWrapPatchPick { node } => {
            let picked = NodeId(node);
            check_patch(state, picked)?;
            edit_skin_wrap_patch(state, "Pick Patch", |d, wrapped, index, st| {
                if picked == wrapped {
                    return Err("pick another object".into());
                }
                d.patch = Some(picked);
                d.resample(&st.scene, wrapped, index);
                Ok(())
            })
        }
        C::SkinWrapPatchSetSampleRate { rate } => edit_skin_wrap_patch(state, "Sample Rate", |d, wrapped, index, st| {
            d.sample_rate = rate.clamp(1, 100);
            d.resample(&st.scene, wrapped, index);
            Ok(())
        }),
        C::SkinWrapPatchResample => edit_skin_wrap_patch(state, "Resample", |d, wrapped, index, st| {
            d.patch.ok_or("Pick Patch first")?;
            d.resample(&st.scene, wrapped, index);
            Ok(())
        }),
        other => Err(format!("{} is not a Morpher command", other.name())),
    }
}

/// Paste Mirror's target: the listed bone nearest the mirror image of bone
/// `from`'s position, within Mirror Threshold.
pub fn mirror_bone(d: &SkinMorphData, from: usize, world: &dyn Fn(NodeId) -> glam::Mat4) -> Option<usize> {
    let origin = |n: NodeId| world(n).w_axis.truncate();
    let want = d.mirror.reflect_point(origin(d.bones.get(from)?.node));
    (0..d.bones.len())
        .filter(|i| *i != from)
        .map(|i| (i, origin(d.bones[i].node).distance(want)))
        .filter(|(_, dist)| *dist <= d.mirror.threshold)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// The vertex nearest the mirror image of vertex `v` (positions `pos`), within Mirror Threshold.
pub fn mirror_vertex(mirror: &loom_scene::morph::MorphMirror, pos: &[Vec3], v: u32) -> Option<u32> {
    let q = mirror.reflect_point(*pos.get(v as usize)?);
    pos.iter().enumerate().map(|(i, p)| (i, p.distance(q))).filter(|(_, x)| *x <= mirror.threshold).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(i, _)| i as u32)
}

pub fn skin_wrap_patch_status(state: &AppState) -> Value {
    match skin_wrap_patch_entry(state) {
        Ok((id, index, d)) => json!({
            "node": id.0,
            "index": index,
            "patch": d.patch.map(|p| json!({ "node": p.0, "name": state.scene.node(p).map(|n| n.name.clone()).unwrap_or_default() })),
            "sample_rate": d.sample_rate,
            "bound": d.binds.iter().filter(|b| !b.is_empty()).count(),
            "vertices": d.binds.len(),
        }),
        Err(_) => Value::Null,
    }
}
