//! Motion panel: constraint target picking and Trajectories (Convert To /
//! Convert From / Collapse, Sub-Object Keys).

use crate::anim_cmds::{edit_nodes, node_change};
use crate::command::{Command, SelectOp};
use crate::execute::commit_exact as commit;
use crate::state::{AppState, PickMode};
use glam::{EulerRot, Mat4, Vec3};
use loom_scene::param::{KeyedTracks, ParamType};
use loom_scene::tracks::{AnyParam, TrackOwner};
use loom_scene::{Change, NodeId, ObjectKind, Param};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConstraintPick {
    Path,
    Position,
    Attachment,
    LookAt,
    Orientation,
    Link,
}

pub fn handles(c: &Command) -> bool {
    use Command as C;
    matches!(c, C::StartConstraintPick { .. } | C::TrajectoryConvertTo { .. } | C::TrajectoryConvertFrom { .. } | C::TrajectoryCollapse { .. } | C::TrajectorySubObjectKeys | C::TrajectorySelectKeys { .. } | C::TrajectoryMoveKeys { .. })
}

/// The picked object becomes the target of the constraint being added.
pub fn pick_constraint(state: &mut AppState, kind: ConstraintPick, target: NodeId) -> Result<Value, String> {
    use Command as C;
    let t = target.0;
    let cmd = match kind {
        ConstraintPick::Path => C::PathConstraint { target: t },
        ConstraintPick::Position => C::PositionConstraint { target: t },
        ConstraintPick::Attachment => C::AttachmentConstraint { target: t, face: None, a: None, b: None },
        ConstraintPick::LookAt => C::LookAtConstraint { target: t },
        ConstraintPick::Orientation => C::OrientationConstraint { target: t },
        ConstraintPick::Link => C::LinkConstraint { target: Some(t) },
    };
    crate::controllers::execute(state, cmd)
}

/// Sample frames from `start` to `end` in `samples` steps (inclusive).
fn frames(state: &AppState, start: Option<i32>, end: Option<i32>, samples: Option<u32>) -> Vec<f32> {
    let (a, b) = (start.unwrap_or(state.frame_range.0) as f32, end.unwrap_or(state.frame_range.1) as f32);
    let n = samples.unwrap_or(10).max(1);
    (0..=n).map(|i| a + (b - a) * i as f32 / n as f32).collect()
}

fn selected(state: &AppState) -> Result<Vec<NodeId>, String> {
    let ids: Vec<NodeId> = state.selection.iter().copied().collect();
    if ids.is_empty() {
        return Err("select the objects first".into());
    }
    Ok(ids)
}

/// Euler XYZ degrees of a matrix, unwrapped toward `prev`.
fn euler_near(m: Mat4, prev: Option<Vec3>) -> Vec3 {
    let (_, q, _) = m.to_scale_rotation_translation();
    let (x, y, z) = q.to_euler(EulerRot::XYZ);
    let mut r = Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees());
    if let Some(p) = prev {
        for i in 0..3 {
            r[i] += ((p[i] - r[i]) / 360.0).round() * 360.0;
        }
    }
    r
}

fn keyed(values: &[(f32, Vec3)], tangents: (loom_scene::anim::TangentType, loom_scene::anim::TangentType)) -> Param<Vec3> {
    let mut k = KeyedTracks::from_rest(ParamType::Vec3, values.first().map(|v| v.1.to_array().to_vec()).unwrap_or(vec![0.0; 3]));
    for (t, v) in values {
        for (c, tr) in k.tracks.iter_mut().enumerate() {
            tr.set_key(*t, v[c], tangents);
        }
    }
    Param { controller: loom_scene::param::Controller::Keys(k) }
}

pub fn execute(state: &mut AppState, cmd: Command) -> Result<Value, String> {
    use Command as C;
    match cmd {
        C::StartConstraintPick { kind } => {
            let k = match kind.as_str() {
                "Path Constraint" => ConstraintPick::Path,
                "Position Constraint" => ConstraintPick::Position,
                "Attachment Constraint" | "Attachment" => ConstraintPick::Attachment,
                "LookAt Constraint" => ConstraintPick::LookAt,
                "Orientation Constraint" => ConstraintPick::Orientation,
                "Link Constraint" => ConstraintPick::Link,
                other => return Err(format!("no constraint \"{other}\"")),
            };
            selected(state)?;
            state.pick_mode = Some(PickMode::Constraint(k));
            state.status_prompt = format!("Pick the {kind} target");
            Ok(json!({ "pick": kind }))
        }
        C::TrajectoryConvertTo { start, end, samples } => {
            let id = *selected(state)?.first().unwrap();
            let fr = frames(state, start, end, samples);
            let pts: Vec<Vec3> = fr.iter().map(|t| state.scene.at_time(*t, || state.scene.world_matrix(id).w_axis.truncate())).collect();
            let origin = pts[0];
            let knots: Vec<loom_shape::Knot> = pts.iter().map(|p| loom_shape::Knot::smooth(*p - origin)).collect();
            let shape = loom_shape::Shape::from_splines(vec![loom_shape::Spline::new(knots, false)]);
            let new = crate::execute::add_object(state, ObjectKind::line_object(shape), origin)?;
            Ok(json!({ "id": new.0, "name": state.scene.node(new).map(|n| n.name.clone()) }))
        }
        C::TrajectoryConvertFrom { shape, start, end, samples } => {
            let sid = NodeId(shape);
            let sh = state.scene.node(sid).and_then(|n| n.object.shape()).ok_or("pick a shape to convert from")?;
            let om = state.scene.object_matrix(sid);
            let fr = frames(state, start, end, samples);
            let n = (fr.len() - 1).max(1) as f32;
            let tangents = state.default_tangent;
            let mut changes = Vec::new();
            for id in selected(state)? {
                // Knot parameterisation: with one sample per segment the keys land on the knots.
                let values: Vec<(f32, Vec3)> = fr
                    .iter()
                    .enumerate()
                    .filter_map(|(i, t)| {
                        let (p, _) = loom_scene::constraint::path_point(&sh, i as f32 / n, false)?;
                        let frame = state.scene.at_time(*t, || state.scene.frame_of(id));
                        Some((*t, frame.inverse().transform_point3(om.transform_point3(p))))
                    })
                    .collect();
                let n0 = state.scene.node(id).unwrap();
                let mut after = n0.clone();
                after.transform.position = keyed(&values, tangents);
                after.constraints.position = None;
                if let Some(c) = node_change(n0, &after) {
                    changes.push(c);
                }
            }
            commit(state, "Convert From", Change::Group(changes))?;
            Ok(json!({ "keys": fr.len() }))
        }
        C::TrajectoryCollapse { start, end, samples, position, rotation, scale } => {
            let (pos, rot, scl) = (position.unwrap_or(true), rotation.unwrap_or(true), scale.unwrap_or(true));
            let fr = frames(state, start, end, samples);
            let tangents = state.default_tangent;
            let mut changes = Vec::new();
            for id in selected(state)? {
                // World pose per sample, brought into the plain parent frame.
                let mut pv = Vec::new();
                let mut rv = Vec::new();
                let mut sv = Vec::new();
                let mut prev = None;
                for t in &fr {
                    let local = state.scene.at_time(*t, || state.scene.parent_frame(id).inverse() * state.scene.world_matrix(id));
                    let (s, _, p) = local.to_scale_rotation_translation();
                    let r = euler_near(local, prev);
                    prev = Some(r);
                    pv.push((*t, p));
                    rv.push((*t, r));
                    sv.push((*t, s));
                }
                let n0 = state.scene.node(id).unwrap();
                let mut after = n0.clone();
                if pos {
                    after.transform.position = keyed(&pv, tangents);
                    after.constraints.position = None;
                }
                if rot {
                    after.transform.rotation = keyed(&rv, tangents);
                    after.constraints.rotation = None;
                }
                if scl {
                    after.transform.scale = keyed(&sv, tangents);
                }
                if pos && rot {
                    after.constraints.link = None;
                }
                if let Some(c) = node_change(n0, &after) {
                    changes.push(c);
                }
            }
            commit(state, "Collapse", Change::Group(changes))?;
            Ok(json!({ "samples": fr.len() }))
        }
        C::TrajectorySubObjectKeys => {
            state.trajectory_keys = !state.trajectory_keys;
            state.trajectory_selected.clear();
            state.show_trajectories |= state.trajectory_keys;
            Ok(json!({ "sub_object_keys": state.trajectory_keys }))
        }
        C::TrajectorySelectKeys { frames, op } => {
            let s = &mut state.trajectory_selected;
            match op {
                SelectOp::Replace => *s = frames.into_iter().collect(),
                SelectOp::Add => s.extend(frames),
                SelectOp::Subtract => {
                    for f in frames {
                        s.remove(&f);
                    }
                }
            }
            Ok(json!({ "selected": s.iter().collect::<Vec<_>>() }))
        }
        C::TrajectoryMoveKeys { delta } => {
            let frames: Vec<f32> = state.trajectory_selected.iter().map(|f| *f as f32).collect();
            if frames.is_empty() {
                return Err("select trajectory keys first".into());
            }
            let d = Vec3::from(delta);
            let ids = selected(state)?;
            let locals: Vec<(NodeId, Vec3)> = ids.iter().map(|id| (*id, state.scene.frame_of(*id).inverse().transform_vector3(d))).collect();
            let tangents = state.default_tangent;
            edit_nodes(state, "Move Trajectory Keys", &ids, |t| t.owner == TrackOwner::Transform && t.pointer == "/position", |t, p| {
                let Some(ld) = locals.iter().find(|(i, _)| *i == t.node).map(|(_, v)| *v) else { return };
                let cur: Vec<Vec<f32>> = frames.iter().map(|f| p.comps_at(*f)).collect();
                let AnyParam::Vec3(_) = p else { return };
                let Some(k) = p.ensure_keys() else { return };
                for (fi, f) in frames.iter().enumerate() {
                    for c in 0..3 {
                        let tr = &mut k.tracks[c];
                        match tr.key_at(*f) {
                            Some(i) => tr.keys[i].value += ld[c],
                            None if ld[c] != 0.0 => {
                                tr.set_key(*f, cur[fi][c] + ld[c], tangents);
                            }
                            None => {}
                        }
                    }
                }
            })
        }
        _ => Err("not a Motion panel command".into()),
    }
}

/// A selected object's path for the viewports: world position per frame of
/// the active segment, and the frames its position (or transform) is keyed at.
pub fn trajectory(state: &AppState, id: NodeId) -> Option<(Vec<(i32, Vec3)>, Vec<i32>)> {
    let n = state.scene.node(id)?;
    let t = &n.transform;
    let moving = t.position.is_animated() || t.rotation.is_animated() || n.constraints.position.is_some() || n.constraints.link.is_some() || n.parent.is_some_and(|p| state.scene.node(p).is_some_and(|pn| pn.transform.position.is_animated() || pn.transform.rotation.is_animated()));
    if !moving {
        return None;
    }
    let (a, b) = state.frame_range;
    let step = (((b - a) / 1000) + 1).max(1);
    let pts: Vec<(i32, Vec3)> = (a..=b).step_by(step as usize).map(|f| (f, state.scene.at_time(f as f32, || state.scene.world_matrix(id).w_axis.truncate()))).collect();
    let mut keys: Vec<i32> = t.position.keys().map(|k| k.tracks.iter().flat_map(|tr| tr.keys.iter().map(|k| k.time.round() as i32)).collect()).unwrap_or_default();
    keys.sort();
    keys.dedup();
    Some((pts, keys))
}
