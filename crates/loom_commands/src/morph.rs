//! Morpher and Skin Morph commands.

use crate::execute::commit;
use crate::state::AppState;
use crate::Command;
use glam::Vec3;
use loom_mesh::Mesh;
use loom_scene::morph::{MorphTarget, MorpherData, SkinMorphData, SkinMorphTarget, SkinWrapData};
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
                let angle = d.bone_angle(i, &|id| Some(st.scene.world_matrix(id))).unwrap_or(0.0);
                let name = format!("Morph {}", d.morphs.len() + 1);
                d.morphs.push(SkinMorphTarget { bone: i, name, angle_deg: angle, influence_deg: 90.0, falloff: 0, enabled: true, deltas: Vec::new() });
                Ok(())
            })?;
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
                "Falloff" => m.falloff = (value.max(0.0) as u32).min(3),
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
                Ok(())
            })?;
            state.skin_morph_current = None;
            Ok(r)
        }
        C::SkinMorphReload { morph } => edit_skin_morph(state, "Reload Morph", |d, _, _, st| {
            let bone = d.morphs.get(morph as usize).ok_or("no such morph")?.bone;
            let angle = d.bone_angle(bone, &|id| Some(st.scene.world_matrix(id))).unwrap_or(0.0);
            d.morphs[morph as usize].angle_deg = angle;
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
            match name.as_str() {
                "Deformation Engine" => d.engine = (value.max(0.0) as u32).min(1),
                "Falloff" => d.falloff = (value as f32).max(0.0),
                "Distance Influence" => d.distance_influence = (value as f32).max(0.001),
                "Face Limit" => d.face_limit = (value.max(1.0)) as u32,
                "Threshold" => d.threshold = (value as f32).max(0.0),
                "Weight All Points" => d.weight_all_points = value != 0.0,
                other => return Err(format!("Skin Wrap has no option {other:?}")),
            }
            d.rebind(&st.scene, wrapped, index);
            Ok(())
        }),
        C::SkinWrapReset => edit_skin_wrap(state, "Skin Wrap Reset", |d, wrapped, index, st| {
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
        other => Err(format!("{} is not a Morpher command", other.name())),
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
            "morphs": d.morphs.iter().map(|m| json!({ "name": m.name, "bone": d.bones.get(m.bone).map(|b| b.node.0), "angle": m.angle_deg, "influence": m.influence_deg, "falloff": m.falloff, "enabled": m.enabled, "deltas": m.deltas.len(), "current_angle": d.bone_angle(m.bone, &|id| Some(state.scene.world_matrix(id))) })).collect::<Vec<_>>(),
            "current": state.skin_morph_current,
        }),
        Err(_) => Value::Null,
    }
}
