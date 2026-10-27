//! Test-only GPU current-attribute welding and corrected edge preparation.
//! Material parts are independent. No rest-pose adjacency is reused.
#[cfg(test)]
use super::PackedVertex;
use wgpu::util::DeviceExt;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MeshData;

    fn vertex(position: [f32; 3], uv: [f32; 2]) -> PackedVertex {
        PackedVertex {
            position: [position[0], position[1], position[2], 1.0],
            normal: [0.0, 0.0, 1.0, 0.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            uv: [uv[0], uv[1], 0.0, 0.0],
        }
    }
    fn device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(&Default::default());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
            .expect("real GPU required");
        pollster::block_on(adapter.request_device(&crate::device_descriptor(&adapter), None))
            .unwrap()
    }
    #[test]
    fn current_weld_handles_signed_zero_seams_degenerate_and_nonmanifold_edges() {
        let (device, queue) = device();
        let vertices = vec![
            vertex([0.0, 0.0, 0.0], [0.0, 0.0]),
            vertex([1.0, 0.0, 0.0], [1.0, 0.0]),
            vertex([0.0, 1.0, 0.0], [0.0, 1.0]),
            vertex([-0.0, 0.0, 0.0], [-0.0, 0.0]),
            vertex([0.0, -1.0, 0.0], [0.0, -1.0]),
            vertex([0.0, 0.0, 1.0], [0.0, 1.0]),
            vertex([0.0, 0.0, 0.0], [0.5, 0.0]),
        ];
        // Four incidences around the first edge: deterministic opposing pairs.
        // A repeated-position degenerate is interspersed before later good faces.
        let indices = vec![0, 1, 2, 0, 0, 1, 1, 3, 4, 0, 1, 5, 1, 0, 4, 6, 1, 2];
        let output = run(&device, &queue, &vertices, &indices);
        assert_eq!(output.active_count, 5);
        assert_eq!(output.canonical[0], output.canonical[7], "signed zero weld");
        assert_ne!(
            output.canonical[0], output.canonical[15],
            "UV seam must remain split"
        );
        assert_eq!(
            output.original_faces,
            vec![0, 2, 3, 4, 5, 1],
            "original CPU DegenPrologue ordering"
        );
        assert_eq!(output.neighbors[0][0], 1);
        assert_eq!(output.neighbors[1][0], 0);
        assert_eq!(output.neighbors[2][0], 3);
        assert_eq!(output.neighbors[3][0], 2);
        assert_eq!(output.neighbors[4][0], -1, "UV seam must not connect");
        let data = MeshData {
            positions: vertices
                .iter()
                .map(|v| v.position[..3].try_into().unwrap())
                .collect(),
            normals: vertices
                .iter()
                .map(|v| v.normal[..3].try_into().unwrap())
                .collect(),
            uvs: vertices
                .iter()
                .map(|v| v.uv[..2].try_into().unwrap())
                .collect(),
            indices,
        };
        compare_cpu(&data, &output);
    }
    #[test]
    fn current_normal_seams_update_in_place_and_all_degenerate_tails_remain() {
        let (device, queue) = device();
        let mut vertices = vec![
            vertex([0.0, 0.0, 0.0], [0.0, 0.0]),
            vertex([1.0, 0.0, 0.0], [1.0, 0.0]),
            vertex([0.0, 1.0, 0.0], [0.0, 1.0]),
        ];
        vertices.extend_from_within(..);
        let indices = [0, 1, 2, 3, 5, 4];
        let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("persistent CURRENT attributes"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let preparation = GpuPreparation::new(&device, &input, vertices.len(), &indices).unwrap();
        for pose in 0..2 {
            if pose == 1 {
                for vertex in &mut vertices[3..] {
                    vertex.normal = [0.0, 0.0, -1.0, 0.0];
                }
                queue.write_buffer(&input, 0, bytemuck::cast_slice(&vertices));
            }
            let mut encoder = device.create_command_encoder(&Default::default());
            preparation.encode(&mut encoder);
            queue.submit([encoder.finish()]);
            let output = preparation.readback(&device, &queue).unwrap();
            assert_eq!(
                output.canonical[0] == output.canonical[3],
                pose == 0,
                "current normal seam must invalidate weld without buffer replacement"
            );
            let data = MeshData {
                positions: vertices
                    .iter()
                    .map(|v| v.position[..3].try_into().unwrap())
                    .collect(),
                normals: vertices
                    .iter()
                    .map(|v| v.normal[..3].try_into().unwrap())
                    .collect(),
                uvs: vertices
                    .iter()
                    .map(|v| v.uv[..2].try_into().unwrap())
                    .collect(),
                indices: indices.to_vec(),
            };
            compare_cpu(&data, &output);
        }
        let degenerate = run(&device, &queue, &vertices, &[0, 0, 1, 3, 3, 5]);
        assert_eq!(degenerate.active_count, 0);
        assert_eq!(degenerate.flags, vec![1, 1]);
        assert_eq!(degenerate.original_faces, vec![0, 1]);
    }
    #[test]
    fn forced_hash_collisions_preserve_full_key_equality_and_invalid_attrs_report_error() {
        let (device, queue) = device();
        let mut vertices = vec![
            vertex([0.0, 0.0, 0.0], [0.0, 0.0]),
            vertex([1.0, 0.0, 0.0], [1.0, 0.0]),
            vertex([0.0, 1.0, 0.0], [0.0, 1.0]),
            vertex([0.0, 0.0, 0.0], [0.5, 0.0]),
            vertex([0.0, -1.0, 0.0], [0.0, -1.0]),
        ];
        let indices = [0, 1, 2, 1, 0, 4, 3, 1, 2];
        let source = include_str!("gpu_mikk_prepare.wgsl")
            .replace("return h;", "return 0u;")
            .replace(
                "var slot=mix(mix(2166136261u,key.x),key.y)&params.counts.w;",
                "var slot=0u;",
            );
        assert!(source.contains("return 0u;") && source.contains("var slot=0u;"));
        let program = Program::with_shader(&device, &source).unwrap();
        let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("collision current attrs"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let preparation =
            GpuPreparation::with_program(&device, &input, vertices.len(), &indices, &program)
                .unwrap();
        let mut encoder = device.create_command_encoder(&Default::default());
        preparation.encode(&mut encoder);
        queue.submit([encoder.finish()]);
        let output = preparation.readback(&device, &queue).unwrap();
        let data = MeshData {
            positions: vertices
                .iter()
                .map(|v| v.position[..3].try_into().unwrap())
                .collect(),
            normals: vertices
                .iter()
                .map(|v| v.normal[..3].try_into().unwrap())
                .collect(),
            uvs: vertices
                .iter()
                .map(|v| v.uv[..2].try_into().unwrap())
                .collect(),
            indices: indices.to_vec(),
        };
        compare_cpu(&data, &output);
        vertices[0].normal[0] = f32::NAN;
        queue.write_buffer(&input, 0, bytemuck::cast_slice(&vertices));
        let mut encoder = device.create_command_encoder(&Default::default());
        preparation.encode(&mut encoder);
        queue.submit([encoder.finish()]);
        assert!(preparation
            .readback(&device, &queue)
            .unwrap_err()
            .contains("nonfinite"));
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Prepared {
    pub(crate) active_count: usize,
    pub(crate) canonical: Vec<u32>,
    pub(crate) original_faces: Vec<u32>,
    pub(crate) flags: Vec<u32>,
    neighbors: Vec<[i32; 3]>,
}
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct StructuralKey {
    canonical: Vec<u32>,
    original_faces: Vec<u32>,
    flags: Vec<u32>,
}
impl Prepared {
    pub(crate) fn structural_key(&self) -> StructuralKey {
        StructuralKey {
            canonical: self.canonical.clone(),
            original_faces: self.original_faces.clone(),
            flags: self.flags.clone(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    counts: [u32; 4],
    offsets0: [u32; 4],
    offsets1: [u32; 4],
}
pub(crate) struct GpuPreparation {
    pipelines: Vec<wgpu::ComputePipeline>,
    group: wgpu::BindGroup,
    arena: wgpu::Buffer,
    weld: wgpu::Buffer,
    edges: wgpu::Buffer,
    params: Params,
    words: usize,
}
pub(crate) struct Program {
    layout: wgpu::BindGroupLayout,
    pipelines: Vec<wgpu::ComputePipeline>,
}
impl Program {
    pub(crate) fn new(device: &wgpu::Device) -> Result<Self, String> {
        Self::with_shader(device, include_str!("gpu_mikk_prepare.wgsl"))
    }
    fn with_shader(device: &wgpu::Device, source: &str) -> Result<Self, String> {
        if device.limits().max_storage_buffers_per_shader_stage < 5 {
            return Err("GPU preparation needs five compute storage bindings".into());
        }
        let entries: Vec<_> = (0..6)
            .map(|binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: if binding == 5 {
                        wgpu::BufferBindingType::Uniform
                    } else {
                        wgpu::BufferBindingType::Storage {
                            read_only: binding < 2,
                        }
                    },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            })
            .collect();
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("current Mikk preparation"),
            entries: &entries,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("current Mikk weld and corrected edges"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let pipelines = [
            "weld",
            "canonical",
            "degen_flags",
            "degen_prologue",
            "flags",
            "edges",
            "pair",
        ]
        .into_iter()
        .map(|entry| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        })
        .collect();
        Ok(Self { layout, pipelines })
    }
}
impl GpuPreparation {
    pub(crate) fn new(
        device: &wgpu::Device,
        vertices: &wgpu::Buffer,
        vertex_count: usize,
        indices: &[u32],
    ) -> Result<Self, String> {
        Self::with_program(
            device,
            vertices,
            vertex_count,
            indices,
            &Program::new(device)?,
        )
    }
    pub(crate) fn with_program(
        device: &wgpu::Device,
        vertices: &wgpu::Buffer,
        vertex_count: usize,
        indices: &[u32],
        program: &Program,
    ) -> Result<Self, String> {
        if indices.is_empty()
            || indices.len() % 3 != 0
            || indices.iter().any(|i| *i as usize >= vertex_count)
        {
            return Err("GPU preparation requires nonempty indexed triangles".into());
        }
        let corners = u32::try_from(indices.len()).map_err(|_| "too many GPU triangle corners")?;
        let triangles = corners / 3;
        let slots = corners
            .checked_mul(2)
            .and_then(u32::checked_next_power_of_two)
            .ok_or("GPU hash capacity overflow")?;
        let mut next = 4u32;
        let mut reserve = |count: u32| -> Result<u32, String> {
            let offset = next;
            next = next.checked_add(count).ok_or("GPU arena overflow")?;
            Ok(offset)
        };
        let offsets0 = [
            reserve(corners)?,
            reserve(corners)?,
            reserve(triangles)?,
            reserve(triangles)?,
        ];
        let offsets1 = [
            reserve(corners)?,
            reserve(corners)?,
            reserve(corners)?,
            reserve(corners)?,
        ];
        let vertex_count = u32::try_from(vertex_count).map_err(|_| "GPU vertex count overflow")?;
        let params = Params {
            counts: [vertex_count, triangles, slots - 1, slots - 1],
            offsets0,
            offsets1,
        };
        let limits = device.limits();
        if limits.max_storage_buffers_per_shader_stage < 5
            || corners.div_ceil(64) > limits.max_compute_workgroups_per_dimension
        {
            return Err("GPU preparation exceeds compute limits".into());
        }
        let sizes = [
            u64::from(next) * 4,
            u64::from(slots) * 4,
            u64::from(slots) * 8,
            u64::from(triangles) * 16,
            u64::from(vertex_count) * 64,
        ];
        if !vertices.usage().contains(wgpu::BufferUsages::STORAGE)
            || vertices.size() > u64::from(limits.max_storage_buffer_binding_size)
            || vertices.size() < sizes[4]
            || sizes.iter().any(|size| {
                *size > limits.max_buffer_size
                    || *size > u64::from(limits.max_storage_buffer_binding_size)
            })
        {
            return Err("GPU preparation exceeds storage limits".into());
        }
        let mutable = |label, size| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            })
        };
        let arena = mutable("current Mikk preparation arena", sizes[0]);
        let weld = mutable("collision-checked current attribute hash", sizes[1]);
        let edges = mutable("corrected current edge incidence hash", sizes[2]);
        let triangles: Vec<[u32; 4]> = indices
            .chunks_exact(3)
            .map(|i| [i[0], i[1], i[2], 0])
            .collect();
        let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("immutable triangle indices"),
            contents: bytemuck::cast_slice(&triangles),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("GPU preparation parameters"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let layout = &program.layout;
        let pipelines = program.pipelines.clone();
        let entries: Vec<_> = [vertices, &input, &weld, &edges, &arena, &uniform]
            .into_iter()
            .enumerate()
            .map(|(binding, buffer)| wgpu::BindGroupEntry {
                binding: binding as u32,
                resource: buffer.as_entire_binding(),
            })
            .collect();
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("current Mikk attributes and static topology"),
            layout: &layout,
            entries: &entries,
        });
        Ok(Self {
            pipelines,
            group,
            arena,
            weld,
            edges,
            params,
            words: next as usize,
        })
    }
    pub(crate) fn recipe_input(&self) -> (&wgpu::Buffer, [u32; 3]) {
        (
            &self.arena,
            [
                self.params.offsets0[1],
                self.params.offsets0[2],
                self.params.offsets0[3],
            ],
        )
    }
    pub(crate) fn encode(&self, encoder: &mut wgpu::CommandEncoder) {
        self.encode_timed(encoder, None);
    }
    pub(crate) fn encode_timed(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        queries: Option<&wgpu::QuerySet>,
    ) {
        Self::encode_batch(encoder, &[self], queries);
    }
    pub(crate) fn encode_batch(
        encoder: &mut wgpu::CommandEncoder,
        parts: &[&Self],
        queries: Option<&wgpu::QuerySet>,
    ) {
        let Some(first) = parts.first() else {
            return;
        };
        if let Some(queries) = queries {
            encoder.write_timestamp(queries, 0);
        }
        for part in parts {
            for buffer in [&part.arena, &part.weld, &part.edges] {
                encoder.clear_buffer(buffer, 0, None);
            }
        }
        if let Some(queries) = queries {
            encoder.write_timestamp(queries, 1);
        }
        for (i, pipeline) in first.pipelines.iter().enumerate() {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("batched current Mikk preparation stage"),
                timestamp_writes: queries.map(|query_set| wgpu::ComputePassTimestampWrites {
                    query_set,
                    beginning_of_pass_write_index: Some(2 + 2 * i as u32),
                    end_of_pass_write_index: Some(3 + 2 * i as u32),
                }),
            });
            pass.set_pipeline(pipeline);
            for part in parts {
                pass.set_bind_group(0, &part.group, &[]);
                let count = if i == 3 {
                    1
                } else if i == 2 || i == 4 {
                    part.params.counts[1].div_ceil(64)
                } else {
                    (part.params.counts[1] * 3).div_ceil(64)
                };
                pass.dispatch_workgroups(count, 1, 1);
            }
        }
    }
    pub(crate) fn readback(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<Prepared, String> {
        let values: Vec<u32> = super::read_records(device, queue, &self.arena, self.words);
        if values[1] != 0 {
            return Err(format!("{} nonfinite current attribute corners", values[1]));
        }
        let corners = self.params.counts[1] as usize * 3;
        let slice =
            |offset: u32, count: usize| values[offset as usize..offset as usize + count].to_vec();
        let original_faces = slice(self.params.offsets0[2], corners / 3);
        let original_flags = slice(self.params.offsets0[3], corners / 3);
        let flags = original_faces
            .iter()
            .map(|i| original_flags[*i as usize])
            .collect();
        Ok(Prepared {
            active_count: values[0] as usize,
            canonical: slice(self.params.offsets0[1], corners),
            original_faces,
            flags,
            neighbors: slice(self.params.offsets1[3], corners)
                .chunks_exact(3)
                .map(|v| [v[0] as i32 - 1, v[1] as i32 - 1, v[2] as i32 - 1])
                .collect(),
        })
    }
}
#[cfg(test)]
fn run(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    vertices: &[PackedVertex],
    indices: &[u32],
) -> Prepared {
    let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("SAME current GPU attribute oracle fixture"),
        contents: bytemuck::cast_slice(vertices),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let preparation = GpuPreparation::new(device, &input, vertices.len(), indices).unwrap();
    let mut encoder = device.create_command_encoder(&Default::default());
    preparation.encode(&mut encoder);
    queue.submit([encoder.finish()]);
    preparation.readback(device, queue).unwrap()
}

#[cfg(test)]
pub(crate) fn compare_cpu(data: &crate::MeshData, output: &Prepared) {
    let cpu = crate::mikktspace::capture_preparation(data);
    assert_eq!(
        output.active_count, cpu.active_count,
        "current degenerate count"
    );
    let mut representatives = vec![0i32; data.indices.len()];
    let mut minima = std::collections::HashMap::<i32, u32>::new();
    for (prepared, triangle) in cpu.triangles.iter().enumerate() {
        assert_eq!(
            output.original_faces[prepared], triangle.original_face as u32,
            "current DegenPrologue order"
        );
        assert_eq!(
            output.flags[prepared], triangle.flags as u32,
            "current derivative orientation/degenerate flags"
        );
        if prepared < cpu.active_count {
            assert_eq!(
                output.neighbors[prepared], triangle.neighbors,
                "corrected current edge pairing at prepared face {prepared}"
            );
        }
        for corner in 0..3 {
            let original = triangle.original_face as usize * 3 + corner;
            let representative = cpu.indices[prepared * 3 + corner];
            representatives[original] = representative;
            minima
                .entry(representative)
                .and_modify(|minimum| *minimum = (*minimum).min(original as u32))
                .or_insert(original as u32);
        }
    }
    let canonical: Vec<_> = representatives
        .iter()
        .map(|representative| minima[representative])
        .collect();
    assert_eq!(
        output.canonical, canonical,
        "canonical current position/normal/UV weld classes"
    );
}
