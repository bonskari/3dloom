//! Transform tools of the Extras toolbar's Array flyout and the Align
//! flyout: Snapshot (clones over time), Clone and Align, Place Highlight.

use crate::execute::commit;
use crate::state::{AppState, CloneKind, PickMode};
use crate::viewport::Dialog;
use crate::Command;
use glam::{EulerRot, Mat4, Quat, Vec3};
use loom_scene::{Change, Node, NodeId, ObjectKind, PolyObject, Transform};
use serde_json::{json, Value};

fn missing(id: NodeId) -> String {
    format!("no node {}", id.0)
}

fn base_of(name: &str) -> &str {
    name.trim_end_matches(|c: char| c.is_ascii_digit())
}

/// The frames a Snapshot clones at: the current one, or `copies` spread
/// evenly from `from` to `to`.
pub fn snapshot_frames(current: f32, range: bool, from: i32, to: i32, copies: u32) -> Vec<f32> {
    if !range {
        return vec![current];
    }
    let n = copies.max(1);
    if n == 1 {
        return vec![from as f32];
    }
    (0..n).map(|i| from as f32 + (to - from) as f32 * i as f32 / (n - 1) as f32).collect()
}

/// Tools > Snapshot: a clone of each selected object as it is at each
/// frame, without transform animation and unlinked. `method`: 0 Copy,
/// 1 Instance, 2 Reference, 3 Mesh (the evaluated geometry as an Editable Poly).
pub fn snapshot(state: &mut AppState, range: bool, from: i32, to: i32, copies: u32, method: u32) -> Result<Value, String> {
    let ids: Vec<NodeId> = state.selection.iter().copied().collect();
    if ids.is_empty() {
        return Err("select the object to snapshot first".into());
    }
    if method > 3 {
        return Err("method is 0 Copy, 1 Instance, 2 Reference or 3 Mesh".into());
    }
    let mut sim = state.scene.clone();
    let mut changes = Vec::new();
    let mut created = Vec::new();
    for t in snapshot_frames(state.scene.time(), range, from, to, copies) {
        for id in &ids {
            let src = state.scene.node(*id).ok_or_else(|| missing(*id))?;
            let (world, mesh) = state.scene.at_time(t, || (state.scene.world_matrix(*id), (method == 3).then(|| state.scene.eval_mesh(*id).value.mesh.clone())));
            let mut node = src.clone();
            node.id = NodeId(0);
            node.name = sim.next_name(base_of(&src.name));
            node.parent = None;
            node.constraints = Default::default();
            node.transform = Transform::from_matrix(world);
            match method {
                0 => {
                    node.instance_of = None;
                    node.reference_barrier = None;
                }
                1 => {
                    node.instance_of = Some(src.instance_of.unwrap_or(*id));
                    node.reference_barrier = src.reference_barrier;
                }
                2 => {
                    let root = src.instance_of.unwrap_or(*id);
                    node.instance_of = Some(root);
                    node.reference_barrier = Some(node.modifiers.len());
                }
                _ => {
                    node.object = ObjectKind::EditablePoly(PolyObject::from_mesh(mesh.unwrap_or_default()));
                    node.modifiers.clear();
                    node.active_entry = None;
                    node.instance_of = None;
                    node.reference_barrier = None;
                }
            }
            let nid = sim.add_node(node.clone());
            node.id = nid;
            changes.push(Change::AddNode(node));
            created.push(nid.0);
        }
    }
    commit(state, "Snapshot", Change::Group(changes))?;
    Ok(json!({ "created": created }))
}

/// Settings of Clone and Align's Clone / Align Parameters rollouts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CloneAlign {
    pub kind: CloneKind,
    pub link: bool,
    pub position: [bool; 3],
    /// In each destination's local axes.
    pub position_offset: [f32; 3],
    pub orientation: [bool; 3],
    /// Degrees about each local axis.
    pub orientation_offset: [f32; 3],
    pub match_scale: [bool; 3],
}

/// Tools > Align > Clone and Align: the selection (the sources) is cloned
/// onto every destination; one source aligns its pivot, several their
/// selection centre, to each destination's pivot on the chosen world axes.
pub fn clone_and_align(state: &mut AppState, destinations: &[NodeId], s: CloneAlign) -> Result<Value, String> {
    let sources: Vec<NodeId> = state.selection.iter().copied().filter(|id| !destinations.contains(id)).collect();
    if sources.is_empty() {
        return Err("select the source objects first".into());
    }
    if destinations.is_empty() {
        return Err("pick destination objects".into());
    }
    for d in destinations {
        state.scene.node(*d).ok_or_else(|| missing(*d))?;
    }
    // The sources' frame: one source's pivot and rotation, else the centre of their bounds.
    let source_frame = if let [one] = sources.as_slice() {
        let (_, q, t) = state.scene.world_matrix(*one).to_scale_rotation_translation();
        Mat4::from_rotation_translation(q, t)
    } else {
        let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
        for id in &sources {
            let (a, b) = state.object_bounds(*id).unwrap_or_else(|| {
                let p = state.scene.world_matrix(*id).w_axis.truncate();
                (p, p)
            });
            min = min.min(a);
            max = max.max(b);
        }
        Mat4::from_translation((min + max) * 0.5)
    };
    let (_, sq, sp) = source_frame.to_scale_rotation_translation();
    let mut sim = state.scene.clone();
    let mut changes = Vec::new();
    let mut created = Vec::new();
    for d in destinations {
        let dw = state.scene.world_matrix(*d);
        let (dscale, dq, dt) = dw.to_scale_rotation_translation();
        let off = Vec3::from(s.position_offset) * Vec3::from(s.position.map(|on| if on { 1.0 } else { 0.0 }));
        let at = dt + dq * off;
        let mut p = sp;
        for i in 0..3 {
            if s.position[i] {
                p[i] = at[i];
            }
        }
        let offs = s.orientation_offset.map(|a| a.to_radians());
        let rot = if s.orientation == [true; 3] {
            dq * Quat::from_euler(EulerRot::XYZ, offs[0], offs[1], offs[2])
        } else {
            let mine = sq.to_euler(EulerRot::XYZ);
            let theirs = dq.to_euler(EulerRot::XYZ);
            let pick = |i: usize, m: f32, t: f32| if s.orientation[i] { t + offs[i] } else { m };
            Quat::from_euler(EulerRot::XYZ, pick(0, mine.0, theirs.0), pick(1, mine.1, theirs.1), pick(2, mine.2, theirs.2))
        };
        let delta = Mat4::from_rotation_translation(rot, p) * source_frame.inverse();
        for src_id in &sources {
            let src = state.scene.node(*src_id).ok_or_else(|| missing(*src_id))?;
            let mut node = src.clone();
            node.id = NodeId(0);
            node.name = sim.next_name(base_of(&src.name));
            node.instance_of = if s.kind == CloneKind::Copy { None } else { Some(src.instance_of.unwrap_or(*src_id)) };
            node.reference_barrier = match s.kind {
                CloneKind::Reference => Some(node.modifiers.len()),
                CloneKind::Instance => src.reference_barrier,
                CloneKind::Copy => None,
            };
            let mut world = delta * state.scene.world_matrix(*src_id);
            if s.match_scale.iter().any(|m| *m) {
                let (mut sc, q, t) = world.to_scale_rotation_translation();
                for i in 0..3 {
                    if s.match_scale[i] {
                        sc[i] = dscale[i];
                    }
                }
                world = Mat4::from_scale_rotation_translation(sc, q, t);
            }
            let parent = if s.link { Some(*d) } else { None };
            node.parent = parent;
            let frame = parent.map(|p| state.scene.world_matrix(p)).unwrap_or(Mat4::IDENTITY);
            node.transform = Transform::from_matrix(frame.inverse() * world);
            let nid = sim.add_node(node.clone());
            node.id = nid;
            changes.push(Change::AddNode(node));
            created.push(nid.0);
        }
    }
    commit(state, "Clone and Align", Change::Group(changes))?;
    Ok(json!({ "created": created }))
}

/// Place Highlight at a surface point: every selected object moves onto the
/// view ray reflected about the normal, at the distance it had from the
/// point before Place Highlight began, and turns its -Z to the point (a
/// targeted light or camera moves its target there instead).
pub fn place_highlight_at(state: &mut AppState, point: Vec3, normal: Vec3, ray: Vec3) -> Result<Value, String> {
    if state.place_highlight_from.is_empty() {
        state.place_highlight_from = state.selection.iter().map(|id| (*id, state.scene.world_matrix(*id).w_axis.truncate())).collect();
    }
    let n = normal.normalize_or_zero();
    let refl = (ray - n * (2.0 * ray.dot(n))).normalize_or_zero();
    if refl == Vec3::ZERO {
        return Err("no surface normal".into());
    }
    let mut changes = Vec::new();
    for (id, from) in state.place_highlight_from.clone() {
        let Some(node) = state.scene.node(id) else { continue };
        let at = point + refl * from.distance(point);
        let world = state.scene.world_matrix(id);
        let (scale, q, _) = world.to_scale_rotation_translation();
        let target = match &node.object {
            ObjectKind::Camera { target, .. } | ObjectKind::Tape { target, .. } => target.filter(|t| state.scene.node(*t).is_some()),
            ObjectKind::Light(l) => l.target.filter(|t| state.scene.node(*t).is_some()),
            _ => None,
        };
        let rot = if target.is_some() { q } else { loom_scene::constraint::aim(point - at, 2, true, Vec3::Z) };
        let local = state.scene.parent_frame(id).inverse() * Mat4::from_scale_rotation_translation(scale, rot, at);
        changes.push(Change::SetTransform { id, before: node.transform.clone(), after: Transform::from_matrix(local) });
        if let Some(t) = target {
            let tn = state.scene.node(t).ok_or_else(|| missing(t))?;
            let (ts, tq, _) = state.scene.world_matrix(t).to_scale_rotation_translation();
            let local = state.scene.parent_frame(t).inverse() * Mat4::from_scale_rotation_translation(ts, tq, point);
            changes.push(Change::SetTransform { id: t, before: tn.transform.clone(), after: Transform::from_matrix(local) });
        }
    }
    commit(state, "Place Highlight", Change::Group(changes))?;
    Ok(json!({}))
}

pub fn execute(state: &mut AppState, cmd: Command) -> Result<Value, String> {
    use Command as C;
    match cmd {
        C::SnapshotDialog => {
            if state.selection.is_empty() {
                return Err("select the object to snapshot first".into());
            }
            state.dialog = Some(Dialog::Snapshot);
            Ok(json!({}))
        }
        C::Snapshot { range, from, to, copies, method } => snapshot(state, range, from, to, copies, method),
        C::CloneAndAlignDialog => {
            state.dialog = Some(Dialog::CloneAndAlign);
            Ok(json!({}))
        }
        C::CloneAndAlignPick => {
            state.pick_mode = if state.pick_mode == Some(PickMode::CloneAlignDestination) { None } else { Some(PickMode::CloneAlignDestination) };
            Ok(json!({ "picking": state.pick_mode.is_some() }))
        }
        C::CloneAndAlignSetDestinations { ids } => {
            let ids: Vec<NodeId> = ids.into_iter().map(NodeId).filter(|id| state.scene.node(*id).is_some() && !state.selection.contains(id)).collect();
            state.clone_align_destinations = ids;
            Ok(json!({ "destinations": state.clone_align_destinations.len() }))
        }
        C::CloneAndAlign { destinations, kind, link, position, position_offset, orientation, orientation_offset, match_scale } => {
            let d: Vec<NodeId> = destinations.into_iter().map(NodeId).collect();
            clone_and_align(state, &d, CloneAlign { kind, link, position, position_offset, orientation, orientation_offset, match_scale })
        }
        C::PlaceHighlight => {
            if state.pick_mode == Some(PickMode::PlaceHighlight) {
                state.pick_mode = None;
                state.place_highlight_from.clear();
                return Ok(json!({ "on": false }));
            }
            if state.selection.is_empty() {
                return Err("select the light or object to place first".into());
            }
            state.place_highlight_from = state.selection.iter().map(|id| (*id, state.scene.world_matrix(*id).w_axis.truncate())).collect();
            state.pick_mode = Some(PickMode::PlaceHighlight);
            Ok(json!({ "on": true }))
        }
        C::PlaceHighlightAt { point, normal, ray } => place_highlight_at(state, Vec3::from(point), Vec3::from(normal), Vec3::from(ray)),
        C::ManageSceneStates => {
            state.dialog = Some(Dialog::ManageSceneStates);
            let names: Vec<String> = state.scene.scene_states.iter().map(|s| s.name.clone()).collect();
            Ok(json!({ "dialog": "ManageSceneStates", "states": names }))
        }
        C::SaveSceneState { name, parts } => save_scene_state(state, &name, parts),
        C::RestoreSceneState { name, parts } => restore_scene_state(state, &name, parts),
        C::DeleteSceneState { name } => delete_scene_state(state, &name),
        C::WalkThroughStep { forward, right, up } => walkthrough_step(state, forward, right, up),
        C::WalkThroughAccelerateToggle => {
            state.walk.speed = if state.walk.speed == 1 { 0 } else { 1 };
            Ok(json!({ "speed": state.walk.speed }))
        }
        C::WalkThroughDecelerateToggle => {
            state.walk.speed = if state.walk.speed == -1 { 0 } else { -1 };
            Ok(json!({ "speed": state.walk.speed }))
        }
        C::WalkThroughIncreaseStepSize | C::WalkThroughDecreaseStepSize | C::WalkThroughResetStepSize => {
            state.walk.step = match cmd {
                C::WalkThroughIncreaseStepSize => (state.walk.step * 1.25).min(1.0e4),
                C::WalkThroughDecreaseStepSize => (state.walk.step / 1.25).max(1.0e-4),
                _ => 1.0,
            };
            Ok(json!({ "step": state.walk.step }))
        }
        C::WalkThroughLockVerticalToggle => {
            state.walk.lock_vertical = !state.walk.lock_vertical;
            Ok(json!({ "lock_vertical": state.walk.lock_vertical }))
        }
        C::WalkThroughLevel => {
            state.record_view_as("Walk Through");
            let cam = &mut state.active_mut().camera;
            let eye = cam.eye_position();
            cam.pitch = 0.0;
            cam.roll = 0.0;
            let (_, _, to_eye) = cam.basis();
            cam.target = eye - to_eye * cam.distance;
            Ok(json!({}))
        }
        C::ChannelInfo => channel_info(state),
        C::ChannelInfoClear { id, channel } => channel_info_clear(state, NodeId(id), channel),
        C::ChannelInfoCopy { id, channel } => channel_info_copy(state, NodeId(id), channel),
        C::ChannelInfoPaste { id, channel } => channel_info_paste(state, NodeId(id), channel),
        C::ChannelInfoAdd { id } => channel_info_add(state, NodeId(id)),
        C::UvwRemove { mode, set_gray } => uvw_remove(state, &mode, set_gray),
        C::SetJointLimits { id, sliding, axis, active, limited, ease, from, to, preferred, spring_back, spring_back_val, spring_tension, damping } => {
            set_joint_limits(state, NodeId(id), sliding, axis, active, limited, ease, from, to, preferred, spring_back, spring_back_val, spring_tension, damping)
        }
        C::ParameterWiringDialog => {
            state.dialog = Some(Dialog::ParameterWiring);
            Ok(json!({ "dialog": "ParameterWiring", "wires": state.scene.parameter_wires.len() }))
        }
        C::WireParameter { source_node, source_param, target_node, target_param, two_way, scale, offset, expression } => {
            wire_parameter(state, NodeId(source_node), source_param, NodeId(target_node), target_param, two_way.unwrap_or(false), scale.unwrap_or(1.0), offset.unwrap_or(0.0), expression.unwrap_or_default())
        }
        C::DisconnectWireParameter { index } => disconnect_wire_parameter(state, index as usize),
        C::AddCustomAttributesDialog => {
            state.dialog = Some(Dialog::AddCustomAttributes);
            Ok(json!({ "dialog": "AddCustomAttributes" }))
        }
        C::ParameterCollector => {
            state.dialog = Some(Dialog::ParameterCollector);
            Ok(json!({ "dialog": "ParameterCollector" }))
        }
        C::AddCustomAttribute { id, name, attr_type, value, min, max } => {
            add_custom_attribute(state, id.map(NodeId), name, attr_type.unwrap_or_else(|| "float".into()), value.unwrap_or(0.0), min.unwrap_or(-100.0), max.unwrap_or(100.0))
        }
        C::SetCustomAttribute { id, name, value } => set_custom_attribute(state, NodeId(id), &name, value),
        C::DeleteCustomAttribute { id, name } => delete_custom_attribute(state, NodeId(id), &name),
        C::CreateContainer { name, path } => create_container(state, name, path),
        C::SaveContainer { id, path } => save_container(state, id.map(NodeId), path),
        C::UnloadContainer { id } => unload_container(state, id.map(NodeId)),
        C::ReloadContainer { id } => reload_container(state, id.map(NodeId)),
        C::SetContainerOpen { id, open } => set_container_open(state, id.map(NodeId), open),
        _ => Err("not a transform tool command".into()),
    }
}

/// The 9 scene state part names.
pub const SCENE_STATE_PARTS: [&str; 9] = [
    "Light Properties",
    "Light Transforms",
    "Object Properties",
    "Camera Transforms",
    "Camera Properties",
    "Layer Properties",
    "Layer Assignment",
    "Materials",
    "Environment",
];

fn save_scene_state(state: &mut AppState, name: &str, parts: Option<Vec<String>>) -> Result<Value, String> {
    let name = name.trim().to_owned();
    if name.is_empty() {
        return Err("a scene state needs a name".into());
    }
    let parts = parts.unwrap_or_else(|| SCENE_STATE_PARTS.iter().map(|s| (*s).to_owned()).collect());
    let nodes = state
        .scene
        .nodes()
        .map(|n| (n.id, n.transform.clone(), n.object.clone(), n.props(), (n.hidden, n.frozen), n.layer, n.material))
        .collect();
    let rec = loom_scene::SceneStateRecord {
        name: name.clone(),
        parts: parts.clone(),
        nodes,
        layers: state.scene.layers.clone(),
        current_layer: state.scene.current_layer,
        materials: (state.scene.materials.clone(), state.scene.material_editor.clone()),
        environment: state.scene.environment.clone(),
    };
    let before = state.scene.scene_states.clone();
    let mut after = before.clone();
    if let Some(existing) = after.iter_mut().find(|r| r.name == name) {
        *existing = rec;
    } else {
        after.push(rec);
    }
    commit(state, "Save Scene State", Change::SetSceneStates { before, after })?;
    Ok(json!({ "saved": name, "parts": parts }))
}

fn restore_scene_state(state: &mut AppState, name: &str, parts: Option<Vec<String>>) -> Result<Value, String> {
    let rec = state.scene.scene_states.iter().find(|r| r.name == name).ok_or_else(|| format!("no scene state {name:?}"))?.clone();
    let active: Vec<String> = match parts {
        Some(p) => p.into_iter().filter(|x| rec.parts.contains(x)).collect(),
        None => rec.parts.clone(),
    };
    let has = |p: &str| active.iter().any(|a| a.eq_ignore_ascii_case(p));
    let mut changes = Vec::new();
    if has("Environment") && state.scene.environment != rec.environment {
        changes.push(Change::SetEnvironment { before: Box::new(state.scene.environment.clone()), after: Box::new(rec.environment.clone()) });
    }
    if has("Materials") && (state.scene.materials != rec.materials.0 || state.scene.material_editor != rec.materials.1) {
        changes.push(Change::SetMaterials {
            before: Box::new((state.scene.materials.clone(), state.scene.material_editor.clone())),
            after: Box::new(rec.materials.clone()),
        });
    }
    if has("Layer Properties") && (state.scene.layers != rec.layers || state.scene.current_layer != rec.current_layer) {
        changes.push(Change::SetLayers {
            before: (state.scene.layers.clone(), state.scene.current_layer),
            after: (rec.layers.clone(), rec.current_layer),
        });
    }
    for (id, tf, obj, props, flags, layer, mat) in &rec.nodes {
        let Some(cur) = state.scene.node(*id) else { continue };
        let is_light = matches!(cur.object, ObjectKind::Light(_));
        let is_camera = matches!(cur.object, ObjectKind::Camera { .. });
        if ((is_light && has("Light Transforms")) || (is_camera && has("Camera Transforms")) || (!is_light && !is_camera && has("Object Properties")))
            && cur.transform != *tf {
                changes.push(Change::SetTransform { id: *id, before: cur.transform.clone(), after: tf.clone() });
            }
        if ((is_light && has("Light Properties")) || (is_camera && has("Camera Properties")))
            && cur.object != *obj {
                changes.push(Change::ReplaceObject { id: *id, before: Box::new(cur.object.clone()), after: Box::new(obj.clone()) });
            }
        if has("Object Properties") {
            if (cur.hidden, cur.frozen) != *flags {
                changes.push(Change::SetFlags { id: *id, before: (cur.hidden, cur.frozen), after: *flags });
            }
            let cur_props = cur.props();
            if cur_props != *props {
                changes.push(Change::SetNodeProps { id: *id, before: Box::new(cur_props), after: Box::new(props.clone()) });
            }
        }
        if has("Layer Assignment") && cur.layer != *layer {
            changes.push(Change::SetLayer { id: *id, before: cur.layer, after: *layer });
        }
        if has("Materials") && cur.material != *mat {
            changes.push(Change::SetMaterial { id: *id, before: cur.material, after: *mat });
        }
    }
    if !changes.is_empty() {
        commit(state, "Restore Scene State", Change::Group(changes))?;
    }
    Ok(json!({ "restored": name, "parts": active }))
}

fn delete_scene_state(state: &mut AppState, name: &str) -> Result<Value, String> {
    let before = state.scene.scene_states.clone();
    let after: Vec<_> = before.iter().filter(|r| r.name != name).cloned().collect();
    if after.len() == before.len() {
        return Err(format!("no scene state {name:?}"));
    }
    commit(state, "Delete Scene State", Change::SetSceneStates { before, after })?;
    Ok(json!({ "deleted": name }))
}

fn walkthrough_step(state: &mut AppState, forward: f32, right: f32, up: f32) -> Result<Value, String> {
    state.record_view_as("Walk Through");
    let cam = &mut state.active_mut().camera;
    let (r, _, to_eye) = cam.basis();
    let fwd_horiz = Vec3::new(-to_eye.x, -to_eye.y, 0.0).normalize_or_zero();
    let fwd = if fwd_horiz != Vec3::ZERO { fwd_horiz } else { -to_eye };
    let delta = fwd * forward + r * right + Vec3::Z * up;
    cam.target += delta;
    let eye = cam.eye_position();
    Ok(json!({ "eye": [eye.x, eye.y, eye.z], "target": [cam.target.x, cam.target.y, cam.target.z] }))
}

fn channel_info(state: &AppState) -> Result<Value, String> {
    let mut rows = Vec::new();
    for id in &state.selection {
        let Some(node) = state.scene.node(*id) else { continue };
        let eval = state.scene.eval_mesh(*id);
        let mesh = &eval.value.mesh;
        rows.push(json!({
            "id": id.0,
            "object": node.name,
            "channel_id": "poly",
            "channel": 0,
            "name": "mesh",
            "num_verts": mesh.vertex_count(),
            "num_faces": mesh.face_count(),
        }));
        rows.push(json!({
            "id": id.0,
            "object": node.name,
            "channel_id": "0:vc",
            "channel": 0,
            "name": "Vertex Color",
            "num_verts": mesh.vertex_count(),
            "num_faces": mesh.face_count(),
        }));
        for (&ch, map) in mesh.texture_maps() {
            rows.push(json!({
                "id": id.0,
                "object": node.name,
                "channel_id": format!("{ch}:map"),
                "channel": ch,
                "name": format!("Map Channel {ch}"),
                "num_verts": map.verts.len(),
                "num_faces": mesh.face_count(),
            }));
        }
    }
    Ok(json!({ "channels": rows }))
}

fn edit_node_mesh(state: &mut AppState, id: NodeId, label: &str, f: impl FnOnce(&mut loom_mesh::Mesh) -> Result<(), String>) -> Result<(), String> {
    let node = state.scene.node(id).ok_or_else(|| missing(id))?;
    let mut mesh = match &node.object {
        ObjectKind::EditablePoly(p) | ObjectKind::EditableMesh(p) => p.mesh.clone(),
        _ => state.scene.eval_mesh(id).value.mesh.clone(),
    };
    f(&mut mesh)?;
    let after = match &node.object {
        ObjectKind::EditableMesh(p) => {
            let mut p = p.clone();
            p.mesh = mesh;
            ObjectKind::EditableMesh(p)
        }
        ObjectKind::EditablePoly(p) => {
            let mut p = p.clone();
            p.mesh = mesh;
            ObjectKind::EditablePoly(p)
        }
        _ => ObjectKind::EditablePoly(PolyObject::from_mesh(mesh)),
    };
    commit(state, label, Change::ReplaceObject { id, before: Box::new(node.object.clone()), after: Box::new(after) })
}

fn channel_info_clear(state: &mut AppState, id: NodeId, channel: u32) -> Result<Value, String> {
    edit_node_mesh(state, id, "Channel Info Clear", |m| {
        m.remove_map(channel);
        Ok(())
    })?;
    Ok(json!({ "id": id.0, "cleared": channel }))
}

fn channel_info_copy(state: &mut AppState, id: NodeId, channel: u32) -> Result<Value, String> {
    let eval = state.scene.eval_mesh(id);
    let map = eval.value.mesh.map(channel).ok_or_else(|| format!("object {} has no map channel {channel}", id.0))?.clone();
    state.channel_clipboard = Some(map);
    Ok(json!({ "copied": channel }))
}

fn channel_info_paste(state: &mut AppState, id: NodeId, channel: u32) -> Result<Value, String> {
    let clip = state.channel_clipboard.clone().ok_or("channel clipboard is empty")?;
    edit_node_mesh(state, id, "Channel Info Paste", |m| {
        let mut map = clip;
        map.corners.resize(m.half_edge_count(), 0);
        m.set_map(channel, map);
        Ok(())
    })?;
    Ok(json!({ "id": id.0, "pasted": channel }))
}

fn channel_info_add(state: &mut AppState, id: NodeId) -> Result<Value, String> {
    let mut added = 1u32;
    edit_node_mesh(state, id, "Channel Info Add", |m| {
        while m.map(added).is_some() && added < 99 {
            added += 1;
        }
        m.ensure_map(added);
        Ok(())
    })?;
    Ok(json!({ "id": id.0, "added": added }))
}

fn uvw_remove(state: &mut AppState, mode: &str, set_gray: bool) -> Result<Value, String> {
    let ids: Vec<NodeId> = state.selection.iter().copied().collect();
    if ids.is_empty() {
        return Err("select objects first".into());
    }
    let mut changes = Vec::new();
    match mode.to_ascii_lowercase().as_str() {
        "uvw" => {
            for id in &ids {
                let Some(n) = state.scene.node(*id) else { continue };
                let after_stack: Vec<_> = n
                    .modifiers
                    .iter()
                    .filter(|s| !matches!(s.modifier, loom_scene::Modifier::UnwrapUVW(_) | loom_scene::Modifier::UVWMap { .. } | loom_scene::Modifier::UVWXForm { .. } | loom_scene::Modifier::UvwMappingAdd | loom_scene::Modifier::UvwMappingClear { .. } | loom_scene::Modifier::MapScaler { .. }))
                    .cloned()
                    .collect();
                if after_stack.len() != n.modifiers.len() {
                    changes.push(Change::SetStack { id: *id, before: n.modifiers.clone(), after: after_stack });
                }
                if let ObjectKind::EditablePoly(p) | ObjectKind::EditableMesh(p) = &n.object {
                    let mut p2 = p.clone();
                    let extra: Vec<u32> = p2.mesh.texture_maps().map(|(&c, _)| c).collect();
                    if !extra.is_empty() {
                        for c in extra {
                            p2.mesh.remove_map(c);
                        }
                        let after_obj = match &n.object {
                            ObjectKind::EditableMesh(_) => ObjectKind::EditableMesh(p2),
                            _ => ObjectKind::EditablePoly(p2),
                        };
                        changes.push(Change::ReplaceObject { id: *id, before: Box::new(n.object.clone()), after: Box::new(after_obj) });
                    }
                }
            }
        }
        "materials" => {
            for id in &ids {
                let Some(n) = state.scene.node(*id) else { continue };
                if n.material.is_some() {
                    changes.push(Change::SetMaterial { id: *id, before: n.material, after: None });
                }
                if set_gray && n.wire_color != [128, 128, 128] {
                    changes.push(Change::SetWireColor { id: *id, before: n.wire_color, after: [128, 128, 128] });
                }
            }
        }
        other => return Err(format!("unknown UVW Remove mode {other:?} (\"uvw\" or \"materials\")")),
    }
    if !changes.is_empty() {
        commit(state, "UVW Remove", Change::Group(changes))?;
    }
    Ok(json!({ "mode": mode, "objects": ids.len() }))
}

/// Clone and Align > Pick: a clicked object joins the destinations (not a
/// source, not twice); Pick stays on.
pub fn pick_destination(state: &mut AppState, id: NodeId) -> Result<Value, String> {
    if state.selection.contains(&id) {
        return Err("a source object cannot be a destination".into());
    }
    if !state.clone_align_destinations.contains(&id) {
        state.clone_align_destinations.push(id);
    }
    Ok(json!({ "destinations": state.clone_align_destinations.len() }))
}

#[allow(clippy::too_many_arguments)]
fn set_joint_limits(
    state: &mut AppState,
    id: NodeId,
    sliding: bool,
    axis: u32,
    active: Option<bool>,
    limited: Option<bool>,
    ease: Option<bool>,
    from: Option<f32>,
    to: Option<f32>,
    preferred: Option<f32>,
    spring_back: Option<bool>,
    spring_back_val: Option<f32>,
    spring_tension: Option<f32>,
    damping: Option<f32>,
) -> Result<Value, String> {
    let idx = (axis as usize).min(2);
    let node = state.scene.node(id).ok_or_else(|| missing(id))?;
    let before = node.props();
    let mut after = before.clone();
    let joint = if sliding { &mut after.joints.sliding[idx] } else { &mut after.joints.rotational[idx] };
    if let Some(v) = active { joint.active = v; }
    if let Some(v) = limited { joint.limited = v; }
    if let Some(v) = ease { joint.ease = v; }
    if let Some(v) = from { joint.from = v; }
    if let Some(v) = to { joint.to = v; }
    if let Some(v) = preferred { joint.preferred = v; }
    if let Some(v) = spring_back { joint.spring_back = v; }
    if let Some(v) = spring_back_val { joint.spring_back_val = v; }
    if let Some(v) = spring_tension { joint.spring_tension = v.clamp(0.0, 1.0); }
    if let Some(v) = damping { joint.damping = v.clamp(0.0, 1.0); }
    let snap = *joint;
    commit(state, "Joint Limits", Change::SetNodeProps { id, before: Box::new(before), after: Box::new(after) })?;
    Ok(json!({ "id": id.0, "sliding": sliding, "axis": idx, "joint": snap }))
}

/// Evaluates a simple wiring expression (e.g. `""`, `"x"`, `"-x"`, `"x * 2"`, `"degtorad(x)"`, `"radtodeg(x)"`)
/// on top of `scale * x + offset`.
pub fn eval_wire_value(x: f32, scale: f32, offset: f32, expr: &str) -> f32 {
    let base = x * scale + offset;
    let e = expr.trim();
    if e.is_empty() || e.eq_ignore_ascii_case("x") {
        return base;
    }
    if e == "-x" {
        return -base;
    }
    if e.eq_ignore_ascii_case("degtorad(x)") {
        return base.to_radians();
    }
    if e.eq_ignore_ascii_case("radtodeg(x)") {
        return base.to_degrees();
    }
    if let Some(rest) = e.strip_prefix("x*").or_else(|| e.strip_prefix("x *")) {
        if let Ok(k) = rest.trim().parse::<f32>() {
            return base * k;
        }
    }
    if let Some(rest) = e.strip_prefix("x/").or_else(|| e.strip_prefix("x /")) {
        if let Ok(k) = rest.trim().parse::<f32>() {
            if k.abs() > 1e-6 {
                return base / k;
            }
        }
    }
    if let Some(rest) = e.strip_prefix("x+").or_else(|| e.strip_prefix("x +")) {
        if let Ok(k) = rest.trim().parse::<f32>() {
            return base + k;
        }
    }
    if let Some(rest) = e.strip_prefix("x-").or_else(|| e.strip_prefix("x -")) {
        if let Ok(k) = rest.trim().parse::<f32>() {
            return base - k;
        }
    }
    base
}

pub fn read_node_wire_param(scene: &loom_scene::Scene, id: NodeId, param: &str) -> Option<f32> {
    let n = scene.node(id)?;
    match param {
        "Position X" | "Position.X" => Some(n.transform.position.get().x),
        "Position Y" | "Position.Y" => Some(n.transform.position.get().y),
        "Position Z" | "Position.Z" => Some(n.transform.position.get().z),
        "Rotation X" | "Rotation.X" => Some(n.transform.rotation.get().x),
        "Rotation Y" | "Rotation.Y" => Some(n.transform.rotation.get().y),
        "Rotation Z" | "Rotation.Z" => Some(n.transform.rotation.get().z),
        "Scale X" | "Scale.X" => Some(n.transform.scale.get().x),
        "Scale Y" | "Scale.Y" => Some(n.transform.scale.get().y),
        "Scale Z" | "Scale.Z" => Some(n.transform.scale.get().z),
        _ => {
            if let Some(ca) = n.custom_attributes.iter().find(|a| a.name.eq_ignore_ascii_case(param)) {
                return Some(ca.value);
            }
            n.object.parameters().into_iter().find(|(k, _)| k.eq_ignore_ascii_case(param)).map(|(_, v)| v.as_f64() as f32)
        }
    }
}

pub fn write_node_wire_change(scene: &loom_scene::Scene, id: NodeId, param: &str, val: f32) -> Option<Change> {
    let n = scene.node(id)?;
    match param {
        "Position X" | "Position.X" | "Position Y" | "Position.Y" | "Position Z" | "Position.Z" => {
            let mut after = n.transform.clone();
            let mut p = after.position.get();
            if param.ends_with('X') { p.x = val; } else if param.ends_with('Y') { p.y = val; } else { p.z = val; }
            if (p - n.transform.position.get()).length_squared() < 1e-10 { return None; }
            after.position.set(p);
            Some(Change::SetTransform { id, before: n.transform.clone(), after })
        }
        "Rotation X" | "Rotation.X" | "Rotation Y" | "Rotation.Y" | "Rotation Z" | "Rotation.Z" => {
            let mut after = n.transform.clone();
            let mut r = after.rotation.get();
            if param.ends_with('X') { r.x = val; } else if param.ends_with('Y') { r.y = val; } else { r.z = val; }
            if (r - n.transform.rotation.get()).length_squared() < 1e-10 { return None; }
            after.rotation.set(r);
            Some(Change::SetTransform { id, before: n.transform.clone(), after })
        }
        "Scale X" | "Scale.X" | "Scale Y" | "Scale.Y" | "Scale Z" | "Scale.Z" => {
            let mut after = n.transform.clone();
            let mut s = after.scale.get();
            if param.ends_with('X') { s.x = val; } else if param.ends_with('Y') { s.y = val; } else { s.z = val; }
            if (s - n.transform.scale.get()).length_squared() < 1e-10 { return None; }
            after.scale.set(s);
            Some(Change::SetTransform { id, before: n.transform.clone(), after })
        }
        _ => {
            if n.custom_attributes.iter().any(|a| a.name.eq_ignore_ascii_case(param)) {
                let before = n.props();
                let mut after = before.clone();
                if let Some(ca) = after.custom_attributes.iter_mut().find(|a| a.name.eq_ignore_ascii_case(param)) {
                    let clamped = val.clamp(ca.min.min(ca.max), ca.min.max(ca.max));
                    if (ca.value - clamped).abs() < 1e-6 { return None; }
                    ca.value = clamped;
                }
                return Some(Change::SetNodeProps { id, before: Box::new(before), after: Box::new(after) });
            }
            let mut obj = n.object.clone();
            if obj.set_parameter(param, val as f64).is_ok() && obj != n.object {
                return Some(Change::ReplaceObject { id, before: Box::new(n.object.clone()), after: Box::new(obj) });
            }
            None
        }
    }
}

/// Propagates all active Parameter Wiring links across `state.scene`.
pub fn sync_parameter_wires(state: &mut AppState) {
    if state.scene.parameter_wires.is_empty() {
        return;
    }
    let wires = state.scene.parameter_wires.clone();
    for w in wires {
        let Some(src) = read_node_wire_param(&state.scene, w.source_node, &w.source_param) else { continue };
        let dst_val = eval_wire_value(src, w.scale, w.offset, &w.expression);
        if let Some(ch) = write_node_wire_change(&state.scene, w.target_node, &w.target_param, dst_val) {
            let _ = ch.apply(&mut state.scene);
            state.mesh_dirty = true;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn wire_parameter(
    state: &mut AppState,
    source_node: NodeId,
    source_param: String,
    target_node: NodeId,
    target_param: String,
    two_way: bool,
    scale: f32,
    offset: f32,
    expression: String,
) -> Result<Value, String> {
    if state.scene.node(source_node).is_none() {
        return Err(missing(source_node));
    }
    if state.scene.node(target_node).is_none() {
        return Err(missing(target_node));
    }
    let src_val = read_node_wire_param(&state.scene, source_node, &source_param)
        .ok_or_else(|| format!("node {} has no wireable parameter {source_param:?}", source_node.0))?;
    if read_node_wire_param(&state.scene, target_node, &target_param).is_none() {
        return Err(format!("node {} has no wireable parameter {target_param:?}", target_node.0));
    }
    let wire = loom_scene::ParameterWire {
        source_node,
        source_param: source_param.clone(),
        target_node,
        target_param: target_param.clone(),
        two_way,
        scale,
        offset,
        expression: expression.clone(),
    };
    let before = state.scene.parameter_wires.clone();
    let mut after = before.clone();
    if let Some(existing) = after.iter_mut().find(|w| w.source_node == source_node && w.source_param == source_param && w.target_node == target_node && w.target_param == target_param) {
        *existing = wire;
    } else {
        after.push(wire);
    }
    let mut changes = vec![Change::SetParameterWires { before, after }];
    let dst_val = eval_wire_value(src_val, scale, offset, &expression);
    if let Some(ch) = write_node_wire_change(&state.scene, target_node, &target_param, dst_val) {
        changes.push(ch);
    }
    commit(state, "Wire Parameter", Change::Group(changes))?;
    Ok(json!({ "wires": state.scene.parameter_wires.len(), "value": dst_val }))
}

fn disconnect_wire_parameter(state: &mut AppState, index: usize) -> Result<Value, String> {
    if index >= state.scene.parameter_wires.len() {
        return Err(format!("no parameter wire {index}"));
    }
    let before = state.scene.parameter_wires.clone();
    let mut after = before.clone();
    after.remove(index);
    commit(state, "Disconnect Wire Parameter", Change::SetParameterWires { before, after })?;
    Ok(json!({ "wires": state.scene.parameter_wires.len() }))
}

fn add_custom_attribute(
    state: &mut AppState,
    id: Option<NodeId>,
    name: String,
    attr_type: String,
    value: f32,
    min: f32,
    max: f32,
) -> Result<Value, String> {
    let name = name.trim().to_owned();
    if name.is_empty() {
        return Err("custom attribute needs a name".into());
    }
    let ids: Vec<NodeId> = match id {
        Some(n) => vec![n],
        None => state.selection.iter().copied().collect(),
    };
    if ids.is_empty() {
        return Err("select an object to add a custom attribute".into());
    }
    let mut changes = Vec::new();
    for nid in &ids {
        let node = state.scene.node(*nid).ok_or_else(|| missing(*nid))?;
        let before = node.props();
        let mut after = before.clone();
        let attr = loom_scene::CustomAttribute {
            name: name.clone(),
            attr_type: attr_type.to_ascii_lowercase(),
            value: value.clamp(min.min(max), min.max(max)),
            min: min.min(max),
            max: min.max(max),
        };
        if let Some(existing) = after.custom_attributes.iter_mut().find(|a| a.name.eq_ignore_ascii_case(&name)) {
            *existing = attr;
        } else {
            after.custom_attributes.push(attr);
        }
        changes.push(Change::SetNodeProps { id: *nid, before: Box::new(before), after: Box::new(after) });
    }
    commit(state, "Add Custom Attribute", Change::Group(changes))?;
    Ok(json!({ "added": name, "nodes": ids.len() }))
}

fn set_custom_attribute(state: &mut AppState, id: NodeId, name: &str, value: f32) -> Result<Value, String> {
    let node = state.scene.node(id).ok_or_else(|| missing(id))?;
    let before = node.props();
    let mut after = before.clone();
    let attr = after
        .custom_attributes
        .iter_mut()
        .find(|a| a.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| format!("{} has no custom attribute {name:?}", node.name))?;
    attr.value = match attr.attr_type.as_str() {
        "boolean" | "bool" => if value != 0.0 { 1.0 } else { 0.0 },
        "integer" | "int" => value.round().clamp(attr.min, attr.max),
        _ => value.clamp(attr.min, attr.max),
    };
    let final_val = attr.value;
    commit(state, "Set Custom Attribute", Change::SetNodeProps { id, before: Box::new(before), after: Box::new(after) })?;
    sync_parameter_wires(state);
    Ok(json!({ "id": id.0, "name": name, "value": final_val }))
}

fn delete_custom_attribute(state: &mut AppState, id: NodeId, name: &str) -> Result<Value, String> {
    let node = state.scene.node(id).ok_or_else(|| missing(id))?;
    let before = node.props();
    let mut after = before.clone();
    let len0 = after.custom_attributes.len();
    after.custom_attributes.retain(|a| !a.name.eq_ignore_ascii_case(name));
    if after.custom_attributes.len() == len0 {
        return Err(format!("{} has no custom attribute {name:?}", node.name));
    }
    commit(state, "Delete Custom Attribute", Change::SetNodeProps { id, before: Box::new(before), after: Box::new(after) })?;
    Ok(json!({ "id": id.0, "deleted": name }))
}

fn resolve_container_id(state: &AppState, id: Option<NodeId>) -> Result<NodeId, String> {
    if let Some(cid) = id {
        if state.scene.node(cid).is_some_and(|n| n.container.is_some()) {
            return Ok(cid);
        }
        return Err(format!("node {} is not a Container", cid.0));
    }
    for &sel in &state.selection {
        if state.scene.node(sel).is_some_and(|n| n.container.is_some()) {
            return Ok(sel);
        }
        for anc in state.scene.ancestors(sel) {
            if state.scene.node(anc).is_some_and(|n| n.container.is_some()) {
                return Ok(anc);
            }
        }
    }
    Err("select a Container".into())
}

fn create_container(state: &mut AppState, name: Option<String>, path: Option<String>) -> Result<Value, String> {
    let members: Vec<NodeId> = state.selection.iter().copied().collect();
    let center = if members.is_empty() {
        Vec3::ZERO
    } else {
        let sum: Vec3 = members
            .iter()
            .filter_map(|id| state.scene.node(*id))
            .map(|n| state.scene.world_matrix(n.id).transform_point3(Vec3::ZERO))
            .sum();
        sum / (members.len() as f32)
    };
    let cname = name.unwrap_or_else(|| state.scene.next_name("Container"));
    let mut node = Node::new(&cname, ObjectKind::dummy(20.0));
    let cid = NodeId(state.scene.next_id());
    node.id = cid;
    node.wire_color = [90, 180, 240];
    node.transform.position.set(center);
    node.container = Some(loom_scene::ContainerInfo {
        path: path.unwrap_or_default(),
        open: true,
        unloaded: false,
        saved_children: Vec::new(),
    });
    let mut changes = vec![Change::AddNode(node)];
    for &m in &members {
        let Some(mn) = state.scene.node(m) else { continue };
        let world = state.scene.world_matrix(m);
        let local = Mat4::from_translation(-center) * world;
        let after_tf = loom_scene::Transform::from_matrix_near(local, &mn.transform, Vec3::ZERO);
        changes.push(Change::SetParent { id: m, before: mn.parent, after: Some(cid) });
        changes.push(Change::SetTransform { id: m, before: mn.transform.clone(), after: after_tf });
    }
    commit(state, "Create Container", Change::Group(changes))?;
    state.selection = std::collections::BTreeSet::from([cid]);
    state.sub_selection.clear();
    Ok(json!({ "container": cid.0, "name": cname, "members": members.len() }))
}

fn save_container(state: &mut AppState, id: Option<NodeId>, path: Option<String>) -> Result<Value, String> {
    let cid = resolve_container_id(state, id)?;
    let desc: Vec<Node> = state
        .scene
        .descendants(cid)
        .into_iter()
        .filter_map(|d| state.scene.node(d).cloned())
        .collect();
    let node = state.scene.node(cid).ok_or_else(|| missing(cid))?;
    let before = node.props();
    let mut after = before.clone();
    let info = after.container.as_mut().ok_or("not a Container")?;
    if let Some(p) = path {
        info.path = p;
    } else if info.path.is_empty() {
        info.path = crate::autosave::loom_home()
            .join("containers")
            .join(format!("{}.loomc", node.name))
            .to_string_lossy()
            .into_owned();
    }
    info.saved_children = desc.clone();
    let out_path = info.path.clone();
    if let Some(parent_dir) = std::path::Path::new(&out_path).parent() {
        let _ = std::fs::create_dir_all(parent_dir);
    }
    let json_bytes = serde_json::to_vec_pretty(&desc).map_err(|e| e.to_string())?;
    std::fs::write(&out_path, json_bytes).map_err(|e| format!("{out_path}: {e}"))?;
    commit(state, "Save Container", Change::SetNodeProps { id: cid, before: Box::new(before), after: Box::new(after) })?;
    Ok(json!({ "container": cid.0, "path": out_path, "saved_nodes": desc.len() }))
}

fn unload_container(state: &mut AppState, id: Option<NodeId>) -> Result<Value, String> {
    let cid = resolve_container_id(state, id)?;
    let desc_ids = state.scene.descendants(cid);
    let desc: Vec<Node> = desc_ids.iter().filter_map(|d| state.scene.node(*d).cloned()).collect();
    let node = state.scene.node(cid).ok_or_else(|| missing(cid))?;
    let before = node.props();
    let mut after = before.clone();
    let info = after.container.as_mut().ok_or("not a Container")?;
    if info.unloaded {
        return Ok(json!({ "container": cid.0, "unloaded": true }));
    }
    info.saved_children = desc;
    info.unloaded = true;
    info.open = false;
    let mut changes = vec![Change::SetNodeProps { id: cid, before: Box::new(before), after: Box::new(after) }];
    for d in desc_ids.into_iter().rev() {
        changes.push(Change::RemoveNode(d));
    }
    commit(state, "Unload Container", Change::Group(changes))?;
    Ok(json!({ "container": cid.0, "unloaded": true }))
}

fn reload_container(state: &mut AppState, id: Option<NodeId>) -> Result<Value, String> {
    let cid = resolve_container_id(state, id)?;
    let node = state.scene.node(cid).ok_or_else(|| missing(cid))?;
    let before = node.props();
    let mut after = before.clone();
    let info = after.container.as_mut().ok_or("not a Container")?;
    let mut children = info.saved_children.clone();
    if children.is_empty() && !info.path.is_empty() {
        if let Ok(bytes) = std::fs::read(&info.path) {
            if let Ok(loaded) = serde_json::from_slice::<Vec<Node>>(&bytes) {
                children = loaded;
            }
        }
    }
    info.unloaded = false;
    let count = children.len();
    let mut changes = vec![Change::SetNodeProps { id: cid, before: Box::new(before), after: Box::new(after) }];
    for ch_node in children {
        if state.scene.node(ch_node.id).is_none() {
            changes.push(Change::AddNode(ch_node));
        }
    }
    commit(state, "Reload Container", Change::Group(changes))?;
    Ok(json!({ "container": cid.0, "reloaded_nodes": count }))
}

fn set_container_open(state: &mut AppState, id: Option<NodeId>, open: bool) -> Result<Value, String> {
    let cid = resolve_container_id(state, id)?;
    let node = state.scene.node(cid).ok_or_else(|| missing(cid))?;
    let before = node.props();
    let mut after = before.clone();
    let info = after.container.as_mut().ok_or("not a Container")?;
    info.open = open;
    commit(state, if open { "Open Container" } else { "Close Container" }, Change::SetNodeProps { id: cid, before: Box::new(before), after: Box::new(after) })?;
    Ok(json!({ "container": cid.0, "open": open }))
}
