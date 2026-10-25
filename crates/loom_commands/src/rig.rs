//! Bones and helpers: Systems > Bones chains, Bone Tools (Bone Edit Mode,
//! Refine, Mirror, Reassign Root, Connect, Create End, fins) and the
//! Dummy / Point helpers.

use crate::execute::commit;
use crate::state::{AppState, SelectionMode};
use crate::viewport::Dialog;
use crate::Command;
use glam::{Mat4, Quat, Vec3};
use loom_scene::ik::{IkChain, IkSolver};
use loom_scene::{Change, Node, NodeId, ObjectKind, Transform};
use serde_json::{json, Value};
use std::collections::BTreeSet;

fn missing(id: NodeId) -> String {
    format!("no node {}", id.0)
}

/// A bone's orientation: local +X along `dir`, local Y as close to `up_hint` as the direction allows.
pub fn bone_rotation(dir: Vec3, up_hint: Vec3) -> Quat {
    let x = dir.normalize_or_zero();
    if x == Vec3::ZERO {
        return Quat::IDENTITY;
    }
    let mut z = up_hint - x * up_hint.dot(x);
    if z.length_squared() < 1e-8 {
        z = Vec3::Y - x * Vec3::Y.dot(x);
    }
    let z = z.normalize();
    let y = z.cross(x).normalize();
    Quat::from_mat3(&glam::Mat3::from_cols(x, y, z))
}

/// A bone's orientation for a bone whose own axis is `axis` (not necessarily +X).
pub fn bone_rotation_axis(dir: Vec3, up_hint: Vec3, axis: Vec3) -> Quat {
    let to_x = if (axis - Vec3::X).length_squared() < 1e-8 { Quat::IDENTITY } else if (axis + Vec3::X).length_squared() < 1e-8 { Quat::from_rotation_z(std::f32::consts::PI) } else { Quat::from_rotation_arc(axis, Vec3::X) };
    bone_rotation(dir, up_hint) * to_x
}

/// Local transform of a bone at `base` pointing at `tip`, under `parent_world`.
fn bone_transform(parent_world: Mat4, base: Vec3, tip: Vec3, prev: Option<&Transform>) -> Transform {
    bone_transform_axis(parent_world, base, tip, prev, Vec3::X)
}

fn bone_transform_axis(parent_world: Mat4, base: Vec3, tip: Vec3, prev: Option<&Transform>, axis: Vec3) -> Transform {
    let world = Mat4::from_rotation_translation(bone_rotation_axis(tip - base, Vec3::Z, axis), base);
    let local = parent_world.inverse() * world;
    match prev {
        Some(p) => Transform::from_matrix_near(local, p, Vec3::ZERO),
        None => Transform::from_matrix(local),
    }
}

fn bone_length(node: &Node) -> f32 {
    node.object.bone_length().unwrap_or(0.0)
}

fn with_length(object: &ObjectKind, len: f32) -> ObjectKind {
    let mut o = object.clone();
    if let ObjectKind::Bone { length, .. } = &mut o {
        length.set(len.max(1e-3));
    }
    o
}

/// The tip of a bone in world space.
pub fn bone_tip(state: &AppState, id: NodeId) -> Option<Vec3> {
    let n = state.scene.node(id)?;
    Some(state.scene.world_matrix(id).transform_point3(n.object.bone_tip_local()?))
}

fn new_bone(state: &AppState, name: &str, taken: &mut BTreeSet<String>, length: f32, like: Option<&Node>) -> Node {
    let mut n = 1;
    let base = name.trim_end_matches(|c: char| c.is_ascii_digit());
    let base = if base.is_empty() { "Bone" } else { base };
    let name = loop {
        let candidate = format!("{base}{n:03}");
        if !taken.contains(&candidate) && !state.scene.nodes().any(|x| x.name == candidate) {
            break candidate;
        }
        n += 1;
    };
    taken.insert(name.clone());
    let object = match like {
        Some(l) => with_length(&l.object, length),
        None => ObjectKind::bone(length),
    };
    let mut node = Node::new(&name, object);
    node.wire_color = [180, 180, 100];
    node.layer = state.scene.current_layer;
    node
}

pub fn execute(state: &mut AppState, cmd: Command) -> Result<Value, String> {
    use Command as C;
    match cmd {
        C::CreateBoneChain { points, name } => {
            if points.len() < 2 {
                return Err("a bone chain needs at least two points".into());
            }
            let pts: Vec<Vec3> = points.iter().map(|p| Vec3::from(*p)).collect();
            let base_name = name.unwrap_or_else(|| "Bone".into());
            let mut taken = BTreeSet::new();
            let first = state.scene.next_id();
            let mut changes = Vec::new();
            let mut ids = Vec::new();
            let mut parent_world = Mat4::IDENTITY;
            let mut parent: Option<NodeId> = None;
            for i in 0..pts.len() {
                let (base, tip, len) = if i + 1 < pts.len() {
                    (pts[i], pts[i + 1], pts[i].distance(pts[i + 1]))
                } else {
                    // The nub: a tenth of the last bone, pointing on along it.
                    let dir = (pts[i] - pts[i - 1]).normalize_or_zero();
                    let len = (pts[i].distance(pts[i - 1]) * 0.1).max(0.5);
                    (pts[i], pts[i] + dir * len, len)
                };
                let mut node = new_bone(state, &base_name, &mut taken, len, None);
                node.transform = bone_transform(parent_world, base, tip, None);
                node.parent = parent;
                let id = NodeId(first + i as u64);
                node.id = id;
                parent_world *= node.transform.matrix();
                parent = Some(id);
                ids.push(id);
                changes.push(Change::AddNode(node));
            }
            commit(state, "Create Bones", Change::Group(changes))?;
            state.selection = ids.iter().copied().collect();
            state.sub_selection.clear();
            state.selection_mode = SelectionMode::Object;
            if state.bones_ik_chain && ids.len() >= 2 {
                let _ = create_ik_chain(state, ids[0], *ids.last().unwrap(), IkSolver::HI, None)?;
                state.selection = ids.iter().copied().collect();
            }
            Ok(json!({ "nodes": ids.iter().map(|i| i.0).collect::<Vec<_>>() }))
        }
        C::BoneTools | C::BoneOptions => {
            state.dialog = Some(Dialog::BoneTools);
            Ok(json!({ "dialog": "BoneTools" }))
        }
        C::BoneEditModeToggle => {
            state.bone_edit_mode = !state.bone_edit_mode;
            Ok(json!({ "bone_edit_mode": state.bone_edit_mode }))
        }
        C::RefineBone { id, t } => {
            let id = NodeId(id);
            let node = state.scene.node(id).ok_or_else(|| missing(id))?.clone();
            if !node.object.is_bone() {
                return Err(format!("{} is not a bone", node.name));
            }
            let t = t.clamp(0.01, 0.99);
            let len = bone_length(&node);
            let (a, b) = (len * t, len * (1.0 - t));
            let world = state.scene.world_matrix(id);
            let new_id = NodeId(state.scene.next_id());
            let mut taken = BTreeSet::new();
            let mut child = new_bone(state, &node.name, &mut taken, b, Some(&node));
            child.id = new_id;
            child.parent = Some(id);
            let dir = node.object.bone_axis().unwrap_or(Vec3::X);
            child.transform.position.set(dir * a);
            let mut changes = vec![Change::ReplaceObject { id, before: Box::new(node.object.clone()), after: Box::new(with_length(&node.object, a)) }, Change::AddNode(child)];
            // The old children hang from the new bone, where they were in the world.
            let new_world = world * Mat4::from_translation(dir * a);
            for c in state.scene.children(id) {
                let Some(cn) = state.scene.node(c) else { continue };
                let after = Transform::from_matrix_near(new_world.inverse() * state.scene.world_matrix(c), &cn.transform, Vec3::ZERO);
                changes.push(Change::SetParent { id: c, before: Some(id), after: Some(new_id) });
                changes.push(Change::SetTransform { id: c, before: cn.transform.clone(), after });
            }
            commit(state, "Refine Bone", Change::Group(changes))?;
            Ok(json!({ "node": new_id.0 }))
        }
        C::MirrorBones { axis, offset, flip } => {
            let roots: Vec<NodeId> = state.selection.iter().copied().filter(|id| state.scene.node(*id).is_some_and(|n| n.object.is_bone())).collect();
            if roots.is_empty() {
                return Err("select bones to mirror".into());
            }
            let a = (axis.min(2)) as usize;
            let mut mirror = Mat4::IDENTITY;
            mirror.col_mut(a)[a] = -1.0;
            let m = Mat4::from_translation(Vec3::AXES[a] * offset) * mirror * Mat4::from_translation(-Vec3::AXES[a] * offset);
            // Only the topmost selected bones start a mirrored subtree.
            let selected: BTreeSet<NodeId> = roots.iter().copied().collect();
            let tops: Vec<NodeId> = roots.iter().copied().filter(|id| !state.scene.ancestors(*id).iter().any(|p| selected.contains(p))).collect();
            let first = state.scene.next_id();
            let mut changes = Vec::new();
            let mut ids = Vec::new();
            let mut taken = BTreeSet::new();
            let mut map: std::collections::HashMap<NodeId, NodeId> = std::collections::HashMap::new();
            let mut queue: Vec<(NodeId, Option<NodeId>)> = tops.iter().map(|t| (*t, state.scene.node(*t).and_then(|n| n.parent))).collect();
            while let Some((src, parent_src)) = queue.pop() {
                let Some(n) = state.scene.node(src) else { continue };
                let world = state.scene.world_matrix(src);
                let len = bone_length(n);
                let base = m.transform_point3(world.transform_point3(Vec3::ZERO));
                let tip = m.transform_point3(world.transform_point3(n.object.bone_tip_local().unwrap_or(Vec3::X * len)));
                let parent_new = parent_src.map(|p| map.get(&p).copied().unwrap_or(p));
                let parent_world = parent_new.map(|p| if map.values().any(|v| *v == p) { mirrored_world(state, &changes, p, first) } else { state.scene.world_matrix(p) }).unwrap_or(Mat4::IDENTITY);
                let mut up = m.transform_vector3(world.transform_vector3(Vec3::Z));
                if flip > 0 {
                    let f = (flip as usize - 1).min(2);
                    let mut fm = Vec3::ONE;
                    fm[f] = -1.0;
                    up *= fm;
                }
                let name = mirrored_name(&n.name);
                let mut node = new_bone(state, &name, &mut taken, len, Some(n));
                if !state.scene.nodes().any(|x| x.name == name) && !taken.contains(&name) {
                    node.name = name.clone();
                    taken.insert(name);
                }
                let id = NodeId(first + ids.len() as u64);
                node.id = id;
                node.parent = parent_new;
                node.transform = Transform::from_matrix(parent_world.inverse() * Mat4::from_rotation_translation(bone_rotation_axis(tip - base, up, n.object.bone_axis().unwrap_or(Vec3::X)), base));
                map.insert(src, id);
                ids.push(id);
                changes.push(Change::AddNode(node));
                for c in state.scene.children(src) {
                    queue.push((c, Some(src)));
                }
            }
            commit(state, "Mirror Bones", Change::Group(changes))?;
            state.selection = ids.iter().copied().collect();
            Ok(json!({ "nodes": ids.iter().map(|i| i.0).collect::<Vec<_>>() }))
        }
        C::ReassignRoot { id } => {
            let id = NodeId(id);
            let node = state.scene.node(id).ok_or_else(|| missing(id))?;
            if !node.object.is_bone() {
                return Err(format!("{} is not a bone", node.name));
            }
            // The chain from the old root down to `id`: every bone on it reverses.
            let mut chain: Vec<NodeId> = vec![id];
            let mut cur = node.parent;
            while let Some(p) = cur {
                let Some(pn) = state.scene.node(p) else { break };
                if !pn.object.is_bone() {
                    break;
                }
                chain.push(p);
                cur = pn.parent;
            }
            if chain.len() < 2 {
                return Ok(json!({ "node": id.0 }));
            }
            // World geometry before anything moves.
            let worlds: Vec<(NodeId, Vec3, Vec3, f32)> = chain.iter().map(|b| { let n = state.scene.node(*b).unwrap(); let w = state.scene.world_matrix(*b); (*b, w.transform_point3(Vec3::ZERO), w.transform_point3(n.object.bone_tip_local().unwrap_or(Vec3::ZERO)), bone_length(n)) }).collect();
            let old_root = *chain.last().unwrap();
            let above = state.scene.node(old_root).and_then(|n| n.parent);
            let mut changes = Vec::new();
            // Side children of each chain bone keep their world placement under the same bone.
            let mut side: Vec<(NodeId, NodeId, Mat4)> = Vec::new();
            for b in &chain {
                for c in state.scene.children(*b) {
                    if !chain.contains(&c) {
                        side.push((c, *b, state.scene.world_matrix(c)));
                    }
                }
            }
            // New chain: id (root) → its old parent → … → old root; bone k now spans
            // from the old joint (its old tip-side base) back to its old base.
            let mut parent: Option<NodeId> = above;
            let mut parent_world = above.map(|p| state.scene.world_matrix(p)).unwrap_or(Mat4::IDENTITY);
            let mut new_worlds: Vec<(NodeId, Mat4)> = Vec::new();
            for (k, (b, base, _tip, len)) in worlds.iter().enumerate() {
                // Bone k's new base is the old base of bone k-1 (the joint below it), or its own tip for the new root.
                let new_base = if k == 0 { worlds[0].2 } else { worlds[k - 1].1 };
                let new_tip = *base;
                let n = state.scene.node(*b).unwrap();
                let t = bone_transform_axis(parent_world, new_base, new_tip, Some(&n.transform), n.object.bone_axis().unwrap_or(Vec3::X));
                let w = parent_world * t.matrix();
                changes.push(Change::SetParent { id: *b, before: n.parent, after: parent });
                changes.push(Change::SetTransform { id: *b, before: n.transform.clone(), after: t });
                let new_len = new_base.distance(new_tip).max(1e-3);
                if (new_len - len).abs() > 1e-5 {
                    changes.push(Change::ReplaceObject { id: *b, before: Box::new(n.object.clone()), after: Box::new(with_length(&n.object, new_len)) });
                }
                new_worlds.push((*b, w));
                parent = Some(*b);
                parent_world = w;
            }
            for (c, b, world_c) in side {
                let Some(cn) = state.scene.node(c) else { continue };
                let bw = new_worlds.iter().find(|(x, _)| *x == b).map(|(_, w)| *w).unwrap_or(Mat4::IDENTITY);
                let after = Transform::from_matrix_near(bw.inverse() * world_c, &cn.transform, Vec3::ZERO);
                changes.push(Change::SetTransform { id: c, before: cn.transform.clone(), after });
            }
            commit(state, "Reassign Root", Change::Group(changes))?;
            Ok(json!({ "node": id.0 }))
        }
        C::DeleteBone { id } => {
            let id = NodeId(id);
            let node = state.scene.node(id).ok_or_else(|| missing(id))?.clone();
            let mut changes = Vec::new();
            let parent_world = node.parent.map(|p| state.scene.world_matrix(p)).unwrap_or(Mat4::IDENTITY);
            for c in state.scene.children(id) {
                let Some(cn) = state.scene.node(c) else { continue };
                let after = Transform::from_matrix_near(parent_world.inverse() * state.scene.world_matrix(c), &cn.transform, Vec3::ZERO);
                changes.push(Change::SetParent { id: c, before: Some(id), after: node.parent });
                changes.push(Change::SetTransform { id: c, before: cn.transform.clone(), after });
            }
            changes.push(Change::RemoveNode(id));
            commit(state, "Delete Bone", Change::Group(changes))?;
            state.selection.remove(&id);
            Ok(json!({}))
        }
        C::ConnectBones { id, to } => {
            let (id, to) = (NodeId(id), NodeId(to));
            let tip = bone_tip(state, id).ok_or_else(|| missing(id))?;
            let target = state.scene.node(to).ok_or_else(|| missing(to))?;
            let base_to = state.scene.world_matrix(to).transform_point3(Vec3::ZERO);
            let world_id = state.scene.world_matrix(id);
            let mut taken = BTreeSet::new();
            let new_id = NodeId(state.scene.next_id());
            let mut link = new_bone(state, &state.scene.node(id).unwrap().name.clone(), &mut taken, tip.distance(base_to).max(1e-3), state.scene.node(id));
            link.id = new_id;
            link.parent = Some(id);
            link.transform = bone_transform(world_id, tip, base_to, None);
            let link_world = world_id * link.transform.matrix();
            let after = Transform::from_matrix_near(link_world.inverse() * state.scene.world_matrix(to), &target.transform, Vec3::ZERO);
            let changes = vec![Change::AddNode(link), Change::SetParent { id: to, before: target.parent, after: Some(new_id) }, Change::SetTransform { id: to, before: target.transform.clone(), after }];
            commit(state, "Connect Bones", Change::Group(changes))?;
            Ok(json!({ "node": new_id.0 }))
        }
        C::CreateEndBone { id } => {
            let id = NodeId(id);
            let node = state.scene.node(id).ok_or_else(|| missing(id))?.clone();
            let len = (bone_length(&node) * 0.1).max(0.5);
            let mut taken = BTreeSet::new();
            let new_id = NodeId(state.scene.next_id());
            let mut nub = new_bone(state, &node.name, &mut taken, len, Some(&node));
            nub.id = new_id;
            nub.parent = Some(id);
            nub.transform.position.set(node.object.bone_tip_local().unwrap_or(Vec3::ZERO));
            commit(state, "Create End", Change::AddNode(nub))?;
            Ok(json!({ "node": new_id.0 }))
        }
        C::SetBoneFins { side, front, back } => {
            let mut changes = Vec::new();
            for id in state.selection.iter().copied() {
                let Some(n) = state.scene.node(id) else { continue };
                let mut o = n.object.clone();
                if let ObjectKind::Bone { fins, .. } = &mut o {
                    if let Some(v) = side { fins.side.set(v); }
                    if let Some(v) = front { fins.front.set(v); }
                    if let Some(v) = back { fins.back.set(v); }
                    changes.push(Change::ReplaceObject { id, before: Box::new(n.object.clone()), after: Box::new(o) });
                }
            }
            if changes.is_empty() {
                return Err("select bones".into());
            }
            commit(state, "Bone Fins", Change::Group(changes))?;
            Ok(json!({}))
        }
        C::CreateHelper { kind, position, size } => {
            let (object, base, colour) = match kind.as_str() {
                "dummy" => (ObjectKind::dummy(size.max(0.001)), "Dummy", [120, 200, 120]),
                "point" => (ObjectKind::point(size.max(0.001)), "Point", [120, 200, 120]),
                "protractor" => (ObjectKind::protractor(), "Protractor", [120, 200, 120]),
                "compass" => (ObjectKind::compass(size.max(0.001)), "Compass", [120, 200, 120]),
                "grid" => (ObjectKind::grid(size.max(0.001), size.max(0.001)), "Grid", [120, 200, 120]),
                "ikgoal" => (ObjectKind::IkGoal { chain: 0 }, "IK Chain", [0, 200, 255]),
                other => return Err(format!("unknown helper kind {other:?} (dummy, point, protractor, compass, grid, ikgoal)")),
            };
            let mut node = Node::new(&state.scene.next_name(base), object);
            node.transform.position.set(Vec3::from(position));
            node.wire_color = colour;
            let id = NodeId(state.scene.next_id());
            commit(state, &format!("Create {base}"), Change::AddNode(node))?;
            state.selection = BTreeSet::from([id]);
            state.sub_selection.clear();
            Ok(json!({ "node": id.0 }))
        }
        C::Dummy => execute(state, C::CreateHelper { kind: "dummy".into(), position: [0.0; 3], size: 10.0 }),
        C::HistoryIndependentIkSolver => chain_from_selection(state, IkSolver::HI),
        C::IkLimbSolver => chain_from_selection(state, IkSolver::Limb),
        C::SplineIkSolver => chain_from_selection(state, IkSolver::Spline),
        C::BonesIkChain => {
            state.bones_ik_chain = !state.bones_ik_chain;
            Ok(json!({ "bones_ik_chain": state.bones_ik_chain }))
        }
        C::CreateIkChain { start, end, solver, name } => {
            let solver = IkSolver::parse(&solver).ok_or_else(|| format!("unknown solver {solver:?} (hi, limb, spline)"))?;
            let goal = create_ik_chain(state, NodeId(start), NodeId(end), solver, name)?;
            Ok(json!({ "goal": goal.0, "chain": state.scene.ik_chains.last().map(|c| c.id) }))
        }
        C::IkSolverToggle => {
            let idx = selected_chain(state)?;
            let mut after = state.scene.ik_chains.clone();
            after[idx].enabled = !after[idx].enabled;
            set_chains(state, "IK Solver Toggle", after)?;
            Ok(json!({ "enabled": state.scene.ik_chains[idx].enabled }))
        }
        C::SetIkChainParameter { chain, name, value } => {
            let idx = state.scene.ik_chains.iter().position(|c| c.id == chain).ok_or_else(|| format!("no IK chain {chain}"))?;
            let mut after = state.scene.ik_chains.clone();
            let c = &mut after[idx];
            match name.as_str() {
                "Enabled" => c.enabled = value != 0.0,
                "IK For FK Pose" => c.ik_for_fk = value != 0.0,
                "Swivel Angle" => c.swivel_deg = value as f32,
                "Threshold" => c.threshold = (value as f32).max(0.001),
                "Iterations" => c.iterations = (value.round().max(1.0)) as u32,
                "Solver" => c.solver = match value as i32 { 1 => IkSolver::Limb, 2 => IkSolver::Spline, _ => IkSolver::HI },
                other => return Err(format!("IK chain has no parameter {other:?}")),
            }
            set_chains(state, "IK Chain Parameter", after)?;
            Ok(json!({}))
        }
        C::PickSplineIkShape { chain, node } => {
            let idx = state.scene.ik_chains.iter().position(|c| c.id == chain).ok_or_else(|| format!("no IK chain {chain}"))?;
            let n = state.scene.node(NodeId(node)).ok_or_else(|| missing(NodeId(node)))?;
            if n.object.shape().is_none() {
                return Err(format!("{} is not a shape", n.name));
            }
            let mut after = state.scene.ik_chains.clone();
            after[idx].spline = Some(NodeId(node));
            after[idx].solver = IkSolver::Spline;
            set_chains(state, "Pick Spline IK Shape", after)?;
            Ok(json!({}))
        }
        C::IkChainFkSnap => {
            let idx = selected_chain(state)?;
            fk_snap(state, idx)?;
            Ok(json!({}))
        }
        C::IkChainIkSnap => {
            let idx = selected_chain(state)?;
            ik_snap(state, idx)?;
            Ok(json!({}))
        }
        C::IkChainSnapAction => {
            let idx = selected_chain(state)?;
            fk_snap(state, idx)?;
            ik_snap(state, idx)?;
            Ok(json!({}))
        }
        C::IkTerminatorToggle => {
            let bone = state.selection.iter().copied().find(|id| state.scene.node(*id).is_some_and(|n| n.object.is_bone())).ok_or("select a bone of an IK chain")?;
            let idx = state.scene.ik_chains.iter().position(|c| state.scene.ik_chain_bones(c).contains(&bone) || state.scene.descendants(bone).contains(&c.end)).ok_or("the bone is on no IK chain")?;
            let mut after = state.scene.ik_chains.clone();
            if after[idx].start == bone {
                // Toggle back: the terminator moves up to the topmost bone above.
                let mut top = bone;
                while let Some(p) = state.scene.node(top).and_then(|n| n.parent).filter(|p| state.scene.node(*p).is_some_and(|n| n.object.is_bone())) {
                    top = p;
                }
                after[idx].start = top;
            } else {
                after[idx].start = bone;
            }
            set_chains(state, "IK Terminator Toggle", after)?;
            Ok(json!({ "start": state.scene.ik_chains[idx].start.0 }))
        }
        other => Err(format!("{} is not a rig command", other.name())),
    }
}

fn set_chains(state: &mut AppState, label: &str, after: Vec<IkChain>) -> Result<(), String> {
    let before = state.scene.ik_chains.clone();
    commit(state, label, Change::SetIkChains { before, after })
}

/// The chain of the selected goal or bone.
fn selected_chain(state: &AppState) -> Result<usize, String> {
    for id in state.selection.iter() {
        if let Some(i) = state.scene.ik_chains.iter().position(|c| c.goal == *id || state.scene.ik_chain_bones(c).contains(id)) {
            return Ok(i);
        }
    }
    Err("select an IK goal or a bone of an IK chain".into())
}

fn chain_from_selection(state: &mut AppState, solver: IkSolver) -> Result<Value, String> {
    let mut bones: Vec<NodeId> = state.selection.iter().copied().filter(|id| state.scene.node(*id).is_some_and(|n| n.object.is_bone())).collect();
    if bones.len() < 2 {
        return Err("select at least two bones (the chain's start and end)".into());
    }
    bones.sort_by_key(|id| state.scene.depth(*id));
    let (start, end) = (bones[0], *bones.last().unwrap());
    let goal = create_ik_chain(state, start, end, solver, None)?;
    Ok(json!({ "goal": goal.0, "chain": state.scene.ik_chains.last().map(|c| c.id) }))
}

/// Adds a chain and its goal node (at the end bone's base); the goal becomes the selection.
fn create_ik_chain(state: &mut AppState, start: NodeId, end: NodeId, solver: IkSolver, name: Option<String>) -> Result<NodeId, String> {
    if !state.scene.ancestors(end).contains(&start) {
        return Err("the end bone must descend from the start bone".into());
    }
    let id = state.scene.ik_chains.iter().map(|c| c.id).max().unwrap_or(0) + 1;
    let name = name.unwrap_or_else(|| state.scene.next_name("IK Chain"));
    let base = state.scene.fk_world_matrix(end).transform_point3(Vec3::ZERO);
    let mut goal = Node::new(&name, ObjectKind::IkGoal { chain: id });
    goal.transform.position.set(base);
    goal.wire_color = [0, 200, 255];
    let goal_id = NodeId(state.scene.next_id());
    let mut after = state.scene.ik_chains.clone();
    after.push(IkChain { id, name: name.clone(), solver, start, end, goal: goal_id, enabled: true, swivel_deg: 0.0, threshold: 0.1, iterations: 50, spline: None, ik_for_fk: false });
    let before = state.scene.ik_chains.clone();
    commit(state, &format!("Create {}", solver.label()), Change::Group(vec![Change::AddNode(goal), Change::SetIkChains { before, after }]))?;
    state.selection = BTreeSet::from([goal_id]);
    state.sub_selection.clear();
    Ok(goal_id)
}

/// FK snap: the solved rotations become the bones' own transforms.
fn fk_snap(state: &mut AppState, idx: usize) -> Result<(), String> {
    let chain = state.scene.ik_chains[idx].clone();
    let mut changes = Vec::new();
    for id in state.scene.ik_chain_bones(&chain) {
        let Some(m) = state.scene.ik_rotation(id) else { continue };
        let Some(n) = state.scene.node(id) else { continue };
        let after = Transform::from_matrix_near(m, &n.transform, Vec3::ZERO);
        changes.push(Change::SetTransform { id, before: n.transform.clone(), after });
    }
    if changes.is_empty() {
        return Ok(());
    }
    commit(state, "IK Chain FK Snap", Change::Group(changes))
}

/// IK snap: the goal moves to the FK end joint.
fn ik_snap(state: &mut AppState, idx: usize) -> Result<(), String> {
    let chain = state.scene.ik_chains[idx].clone();
    let target = state.scene.fk_world_matrix(chain.end).transform_point3(Vec3::ZERO);
    let goal = state.scene.node(chain.goal).ok_or_else(|| missing(chain.goal))?;
    let frame = state.scene.parent_frame(chain.goal);
    let mut after = goal.transform.clone();
    after.position.set(frame.inverse().transform_point3(target));
    commit(state, "IK Chain IK Snap", Change::SetTransform { id: chain.goal, before: goal.transform.clone(), after })
}

/// World matrix of a bone added earlier in this Mirror run (not yet in the scene).
fn mirrored_world(state: &AppState, changes: &[Change], id: NodeId, _first: u64) -> Mat4 {
    let mut chain: Vec<&Node> = Vec::new();
    let mut cur = Some(id);
    while let Some(c) = cur {
        let Some(n) = changes.iter().find_map(|ch| match ch { Change::AddNode(n) if n.id == c => Some(n), _ => None }) else {
            return cur.map(|c| state.scene.world_matrix(c)).unwrap_or(Mat4::IDENTITY) * chain.iter().rev().fold(Mat4::IDENTITY, |m, n| m * n.transform.matrix());
        };
        chain.push(n);
        cur = n.parent;
    }
    chain.iter().rev().fold(Mat4::IDENTITY, |m, n| m * n.transform.matrix())
}

/// `Bone_L` ↔ `Bone_R`, `LeftArm` ↔ `RightArm`, else `<name>_mirror`.
fn mirrored_name(name: &str) -> String {
    for (a, b) in [("_L", "_R"), ("_R", "_L"), ("Left", "Right"), ("Right", "Left"), (" L", " R"), (" R", " L")] {
        if let Some(stem) = name.strip_suffix(a) {
            return format!("{stem}{b}");
        }
        if name.contains(a) && !name.contains(b) {
            return name.replacen(a, b, 1);
        }
    }
    format!("{name}_mirror")
}

/// Bone Edit Mode: a moved node whose parent is a bone (and not itself moving)
/// re-aims and stretches that parent so its tip stays on the child; the
/// parent's other children keep their world placement.
pub fn bone_edit_adjust(state: &AppState, changes: &mut Vec<Change>) {
    let moved: Vec<(NodeId, Transform)> = changes.iter().filter_map(|c| match c { Change::SetTransform { id, after, .. } => Some((*id, after.clone())), _ => None }).collect();
    let moving: BTreeSet<NodeId> = moved.iter().map(|(id, _)| *id).collect();
    for (id, after) in moved {
        let Some(node) = state.scene.node(id) else { continue };
        let Some(p) = node.parent else { continue };
        if moving.contains(&p) {
            continue;
        }
        let Some(parent) = state.scene.node(p) else { continue };
        if !parent.object.is_bone() {
            continue;
        }
        let parent_world = state.scene.world_matrix(p);
        let child_world = state.scene.parent_frame(id) * after.matrix();
        let child_pos = child_world.transform_point3(Vec3::ZERO);
        let base = parent_world.transform_point3(Vec3::ZERO);
        let new_len = base.distance(child_pos);
        if new_len < 1e-4 {
            continue;
        }
        // Turn the parent so its own axis points at the moved child (any bone axis).
        let cur_dir = parent_world.transform_vector3(parent.object.bone_axis().unwrap_or(Vec3::X)).normalize_or_zero();
        let new_dir = (child_pos - base).normalize_or_zero();
        let (_, rot, _) = parent_world.to_scale_rotation_translation();
        let turn = if cur_dir == Vec3::ZERO || new_dir == Vec3::ZERO { Quat::IDENTITY } else { Quat::from_rotation_arc(cur_dir, new_dir) };
        let new_parent_world = Mat4::from_rotation_translation((turn * rot).normalize(), base);
        let frame = state.scene.parent_frame(p);
        let new_parent_t = Transform::from_matrix_near(frame.inverse() * new_parent_world, &parent.transform, Vec3::ZERO);
        changes.push(Change::SetTransform { id: p, before: parent.transform.clone(), after: new_parent_t });
        changes.push(Change::ReplaceObject { id: p, before: Box::new(parent.object.clone()), after: Box::new(with_length(&parent.object, new_len)) });
        for c in state.scene.children(p) {
            let Some(cn) = state.scene.node(c) else { continue };
            let target = if c == id { child_world } else { state.scene.world_matrix(c) };
            let new_frame = loom_scene::Scene::frame_from(new_parent_world, cn.inherit);
            let t = Transform::from_matrix_near(new_frame.inverse() * target, &cn.transform, Vec3::ZERO);
            if c == id {
                for ch in changes.iter_mut() {
                    if let Change::SetTransform { id: x, after, .. } = ch {
                        if *x == id {
                            *after = t.clone();
                        }
                    }
                }
            } else {
                changes.push(Change::SetTransform { id: c, before: cn.transform.clone(), after: t });
            }
        }
    }
}
