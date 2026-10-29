//! Characterization and undoable control-rig operations; never rewrite source keys.
use crate::execute::commit;
use crate::{AppState, Command};
use glam::{Mat4, Vec3};
use loom_scene::ik::IkSolver;
use loom_scene::loomrig::{ControlBinding, HumanoidMapping, LoomRig, RigLimb};
use loom_scene::{Change, Node, NodeId, ObjectKind};
use serde_json::{json, Value};
fn controls(r: &LoomRig) -> Vec<NodeId> {
    r.bindings
        .iter()
        .map(|b| b.control)
        .chain(r.limbs.iter().flat_map(|l| [l.goal, l.pole]))
        .collect()
}
fn build(
    state: &AppState,
    id: u32,
    mapping: HumanoidMapping,
    first: u64,
) -> Result<(LoomRig, Vec<Change>), String> {
    mapping.validate(&state.scene)?;
    let mut next = first;
    let mut changes = vec![];
    let mut helper = |name: String, parent: Option<NodeId>, position: Vec3| {
        let node_id = NodeId(next);
        next += 1;
        let mut n = Node::new(&name, ObjectKind::point(4.));
        n.id = node_id;
        n.parent = parent;
        n.transform.position = loom_scene::Param::new(position);
        n.wire_color = [60, 210, 255];
        changes.push(Change::AddNode(n));
        node_id
    };
    let root_world = state.scene.world_matrix(mapping.root);
    let master = helper(
        format!("LoomRig{id} Master"),
        None,
        root_world.transform_point3(Vec3::ZERO),
    );
    let mut bindings = vec![ControlBinding {
        bone: mapping.root,
        control: master,
        rest: root_world,
    }];
    let mut mapped = std::collections::BTreeSet::from([mapping.root]);
    for (name, bone) in std::iter::once(("Pelvis".to_string(), mapping.pelvis))
        .chain(
            mapping
                .spine
                .iter()
                .enumerate()
                .map(|(i, b)| (format!("Spine {}", i + 1), *b)),
        )
        .chain(std::iter::once(("Head".to_string(), mapping.head)))
        .chain(mapping.limbs.iter().flat_map(|l| {
            [
                (format!("{} FK Upper", l.name), l.start),
                (format!("{} FK Lower", l.name), l.middle),
                (format!("{} FK End", l.name), l.end),
            ]
        }))
    {
        if mapped.insert(bone) {
            let c = helper(format!("LoomRig{id} {name}"), Some(bone), Vec3::ZERO);
            bindings.push(ControlBinding {
                bone,
                control: c,
                rest: Mat4::IDENTITY,
            });
        }
    }
    let mut limbs = vec![];
    for l in &mapping.limbs {
        let root = state
            .scene
            .world_matrix(l.start)
            .transform_point3(Vec3::ZERO);
        let mid = state
            .scene
            .world_matrix(l.middle)
            .transform_point3(Vec3::ZERO);
        let end = state.scene.world_matrix(l.end).transform_point3(Vec3::ZERO);
        let axis = (end - root).normalize_or_zero();
        let mut bend = mid - root - axis * (mid - root).dot(axis);
        if bend.length_squared() < 1e-8 {
            bend = Vec3::Y - axis * Vec3::Y.dot(axis);
            if bend.length_squared() < 1e-8 {
                bend = Vec3::Z - axis * Vec3::Z.dot(axis);
            }
        }
        let pole_pos =
            mid + bend.normalize_or_zero() * (root.distance(mid) + mid.distance(end)).max(1.);
        let goal = helper(format!("LoomRig{id} {} Goal", l.name), None, end);
        let pole = helper(format!("LoomRig{id} {} Pole", l.name), None, pole_pos);
        limbs.push(RigLimb {
            mapping: l.clone(),
            goal,
            pole,
            goal_rest: end,
            pole_rest: pole_pos,
            solver: IkSolver::Limb,
            blend: loom_scene::Param::new(0.),
            pose_offsets: vec![],
            contacts: Default::default(),
        });
    }
    drop(helper);
    for change in &mut changes {
        if let Change::AddNode(n) = change {
            if n.id == master {
                n.transform = loom_scene::Transform::from_matrix(root_world);
            } else if let Some(l) = limbs.iter().find(|l| l.goal == n.id) {
                n.transform.rotation =
                    loom_scene::Transform::from_matrix(state.scene.world_matrix(l.mapping.end))
                        .rotation;
            }
        }
    }
    Ok((
        LoomRig {
            id,
            name: format!("LoomRig{id}"),
            mapping,
            master,
            bindings,
            limbs,
        },
        changes,
    ))
}
pub fn execute(state: &mut AppState, cmd: Command) -> Result<Value, String> {
    match cmd {
        Command::CreateLoomRig { root, mapping } => {
            let mapping = match mapping {
                Some(m) => {
                    if m.root != NodeId(root) {
                        return Err("mapping root differs from selected root".into());
                    }
                    m
                }
                None => HumanoidMapping::metahuman(&state.scene, NodeId(root))?,
            };
            if state
                .scene
                .loom_rigs
                .iter()
                .any(|r| r.mapping.root == mapping.root)
            {
                return Err("this skeleton already has a LoomRig".into());
            }
            let id = state
                .scene
                .loom_rigs
                .iter()
                .map(|r| r.id)
                .max()
                .unwrap_or(0)
                + 1;
            let (rig, mut changes) = build(state, id, mapping, state.scene.next_id())?;
            let before = state.scene.loom_rigs.clone();
            let mut after = before.clone();
            after.push(rig);
            changes.push(Change::SetLoomRigs { before, after });
            commit(state, "Create LoomRig", Change::Group(changes))?;
            Ok(json!({"rig":id}))
        }
        Command::RemoveLoomRig { rig } => {
            let before = state.scene.loom_rigs.clone();
            let old = before
                .iter()
                .find(|r| r.id == rig)
                .ok_or("no LoomRig with that id")?;
            let mut changes: Vec<_> = controls(old).into_iter().map(Change::RemoveNode).collect();
            let after = before.iter().filter(|r| r.id != rig).cloned().collect();
            changes.push(Change::SetLoomRigs { before, after });
            commit(state, "Remove LoomRig", Change::Group(changes))?;
            Ok(json!({}))
        }
        Command::SetLoomRigMapping { rig, mapping } => {
            let before = state.scene.loom_rigs.clone();
            let index = before
                .iter()
                .position(|r| r.id == rig)
                .ok_or("no LoomRig with that id")?;
            if mapping.root != before[index].mapping.root {
                return Err("changing an authored rig's root requires a separate rig".into());
            }
            let (mut new, proposed) = build(state, rig, mapping, state.scene.next_id())?;
            let old = &before[index];
            let proposed_nodes: std::collections::BTreeMap<_, _> = proposed
                .iter()
                .filter_map(|c| {
                    if let Change::AddNode(n) = c {
                        Some((n.id, n))
                    } else {
                        None
                    }
                })
                .collect();
            let mut reused = std::collections::BTreeMap::new();
            reused.insert(new.master, old.master);
            for binding in &mut new.bindings {
                let candidate =
                    old.bindings
                        .iter()
                        .find(|b| b.bone == binding.bone)
                        .or_else(|| {
                            old.bindings.iter().find(|b| {
                                state.scene.node(b.control).is_some_and(|n| {
                                    n.name == proposed_nodes[&binding.control].name
                                })
                            })
                        });
                if let Some(existing) = candidate {
                    reused.insert(binding.control, existing.control);
                    binding.rest = existing.rest;
                }
            }
            for limb in &mut new.limbs {
                if let Some(existing) = old
                    .limbs
                    .iter()
                    .find(|l| l.mapping.name == limb.mapping.name)
                {
                    reused.insert(limb.goal, existing.goal);
                    reused.insert(limb.pole, existing.pole);
                    let mapping = limb.mapping.clone();
                    *limb = existing.clone();
                    limb.mapping = mapping;
                }
            }
            let kept: std::collections::BTreeSet<_> = reused.values().copied().collect();
            if controls(old).iter().any(|id| !kept.contains(id)) {
                return Err("mapping edit would discard existing controls; keep their bone/role and limb names or create a separate rig".into());
            }
            let mut changes = vec![];
            for c in proposed {
                if let Change::AddNode(n) = c {
                    if let Some(existing) = reused.get(&n.id) {
                        let current = state
                            .scene
                            .node(*existing)
                            .ok_or("rig control is missing")?;
                        if current.parent != n.parent {
                            changes.push(Change::SetParent {
                                id: *existing,
                                before: current.parent,
                                after: n.parent,
                            });
                        }
                    } else {
                        changes.push(Change::AddNode(n));
                    }
                }
            }
            new.master = old.master;
            new.name = old.name.clone();
            for binding in &mut new.bindings {
                if let Some(id) = reused.get(&binding.control) {
                    binding.control = *id;
                }
            }
            for limb in &mut new.limbs {
                if let Some(id) = reused.get(&limb.goal) {
                    limb.goal = *id;
                }
                if let Some(id) = reused.get(&limb.pole) {
                    limb.pole = *id;
                }
            }
            let mut after = before.clone();
            after[index] = new;
            changes.push(Change::SetLoomRigs { before, after });
            commit(state, "Set LoomRig Mapping", Change::Group(changes))?;
            Ok(json!({}))
        }
        Command::SetLoomRigLimb {
            rig,
            limb,
            solver,
            blend,
        } => {
            if !blend.is_finite() || !(0.0..=1.0).contains(&blend) {
                return Err("IK/FK blend must be between zero and one".into());
            }
            let solver = IkSolver::parse(&solver)
                .filter(|s| *s != IkSolver::Spline)
                .ok_or("LoomRig limb solver must be hi or limb")?;
            let before = state.scene.loom_rigs.clone();
            let ri = before
                .iter()
                .position(|r| r.id == rig)
                .ok_or("no LoomRig with that id")?;
            let li = before[ri]
                .limbs
                .iter()
                .position(|l| l.mapping.name == limb)
                .ok_or("no limb with that name")?;
            let l = &before[ri].limbs[li];
            let ids = [l.mapping.start, l.mapping.middle, l.mapping.end];
            let visible: Vec<_> = ids
                .iter()
                .map(|id| {
                    state
                        .scene
                        .local_matrix(*id)
                        .to_scale_rotation_translation()
                        .1
                })
                .collect();
            let mut after = before.clone();
            after[ri].limbs[li].solver = solver;
            after[ri].limbs[li].blend.set(blend);
            let mut probe_after = after.clone();
            probe_after[ri].limbs[li].pose_offsets.clear();
            probe_after[ri].limbs[li].blend = loom_scene::Param::new(0.);
            let mut probe = state.scene.clone();
            Change::SetLoomRigs {
                before: before.clone(),
                after: probe_after,
            }
            .apply(&mut probe)?;
            while after[ri].limbs[li].pose_offsets.len() < ids.len() {
                after[ri].limbs[li]
                    .pose_offsets
                    .push(loom_scene::Param::new(Vec3::ZERO));
            }
            for (i, (id, old)) in ids.iter().zip(visible).enumerate() {
                let delta = (probe
                    .local_matrix(*id)
                    .to_scale_rotation_translation()
                    .1
                    .inverse()
                    * old)
                    .normalize();
                let (x, y, z) = delta.to_euler(glam::EulerRot::XYZ);
                let mut value = Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees());
                let previous = after[ri].limbs[li].pose_offsets[i].get();
                for axis in 0..3 {
                    value[axis] += 360. * ((previous[axis] - value[axis]) / 360.).round();
                }
                after[ri].limbs[li].pose_offsets[i].set(value);
            }
            let limb = &before[ri].limbs[li];
            let end_world = state.scene.world_matrix(limb.mapping.end);
            let mut changes = vec![];
            let start = state
                .scene
                .world_matrix(limb.mapping.start)
                .transform_point3(Vec3::ZERO);
            let middle = state
                .scene
                .world_matrix(limb.mapping.middle)
                .transform_point3(Vec3::ZERO);
            let end = end_world.transform_point3(Vec3::ZERO);
            let axis = (end - start).normalize_or_zero();
            let mut bend = middle - start - axis * (middle - start).dot(axis);
            if bend.length_squared() < 1e-8 {
                bend = state
                    .scene
                    .world_matrix(limb.pole)
                    .transform_point3(Vec3::ZERO)
                    - start;
                bend -= axis * bend.dot(axis);
            }
            let pole_world = middle
                + bend.normalize_or_zero()
                    * (start.distance(middle) + middle.distance(end)).max(1.);
            for (id, target) in [(limb.goal, end), (limb.pole, pole_world)] {
                let node = state.scene.node(id).ok_or("rig control is missing")?;
                let frame = state.scene.parent_frame(id);
                let mut transform = node.transform.clone();
                transform
                    .position
                    .set(frame.inverse().transform_point3(target));
                if id == limb.goal {
                    transform.set_rotation_quat(
                        frame.to_scale_rotation_translation().1.inverse()
                            * end_world.to_scale_rotation_translation().1,
                    );
                }
                changes.push(Change::SetTransform {
                    id,
                    before: node.transform.clone(),
                    after: transform,
                });
            }
            changes.push(Change::SetLoomRigs { before, after });
            commit(state, "Set LoomRig Limb", Change::Group(changes))?;
            Ok(json!({}))
        }
        _ => Err("not a LoomRig command".into()),
    }
}
