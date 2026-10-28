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

// A retained exact raw-descriptor proof avoids rebuilding every face fan on
// pose-only changes. Unused map edits may conservatively miss; no position is
// included and no changed effective descriptor can pass this comparison.
struct DescriptorSnapshot {
    vertex_edges: Vec<Option<loom_mesh::HalfEdgeId>>,
    half_edges: loom_mesh::HalfEdgeSnapshot,
    faces: Vec<loom_mesh::half_edge::Face>,
    maps: [Option<MapSnapshot>; 2],
}
struct MapSnapshot { corners: Vec<u32>, values: Vec<[u32; 3]> }
impl DescriptorSnapshot {
    fn new(mesh: &Mesh) -> Self {
        Self {
            vertex_edges: mesh.vertices().iter().map(|v|v.he).collect(),
            half_edges: mesh.half_edge_snapshot(), faces: mesh.faces().to_vec(),
            maps: [1,loom_mesh::NORMALS_CHANNEL].map(|channel|mesh.map(channel).map(|m|MapSnapshot {
                corners:m.corners.clone(), values:m.verts.iter().map(|v|v.to_array().map(f32::to_bits)).collect(),
            })),
        }
    }
    fn matches(&self, mesh: &Mesh) -> bool {
        self.vertex_edges.len()==mesh.vertex_count()
            && self.vertex_edges.iter().zip(mesh.vertices()).all(|(he,v)|*he==v.he)
            && {
                let _scope = loom_render::profile::Scope::new("gpu_pose.half_edge_check");
                self.half_edges.matches(mesh)
            } && self.faces==mesh.faces()
            && self.maps.iter().zip([1,loom_mesh::NORMALS_CHANNEL]).all(|(old,channel)|match (old,mesh.map(channel)) {
                (None,None)=>true,
                (Some(old),Some(current))=>old.corners==current.corners && old.values.len()==current.verts.len()
                    && old.values.iter().zip(&current.verts).all(|(a,b)|*a==b.to_array().map(f32::to_bits)),
                _=>false,
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
    tangent: Option<wgpu::Buffer>,
    output: wgpu::Buffer,
    counts: [u32; 4],
    pub parts: Vec<Range<usize>>,
    part_descriptors: Vec<(Option<u16>, bool)>,
    descriptor_snapshot: DescriptorSnapshot,
    corner_sources: Vec<u32>,
}
impl GpuPoseGeometry {
    pub(crate) fn from_current_positions(
        device: &wgpu::Device,
        positions: &wgpu::Buffer,
        mesh: &Mesh,
        parts: &[(Option<u16>, bool)],
        world: Mat4,
    ) -> Result<Self, String> {
        Self::new_with_math(device, positions, mesh, parts, world, None, true)
    }
    #[cfg(test)]
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
            Some(oracle_tangents),
            false,
        )
    }
    #[cfg(test)]
    pub fn new_origin_relative(
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
            Some(oracle_tangents),
            true,
        )
    }
    fn new_with_math(
        device: &wgpu::Device,
        positions: &wgpu::Buffer,
        mesh: &Mesh,
        parts: &[(Option<u16>, bool)],
        world: Mat4,
        oracle_tangents: Option<&[[f32; 4]]>,
        origin_relative: bool,
    ) -> Result<Self, String> {
        let topology = Topology::new(mesh, parts)?;
        if positions.size() < topology.vertices as u64 * 16
            || oracle_tangents.is_some_and(|t| t.len() != topology.corners.len())
        {
            return Err("GPU position/oracle tangent counts do not match topology".into());
        }
        if !world.is_finite()
            || !world.row(3).abs_diff_eq(glam::Vec4::W, 1e-6)
            || oracle_tangents.is_some_and(|t| !t.iter().flatten().all(|v| v.is_finite()))
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
            oracle_tangents.map_or(0, |t| t.len()) * 16,
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
        let tangent = oracle_tangents.map(|t| {
            storage(
                "TEST ONLY current CPU tangent oracle",
                bytemuck::cast_slice(t),
                true,
            )
        });
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
        let corner_pipeline = pipeline(if tangent.is_some() {
            "pack_corners"
        } else {
            "pack_geometry_only"
        });
        let buffers = [
            Some(positions),
            Some(&metadata),
            Some(&corners),
            tangent.as_ref(),
            Some(&face_normals),
            Some(&output),
            Some(&pose),
            Some(&world_positions),
        ];
        let binding = |pipeline: &wgpu::ComputePipeline, indices: &[usize]| {
            let entries: Vec<_> = indices
                .iter()
                .map(|&i| wgpu::BindGroupEntry {
                    binding: i as u32,
                    resource: buffers[i]
                        .expect("entry-specific normal stage binding")
                        .as_entire_binding(),
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
        let corner_binding = binding(
            &corner_pipeline,
            if tangent.is_some() {
                &[1, 2, 3, 4, 5, 6, 7]
            } else {
                &[1, 2, 4, 5, 6, 7]
            },
        );
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
            parts: topology.parts.clone(),
            part_descriptors: parts.to_vec(),
            descriptor_snapshot: DescriptorSnapshot::new(mesh),
            corner_sources: topology.corners.iter().map(|c| c.source[0]).collect(),
        })
    }
    #[cfg(test)]
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
        queue.write_buffer(
            self.tangent
                .as_ref()
                .ok_or("normal-only pipeline has no CPU tangent oracle")?,
            0,
            bytemuck::cast_slice(tangents),
        );
        Ok(())
    }
    /// Exact effective GPU descriptors, intentionally excluding posed positions.
    pub(crate) fn matches_descriptors(
        &self,
        mesh: &Mesh,
        parts: &[(Option<u16>, bool)],
    ) -> Result<bool, String> {
        if parts != self.part_descriptors {
            return Ok(false);
        }
        if self.descriptor_snapshot.matches(mesh) { return Ok(true); }
        // Changed/invalid descriptors retain the checked constructor's errors.
        mesh.check_invariants()?;
        Ok(false)
    }
    pub(crate) fn corner_sources(&self) -> &[u32] {
        &self.corner_sources
    }
    pub(crate) fn intermediate_buffer(&self) -> &wgpu::Buffer {
        &self.output
    }
    pub(crate) fn source_vertex_count(&self) -> u32 {
        self.counts[2]
    }
    pub(crate) fn update_transform(&self, queue: &wgpu::Queue, world: Mat4) -> Result<(), String> {
        if !world.is_finite() || !world.row(3).abs_diff_eq(glam::Vec4::W, 1e-6) {
            return Err("current geometry transform must be finite affine".into());
        }
        queue.write_buffer(
            &self.pose,
            0,
            bytemuck::bytes_of(&Pose {
                world: world.to_cols_array(),
                counts: self.counts,
            }),
        );
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
#[path = "gpu_mikk_f64.rs"]
pub(crate) mod mikk_f64;
#[path = "gpu_mikk_prepare.rs"]
pub(crate) mod mikk_prepare;
#[cfg(test)]
#[path = "gpu_mikk_probe.rs"]
mod mikk_probe;
#[path = "gpu_mikk_recipe.rs"]
pub(crate) mod mikk_recipe;
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
    fn current_position_normal_factory_has_no_cpu_tangent_resource() {
        let (device, queue) = device();
        let mesh = enriched_box();
        let parts = [(Some(0), false), (Some(1), true)];
        let positions: Vec<_> = mesh
            .vertices()
            .iter()
            .map(|v| v.position.extend(1.0).to_array())
            .collect();
        let source = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("actual CPU final positions"),
            contents: bytemuck::cast_slice(&positions),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let geometry = GpuPoseGeometry::from_current_positions(
            &device,
            &source,
            &mesh,
            &parts,
            Mat4::IDENTITY,
        )
        .unwrap();
        assert!(geometry.tangent.is_none());
        assert_eq!(geometry.source_vertex_count(), positions.len() as u32);
        assert_eq!(
            geometry.intermediate_buffer().size(),
            geometry.counts[1] as u64 * 64
        );
        geometry.update_transform(&queue, Mat4::IDENTITY).unwrap();
        let mut encoder = device.create_command_encoder(&Default::default());
        geometry.encode(&mut encoder);
        queue.submit([encoder.finish()]);
        let actual = geometry.readback(&device, &queue);
        let (reference, _) = oracle(&mesh, &parts);
        assert_eq!(
            actual.len(),
            reference.iter().map(|p| p.positions.len()).sum::<usize>()
        );
        for (a, (p, n)) in actual.iter().zip(
            reference
                .iter()
                .flat_map(|p| p.positions.iter().zip(&p.normals)),
        ) {
            assert!(
                Vec3::from_array(a.position[..3].try_into().unwrap())
                    .distance(Vec3::from_array(*p))
                    < 1e-6
            );
            assert!(
                Vec3::from_array(a.normal[..3].try_into().unwrap()).distance(Vec3::from_array(*n))
                    < 1e-5
            );
            assert_eq!(
                a.tangent, [0.0; 4],
                "unrendered intermediate has no CPU tangent field"
            );
        }
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
        ray_geometry_audit(true, false, RecipeAudit::Off, &[0.0, 39.25, 149.0]);
    }

    #[test]
    #[ignore = "actual Ray origin-relative GPU normals require LOOM_PERF_SCENE"]
    fn actual_ray_origin_relative_sg_accuracy_against_same_position_f64() {
        ray_geometry_audit(false, false, RecipeAudit::Off, &[0.0, 39.25, 149.0]);
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
        ray_geometry_audit(false, true, RecipeAudit::Off, &times);
    }

    #[test]
    #[ignore = "actual current hybrid GPU tangents require LOOM_PERF_SCENE"]
    fn actual_ray_hybrid_current_gpu_tangents() {
        let count = std::env::var("LOOM_GPU_RECIPE_POSES")
            .ok()
            .map(|v| v.parse::<usize>().unwrap())
            .unwrap_or(3);
        let times: Vec<_> = if count == 3 {
            vec![0.0, 39.25, 149.0]
        } else {
            (0..count)
                .map(|i| 149.0 * i as f32 / (count - 1).max(1) as f32)
                .collect()
        };
        ray_geometry_audit(false, true, RecipeAudit::Strict, &times);
    }
    #[derive(PartialEq, Eq)]
    enum RecipeAudit {
        Off,
        Strict,
        Diagnostic,
        F64,
        F64Diagnostic,
    }
    #[test]
    #[ignore = "actual GPU tangent numeric diagnostic; no acceptance claim"]
    fn actual_ray_hybrid_current_gpu_tangent_diagnostics() {
        let count = std::env::var("LOOM_GPU_RECIPE_POSES")
            .ok()
            .map(|v| v.parse::<usize>().unwrap())
            .unwrap_or(3);
        let times: Vec<_> = if count == 3 {
            vec![0.0, 39.25, 149.0]
        } else {
            (0..count)
                .map(|i| 149.0 * i as f32 / (count - 1).max(1) as f32)
                .collect()
        };
        ray_geometry_audit(false, true, RecipeAudit::Diagnostic, &times);
    }
    #[test]
    #[ignore = "independent current F64 full-vertex tangent acceptance requires actual Ray scene"]
    fn actual_ray_hybrid_current_gpu_tangents_f64_acceptance() {
        let count = std::env::var("LOOM_GPU_RECIPE_POSES")
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
        ray_geometry_audit(false, true, RecipeAudit::F64, &times);
    }
    #[test]
    #[ignore = "full F64 diagnostic records failures; never an acceptance gate"]
    fn actual_ray_hybrid_current_gpu_tangents_f64_diagnostics() {
        let count = std::env::var("LOOM_GPU_RECIPE_POSES")
            .ok()
            .map(|v| v.parse::<usize>().unwrap())
            .unwrap_or(317);
        let times: Vec<_> = (0..count)
            .map(|i| 149.0 * i as f32 / (count - 1).max(1) as f32)
            .collect();
        ray_geometry_audit(false, true, RecipeAudit::F64Diagnostic, &times);
    }
    fn ray_geometry_audit(
        strict_legacy: bool,
        prepare: bool,
        recipe_math: RecipeAudit,
        times: &[f32],
    ) {
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
        let mut worst_tangent_error = 0.0f32;
        let mut recipes = std::collections::HashMap::new();
        let mut first_keys = std::collections::HashMap::new();
        let mut previous_keys = std::collections::HashMap::new();
        let (mut first_hits, mut previous_hits, mut guard_checks) = (0usize, 0usize, 0usize);
        for &time in times {
            let mut preparation_jobs = Vec::new();
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
                        preparation_jobs.push((
                            node.name.clone(),
                            part,
                            indices.len(),
                            preparation,
                            same_gpu,
                            stable.output.clone(),
                            range.start as u32,
                        ));
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
            if !preparation_jobs.is_empty() {
                assert!(device.features().contains(
                    wgpu::Features::TIMESTAMP_QUERY
                        | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS
                ));
                let queries = device.create_query_set(&wgpu::QuerySetDescriptor {
                    label: Some("actual all-part current preparation stage costs"),
                    ty: wgpu::QueryType::Timestamp,
                    count: 16,
                });
                let query_results = device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: 16 * 8,
                    usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                });
                let refs: Vec<_> = preparation_jobs.iter().map(|job| &job.3).collect();
                let mut encoder = device.create_command_encoder(&Default::default());
                let dispatch = std::time::Instant::now();
                super::mikk_prepare::GpuPreparation::encode_batch(
                    &mut encoder,
                    &refs,
                    Some(&queries),
                );
                encoder.resolve_query_set(&queries, 0..16, &query_results, 0);
                let encode_ms = dispatch.elapsed().as_secs_f64() * 1000.0;
                let submit = std::time::Instant::now();
                queue.submit([encoder.finish()]);
                let submit_ms = submit.elapsed().as_secs_f64() * 1000.0;
                let stamps: Vec<u64> = super::read_records(&device, &queue, &query_results, 16);
                let costs: Vec<_> = stamps
                    .chunks_exact(2)
                    .map(|v| (v[1] - v[0]) as f64 * f64::from(queue.get_timestamp_period()) / 1e6)
                    .collect();
                let span =
                    (stamps[15] - stamps[0]) as f64 * f64::from(queue.get_timestamp_period()) / 1e6;
                eprintln!("GPU_PREP_BATCH time={time} parts={} clear_ms={:.6} weld_ms={:.6} canonical_ms={:.6} degen_flags_ms={:.6} degen_ms={:.6} flags_ms={:.6} edges_ms={:.6} pair_ms={:.6} gpu_span_ms={span:.6} cpu_encode_ms={encode_ms:.6} cpu_submit_ms={submit_ms:.6}",refs.len(),costs[0],costs[1],costs[2],costs[3],costs[4],costs[5],costs[6],costs[7]);
                for (name, part, corners, preparation, same_gpu, vertices, base) in preparation_jobs
                {
                    let output = preparation.readback(&device, &queue).unwrap();
                    let identity = (name.clone(), part);
                    let key = output.structural_key();
                    if let Some(first) = first_keys.get(&identity) {
                        guard_checks += 1;
                        if first == &key {
                            first_hits += 1;
                        } else {
                            eprintln!("GPU_RECIPE_GUARD_MISS baseline=first node={name} time={time} part={part}");
                        }
                        if previous_keys.get(&identity) == Some(&key) {
                            previous_hits += 1;
                        } else {
                            eprintln!("GPU_RECIPE_GUARD_MISS baseline=previous node={name} time={time} part={part}");
                        }
                    } else {
                        first_keys.insert(identity.clone(), key.clone());
                    }
                    previous_keys.insert(identity, key);
                    super::mikk_prepare::compare_cpu(&same_gpu, &output);
                    if recipe_math != RecipeAudit::Off {
                        let cached = recipes.entry((name.clone(), part)).or_insert_with(|| {
                            (
                                output.clone(),
                                crate::mikktspace::compile_structure(&same_gpu),
                            )
                        });
                        let math = super::mikk_recipe::GpuRecipeMath::new(
                            &device,
                            &vertices,
                            base,
                            &same_gpu,
                            &preparation,
                            &cached.0,
                            &cached.1,
                        )
                        .unwrap();
                        let mut encoder = device.create_command_encoder(&Default::default());
                        math.encode(&mut encoder);
                        queue.submit([encoder.finish()]);
                        let candidate = math.readback(&device, &queue).unwrap();
                        let reference = crate::mikktspace::generate_serial(&same_gpu)
                            .expect("actual current CPU tangent reference");
                        assert_eq!(candidate.len(), same_gpu.positions.len());
                        assert_eq!(reference.len(), candidate.len());
                        let mut max_error = 0.0f32;
                        let mut worst_vertex = 0;
                        let mut different = 0;
                        for (i, (actual, expected)) in candidate.iter().zip(&reference).enumerate()
                        {
                            assert_eq!(actual.tangent[3],expected[3],"hybrid CURRENT handedness node={name} time={time} part={part} vertex={i}");
                            assert!(actual.tangent.iter().all(|v| v.is_finite()));
                            let error =
                                glam::Vec3::from_array(actual.tangent[..3].try_into().unwrap())
                                    .distance(glam::Vec3::from_array(
                                        expected[..3].try_into().unwrap(),
                                    ));
                            if error > max_error {
                                max_error = error;
                                worst_vertex = i;
                            }
                            different += usize::from(
                                actual.tangent.map(f32::to_bits) != expected.map(f32::to_bits),
                            );
                        }
                        if max_error > worst_tangent_error {
                            worst_tangent_error = max_error;
                            let owner = same_gpu
                                .indices
                                .iter()
                                .rposition(|v| *v as usize == worst_vertex)
                                .unwrap();
                            let face = output
                                .original_faces
                                .iter()
                                .position(|f| *f as usize == owner / 3)
                                .unwrap();
                            let group = cached.1.assigned_groups[face][owner % 3];
                            if group >= 0 {
                                let group = &cached.1.groups[group as usize];
                                let source = (group.representative as usize >> 2) * 3
                                    + (group.representative as usize & 3);
                                let mut members = group.members.clone();
                                members.sort_unstable();
                                let records:Vec<_>=members.iter().map(|m|{let orig=output.original_faces[*m as usize] as usize;let ids=&same_gpu.indices[orig*3..orig*3+3];serde_json::json!({"prepared":m,"original":orig,"flags":cached.1.triangles[*m as usize].flags,"position":ids.iter().map(|i|same_gpu.positions[*i as usize]).collect::<Vec<_>>(),"uv":ids.iter().map(|i|same_gpu.uvs[*i as usize]).collect::<Vec<_>>()})}).collect();
                                let mut artifact = serde_json::json!({"node":name,"part":part,"time":time,"vector_error":max_error,"vertex":worst_vertex,"target_original_face":owner/3,"normal":same_gpu.normals[same_gpu.indices[source] as usize],"representative_position":same_gpu.positions[same_gpu.indices[source] as usize],"gpu":candidate[worst_vertex].tangent,"cpu":reference[worst_vertex],"members":records});
                                if recipe_math == RecipeAudit::Diagnostic {
                                    let current_structure =
                                        crate::mikktspace::capture_group_preparation(&same_gpu);
                                    let isolation = math.diagnose_current_math(
                                        &device,
                                        &queue,
                                        &same_gpu,
                                        &output,
                                        &current_structure,
                                        &reference,
                                        worst_vertex,
                                    );
                                    eprintln!("GPU_STAGE_ISOLATION node={name} time={time} part={part} report={isolation}");
                                    artifact["isolation"] = isolation;
                                }
                                let safe_name: String = name
                                    .chars()
                                    .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                                    .collect();
                                std::fs::write(
                                    format!(
                                        "/tmp/loom-gpu-hybrid-worst-{safe_name}-{time}-{part}.json"
                                    ),
                                    serde_json::to_vec_pretty(&artifact).unwrap(),
                                )
                                .unwrap();
                                std::fs::write(
                                    "/tmp/loom-gpu-hybrid-worst.json",
                                    serde_json::to_vec_pretty(&artifact).unwrap(),
                                )
                                .unwrap();
                            }
                        }
                        if matches!(recipe_math, RecipeAudit::F64 | RecipeAudit::F64Diagnostic) {
                            let truth = super::mikk_f64::tangents_f64(&same_gpu, &cached.1);
                            assert_eq!(truth.len(), candidate.len());
                            let mut worst = 0.0f64;
                            let mut offenders = 0;
                            for (i, ((actual, cpu), f64)) in
                                candidate.iter().zip(&reference).zip(&truth).enumerate()
                            {
                                assert_eq!(f64::from(actual.tangent[3]), f64[3]);
                                let error =
                                    glam::Vec3::from_array(actual.tangent[..3].try_into().unwrap())
                                        .as_dvec3()
                                        .distance(glam::DVec3::from_array(
                                            f64[..3].try_into().unwrap(),
                                        ));
                                worst = worst.max(error);
                                let cpu_delta =
                                    glam::Vec3::from_array(actual.tangent[..3].try_into().unwrap())
                                        .distance(glam::Vec3::from_array(
                                            cpu[..3].try_into().unwrap(),
                                        ));
                                if cpu_delta > 2e-5 {
                                    offenders += 1;
                                    let cpu_error =
                                        glam::Vec3::from_array(cpu[..3].try_into().unwrap())
                                            .as_dvec3()
                                            .distance(glam::DVec3::from_array(
                                                f64[..3].try_into().unwrap(),
                                            ));
                                    eprintln!("GPU_F64_CPU_OFFENDER node={name} time={time} part={part} vertex={i} gpu_f64_error={error:e} cpu_f64_error={cpu_error:e} cpu_delta={cpu_delta:e}");
                                }
                                if error >= 2e-5 {
                                    eprintln!("GPU_F64_GATE_FAILURE node={name} time={time} part={part} vertex={i} gpu_f64_error={error:e}");
                                }
                                if recipe_math == RecipeAudit::F64 {
                                    assert!(error<2e-5,"independent F64 tangent mismatch node={name} time={time} part={part} vertex={i}: {error:e}");
                                }
                            }
                            eprintln!("GPU_F64_ALL_VERTICES node={name} time={time} part={part} vertices={} max_error={worst:e} cpu_offenders={offenders}",truth.len());
                        }
                        eprintln!("GPU_CURRENT_TANGENTS node={name} time={time} part={part} vertices={} different_bits={different} max_vector_error={max_error:e}",candidate.len());
                        if recipe_math == RecipeAudit::Strict {
                            assert!(max_error<2e-5,"current hybrid tangent mismatch node={name} time={time} part={part}: {max_error:e}");
                        }
                    }

                    eprintln!("GPU_CURRENT_PREP node={name} time={time} part={part} corners={corners} degen={} any={} exact_weld_flags_neighbors=PASS",corners/3-output.active_count,output.flags[..output.active_count].iter().filter(|f|**f&4!=0).count());
                }
            }
        }
        if prepare {
            eprintln!("GPU_RECIPE_GUARD_RESULT poses={} parts={} checks={guard_checks} first_hits={first_hits} previous_hits={previous_hits}",times.len(),first_keys.len());
        }
    }
}

#[cfg(test)]
mod descriptor_snapshot_tests {
    use super::*;
    fn equivalent(a:&Topology,b:&Topology)->bool {
        a.vertices==b.vertices&&a.faces==b.faces&&a.parts==b.parts&&a.arena==b.arena&&bytemuck::cast_slice::<Corner,u8>(&a.corners)==bytemuck::cast_slice::<Corner,u8>(&b.corners)
    }
    #[test]
    fn exact_raw_snapshot_excludes_positions_and_rejects_changed_inputs(){
        let mut mesh=loom_mesh::sphere_segmented(1.0,8,0.0);
        loom_mesh::uvmap::auto_map(&mut mesh,loom_mesh::uvmap::AutoMap::Spherical,1);
        let snapshot=DescriptorSnapshot::new(&mesh);let parts=[(None,false)];let original=Topology::new(&mesh,&parts).unwrap();
        let mut posed=mesh.clone();posed.vertices_mut()[0].position+=Vec3::new(2.0,-3.0,4.0);
        posed.vertices_mut()[1].color=[0.0;3];
        assert!(snapshot.matches(&posed));assert!(equivalent(&original,&Topology::new(&posed,&parts).unwrap()));
        for field in 0..8 {
            let mut changed=mesh.clone();
            match field {
                0=>changed.faces_mut()[0].hidden=true,
                1=>changed.faces_mut()[0].material_id+=1,
                2=>changed.faces_mut()[0].smoothing_groups^=1,
                3=>changed.faces_mut()[0].he=changed.faces()[1].he,
                4=>changed.vertices_mut()[0].he=None,
                5=>changed.map_mut(1).unwrap().verts[0].x+=0.25,
                6=>changed.map_mut(1).unwrap().corners[0]=changed.map(1).unwrap().corners[0].wrapping_add(1),
                _=>{changed.ensure_map(loom_mesh::NORMALS_CHANNEL).verts[0]=Vec3::X;},
            }
            assert!(!snapshot.matches(&changed),"field={field}");
        }
        let mut raw=serde_json::to_value(&mesh).unwrap();raw["half_edges"][0]["origin"]=serde_json::json!(1);
        let changed:Mesh=serde_json::from_value(raw).unwrap();assert!(!snapshot.matches(&changed));
        let mut zero=mesh.clone();zero.map_mut(1).unwrap().verts[0].x=0.0;let zero_snapshot=DescriptorSnapshot::new(&zero);
        zero.map_mut(1).unwrap().verts[0].x=-0.0;assert!(!zero_snapshot.matches(&zero));
        let mut nan=mesh.clone();nan.map_mut(1).unwrap().verts[0].x=f32::from_bits(0x7fc00001);let nan_snapshot=DescriptorSnapshot::new(&nan);
        assert!(nan_snapshot.matches(&nan));nan.map_mut(1).unwrap().verts[0].x=f32::from_bits(0x7fc00002);assert!(!nan_snapshot.matches(&nan));
    }
    #[test]
    #[ignore="actual descriptor oracle/timing requires LOOM_PERF_SCENE"]
    fn actual_ray_raw_descriptors_match_rebuilt_topology(){
        let value:serde_json::Value=serde_json::from_str(&std::fs::read_to_string(std::env::var("LOOM_PERF_SCENE").unwrap()).unwrap()).unwrap();
        let scene:loom_scene::Scene=serde_json::from_value(value["scene"].clone()).unwrap();
        let ids:Vec<_>=scene.nodes().filter(|n|n.modifiers.iter().any(|m|matches!(&m.modifier,loom_scene::modifier::Modifier::Skin(_)))).map(|n|n.id).collect();assert_eq!(ids.len(),5);
        let benchmark=std::env::var_os("LOOM_DESCRIPTOR_BENCH").is_some();
        let frames:Vec<_>=if benchmark{vec![0.0,39.25,149.0]}else{(0..=157).map(|f|f as f32).chain([39.25]).chain((0..=157).map(|f|f as f32+0.5)).collect()};let poses=frames.len();let repeats=if benchmark{16}else{1};
        let mut originals=std::collections::HashMap::new();let mut old_time=std::time::Duration::ZERO;let mut new_time=old_time;let mut checks=0;
        for frame in frames {
            scene.set_time(frame);
            for id in &ids {
                let mut mesh=scene.eval_mesh(*id).value.mesh.clone();loom_mesh::edit_ops::transform_mesh(&mut mesh,scene.object_matrix(*id)).unwrap();
                let surfaces=scene.node_surfaces(*id).unwrap();let materials:std::collections::BTreeSet<_>=mesh.faces().iter().filter(|f|!f.hidden).map(|f|f.material_id).collect();let parts:Vec<_>=materials.into_iter().map(|m|(Some(m),surfaces.for_face(m).faceted)).collect();
                let (snapshot,original)=originals.entry(*id).or_insert_with(||(DescriptorSnapshot::new(&mesh),Topology::new(&mesh,&parts).unwrap()));
                // Alternate timed order; both compare the same current mesh.
                for iteration in 0..repeats {
                    let mut old=||{let t=std::time::Instant::now();let current=Topology::new(&mesh,&parts).unwrap();let same=equivalent(original,&current);old_time+=t.elapsed();assert!(same);};
                    let mut new=||{let t=std::time::Instant::now();let same=snapshot.matches(&mesh);new_time+=t.elapsed();assert!(same);};
                    if iteration%2==0{old();new();}else{new();old();}
                }
                checks+=1;
            }
        }
        eprintln!("DESCRIPTORS poses={poses} checks={checks} repeats={repeats} old_ms_per_pose={:.3} snapshot_ms_per_pose={:.3}",old_time.as_secs_f64()*1000.0/(poses*repeats)as f64,new_time.as_secs_f64()*1000.0/(poses*repeats)as f64);
    }
}
