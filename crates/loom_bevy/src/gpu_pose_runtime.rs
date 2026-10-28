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

pub(crate) struct Candidate<'a> {
    pub output: &'a wgpu::Buffer,
    pub guard: &'a wgpu::Buffer,
    pub positions: &'a [[f32; 3]],
    pub indices: &'a [u32],
}
struct Part {
    indices: Vec<u32>,
    positions: Vec<[f32; 3]>,
    preparation: GpuPreparation,
    math: Option<GpuRecipeMath>,
}
pub(crate) struct GpuPoseRuntime {
    source: wgpu::Buffer,
    geometry: GpuPoseGeometry,
    parts: Vec<Part>,
    guards: GuardReadback,
}
impl GpuPoseRuntime {
    /// Mesh positions are already the final CPU world-baked pose.
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &Mesh,
        descriptors: &[(Option<u16>, bool)],
    ) -> Result<Self, String> {
        let positions = source_positions(mesh)?;
        let source = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("current CPU world-baked source positions"),
            contents: bytemuck::cast_slice(&positions),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
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
            source,
            geometry,
            parts,
            guards,
        };
        runtime.gather_positions(mesh);
        let mut encoder = device.create_command_encoder(&Default::default());
        runtime.encode_preparation(&mut encoder);
        queue.submit([encoder.finish()]);
        runtime.recompile(device, queue, &(0..runtime.parts.len()).collect::<Vec<_>>())?;
        Ok(runtime)
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
            *self = Self::new(device, queue, mesh, descriptors)?;
            return Ok(true);
        }
        {
            let _scope = loom_render::profile::Scope::new("gpu_pose.source_upload");
            let positions = source_positions(mesh)?;
            queue.write_buffer(&self.source, 0, bytemuck::cast_slice(&positions));
        }
        self.gather_positions(mesh);
        Ok(false)
    }
    fn gather_positions(&mut self, mesh: &Mesh) {
        let _scope = loom_render::profile::Scope::new("gpu_pose.corner_gather");
        for (part, range) in self.parts.iter_mut().zip(&self.geometry.parts) {
            part.positions.clear();
            part.positions.extend(
                self.geometry.corner_sources()[range.clone()]
                    .iter()
                    .map(|i| mesh.vertices()[*i as usize].position.to_array()),
            );
        }
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
            part.math = Some(GpuRecipeMath::new(
                device,
                self.geometry.intermediate_buffer(),
                range.start as u32,
                &data,
                &part.preparation,
                &prepared,
                &structure,
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
    pub(crate) fn candidates(&self) -> Vec<Candidate<'_>> {
        self.parts
            .iter()
            .map(|p| {
                let math = p.math.as_ref().unwrap();
                Candidate {
                    output: math.output(),
                    guard: math.guard_buffer(),
                    positions: &p.positions,
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
    mesh.vertices()
        .iter()
        .map(|v| {
            if v.position.is_finite() {
                Ok([v.position.x, v.position.y, v.position.z, 1.0])
            } else {
                Err("nonfinite current world position".into())
            }
        })
        .collect()
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
    use super::*;
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
        assert_eq!(
            candidates[0].positions.len() as u64 * 64,
            candidates[0].output.size()
        );
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
}
