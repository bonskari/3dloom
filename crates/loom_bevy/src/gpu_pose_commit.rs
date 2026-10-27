//! One synchronization point for all candidate geometry guards before commit.

pub(crate) struct GuardReadback {
    staging: wgpu::Buffer,
    count: usize,
}

impl GuardReadback {
    pub(crate) fn new(device: &wgpu::Device, count: usize) -> Result<Self, String> {
        let bytes = u64::try_from(count).ok().and_then(|n| n.checked_mul(4))
            .filter(|n| *n > 0 && *n <= device.limits().max_buffer_size)
            .ok_or("Geometry guard batch exceeds staging limits or is empty")?;
        Ok(Self {
            staging: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("all current geometry guards"), size: bytes,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }), count,
        })
    }

    /// This submission also completes preceding candidate compute submissions.
    /// A nonzero word rejects the complete candidate pose; callers recompile
    /// current structure before copying any candidate into render buffers.
    pub(crate) fn read(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, guards: &[&wgpu::Buffer]) -> Result<Vec<u32>, String> {
        if guards.len() != self.count || guards.iter().any(|b|
            b.size() < 4 || !b.usage().contains(wgpu::BufferUsages::COPY_SRC)) {
            return Err("Geometry guard count or source layout changed".into());
        }
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("batch geometry guard readback"),
        });
        for (i, guard) in guards.iter().enumerate() {
            encoder.copy_buffer_to_buffer(guard, 0, &self.staging, i as u64 * 4, 4);
        }
        let submission = queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        self.staging.slice(..).map_async(wgpu::MapMode::Read, move |result| { let _ = tx.send(result); });
        device.poll(wgpu::Maintain::WaitForSubmissionIndex(submission));
        rx.recv().map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
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
    fn batch_reads_current_guards_and_reuses_one_staging_buffer() {
        let instance = wgpu::Instance::new(&Default::default());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        let (device, queue) = pollster::block_on(adapter.request_device(&crate::device_descriptor(&adapter), None)).unwrap();
        assert!(GuardReadback::new(&device, 0).is_err());
        assert!(GuardReadback::new(&device, usize::MAX).is_err());
        let guards: Vec<_> = (0..13).map(|_| device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None, contents: bytemuck::bytes_of(&0u32), usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
        })).collect();
        let refs: Vec<_> = guards.iter().collect();
        let mut batch = GuardReadback::new(&device, guards.len()).unwrap();
        assert_eq!(batch.staging.size(), 52);
        assert_eq!(batch.read(&device, &queue, &refs).unwrap(), vec![0; 13]);
        queue.write_buffer(&guards[7], 0, bytemuck::bytes_of(&17u32));
        let mut expected = vec![0; 13]; expected[7] = 17;
        assert_eq!(batch.read(&device, &queue, &refs).unwrap(), expected);
        assert!(batch.read(&device, &queue, &refs[..12]).is_err());
        let invalid = device.create_buffer(&wgpu::BufferDescriptor {
            label: None, size: 4, usage: wgpu::BufferUsages::STORAGE, mapped_at_creation: false,
        });
        let mut invalid_refs = refs.clone(); invalid_refs[5] = &invalid;
        assert!(batch.read(&device, &queue, &invalid_refs).is_err());
        queue.write_buffer(&guards[7], 0, bytemuck::bytes_of(&0u32));
        assert_eq!(batch.read(&device, &queue, &refs).unwrap(), vec![0; 13]);
    }
}
