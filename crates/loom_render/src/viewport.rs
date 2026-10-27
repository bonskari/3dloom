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
    /// x = lighting mode (0 headlight, 1 flat, 2 the lights below), y = light count.
    params: [f32; 4],
    /// Hidden Line: draw every mesh in this colour (a > 0 enables).
    override_color: [f32; 4],
    /// Ambient light (rgb) of mode 2.
    ambient: [f32; 4],
    lights: [GpuLight; MAX_VIEW_LIGHTS],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct GpuLight {
    /// xyz position, w kind (0 point, 1 spot, 2 directional).
    pos_kind: [f32; 4],
    /// xyz direction the light travels, w cos(hotspot / 2).
    dir_hot: [f32; 4],
    /// rgb colour times multiplier, a cos(falloff / 2).
    color_fall: [f32; 4],
    /// Near start, near end, far start, far end.
    atten: [f32; 4],
    /// Decay exponent (0, 1, 2), decay start, near used, far used.
    decay: [f32; 4],
}

/// The most lights a shaded viewport draws.
pub const MAX_VIEW_LIGHTS: usize = 8;

/// How the light reaches a point in the shaded viewport.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ViewLightKind {
    #[default]
    Point,
    /// Hotspot and falloff as full cone angles in degrees.
    Spot { hotspot: f32, falloff: f32 },
    Directional,
}

/// One light of the shaded viewport (linear colour times multiplier).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ViewLight {
    pub kind: ViewLightKind,
    pub position: Vec3,
    pub direction: Vec3,
    pub color: Vec3,
    pub decay: u32,
    pub decay_start: f32,
    pub near: Option<(f32, f32)>,
    pub far: Option<(f32, f32)>,
}

/// Shading viewport label menu modes the renderer understands.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ViewShading {
    pub lighting: u32,
    pub override_color: Option<[f32; 3]>,
    /// Lighting mode 2: up to [`MAX_VIEW_LIGHTS`] lights and the ambient light.
    pub lights: [Option<ViewLight>; MAX_VIEW_LIGHTS],
    pub ambient: [f32; 3],
}

impl ViewShading {
    /// Mode 2 with `lights` (the first [`MAX_VIEW_LIGHTS`]) and `ambient`.
    pub fn with_lights(mut self, lights: &[ViewLight], ambient: [f32; 3]) -> Self {
        self.lighting = 2;
        self.lights = [None; MAX_VIEW_LIGHTS];
        for (slot, l) in self.lights.iter_mut().zip(lights) {
            *slot = Some(*l);
        }
        self.ambient = ambient;
        self
    }
}

fn gpu_light(l: &ViewLight) -> GpuLight {
    let (kind, hot, fall) = match l.kind {
        ViewLightKind::Point => (0.0, -2.0, -2.0),
        ViewLightKind::Spot { hotspot, falloff } => (1.0, (hotspot * 0.5).to_radians().cos(), (falloff * 0.5).to_radians().cos()),
        ViewLightKind::Directional => (2.0, -2.0, -2.0),
    };
    let (near, far) = (l.near.unwrap_or((0.0, 0.0)), l.far.unwrap_or((0.0, 0.0)));
    GpuLight {
        pos_kind: [l.position.x, l.position.y, l.position.z, kind],
        dir_hot: [l.direction.x, l.direction.y, l.direction.z, hot],
        color_fall: [l.color.x, l.color.y, l.color.z, fall],
        atten: [near.0, near.1, far.0, far.1],
        decay: [l.decay as f32, l.decay_start, l.near.is_some() as u32 as f32, l.far.is_some() as u32 as f32],
    }
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
        let mut lights = [GpuLight::zeroed(); MAX_VIEW_LIGHTS];
        let mut count = 0;
        for l in shading.lights.iter().flatten() {
            lights[count] = gpu_light(l);
            count += 1;
        }
        let a = shading.ambient;
        let u = ViewUniform {
            view_proj: view_proj.to_cols_array_2d(),
            camera_pos: [camera_pos.x, camera_pos.y, camera_pos.z, 0.0],
            params: [shading.lighting as f32, count as f32, 0.0, 0.0],
            override_color: oc,
            ambient: [a[0], a[1], a[2], 0.0],
            lights,
        };
        let slot = slot.min(MAX_VIEWPORTS - 1);
        queue.write_buffer(&self.slots[slot].0, 0, bytemuck::bytes_of(&u));
    }

    /// Records draw commands into an already-active render pass.
    /// Caller is responsible for the pass lifecycle (clear, store, etc.).
    ///
    /// `selected` lists indices into `meshes` of currently-selected meshes —
    /// each selected mesh additionally gets a white wireframe overlay.
    /// `shade_only`: in a wireframe view (`shaded` false), these meshes are
    /// shaded anyway (Views > Shade Selected).
    #[allow(clippy::too_many_arguments)]
    pub fn record_into_pass(
        &self,
        pass: &mut RenderPass<'_>,
        slot: usize,
        meshes: &[&MeshBuffer],
        selected: &[usize],
        show_edges: bool,
        shaded: bool,
        shade_only: &[usize],
        grid: Option<GridPlane>,
        orthographic: bool,
        textures: &TextureSet,
        lines: Option<&crate::mesh_buffer::LineBuffer>,
    ) {
        let bind_group = &self.slots[slot.min(MAX_VIEWPORTS - 1)].1;
        if let Some(plane) = grid {
            self.grid.record(pass, bind_group, plane, orthographic);
        }
        let shade = shaded_meshes(meshes.len(), shaded, shade_only);
        if !shade.is_empty() {
            // Opaque faces first (per-surface cull setting), then see-through;
            // one draw per range (texture / transparency / culling).
            for transparent in [false, true] {
                for mb in shade.iter().map(|i| meshes[*i]).filter(|mb| mb.index_count > 0) {
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
            // Wire materials: their faces' edges.
            pass.set_pipeline(self.grid.line_pipeline());
            pass.set_bind_group(0, bind_group, &[]);
            for mb in shade.iter().map(|i| meshes[*i]).filter(|mb| mb.wire_vertex_count > 0) {
                pass.set_vertex_buffer(0, mb.wire_vertex_buffer.slice(..));
                pass.draw(0..mb.wire_vertex_count, 0..1);
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
            if !selected.is_empty() {
                pass.set_pipeline(self.grid.selected_line_pipeline());
            }
            for &idx in selected {
                if let Some(mb) = meshes.get(idx).filter(|mb| mb.edge_vertex_count > 0) {
                    pass.set_vertex_buffer(0, mb.edge_vertex_buffer.slice(..));
                    pass.draw(0..mb.edge_vertex_count, 0..1);
                }
            }
        }
        if let Some(lines) = lines {
            pass.set_pipeline(self.grid.line_pipeline());
            pass.set_bind_group(0, bind_group, &[]);
            pass.set_vertex_buffer(0, lines.buffer.slice(..));
            pass.draw(0..lines.count, 0..1);
        }
    }
}

/// The meshes a viewport draws shaded: all of them in a shaded view, else
/// only `shade_only` (Shade Selected in a wireframe view).
pub fn shaded_meshes(count: usize, shaded: bool, shade_only: &[usize]) -> Vec<usize> {
    if shaded {
        (0..count).collect()
    } else {
        shade_only.iter().copied().filter(|i| *i < count).collect()
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

#[cfg(test)]
mod shade_tests {
    use super::shaded_meshes;

    /// Shade Selected: a wireframe view shades the selected meshes only.
    #[test]
    fn wireframe_views_shade_only_the_shade_selected_meshes() {
        assert_eq!(shaded_meshes(3, true, &[]), vec![0, 1, 2]);
        assert_eq!(shaded_meshes(3, false, &[]), Vec::<usize>::new());
        assert_eq!(shaded_meshes(3, false, &[2, 7]), vec![2]);
    }
}

#[cfg(test)]
mod selected_line_tests {
    use super::*;
    use wgpu::util::{BufferInitDescriptor, DeviceExt};

    #[test]
    fn selected_viewport_matches_legacy_white_edges_with_default_and_subobject_lines() {
        let instance = Instance::new(&Default::default());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
            .expect("real GPU required");
        let (device, queue) =
            pollster::block_on(adapter.request_device(&DeviceDescriptor::default(), None)).unwrap();
        for format in [TextureFormat::Rgba8Unorm, TextureFormat::Rgba8UnormSrgb] {
            let renderer = ViewportRenderer::new(&device, format);
            renderer.update_view(
                &queue,
                0,
                Mat4::from_rotation_z(0.13),
                Vec3::Z * 2.0,
                ViewShading::default(),
            );
            let textures = renderer.make_texture_set(&device, &queue, &[]);
            let meshes = [
                loom_mesh::quad_primitive(Vec3::new(-0.9, -0.8, 0.3), Vec3::X * 1.4, Vec3::Y * 1.3),
                loom_mesh::quad_primitive(Vec3::new(-0.2, -0.4, 0.6), Vec3::X * 1.0, Vec3::Y * 1.1),
            ];
            let styles = [
                crate::MeshStyle {
                    color: [0.2, 0.6, 0.8],
                    wire_color: Some([0.8, 0.2, 0.1]),
                    diagonals: true,
                    ..Default::default()
                },
                crate::MeshStyle {
                    color: [0.3, 0.8, 0.2],
                    wire: true,
                    ..Default::default()
                },
            ];
            let buffers: Vec<_> = meshes
                .iter()
                .zip(&styles)
                .map(|(m, s)| MeshBuffer::from_mesh(&device, m, s.clone()))
                .collect();
            let mut white = Vec::new();
            for (mesh, style) in meshes.iter().zip(&styles) {
                let mut edges = mesh.unique_edges();
                if style.diagonals {
                    edges.extend(crate::mesh_buffer::dashed(
                        &crate::mesh_buffer::fan_diagonals(mesh),
                        crate::mesh_buffer::DIAGONAL_DASHES,
                    ));
                }
                let vertices: Vec<_> = edges
                    .iter()
                    .flat_map(|(a, b)| {
                        [
                            crate::grid::LineVertex {
                                position: a.to_array(),
                                color: [1.0; 3],
                            },
                            crate::grid::LineVertex {
                                position: b.to_array(),
                                color: [1.0; 3],
                            },
                        ]
                    })
                    .collect();
                assert_eq!(
                    vertices.len() as u32,
                    buffers[white.len()].edge_vertex_count
                );
                white.push(device.create_buffer_init(&BufferInitDescriptor {
                    label: Some("legacy selected white edge reference"),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: BufferUsages::VERTEX,
                }));
            }
            let lines = crate::mesh_buffer::LineBuffer::new(
                &device,
                &[(
                    Vec3::new(-0.8, 0.1, 0.1),
                    Vec3::new(0.8, 0.1, 0.1),
                    [0.0, 0.0, 1.0],
                )],
            )
            .unwrap();
            let references: Vec<_> = buffers.iter().collect();
            let render = |selected: &[usize], show_edges: bool, shaded: bool, legacy: bool| {
                let texture = device.create_texture(&TextureDescriptor {
                    label: None,
                    size: Extent3d {
                        width: 64,
                        height: 64,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: TextureDimension::D2,
                    format,
                    usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
                    view_formats: &[],
                });
                let depth = device.create_texture(&TextureDescriptor {
                    label: None,
                    size: texture.size(),
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: TextureDimension::D2,
                    format: TextureFormat::Depth24Plus,
                    usage: TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                });
                let view = texture.create_view(&Default::default());
                let depth = depth.create_view(&Default::default());
                let staging = device.create_buffer(&BufferDescriptor {
                    label: None,
                    size: 64 * 256,
                    usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let mut encoder = device.create_command_encoder(&Default::default());
                {
                    let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                        label: None,
                        color_attachments: &[Some(RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: Operations {
                                load: LoadOp::Clear(Color {
                                    r: 0.03,
                                    g: 0.07,
                                    b: 0.11,
                                    a: 1.0,
                                }),
                                store: StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                            view: &depth,
                            depth_ops: Some(Operations {
                                load: LoadOp::Clear(1.0),
                                store: StoreOp::Store,
                            }),
                            stencil_ops: None,
                        }),
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                    renderer.record_into_pass(
                        &mut pass,
                        0,
                        &references,
                        if legacy { &[] } else { selected },
                        show_edges,
                        shaded,
                        &[1],
                        Some(GridPlane::XY),
                        false,
                        &textures,
                        if legacy { None } else { Some(&lines) },
                    );
                    if legacy {
                        pass.set_pipeline(renderer.grid.line_pipeline());
                        pass.set_bind_group(0, &renderer.slots[0].1, &[]);
                        for &i in selected {
                            pass.set_vertex_buffer(0, white[i].slice(..));
                            pass.draw(0..buffers[i].edge_vertex_count, 0..1);
                        }
                        pass.set_vertex_buffer(0, lines.buffer.slice(..));
                        pass.draw(0..lines.count, 0..1);
                    }
                }
                encoder.copy_texture_to_buffer(
                    TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: 0,
                        origin: Origin3d::ZERO,
                        aspect: TextureAspect::All,
                    },
                    TexelCopyBufferInfo {
                        buffer: &staging,
                        layout: TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(256),
                            rows_per_image: Some(64),
                        },
                    },
                    texture.size(),
                );
                queue.submit([encoder.finish()]);
                let slice = staging.slice(..);
                let (tx, rx) = std::sync::mpsc::channel();
                slice.map_async(MapMode::Read, move |r| tx.send(r).unwrap());
                device.poll(Maintain::Wait);
                rx.recv().unwrap().unwrap();
                let bytes = slice.get_mapped_range().to_vec();
                staging.unmap();
                bytes
            };
            for selected in [&[][..], &[0][..], &[0, 1][..]] {
                for show_edges in [false, true] {
                    for shaded in [false, true] {
                        assert_eq!(
                            render(selected, show_edges, shaded, false),
                            render(selected, show_edges, shaded, true),
                            "selected={selected:?} edges={show_edges} shaded={shaded}"
                        );
                    }
                }
            }
            let pixels = render(&[0], false, false, false);
            assert!(
                pixels.chunks_exact(4).any(|p| p == [0, 0, 255, 255]),
                "subobject lines must retain blue"
            );
            assert!(
                pixels.chunks_exact(4).any(|p| p == [255; 4]),
                "selection must be visible white"
            );
            assert_ne!(
                pixels,
                render(&[], false, false, false),
                "selection positive control must change pixels"
            );
        }
    }
}
