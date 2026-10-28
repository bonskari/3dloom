//! Exact current-position edge gathering. This component does not select a
//! renderer backend or establish a current-pose epoch; callers own that proof.

use crate::MeshStyle;
use loom_mesh::{HalfEdgeId, Mesh};
use std::{fmt, num::NonZeroU64};
use wgpu::util::DeviceExt;

#[derive(Debug, PartialEq, Eq)]
pub enum GpuEdgeError {
    Diagonals,
    Invalid(&'static str),
    InvalidMesh(String),
}
impl fmt::Display for GpuEdgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Diagonals => f.write_str("diagonal styles require the ordinary edge path"),
            Self::Invalid(message) => f.write_str(message),
            Self::InvalidMesh(message) => write!(f, "invalid edge mesh: {message}"),
        }
    }
}
impl std::error::Error for GpuEdgeError {}

/// Source positions are raw vec4 records, including unused mesh vertices.
/// The caller must prove current pose/mesh identity, same-device ownership and
/// submission ordering; a cloned buffer handle does not freeze its contents.
pub struct EdgeSource<'a> {
    pub buffer: &'a wgpu::Buffer,
    pub offset: u64,
    pub vertex_count: usize,
}

#[derive(Clone)]
pub struct GpuEdgeProgram {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::ComputePipeline,
}
impl GpuEdgeProgram {
    pub fn new(device: &wgpu::Device) -> Result<Self, GpuEdgeError> {
        let limits = device.limits();
        if limits.max_storage_buffers_per_shader_stage < 3
            || limits.max_compute_workgroup_size_x < 64
            || limits.max_compute_invocations_per_workgroup < 64
        {
            return Err(GpuEdgeError::Invalid(
                "device does not support the edge gather layout",
            ));
        }
        let entries = [
            (0, wgpu::BufferBindingType::Storage { read_only: true }),
            (1, wgpu::BufferBindingType::Storage { read_only: true }),
            (2, wgpu::BufferBindingType::Storage { read_only: false }),
            (3, wgpu::BufferBindingType::Uniform),
        ]
        .map(|(binding, ty)| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("exact edge gather"),
            entries: &entries,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("exact edge gather"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("exact edge gather"),
            source: wgpu::ShaderSource::Wgsl(include_str!("gpu_edges.wgsl").into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("exact edge gather"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("gather"),
            compilation_options: Default::default(),
            cache: None,
        });
        Ok(Self { layout, pipeline })
    }
}

/// An immutable endpoint recipe and persistent output buffer. Source-buffer,
/// topology or effective visibility changes require a new instance. Positions
/// and wire colour can change without rebuilding. No final tangent data is used.
pub struct GpuEdges {
    program: GpuEdgeProgram,
    binding: wgpu::BindGroup,
    output: wgpu::Buffer,
    params: wgpu::Buffer,
    vertex_count: u32,
    source_count: usize,
    source: wgpu::Buffer,
    source_offset: u64,
    topology: loom_mesh::HalfEdgeSnapshot,
    visible: Vec<bool>,
    all_visible: bool,
}
impl GpuEdges {
    pub fn new(
        device: &wgpu::Device,
        program: &GpuEdgeProgram,
        source: EdgeSource<'_>,
        mesh: &Mesh,
        style: &MeshStyle,
    ) -> Result<Self, GpuEdgeError> {
        if style.diagonals {
            return Err(GpuEdgeError::Diagonals);
        }
        if source.vertex_count != mesh.vertex_count() {
            return Err(GpuEdgeError::Invalid(
                "source and mesh vertex counts differ",
            ));
        }
        if mesh.half_edge_count() > u32::MAX as usize {
            return Err(GpuEdgeError::Invalid("too many halfedges"));
        }
        mesh.check_invariants().map_err(GpuEdgeError::InvalidMesh)?;
        let limits = device.limits();
        let source_bytes = u64::try_from(source.vertex_count)
            .ok()
            .and_then(|v| v.checked_mul(16))
            .ok_or(GpuEdgeError::Invalid("source size overflow"))?
            .max(4);
        if !source.buffer.usage().contains(wgpu::BufferUsages::STORAGE)
            || source.offset % u64::from(limits.min_storage_buffer_offset_alignment) != 0
            || source
                .offset
                .checked_add(source_bytes)
                .is_none_or(|end| end > source.buffer.size())
            || source_bytes > u64::from(limits.max_storage_buffer_binding_size)
        {
            return Err(GpuEdgeError::Invalid(
                "invalid source storage range or usage",
            ));
        }
        let half_edges: Vec<_> = mesh
            .half_edges()
            .iter()
            .map(|he| (he.origin.0, he.twin.0))
            .collect();
        let visible: Vec<_> = (0..half_edges.len())
            .map(|i| mesh.edge_visible(HalfEdgeId(i as u32)))
            .collect();
        let mut endpoints = Vec::new();
        for (i, &(origin, twin)) in half_edges.iter().enumerate() {
            if (i as u32) < twin && visible[i] {
                endpoints.push(origin);
                endpoints.push(half_edges[twin as usize].0);
            }
        }
        let vertex_count = u32::try_from(endpoints.len())
            .map_err(|_| GpuEdgeError::Invalid("too many edge vertices"))?;
        let output_bytes = u64::from(vertex_count)
            .checked_mul(24)
            .ok_or(GpuEdgeError::Invalid("edge size overflow"))?
            .max(24);
        let endpoint_bytes = (endpoints.len() as u64)
            .checked_mul(4)
            .ok_or(GpuEdgeError::Invalid("endpoint size overflow"))?
            .max(4);
        if output_bytes > u64::from(limits.max_storage_buffer_binding_size)
            || endpoint_bytes > u64::from(limits.max_storage_buffer_binding_size)
            || output_bytes > limits.max_buffer_size
            || vertex_count.div_ceil(64) > limits.max_compute_workgroups_per_dimension
        {
            return Err(GpuEdgeError::Invalid(
                "edge gather exceeds device storage or dispatch limits",
            ));
        }
        let output = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("exact current edge vertices"),
            size: output_bytes,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let endpoint_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("exact ordered edge endpoints"),
            contents: if endpoints.is_empty() {
                bytemuck::bytes_of(&0u32)
            } else {
                bytemuck::cast_slice(&endpoints)
            },
            usage: wgpu::BufferUsages::STORAGE,
        });
        let words = parameters(vertex_count, style);
        let params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("current edge colour"),
            contents: bytemuck::cast_slice(&words),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("exact current edge gather"),
            layout: &program.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: source.buffer,
                        offset: source.offset,
                        size: NonZeroU64::new(source_bytes),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: endpoint_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: output.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: params.as_entire_binding(),
                },
            ],
        });
        Ok(Self {
            program: program.clone(),
            binding,
            output,
            params,
            vertex_count,
            source_count: source.vertex_count,
            source: source.buffer.clone(),
            source_offset: source.offset,
            topology: mesh.half_edge_snapshot(),
            all_visible: visible.iter().all(|visible| *visible),
            visible,
        })
    }
    /// Conservative connectivity and exact effective visibility guard. Does not
    /// establish source identity or pose freshness; a miss requires rebuilding.
    pub fn matches_geometry(&self, mesh: &Mesh, style: &MeshStyle) -> Result<bool, GpuEdgeError> {
        if style.diagonals {
            return Err(GpuEdgeError::Diagonals);
        }
        Ok(self.source_count == mesh.vertex_count()
            && self.topology.matches(mesh)
            && if self.all_visible {
                // The same effective predicate used by ordinary edge emission;
                // an absent visibility array needs no per-half-edge scan.
                !mesh.has_invisible_edges()
            } else {
                self.visible.iter().enumerate()
                    .all(|(i, visible)| *visible == mesh.edge_visible(HalfEdgeId(i as u32)))
            })
    }
    /// A replacement source allocation or changed range requires rebinding,
    /// even when mesh topology is unchanged.
    pub fn matches_source(&self, source: &EdgeSource<'_>) -> bool {
        self.source == *source.buffer
            && self.source_offset == source.offset
            && self.source_count == source.vertex_count
    }
    pub fn update_style(&self, queue: &wgpu::Queue, style: &MeshStyle) -> Result<(), GpuEdgeError> {
        if style.diagonals {
            return Err(GpuEdgeError::Diagonals);
        }
        queue.write_buffer(
            &self.params,
            0,
            bytemuck::cast_slice(&parameters(self.vertex_count, style)),
        );
        Ok(())
    }
    /// Record before drawing output, on the source's device/queue. Callers must
    /// not update the source/parameters for a later pose before submission.
    pub fn encode(&self, encoder: &mut wgpu::CommandEncoder) {
        if self.vertex_count == 0 {
            return;
        }
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("exact current edge gather"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.program.pipeline);
        pass.set_bind_group(0, &self.binding, &[]);
        pass.dispatch_workgroups(self.vertex_count.div_ceil(64), 1, 1);
    }
    pub fn output_buffer(&self) -> &wgpu::Buffer {
        &self.output
    }
    pub fn vertex_count(&self) -> u32 {
        self.vertex_count
    }
}
fn parameters(count: u32, style: &MeshStyle) -> [u32; 8] {
    let color = style.wire_color.unwrap_or(style.color);
    [
        count,
        0,
        0,
        0,
        color[0].to_bits(),
        color[1].to_bits(),
        color[2].to_bits(),
        0,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{grid::LineVertex, MeshStyle};
    use glam::Vec3;
    use wgpu::util::DeviceExt;

    fn device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(&Default::default());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
            .expect("real GPU required");
        pollster::block_on(adapter.request_device(&Default::default(), None)).unwrap()
    }
    fn positions(mesh: &loom_mesh::Mesh) -> Vec<[f32; 4]> {
        mesh.vertices()
            .iter()
            .map(|v| [v.position.x, v.position.y, v.position.z, 1.0])
            .collect()
    }
    fn expected(mesh: &loom_mesh::Mesh, style: &MeshStyle) -> Vec<LineVertex> {
        let color = style.wire_color.unwrap_or(style.color);
        mesh.unique_edges()
            .iter()
            .flat_map(|(a, b)| {
                [
                    LineVertex {
                        position: a.to_array(),
                        color,
                    },
                    LineVertex {
                        position: b.to_array(),
                        color,
                    },
                ]
            })
            .collect()
    }
    fn read(device: &wgpu::Device, queue: &wgpu::Queue, edges: &GpuEdges) -> Vec<u8> {
        if edges.vertex_count() == 0 {
            return Vec::new();
        }
        let bytes = edges.vertex_count() as u64 * 24;
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: bytes,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        edges.encode(&mut encoder);
        encoder.copy_buffer_to_buffer(edges.output_buffer(), 0, &staging, 0, bytes);
        queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        staging
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
        device.poll(wgpu::Maintain::Wait);
        rx.recv().unwrap().unwrap();
        let result = staging.slice(..).get_mapped_range().to_vec();
        staging.unmap();
        result
    }
    #[test]
    fn current_gpu_edges_match_cpu_bits_and_require_descriptor_rebuilds() {
        let (device, queue) = device();
        let program = GpuEdgeProgram::new(&device).unwrap();
        let mut mesh = loom_mesh::box_primitive(Vec3::ONE);
        mesh.vertices_mut()[0].position = Vec3::new(-0.0, 0.0, -0.0);
        let source = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&positions(&mesh)),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let mut style = MeshStyle {
            wire_color: Some([-0.0, 0.3, 0.8]),
            ..Default::default()
        };
        let edges = GpuEdges::new(
            &device,
            &program,
            EdgeSource {
                buffer: &source,
                offset: 0,
                vertex_count: mesh.vertex_count(),
            },
            &mesh,
            &style,
        )
        .unwrap();
        let allocation = edges.output_buffer().clone();
        for pose in 0..3 {
            if pose > 0 {
                for v in mesh.vertices_mut() {
                    v.position += Vec3::new(0.125, -0.25, 0.5);
                }
            }
            style.wire_color = Some([pose as f32 / 3.0, -0.0, 0.75]);
            assert!(edges.matches_geometry(&mesh, &style).unwrap());
            queue.write_buffer(&source, 0, bytemuck::cast_slice(&positions(&mesh)));
            edges.update_style(&queue, &style).unwrap();
            assert_eq!(
                read(&device, &queue, &edges),
                bytemuck::cast_slice::<_, u8>(&expected(&mesh, &style))
            );
            assert_eq!(allocation, *edges.output_buffer());
        }
        let mut reordered = loom_mesh::indexed::IndexedMesh::from_mesh(&mesh);
        reordered.faces[0].ids.rotate_left(1);
        let reordered = reordered.build().unwrap();
        assert_eq!(reordered.vertex_count(), mesh.vertex_count());
        assert_eq!(reordered.half_edge_count(), mesh.half_edge_count());
        assert!(!edges.matches_geometry(&reordered, &style).unwrap());
        let replacement = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: source.size(),
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        assert!(!edges.matches_source(&EdgeSource {
            buffer: &replacement,
            offset: 0,
            vertex_count: mesh.vertex_count()
        }));
        mesh.set_edge_visible(loom_mesh::HalfEdgeId(0), false);
        assert!(!edges.matches_geometry(&mesh, &style).unwrap());
        let hidden = GpuEdges::new(
            &device,
            &program,
            EdgeSource {
                buffer: &source,
                offset: 0,
                vertex_count: mesh.vertex_count(),
            },
            &mesh,
            &style,
        )
        .unwrap();
        assert_eq!(
            read(&device, &queue, &hidden),
            bytemuck::cast_slice::<_, u8>(&expected(&mesh, &style))
        );
        mesh.set_edge_visible(loom_mesh::HalfEdgeId(0), true);
        assert!(!hidden.matches_geometry(&mesh, &style).unwrap());
        let changed = loom_mesh::sphere_segmented(1.0, 8, 0.0);
        assert!(!edges.matches_geometry(&changed, &style).unwrap());
        style.diagonals = true;
        assert!(matches!(
            edges.matches_geometry(&mesh, &style),
            Err(GpuEdgeError::Diagonals)
        ));
        assert!(matches!(
            edges.update_style(&queue, &style),
            Err(GpuEdgeError::Diagonals)
        ));
        assert!(matches!(
            GpuEdges::new(
                &device,
                &program,
                EdgeSource {
                    buffer: &source,
                    offset: 0,
                    vertex_count: mesh.vertex_count()
                },
                &mesh,
                &style
            ),
            Err(GpuEdgeError::Diagonals)
        ));
    }
    #[test]
    fn gpu_edge_source_ranges_are_checked_before_binding() {
        let (device, _) = device();
        let program = GpuEdgeProgram::new(&device).unwrap();
        let mesh = loom_mesh::box_primitive(Vec3::ONE);
        let style = MeshStyle::default();
        let source = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 16,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        assert!(GpuEdges::new(
            &device,
            &program,
            EdgeSource {
                buffer: &source,
                offset: 0,
                vertex_count: mesh.vertex_count()
            },
            &mesh,
            &style
        )
        .is_err());
        let source = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 1024,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        for (offset, count) in [
            (1, mesh.vertex_count()),
            (u64::MAX, mesh.vertex_count()),
            (0, usize::MAX),
            (0, 0),
        ] {
            assert!(GpuEdges::new(
                &device,
                &program,
                EdgeSource {
                    buffer: &source,
                    offset,
                    vertex_count: count
                },
                &mesh,
                &style
            )
            .is_err());
        }
        let wrong = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 1024,
            usage: wgpu::BufferUsages::VERTEX,
            mapped_at_creation: false,
        });
        assert!(GpuEdges::new(
            &device,
            &program,
            EdgeSource {
                buffer: &wrong,
                offset: 0,
                vertex_count: mesh.vertex_count()
            },
            &mesh,
            &style
        )
        .is_err());
    }
    #[test]
    fn open_empty_unused_vertices_and_nonzero_source_ranges_are_exact() {
        let (device, queue) = device();
        let program = GpuEdgeProgram::new(&device).unwrap();
        let quad = loom_mesh::quad_primitive(Vec3::ZERO, Vec3::X, Vec3::Y);
        let mut unused = loom_mesh::box_primitive(Vec3::ONE);
        let faces: Vec<_> = (1..unused.face_count())
            .map(|i| loom_mesh::FaceId(i as u32))
            .collect();
        loom_mesh::edit_ops::delete_faces_keep_vertices(&mut unused, &faces).unwrap();
        assert!(unused.vertices().iter().any(|v| v.he.is_none()));
        for mesh in [quad, unused, loom_mesh::Mesh::new()] {
            let offset = u64::from(device.limits().min_storage_buffer_offset_alignment);
            let size = (mesh.vertex_count() as u64 * 16).max(4);
            let source = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: offset + size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            if mesh.vertex_count() > 0 {
                queue.write_buffer(&source, offset, bytemuck::cast_slice(&positions(&mesh)));
            }
            let descriptor = EdgeSource {
                buffer: &source,
                offset,
                vertex_count: mesh.vertex_count(),
            };
            let style = MeshStyle {
                color: [-0.0, 0.4, 1.0],
                wire_color: None,
                ..Default::default()
            };
            let edges = GpuEdges::new(&device, &program, descriptor, &mesh, &style).unwrap();
            assert!(edges.matches_source(&EdgeSource {
                buffer: &source,
                offset,
                vertex_count: mesh.vertex_count()
            }));
            assert!(!edges.matches_source(&EdgeSource {
                buffer: &source,
                offset: 0,
                vertex_count: mesh.vertex_count()
            }));
            assert_eq!(
                read(&device, &queue, &edges),
                bytemuck::cast_slice::<_, u8>(&expected(&mesh, &style))
            );
            assert_eq!(std::mem::size_of::<LineVertex>(), 24);
            assert!(edges
                .output_buffer()
                .usage()
                .contains(wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::STORAGE));
        }
    }
}
