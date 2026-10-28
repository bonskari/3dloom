//! Scoped Skin jobs: scene access stays on the caller, mesh work can use its pool.
use super::{finish_modifier_value, Scene};
use crate::{anim, modifier::{Evaluated, StackValue}, skin::SkinData, Modifier, NodeId, ObjectKind};
use glam::Mat4;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

/// An independent Skin evaluation prepared by [`Scene::evaluated_batch`].
/// Run it on the caller's bounded worker pool; only immutable source data is shared.
pub struct SkinEvaluationJob<'a> {
    id: NodeId,
    key: u64,
    time: f32,
    rendering: bool,
    animated: bool,
    data: &'a SkinData,
    source: &'a crate::PolyObject,
    is_poly: bool,
    object_world: Mat4,
    worlds: Arc<HashMap<NodeId, (Mat4, bool, bool)>>,
}

/// A completed job. The originating scene publishes it to its ordinary cache.
pub struct SkinEvaluationResult {
    id: NodeId,
    key: u64,
    time: f32,
    rendering: bool,
    animated: bool,
    evaluated: Arc<Evaluated>,
}

impl SkinEvaluationJob<'_> {
    pub fn run(self) -> SkinEvaluationResult {
        let reads = anim::animated_reads();
        let evaluated = anim::with(self.time, false, || anim::with_rendering_state(self.rendering, || {
            let lookup = |id| self.worlds.get(&id).map(|v| v.0);
            let squash = |id| self.worlds.get(&id).is_some_and(|v| v.1);
            let ctx = crate::EvalCtx { object_world: self.object_world, object_inverse: self.object_world.inverse(), world_lookup: Some(&lookup), bone_squashes: Some(&squash), ..Default::default() };
            let value = StackValue {
                mesh: if self.is_poly { self.source.display_mesh().into_owned() } else { self.source.mesh.clone() },
                selection: self.source.passes_up.clone(),
                shape: None, shape_selection: None, warnings: Vec::new(), patch: None, patch_steps: None,
            };
            let counts = (value.mesh.vertex_count(), value.mesh.face_count());
            let mut value = crate::skin::apply_owned(self.data, value, &ctx);
            let mut warnings = Vec::new();
            finish_modifier_value(&mut value, &mut warnings, false, true, counts);
            Arc::new(Evaluated { value, warnings })
        }));
        SkinEvaluationResult { id: self.id, key: self.key, time: self.time, rendering: self.rendering, animated: self.animated || reads != anim::animated_reads(), evaluated }
    }
}

impl Scene {
    /// Evaluates full stacks in input order, preparing independent single-Skin
    /// stacks together. `execute` runs jobs on an existing bounded pool and
    /// returns their results in any order. Other stacks use `evaluated`.
    pub fn evaluated_batch<'a>(
        &'a self,
        ids: &[NodeId],
        execute: impl FnOnce(Vec<SkinEvaluationJob<'a>>) -> Vec<SkinEvaluationResult>,
    ) -> Vec<Arc<Evaluated>> {
        self.sync_time();
        // Attachment transforms can evaluate geometry and depend on the
        // serial evaluator's recursion guards; keep those scenes serial.
        let geometry_constraint = self.nodes.values().any(|n| matches!(&n.constraints.position, Some(crate::constraint::PositionConstraint::Attachment { .. })));
        if !self.eval_in_progress.borrow().is_empty() || geometry_constraint {
            return ids.iter().map(|id| self.evaluated(*id, None)).collect();
        }
        let time = self.time.get();
        let rendering = anim::rendering();
        let revision = self.revision();
        let mut ready = HashMap::new();
        let mut eligible = Vec::new();
        let mut bones = BTreeSet::new();
        for id in ids.iter().copied().collect::<BTreeSet<_>>() {
            let Some(node) = self.nodes.get(&id) else { continue };
            if node.instance_of.is_some() || node.modifiers.len() != 1 || !matches!(&node.object, ObjectKind::EditablePoly(_) | ObjectKind::EditableMesh(_)) { continue; }
            let slot = &node.modifiers[0];
            if !slot.enabled || !slot.enabled_in_viewport || slot.instance_of.is_some() { continue; }
            let Modifier::Skin(data) = &slot.modifier else { continue };
            let key = self.eval_key(id);
            if let Some((k, at, evaluated)) = self.eval_cache.borrow().get(&(id, None)) {
                if *k == key && at.is_none_or(|t| t == time) {
                    ready.insert(id, (evaluated.clone(), at.is_some()));
                    continue;
                }
            }
            bones.extend(data.bones.iter().map(|b| b.node));
            eligible.push((id, key, node, data.as_ref()));
        }
        let mut worlds = HashMap::with_capacity(bones.len());
        for bone in bones {
            let Some(node) = self.nodes.get(&bone) else { continue };
            let reads = anim::animated_reads();
            let world = self.world_matrix(bone);
            let squash = matches!(&node.object, ObjectKind::Bone { stretch, .. } if stretch.get() == 2);
            worlds.insert(bone, (world, squash, reads != anim::animated_reads()));
        }
        let worlds = Arc::new(worlds);
        let jobs = eligible.into_iter().map(|(id, key, node, data)| {
            let reads = anim::animated_reads();
            let object_world = self.object_matrix(id);
            let animated = reads != anim::animated_reads() || data.bones.iter().any(|b| worlds.get(&b.node).is_some_and(|v| v.2));
            let (source, is_poly) = match &node.object {
                ObjectKind::EditablePoly(p) => (p, true),
                ObjectKind::EditableMesh(p) => (p, false),
                _ => unreachable!("eligible mesh object"),
            };
            SkinEvaluationJob { id, key, time, rendering, animated, data, source, is_poly, object_world, worlds: worlds.clone() }
        }).collect();
        let results = execute(jobs);
        self.sync_time();
        // Nodes cannot be edited through this shared borrow. The executor can
        // still change ambient time/render mode or bump the scene revision.
        let unchanged = self.revision() == revision && self.time.get() == time && anim::rendering() == rendering;
        if unchanged {
            for result in results {
                if result.time != time || result.rendering != rendering { continue; }
                self.eval_count.set(self.eval_count.get() + 1);
                ready.insert(result.id, (result.evaluated.clone(), result.animated));
                self.eval_cache.borrow_mut().insert((result.id, None), (result.key, result.animated.then_some(result.time), result.evaluated));
            }
        } else {
            ready.clear();
        }
        ids.iter().map(|id| {
            if let Some((evaluated, animated)) = ready.get(id) {
                if *animated { anim::note_animated_read(); }
                evaluated.clone()
            } else {
                self.evaluated(*id, None)
            }
        }).collect()
    }
}

#[cfg(test)]
#[path = "skin_batch_tests.rs"]
mod tests;
