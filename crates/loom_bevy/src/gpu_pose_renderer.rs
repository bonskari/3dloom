//! Experimental renderer integration for internal native comparison runs.
use super::*;
use crate::gpu_pose_commit::{commit_pose, GuardReadback, PoseCandidate, PoseCommitError};
use crate::gpu_pose_geometry::mikk_prepare::GpuPreparation;
use crate::gpu_pose_runtime::GpuPoseRuntime;
use bevy::render::primitives::Aabb;

/// Observed state of the opt-in internal geometry backend.
pub struct GpuPoseStatus<'a> {
    pub committed_objects: usize,
    pub committed_parts: usize,
    pub recovered_recipes: usize,
    pub last_error: Option<&'a str>,
}

#[derive(Default)]
pub(super) struct GpuPoseState {
    cache: HashMap<(bool, u64), GpuPoseRuntime>,
    reader: Option<(usize, GuardReadback)>,
    last_error: Option<String>,
    pub(super) recovered_recipes: usize,
}

impl BevyRenderer {
    pub fn gpu_pose_status(&self) -> Option<GpuPoseStatus<'_>> {
        let state = self.gpu_poses.as_ref()?;
        let objects: Vec<_> = self
            .objects
            .values()
            .filter(|o| o.gpu_pose_current)
            .collect();
        Some(GpuPoseStatus {
            committed_objects: objects.len(),
            committed_parts: objects.iter().map(|o| o.buffers.len()).sum(),
            recovered_recipes: state.recovered_recipes,
            last_error: state.last_error.as_deref(),
        })
    }

    #[cfg(test)]
    pub(crate) fn enable_gpu_poses(&mut self) {
        self.gpu_poses = Some(GpuPoseState::default());
    }

    pub(super) fn try_gpu_poses(
        &mut self,
        set: &mut HashMap<u64, Object>,
        objects: &[SceneObject],
        hidden: bool,
        deforming: &HashSet<u64>,
    ) -> (HashSet<u64>, Option<(Vec3, Vec3)>) {
        let Some(mut state) = self.gpu_poses.take() else {
            return (HashSet::new(), None);
        };
        let result = state.update(&self.device, &self.queue, set, objects, hidden, deforming);
        let answer = match result {
            Ok((updates, bounds)) => {
                let mut ids = HashSet::new();
                for (id, part, aabb) in updates {
                    let object = set.get_mut(&id).unwrap();
                    object.gpu_pose_current = true;
                    let entity = object.parts[part].0;
                    let source = objects.iter().find(|o| o.id == id).unwrap();
                    let layers = self.object_layers(id, source.renderable, source.flags, hidden);
                    self.app
                        .world_mut()
                        .entity_mut(entity)
                        .insert((aabb, layers));
                    ids.insert(id);
                }
                if !ids.is_empty() {
                    self.settle = self.settle.max(3);
                }
                state.last_error = None;
                (ids, bounds)
            }
            Err(error) => {
                if state.last_error.as_ref() != Some(&error) {
                    eprintln!("GPU pose not committed; evaluating current CPU geometry: {error}");
                }
                state.last_error = Some(error);
                (HashSet::new(), None)
            }
        };
        self.gpu_poses = Some(state);
        answer
    }
}

impl GpuPoseState {
    fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        set: &mut HashMap<u64, Object>,
        objects: &[SceneObject],
        hidden: bool,
        deforming: &HashSet<u64>,
    ) -> Result<(Vec<(u64, usize, Aabb)>, Option<(Vec3, Vec3)>), String> {
        let _scope = loom_render::profile::Scope::new("gpu_pose.renderer");
        let seen: HashSet<_> = objects.iter().map(|o| o.id).collect();
        self.cache
            .retain(|(h, id), _| *h != hidden || seen.contains(id));
        let mut eligible = HashSet::new();
        for source in objects {
            let Some(object) = set.get(&source.id).filter(|o| {
                o.dynamic
                    && !o.material_dirty
                    && o.surfaces == source.surfaces
                    && o.wire_color == source.wire_color
                    && o.renderable == source.renderable
                    && o.flags == source.flags
                    && o.transform == source.transform
                    && !o.buffers.is_empty()
            }) else {
                continue;
            };
            if !deforming.contains(&source.id) || source.transform != glam::Mat4::IDENTITY {
                continue;
            }
            let part_ids: Vec<_> = if matches!(source.surfaces, Some(Surfaces::PerId(_))) {
                source
                    .mesh
                    .faces()
                    .iter()
                    .filter(|f| !f.hidden)
                    .map(|f| f.material_id)
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .map(Some)
                    .collect()
            } else {
                vec![None]
            };
            if part_ids != object.part_ids || part_ids.len() != object.buffers.len() {
                continue;
            }
            let descriptors: Vec<_> = part_ids
                .iter()
                .map(|id| {
                    (
                        *id,
                        source
                            .surfaces
                            .as_ref()
                            .is_some_and(|s| s.for_face(id.unwrap_or(0)).faceted),
                    )
                })
                .collect();
            let key = (hidden, source.id);
            if let Some(runtime) = self.cache.get_mut(&key) {
                runtime.update(device, queue, source.mesh, &descriptors)?;
            } else {
                self.cache.insert(
                    key,
                    GpuPoseRuntime::new(device, queue, source.mesh, &descriptors)?,
                );
            }
            eligible.insert(source.id);
        }
        if eligible.is_empty() {
            return Ok((Vec::new(), None));
        }
        let encode_scope = loom_render::profile::Scope::new("gpu_pose.encode_submit");
        let mut encoder = device.create_command_encoder(&Default::default());
        for id in &eligible {
            self.cache[&(hidden, *id)].encode_normals(&mut encoder);
        }
        let preparations: Vec<_> = eligible
            .iter()
            .flat_map(|id| self.cache[&(hidden, *id)].preparations())
            .collect();
        GpuPreparation::encode_batch(&mut encoder, &preparations, None);
        for id in &eligible {
            self.cache[&(hidden, *id)].encode_math(&mut encoder);
        }
        queue.submit([encoder.finish()]);
        drop(encode_scope);
        let (mut result, mut order) = self.commit(device, queue, set, hidden, &eligible)?;
        if let Err(PoseCommitError::Guards(flags)) = &result {
            let rejected: HashSet<_> = order
                .iter()
                .zip(flags)
                .filter_map(|((id, _), flag)| (*flag != 0).then_some(*id))
                .collect();
            for id in rejected {
                self.recovered_recipes += self
                    .cache
                    .get_mut(&(hidden, id))
                    .unwrap()
                    .recover_guards(device, queue)?;
            }
            (result, order) = self.commit(device, queue, set, hidden, &eligible)?;
        }
        let bounds = result.map_err(|e| format!("{e:?}"))?;
        let updates = order
            .into_iter()
            .zip(bounds)
            .map(|((id, part), bounds)| (id, part, bounds))
            .collect();
        let _bounds_scope = loom_render::profile::Scope::new("gpu_pose.scene_bounds");
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        for id in &eligible {
            for candidate in self.cache[&(hidden, *id)].candidates() {
                for p in candidate.positions {
                    lo = lo.min(Vec3::from_array(*p));
                    hi = hi.max(Vec3::from_array(*p));
                }
            }
        }
        Ok((updates, Some((lo, hi))))
    }

    fn commit(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        set: &mut HashMap<u64, Object>,
        hidden: bool,
        eligible: &HashSet<u64>,
    ) -> Result<(Result<Vec<Aabb>, PoseCommitError>, Vec<(u64, usize)>), String> {
        let mut candidates = Vec::new();
        let mut order = Vec::new();
        for (id, object) in set.iter_mut().filter(|(id, _)| eligible.contains(id)) {
            let parts = self.cache[&(hidden, *id)].candidates();
            if parts.len() != object.buffers.len() {
                return Err("GPU material part count changed".into());
            }
            for (part, (target, source)) in object.buffers.iter_mut().zip(parts).enumerate() {
                order.push((*id, part));
                candidates.push(PoseCandidate {
                    target,
                    guard: source.guard,
                    output: source.output,
                    offset: 0,
                    positions: source.positions,
                    indices: Some(source.indices),
                });
            }
        }
        let count = candidates.len();
        if self.reader.as_ref().is_none_or(|(n, _)| *n != count) {
            self.reader = Some((count, GuardReadback::new(device, count)?));
        }
        Ok((
            commit_pose(
                device,
                queue,
                &mut self.reader.as_mut().unwrap().1,
                candidates,
            ),
            order,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "actual full Ray scene GPU renderer integration requires LOOM_PERF_SCENE"]
    fn actual_ray_all_thirteen_parts_keep_current_pose_and_resources() {
        let path = std::env::var("LOOM_PERF_SCENE").expect("actual saved Ray scene");
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let scene: loom_scene::Scene = serde_json::from_value(value["scene"].clone()).unwrap();
        let ids: Vec<_> = scene
            .nodes()
            .filter(|n| {
                n.modifiers
                    .iter()
                    .any(|m| matches!(&m.modifier, loom_scene::modifier::Modifier::Skin(_)))
            })
            .map(|n| n.id)
            .collect();
        assert_eq!(ids.len(), 5);
        let instance = wgpu::Instance::new(&Default::default());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        let (device, queue) =
            pollster::block_on(adapter.request_device(&crate::device_descriptor(&adapter), None))
                .unwrap();
        let mut renderer = BevyRenderer::new(adapter, device, queue);
        renderer.enable_gpu_poses();
        let deforming = ids.iter().map(|id| id.0).collect();
        let mut snapshots = HashMap::new();
        let mut readback = GuardReadback::new(&renderer.device, 13).unwrap();
        for (frame, time) in [0.0, 39.25, 95.71835, 149.0].into_iter().enumerate() {
            scene.set_time(time);
            let owned: Vec<_> = ids
                .iter()
                .map(|id| {
                    let mut mesh = scene.eval_mesh(*id).value.mesh.clone();
                    loom_mesh::edit_ops::transform_mesh(&mut mesh, scene.object_matrix(*id))
                        .unwrap();
                    (*id, mesh, scene.node_surfaces(*id))
                })
                .collect();
            let objects: Vec<_> = owned
                .iter()
                .map(|(id, mesh, surfaces)| SceneObject {
                    id: id.0,
                    mesh,
                    transform: glam::Mat4::IDENTITY,
                    wire_color: [128; 3],
                    surfaces: surfaces.clone(),
                    renderable: true,
                    flags: ObjectFlags::default(),
                })
                .collect();
            let started = std::time::Instant::now();
            renderer.sync_deforming(&objects, &deforming);
            eprintln!(
                "RAY_GPU_RENDERER time={time} sync_ms={:.3}",
                started.elapsed().as_secs_f64() * 1000.0
            );
            let mut expected = Vec::new();
            let mut buffers = Vec::new();
            for source in &objects {
                let object = &renderer.objects[&source.id];
                let handles: Vec<_> = object
                    .parts
                    .iter()
                    .zip(&object.buffers)
                    .map(|((entity, _), buffer)| (*entity, buffer.buffer().id()))
                    .collect();
                if frame == 0 {
                    snapshots.insert(source.id, handles);
                } else {
                    assert!(
                        object.gpu_pose_current,
                        "GPU pose was not committed for node{} at{time}",
                        source.id
                    );
                    assert_eq!(handles, snapshots[&source.id]);
                }
                let data = crate::mesh_data(source.mesh);
                for (id, buffer) in object.part_ids.iter().zip(&object.buffers) {
                    let part = crate::convert::mesh_data_part(source.mesh, &data, *id, false);
                    expected.push(part.positions[0][0].to_bits());
                    buffers.push(&**buffer.buffer());
                }
            }
            assert_eq!(buffers.len(), 13);
            assert_eq!(
                readback
                    .read(&renderer.device, &renderer.queue, &buffers)
                    .unwrap(),
                expected
            );
        }
        assert!(renderer.gpu_poses.as_ref().unwrap().last_error.is_none());
        let status = renderer.gpu_pose_status().unwrap();
        assert_eq!((status.committed_objects, status.committed_parts), (5, 13));
    }
}
