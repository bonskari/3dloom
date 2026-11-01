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
    /// Each shadow map layer's view-projection (world to the light's clip space).
    shadow_vps: [[[f32; 4]; 4]; MAX_SHADOW_LAYERS],
    /// Per light: x = its first layer (-1 none), y = layer count (1, or 6 for a point light's cube).
    light_shadow: [[f32; 4]; MAX_VIEW_LIGHTS],
    /// x = shadows on, y = soft (filtered edges), z = depth bias, w = intensity (0-1).
    shadow: [f32; 4],
    /// Clip space back to world (the AO pass rebuilds neighbours' positions).
    inv_view_proj: [[f32; 4]; 4],
    /// x = AO on, y = radius (world units), z = intensity.
    ao: [f32; 4],
    /// The view's rectangle in framebuffer pixels: x, y, width, height.
    viewport_px: [f32; 4],
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

/// Per-view drawing switches (`ViewShading::flags`): the Rendering Level
/// variants and the Viewport Configuration rendering options.
pub mod view_flags {
    /// Smooth / Facets: no specular highlights.
    pub const NO_HIGHLIGHTS: u32 = 1;
    /// Facets / Facets + Highlights: each face shaded flat.
    pub const FACETED: u32 = 2;
    /// Transparency None: see-through faces draw opaque.
    pub const OPAQUE: u32 = 4;
    /// Transparency Simple: see-through faces draw as a screen door.
    pub const SCREEN_DOOR: u32 = 8;
    /// Disable Textures: maps are not shown.
    pub const NO_TEXTURES: u32 = 16;
    /// Force 2-Sided: back faces draw too.
    pub const TWO_SIDED: u32 = 32;
}

/// Shading viewport label menu modes the renderer understands.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ViewShading {
    pub lighting: u32,
    /// `view_flags` bits.
    pub flags: u32,
    pub override_color: Option<[f32; 3]>,
    /// Lighting mode 2: up to [`MAX_VIEW_LIGHTS`] lights and the ambient light.
    pub lights: [Option<ViewLight>; MAX_VIEW_LIGHTS],
    pub ambient: [f32; 3],
    /// Viewport shadows: the shadow maps of the view's lights.
    pub shadow: Option<ShadowSet>,
    /// Viewport ambient occlusion: radius in world units, intensity (0-1) and
    /// the view-projection its depth pass uses (the view's, with near and
    /// far fitted to the scene so depths rebuild positions precisely).
    pub ao: Option<(f32, f32, Mat4)>,
    /// The view's rectangle in framebuffer pixels (x, y, width, height).
    pub viewport_px: [f32; 4],
}

/// The shadow maps a shaded view draws: a layer per spot or directional
/// light and six (a cube) per point light, as many as fit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowSet {
    pub layers: [Mat4; MAX_SHADOW_LAYERS],
    pub layer_count: u32,
    /// Per light: (first layer, count); count 0 casts none.
    pub lights: [(u32, u32); MAX_VIEW_LIGHTS],
    /// Soft: filtered edges; off: hard.
    pub soft: bool,
    /// How dark the shadows are (0 none, 1 the light fully blocked).
    pub intensity: f32,
}

/// Shadow map size in texels (each side).
pub const SHADOW_MAP_SIZE: u32 = 1024;
/// Shadow map layers a view holds (a point light takes six).
pub const MAX_SHADOW_LAYERS: usize = 16;

/// A point light's six cube faces (+X, -X, +Y, -Y, +Z, -Z, as the shader
/// picks them), each a 90° view out to the scene bounds.
pub fn point_shadow_view_projs(position: Vec3, bounds: (Vec3, Vec3)) -> Option<[Mat4; 6]> {
    let (lo, hi) = bounds;
    if !(lo.is_finite() && hi.is_finite()) || lo.cmpgt(hi).any() {
        return None;
    }
    // Out to the farthest corner of the bounds.
    let far = (0..8)
        .map(|i| Vec3::new(if i & 1 == 0 { lo.x } else { hi.x }, if i & 2 == 0 { lo.y } else { hi.y }, if i & 4 == 0 { lo.z } else { hi.z }))
        .map(|c| (c - position).length())
        .fold(0.0, f32::max)
        .max(1e-2);
    let near = (far * 0.005).max(0.01);
    let proj = Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, near, far);
    let faces = [(Vec3::X, -Vec3::Y), (-Vec3::X, -Vec3::Y), (Vec3::Y, Vec3::Z), (-Vec3::Y, -Vec3::Z), (Vec3::Z, -Vec3::Y), (-Vec3::Z, -Vec3::Y)];
    Some(faces.map(|(dir, up)| proj * Mat4::look_to_rh(position, dir, up)))
}

/// The shadow maps for `lights` framing `bounds`: spot and directional
/// lights a layer each, point lights a cube, in light order while layers last.
pub fn shadow_set(lights: &[Option<ViewLight>; MAX_VIEW_LIGHTS], bounds: (Vec3, Vec3), soft: bool, intensity: f32) -> Option<ShadowSet> {
    let mut set = ShadowSet { layers: [Mat4::IDENTITY; MAX_SHADOW_LAYERS], layer_count: 0, lights: [(0, 0); MAX_VIEW_LIGHTS], soft, intensity: intensity.clamp(0.0, 1.0) };
    for (i, l) in lights.iter().enumerate() {
        let Some(l) = l else { continue };
        let mats: Vec<Mat4> = match l.kind {
            ViewLightKind::Point => point_shadow_view_projs(l.position, bounds).map(|m| m.to_vec()).unwrap_or_default(),
            _ => shadow_view_proj(l, bounds).into_iter().collect(),
        };
        let start = set.layer_count as usize;
        if mats.is_empty() || start + mats.len() > MAX_SHADOW_LAYERS {
            continue;
        }
        set.layers[start..start + mats.len()].copy_from_slice(&mats);
        set.lights[i] = (start as u32, mats.len() as u32);
        set.layer_count += mats.len() as u32;
    }
    (set.layer_count > 0).then_some(set)
}

/// The view-projection a spot or directional light casts its shadow map
/// with, framing the scene bounds `(min, max)`; point lights cast none here.
pub fn shadow_view_proj(l: &ViewLight, bounds: (Vec3, Vec3)) -> Option<Mat4> {
    let (lo, hi) = bounds;
    if !(lo.is_finite() && hi.is_finite()) || lo.cmpgt(hi).any() {
        return None;
    }
    let centre = (lo + hi) * 0.5;
    let radius = ((hi - lo).length() * 0.5).max(1e-3);
    let dir = l.direction.try_normalize()?;
    let up = if dir.cross(Vec3::Z).length_squared() < 1e-6 { Vec3::Y } else { Vec3::Z };
    match l.kind {
        ViewLightKind::Directional => {
            let eye = centre - dir * radius * 2.0;
            let view = Mat4::look_at_rh(eye, centre, up);
            Some(Mat4::orthographic_rh(-radius, radius, -radius, radius, radius * 0.01, radius * 4.0) * view)
        }
        ViewLightKind::Spot { falloff, .. } => {
            let view = Mat4::look_to_rh(l.position, dir, up);
            // Near as far out as the scene allows: depth precision follows far / near.
            let d = (l.position - centre).length();
            let far = d + radius;
            let near = (d - radius).max(far * 0.02).max(0.01);
            Some(Mat4::perspective_rh(falloff.clamp(1.0, 170.0).to_radians(), 1.0, near, far.max(near * 2.0)) * view)
        }
        ViewLightKind::Point => None,
    }
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

/// One viewport's shadow map: its depth texture, the light-view uniform the
/// depth pass draws with, and the group the shaded pass samples it through.
struct ShadowSlot {
    _texture: Texture,
    /// One render view, uniform and group per layer.
    layers: Vec<(TextureView, Buffer, BindGroup)>,
    /// All the layers, sampled by the shaded pass.
    all: TextureView,
}

/// One viewport's depth from its own camera, for ambient occlusion.
struct AoSlot {
    _texture: Texture,
    view: TextureView,
    size: [u32; 2],
    uniform: Buffer,
    group: BindGroup,
}

/// What a viewport's shaded pass samples besides its maps: shadow maps and AO depth.
#[derive(Default)]
struct Extras {
    shadow: Option<ShadowSlot>,
    ao: Option<AoSlot>,
    group: Option<BindGroup>,
}

pub struct ViewportRenderer {
    mesh_pipeline: RenderPipeline,
    /// Depth only, from a light: the shadow maps.
    shadow_pipeline: RenderPipeline,
    /// Depth only, from the view's camera, unbiased: the AO depth.
    depth_pipeline: RenderPipeline,
    view_layout: BindGroupLayout,
    shadow_layout: BindGroupLayout,
    shadow_sampler: Sampler,
    /// A 1×1 shadow map for views without shadows.
    no_shadow: BindGroup,
    /// The 1×1 depth views the shared group uses where a viewport has no map.
    no_shadow_view: TextureView,
    no_depth_view: TextureView,
    extras: Vec<Extras>,
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
        let shadow_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("shadow_bind_group_layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture { sample_type: TextureSampleType::Depth, view_dimension: TextureViewDimension::D2Array, multisampled: false },
                    count: None,
                },
                BindGroupLayoutEntry { binding: 1, visibility: ShaderStages::FRAGMENT, ty: BindingType::Sampler(SamplerBindingType::Comparison), count: None },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture { sample_type: TextureSampleType::Depth, view_dimension: TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
            ],
        });
        let shadow_sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("shadow_sampler"),
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            compare: Some(CompareFunction::LessEqual),
            ..Default::default()
        });
        let no_shadow_tex = device.create_texture(&TextureDescriptor {
            label: Some("no_shadow"),
            size: Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Depth32Float,
            usage: TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let no_shadow_view = no_shadow_tex.create_view(&TextureViewDescriptor { dimension: Some(TextureViewDimension::D2Array), ..Default::default() });
        let no_depth_view = no_shadow_tex.create_view(&TextureViewDescriptor { dimension: Some(TextureViewDimension::D2), ..Default::default() });
        let no_shadow = device.create_bind_group(&BindGroupDescriptor {
            label: Some("no_shadow"),
            layout: &shadow_layout,
            entries: &[
                BindGroupEntry { binding: 0, resource: BindingResource::TextureView(&no_shadow_view) },
                BindGroupEntry { binding: 1, resource: BindingResource::Sampler(&shadow_sampler) },
                BindGroupEntry { binding: 2, resource: BindingResource::TextureView(&no_depth_view) },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("mesh_pipeline_layout"),
            bind_group_layouts: &[&view_bind_group_layout, &texture_layout, &shadow_layout],
            push_constant_ranges: &[],
        });
        let shadow_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("shadow_pipeline_layout"),
            bind_group_layouts: &[&view_bind_group_layout],
            push_constant_ranges: &[],
        });
        let make_depth = |label: &str, bias: DepthBiasState| device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&shadow_pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_depth"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[VertexBufferLayout {
                    array_stride: std::mem::size_of::<GpuVertex>() as u64,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x4, 4 => Float32x4, 5 => Float32x4],
                }],
            },
            fragment: None,
            primitive: PrimitiveState { topology: PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
            depth_stencil: Some(DepthStencilState {
                format: TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: CompareFunction::Less,
                stencil: StencilState::default(),
                bias,
            }),
            multisample: MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let shadow_pipeline = make_depth("shadow_pipeline", DepthBiasState { constant: 2, slope_scale: 2.0, clamp: 0.0 });
        // The view's own depth for AO: no bias, positions rebuild exactly.
        let depth_pipeline = make_depth("depth_pipeline", DepthBiasState::default());

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
            shadow_pipeline,
            depth_pipeline,
            view_layout: view_bind_group_layout,
            shadow_layout,
            shadow_sampler,
            no_shadow,
            no_shadow_view,
            no_depth_view,
            extras: (0..MAX_VIEWPORTS).map(|_| Extras::default()).collect(),
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
        let mut shadow_vps = [Mat4::IDENTITY.to_cols_array_2d(); MAX_SHADOW_LAYERS];
        let mut light_shadow = [[-1.0, 0.0, 0.0, 0.0]; MAX_VIEW_LIGHTS];
        let mut shadow = [0.0; 4];
        if let Some(s) = shading.shadow {
            for (d, m) in shadow_vps.iter_mut().zip(&s.layers) {
                *d = m.to_cols_array_2d();
            }
            for (d, (start, n)) in light_shadow.iter_mut().zip(s.lights) {
                if n > 0 {
                    *d = [start as f32, n as f32, 0.0, 0.0];
                }
            }
            shadow = [1.0, if s.soft { 1.0 } else { 0.0 }, 0.0002, s.intensity];
        }
        let ao = shading.ao.map_or([0.0; 4], |(r, i, _)| [1.0, r, i, 0.0]);
        let u = ViewUniform {
            inv_view_proj: shading.ao.map_or(view_proj, |a| a.2).inverse().to_cols_array_2d(),
            ao,
            viewport_px: shading.viewport_px,
            shadow_vps,
            light_shadow,
            shadow,
            view_proj: view_proj.to_cols_array_2d(),
            camera_pos: [camera_pos.x, camera_pos.y, camera_pos.z, 0.0],
            params: [shading.lighting as f32, count as f32, shading.flags as f32, 0.0],
            override_color: oc,
            ambient: [a[0], a[1], a[2], 0.0],
            lights,
        };
        let slot = slot.min(MAX_VIEWPORTS - 1);
        queue.write_buffer(&self.slots[slot].0, 0, bytemuck::bytes_of(&u));
    }

    /// Draws the shadow maps of `slot` (call before the viewport's pass;
    /// the maps stay for its shaded pass).
    pub fn render_shadows(&mut self, device: &Device, queue: &Queue, encoder: &mut CommandEncoder, slot: usize, meshes: &[&MeshBuffer], set: &ShadowSet) {
        let slot = slot.min(MAX_VIEWPORTS - 1);
        if self.extras[slot].shadow.is_none() {
            let texture = device.create_texture(&TextureDescriptor {
                label: Some("shadow_maps"),
                size: Extent3d { width: SHADOW_MAP_SIZE, height: SHADOW_MAP_SIZE, depth_or_array_layers: MAX_SHADOW_LAYERS as u32 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: TextureFormat::Depth32Float,
                usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let layers = (0..MAX_SHADOW_LAYERS as u32)
                .map(|layer| {
                    let view = texture.create_view(&TextureViewDescriptor { dimension: Some(TextureViewDimension::D2), base_array_layer: layer, array_layer_count: Some(1), ..Default::default() });
                    let uniform = device.create_buffer(&BufferDescriptor {
                        label: Some("shadow_view_uniform"),
                        size: std::mem::size_of::<ViewUniform>() as u64,
                        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    let group = device.create_bind_group(&BindGroupDescriptor {
                        label: Some("shadow_view"),
                        layout: &self.view_layout,
                        entries: &[BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }],
                    });
                    (view, uniform, group)
                })
                .collect();
            let all = texture.create_view(&TextureViewDescriptor { dimension: Some(TextureViewDimension::D2Array), ..Default::default() });
            self.extras[slot].shadow = Some(ShadowSlot { _texture: texture, layers, all });
        }
        let s = self.extras[slot].shadow.as_ref().expect("made above");
        for (m, (view, uniform, group)) in set.layers.iter().zip(&s.layers).take(set.layer_count as usize) {
            let mut u = ViewUniform::zeroed();
            u.view_proj = m.to_cols_array_2d();
            queue.write_buffer(uniform, 0, bytemuck::bytes_of(&u));
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("shadow_pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                    view,
                    depth_ops: Some(Operations { load: LoadOp::Clear(1.0), store: StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.shadow_pipeline);
            pass.set_bind_group(0, group, &[]);
            for mb in meshes.iter().filter(|mb| mb.index_count > 0) {
                for r in mb.ranges.iter().filter(|r| r.count > 0 && !r.transparent) {
                    pass.set_vertex_buffer(0, mb.vertex_buffer.slice(..));
                    pass.set_index_buffer(mb.index_buffer.slice(..), IndexFormat::Uint32);
                    pass.draw_indexed(r.start..r.start + r.count, 0, 0..1);
                }
            }
        }
    }

    /// Draws the view's own depth for ambient occlusion (`size`: its pixels).
    #[allow(clippy::too_many_arguments)]
    pub fn render_ao_depth(&mut self, device: &Device, queue: &Queue, encoder: &mut CommandEncoder, slot: usize, meshes: &[&MeshBuffer], view_proj: Mat4, size: [u32; 2]) {
        let slot = slot.min(MAX_VIEWPORTS - 1);
        let size = [size[0].clamp(1, 8192), size[1].clamp(1, 8192)];
        if self.extras[slot].ao.as_ref().is_none_or(|a| a.size != size) {
            let texture = device.create_texture(&TextureDescriptor {
                label: Some("ao_depth"),
                size: Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: TextureFormat::Depth32Float,
                usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&TextureViewDescriptor::default());
            let uniform = device.create_buffer(&BufferDescriptor {
                label: Some("ao_view_uniform"),
                size: std::mem::size_of::<ViewUniform>() as u64,
                usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let group = device.create_bind_group(&BindGroupDescriptor {
                label: Some("ao_view"),
                layout: &self.view_layout,
                entries: &[BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }],
            });
            self.extras[slot].ao = Some(AoSlot { _texture: texture, view, size, uniform, group });
        }
        let a = self.extras[slot].ao.as_ref().expect("made above");
        let mut u = ViewUniform::zeroed();
        u.view_proj = view_proj.to_cols_array_2d();
        queue.write_buffer(&a.uniform, 0, bytemuck::bytes_of(&u));
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("ao_depth_pass"),
            color_attachments: &[],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &a.view,
                depth_ops: Some(Operations { load: LoadOp::Clear(1.0), store: StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.depth_pipeline);
        pass.set_bind_group(0, &a.group, &[]);
        for mb in meshes.iter().filter(|mb| mb.index_count > 0) {
            for r in mb.ranges.iter().filter(|r| r.count > 0 && !r.transparent) {
                pass.set_vertex_buffer(0, mb.vertex_buffer.slice(..));
                pass.set_index_buffer(mb.index_buffer.slice(..), IndexFormat::Uint32);
                pass.draw_indexed(r.start..r.start + r.count, 0, 0..1);
            }
        }
    }

    /// Builds the group the shaded pass of `slot` samples its shadow maps and
    /// AO depth through (1×1 stand-ins where it has none).
    pub fn bind_extras(&mut self, device: &Device, slot: usize) {
        let slot = slot.min(MAX_VIEWPORTS - 1);
        let e = &self.extras[slot];
        let shadow = e.shadow.as_ref().map_or(&self.no_shadow_view, |s| &s.all);
        let depth = e.ao.as_ref().map_or(&self.no_depth_view, |a| &a.view);
        let group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("view_extras"),
            layout: &self.shadow_layout,
            entries: &[
                BindGroupEntry { binding: 0, resource: BindingResource::TextureView(shadow) },
                BindGroupEntry { binding: 1, resource: BindingResource::Sampler(&self.shadow_sampler) },
                BindGroupEntry { binding: 2, resource: BindingResource::TextureView(depth) },
            ],
        });
        self.extras[slot].group = Some(group);
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
        flags: u32,
        shadowed: bool,
    ) {
        let bind_group = &self.slots[slot.min(MAX_VIEWPORTS - 1)].1;
        if let Some(plane) = grid {
            self.grid.record(pass, bind_group, plane, orthographic);
        }
        let shade = shaded_meshes(meshes.len(), shaded, shade_only);
        let shadow_group = self.extras[slot.min(MAX_VIEWPORTS - 1)].group.as_ref().filter(|_| shadowed).unwrap_or(&self.no_shadow);
        if !shade.is_empty() {
            // Opaque faces first (per-surface cull setting), then see-through;
            // one draw per range (texture / transparency / culling).
            // Transparency None / Simple: see-through faces draw with the opaque ones.
            let blend = flags & (view_flags::OPAQUE | view_flags::SCREEN_DOOR) == 0;
            for transparent in [false, true] {
                for mb in shade.iter().map(|i| meshes[*i]).filter(|mb| mb.index_count > 0) {
                    for r in mb.ranges.iter().filter(|r| r.count > 0 && r.transparent == transparent) {
                        let pipeline = if transparent && blend {
                            &self.mesh_pipeline_blend
                        } else if r.cull && flags & view_flags::TWO_SIDED == 0 {
                            &self.mesh_pipeline_cull
                        } else {
                            &self.mesh_pipeline
                        };
                        pass.set_pipeline(pipeline);
                        pass.set_bind_group(0, bind_group, &[]);
                        pass.set_bind_group(1, textures.bind_group(r.texture), &[]);
                        pass.set_bind_group(2, shadow_group, &[]);
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
            for &idx in selected {
                if let Some(mb) = meshes.get(idx).filter(|mb| mb.edge_vertex_count > 0) {
                    pass.set_vertex_buffer(0, mb.selected_edge_vertex_buffer.slice(..));
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
mod shadow_tests {
    use super::*;

    fn light(kind: ViewLightKind) -> ViewLight {
        ViewLight { kind, position: Vec3::new(0.0, 0.0, 100.0), direction: -Vec3::Z, color: Vec3::ONE, ..Default::default() }
    }

    /// The scene bounds land inside the light's clip space, nearer points at a smaller depth.
    #[test]
    fn spot_and_directional_lights_frame_the_scene_and_point_lights_cast_none() {
        let bounds = (Vec3::new(-10.0, -10.0, 0.0), Vec3::new(10.0, 10.0, 20.0));
        for kind in [ViewLightKind::Directional, ViewLightKind::Spot { hotspot: 40.0, falloff: 60.0 }] {
            let m = shadow_view_proj(&light(kind), bounds).expect("a shadow map");
            let top = m.project_point3(Vec3::new(5.0, 5.0, 20.0));
            let bottom = m.project_point3(Vec3::new(5.0, 5.0, 0.0));
            for p in [top, bottom] {
                assert!(p.x.abs() <= 1.0 && p.y.abs() <= 1.0 && (0.0..=1.0).contains(&p.z), "{kind:?}: {p}");
            }
            assert!(top.z < bottom.z, "{kind:?}: the top is nearer the light");
        }
        assert!(shadow_view_proj(&light(ViewLightKind::Point), bounds).is_none());
    }

    /// A point light takes six layers (a cube), the others one each; the
    /// cube faces see the points along their axes.
    #[test]
    fn a_shadow_set_gives_point_lights_a_cube() {
        let bounds = (Vec3::new(-10.0, -10.0, 0.0), Vec3::new(10.0, 10.0, 20.0));
        let mut lights = [None; MAX_VIEW_LIGHTS];
        lights[0] = Some(light(ViewLightKind::Point));
        lights[1] = Some(light(ViewLightKind::Directional));
        let set = shadow_set(&lights, bounds, true, 0.5).expect("shadows");
        assert_eq!((set.layer_count, set.lights[0], set.lights[1]), (7, (0, 6), (6, 1)));
        // -Z face (index 5) sees the scene below the light.
        let p = set.layers[5].project_point3(Vec3::new(0.0, 0.0, 10.0));
        assert!(p.x.abs() < 1e-3 && p.y.abs() < 1e-3 && (0.0..=1.0).contains(&p.z), "{p}");
        // Two point lights fill 12 layers; a third no longer fits.
        let lights = [Some(light(ViewLightKind::Point)); MAX_VIEW_LIGHTS];
        let set = shadow_set(&lights, bounds, false, 1.0).unwrap();
        assert_eq!((set.layer_count, set.lights[2]), (12, (0, 0)));
    }
}
