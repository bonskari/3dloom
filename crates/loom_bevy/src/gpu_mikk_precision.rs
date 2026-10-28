//! Opt-in condition-selected whole-current-group precision. No CPU tangent values.
use std::sync::Arc;

pub(crate) struct PrecisionProgram {
    pub layout: wgpu::BindGroupLayout,
    pub parallel_refinement: bool,
    pub pipelines: Arc<[wgpu::ComputePipeline]>,
}
impl PrecisionProgram {
    /// Check before parsing/creating either f64 shader. Unsupported devices keep
    /// the ordinary CPU geometry backend; the fast5 comparator is separate.
    pub fn new(device: &wgpu::Device) -> Result<Self, String> {
        Self::with_refinement(device, include_str!("gpu_mikk_refinement.wgsl"), true)
    }
    #[cfg(test)]
    pub fn serial(device: &wgpu::Device) -> Result<Self, String> {
        Self::with_refinement(
            device,
            include_str!("gpu_mikk_refinement_serial_test.wgsl"),
            false,
        )
    }
    fn with_refinement(
        device: &wgpu::Device,
        refinement_source: &'static str,
        parallel_refinement: bool,
    ) -> Result<Self, String> {
        let limits = device.limits();
        if !device.features().contains(wgpu::Features::SHADER_F64)
            || limits.max_storage_buffers_per_shader_stage < 9
            || limits.max_bindings_per_bind_group < 10
            || limits.max_compute_workgroup_size_x < 64
            || limits.max_compute_invocations_per_workgroup < 64
        {
            return Err("precise8 requires SHADER_F64, nine storage buffers, ten bindings and 64-wide compute".into());
        }
        let entries: Vec<_> = (0..10)
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
            label: Some("precise8 current geometry layout"),
            entries: &entries,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("precise8 current geometry"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        // Preserve the exact tested source/module/entrypoint combinations.
        let baseline = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("unchanged fast GPU tangent math"),
            source: wgpu::ShaderSource::Wgsl(include_str!("gpu_mikk_recipe.wgsl").into()),
        });
        let risk = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("current positive-cone precision risk"),
            source: wgpu::ShaderSource::Wgsl(include_str!("gpu_mikk_precision.wgsl").into()),
        });
        let refinement = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("finite current whole-group refinement and output validation"),
            source: wgpu::ShaderSource::Wgsl(refinement_source.into()),
        });
        let pipelines = [
            "guard_init",
            "derive",
            "evaluate",
            "risk",
            "refine_group",
            "epilogue",
            "pack",
            "validate_output",
        ]
        .into_iter()
        .map(|entry| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                module: if entry == "risk" {
                    &risk
                } else if matches!(entry, "refine_group" | "validate_output") {
                    &refinement
                } else {
                    &baseline
                },
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        })
        .collect::<Vec<_>>()
        .into();
        Ok(Self {
            layout,
            pipelines,
            parallel_refinement,
        })
    }
}
