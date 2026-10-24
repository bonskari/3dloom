//! Assign Controller, List controllers and the transform constraints.

use crate::anim_cmds::edit_nodes;
use crate::command::Command;
use crate::execute::commit_exact as commit;
use crate::state::AppState;
use glam::{Quat, Vec3};
use loom_scene::constraint::{Constraints, LinkConstraint, PathConstraint, PositionConstraint, RotationConstraint, Target};
use loom_scene::tracks::{self, convert_controller, Role, TrackPath};
use loom_scene::{Change, NodeId, Param};
use serde_json::{json, Value};

pub fn handles(c: &Command) -> bool {
    use Command as C;
    matches!(
        c,
        C::AssignController { .. }
            | C::ListAddController { .. }
            | C::ListSetWeight { .. }
            | C::ListSetActive { .. }
            | C::ListDeleteController { .. }
            | C::PathConstraint { .. }
            | C::PositionConstraint { .. }
            | C::AttachmentConstraint { .. }
            | C::LookAtConstraint { .. }
            | C::OrientationConstraint { .. }
            | C::LinkConstraint { .. }
            | C::SetConstraintParameter { .. }
            | C::SetConstraintWeight { .. }
            | C::DeleteConstraintTarget { .. }
            | C::DeleteLink { .. }
    )
}

/// A track of node `id` by display name (case-insensitive) or JSON pointer.
pub fn find_track(state: &AppState, id: NodeId, track: &str) -> Result<TrackPath, String> {
    tracks::list_tracks(&state.scene, id)
        .into_iter()
        .find(|t| t.name.eq_ignore_ascii_case(track) || t.pointer == track)
        .ok_or_else(|| format!("node {} has no track \"{track}\"", id.0))
}

fn edit_track(state: &mut AppState, label: &str, id: NodeId, track: &TrackPath, f: impl FnMut(&TrackPath, &mut tracks::AnyParam)) -> Result<Value, String> {
    let t = track.clone();
    edit_nodes(state, label, &[id], move |x| *x == t, f)
}

fn set_constraints(state: &mut AppState, label: &str, id: NodeId, f: impl FnOnce(&mut Constraints, &AppState) -> Result<(), String>) -> Result<Value, String> {
    let n = state.scene.node(id).ok_or_else(|| format!("no node {}", id.0))?;
    let before = n.constraints.clone();
    let mut after = before.clone();
    f(&mut after, state)?;
    if after != before {
        commit(state, label, Change::SetConstraints { id, before: Box::new(before), after: Box::new(after) })?;
    }
    Ok(json!({ "id": id.0 }))
}

fn for_selection(state: &mut AppState, label: &str, target: Option<NodeId>, f: impl Fn(&mut Constraints, NodeId, &AppState) -> Result<(), String>) -> Result<Value, String> {
    if let Some(t) = target {
        state.scene.node(t).ok_or_else(|| format!("no node {}", t.0))?;
    }
    let ids: Vec<NodeId> = state.selection.iter().copied().filter(|i| Some(*i) != target).collect();
    if ids.is_empty() {
        return Err("select the objects to constrain".into());
    }
    let mut changes = Vec::new();
    for id in ids {
        let n = state.scene.node(id).unwrap();
        let mut after = n.constraints.clone();
        f(&mut after, id, state)?;
        if after != n.constraints {
            changes.push(Change::SetConstraints { id, before: Box::new(n.constraints.clone()), after: Box::new(after) });
        }
    }
    let count = changes.len();
    if count > 0 {
        commit(state, label, Change::Group(changes))?;
    }
    Ok(json!({ "changed": count }))
}

/// % Along Path keyed 0 at the segment start and 100 at its end.
fn percent_param(state: &AppState) -> Param<f32> {
    let mut p = Param::new(0.0f32);
    let (s0, s1) = state.frame_range;
    let tangents = state.default_tangent;
    if let Some(k) = p.ensure_keys() {
        k.tracks[0].set_key(s0 as f32, 0.0, tangents);
        k.tracks[0].set_key(s1 as f32, 100.0, tangents);
    }
    p
}

fn bool_of(v: f64) -> bool {
    v != 0.0
}

pub fn execute(state: &mut AppState, cmd: Command) -> Result<Value, String> {
    use Command as C;
    match cmd {
        C::AssignController { id, track, controller } => assign(state, NodeId(id), &track, &controller),
        C::ListAddController { id, track, controller } => {
            let id = NodeId(id);
            let t = find_track(state, id, &track)?;
            let role = Role::of(&t);
            let cur = tracks::read(&state.scene, &t).ok_or("no such track")?;
            if cur.controller_json().get("List").is_none() {
                return Err(format!("{} is not a List controller", t.name));
            }
            // A new entry starts from zero so it adds nothing until edited.
            let zero = tracks::AnyParam::from_json(&json!({ "controller": { "Constant": zero_value(&cur) } }), cur.ty()).ok_or("bad value type")?;
            let c = convert_controller(&zero, role, &controller, id.0 as u32 ^ 0x5eed)?;
            let entry = json!({ "name": controller, "weight": 100.0, "controller": c });
            edit_track(state, "List Add Controller", id, &t, |_, p| {
                let mut c = p.controller_json();
                if let Some(items) = c.get_mut("List").and_then(|l| l.as_array_mut()) {
                    items.push(entry.clone());
                }
                let _ = p.set_controller_json(&c);
            })
        }
        C::ListSetWeight { id, track, index, weight } => list_edit(state, NodeId(id), &track, "List Set Weight", move |items| {
            let it = items.get_mut(index as usize).ok_or("no such list entry")?;
            it["weight"] = json!(weight);
            Ok(())
        }),
        C::ListSetActive { id, track, index } => list_edit(state, NodeId(id), &track, "List Set Active", move |items| {
            if index as usize >= items.len() {
                return Err("no such list entry".into());
            }
            let it = items.remove(index as usize);
            items.insert(0, it);
            Ok(())
        }),
        C::ListDeleteController { id, track, index } => list_edit(state, NodeId(id), &track, "List Delete Controller", move |items| {
            if items.len() <= 1 {
                return Err("a List keeps at least one entry".into());
            }
            if index as usize >= items.len() {
                return Err("no such list entry".into());
            }
            items.remove(index as usize);
            Ok(())
        }),
        C::PathConstraint { target } => {
            let t = NodeId(target);
            if state.scene.node(t).and_then(|n| n.object.shape()).is_none() {
                return Err("the path must be a shape".into());
            }
            let percent = percent_param(state);
            for_selection(state, "Path Constraint", Some(t), move |c, _, _| {
                match &mut c.position {
                    Some(PositionConstraint::Path(p)) => {
                        if !p.targets.iter().any(|x| x.node == t) {
                            p.targets.push(Target::new(t));
                        }
                    }
                    _ => c.position = Some(PositionConstraint::Path(PathConstraint { targets: vec![Target::new(t)], percent: percent.clone(), ..Default::default() })),
                }
                Ok(())
            })
        }
        C::PositionConstraint { target } => {
            let t = NodeId(target);
            for_selection(state, "Position Constraint", Some(t), move |c, _, _| {
                match &mut c.position {
                    Some(PositionConstraint::Position { targets, .. }) => {
                        if !targets.iter().any(|x| x.node == t) {
                            targets.push(Target::new(t));
                        }
                    }
                    _ => c.position = Some(PositionConstraint::Position { targets: vec![Target::new(t)], keep_offset: false, offset: Vec3::ZERO }),
                }
                Ok(())
            })
        }
        C::AttachmentConstraint { target, face, a, b } => {
            let t = NodeId(target);
            let faces = state.scene.eval_mesh(t).value.mesh.face_count() as u32;
            let face = face.unwrap_or(0);
            if face >= faces.max(1) {
                return Err(format!("the target has {faces} faces"));
            }
            for_selection(state, "Attachment Constraint", Some(t), move |c, _, _| {
                c.position = Some(PositionConstraint::Attachment { target: t, face, a: a.unwrap_or(0.0), b: b.unwrap_or(0.0), align: true });
                Ok(())
            })
        }
        C::LookAtConstraint { target } => {
            let t = NodeId(target);
            for_selection(state, "LookAt Constraint", Some(t), move |c, _, _| {
                match &mut c.rotation {
                    Some(RotationConstraint::LookAt { targets, .. }) => {
                        if !targets.iter().any(|x| x.node == t) {
                            targets.push(Target::new(t));
                        }
                    }
                    _ => c.rotation = Some(RotationConstraint::LookAt { targets: vec![Target::new(t)], axis: 0, flip: false, upnode: None, keep_offset: false, offset: Quat::IDENTITY }),
                }
                Ok(())
            })
        }
        C::OrientationConstraint { target } => {
            let t = NodeId(target);
            for_selection(state, "Orientation Constraint", Some(t), move |c, _, _| {
                match &mut c.rotation {
                    Some(RotationConstraint::Orientation { targets, .. }) => {
                        if !targets.iter().any(|x| x.node == t) {
                            targets.push(Target::new(t));
                        }
                    }
                    _ => c.rotation = Some(RotationConstraint::Orientation { targets: vec![Target::new(t)], keep_offset: false, offset: Quat::IDENTITY }),
                }
                Ok(())
            })
        }
        C::LinkConstraint { target } => {
            let t = target.map(NodeId);
            let frame = state.current_frame as f32;
            for_selection(state, "Link Constraint", t, move |c, id, st| {
                if t.is_some_and(|t| t == id || st.scene.is_descendant(t, id)) {
                    return Err("an object cannot link to itself or its children".into());
                }
                // The new link holds the node's current frame in its target's space.
                let offset = st.scene.link_offset(id, t, frame);
                let l = c.link.get_or_insert_with(LinkConstraint::default);
                if l.offsets.len() != l.links.len() {
                    // An older link list without offsets: fill them from the link frames first.
                    l.offsets = l.links.iter().map(|(f, tg)| st.scene.link_offset(id, *tg, *f)).collect();
                }
                let mut pairs: Vec<((f32, Option<NodeId>), glam::Mat4)> = l.links.iter().copied().zip(l.offsets.iter().copied()).filter(|((f, _), _)| (*f - frame).abs() > 1e-4).collect();
                pairs.push(((frame, t), offset));
                pairs.sort_by(|a, b| a.0 .0.total_cmp(&b.0 .0));
                l.links = pairs.iter().map(|p| p.0).collect();
                l.offsets = pairs.iter().map(|p| p.1).collect();
                Ok(())
            })
        }
        C::SetConstraintParameter { id, name, value } => set_parameter(state, NodeId(id), &name, value),
        C::SetConstraintWeight { id, rotation, index, weight } => {
            let id = NodeId(id);
            set_constraints(state, "Set Constraint Weight", id, move |c, _| {
                let targets = targets_mut(c, rotation).ok_or("no constraint with targets")?;
                let t = targets.get_mut(index as usize).ok_or("no such target")?;
                t.weight.set(weight.clamp(0.0, 100.0));
                Ok(())
            })
        }
        C::DeleteConstraintTarget { id, rotation, index } => set_constraints(state, "Delete Constraint Target", NodeId(id), move |c, _| {
            let targets = targets_mut(c, rotation).ok_or("no constraint with targets")?;
            if index as usize >= targets.len() {
                return Err("no such target".into());
            }
            targets.remove(index as usize);
            Ok(())
        }),
        C::DeleteLink { id, index } => set_constraints(state, "Delete Link", NodeId(id), move |c, _| {
            let l = c.link.as_mut().ok_or("no Link Constraint")?;
            if index as usize >= l.links.len() {
                return Err("no such link".into());
            }
            l.links.remove(index as usize);
            if (index as usize) < l.offsets.len() {
                l.offsets.remove(index as usize);
            }
            if l.links.is_empty() {
                c.link = None;
            }
            Ok(())
        }),
        _ => Err("not a controller command".into()),
    }
}

fn zero_value(p: &tracks::AnyParam) -> Value {
    match p.ty() {
        loom_scene::param::ParamType::Vec3 => json!([0.0, 0.0, 0.0]),
        loom_scene::param::ParamType::Bool => json!(false),
        loom_scene::param::ParamType::U32 => json!(0),
        loom_scene::param::ParamType::F32 => json!(0.0),
    }
}

fn targets_mut(c: &mut Constraints, rotation: bool) -> Option<&mut Vec<Target>> {
    if rotation {
        match c.rotation.as_mut()? {
            RotationConstraint::LookAt { targets, .. } | RotationConstraint::Orientation { targets, .. } => Some(targets),
        }
    } else {
        match c.position.as_mut()? {
            PositionConstraint::Path(p) => Some(&mut p.targets),
            PositionConstraint::Position { targets, .. } => Some(targets),
            PositionConstraint::Attachment { .. } => None,
        }
    }
}

fn list_edit(state: &mut AppState, id: NodeId, track: &str, label: &str, f: impl Fn(&mut Vec<Value>) -> Result<(), String>) -> Result<Value, String> {
    let t = find_track(state, id, track)?;
    let cur = tracks::read(&state.scene, &t).ok_or("no such track")?;
    let mut c = cur.controller_json();
    let items = c.get_mut("List").and_then(|l| l.as_array_mut()).ok_or_else(|| format!("{} is not a List controller", t.name))?;
    f(items)?;
    edit_track(state, label, id, &t, |_, p| {
        let _ = p.set_controller_json(&c);
    })
}

fn assign(state: &mut AppState, id: NodeId, track: &str, controller: &str) -> Result<Value, String> {
    let t = find_track(state, id, track)?;
    let role = Role::of(&t);
    if !role.choices().iter().any(|c| c == controller) {
        return Err(format!("{controller} cannot drive {}", t.name));
    }
    // Constraints sit on the node; a plain controller replaces them on that track.
    let constraint = matches!(controller, "Path Constraint" | "Position Constraint" | "Attachment" | "LookAt Constraint" | "Orientation Constraint");
    if constraint {
        let percent = percent_param(state);
        let name = controller.to_owned();
        return set_constraints(state, "Assign Controller", id, move |c, _| {
            match name.as_str() {
                "Path Constraint" if !matches!(c.position, Some(PositionConstraint::Path(_))) => c.position = Some(PositionConstraint::Path(PathConstraint { percent, ..Default::default() })),
                "Position Constraint" if !matches!(c.position, Some(PositionConstraint::Position { .. })) => c.position = Some(PositionConstraint::Position { targets: Vec::new(), keep_offset: false, offset: Vec3::ZERO }),
                "Attachment" => return Err("Attachment needs a target: use Attachment Constraint".into()),
                "LookAt Constraint" if !matches!(c.rotation, Some(RotationConstraint::LookAt { .. })) => c.rotation = Some(RotationConstraint::LookAt { targets: Vec::new(), axis: 0, flip: false, upnode: None, keep_offset: false, offset: Quat::IDENTITY }),
                "Orientation Constraint" if !matches!(c.rotation, Some(RotationConstraint::Orientation { .. })) => c.rotation = Some(RotationConstraint::Orientation { targets: Vec::new(), keep_offset: false, offset: Quat::IDENTITY }),
                _ => {}
            }
            Ok(())
        });
    }
    let cur = tracks::read(&state.scene, &t).ok_or("no such track")?;
    let c = convert_controller(&cur, role, controller, (id.0 as u32).wrapping_mul(2654435761) ^ t.pointer.len() as u32)?;
    let mut after = state.scene.node(id).unwrap().clone();
    tracks::edit_node(&mut after, std::slice::from_ref(&t), |_, p| {
        let _ = p.set_controller_json(&c);
    })?;
    match role {
        Role::Position => after.constraints.position = None,
        Role::Rotation => after.constraints.rotation = None,
        _ => {}
    }
    let before = state.scene.node(id).unwrap();
    if let Some(ch) = crate::anim_cmds::node_change(before, &after) {
        commit(state, "Assign Controller", ch)?;
    }
    Ok(json!({ "id": id.0, "track": t.name, "controller": controller }))
}

fn set_parameter(state: &mut AppState, id: NodeId, name: &str, value: f64) -> Result<Value, String> {
    let v = value as f32;
    let b = bool_of(value);
    // Keep Initial Offset needs the unconstrained pose.
    let (free_pos, free_rot, held_pos, held_rot) = {
        let n = state.scene.node(id).ok_or_else(|| format!("no node {}", id.0))?;
        let free = state.scene.frame_of(id) * n.transform.matrix();
        let (_, fr, fp) = free.to_scale_rotation_translation();
        let mut probe = state.scene.clone();
        if let Some(pn) = probe.node_mut(id) {
            match &mut pn.constraints.position {
                Some(PositionConstraint::Position { keep_offset, .. }) => *keep_offset = false,
                _ => {}
            }
            match &mut pn.constraints.rotation {
                Some(RotationConstraint::LookAt { keep_offset, .. }) | Some(RotationConstraint::Orientation { keep_offset, .. }) => *keep_offset = false,
                _ => {}
            }
        }
        probe.set_time(state.scene.time());
        let (_, hr, hp) = probe.world_matrix(id).to_scale_rotation_translation();
        state.scene.set_time(state.scene.time());
        (fp, fr, hp, hr)
    };
    let name_l = name.to_ascii_lowercase();
    set_constraints(state, "Set Constraint Parameter", id, move |c, _| {
        let mut done = false;
        match &mut c.position {
            Some(PositionConstraint::Path(p)) => {
                done = true;
                match name_l.as_str() {
                    "% along path" => p.percent.set(v),
                    "follow" => p.follow = b,
                    "bank" => p.bank = b,
                    "bank amount" => p.bank_amount = v,
                    "smoothness" => p.smoothness = v.clamp(0.0, 10.0),
                    "allow upside down" => p.allow_upside_down = b,
                    "constant velocity" => p.constant_velocity = b,
                    "loop" => p.loop_path = b,
                    "relative" => {
                        p.relative = b;
                        p.offset = if b { free_pos - held_pos } else { Vec3::ZERO };
                    }
                    "axis" => p.axis = (v as u32).min(2),
                    "flip" => p.flip = b,
                    _ => done = false,
                }
            }
            Some(PositionConstraint::Position { keep_offset, offset, .. }) if name_l == "keep initial offset" => {
                *keep_offset = b;
                *offset = free_pos - held_pos;
                done = true;
            }
            Some(PositionConstraint::Attachment { face, a, b: bb, align, .. }) => {
                done = true;
                match name_l.as_str() {
                    "face" => *face = v.max(0.0) as u32,
                    "a" => *a = v,
                    "b" => *bb = v,
                    "align to surface" => *align = b,
                    _ => done = false,
                }
            }
            _ => {}
        }
        if !done {
            match &mut c.rotation {
                Some(RotationConstraint::LookAt { axis, flip, keep_offset, offset, .. }) => {
                    done = true;
                    match name_l.as_str() {
                        "axis" => *axis = (v as u32).min(2),
                        "flip" => *flip = b,
                        "keep initial offset" => {
                            *keep_offset = b;
                            *offset = held_rot.inverse() * free_rot;
                        }
                        _ => done = false,
                    }
                }
                Some(RotationConstraint::Orientation { keep_offset, offset, .. }) if name_l == "keep initial offset" => {
                    *keep_offset = b;
                    *offset = held_rot.inverse() * free_rot;
                    done = true;
                }
                _ => {}
            }
        }
        if done { Ok(()) } else { Err(format!("no constraint parameter \"{name_l}\" on this object")) }
    })
}

/// The node's Position / Rotation / Scale controller names (constraints
/// first), and whether each is animated.
pub fn transform_controllers_json(n: &loom_scene::Node) -> Value {
    let name = |p: &Param<Vec3>, role: Role| tracks::AnyParam::Vec3(p.clone()).controller_name(role);
    json!({
        "position": n.constraints.position_name().map(str::to_owned).unwrap_or_else(|| name(&n.transform.position, Role::Position)),
        "rotation": n.constraints.rotation_name().map(str::to_owned).unwrap_or_else(|| name(&n.transform.rotation, Role::Rotation)),
        "scale": name(&n.transform.scale, Role::Scale),
        "link": n.constraints.link.is_some(),
        "animated": n.transform.position.is_animated() || n.transform.rotation.is_animated() || n.transform.scale.is_animated(),
    })
}
