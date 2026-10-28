//! Unwired current-world-position GPU geometry orchestration.
use crate::gpu_pose_commit::GuardReadback;
use crate::gpu_pose_geometry::{
    mikk_prepare::{GpuPreparation, Program},
    mikk_recipe::GpuRecipeMath,
    GpuPoseGeometry,
};
use crate::MeshData;
use loom_mesh::{FaceId, Mesh};
use wgpu::util::DeviceExt;

fn fused_positions_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("LOOM_EXPERIMENTAL_FUSED_POSE_POSITIONS").as_deref().is_none_or(|value| value == std::ffi::OsStr::new("1")))
}

pub(crate) struct Candidate<'a> {
    pub output: &'a wgpu::Buffer,
    pub guard: &'a wgpu::Buffer,
    pub positions: &'a [[f32; 3]],
    pub prepared: Option<&'a mut crate::dynamic_geometry::PreparedPositions>,
    pub indices: &'a [u32],
}
struct Part {
    indices: Vec<u32>,
    positions: Vec<[f32; 3]>,
    prepared: crate::dynamic_geometry::PreparedPositions,
    preparation: GpuPreparation,
    math: Option<GpuRecipeMath>,
}
pub(crate) struct GpuPoseRuntime {
    fused: bool,
    source: bevy::render::render_resource::Buffer,
    geometry: GpuPoseGeometry,
    parts: Vec<Part>,
    guards: GuardReadback,
    precision: Option<std::sync::Arc<crate::gpu_mikk_precision::PrecisionProgram>>,
}
impl GpuPoseRuntime {
    /// Mesh positions are already the final CPU world-baked pose.
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &Mesh,
        descriptors: &[(Option<u16>, bool)],
    ) -> Result<Self, String> {
        Self::with_precision(device, queue, mesh, descriptors, None)
    }
    pub(crate) fn with_precision(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &Mesh,
        descriptors: &[(Option<u16>, bool)],
        precision: Option<std::sync::Arc<crate::gpu_mikk_precision::PrecisionProgram>>,
    ) -> Result<Self, String> {
        let positions = source_positions(mesh)?;
        let source_bytes = u64::try_from(positions.len()).ok().and_then(|n| n.checked_mul(16))
            .ok_or("current source position byte count overflow")?;
        let limits = device.limits();
        if source_bytes > limits.max_buffer_size
            || source_bytes > u64::from(limits.max_storage_buffer_binding_size)
        {
            return Err("current source positions exceed GPU buffer/storage limits".into());
        }
        let source: bevy::render::render_resource::Buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("current CPU world-baked source positions"),
            contents: bytemuck::cast_slice(&positions),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        }).into();
        let geometry = GpuPoseGeometry::from_current_positions(
            device,
            &source,
            mesh,
            descriptors,
            glam::Mat4::IDENTITY,
        )?;
        let indices = part_indices(mesh, descriptors)?;
        let program = Program::new(device)?;
        let mut parts = Vec::new();
        let count = geometry.corner_sources().len();
        for (range, indices) in geometry.parts.iter().zip(indices) {
            if indices.is_empty() {
                return Err("GPU runtime requires nonempty material parts".into());
            }
            let global: Vec<_> = indices.iter().map(|i| *i + range.start as u32).collect();
            parts.push(Part {
                indices,
                positions: Vec::new(),
                prepared: Default::default(),
                preparation: GpuPreparation::with_program(
                    device,
                    geometry.intermediate_buffer(),
                    count,
                    &global,
                    &program,
                )?,
                math: None,
            });
        }
        let guards = GuardReadback::new(device, parts.len())?;
        let mut runtime = Self {
            fused: fused_positions_enabled(),
            source,
            geometry,
            parts,
            guards,
            precision,
        };
        runtime.gather_positions(mesh)?;
        let mut encoder = device.create_command_encoder(&Default::default());
        runtime.encode_preparation(&mut encoder);
        queue.submit([encoder.finish()]);
        runtime.recompile(device, queue, &(0..runtime.parts.len()).collect::<Vec<_>>())?;
        Ok(runtime)
    }
    pub(crate) fn source_buffer(&self) -> bevy::render::render_resource::Buffer {
        self.source.clone()
    }
    /// Exact descriptor comparison; a changed topology/UV/normal/material layout
    /// creates a new candidate cache without modifying committed render geometry.
    pub(crate) fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &Mesh,
        descriptors: &[(Option<u16>, bool)],
    ) -> Result<bool, String> {
        let same = {
            let _scope = loom_render::profile::Scope::new("gpu_pose.descriptor_check");
            self.geometry.matches_descriptors(mesh, descriptors)?
        };
        if !same {
            *self = Self::with_precision(device, queue, mesh, descriptors, self.precision.clone())?;
            return Ok(true);
        }
        {
            let _scope = loom_render::profile::Scope::new("gpu_pose.source_upload");
            let positions = source_positions(mesh)?;
            queue.write_buffer(&self.source, 0, bytemuck::cast_slice(&positions));
        }
        self.gather_positions(mesh)?;
        Ok(false)
    }
    fn gather_positions(&mut self, mesh: &Mesh) -> Result<(), String> {
        let _scope = loom_render::profile::Scope::new("gpu_pose.corner_gather");
        for (part, range) in self.parts.iter_mut().zip(&self.geometry.parts) {
            let positions = self.geometry.corner_sources()[range.clone()].iter()
                .map(|i| mesh.vertices()[*i as usize].position.to_array());
            if self.fused { part.prepared.gather(positions)?; }
            else { part.positions.clear(); part.positions.extend(positions); }
        }
        Ok(())
    }
    pub(crate) fn encode_normals(&self, encoder: &mut wgpu::CommandEncoder) {
        self.geometry.encode(encoder);
    }
    pub(crate) fn preparations(&self) -> Vec<&GpuPreparation> {
        self.parts.iter().map(|p| &p.preparation).collect()
    }
    fn encode_preparation(&self, encoder: &mut wgpu::CommandEncoder) {
        self.encode_normals(encoder);
        GpuPreparation::encode_batch(encoder, &self.preparations(), None);
    }
    pub(crate) fn encode(&self, encoder: &mut wgpu::CommandEncoder) {
        self.encode_preparation(encoder);
        self.encode_math(encoder);
    }
    pub(crate) fn encode_math(&self, encoder: &mut wgpu::CommandEncoder) {
        for part in &self.parts {
            part.math
                .as_ref()
                .expect("compiled current recipe")
                .encode(encoder);
        }
    }
    pub(crate) fn maths(&self) -> Vec<&GpuRecipeMath> {
        self.parts.iter().map(|part| part.math.as_ref().expect("compiled current recipe")).collect()
    }
    /// Run only after the current normal/preparation submission. Rejected recipes
    /// read that SAME current GPU intermediate; no rest/CPU-final tangent input.
    pub(crate) fn recompile(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        rejected: &[usize],
    ) -> Result<(), String> {
        if rejected.iter().any(|i| *i >= self.parts.len()) {
            return Err("invalid rejected GPU part".into());
        }
        let records = self.geometry.readback(device, queue);
        for &i in rejected {
            let range = self.geometry.parts[i].clone();
            let current = &records[range.clone()];
            let part = &mut self.parts[i];
            let data = MeshData {
                positions: current
                    .iter()
                    .map(|v| v.position[..3].try_into().unwrap())
                    .collect(),
                normals: current
                    .iter()
                    .map(|v| v.normal[..3].try_into().unwrap())
                    .collect(),
                uvs: current
                    .iter()
                    .map(|v| v.uv[..2].try_into().unwrap())
                    .collect(),
                indices: part.indices.clone(),
            };
            let prepared = part.preparation.readback(device, queue)?;
            let structure = crate::mikktspace::compile_structure(&data);
            part.math = Some(GpuRecipeMath::with_precision(
                device,
                self.geometry.intermediate_buffer(),
                range.start as u32,
                &data,
                &part.preparation,
                &prepared,
                &structure,
                self.precision.as_deref(),
            )?);
        }
        let mut encoder = device.create_command_encoder(&Default::default());
        for &i in rejected {
            self.parts[i].math.as_ref().unwrap().encode(&mut encoder);
        }
        queue.submit([encoder.finish()]);
        Ok(())
    }
    /// A miss never authorizes commit. Recompile from current attributes, rerun
    /// math, and require every guard to pass; a second failure is returned.
    pub(crate) fn recover_guards(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<usize, String> {
        let flags = self.read_guards(device, queue)?;
        let rejected: Vec<_> = flags
            .iter()
            .enumerate()
            .filter_map(|(i, f)| (*f != 0).then_some(i))
            .collect();
        if rejected.is_empty() {
            return Ok(0);
        }
        self.recompile(device, queue, &rejected)?;
        let flags = self.read_guards(device, queue)?;
        if flags.iter().any(|f| *f != 0) {
            return Err(format!("fresh current GPU recipe rejected: {flags:?}"));
        }
        Ok(rejected.len())
    }
    fn read_guards(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<Vec<u32>, String> {
        let refs: Vec<_> = self
            .parts
            .iter()
            .map(|p| p.math.as_ref().unwrap().guard_buffer())
            .collect();
        self.guards.read(device, queue, &refs)
    }
    #[cfg(test)]
    pub(crate) fn uses_fused_positions(&self) -> bool { self.fused }
    pub(crate) fn commit_candidates(&mut self) -> Vec<Candidate<'_>> {
        let fused = self.fused;
        self.parts.iter_mut().map(|p| {
            let math = p.math.as_ref().unwrap();
            Candidate { output: math.output(), guard: math.guard_buffer(), positions: &p.positions,
                prepared: fused.then_some(&mut p.prepared), indices: &p.indices }
        }).collect()
    }
    #[cfg(test)]
    pub(crate) fn candidates(&self) -> Vec<Candidate<'_>> {
        self.parts
            .iter()
            .map(|p| {
                let math = p.math.as_ref().unwrap();
                Candidate {
                    output: math.output(),
                    guard: math.guard_buffer(),
                    positions: &p.positions,
                    prepared: None,
                    indices: &p.indices,
                }
            })
            .collect()
    }
}
fn source_positions(mesh: &Mesh) -> Result<Vec<[f32; 4]>, String> {
    if mesh.vertex_count() == 0 {
        return Err("GPU runtime requires nonempty source positions".into());
    }
    let mut values = Vec::with_capacity(mesh.vertex_count());
    for v in mesh.vertices() {
        if !v.position.is_finite() {
            return Err("nonfinite current world position".into());
        }
        values.push([v.position.x, v.position.y, v.position.z, 1.0]);
    }
    Ok(values)
}
fn part_indices(mesh: &Mesh, parts: &[(Option<u16>, bool)]) -> Result<Vec<Vec<u32>>, String> {
    parts
        .iter()
        .map(|(material, _)| {
            let mut indices = Vec::new();
            let mut base = 0u32;
            for (i, _) in mesh
                .faces()
                .iter()
                .enumerate()
                .filter(|(_, f)| !f.hidden && material.is_none_or(|id| id == f.material_id))
            {
                let count = mesh
                    .face_vertex_ids(FaceId(i as u32))
                    .ok_or("invalid GPU face loop")?
                    .len() as u32;
                if count < 3 {
                    continue;
                }
                for corner in 1..count - 1 {
                    indices.extend_from_slice(&[base, base + corner, base + corner + 1]);
                }
                base += count;
            }
            Ok(indices)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn fused_positions_are_enabled_by_default() {
        assert!(std::env::var_os("LOOM_EXPERIMENTAL_FUSED_POSE_POSITIONS").is_none());
        assert!(super::fused_positions_enabled());
    }
    use super::*;
    fn isolated_source_mesh(count: usize) -> Mesh {
        let mut indexed = loom_mesh::indexed::IndexedMesh::default();
        for i in 0..count {
            let id = indexed.add_vertex(glam::Vec3::new(i as f32, -(i as f32), 0.5), None);
            indexed.keep.insert(id);
        }
        indexed.build().unwrap()
    }
    fn reference_source_positions(mesh: &Mesh) -> Result<Vec<[f32;4]>,String> {
        if mesh.vertex_count() == 0 { return Err("GPU runtime requires nonempty source positions".into()); }
        mesh.vertices().iter().map(|v| if v.position.is_finite() {
            Ok([v.position.x,v.position.y,v.position.z,1.0])
        } else { Err("nonfinite current world position".into()) }).collect()
    }
    #[test]
    fn source_gpu_buffer_limits_reject_before_resource_creation() {
        let instance = wgpu::Instance::new(&Default::default());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        let mesh = isolated_source_mesh(17); //272 source bytes; no huge allocation.
        for buffer_limit in [true,false] {
            let mut descriptor = crate::device_descriptor(&adapter);
            descriptor.required_limits.max_storage_buffer_binding_size = 256;
            if buffer_limit { descriptor.required_limits.max_buffer_size = 256; }
            let (device,queue) = pollster::block_on(adapter.request_device(&descriptor,None)).unwrap();
            device.push_error_scope(wgpu::ErrorFilter::Validation);
            let error = GpuPoseRuntime::with_precision(&device,&queue,&mesh,&[(None,false)],None).err().expect("oversized source must be rejected");
            assert_eq!(error,"current source positions exceed GPU buffer/storage limits");
            assert!(pollster::block_on(device.pop_error_scope()).is_none(),"oversized source must not create invalid buffer/binding");
        }
    }
    #[test]
    fn source_positions_reserves_exact_vertex_count() {
        for count in [3,17] {
            let mesh = isolated_source_mesh(count);
            let values = source_positions(&mesh).unwrap();
            assert_eq!(values.len(),count);
            assert_eq!(values.capacity(),count,"known source count must not geometrically over-reserve");
        }
    }
    #[test]
    fn source_positions_preserves_every_float_bit_and_error_semantics() {
        let mut mesh = isolated_source_mesh(17);
        mesh.vertices_mut()[0].position = glam::Vec3::new(-0.0,f32::from_bits(1),f32::MAX);
        let actual=source_positions(&mesh).unwrap();let expected=reference_source_positions(&mesh).unwrap();
        assert_eq!(actual.len(),expected.len());
        for(a,b)in actual.iter().zip(&expected){assert_eq!(a.map(f32::to_bits),b.map(f32::to_bits));}
        assert_eq!(source_positions(&Mesh::new()).unwrap_err(),"GPU runtime requires nonempty source positions");
        for invalid in [f32::NAN,f32::INFINITY,f32::NEG_INFINITY] {
            for index in [0,8,16] { for component in 0..3 {
                let mut invalid_mesh=mesh.clone();invalid_mesh.vertices_mut()[index].position[component]=invalid;
                assert_eq!(source_positions(&invalid_mesh).unwrap_err(),reference_source_positions(&invalid_mesh).unwrap_err());
                assert_eq!(source_positions(&invalid_mesh).unwrap_err(),"nonfinite current world position");
            }}
        }
    }
    #[test]
    #[ignore = "requires frozen LOOM_PERF_SCENE; real five-mesh CPU source parity"]
    fn actual_ray_source_positions_reservation_matches_legacy_bits() {
        let value:serde_json::Value=serde_json::from_slice(&std::fs::read(std::env::var("LOOM_PERF_SCENE").unwrap()).unwrap()).unwrap();
        let scene:loom_scene::Scene=serde_json::from_value(value["scene"].clone()).unwrap();scene.set_time(39.0);
        let ids:Vec<_>=scene.nodes().filter(|n|n.modifiers.iter().any(|m|matches!(&m.modifier,loom_scene::modifier::Modifier::Skin(_)))).map(|n|n.id).collect();
        assert_eq!(ids.len(),5);
        let meshes:Vec<_>=ids.iter().map(|id|{let mut mesh=scene.eval_mesh(*id).value.mesh.clone();loom_mesh::edit_ops::transform_mesh(&mut mesh,scene.object_matrix(*id)).unwrap();mesh}).collect();
        let before=serde_json::to_vec(&scene).unwrap();let mut total=0;
        for mesh in &meshes {
            let actual=source_positions(mesh).unwrap();let expected=reference_source_positions(mesh).unwrap();
            assert_eq!(actual.len(),expected.len());assert_eq!(actual.capacity(),mesh.vertex_count());
            for(a,b)in actual.iter().zip(&expected){assert_eq!(a.map(f32::to_bits),b.map(f32::to_bits));}
            total+=actual.len();
        }
        assert!(before==serde_json::to_vec(&scene).unwrap(),"source packing must leave serialized Scene unchanged");
        eprintln!("ACTUAL_SOURCE_POSITIONS_BIT_PARITY frame=39 meshes={} vertices={total} scene_json_bytes_exact=PASS",meshes.len());
    }
    fn device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(&Default::default());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        pollster::block_on(adapter.request_device(&crate::device_descriptor(&adapter), None))
            .unwrap()
    }
    fn run(runtime: &mut GpuPoseRuntime, device: &wgpu::Device, queue: &wgpu::Queue) {
        let mut encoder = device.create_command_encoder(&Default::default());
        runtime.encode(&mut encoder);
        queue.submit([encoder.finish()]);
        runtime.recover_guards(device, queue).unwrap();
    }
    fn output(
        runtime: &GpuPoseRuntime,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Vec<Vec<u8>> {
        runtime
            .parts
            .iter()
            .map(|p| {
                bytemuck::cast_slice(&p.math.as_ref().unwrap().readback(device, queue).unwrap())
                    .to_vec()
            })
            .collect()
    }
    #[test]
    fn stage_batched_math_matches_serial_parts_for_current_poses() {
        let (device, queue) = device();
        let mut meshes: Vec<_> = [8, 12].into_iter().map(|segments| {
            let mut mesh = loom_mesh::sphere_segmented(1.0, segments, 0.0);
            loom_mesh::uvmap::auto_map(&mut mesh, loom_mesh::uvmap::AutoMap::Spherical, 1);
            mesh
        }).collect();
        let descriptors = [(None, false), (None, true)];
        let mut runtimes: Vec<_> = meshes.iter().map(|mesh|
            GpuPoseRuntime::new(&device, &queue, mesh, &descriptors).unwrap()
        ).collect();
        for step in 0..3 {
            for (mesh, runtime) in meshes.iter_mut().zip(&mut runtimes) {
                loom_mesh::edit_ops::translate_mesh(mesh, glam::Vec3::new(0.13, -0.07, step as f32 * 0.1)).unwrap();
                runtime.update(&device, &queue, mesh, &descriptors).unwrap();
                run(runtime, &device, &queue);
            }
            let expected: Vec<_> = runtimes.iter().map(|r| output(r, &device, &queue)).collect();
            let mut encoder = device.create_command_encoder(&Default::default());
            for runtime in &runtimes { runtime.encode_normals(&mut encoder); }
            let preparations: Vec<_> = runtimes.iter().flat_map(|r| r.preparations()).collect();
            GpuPreparation::encode_batch(&mut encoder, &preparations, None);
            let maths: Vec<_> = runtimes.iter().flat_map(|r| r.parts.iter().map(|p| p.math.as_ref().unwrap())).collect();
            GpuRecipeMath::encode_batch(&mut encoder, &maths);
            queue.submit([encoder.finish()]);
            for (runtime, expected) in runtimes.iter().zip(expected) {
                let flags = runtime.parts.iter().map(|p| p.math.as_ref().unwrap().guard_buffer()).collect::<Vec<_>>();
                let mut reader = GuardReadback::new(&device, flags.len()).unwrap();
                assert_eq!(reader.read(&device, &queue, &flags).unwrap(), vec![0; flags.len()]);
                assert_eq!(output(runtime, &device, &queue), expected);
            }
        }
    }
    #[test]
    fn cached_runtime_rebuilds_current_guards_and_changed_descriptors() {
        let (device, queue) = device();
        let mut mesh = loom_mesh::sphere_segmented(1.0, 8, 0.0);
        loom_mesh::uvmap::auto_map(&mut mesh, loom_mesh::uvmap::AutoMap::Spherical, 1);
        let parts = [(None, false)];
        let mut runtime = GpuPoseRuntime::new(&device, &queue, &mesh, &parts).unwrap();
        run(&mut runtime, &device, &queue);
        assert!(!runtime.update(&device, &queue, &mesh, &parts).unwrap());
        run(&mut runtime, &device, &queue);
        queue.write_buffer(
            runtime.parts[0].math.as_ref().unwrap().guard_buffer(),
            0,
            bytemuck::bytes_of(&8u32),
        );
        assert_eq!(runtime.recover_guards(&device, &queue).unwrap(), 1);
        let fresh = GpuPoseRuntime::new(&device, &queue, &mesh, &parts).unwrap();
        assert_eq!(
            output(&runtime, &device, &queue),
            output(&fresh, &device, &queue)
        );
        mesh.vertices_mut()[0].position = mesh.vertices()[1].position;
        assert!(!runtime.update(&device, &queue, &mesh, &parts).unwrap());
        let mut encoder = device.create_command_encoder(&Default::default());
        runtime.encode(&mut encoder);
        queue.submit([encoder.finish()]);
        assert!(runtime.recover_guards(&device, &queue).unwrap() > 0);
        let fresh = GpuPoseRuntime::new(&device, &queue, &mesh, &parts).unwrap();
        assert_eq!(
            output(&runtime, &device, &queue),
            output(&fresh, &device, &queue)
        );
        let changed = [(None, true)];
        assert!(runtime.update(&device, &queue, &mesh, &changed).unwrap());
        run(&mut runtime, &device, &queue);
        let fresh = GpuPoseRuntime::new(&device, &queue, &mesh, &changed).unwrap();
        assert_eq!(
            output(&runtime, &device, &queue),
            output(&fresh, &device, &queue)
        );
        let candidates = runtime.candidates();
        assert_eq!(candidates.len(), 1);
        if runtime.fused {
            let positions: Vec<_> = runtime.geometry.corner_sources()[runtime.geometry.parts[0].clone()].iter().map(|i| mesh.vertices()[*i as usize].position.to_array()).collect();
            runtime.parts[0].prepared.assert_current_bits(&positions);
            assert_eq!(positions.len() as u64 * 64, candidates[0].output.size());
        } else {
            assert_eq!(candidates[0].positions.len() as u64 * 64, candidates[0].output.size());
        }
        assert!(!candidates[0].indices.is_empty());
        assert!(candidates[0].guard.size() >= 4);
    }
    #[test]
    #[ignore = "actual current runtime requires LOOM_PERF_SCENE"]
    fn actual_ray_runtime_three_current_poses() {
        let (device, queue) = device();
        let value: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(std::env::var("LOOM_PERF_SCENE").unwrap()).unwrap(),
        )
        .unwrap();
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
        let mut runtimes = std::collections::HashMap::new();
        let mut update_time = std::time::Duration::ZERO;
        for frame in [0.0, 39.25, 149.0] {
            scene.set_time(frame);
            let mut count = 0;
            for id in &ids {
                let mut mesh = scene.eval_mesh(*id).value.mesh.clone();
                loom_mesh::edit_ops::transform_mesh(&mut mesh, scene.object_matrix(*id)).unwrap();
                let surfaces = scene.node_surfaces(*id).unwrap();
                let materials: std::collections::BTreeSet<_> = mesh
                    .faces()
                    .iter()
                    .filter(|f| !f.hidden)
                    .map(|f| f.material_id)
                    .collect();
                let parts: Vec<_> = materials
                    .into_iter()
                    .map(|m| (Some(m), surfaces.for_face(m).faceted))
                    .collect();
                if !runtimes.contains_key(id) {
                    runtimes.insert(
                        *id,
                        GpuPoseRuntime::new(&device, &queue, &mesh, &parts).unwrap(),
                    );
                }
                let runtime = runtimes.get_mut(id).unwrap();
                let start = std::time::Instant::now();
                assert!(!runtime.update(&device, &queue, &mesh, &parts).unwrap());
                update_time += start.elapsed();
                run(runtime, &device, &queue);
                let fresh = GpuPoseRuntime::new(&device, &queue, &mesh, &parts).unwrap();
                assert_eq!(
                    output(runtime, &device, &queue),
                    output(&fresh, &device, &queue),
                    "frame={frame} node={id:?}"
                );
                for candidate in runtime.candidates() {
                    assert!(candidate.positions.iter().flatten().all(|v| v.is_finite()));
                    count += 1;
                }
            }
            assert_eq!(count, 13);
        }
        eprintln!(
            "RUNTIME cache_hit_descriptor_upload_gather_ms_per_pose={:.3}",
            update_time.as_secs_f64() * 1000.0 / 3.0
        );
    }
    #[test]
    #[ignore = "full current CPU-position producer requires LOOM_PERF_SCENE; LOOM_PRECISE_FULL317=1 selects all poses"]
    fn actual_ray_precise_current_cpu_positions_f64_quality() {
        current_cpu_positions_quality(true);
    }
    #[test]
    #[ignore = "positive control characterizes the original fast5 numeric failure on actual CPU-position input"]
    #[should_panic(expected = "production precise8 F64")]
    fn actual_ray_fast_current_cpu_positions_f64_positive_control() {
        current_cpu_positions_quality(false);
    }
    #[test]
    #[ignore = "bounded actual native timeline endpoint CPU producer parity"]
    fn actual_ray_parallel_native_endpoint_cpu_positions_f64_quality() {
        current_cpu_positions_quality_at(true, vec![149.000015,150.0,153.5,157.0]);
    }
    fn current_cpu_positions_quality(precise: bool) {
        let times: Vec<f32> = if std::env::var_os("LOOM_PRECISE_FULL317").is_some() { (0..317).map(|i| i as f32 * 149.0 / 316.0).collect() } else { vec![0.0, 39.25, 95.71835, 149.0] };
        current_cpu_positions_quality_at(precise, times);
    }
    fn current_cpu_positions_quality_at(precise: bool, times: Vec<f32>) {
        let (device, queue) = device();
        let program = precise.then(|| std::sync::Arc::new(crate::gpu_mikk_precision::PrecisionProgram::new(&device).unwrap()));
        let value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(std::env::var("LOOM_PERF_SCENE").unwrap()).unwrap()).unwrap();
        let scene: loom_scene::Scene = serde_json::from_value(value["scene"].clone()).unwrap();
        let ids: Vec<_> = scene.nodes().filter(|n| n.modifiers.iter().any(|m| matches!(&m.modifier, loom_scene::modifier::Modifier::Skin(_)))).map(|n|n.id).collect();
        assert_eq!(ids.len(), 5);

        let reference_program = precise.then(|| std::sync::Arc::new(crate::gpu_mikk_precision::PrecisionProgram::serial(&device).unwrap()));
        let mut references = std::collections::HashMap::new();
        let mut runtimes = std::collections::HashMap::new();
        let mut total_parts = 0usize;
        let mut total_vertices = 0usize;
        let mut maximum = 0.0f64;
        for frame in &times {
            scene.set_time(*frame);
            let mut count = 0;
            for id in &ids {
                let mut mesh = scene.eval_mesh(*id).value.mesh.clone();
                loom_mesh::edit_ops::transform_mesh(&mut mesh, scene.object_matrix(*id)).unwrap();
                let surfaces = scene.node_surfaces(*id).unwrap();
                let materials: std::collections::BTreeSet<_> = mesh.faces().iter().filter(|f|!f.hidden).map(|f|f.material_id).collect();
                let parts: Vec<_> = materials.into_iter().map(|m|(Some(m),surfaces.for_face(m).faceted)).collect();
                if !runtimes.contains_key(id) {
                    runtimes.insert(*id, GpuPoseRuntime::with_precision(&device, &queue, &mesh, &parts, program.clone()).unwrap());
                }
                let runtime = runtimes.get_mut(id).unwrap();
                runtime.update(&device, &queue, &mesh, &parts).unwrap();
                run(runtime, &device, &queue);
                if !references.contains_key(id) { references.insert(*id, GpuPoseRuntime::with_precision(&device, &queue, &mesh, &parts, reference_program.clone()).unwrap()); }
                let reference = references.get_mut(id).unwrap();
                reference.update(&device, &queue, &mesh, &parts).unwrap(); run(reference, &device, &queue);
                let records = runtime.geometry.readback(&device, &queue);
                for (part_index, (part, range)) in runtime.parts.iter().zip(&runtime.geometry.parts).enumerate() {
                    let attrs = &records[range.clone()];
                    let data = MeshData { positions: attrs.iter().map(|v|v.position[..3].try_into().unwrap()).collect(), normals: attrs.iter().map(|v|v.normal[..3].try_into().unwrap()).collect(), uvs: attrs.iter().map(|v|v.uv[..2].try_into().unwrap()).collect(), indices: part.indices.clone() };
                    let structure = crate::mikktspace::compile_structure(&data);
                    let truth = crate::gpu_pose_geometry::mikk_f64::tangents_f64(&data, &structure);
                    let actual = part.math.as_ref().unwrap().readback(&device, &queue).unwrap();
                    let reference_math = reference.parts[part_index].math.as_ref().unwrap();
                    let serial = reference_math.readback(&device, &queue).unwrap();
                    assert_eq!(part.math.as_ref().unwrap().read_precision_risks(&device, &queue), reference_math.read_precision_risks(&device, &queue), "parallel changes risk flags");
                    assert_eq!(bytemuck::cast_slice::<_,u8>(&actual),bytemuck::cast_slice::<_,u8>(&serial),"parallel output bytes frame={frame} node={id:?} part={part_index}");
                    assert_eq!(actual.len(), truth.len()); assert_eq!(actual.len(), attrs.len());
                    let mut part_max = 0.0f64;
                    for (i, ((v,t),a)) in actual.iter().zip(&truth).zip(attrs).enumerate() {
                        assert_eq!(bytemuck::bytes_of(&v.position),bytemuck::bytes_of(&a.position));
                        assert_eq!(bytemuck::bytes_of(&v.normal),bytemuck::bytes_of(&a.normal));
                        assert_eq!(bytemuck::bytes_of(&v.uv),bytemuck::bytes_of(&a.uv));
                        assert_eq!(v.position[..3],part.positions[i]);
                        assert!(v.tangent.iter().all(|x|x.is_finite()));
                        assert_eq!(f64::from(v.tangent[3]),t[3],"handedness frame={frame} node={id:?} part={part_index} vertex={i}");
                        let error = glam::DVec3::new(v.tangent[0] as f64,v.tangent[1] as f64,v.tangent[2] as f64).distance(glam::DVec3::new(t[0],t[1],t[2]));
                        part_max = part_max.max(error);
                        assert!(error <= 2e-5,"production precise8 F64 frame={frame} node={id:?} part={part_index} vertex={i} error={error:e}");
                    }
                    maximum = maximum.max(part_max); total_vertices += actual.len(); total_parts += 1; count += 1;
                    eprintln!("PARALLEL_SERIAL_BYTES_F64_PASS frame={frame} node={id:?} part={part_index} vertices={} max_f64={part_max:e}",actual.len());
                }
            }
            assert_eq!(count,13);
        }
        assert_eq!(total_parts,times.len()*13);
        eprintln!("PRODUCTION_PRECISE_F64_COMPLETE poses={} parts={total_parts} vertices={total_vertices} max_f64={maximum:e}",times.len());
    }

}
