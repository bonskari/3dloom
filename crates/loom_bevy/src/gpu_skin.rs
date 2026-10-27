//! Unwired all-influence GPU position-skinning prototype.
//!
//! This does not replace posed smoothing normals/MikkTSpace, handle Skin gizmos,
//! or connect to the application's render/picking paths. No production plugin
//! installs it. Matrices must come from `loom_scene::skin::bone_matrices`.

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{Mat4, Vec3};

    // Truncating to four influences, dropping joint 874, or normalizing the
    // residual weight away must change these independently computed results.
    #[test]
    fn gpu_positions_preserve_twelve_influences_relative_weights_and_missing_bones() {
        let instance = wgpu::Instance::new(&Default::default());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
            .expect("real GPU adapter required");
        let (device, queue) =
            pollster::block_on(adapter.request_device(&crate::device_descriptor(&adapter), None))
                .unwrap();
        eprintln!("GPU skin validation adapter: {:?}", adapter.get_info());
        let positions = vec![Vec3::new(1.0, 2.0, 3.0); 7];
        let mut bones = vec![Some(Mat4::IDENTITY); 875];
        for (i, bone) in bones.iter_mut().enumerate() {
            *bone = Some(Mat4::from_scale_rotation_translation(
                Vec3::new(1.0 + i as f32 * 0.0002, 0.9, 1.1),
                glam::Quat::from_rotation_y(i as f32 * 0.005),
                Vec3::new(i as f32 * 0.01, i as f32 * -0.02, 0.125),
            ));
        }
        bones[3] = None;
        let weights = vec![
            (0..12)
                .map(|i| (if i == 11 { 874 } else { i }, 1.0 / 12.0))
                .collect::<Vec<_>>(),
            vec![(874, 0.25)],
            vec![(3, 0.25), (874, 0.25)],
            vec![(3, 0.75)],
            Vec::new(),
            vec![(874, 1.5), (1, 0.5)],
            vec![(874, 1e-7)],
        ];
        let mesh_bind = Mat4::from_scale_rotation_translation(
            Vec3::new(1.2, 0.8, 1.1),
            glam::Quat::from_rotation_z(0.3),
            Vec3::new(2.0, -1.0, 0.5),
        );
        let object_world = Mat4::from_translation(Vec3::new(-3.0, 4.0, 1.0));
        for back in [mesh_bind.inverse(), object_world.inverse()] {
            let input = SkinInput {
                positions: &positions,
                weights: &weights,
                bones: &bones,
                mesh_bind,
                object_world,
                back,
            };
            let skin = GpuSkinPositions::new(&device, &input).unwrap();
            let output = skin.readback(&device, &queue).unwrap();
            for (i, actual) in output.iter().enumerate() {
                let p = positions[i];
                let sum: f32 = weights[i].iter().map(|(_, w)| *w).sum();
                let mut acc = Vec3::ZERO;
                let mut total = 0.0;
                for &(bone, w) in &weights[i] {
                    if let Some(m) = bones[bone as usize] {
                        acc += m.transform_point3(mesh_bind.transform_point3(p)) * w;
                        total += w;
                    }
                }
                let expected = if sum > 1e-6 && total > 1e-6 {
                    back.transform_point3(
                        acc / total * sum.min(1.0)
                            + object_world.transform_point3(p) * (1.0 - sum.min(1.0)),
                    )
                } else {
                    p
                };
                assert!(
                    actual.distance(expected) < 2e-5,
                    "vertex {i}: GPU {actual:?}, CPU {expected:?}"
                );
            }
            assert_eq!(skin.output().size(), positions.len() as u64 * 16);
            let delta = Vec3::new(0.5, -0.25, 0.75);
            let moved: Vec<_> = bones
                .iter()
                .map(|m| m.map(|m| Mat4::from_translation(delta) * m))
                .collect();
            skin.update_pose(&queue, &moved, mesh_bind, object_world, back)
                .unwrap();
            let updated = skin.readback(&device, &queue).unwrap();
            for (i, (before, after)) in output.iter().zip(updated).enumerate() {
                let sum: f32 = weights[i].iter().map(|(_, w)| *w).sum();
                let total: f32 = weights[i]
                    .iter()
                    .filter(|(b, _)| bones[*b as usize].is_some())
                    .map(|(_, w)| *w)
                    .sum();
                let valid = sum > 1e-6 && total > 1e-6;
                let change = if valid {
                    back.transform_vector3(delta) * sum.min(1.0)
                } else {
                    Vec3::ZERO
                };
                assert!(
                    after.distance(*before + change) < 2e-5,
                    "updated palette vertex {i}"
                );
            }
            assert!(skin
                .update_pose(&queue, &moved[..874], mesh_bind, object_world, back)
                .is_err());
        }
    }

    #[test]
    fn validation_rejects_invalid_data_before_gpu_upload() {
        let positions = [Vec3::ZERO];
        let bones = [Some(Mat4::IDENTITY)];
        for weights in [
            vec![vec![(1, 1.0)]],
            vec![vec![(0, f32::NAN)]],
            vec![vec![(0, -0.1)]],
        ] {
            let input = SkinInput {
                positions: &positions,
                weights: &weights,
                bones: &bones,
                mesh_bind: Mat4::IDENTITY,
                object_world: Mat4::IDENTITY,
                back: Mat4::IDENTITY,
            };
            assert!(input.validate().is_err());
        }
    }
}

use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

/// Positions immediately below Skin and their complete effective weights.
/// Gizmos and modifiers after Skin are deliberately outside this prototype.
pub struct SkinInput<'a> {
    pub positions: &'a [Vec3],
    pub weights: &'a [Vec<(u32, f32)>],
    /// `None` matches a missing scene bone, rather than an identity bone.
    pub bones: &'a [Option<Mat4>],
    pub mesh_bind: Mat4,
    pub object_world: Mat4,
    /// `object_inverse` with Back Transform; otherwise `mesh_bind.inverse()`.
    pub back: Mat4,
}

impl SkinInput<'_> {
    pub fn validate(&self) -> Result<(), String> {
        if self.positions.is_empty() || self.positions.len() != self.weights.len() {
            return Err("Skin needs nonempty positions and one weight list per position".into());
        }
        if self.positions.len() > u32::MAX as usize || self.bones.len() > u32::MAX as usize {
            return Err("Skin array exceeds u32 indexing".into());
        }
        validate_pose(self.bones, self.mesh_bind, self.object_world, self.back)?;
        let mut count = 0usize;
        for (p, weights) in self.positions.iter().zip(self.weights) {
            if !p.is_finite() {
                return Err("Skin position is not finite".into());
            }
            let mut sum = 0.0f32;
            for &(bone, weight) in weights {
                if bone as usize >= self.bones.len() || !weight.is_finite() || weight < 0.0 {
                    return Err("Skin influence has invalid bone or weight".into());
                }
                sum += weight;
            }
            if !sum.is_finite() {
                return Err("Skin weight sum is not finite".into());
            }
            count = count
                .checked_add(weights.len())
                .ok_or("Skin influence count overflow")?;
        }
        if count > u32::MAX as usize {
            return Err("Skin influences exceed u32 indexing".into());
        }
        Ok(())
    }
}

fn validate_pose(
    bones: &[Option<Mat4>],
    mesh_bind: Mat4,
    object_world: Mat4,
    back: Mat4,
) -> Result<(), String> {
    for matrix in [Some(mesh_bind), Some(object_world), Some(back)]
        .into_iter()
        .chain(bones.iter().copied())
    {
        if let Some(m) = matrix {
            if !m.is_finite() || m.row(3) != glam::Vec4::W {
                return Err("Skin matrices must be finite and affine".into());
            }
        }
    }
    Ok(())
}

fn push_f32(bytes: &mut Vec<u8>, values: impl IntoIterator<Item = f32>) {
    for v in values {
        bytes.extend(v.to_le_bytes());
    }
}

fn bone_bytes(bones: &[Option<Mat4>]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(bones.len().max(1) * 80);
    for &bone in bones {
        push_f32(&mut bytes, bone.unwrap_or(Mat4::IDENTITY).to_cols_array());
        bytes.extend(u32::from(bone.is_some()).to_le_bytes());
        bytes.extend([0u8; 12]);
    }
    // WebGPU bindings cannot have a zero-sized range. This unused record is
    // never indexed because every validated vertex has an empty weight list.
    if bytes.is_empty() {
        bytes.resize(80, 0);
    }
    bytes
}

fn pose_bytes(count: u32, mesh_bind: Mat4, object_world: Mat4, back: Mat4) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(208);
    for m in [mesh_bind, object_world, back] {
        push_f32(&mut bytes, m.to_cols_array());
    }
    bytes.extend(count.to_le_bytes());
    bytes.extend([0u8; 12]);
    bytes
}

/// Persistent device buffers. `encode` adds a real GPU skin pass to the host's
/// command encoder; `output` can be consumed without CPU readback. It contains
/// one vec4 position per source vertex, not expanded material face corners.
pub struct GpuSkinPositions {
    pipeline: wgpu::ComputePipeline,
    bind_group: wgpu::BindGroup,
    bones: wgpu::Buffer,
    pose: wgpu::Buffer,
    output: wgpu::Buffer,
    count: u32,
    bone_count: usize,
}

impl GpuSkinPositions {
    pub fn new(device: &wgpu::Device, input: &SkinInput<'_>) -> Result<Self, String> {
        input.validate()?;
        let mut positions = Vec::with_capacity(input.positions.len() * 16);
        for p in input.positions {
            push_f32(&mut positions, [p.x, p.y, p.z, 1.0]);
        }
        let mut ranges = Vec::with_capacity(input.weights.len() * 8);
        let mut influences = Vec::new();
        let mut offset = 0u32;
        for weights in input.weights {
            ranges.extend(offset.to_le_bytes());
            ranges.extend((weights.len() as u32).to_le_bytes());
            for &(bone, weight) in weights {
                influences.extend(bone.to_le_bytes());
                influences.extend(weight.to_le_bytes());
            }
            offset += weights.len() as u32;
        }
        if influences.is_empty() {
            influences.resize(8, 0);
        }
        let bones_data = bone_bytes(input.bones);
        let limits = device.limits();
        for size in [
            positions.len(),
            ranges.len(),
            influences.len(),
            bones_data.len(),
        ] {
            if size as u64 > limits.max_buffer_size
                || size as u64 > limits.max_storage_buffer_binding_size as u64
            {
                return Err("Skin buffer exceeds device storage limit".into());
            }
        }
        let count = input.positions.len() as u32;
        if count.div_ceil(64) > limits.max_compute_workgroups_per_dimension
            || limits.max_storage_buffers_per_shader_stage < 5
        {
            return Err("Skin dispatch exceeds device compute limits".into());
        }
        let storage = |label, contents: &[u8], mutable| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents,
                usage: wgpu::BufferUsages::STORAGE
                    | if mutable {
                        wgpu::BufferUsages::COPY_DST
                    } else {
                        wgpu::BufferUsages::empty()
                    },
            })
        };
        let positions = storage("skin source positions", &positions, false);
        let ranges = storage("skin complete influence ranges", &ranges, false);
        let influences = storage("skin complete influences", &influences, false);
        let bones = storage("skin bone matrices and validity", &bones_data, true);
        let pose = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("skin pose transforms"),
            contents: &pose_bytes(count, input.mesh_bind, input.object_world, input.back),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let output = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("skin output positions"),
            size: count as u64 * 16,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("all-influence position skinning"),
            source: wgpu::ShaderSource::Wgsl(include_str!("gpu_skin_compute.wgsl").into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("all-influence position skinning"),
            layout: None,
            module: &shader,
            entry_point: Some("skin"),
            compilation_options: Default::default(),
            cache: None,
        });
        let entries: Vec<_> = [&positions, &ranges, &influences, &bones, &pose, &output]
            .into_iter()
            .enumerate()
            .map(|(binding, b)| wgpu::BindGroupEntry {
                binding: binding as u32,
                resource: b.as_entire_binding(),
            })
            .collect();
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("all-influence skin data"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &entries,
        });
        Ok(Self {
            pipeline,
            bind_group,
            bones,
            pose,
            output,
            count,
            bone_count: input.bones.len(),
        })
    }

    /// Updates only the palette and transforms; source/influence/output buffers
    /// and pipeline remain allocated. A topology/palette-size edit rebuilds it.
    pub fn update_pose(
        &self,
        queue: &wgpu::Queue,
        bones: &[Option<Mat4>],
        mesh_bind: Mat4,
        object_world: Mat4,
        back: Mat4,
    ) -> Result<(), String> {
        if bones.len() != self.bone_count {
            return Err("Skin palette size changed; rebuild GPU skin".into());
        }
        validate_pose(bones, mesh_bind, object_world, back)?;
        queue.write_buffer(&self.bones, 0, &bone_bytes(bones));
        queue.write_buffer(
            &self.pose,
            0,
            &pose_bytes(self.count, mesh_bind, object_world, back),
        );
        Ok(())
    }

    pub fn encode(&self, encoder: &mut wgpu::CommandEncoder) {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("all-influence skin"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(self.count.div_ceil(64), 1, 1);
    }

    pub fn output(&self) -> &wgpu::Buffer {
        &self.output
    }

    /// Explicit blocking validation helper; rendering should consume `output`
    /// directly, without introducing this readback into the frame loop.
    pub fn readback(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<Vec<Vec3>, String> {
        let size = self.count as u64 * 16;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("skin validation readback"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        self.encode(&mut encoder);
        encoder.copy_buffer_to_buffer(&self.output, 0, &readback, 0, size);
        queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
        device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let bytes = readback.slice(..).get_mapped_range();
        let output = bytes
            .chunks_exact(16)
            .map(|p| {
                let component = |i| f32::from_le_bytes(p[i..i + 4].try_into().unwrap());
                Vec3::new(component(0), component(4), component(8))
            })
            .collect();
        drop(bytes);
        readback.unmap();
        Ok(output)
    }
}
