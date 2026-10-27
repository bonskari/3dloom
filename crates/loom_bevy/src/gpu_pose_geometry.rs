//! Test-only current-pose GPU geometry experiment.
//!
//! Positions may come directly from the all-influence skin compute buffer.
//! Newell face normals and SG fan normals are reconstructed from those posed
//! positions. Tangents are explicitly current CPU-oracle inputs until the
//! separate Mikk group experiment establishes its arithmetic behavior. This is
//! not a production GPU skinning/tangent implementation.
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use loom_mesh::{FaceId, Mesh, VertexId};
use std::ops::Range;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Corner {
    source: [u32; 4],
    flags: [u32; 4],
    normal: [f32; 4],
    uv: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Pose {
    world: [f32; 16],
    counts: [u32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub(crate) struct PackedVertex {
    pub position: [f32; 4],
    pub normal: [f32; 4],
    pub tangent: [f32; 4],
    pub uv: [f32; 4],
}
const _: [(); 64] = [(); std::mem::size_of::<Corner>()];
const _: [(); 64] = [(); std::mem::size_of::<PackedVertex>()];

/// Owned immutable descriptors. Faces/fans include smoothing neighbors across
/// material and hidden-face boundaries. Parts follow the CPU material order.
struct Topology {
    arena: Vec<u32>,
    corners: Vec<Corner>,
    parts: Vec<Range<usize>>,
    faces: usize,
    vertices: usize,
}
impl Topology {
    fn new(mesh: &Mesh, parts: &[(Option<u16>, bool)]) -> Result<Self, String> {
        mesh.check_invariants()?;
        if mesh.vertex_count() == 0 || mesh.face_count() == 0 || parts.is_empty() {
            return Err("GPU prototype requires nonempty topology and parts".into());
        }
        let mut arena = vec![
            0u32;
            mesh.face_count()
                .checked_mul(4)
                .ok_or("face count overflow")?
        ];
        for (i, face) in mesh.faces().iter().enumerate() {
            let vertices = mesh
                .face_vertex_ids(FaceId(i as u32))
                .ok_or("invalid face loop")?;
            let offset = u32::try_from(arena.len()).map_err(|_| "topology exceeds u32")?;
            arena[i * 4..i * 4 + 4].copy_from_slice(&[
                offset,
                vertices.len() as u32,
                face.smoothing_groups,
                0,
            ]);
            arena.extend(vertices.iter().map(|v| v.0));
        }
        let mut fans = Vec::with_capacity(mesh.vertex_count());
        for i in 0..mesh.vertex_count() {
            let faces = mesh.faces_around_vertex(VertexId(i as u32));
            let offset = u32::try_from(arena.len()).map_err(|_| "topology exceeds u32")?;
            fans.push([offset, faces.len() as u32]);
            arena.extend(faces.iter().map(|f| f.0));
        }
        let mut corners = Vec::new();
        let mut ranges = Vec::new();
        for &(material, faceted) in parts {
            let start = corners.len();
            for (i, face) in mesh
                .faces()
                .iter()
                .enumerate()
                .filter(|(_, f)| !f.hidden && material.is_none_or(|id| id == f.material_id))
            {
                let vertices = mesh
                    .face_vertex_ids(FaceId(i as u32))
                    .ok_or("invalid face loop")?;
                if vertices.len() < 3 {
                    continue;
                }
                let mut he = face.he;
                for _ in &vertices {
                    let vertex = mesh.half_edges()[he.0 as usize].origin.0;
                    let [offset, count] = fans[vertex as usize];
                    let explicit = mesh.explicit_normal(he);
                    let n = explicit.unwrap_or(Vec3::ZERO);
                    let uv = mesh.corner_uv(1, he).unwrap_or(Vec3::ZERO);
                    corners.push(Corner {
                        source: [vertex, i as u32, offset, count],
                        flags: [
                            face.smoothing_groups,
                            u32::from(explicit.is_some()),
                            u32::from(faceted),
                            0,
                        ],
                        normal: [n.x, n.y, n.z, 0.0],
                        uv: [
                            1.0 * uv.x + 0.0 * uv.y + 0.0,
                            1.0 - (0.0 * uv.x + 1.0 * uv.y + 0.0),
                            0.0,
                            0.0,
                        ],
                    });
                    he = mesh.half_edges()[he.0 as usize].next;
                }
            }
            ranges.push(start..corners.len());
        }
        if corners.is_empty() {
            return Err("GPU prototype has no visible corners".into());
        }
        Ok(Self {
            arena,
            corners,
            parts: ranges,
            faces: mesh.face_count(),
            vertices: mesh.vertex_count(),
        })
    }
}

pub(crate) struct GpuPoseGeometry {
    world_pipeline: wgpu::ComputePipeline,
    world_binding: wgpu::BindGroup,
    world_positions: wgpu::Buffer,
    face_pipeline: wgpu::ComputePipeline,
    corner_pipeline: wgpu::ComputePipeline,
    face_binding: wgpu::BindGroup,
    corner_binding: wgpu::BindGroup,
    pose: wgpu::Buffer,
    tangent: wgpu::Buffer,
    output: wgpu::Buffer,
    counts: [u32; 4],
    pub parts: Vec<Range<usize>>,
}
impl GpuPoseGeometry {
    pub fn new(
        device: &wgpu::Device,
        positions: &wgpu::Buffer,
        mesh: &Mesh,
        parts: &[(Option<u16>, bool)],
        world: Mat4,
        oracle_tangents: &[[f32; 4]],
    ) -> Result<Self, String> {
        Self::new_with_math(
            device,
            positions,
            mesh,
            parts,
            world,
            oracle_tangents,
            false,
        )
    }
    pub fn new_origin_relative(
        device: &wgpu::Device,
        positions: &wgpu::Buffer,
        mesh: &Mesh,
        parts: &[(Option<u16>, bool)],
        world: Mat4,
        oracle_tangents: &[[f32; 4]],
    ) -> Result<Self, String> {
        Self::new_with_math(device, positions, mesh, parts, world, oracle_tangents, true)
    }
    fn new_with_math(
        device: &wgpu::Device,
        positions: &wgpu::Buffer,
        mesh: &Mesh,
        parts: &[(Option<u16>, bool)],
        world: Mat4,
        oracle_tangents: &[[f32; 4]],
        origin_relative: bool,
    ) -> Result<Self, String> {
        let topology = Topology::new(mesh, parts)?;
        if positions.size() < topology.vertices as u64 * 16
            || oracle_tangents.len() != topology.corners.len()
        {
            return Err("GPU position/oracle tangent counts do not match topology".into());
        }
        if !world.is_finite()
            || !world.row(3).abs_diff_eq(glam::Vec4::W, 1e-6)
            || !oracle_tangents.iter().flatten().all(|v| v.is_finite())
        {
            return Err("GPU prototype inputs must be finite and affine".into());
        }
        let limits = device.limits();
        if limits.max_storage_buffers_per_shader_stage < 6
            || topology.vertices.div_ceil(64) > limits.max_compute_workgroups_per_dimension as usize
            || topology.faces.div_ceil(64) > limits.max_compute_workgroups_per_dimension as usize
            || topology.corners.len().div_ceil(64)
                > limits.max_compute_workgroups_per_dimension as usize
        {
            return Err("GPU prototype exceeds compute limits".into());
        }
        for size in [
            topology.arena.len() * 4,
            topology.corners.len() * 64,
            topology.faces * 16,
            topology.vertices * 16,
            oracle_tangents.len() * 16,
        ] {
            if size as u64 > limits.max_buffer_size
                || size as u64 > u64::from(limits.max_storage_buffer_binding_size)
            {
                return Err("GPU prototype exceeds storage limits".into());
            }
        }
        let storage = |label, bytes: &[u8], mutable| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: bytes,
                usage: wgpu::BufferUsages::STORAGE
                    | if mutable {
                        wgpu::BufferUsages::COPY_DST
                    } else {
                        wgpu::BufferUsages::empty()
                    },
            })
        };
        let metadata = storage(
            "posed normal topology",
            bytemuck::cast_slice(&topology.arena),
            false,
        );
        let corners = storage(
            "posed material corners",
            bytemuck::cast_slice(&topology.corners),
            false,
        );
        let tangent = storage(
            "CURRENT CPU tangent oracle",
            bytemuck::cast_slice(oracle_tangents),
            true,
        );
        let face_normals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("posed Newell normals"),
            size: topology.faces as u64 * 16,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let output = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("posed packed corners"),
            size: topology.corners.len() as u64 * 64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let world_positions = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("CURRENT GPU world positions"),
            size: topology.vertices as u64 * 16,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let counts = [
            topology.faces as u32,
            topology.corners.len() as u32,
            topology.vertices as u32,
            0,
        ];
        let pose = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("posed normal world transform"),
            contents: bytemuck::bytes_of(&Pose {
                world: world.to_cols_array(),
                counts,
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("current-pose SG normals"),
            source: wgpu::ShaderSource::Wgsl(include_str!("gpu_pose_geometry.wgsl").into()),
        });
        let pipeline = |entry| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: None,
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let world_pipeline = pipeline("transform_positions");
        let face_pipeline = pipeline(if origin_relative {
            "faces_origin_relative"
        } else {
            "faces"
        });
        let corner_pipeline = pipeline("pack_corners");
        let buffers = [
            positions,
            &metadata,
            &corners,
            &tangent,
            &face_normals,
            &output,
            &pose,
            &world_positions,
        ];
        let binding = |pipeline: &wgpu::ComputePipeline, indices: &[usize]| {
            let entries: Vec<_> = indices
                .iter()
                .map(|&i| wgpu::BindGroupEntry {
                    binding: i as u32,
                    resource: buffers[i].as_entire_binding(),
                })
                .collect();
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("posed normal inputs"),
                layout: &pipeline.get_bind_group_layout(0),
                entries: &entries,
            })
        };
        let world_binding = binding(&world_pipeline, &[0, 6, 7]);
        let face_binding = binding(&face_pipeline, &[1, 4, 6, 7]);
        let corner_binding = binding(&corner_pipeline, &[1, 2, 3, 4, 5, 6, 7]);
        Ok(Self {
            world_pipeline,
            world_binding,
            world_positions,
            face_pipeline,
            corner_pipeline,
            face_binding,
            corner_binding,
            pose,
            tangent,
            output,
            counts,
            parts: topology.parts,
        })
    }
    pub fn update_oracle(
        &self,
        queue: &wgpu::Queue,
        world: Mat4,
        tangents: &[[f32; 4]],
    ) -> Result<(), String> {
        if tangents.len() != self.counts[1] as usize
            || !world.is_finite()
            || !world.row(3).abs_diff_eq(glam::Vec4::W, 1e-6)
            || !tangents.iter().flatten().all(|v| v.is_finite())
        {
            return Err("invalid current-pose oracle update".into());
        }
        queue.write_buffer(
            &self.pose,
            0,
            bytemuck::bytes_of(&Pose {
                world: world.to_cols_array(),
                counts: self.counts,
            }),
        );
        queue.write_buffer(&self.tangent, 0, bytemuck::cast_slice(tangents));
        Ok(())
    }
    pub fn encode(&self, encoder: &mut wgpu::CommandEncoder) {
        for (pipeline, binding, count) in [
            (&self.world_pipeline, &self.world_binding, self.counts[2]),
            (&self.face_pipeline, &self.face_binding, self.counts[0]),
            (&self.corner_pipeline, &self.corner_binding, self.counts[1]),
        ] {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("current-pose normals"),
                timestamp_writes: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, binding, &[]);
            pass.dispatch_workgroups(count.div_ceil(64), 1, 1);
        }
    }
    pub fn read_world_positions(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Vec<[f32; 4]> {
        read_records(
            device,
            queue,
            &self.world_positions,
            self.counts[2] as usize,
        )
    }
    pub fn readback(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> Vec<PackedVertex> {
        read_records(device, queue, &self.output, self.counts[1] as usize)
    }
}
fn read_records<T: Pod>(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    buffer: &wgpu::Buffer,
    count: usize,
) -> Vec<T> {
    let size = (count * std::mem::size_of::<T>()) as u64;
    let target = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("prototype validation readback"),
        size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_buffer_to_buffer(buffer, 0, &target, 0, size);
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    target.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::Maintain::Wait);
    rx.recv().unwrap().unwrap();
    let bytes = target.slice(..).get_mapped_range();
    let records = bytes
        .chunks_exact(std::mem::size_of::<T>())
        .map(bytemuck::pod_read_unaligned)
        .collect();
    drop(bytes);
    target.unmap();
    records
}

#[cfg(test)]
#[path = "gpu_mikk_prepare.rs"]
mod mikk_prepare;
#[cfg(test)]
#[path = "gpu_mikk_probe.rs"]
mod mikk_probe;
#[cfg(test)]
#[path = "gpu_pose_precision.rs"]
mod precision;
#[cfg(test)]
#[path = "gpu_pose_render.rs"]
mod render_audit;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu_skin::{GpuSkinPositions, SkinInput};
    use loom_scene::{EvalCtx, NodeId};
    pub(super) fn device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(&Default::default());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
            .expect("real GPU required");
        eprintln!("GPU posed normals adapter: {:?}", adapter.get_info());
        pollster::block_on(adapter.request_device(&crate::device_descriptor(&adapter), None))
            .unwrap()
    }
    fn oracle(mesh: &Mesh, parts: &[(Option<u16>, bool)]) -> (Vec<crate::MeshData>, Vec<[f32; 4]>) {
        let all = crate::mesh_data(mesh);
        let mut data = Vec::new();
        let mut tangents = Vec::new();
        for &(id, faceted) in parts {
            let d = crate::convert::mesh_data_part(mesh, &all, id, faceted);
            tangents.extend(
                crate::mikktspace::generate_serial(&d).expect("current corrected CPU Mikk oracle"),
            );
            data.push(d);
        }
        (data, tangents)
    }
    fn compare(
        actual: &[PackedVertex],
        parts: &[crate::MeshData],
        tangents: &[[f32; 4]],
    ) -> (f32, f32) {
        let mut cursor = 0;
        let (mut p_error, mut n_error) = (0.0f32, 0.0f32);
        for part in parts {
            for i in 0..part.positions.len() {
                let got = actual[cursor];
                let p = Vec3::from_array(got.position[..3].try_into().unwrap());
                let n = Vec3::from_array(got.normal[..3].try_into().unwrap());
                p_error = p_error.max(p.distance(Vec3::from_array(part.positions[i])));
                n_error = n_error.max(n.distance(Vec3::from_array(part.normals[i])));
                assert_eq!(
                    got.tangent.map(f32::to_bits),
                    tangents[cursor].map(f32::to_bits),
                    "current oracle tangent must be copied verbatim"
                );
                assert_eq!(
                    [got.uv[0].to_bits(), got.uv[1].to_bits()],
                    part.uvs[i].map(f32::to_bits)
                );
                assert!(got
                    .position
                    .iter()
                    .chain(&got.normal)
                    .all(|v| v.is_finite()));
                cursor += 1;
            }
        }
        assert_eq!(cursor, actual.len());
        (p_error, n_error)
    }
    fn enriched_box() -> Mesh {
        let mut mesh = loom_mesh::box_primitive(Vec3::splat(3.0));
        for (i, face) in mesh.faces_mut().iter_mut().enumerate() {
            face.smoothing_groups = if i % 3 == 0 { 0 } else { 3 };
            face.material_id = (i % 2) as u16;
        }
        mesh.faces_mut()[0].hidden = true;
        let uvs: Vec<_> = mesh
            .half_edges()
            .iter()
            .map(|he| {
                let p = mesh.vertices()[he.origin.0 as usize].position;
                Vec3::new((p.x + p.z) * 0.13, (p.y - p.z) * 0.17, 0.0)
            })
            .collect();
        let count = mesh.half_edge_count();
        let map = mesh.ensure_map(1);
        map.verts = uvs;
        map.corners = (0..count as u32).collect();
        let explicit_corner = mesh.faces()[1].he.0 as usize;
        let map = mesh.ensure_map(loom_mesh::NORMALS_CHANNEL);
        map.verts = vec![Vec3::ZERO, Vec3::new(0.2, 0.3, 0.9)];
        map.corners.fill(0);
        map.corners[explicit_corner] = 1;
        mesh
    }
    #[test]
    fn gpu_skin_and_posed_newell_sg_normals_preserve_all_twelve_influences() {
        let (device, queue) = device();
        let rest = enriched_box();
        let positions: Vec<_> = rest.vertices().iter().map(|v| v.position).collect();
        let bone_defs: Vec<_> = (0..875)
            .map(|i| {
                (
                    NodeId(i),
                    Mat4::IDENTITY,
                    loom_scene::skin::Envelope::default(),
                )
            })
            .collect();
        let raw_weights: Vec<_> = (0..positions.len())
            .map(|v| {
                (0..12)
                    .map(|b| (if b == 11 { 874 } else { b }, (b + v as u32 + 1) as f32))
                    .collect::<Vec<_>>()
            })
            .collect();
        let data = loom_scene::skin::SkinData::from_file(
            &bone_defs,
            Mat4::from_translation(Vec3::new(0.2, -0.4, 0.3)),
            raw_weights,
        );
        let world = Mat4::from_scale_rotation_translation(
            Vec3::new(1.2, 0.8, 1.1),
            glam::Quat::from_rotation_y(0.2),
            Vec3::new(2.0, 1.0, -0.3),
        );
        let parts = [(Some(0), false), (Some(1), true)];
        for pose in [0.0f32, 0.35] {
            let lookup = |id: NodeId| {
                if id.0 == 3 {
                    None
                } else {
                    Some(Mat4::from_scale_rotation_translation(
                        Vec3::new(1.0 + id.0 as f32 * 0.0002, 0.95, 1.1),
                        glam::Quat::from_rotation_z(pose + id.0 as f32 * 0.001),
                        Vec3::new(id.0 as f32 * 0.002, 0.0, 0.2),
                    ))
                }
            };
            let ctx = EvalCtx {
                object_world: world,
                object_inverse: world.inverse(),
                world_lookup: Some(&lookup),
                ..Default::default()
            };
            let matrices = loom_scene::skin::bone_matrices(&data, &ctx);
            let weights: Vec<_> = positions
                .iter()
                .enumerate()
                .map(|(i, p)| data.vertex_weights(i, data.mesh_bind.transform_point3(*p)))
                .collect();
            assert!(weights.iter().all(|w| w.len() == 12));
            let input = SkinInput {
                positions: &positions,
                weights: &weights,
                bones: &matrices,
                mesh_bind: data.mesh_bind,
                object_world: world,
                back: world.inverse(),
            };
            let skin = GpuSkinPositions::new(&device, &input).unwrap();
            let value = loom_scene::modifier::StackValue {
                mesh: rest.clone(),
                selection: None,
                shape: None,
                shape_selection: None,
                warnings: Vec::new(),
                patch: None,
                patch_steps: None,
            };
            let mut expected = loom_scene::skin::apply(&data, value, &ctx).unwrap().mesh;
            loom_mesh::edit_ops::transform_mesh(&mut expected, world).unwrap();
            let (reference, tangents) = oracle(&expected, &parts);
            let geometry = GpuPoseGeometry::new(
                &device,
                skin.output(),
                &rest,
                &parts,
                Mat4::IDENTITY,
                &tangents,
            )
            .unwrap();
            geometry.update_oracle(&queue, world, &tangents).unwrap();
            assert_eq!(
                geometry.parts.iter().map(|r| r.len()).collect::<Vec<_>>(),
                reference
                    .iter()
                    .map(|d| d.positions.len())
                    .collect::<Vec<_>>()
            );
            let mut encoder = device.create_command_encoder(&Default::default());
            skin.encode(&mut encoder);
            geometry.encode(&mut encoder);
            queue.submit([encoder.finish()]);
            let actual = geometry.readback(&device, &queue);
            let (p_error, n_error) = compare(&actual, &reference, &tangents);
            eprintln!("GPU_POSE_SYNTHETIC pose={pose} position_max={p_error:e} normal_vector_max={n_error:e}");
            assert!(p_error<2e-5 && n_error<2e-5,"roundoff must remain within the prototype's explicit numeric acceptance, not production pixel parity");
        }
    }

    #[test]
    #[ignore = "actual Ray GPU posed normals requires LOOM_PERF_SCENE"]
    fn actual_ray_gpu_skin_normals_and_material_packing() {
        ray_geometry_audit(true, false, &[0.0, 39.25, 149.0]);
    }

    #[test]
    #[ignore = "actual Ray origin-relative GPU normals require LOOM_PERF_SCENE"]
    fn actual_ray_origin_relative_sg_accuracy_against_same_position_f64() {
        ray_geometry_audit(false, false, &[0.0, 39.25, 149.0]);
    }

    #[test]
    #[ignore = "actual current GPU weld/edge preparation requires LOOM_PERF_SCENE"]
    fn actual_ray_current_gpu_weld_and_corrected_edges() {
        let count = std::env::var("LOOM_GPU_PREP_POSES")
            .ok()
            .map(|v| v.parse::<usize>().unwrap())
            .unwrap_or(317);
        let times: Vec<_> = if count == 3 {
            vec![0.0, 39.25, 149.0]
        } else {
            (0..count)
                .map(|i| 149.0 * i as f32 / (count - 1).max(1) as f32)
                .collect()
        };
        ray_geometry_audit(false, true, &times);
    }

    fn ray_geometry_audit(strict_legacy: bool, prepare: bool, times: &[f32]) {
        let path = std::env::var("LOOM_PERF_SCENE").expect("real saved scene");
        let mut value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let scene: loom_scene::Scene = serde_json::from_value(value["scene"].take()).unwrap();
        let ids: Vec<_> = scene
            .nodes()
            .filter(|n| {
                n.modifiers
                    .iter()
                    .any(|m| matches!(m.modifier, loom_scene::Modifier::Skin(_)))
            })
            .map(|n| n.id)
            .collect();
        let (device, queue) = device();
        let preparation_program =
            prepare.then(|| super::mikk_prepare::Program::new(&device).unwrap());
        for &time in times {
            scene.set_time(time);
            for id in &ids {
                let node = scene.node(*id).unwrap();
                assert_eq!(
                    node.modifiers.len(),
                    1,
                    "prototype must audit before claiming whole-stack support"
                );
                let loom_scene::Modifier::Skin(data) = &node.modifiers[0].modifier else {
                    panic!()
                };
                assert!(
                    data.gizmos.iter().all(|g| !g.enabled),
                    "GPU gizmos are not implemented"
                );
                assert!(data.always_deform && !data.fast_updates);
                let rest = node.object.display_output();
                let positions: Vec<_> = rest.vertices().iter().map(|v| v.position).collect();
                let world = scene.object_matrix(*id);
                let lookup = |id| scene.node(id).map(|_| scene.world_matrix(id));
                let squash = |id| {
                    scene.node(id).is_some_and(|n|matches!(&n.object,loom_scene::ObjectKind::Bone{stretch,..} if stretch.get()==2))
                };
                let ctx = EvalCtx {
                    object_world: world,
                    object_inverse: world.inverse(),
                    world_lookup: Some(&lookup),
                    bone_squashes: Some(&squash),
                    ..Default::default()
                };
                let matrices = loom_scene::skin::bone_matrices(data, &ctx);
                let weights: Vec<_> = positions
                    .iter()
                    .enumerate()
                    .map(|(i, p)| data.vertex_weights(i, data.mesh_bind.transform_point3(*p)))
                    .collect();
                let back = if data.back_transform {
                    world.inverse()
                } else {
                    data.mesh_bind.inverse()
                };
                let mut nonfinite = 0;
                let mut row_error = 0.0f32;
                for matrix in [Some(data.mesh_bind), Some(world), Some(back)]
                    .into_iter()
                    .chain(matrices.iter().copied())
                    .flatten()
                {
                    nonfinite += usize::from(!matrix.is_finite());
                    row_error = row_error.max((matrix.row(3) - glam::Vec4::W).abs().max_element());
                }
                eprintln!("GPU_POSE_MATRIX_AUDIT node={} time={time} nonfinite={nonfinite} homogeneous_row_error={row_error:e}",node.name);
                let skin = GpuSkinPositions::new(
                    &device,
                    &SkinInput {
                        positions: &positions,
                        weights: &weights,
                        bones: &matrices,
                        mesh_bind: data.mesh_bind,
                        object_world: world,
                        back,
                    },
                )
                .unwrap();
                let mut expected = scene.eval_mesh(*id).value.mesh.clone();
                loom_mesh::edit_ops::transform_mesh(&mut expected, world).unwrap();
                let material_ids: std::collections::BTreeSet<_> = rest
                    .faces()
                    .iter()
                    .filter(|f| !f.hidden)
                    .map(|f| f.material_id)
                    .collect();
                let surfaces =
                    scene.node_surfaces_for(*id, loom_scene::material::ShellView::Viewport);
                let parts: Vec<_> = material_ids
                    .into_iter()
                    .map(|material| {
                        let faceted = surfaces
                            .as_ref()
                            .is_some_and(|s| s.for_face(material).faceted);
                        (Some(material), faceted)
                    })
                    .collect();
                let (reference, tangents) = oracle(&expected, &parts);
                if let Some(program) = &preparation_program {
                    let stable = GpuPoseGeometry::new_origin_relative(
                        &device,
                        skin.output(),
                        &rest,
                        &parts,
                        world,
                        &tangents,
                    )
                    .unwrap();
                    let mut encoder = device.create_command_encoder(&Default::default());
                    skin.encode(&mut encoder);
                    stable.encode(&mut encoder);
                    queue.submit([encoder.finish()]);
                    let records = stable.readback(&device, &queue);
                    for (part, (range, template)) in stable.parts.iter().zip(&reference).enumerate()
                    {
                        let indices: Vec<_> = template
                            .indices
                            .iter()
                            .map(|i| *i + range.start as u32)
                            .collect();
                        let preparation = super::mikk_prepare::GpuPreparation::with_program(
                            &device,
                            &stable.output,
                            records.len(),
                            &indices,
                            program,
                        )
                        .unwrap();
                        let mut encoder = device.create_command_encoder(&Default::default());
                        preparation.encode(&mut encoder);
                        queue.submit([encoder.finish()]);
                        let output = preparation.readback(&device, &queue).unwrap();
                        let current = &records[range.clone()];
                        let same_gpu = crate::MeshData {
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
                            indices: template.indices.clone(),
                        };
                        super::mikk_prepare::compare_cpu(&same_gpu, &output);
                        eprintln!("GPU_CURRENT_PREP node={} time={time} part={part} corners={} exact_weld_flags_neighbors=PASS",node.name,indices.len());
                    }
                    continue;
                }

                let geometry =
                    GpuPoseGeometry::new(&device, skin.output(), &rest, &parts, world, &tangents)
                        .unwrap();
                let mut encoder = device.create_command_encoder(&Default::default());
                skin.encode(&mut encoder);
                geometry.encode(&mut encoder);
                queue.submit([encoder.finish()]);
                let actual = geometry.readback(&device, &queue);
                let (p_error, n_error) = compare(&actual, &reference, &tangents);
                eprintln!("GPU_POSE_RAY node={} time={time} vertices={} palette={} influences={} position_max={p_error:e} normal_vector_max={n_error:e}",node.name,positions.len(),matrices.len(),weights.iter().map(Vec::len).max().unwrap());
                if time == 0.0 && node.name == "Body001" {
                    let cpu_positions: Vec<[f32; 4]> = expected
                        .vertices()
                        .iter()
                        .map(|v| [v.position.x, v.position.y, v.position.z, 1.0])
                        .collect();
                    let cpu_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("test oracle CURRENT world positions"),
                        contents: bytemuck::cast_slice(&cpu_positions),
                        usage: wgpu::BufferUsages::STORAGE,
                    });
                    let normal_only = GpuPoseGeometry::new(
                        &device,
                        &cpu_buffer,
                        &rest,
                        &parts,
                        Mat4::IDENTITY,
                        &tangents,
                    )
                    .unwrap();
                    let mut encoder = device.create_command_encoder(&Default::default());
                    normal_only.encode(&mut encoder);
                    queue.submit([encoder.finish()]);
                    let (p, n) = compare(
                        &normal_only.readback(&device, &queue),
                        &reference,
                        &tangents,
                    );
                    eprintln!("GPU_SG_ONLY current_CPU_world_positions position_max={p:e} normal_vector_max={n:e} bounds={:?}",expected.aabb());
                    let (lo, hi) = expected.aabb();
                    let center = (lo + hi) * 0.5;
                    let mut centered = expected.clone();
                    for (source, target) in expected.vertices().iter().zip(centered.vertices_mut())
                    {
                        target.position = source.position - center;
                        assert_eq!(
                            source.position.as_dvec3() - center.as_dvec3(),
                            target.position.as_dvec3(),
                            "diagnostic centering must not alter represented geometry"
                        );
                    }
                    let (center_reference, center_tangents) = oracle(&centered, &parts);
                    let center_positions: Vec<[f32; 4]> = centered
                        .vertices()
                        .iter()
                        .map(|v| [v.position.x, v.position.y, v.position.z, 1.0])
                        .collect();
                    let center_buffer =
                        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("test oracle EXACTLY centered current positions"),
                            contents: bytemuck::cast_slice(&center_positions),
                            usage: wgpu::BufferUsages::STORAGE,
                        });
                    let center_geometry = GpuPoseGeometry::new(
                        &device,
                        &center_buffer,
                        &rest,
                        &parts,
                        Mat4::IDENTITY,
                        &center_tangents,
                    )
                    .unwrap();
                    let mut encoder = device.create_command_encoder(&Default::default());
                    center_geometry.encode(&mut encoder);
                    queue.submit([encoder.finish()]);
                    let (cp, cn) = compare(
                        &center_geometry.readback(&device, &queue),
                        &center_reference,
                        &center_tangents,
                    );
                    let (mut far_f64_error, mut center_f64_error) = (0.0f64, 0.0f64);
                    for fi in 0..expected.face_count() {
                        let pts = expected.face_positions(FaceId(fi as u32)).unwrap();
                        let mut normal = glam::DVec3::ZERO;
                        for i in 0..pts.len() {
                            let a = pts[i].as_dvec3();
                            let b = pts[(i + 1) % pts.len()].as_dvec3();
                            normal += glam::DVec3::new(
                                (a.y - b.y) * (a.z + b.z),
                                (a.z - b.z) * (a.x + b.x),
                                (a.x - b.x) * (a.y + b.y),
                            );
                        }
                        let truth = normal.normalize_or_zero();
                        far_f64_error = far_f64_error.max(
                            expected
                                .face_normal(FaceId(fi as u32))
                                .as_dvec3()
                                .distance(truth),
                        );
                        center_f64_error = center_f64_error.max(
                            centered
                                .face_normal(FaceId(fi as u32))
                                .as_dvec3()
                                .distance(truth),
                        );
                    }
                    eprintln!("GPU_SG_TRANSLATION exact_center={center:?} gpu_cpu_position_max={cp:e} gpu_cpu_normal_max={cn:e} cpu_far_face_f64_max={far_f64_error:e} cpu_centered_face_f64_max={center_f64_error:e}");
                }
                let stable = GpuPoseGeometry::new_origin_relative(
                    &device,
                    skin.output(),
                    &rest,
                    &parts,
                    world,
                    &tangents,
                )
                .unwrap();
                let mut encoder = device.create_command_encoder(&Default::default());
                stable.encode(&mut encoder);
                queue.submit([encoder.finish()]);
                let stable_records = stable.readback(&device, &queue);
                let gpu_positions = stable.read_world_positions(&device, &queue);
                let cpu_positions: Vec<[f32; 4]> = expected
                    .vertices()
                    .iter()
                    .map(|v| [v.position.x, v.position.y, v.position.z, 1.0])
                    .collect();
                let gpu_truth = super::precision::normals_f64(&rest, &parts, &gpu_positions);
                let cpu_truth = super::precision::normals_f64(&rest, &parts, &cpu_positions);
                let stable_f64 = super::precision::packed_error(&stable_records, &gpu_truth);
                let cpu_f64 = reference
                    .iter()
                    .flat_map(|d| &d.normals)
                    .zip(&cpu_truth)
                    .map(|(a, b)| Vec3::from_array(*a).distance(*b))
                    .fold(0.0f32, f32::max);
                let shape_difference = cpu_truth
                    .iter()
                    .zip(&gpu_truth)
                    .map(|(a, b)| a.distance(*b))
                    .fold(0.0f32, f32::max);
                eprintln!("GPU_ORIGIN_RELATIVE node={} time={time} gpu_same_position_f64_max={stable_f64:e} cpu_same_position_f64_max={cpu_f64:e} f64_shape_sensitivity={shape_difference:e}",node.name);
                assert!(stable_f64<1e-5,"origin-relative normals must match higher precision on the SAME GPU-represented positions; not a visual shipping acceptance");
                if strict_legacy {
                    assert!(p_error < 2e-4, "actual pose position mismatch");
                    assert!(
                        n_error < 2e-4,
                        "actual posed SG normal mismatch; do not replace with bind normals"
                    );
                }
            }
        }
    }
}
