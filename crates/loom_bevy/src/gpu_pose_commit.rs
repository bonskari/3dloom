//! One synchronization point for all candidate geometry guards before commit.

pub(crate) struct PoseCandidate<'a> {
    pub target: &'a mut crate::dynamic_geometry::DynamicGeometryBuffer,
    pub guard: &'a wgpu::Buffer,
    pub output: &'a wgpu::Buffer,
    pub offset: u64,
    pub positions: &'a [[f32; 3]],
    pub prepared: Option<&'a mut crate::dynamic_geometry::PreparedPositions>,
    pub indices: Option<&'a [u32]>,
}

#[derive(Debug)]
pub(crate) enum PoseCommitError {
    Layout { part: usize, reason: String },
    Readback(String),
    Guards(Vec<u32>),
}

pub(crate) struct PoseBounds {
    pub parts: Vec<bevy::render::primitives::Aabb>,
    pub min: glam::Vec3,
    pub max: glam::Vec3,
}

/// Validate the complete pose before changing any render buffer or history.
/// Copies are submitted here so successful commits cannot leave an unsubmitted
/// encoder behind. The caller inserts all returned bounds before Bevy update.
pub(crate) fn commit_pose(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    reader: &mut GuardReadback,
    candidates: Vec<PoseCandidate<'_>>,
) -> Result<PoseBounds, PoseCommitError> {
    for (part, candidate) in candidates.iter().enumerate() {
        if candidates
            .iter()
            .any(|other| candidate.output == &**other.target.buffer())
        {
            return Err(PoseCommitError::Layout {
                part,
                reason: "GPU candidate aliases a render target buffer".into(),
            });
        }
    }
    let guards: Vec<_> = candidates.iter().map(|c| c.guard).collect();
    let prepared: Vec<_> = candidates.into_iter().enumerate().map(|(part, c)| {
        match c.prepared {
            Some(prepared) => c.target.prepare_gpu_fused(c.output, c.offset, prepared, c.indices),
            None => c.target.prepare_gpu(c.output, c.offset, c.positions, c.indices),
        }.map_err(|reason| PoseCommitError::Layout { part, reason })
    }).collect::<Result<_, _>>()?;
    let flags = reader
        .read(device, queue, &guards)
        .map_err(PoseCommitError::Readback)?;
    if flags.iter().any(|v| *v != 0) {
        return Err(PoseCommitError::Guards(flags));
    }
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("commit complete current geometry pose"),
    });
    let mut bounds = PoseBounds { parts: Vec::with_capacity(prepared.len()), min: glam::Vec3::splat(f32::MAX), max: glam::Vec3::splat(f32::MIN) };
    for copy in prepared {
        let (part, (min, max)) = copy.commit(&mut encoder);
        bounds.parts.push(part);
        bounds.min = bounds.min.min(min);
        bounds.max = bounds.max.max(max);
    }
    queue.submit([encoder.finish()]);
    Ok(bounds)
}

pub(crate) struct GuardReadback {
    staging: wgpu::Buffer,
    count: usize,
}

impl GuardReadback {
    pub(crate) fn new(device: &wgpu::Device, count: usize) -> Result<Self, String> {
        let bytes = u64::try_from(count)
            .ok()
            .and_then(|n| n.checked_mul(4))
            .filter(|n| *n > 0 && *n <= device.limits().max_buffer_size)
            .ok_or("Geometry guard batch exceeds staging limits or is empty")?;
        Ok(Self {
            staging: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("all current geometry guards"),
                size: bytes,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            count,
        })
    }

    /// This submission also completes preceding candidate compute submissions.
    /// A nonzero word rejects the complete candidate pose; callers recompile
    /// current structure before copying any candidate into render buffers.
    pub(crate) fn read(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        guards: &[&wgpu::Buffer],
    ) -> Result<Vec<u32>, String> {
        let _scope = loom_render::profile::Scope::new("gpu_pose.guard_readback");
        if guards.len() != self.count
            || guards
                .iter()
                .any(|b| b.size() < 4 || !b.usage().contains(wgpu::BufferUsages::COPY_SRC))
        {
            return Err("Geometry guard count or source layout changed".into());
        }
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("batch geometry guard readback"),
        });
        for (i, guard) in guards.iter().enumerate() {
            encoder.copy_buffer_to_buffer(guard, 0, &self.staging, i as u64 * 4, 4);
        }
        let submission = queue.submit([encoder.finish()]);
        let _wait = loom_render::profile::Scope::new("gpu_pose.guard_wait");
        let (tx, rx) = std::sync::mpsc::channel();
        self.staging
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
        device.poll(wgpu::Maintain::WaitForSubmissionIndex(submission));
        rx.recv()
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let flags = {
            let mapped = self.staging.slice(..).get_mapped_range();
            bytemuck::cast_slice::<u8, u32>(&mapped).to_vec()
        };
        self.staging.unmap();
        Ok(flags)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::util::DeviceExt;

    #[test]
    fn one_failed_guard_rejects_every_candidate_before_commit() {
        let instance = wgpu::Instance::new(&Default::default());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        let (device, queue) =
            pollster::block_on(adapter.request_device(&crate::device_descriptor(&adapter), None))
                .unwrap();
        let mut source = loom_mesh::box_primitive(glam::Vec3::ONE);
        let initial = crate::mesh_data(&source);
        let mut targets: Vec<_> = (0..2)
            .map(|_| {
                crate::dynamic_geometry::DynamicGeometryBuffer::new(&device, &initial.to_bevy())
                    .unwrap()
                    .0
            })
            .collect();
        loom_mesh::edit_ops::translate_mesh(&mut source, glam::Vec3::X * 7.0).unwrap();
        let current = crate::mesh_data(&source);
        let output =
            crate::dynamic_geometry::DynamicGeometryBuffer::new(&device, &current.to_bevy())
                .unwrap()
                .0;
        let guards: Vec<_> = [0u32, 8]
            .into_iter()
            .map(|flag| {
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: bytemuck::bytes_of(&flag),
                    usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
                })
            })
            .collect();
        let mut reader = GuardReadback::new(&device, 2).unwrap();
        let candidates = targets
            .iter_mut()
            .zip(&guards)
            .enumerate()
            .map(|(i, (target, guard))| PoseCandidate {
                target,
                guard,
                output: output.buffer(),
                offset: 0,
                positions: if i == 1 {
                    &current.positions[1..]
                } else {
                    &current.positions
                },
                prepared: None,
                indices: Some(&current.indices),
            })
            .collect();
        assert!(matches!(
            commit_pose(&device, &queue, &mut reader, candidates),
            Err(PoseCommitError::Layout { part: 1, .. })
        ));
        let candidates = targets
            .iter_mut()
            .zip(&guards)
            .map(|(target, guard)| PoseCandidate {
                target,
                guard,
                output: output.buffer(),
                offset: 0,
                positions: &current.positions,
                prepared: None,
                indices: Some(&current.indices),
            })
            .collect();
        assert!(
            matches!(commit_pose(&device, &queue, &mut reader, candidates), Err(PoseCommitError::Guards(flags)) if flags == vec![0,8])
        );
        let refs: Vec<_> = targets.iter().map(|t| &**t.buffer()).collect();
        assert_eq!(
            reader.read(&device, &queue, &refs).unwrap(),
            vec![initial.positions[0][0].to_bits(); 2]
        );
        queue.write_buffer(&guards[1], 0, bytemuck::bytes_of(&0u32));
        let alias = targets[0].buffer().clone();
        let candidates = targets
            .iter_mut()
            .zip(&guards)
            .enumerate()
            .map(|(i, (target, guard))| PoseCandidate {
                target,
                guard,
                output: if i == 1 { &alias } else { output.buffer() },
                offset: 0,
                positions: &current.positions,
                prepared: None,
                indices: Some(&current.indices),
            })
            .collect();
        assert!(matches!(
            commit_pose(&device, &queue, &mut reader, candidates),
            Err(PoseCommitError::Layout { part: 1, .. })
        ));
        let refs: Vec<_> = targets.iter().map(|t| &**t.buffer()).collect();
        assert_eq!(
            reader.read(&device, &queue, &refs).unwrap(),
            vec![initial.positions[0][0].to_bits(); 2]
        );
        let candidates = targets
            .iter_mut()
            .zip(&guards)
            .map(|(target, guard)| PoseCandidate {
                target,
                guard,
                output: output.buffer(),
                offset: 0,
                positions: &current.positions,
                prepared: None,
                indices: Some(&current.indices),
            })
            .collect();
        let bounds = commit_pose(&device, &queue, &mut reader, candidates).unwrap();
        assert_eq!(bounds.parts.len(), 2);
        let expected = current.positions.iter().fold((glam::Vec3::splat(f32::MAX), glam::Vec3::splat(f32::MIN)), |(min, max), p| {
            (min.min(glam::Vec3::from_array(*p)), max.max(glam::Vec3::from_array(*p)))
        });
        assert_eq!((bounds.min, bounds.max), expected);

        let refs: Vec<_> = targets.iter().map(|t| &**t.buffer()).collect();
        assert_eq!(
            reader.read(&device, &queue, &refs).unwrap(),
            vec![current.positions[0][0].to_bits(); 2]
        );
    }

    #[test]
    fn batch_reads_current_guards_and_reuses_one_staging_buffer() {
        let instance = wgpu::Instance::new(&Default::default());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        let (device, queue) =
            pollster::block_on(adapter.request_device(&crate::device_descriptor(&adapter), None))
                .unwrap();
        assert!(GuardReadback::new(&device, 0).is_err());
        assert!(GuardReadback::new(&device, usize::MAX).is_err());
        let guards: Vec<_> = (0..13)
            .map(|_| {
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: bytemuck::bytes_of(&0u32),
                    usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
                })
            })
            .collect();
        let refs: Vec<_> = guards.iter().collect();
        let mut batch = GuardReadback::new(&device, guards.len()).unwrap();
        assert_eq!(batch.staging.size(), 52);
        assert_eq!(batch.read(&device, &queue, &refs).unwrap(), vec![0; 13]);
        queue.write_buffer(&guards[7], 0, bytemuck::bytes_of(&17u32));
        let mut expected = vec![0; 13];
        expected[7] = 17;
        assert_eq!(batch.read(&device, &queue, &refs).unwrap(), expected);
        assert!(batch.read(&device, &queue, &refs[..12]).is_err());
        let invalid = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 4,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let mut invalid_refs = refs.clone();
        invalid_refs[5] = &invalid;
        assert!(batch.read(&device, &queue, &invalid_refs).is_err());
        queue.write_buffer(&guards[7], 0, bytemuck::bytes_of(&0u32));
        assert_eq!(batch.read(&device, &queue, &refs).unwrap(), vec![0; 13]);
    }
}
