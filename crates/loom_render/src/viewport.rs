use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use wgpu::*;

use crate::grid::GridPipeline;
use crate::mesh_buffer::{GpuVertex, MeshBuffer};

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct ViewUniform {
    view_proj: [[f32; 4]; 4],
    camera_pos: [f32; 4], // xyz used, w padding
    /// x = lighting mode (0 headlight, 1 flat, 2 scene/key light), yzw unused.
    params: [f32; 4],
    /// Hidden Line: draw every mesh in this colour (a > 0 enables).
    override_color: [f32; 4],
}

/// Shading viewport label menu modes the renderer understands.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ViewShading {
    pub lighting: u32,
    pub override_color: Option<[f32; 3]>,
}

/// Number of simultaneous viewports (uniform slots) supported.
pub const MAX_VIEWPORTS: usize = 16;

/// Construction plane of a viewport's home grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridPlane {
    XY,
    XZ,
    YZ,
}

pub struct ViewportRenderer {
    mesh_pipeline: RenderPipeline,
    /// Backface Cull display property.
    mesh_pipeline_cull: RenderPipeline,
    /// See-Through display property (alpha blended, no depth write).
    mesh_pipeline_blend: RenderPipeline,
    slots: Vec<(Buffer, BindGroup)>,
    grid: GridPipeline,
    texture_layout: BindGroupLayout,
    sampler: Sampler,
}

/// The textures a frame can sample: none (a clear 1×1), the checker and
/// the bitmaps uploaded with the meshes.
pub struct TextureSet {
    pub none: std::sync::Arc<BindGroup>,
    pub checker: std::sync::Arc<BindGroup>,
    pub bitmaps: Vec<std::sync::Arc<BindGroup>>,
}

impl TextureSet {
    pub fn bind_group(&self, sel: crate::mesh_buffer::TextureSel) -> &BindGroup {
        match sel {
            crate::mesh_buffer::TextureSel::None => &self.none,
            crate::mesh_buffer::TextureSel::Checker => &self.checker,
            crate::mesh_buffer::TextureSel::Bitmap(i) => self.bitmaps.get(i as usize).unwrap_or(&self.none),
        }
    }
}

impl ViewportRenderer {
    pub fn new(device: &Device, target_format: TextureFormat) -> Self {
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("mesh.wgsl"),
            source: ShaderSource::Wgsl(include_str!("shaders/mesh.wgsl").into()),
        });

        let view_bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("view_bind_group_layout"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX_FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let slots: Vec<(Buffer, BindGroup)> = (0..MAX_VIEWPORTS)
            .map(|i| {
                let buffer = device.create_buffer(&BufferDescriptor {
                    label: Some("view_uniform"),
                    size: std::mem::size_of::<ViewUniform>() as u64,
                    usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let bind_group = device.create_bind_group(&BindGroupDescriptor {
                    label: Some(&format!("view_bind_group_{i}")),
                    layout: &view_bind_group_layout,
                    entries: &[BindGroupEntry {
                        binding: 0,
                        resource: buffer.as_entire_binding(),
                    }],
                });
                (buffer, bind_group)
            })
            .collect();

        let texture_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("texture_bind_group_layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture { sample_type: TextureSampleType::Float { filterable: true }, view_dimension: TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                BindGroupLayoutEntry { binding: 1, visibility: ShaderStages::FRAGMENT, ty: BindingType::Sampler(SamplerBindingType::Filtering), count: None },
            ],
        });
        let sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("map_sampler"),
            address_mode_u: AddressMode::Repeat,
            address_mode_v: AddressMode::Repeat,
            address_mode_w: AddressMode::Repeat,
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            mipmap_filter: FilterMode::Nearest,
            ..Default::default()
        });
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("mesh_pipeline_layout"),
            bind_group_layouts: &[&view_bind_group_layout, &texture_layout],
            push_constant_ranges: &[],
        });

        let make_pipeline = |label: &str, cull: Option<Face>, blend: BlendState, depth_write: bool| {
            device.create_render_pipeline(&RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: PipelineCompilationOptions::default(),
                    buffers: &[VertexBufferLayout {
                        array_stride: std::mem::size_of::<GpuVertex>() as u64,
                        step_mode: VertexStepMode::Vertex,
                        attributes: &vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x4, 4 => Float32x4, 5 => Float32x4],
                    }],
                },
                fragment: Some(FragmentState {
                    module: &shader,
                    entry_point: Some("fs"),
                    compilation_options: PipelineCompilationOptions::default(),
                    targets: &[Some(ColorTargetState {
                        format: target_format,
                        blend: Some(blend),
                        write_mask: ColorWrites::ALL,
                    })],
                }),
                primitive: PrimitiveState {
                    topology: PrimitiveTopology::TriangleList,
                    // Two-sided by default: back faces show unless
                    // Backface Cull is on for the object.
                    cull_mode: cull,
                    ..Default::default()
                },
                depth_stencil: Some(DepthStencilState {
                    format: TextureFormat::Depth24Plus,
                    depth_write_enabled: depth_write,
                    depth_compare: CompareFunction::LessEqual,
                    stencil: StencilState::default(),
                    bias: DepthBiasState::default(),
                }),
                multisample: MultisampleState::default(),
                multiview: None,
                cache: None,
            })
        };
        let mesh_pipeline = make_pipeline("mesh_pipeline", None, BlendState::REPLACE, true);
        let mesh_pipeline_cull = make_pipeline("mesh_pipeline_cull", Some(Face::Back), BlendState::REPLACE, true);
        let mesh_pipeline_blend = make_pipeline("mesh_pipeline_blend", None, BlendState::ALPHA_BLENDING, false);

        let grid = GridPipeline::new(device, target_format, &view_bind_group_layout, 20);

        Self {
            mesh_pipeline,
            mesh_pipeline_cull,
            mesh_pipeline_blend,
            slots,
            grid,
            texture_layout,
            sampler,
        }
    }

    /// Applies Home Grid settings (rebuilds the grid buffers when changed).
    pub fn set_grid_params(&mut self, device: &Device, params: crate::grid::GridParams) {
        self.grid.set_params(device, params);
    }

    pub fn update_view(&self, queue: &Queue, slot: usize, view_proj: Mat4, camera_pos: Vec3, shading: ViewShading) {
        let oc = shading.override_color.map_or([0.0; 4], |c| [c[0], c[1], c[2], 1.0]);
        let u = ViewUniform {
            view_proj: view_proj.to_cols_array_2d(),
            camera_pos: [camera_pos.x, camera_pos.y, camera_pos.z, 0.0],
            params: [shading.lighting as f32, 0.0, 0.0, 0.0],
            override_color: oc,
        };
        let slot = slot.min(MAX_VIEWPORTS - 1);
        queue.write_buffer(&self.slots[slot].0, 0, bytemuck::bytes_of(&u));
    }

    /// Records draw commands into an already-active render pass.
    /// Caller is responsible for the pass lifecycle (clear, store, etc.).
    ///
    /// `selected` lists indices into `meshes` of currently-selected meshes —
    /// each selected mesh additionally gets a white wireframe overlay.
    #[allow(clippy::too_many_arguments)]
    pub fn record_into_pass(
        &self,
        pass: &mut RenderPass<'_>,
        slot: usize,
        meshes: &[&MeshBuffer],
        selected: &[usize],
        show_edges: bool,
        shaded: bool,
        grid: Option<GridPlane>,
        orthographic: bool,
        textures: &TextureSet,
    ) {
        let bind_group = &self.slots[slot.min(MAX_VIEWPORTS - 1)].1;
        if let Some(plane) = grid {
            self.grid.record(pass, bind_group, plane, orthographic);
        }
        if shaded {
            // Opaque faces first (per-surface cull setting), then see-through;
            // one draw per range (texture / transparency / culling).
            for transparent in [false, true] {
                for mb in meshes.iter().filter(|mb| mb.index_count > 0) {
                    for r in mb.ranges.iter().filter(|r| r.count > 0 && r.transparent == transparent) {
                        let pipeline = if transparent {
                            &self.mesh_pipeline_blend
                        } else if r.cull {
                            &self.mesh_pipeline_cull
                        } else {
                            &self.mesh_pipeline
                        };
                        pass.set_pipeline(pipeline);
                        pass.set_bind_group(0, bind_group, &[]);
                        pass.set_bind_group(1, textures.bind_group(r.texture), &[]);
                        pass.set_vertex_buffer(0, mb.vertex_buffer.slice(..));
                        pass.set_index_buffer(mb.index_buffer.slice(..), IndexFormat::Uint32);
                        pass.draw_indexed(r.start..r.start + r.count, 0, 0..1);
                    }
                }
            }
        }
        if show_edges || !selected.is_empty() {
            pass.set_pipeline(self.grid.line_pipeline());
            pass.set_bind_group(0, bind_group, &[]);
            if show_edges {
                for mb in meshes.iter().filter(|mb| mb.edge_vertex_count > 0) {
                    pass.set_vertex_buffer(0, mb.edge_vertex_buffer.slice(..));
                    pass.draw(0..mb.edge_vertex_count, 0..1);
                }
            }
            for &idx in selected {
                if let Some(mb) = meshes.get(idx).filter(|mb| mb.edge_vertex_count > 0) {
                    pass.set_vertex_buffer(0, mb.selected_edge_vertex_buffer.slice(..));
                    pass.draw(0..mb.edge_vertex_count, 0..1);
                }
            }
        }
    }
}

impl ViewportRenderer {
    /// Uploads a bitmap as a repeat-sampled RGBA texture bind group.
    fn texture_bind_group(&self, device: &Device, queue: &Queue, bm: &crate::mesh_buffer::Bitmap, label: &str) -> BindGroup {
        use wgpu::util::DeviceExt;
        let texture = device.create_texture_with_data(
            queue,
            &TextureDescriptor {
                label: Some(label),
                size: Extent3d { width: bm.width.max(1), height: bm.height.max(1), depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: TextureFormat::Rgba8UnormSrgb,
                usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &bm.rgba,
        );
        let view = texture.create_view(&TextureViewDescriptor::default());
        device.create_bind_group(&BindGroupDescriptor {
            label: Some(label),
            layout: &self.texture_layout,
            entries: &[BindGroupEntry { binding: 0, resource: BindingResource::TextureView(&view) }, BindGroupEntry { binding: 1, resource: BindingResource::Sampler(&self.sampler) }],
        })
    }

    /// The clear "none" texel (alpha 0 leaves the object colour) and the
    /// checker, made once per renderer.
    pub fn base_textures(&self, device: &Device, queue: &Queue) -> (std::sync::Arc<BindGroup>, std::sync::Arc<BindGroup>) {
        let none = crate::mesh_buffer::Bitmap { width: 1, height: 1, rgba: vec![255, 255, 255, 0] };
        (
            std::sync::Arc::new(self.texture_bind_group(device, queue, &none, "texture_none")),
            std::sync::Arc::new(self.texture_bind_group(device, queue, &crate::mesh_buffer::Bitmap::checker(), "texture_checker")),
        )
    }

    /// One bitmap as a texture bind group (cached by the caller per path).
    pub fn bitmap_texture(&self, device: &Device, queue: &Queue, bm: &crate::mesh_buffer::Bitmap, label: &str) -> std::sync::Arc<BindGroup> {
        std::sync::Arc::new(self.texture_bind_group(device, queue, bm, label))
    }

    /// The texture set for a frame: base textures plus the given bitmaps in order.
    pub fn make_texture_set(&self, device: &Device, queue: &Queue, bitmaps: &[crate::mesh_buffer::Bitmap]) -> TextureSet {
        let (none, checker) = self.base_textures(device, queue);
        TextureSet { none, checker, bitmaps: bitmaps.iter().enumerate().map(|(i, b)| self.bitmap_texture(device, queue, b, &format!("texture_bitmap_{i}"))).collect() }
    }
}
