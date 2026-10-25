//! Transform tools of the Extras toolbar's Array flyout and the Align
//! flyout: Snapshot (clones over time), Clone and Align, Place Highlight.

use crate::execute::commit;
use crate::state::{AppState, CloneKind, PickMode};
use crate::viewport::Dialog;
use crate::Command;
use glam::{EulerRot, Mat4, Quat, Vec3};
use loom_scene::{Change, NodeId, ObjectKind, PolyObject, Transform};
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
                0 => node.instance_of = None,
                1 | 2 => node.instance_of = Some(src.instance_of.unwrap_or(*id)),
                _ => {
                    node.object = ObjectKind::EditablePoly(PolyObject::from_mesh(mesh.unwrap_or_default()));
                    node.modifiers.clear();
                    node.active_entry = None;
                    node.instance_of = None;
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
        _ => Err("not a transform tool command".into()),
    }
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
