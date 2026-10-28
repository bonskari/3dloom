//! Test-only hybrid structural recipe and GPU CURRENT tangent math.
//! No CPU tangent, derivative, angle or current normal is stored in the recipe.
use super::{
    mikk_prepare::{GpuPreparation, Prepared},
    PackedVertex,
};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    counts: [u32; 4],
    prep: [u32; 4],
    recipe0: [u32; 4],
    recipe1: [u32; 4],
    scratch: [u32; 4],
    debug: [u32; 4],
}
struct Recipe {
    words: Vec<u32>,
    params: Params,
    scratch_words: usize,
}
impl Recipe {
    fn from_current_structure(
        data: &crate::MeshData,
        initial: &Prepared,
        groups: &crate::mikktspace::StructuralRecipe,
        prep: [u32; 3],
        base: u32,
    ) -> Result<Self, String> {
        let corners = u32::try_from(data.indices.len()).map_err(|_| "recipe corner count")?;
        let triangles = corners / 3;
        if corners == 0
            || corners % 3 != 0
            || data
                .indices
                .iter()
                .any(|i| *i as usize >= data.positions.len())
            || initial.original_faces.len() != triangles as usize
            || initial.canonical.len() != corners as usize
            || initial.flags.len() != triangles as usize
            || groups.triangles.len() != triangles as usize
            || groups.indices.len() != corners as usize
            || initial.active_count > triangles as usize
            || groups.active_count != initial.active_count
            || groups.assigned_groups.len() != triangles as usize
            || groups
                .triangles
                .iter()
                .zip(&initial.original_faces)
                .any(|(t, f)| t.original_face as u32 != *f)
        {
            return Err("current recipe topology/count mismatch".into());
        }
        let mut words = Vec::new();
        let mut append = |values: &[u32]| {
            let offset = words.len() as u32;
            words.extend_from_slice(values);
            offset
        };
        let canonical = append(&initial.canonical);
        let order = append(&initial.original_faces);
        let flags = append(&initial.flags);
        let post = append(
            &groups
                .triangles
                .iter()
                .map(|t| t.flags as u32)
                .collect::<Vec<_>>(),
        );
        let assigned = append(
            &groups
                .assigned_groups
                .iter()
                .flat_map(|g| g.map(|id| (id + 1) as u32))
                .collect::<Vec<_>>(),
        );
        let mut descriptors = Vec::new();
        let mut members = Vec::new();
        for group in &groups.groups {
            if group.representative < 0
                || group
                    .members
                    .iter()
                    .any(|f| *f < 0 || *f as usize >= initial.active_count)
            {
                return Err("invalid current group indices".into());
            }
            let encoded = group.representative as u32;
            let representative = (encoded >> 2) * 3 + (encoded & 3);
            if representative >= corners || encoded & 3 >= 3 {
                return Err("invalid current encoded group vertex".into());
            }
            let mut ordered: Vec<_> = group.members.iter().map(|v| *v as u32).collect();
            ordered.sort_unstable();
            descriptors.extend_from_slice(&[
                members.len() as u32,
                ordered.len() as u32,
                representative,
                u32::from(group.orientation),
            ]);
            members.extend(ordered);
        }
        let group_offset = append(&descriptors);
        let member_offset = append(&members);
        if groups
            .assigned_groups
            .iter()
            .flatten()
            .any(|g| *g < -1 || (*g >= 0 && *g as usize >= groups.groups.len()))
        {
            return Err("assigned current group out of bounds".into());
        }
        let vertices = u32::try_from(data.positions.len()).map_err(|_| "recipe vertex count")?;
        let owner = 4u32;
        let first = owner
            .checked_add(vertices)
            .ok_or("recipe scratch overflow")?;
        let orient = first
            .checked_add(corners)
            .ok_or("recipe scratch overflow")?;
        let inverse = orient
            .checked_add(corners)
            .ok_or("recipe scratch overflow")?;
        let debug_base = inverse
            .checked_add(corners)
            .ok_or("recipe debug overflow")?;
        let debug_words = groups
            .groups
            .iter()
            .map(|g| g.members.len())
            .max()
            .unwrap_or(0)
            .div_ceil(32) as u32;
        let scratch_words = debug_base
            .checked_add(
                debug_words
                    + groups
                        .groups
                        .iter()
                        .map(|g| g.members.len())
                        .max()
                        .unwrap_or(0) as u32
                        * 4,
            )
            .ok_or("recipe debug overflow")? as usize;
        Ok(Self {
            words,
            params: Params {
                counts: [
                    vertices,
                    triangles,
                    initial.active_count as u32,
                    groups.groups.len() as u32,
                ],
                prep: [prep[0], prep[1], prep[2], base],
                recipe0: [canonical, order, flags, post],
                recipe1: [assigned, group_offset, member_offset, 0],
                scratch: [owner, first, orient, inverse],
                debug: [0, debug_base, debug_words, 0],
            },
            scratch_words,
        })
    }
}

pub(crate) struct GpuRecipeMath {
    pipelines: Vec<wgpu::ComputePipeline>,
    group: wgpu::BindGroup,
    scratch: wgpu::Buffer,
    output: wgpu::Buffer,
    counts: [u32; 4],
    params: Params,
    uniform: wgpu::Buffer,
    derivatives: wgpu::Buffer,
}
impl GpuRecipeMath {
    pub(crate) fn new(
        device: &wgpu::Device,
        vertices: &wgpu::Buffer,
        base: u32,
        data: &crate::MeshData,
        preparation: &GpuPreparation,
        initial: &Prepared,
        groups: &crate::mikktspace::StructuralRecipe,
    ) -> Result<Self, String> {
        let (prep, offsets) = preparation.recipe_input();
        let recipe = Recipe::from_current_structure(data, initial, groups, offsets, base)?;
        let limits = device.limits();
        let c = u64::from(recipe.params.counts[1]) * 3;
        let sizes = [
            recipe.words.len() as u64 * 4,
            recipe.scratch_words as u64 * 4,
            c * 32,
            u64::from(recipe.params.counts[0]) * 64,
            u64::from(recipe.params.counts[1]) * 32,
        ];
        if limits.max_storage_buffers_per_shader_stage < 8
            || c.div_ceil(64) > u64::from(limits.max_compute_workgroups_per_dimension)
            || sizes.iter().any(|s| {
                *s > limits.max_buffer_size
                    || *s > u64::from(limits.max_storage_buffer_binding_size)
            })
            || u64::from(base)
                .checked_add(u64::from(recipe.params.counts[0]))
                .and_then(|n| n.checked_mul(64))
                .is_none_or(|bytes| bytes > vertices.size())
        {
            return Err("current recipe exceeds GPU input/storage limits".into());
        }
        let storage = |label, size| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            })
        };
        let scratch = storage("current recipe guards and output ownership", sizes[1]);
        let spaces = storage("CURRENT GPU angle-weighted tangent spaces", sizes[2]);
        let output = storage("CURRENT complete GPU geometry", sizes[3]);
        let derivatives = storage("CURRENT GPU UV derivatives", sizes[4]);
        let recipe_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("current compiled structure NO tangent values"),
            contents: bytemuck::cast_slice(&recipe.words),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let indices: Vec<[u32; 4]> = data
            .indices
            .chunks_exact(3)
            .map(|i| [i[0], i[1], i[2], 0])
            .collect();
        let triangles = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("static recipe triangles"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("current recipe offsets"),
            contents: bytemuck::bytes_of(&recipe.params),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let entries: Vec<_> = (0..9)
            .map(|binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: if binding == 8 {
                        wgpu::BufferBindingType::Uniform
                    } else {
                        wgpu::BufferBindingType::Storage {
                            read_only: matches!(binding, 0 | 1 | 3),
                        }
                    },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            })
            .collect();
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("current recipe dynamic GPU math"),
            entries: &entries,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("all-current GPU tangent derivative subgroup angle epilogue"),
            source: wgpu::ShaderSource::Wgsl(include_str!("gpu_mikk_recipe.wgsl").into()),
        });
        let pipelines = ["guard_init", "derive", "evaluate", "epilogue", "pack"]
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
        let entries: Vec<_> = [
            vertices,
            &triangles,
            prep,
            &recipe_buffer,
            &scratch,
            &spaces,
            &output,
            &derivatives,
            &uniform,
        ]
        .into_iter()
        .enumerate()
        .map(|(binding, b)| wgpu::BindGroupEntry {
            binding: binding as u32,
            resource: b.as_entire_binding(),
        })
        .collect();
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("real current attributes and compiled structure"),
            layout: &layout,
            entries: &entries,
        });
        Ok(Self {
            pipelines,
            group,
            scratch,
            output,
            counts: recipe.params.counts,
            params: recipe.params,
            uniform,
            derivatives,
        })
    }
    pub(crate) fn output(&self) -> &wgpu::Buffer {
        &self.output
    }
    pub(crate) fn guard_buffer(&self) -> &wgpu::Buffer {
        &self.scratch
    }
    pub(crate) fn vertex_count(&self) -> u32 {
        self.counts[0]
    }
    pub(crate) fn encode(&self, encoder: &mut wgpu::CommandEncoder) {
        encoder.clear_buffer(&self.scratch, 0, None);
        for (i, pipeline) in self.pipelines.iter().enumerate() {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("current GPU tangent math"),
                timestamp_writes: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.group, &[]);
            let count = match i {
                1 => self.counts[1],
                4 => self.counts[0],
                _ => self.counts[1] * 3,
            };
            pass.dispatch_workgroups(count.div_ceil(64), 1, 1);
        }
    }
    /// Keep stage barriers while sharing each pass across independent parts.
    pub(crate) fn encode_batch(encoder: &mut wgpu::CommandEncoder, parts: &[&Self]) {
        if parts.is_empty() { return; }
        for part in parts { encoder.clear_buffer(&part.scratch, 0, None); }
        for stage in 0..5 {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("current GPU tangent math batch"),
                timestamp_writes: None,
            });
            for part in parts {
                pass.set_pipeline(&part.pipelines[stage]);
                pass.set_bind_group(0, &part.group, &[]);
                let count = match stage {
                    1 => part.counts[1],
                    4 => part.counts[0],
                    _ => part.counts[1] * 3,
                };
                pass.dispatch_workgroups(count.div_ceil(64), 1, 1);
            }
        }
    }
    #[cfg(test)]
    /// Test-only isolation: current CPU InitTriInfo derivatives are uploaded
    /// solely to distinguish derivative arithmetic from GPU evaluator behavior.
    pub(crate) fn diagnose_current_math(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &crate::MeshData,
        initial: &Prepared,
        groups: &crate::mikktspace::GroupPreparationFixture,
        expected: &[[f32; 4]],
        vertex: usize,
    ) -> serde_json::Value {
        let owner = data
            .indices
            .iter()
            .rposition(|v| *v as usize == vertex)
            .unwrap();
        let face = initial
            .original_faces
            .iter()
            .position(|f| *f as usize == owner / 3)
            .unwrap();
        let corner = owner % 3;
        let group_index = groups.assigned_groups[face][corner];
        assert!(group_index >= 0);
        let group = &groups.groups[group_index as usize];
        let mut members = group.members.clone();
        members.sort_unstable();
        let representative =
            (group.representative as usize >> 2) * 3 + (group.representative as usize & 3);
        let normal = glam::Vec3::from_array(data.normals[data.indices[representative] as usize]);
        let projected = |v: [f32; 4]| {
            let v = glam::Vec3::from_array(v[..3].try_into().unwrap());
            let p = v - normal.dot(v) * normal;
            if p.to_array().iter().any(|v| v.abs() > f32::MIN_POSITIVE) {
                (1.0 / p.length()) * p
            } else {
                p
            }
        };
        let own_s = projected(groups.derivatives[face][0]);
        let own_t = projected(groups.derivatives[face][1]);
        let own_flags = groups.preparation.triangles[face].flags;
        let mut expected_cosines = Vec::new();
        let mut expected_angles = Vec::new();
        let mut expected_bits = vec![0u32; self.params.debug[2] as usize];
        for (j, &member) in members.iter().enumerate() {
            let t = member as usize;
            let os = projected(groups.derivatives[t][0]);
            let ot = projected(groups.derivatives[t][1]);
            let flags = groups.preparation.triangles[t].flags;
            expected_cosines.push([own_s.dot(os), own_t.dot(ot)]);
            let orig = initial.original_faces[t] as usize;
            let ids = &data.indices[orig * 3..orig * 3 + 3];
            let rep = data.positions[data.indices[representative] as usize];
            let corner = ids
                .iter()
                .position(|v| data.positions[*v as usize] == rep)
                .unwrap();
            let center = glam::Vec3::from_array(data.positions[ids[corner] as usize]);
            let before =
                glam::Vec3::from_array(data.positions[ids[(corner + 2) % 3] as usize]) - center;
            let after =
                glam::Vec3::from_array(data.positions[ids[(corner + 1) % 3] as usize]) - center;
            let a = projected([before.x, before.y, before.z, 0.0]);
            let b = projected([after.x, after.y, after.z, 0.0]);
            let cosine = a.dot(b).clamp(-1.0, 1.0);
            expected_angles.push([cosine, (cosine as f64).acos() as f32]);

            if ((own_flags | flags) & 4) != 0
                || groups.preparation.triangles[face].original_face
                    == groups.preparation.triangles[t].original_face
                || (own_s.dot(os) > -1.0 && own_t.dot(ot) > -1.0)
            {
                expected_bits[j / 32] |= 1u32 << (j % 32);
            }
        }
        let read_bits = || {
            let all: Vec<u32> = super::read_records(
                device,
                queue,
                &self.scratch,
                (self.params.debug[1] + self.params.debug[2]) as usize,
            );
            all[self.params.debug[1] as usize..].to_vec()
        };
        let read_cosines = || {
            let start = self.params.debug[1] + self.params.debug[2];
            let all: Vec<u32> = super::read_records(
                device,
                queue,
                &self.scratch,
                start as usize + members.len() * 4,
            );
            all[start as usize..]
                .chunks_exact(4)
                .map(|v| {
                    [
                        f32::from_bits(v[0]),
                        f32::from_bits(v[1]),
                        f32::from_bits(v[2]),
                        f32::from_bits(v[3]),
                    ]
                })
                .collect::<Vec<_>>()
        };
        let execute = |params: &Params| {
            queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(params));
            let mut encoder = device.create_command_encoder(&Default::default());
            self.encode(&mut encoder);
            queue.submit([encoder.finish()]);
        };
        let mut debug = self.params;
        debug.debug[0] = (face * 3 + corner + 1) as u32;
        execute(&debug);
        let gpu_default = self.readback(device, queue).unwrap();
        let native_bits = read_bits();
        let native_cosines = read_cosines();
        let gpu_derivatives: Vec<[[f32; 4]; 2]> =
            super::read_records(device, queue, &self.derivatives, self.counts[1] as usize);
        queue.write_buffer(
            &self.derivatives,
            0,
            bytemuck::cast_slice(&groups.derivatives),
        );
        debug.debug[3] = 1;
        execute(&debug);
        let cpu_derivative_candidate = self.readback(device, queue).unwrap();
        let cpu_derivative_bits = read_bits();
        let cpu_derivative_cosines = read_cosines();
        let vector_error = |a: [f32; 4], b: [f32; 4]| {
            glam::Vec3::from_array(a[..3].try_into().unwrap())
                .distance(glam::Vec3::from_array(b[..3].try_into().unwrap()))
        };
        let max_error = cpu_derivative_candidate
            .iter()
            .zip(expected)
            .map(|(a, b)| vector_error(a.tangent, *b))
            .fold(0.0f32, f32::max);
        let rows:Vec<_>=members.iter().map(|m|serde_json::json!({"prepared":m,"cpu":groups.derivatives[*m as usize],"gpu":gpu_derivatives[*m as usize]})).collect();
        let result = serde_json::json!({"vertex":vertex,"target_prepared":face,"members":members,"expected_membership_bits":expected_bits,"gpu_native_membership_bits":native_bits,"cpu_derivative_membership_bits":cpu_derivative_bits,"native_membership_matches":native_bits==expected_bits,"cpu_derivative_membership_matches":cpu_derivative_bits==expected_bits,"gpu_native_tangent":gpu_default[vertex].tangent,"cpu_derivative_tangent":cpu_derivative_candidate[vertex].tangent,"cpu_expected":expected[vertex],"native_target_error":vector_error(gpu_default[vertex].tangent,expected[vertex]),"cpu_derivative_target_error":vector_error(cpu_derivative_candidate[vertex].tangent,expected[vertex]),"cpu_derivative_global_max_error":max_error,"derivatives":rows,"cpu_expected_cosines":expected_cosines,"cpu_expected_angles":expected_angles,"gpu_native_cosines_angles":native_cosines,"cpu_derivative_cosines_angles":cpu_derivative_cosines});
        execute(&self.params);
        result
    }
    pub(crate) fn readback(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<Vec<PackedVertex>, String> {
        let guard: Vec<u32> = super::read_records(device, queue, &self.scratch, 1);
        if guard[0] != 0 {
            return Err(format!(
                "current recipe guard rejected candidate: {}",
                guard[0]
            ));
        }
        Ok(super::read_records(
            device,
            queue,
            &self.output,
            self.counts[0] as usize,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MeshData;
    fn device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(&Default::default());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
            .expect("real GPU required");
        pollster::block_on(adapter.request_device(&crate::device_descriptor(&adapter), None))
            .unwrap()
    }
    fn fixture() -> MeshData {
        MeshData {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, -1.0, 0.0],
            ],
            normals: vec![[0.0, 0.0, 1.0]; 4],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [2.0, 0.0]],
            indices: vec![0, 1, 2, 1, 0, 3, 0, 0, 1],
        }
    }
    fn packed(data: &MeshData) -> Vec<PackedVertex> {
        (0..data.positions.len())
            .map(|i| PackedVertex {
                position: [
                    data.positions[i][0],
                    data.positions[i][1],
                    data.positions[i][2],
                    1.0,
                ],
                normal: [
                    data.normals[i][0],
                    data.normals[i][1],
                    data.normals[i][2],
                    0.0,
                ],
                uv: [data.uvs[i][0], data.uvs[i][1], 0.0, 0.0],
                tangent: [f32::NAN; 4],
            })
            .collect()
    }
    fn submit(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        preparation: &GpuPreparation,
        math: Option<&GpuRecipeMath>,
    ) {
        let mut encoder = device.create_command_encoder(&Default::default());
        preparation.encode(&mut encoder);
        if let Some(math) = math {
            math.encode(&mut encoder);
        }
        queue.submit([encoder.finish()]);
    }
    fn compare(data: &MeshData, actual: &[PackedVertex]) -> f32 {
        let expected = crate::mikktspace::generate_serial(data).unwrap();
        let mut maximum = 0.0f32;
        for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
            assert!(
                a.tangent.iter().all(|v| v.is_finite()),
                "poisoned CPU input tangents must never become output"
            );
            assert_eq!(
                a.tangent[3], e[3],
                "current orientation/last setter at vertex {i}"
            );
            let error = glam::Vec3::from_array(a.tangent[..3].try_into().unwrap())
                .distance(glam::Vec3::from_array(e[..3].try_into().unwrap()));
            maximum = maximum.max(error);
        }
        maximum
    }
    #[test]
    fn gpu_current_recipe_math_preserves_any_orientation_last_owner_and_degenerate_epilogue() {
        let (device, queue) = device();
        let mut data = fixture();
        let mut values = packed(&data);
        let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("current attrs; CPU tangent fields intentionally poisoned"),
            contents: bytemuck::cast_slice(&values),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let preparation =
            GpuPreparation::new(&device, &input, values.len(), &data.indices).unwrap();
        submit(&device, &queue, &preparation, None);
        let initial = preparation.readback(&device, &queue).unwrap();
        let structure = crate::mikktspace::compile_structure(&data);
        assert!(
            structure
                .triangles
                .iter()
                .any(|t| t.flags & 4 != 0 && t.flags & 8 != 0),
            "fixture must adopt Any orientation globally"
        );
        let math = GpuRecipeMath::new(
            &device,
            &input,
            0,
            &data,
            &preparation,
            &initial,
            &structure,
        )
        .unwrap();
        for pose in 0..3 {
            if pose > 0 {
                data.positions[1] = [1.0, 0.2 * pose as f32, 0.1];
                data.positions[2] = [0.2, 1.0, 0.1 * pose as f32];
                values = packed(&data);
                queue.write_buffer(&input, 0, bytemuck::cast_slice(&values));
            }
            submit(&device, &queue, &preparation, Some(&math));
            let actual = math.readback(&device, &queue).unwrap();
            let error = compare(&data, &actual);
            assert!(
                error < 2e-5,
                "CURRENT recipe math max vector error {error:e}"
            );
        }
        let before: Vec<PackedVertex> =
            super::super::read_records(&device, &queue, &math.output, values.len());
        data.normals[0] = [0.0, 1.0, 0.0];
        // A seam may keep the same class partition when its vertex was already
        // unique; force a UV-degenerate flag change as well, a real guard miss.
        data.uvs[3] = [0.5, 0.5];
        values = packed(&data);
        queue.write_buffer(&input, 0, bytemuck::cast_slice(&values));
        submit(&device, &queue, &preparation, Some(&math));
        assert!(math
            .readback(&device, &queue)
            .unwrap_err()
            .contains("guard rejected"));
        let after: Vec<PackedVertex> =
            super::super::read_records(&device, &queue, &math.output, values.len());
        assert_eq!(
            bytemuck::cast_slice::<PackedVertex, u8>(&before),
            bytemuck::cast_slice::<PackedVertex, u8>(&after),
            "guard miss cannot publish candidate geometry"
        );
    }
    #[test]
    fn current_recipe_handles_interspersed_degenerate_face_order() {
        let (device, queue) = device();
        let mut data = fixture();
        data.indices = vec![0, 1, 2, 0, 0, 1, 1, 0, 3];
        let values = packed(&data);
        let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("real degen-middle synthetic attributes"),
            contents: bytemuck::cast_slice(&values),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let preparation =
            GpuPreparation::new(&device, &input, values.len(), &data.indices).unwrap();
        submit(&device, &queue, &preparation, None);
        let initial = preparation.readback(&device, &queue).unwrap();
        assert_eq!(initial.original_faces, vec![0, 2, 1]);
        let structure = crate::mikktspace::compile_structure(&data);
        let math = GpuRecipeMath::new(
            &device,
            &input,
            0,
            &data,
            &preparation,
            &initial,
            &structure,
        )
        .unwrap();
        submit(&device, &queue, &preparation, Some(&math));
        let actual = math.readback(&device, &queue).unwrap();
        assert!(compare(&data, &actual) < 2e-5);
    }
    #[test]
    fn stable_angular_predicates_preserve_opposite_near_opposite_zero_and_any_inputs() {
        let (device, queue) = device();
        let diagonal = glam::Vec3::new(1.0, 2.0, 3.0).normalize();
        let cases = [
            (glam::Vec3::X, -glam::Vec3::X, false),
            (diagonal, -diagonal, false),
            (
                glam::Vec3::X,
                glam::Vec3::new(-1.0, 1e-4, 0.0).normalize(),
                true,
            ),
            (glam::Vec3::ZERO, glam::Vec3::X, true),
            (glam::Vec3::ZERO, glam::Vec3::ZERO, true),
            (diagonal, diagonal, true),
            (glam::Vec3::X, glam::Vec3::Y, true),
        ];
        let input: Vec<[[f32; 4]; 2]> = cases
            .iter()
            .map(|(a, b, _)| [a.extend(0.0).to_array(), b.extend(0.0).to_array()])
            .collect();
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("real angular boundary cases"),
            contents: bytemuck::cast_slice(&input),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let output = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: cases.len() as u64 * 16,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let source = include_str!("gpu_mikk_recipe.wgsl");
        let begin = source.find("fn nonzero").unwrap();
        let end = source.find("fn descriptor").unwrap();
        let shader_source=format!("{}\nstruct Case{{a:vec4<f32>,b:vec4<f32>}}\n@group(0) @binding(0) var<storage,read> cases:array<Case>;\n@group(0) @binding(1) var<storage,read_write> results:array<vec4<f32>>;\n@compute @workgroup_size(64) fn check(@builtin(global_invocation_id) id:vec3<u32>){{if id.x>=arrayLength(&cases){{return;}}let a=cases[id.x].a.xyz;let b=cases[id.x].b.xyz;results[id.x]=vec4<f32>(select(0.0,1.0,angular_member(a,b)),corner_angle(a,b),select(0.0,1.0,dot(a,b)>-1.0),0.0);}}",&source[begin..end]);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("actual recipe angular functions boundary regression"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: None,
            module: &module,
            entry_point: Some("check"),
            compilation_options: Default::default(),
            cache: None,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: output.as_entire_binding(),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        queue.submit([encoder.finish()]);
        let actual: Vec<[f32; 4]> =
            super::super::read_records(&device, &queue, &output, cases.len());
        for (i, ((a, b, member), result)) in cases.iter().zip(&actual).enumerate() {
            assert_eq!(result[0], f32::from(u8::from(*member)), "case{i}");
            let ideal = if a.length_squared() == 0.0 || b.length_squared() == 0.0 {
                std::f64::consts::FRAC_PI_2
            } else {
                a.as_dvec3()
                    .cross(b.as_dvec3())
                    .length()
                    .atan2(a.as_dvec3().dot(b.as_dvec3()))
            };
            assert!((f64::from(result[1]) - ideal).abs() < 1e-6, "angle case{i}");
        }
        assert_eq!(
            actual[2][2], 0.0,
            "original unit dot predicate must reproduce the nearly opposite false exclusion"
        );
        assert_eq!(actual[2][0], 1.0);
    }
    #[test]
    fn current_recipe_all_any_and_all_position_degenerate_defaults_match_f64() {
        let (device, queue) = device();
        for degenerate in [false, true] {
            let mut data = fixture();
            if degenerate {
                data.positions.fill([0.0; 3]);
            } else {
                data.uvs.fill([0.0; 2]);
            }
            let values = packed(&data);
            let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("actual all-any/all-position-degenerate values"),
                contents: bytemuck::cast_slice(&values),
                usage: wgpu::BufferUsages::STORAGE,
            });
            let preparation =
                GpuPreparation::new(&device, &input, values.len(), &data.indices).unwrap();
            submit(&device, &queue, &preparation, None);
            let initial = preparation.readback(&device, &queue).unwrap();
            let structure = crate::mikktspace::compile_structure(&data);
            assert!(structure.groups.is_empty());
            let math = GpuRecipeMath::new(
                &device,
                &input,
                0,
                &data,
                &preparation,
                &initial,
                &structure,
            )
            .unwrap();
            submit(&device, &queue, &preparation, Some(&math));
            let actual = math.readback(&device, &queue).unwrap();
            let truth = super::super::mikk_f64::tangents_f64(&data, &structure);
            assert_eq!(actual.len(), truth.len());
            for (a, t) in actual.iter().zip(truth) {
                assert_eq!(a.tangent.map(f64::from), t);
                assert_eq!(a.tangent, [1.0, 0.0, 0.0, 1.0]);
            }
        }
    }
}
