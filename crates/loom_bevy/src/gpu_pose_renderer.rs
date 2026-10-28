//! Experimental renderer integration for internal native comparison runs.
use super::*;
use crate::gpu_pose_commit::{commit_pose, GuardReadback, PoseCandidate, PoseCommitError, PoseBounds};
use crate::gpu_pose_geometry::mikk_prepare::GpuPreparation;
use crate::gpu_pose_runtime::GpuPoseRuntime;
use crate::gpu_pose_geometry::mikk_recipe::GpuRecipeMath;
use bevy::render::primitives::Aabb;

/// All source vertices of one successfully committed current world-space pose.
#[derive(Clone)]
pub struct GpuPoseSource {
    pub id: u64,
    pub positions: bevy::render::render_resource::Buffer,
    pub vertex_count: usize,
}

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
    precision: Option<std::sync::Arc<crate::gpu_mikk_precision::PrecisionProgram>>,
    reader: Option<(usize, GuardReadback)>,
    current_sources: HashSet<(bool, u64)>,
    last_error: Option<String>,
    pub(super) recovered_recipes: usize,
}

impl BevyRenderer {
    pub fn gpu_pose_sources(&self) -> Vec<GpuPoseSource> {
        let Some(state) = &self.gpu_poses else { return Vec::new() };
        state.current_sources.iter().filter(|(hidden, _)| !hidden).map(|key| {
            let positions = state.cache[key].source_buffer();
            GpuPoseSource { id: key.1, vertex_count: positions.size() as usize / 16, positions }
        }).collect()
    }

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

/// Sorted material IDs of visible faces over the complete u16 ID range.
fn visible_material_ids(faces: &[loom_mesh::Face]) -> Vec<Option<u16>> {
    let mut bits = [0u64; 1024];
    for face in faces {
        if !face.hidden {
            bits[face.material_id as usize / 64] |= 1u64 << (face.material_id % 64);
        }
    }
    let mut ids = Vec::new();
    for (word, mut set) in bits.into_iter().enumerate() {
        while set != 0 {
            let bit = set.trailing_zeros();
            ids.push(Some((word * 64 + bit as usize) as u16));
            set &= set - 1;
        }
    }
    ids
}

impl GpuPoseState {
    pub(super) fn selected(device: &wgpu::Device, value: Option<&std::ffi::OsStr>) -> Option<Self> {
        match value.and_then(|v| v.to_str()) {
            Some("1") => Some(Self::default()),
            Some("precise8") => match crate::gpu_mikk_precision::PrecisionProgram::new(device) {
                Ok(program) => Some(Self { precision: Some(std::sync::Arc::new(program)), ..Default::default() }),
                Err(error) => { eprintln!("precise8 unavailable; using CPU geometry: {error}"); None }
            },
            _ => None,
        }
    }

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
        self.current_sources.retain(|(h, _)| *h != hidden);
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
            let part_ids: Vec<_> = {
                let _scope = loom_render::profile::Scope::new("gpu_pose.material_ids");
                if matches!(source.surfaces, Some(Surfaces::PerId(_))) {
                    visible_material_ids(source.mesh.faces())
                } else {
                    vec![None]
                }
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
                    GpuPoseRuntime::with_precision(device, queue, source.mesh, &descriptors, self.precision.clone())?,
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
        let maths: Vec<_> = eligible.iter()
            .flat_map(|id| self.cache[&(hidden, *id)].maths())
            .collect();
        GpuRecipeMath::encode_batch(&mut encoder, &maths);
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
            .zip(bounds.parts)
            .map(|((id, part), bounds)| (id, part, bounds))
            .collect();
        self.current_sources.extend(eligible.iter().map(|id| (hidden, *id)));
        Ok((updates, Some((bounds.min, bounds.max))))
    }

    fn commit(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        set: &mut HashMap<u64, Object>,
        hidden: bool,
        eligible: &HashSet<u64>,
    ) -> Result<(Result<PoseBounds, PoseCommitError>, Vec<(u64, usize)>), String> {
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
    fn precise_selection_on_device_without_f64_keeps_cpu_backend() {
        let instance = wgpu::Instance::new(&Default::default());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        let mut descriptor = crate::device_descriptor(&adapter);
        descriptor.required_features.remove(wgpu::Features::SHADER_F64);
        let (device, queue) = pollster::block_on(adapter.request_device(&descriptor, None)).unwrap();
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        assert!(GpuPoseState::selected(&device, Some(std::ffi::OsStr::new("precise8"))).is_none());
        assert!(pollster::block_on(device.pop_error_scope()).is_none());
        assert!(GpuPoseState::selected(&device, Some(std::ffi::OsStr::new("1"))).is_some());
        assert!(GpuPoseState::selected(&device, None).is_none());
        assert!(GpuPoseState::selected(&device, Some(std::ffi::OsStr::new("other"))).is_none());
        let selection = GpuPoseState::selected(&device, Some(std::ffi::OsStr::new("precise8")));
        let mut renderer = BevyRenderer::new(adapter, device, queue);
        renderer.gpu_poses = selection;
        let mesh = loom_mesh::box_primitive(Vec3::ONE);
        let objects = [SceneObject { id: 1, mesh: &mesh, transform: glam::Mat4::IDENTITY, wire_color: [128; 3], surfaces: None, renderable: true, flags: Default::default() }];
        renderer.sync_deforming(&objects, &HashSet::from([1]));
        renderer.sync_deforming(&objects, &HashSet::from([1]));
        assert!(renderer.gpu_pose_status().is_none());
        assert!(!renderer.objects[&1].gpu_pose_current);
        assert!(!renderer.objects[&1].buffers.is_empty(), "ordinary CPU geometry remains rendered");
    }

    #[test]
    fn precise_selection_on_low_device_limits_keeps_cpu_backend() {
        let instance = wgpu::Instance::new(&Default::default());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        for case in 0..4 {
            let mut descriptor = crate::device_descriptor(&adapter);
            match case {
                0 => descriptor.required_limits.max_bindings_per_bind_group = 9,
                1 => descriptor.required_limits.max_compute_workgroup_size_x = 63,
                2 => descriptor.required_limits.max_compute_invocations_per_workgroup = 63,
                _ => descriptor.required_limits.max_storage_buffers_per_shader_stage = 8,
            }
            let (device, _) = pollster::block_on(adapter.request_device(&descriptor, None)).unwrap();
            device.push_error_scope(wgpu::ErrorFilter::Validation);
            assert!(GpuPoseState::selected(&device, Some(std::ffi::OsStr::new("precise8"))).is_none(), "low limit case={case}");
            assert!(pollster::block_on(device.pop_error_scope()).is_none(), "shader/layout creation must not be attempted for case={case}");
        }
    }

    #[test]
    fn failed_current_readback_withdraws_exported_source_handles() {
        failed_current_readback(false);
    }
    #[test]
    fn precise_failed_current_readback_withdraws_exported_source_handles() {
        failed_current_readback(true);
    }
    fn failed_current_readback(precise: bool) {
        let instance = wgpu::Instance::new(&Default::default());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        let (device, queue) = pollster::block_on(adapter.request_device(&crate::device_descriptor(&adapter), None)).unwrap();
        let mut renderer = BevyRenderer::new(adapter, device, queue);
        if precise {
            renderer.gpu_poses = GpuPoseState::selected(&renderer.device, Some(std::ffi::OsStr::new("precise8")));
            assert!(renderer.gpu_poses.as_ref().unwrap().precision.is_some());
        } else { renderer.enable_gpu_poses(); }
        let mesh = loom_mesh::box_primitive(Vec3::ONE);
        let objects = [SceneObject { id: 1, mesh: &mesh, transform: glam::Mat4::IDENTITY, wire_color: [128; 3], surfaces: None, renderable: true, flags: Default::default() }];
        let deforming = HashSet::from([1]);
        renderer.sync_deforming(&objects, &deforming);
        renderer.sync_deforming(&objects, &deforming);
        assert_eq!(renderer.gpu_pose_sources().len(), 1);
        // Inject a real guard-reader range mismatch before the next commit.
        renderer.gpu_poses.as_mut().unwrap().reader = Some((1, GuardReadback::new(&renderer.device, 2).unwrap()));
        renderer.sync_deforming(&objects, &deforming);
        assert!(renderer.gpu_pose_sources().is_empty());
        assert!(renderer.gpu_pose_status().unwrap().last_error.is_some());
        renderer.gpu_poses.as_mut().unwrap().reader = None;
        renderer.sync_deforming(&objects, &deforming);
        assert_eq!(renderer.gpu_pose_sources().len(), 1);
    }

    #[test]
    #[ignore = "actual full Ray scene GPU renderer integration requires LOOM_PERF_SCENE"]
    fn actual_ray_all_thirteen_parts_keep_current_pose_and_resources() {
        current_renderer_resources(false);
    }
    #[test]
    #[ignore = "actual full Ray scene precise8 renderer integration requires LOOM_PERF_SCENE"]
    fn actual_ray_precise_all_thirteen_parts_keep_current_pose_and_resources() {
        current_renderer_resources(true);
    }
    fn current_renderer_resources(precise: bool) {
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
        if precise {
            renderer.gpu_poses = GpuPoseState::selected(&renderer.device, Some(std::ffi::OsStr::new("precise8")));
            assert!(renderer.gpu_poses.as_ref().unwrap().precision.is_some());
        } else { renderer.enable_gpu_poses(); }
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
            if frame > 0 {
                let state = renderer.gpu_poses.as_ref().unwrap();
                let maths: Vec<_> = state.cache.values().flat_map(|runtime| runtime.maths()).collect();
                assert_eq!(maths.len(), 13);
                let batched: Vec<_> = maths.iter().map(|math| math.readback(&renderer.device, &renderer.queue).unwrap()).collect();
                let mut encoder = renderer.device.create_command_encoder(&Default::default());
                for math in &maths { math.encode(&mut encoder); }
                renderer.queue.submit([encoder.finish()]);
                for (math, batched) in maths.into_iter().zip(batched) {
                    let serial = math.readback(&renderer.device, &renderer.queue).unwrap();
                    assert_eq!(bytemuck::cast_slice::<_, u8>(&batched), bytemuck::cast_slice::<_, u8>(&serial), "batch differs from serial at{time}");
                }
            }
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

#[cfg(test)]
mod material_id_tests {
    use super::*;
    fn reference(faces: &[loom_mesh::Face]) -> Vec<Option<u16>> {
        faces.iter().filter(|f| !f.hidden).map(|f| f.material_id)
            .collect::<std::collections::BTreeSet<_>>().into_iter().map(Some).collect()
    }
    fn face(id: u16, hidden: bool) -> loom_mesh::Face {
        let mut face = loom_mesh::Face::new(loom_mesh::HalfEdgeId(0));
        face.material_id = id;
        face.hidden = hidden;
        face
    }
    #[test]
    fn material_ids_cover_boundaries_duplicates_and_hidden_faces() {
        for faces in [vec![], vec![face(65535, false), face(64, false), face(63, false),
            face(0, false), face(1, true), face(64, true), face(63, false)]] {
            assert_eq!(visible_material_ids(&faces), reference(&faces));
        }
    }
    #[test]
    fn material_ids_cover_every_u16_with_independent_hidden_patterns() {
        for pattern in 0..4 {
            let faces: Vec<_> = (0..=u16::MAX).rev().flat_map(|id| {
                let hidden = match pattern { 0 => false, 1 => true, 2 => id % 3 == 0, _ => id % 2 == 0 };
                [face(id, hidden), face(id, true)]
            }).collect();
            assert_eq!(visible_material_ids(&faces), reference(&faces));
        }
    }
    #[test]
    #[ignore = "actual saved Ray scene requires LOOM_PERF_SCENE"]
    fn material_ids_match_actual_ray_evaluated_meshes() {
        let path = std::env::var("LOOM_PERF_SCENE").expect("actual saved Ray scene");
        let document: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let scene: loom_scene::Scene = serde_json::from_value(document["scene"].clone()).unwrap();
        let ids: Vec<_> = scene.nodes().filter(|n| n.modifiers.iter().any(|m|
            matches!(m.modifier, loom_scene::Modifier::Skin(_)))).map(|n| n.id).collect();
        assert_eq!(ids.len(), 5);
        for time in [0., 39., 149.] {
            scene.set_time(time);
            for id in &ids {
                let mesh = scene.eval_mesh(*id);
                assert_eq!(visible_material_ids(mesh.value.mesh.faces()), reference(mesh.value.mesh.faces()));
            }
        }
    }
}
