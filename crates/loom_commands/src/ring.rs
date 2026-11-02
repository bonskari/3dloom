//! Create > Systems > Ring Array: a dummy with a ring of boxes around it.
//! Each box's position controller places it on the ring; Radius,
//! Amplitude, Cycles and Phase are animatable (Auto Key), Number is not.

use crate::command::Command;
use crate::execute::commit_exact as commit;
use crate::state::AppState;
use glam::Vec3;
use loom_scene::param::{Controller, RingCtrl};
use loom_scene::{Change, Node, NodeId, ObjectKind, Param};
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub fn handles(c: &Command) -> bool {
    matches!(c, Command::CreateRingArray { .. } | Command::SetRingArray { .. })
}

/// The boxes start as cubes a quarter of the radius wide (this build's size).
fn box_size(radius: f32) -> f32 {
    (radius * 0.25).max(0.01)
}

/// The ring a node belongs to: its centre dummy and its boxes in ring order.
pub fn members(state: &AppState, id: NodeId) -> Option<(NodeId, Vec<NodeId>)> {
    let is_box = |n: &Node| matches!(n.transform.position.controller, Controller::Ring(_));
    let n = state.scene.node(id)?;
    let centre = if is_box(n) { n.parent? } else { id };
    let mut boxes: Vec<(u32, NodeId)> = state
        .scene
        .children(centre)
        .into_iter()
        .filter_map(|c| match &state.scene.node(c)?.transform.position.controller {
            Controller::Ring(r) => Some((r.index, c)),
            _ => None,
        })
        .collect();
    if boxes.is_empty() {
        return None;
    }
    boxes.sort();
    Some((centre, boxes.into_iter().map(|b| b.1).collect()))
}

/// The ring's parameters (from its first box).
pub fn ring_of(state: &AppState, id: NodeId) -> Option<RingCtrl> {
    let (_, boxes) = members(state, id)?;
    match &state.scene.node(boxes[0])?.transform.position.controller {
        Controller::Ring(r) => Some((**r).clone()),
        _ => None,
    }
}

/// Unique names `Box###` for `n` new boxes.
fn box_names(state: &AppState, n: usize) -> Vec<String> {
    let taken: BTreeSet<String> = state.scene.nodes().map(|x| x.name.clone()).collect();
    (1..).map(|i| format!("Box{i:03}")).filter(|s| !taken.contains(s)).take(n).collect()
}

fn ring_box(name: &str, parent: NodeId, ctl: RingCtrl, size: f32) -> Node {
    let mut b = Node::new(name, ObjectKind::box_object(size, size, size));
    b.parent = Some(parent);
    b.transform.position.controller = Controller::Ring(Box::new(ctl));
    b
}

pub fn execute(state: &mut AppState, cmd: Command) -> Result<Value, String> {
    match cmd {
        Command::CreateRingArray { position, radius } => {
            let radius = radius.max(0.0);
            let mut centre = Node::new(&state.scene.next_name("Dummy"), ObjectKind::dummy(box_size(radius).max(1.0)));
            centre.transform.position.set(Vec3::from(position));
            centre.wire_color = [120, 200, 120];
            let cid = NodeId(state.scene.next_id());
            centre.id = cid;
            // Four boxes, a flat circle: Amplitude, Cycles and Phase start at 0.
            let ctl = |index| RingCtrl { index, number: 4, radius: Param::new(radius), amplitude: Param::new(0.0), cycles: Param::new(0.0), phase: Param::new(0.0) };
            let mut changes = vec![Change::AddNode(centre)];
            let mut ids = Vec::new();
            for (i, name) in box_names(state, 4).iter().enumerate() {
                let mut b = ring_box(name, cid, ctl(i as u32), box_size(radius));
                b.id = NodeId(cid.0 + 1 + i as u64);
                ids.push(b.id.0);
                changes.push(Change::AddNode(b));
            }
            commit(state, "Create Ring Array", Change::Group(changes))?;
            state.mesh_dirty = true;
            state.selection = BTreeSet::from([cid]);
            state.sub_selection.clear();
            Ok(json!({ "dummy": cid.0, "boxes": ids }))
        }
        Command::SetRingArray { id, name, value } => {
            let (centre, boxes) = members(state, NodeId(id)).ok_or("not part of a Ring Array")?;
            let key = name.to_ascii_lowercase();
            let cur = ring_of(state, NodeId(id)).ok_or("not part of a Ring Array")?;
            if key == "number" {
                // Not animatable: boxes are added or taken away.
                let n = value.round().clamp(1.0, 200.0) as u32;
                let mut changes = Vec::new();
                for (i, b) in boxes.iter().enumerate() {
                    if i as u32 >= n {
                        changes.push(Change::RemoveNode(*b));
                        continue;
                    }
                    let before = state.scene.node(*b).unwrap().transform.clone();
                    let mut after = before.clone();
                    if let Controller::Ring(r) = &mut after.position.controller {
                        r.number = n;
                    }
                    changes.push(Change::SetTransform { id: *b, before, after });
                }
                let size = match &state.scene.node(boxes[0]).unwrap().object {
                    ObjectKind::Box { length, .. } => length.get(),
                    _ => box_size(cur.radius.get()),
                };
                let names = box_names(state, (n as usize).saturating_sub(boxes.len()));
                let mut next = state.scene.next_id();
                for (k, name) in names.iter().enumerate() {
                    let mut ctl = cur.clone();
                    ctl.index = (boxes.len() + k) as u32;
                    ctl.number = n;
                    let mut b = ring_box(name, centre, ctl, size);
                    b.id = NodeId(next);
                    next += 1;
                    changes.push(Change::AddNode(b));
                }
                state.selection.retain(|s| state.scene.node(*s).is_some_and(|_| !boxes.iter().skip(n as usize).any(|b| b == s)));
                commit(state, "Ring Array Number", Change::Group(changes))?;
                state.mesh_dirty = true;
                return Ok(json!({ "number": n }));
            }
            let v = value as f32;
            let mut changes = Vec::new();
            for b in &boxes {
                let before = state.scene.node(*b).unwrap().transform.clone();
                let mut after = before.clone();
                if let Controller::Ring(r) = &mut after.position.controller {
                    let p = match key.as_str() {
                        "radius" => &mut r.radius,
                        "amplitude" => &mut r.amplitude,
                        "cycles" => &mut r.cycles,
                        "phase" => &mut r.phase,
                        _ => return Err(format!("no Ring Array parameter \"{name}\" (Radius, Amplitude, Cycles, Phase, Number)")),
                    };
                    p.set(if key == "radius" { v.max(0.0) } else { v });
                }
                changes.push(Change::SetTransform { id: *b, before, after });
            }
            commit(state, "Ring Array Parameter", Change::Group(changes))?;
            state.mesh_dirty = true;
            Ok(json!({ "boxes": boxes.len() }))
        }
        _ => Err("not a Ring Array command".into()),
    }
}
