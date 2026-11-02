use crate::viewport::GridPlane;
use bytemuck::{Pod, Zeroable};
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    *,
};

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct LineVertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
}

pub struct GridPipeline {
    pipeline: RenderPipeline,
    /// Each viewport's grid lines and the settings they were built from
    /// (views zoom and pan on their own, so each has its own grid level).
    slots: Vec<Option<(GridParams, Buffer, u32)>>,
}

impl GridPipeline {
    /// The line pipeline; each viewport's lines come with `set_params`.
    pub fn new(
        device: &Device,
        target_format: TextureFormat,
        layout: &BindGroupLayout,
        samples: u32,
    ) -> Self {
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("grid.wgsl"),
            source: ShaderSource::Wgsl(include_str!("shaders/grid.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("grid_pipeline_layout"),
            bind_group_layouts: &[layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("grid_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[VertexBufferLayout {
                    array_stride: std::mem::size_of::<LineVertex>() as u64,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &vertex_attr_array![0 => Float32x3, 1 => Float32x3],
                }],
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format: target_format,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::LineList,
                ..Default::default()
            },
            depth_stencil: Some(DepthStencilState {
                format: TextureFormat::Depth24Plus,
                depth_write_enabled: false,
                depth_compare: CompareFunction::LessEqual,
                stencil: StencilState::default(),
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState { count: samples, ..Default::default() },
            multiview: None,
            cache: None,
        });

        Self { pipeline, slots: Vec::new() }
    }

    /// Rebuilds a viewport's lines when its grid changes (settings, level,
    /// plane or the major square the view is centred on).
    pub fn set_params(&mut self, device: &Device, slot: usize, params: GridParams) {
        if self.slots.len() <= slot {
            self.slots.resize_with(slot + 1, || None);
        }
        if self.slots[slot].as_ref().is_some_and(|(p, _, _)| *p == params) {
            return;
        }
        let verts = grid_vertices(&params);
        let buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("grid_vertex_buffer"),
            contents: bytemuck::cast_slice(&verts),
            usage: BufferUsages::VERTEX,
        });
        self.slots[slot] = Some((params, buffer, verts.len() as u32));
    }

    pub fn record(&self, pass: &mut RenderPass<'_>, view_bind_group: &BindGroup, slot: usize) {
        let Some(Some((_, buffer, count))) = self.slots.get(slot) else { return };
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, view_bind_group, &[]);
        pass.set_vertex_buffer(0, buffer.slice(..));
        pass.draw(0..*count, 0..1);
    }

    /// Exposes the line render pipeline so other line-list vertex buffers
    /// (such as selection-overlay edges) can be drawn with the same shader.
    pub fn line_pipeline(&self) -> &RenderPipeline {
        &self.pipeline
    }
}

/// One viewport's home grid: what its line buffer is built from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridParams {
    pub plane: GridPlane,
    /// Distance between lines (Grid Spacing at the view's grid level).
    pub spacing: f32,
    /// Major Lines every Nth Grid Line, counted from the origin.
    pub major_every: u32,
    /// Lines each way from `center`.
    pub cells: i32,
    /// The line the grid is centred on, in lines from the origin (u, v).
    pub center: [i32; 2],
    /// Line colours (Customize User Interface > Colors): grid lines, major lines, world axes.
    pub colors: [[f32; 3]; 3],
}

/// The grid's own colours: gray lines, brighter major lines, black axes.
pub const GRID_COLORS: [[f32; 3]; 3] = [[0.38, 0.38, 0.38], [0.55, 0.55, 0.55], [0.0, 0.0, 0.0]];

/// Grid lines `cells` each way from line `center` on the plane. The two
/// lines through the origin are black, every `major_every`th line from the
/// origin brighter, the rest gray; there is no Z axis line.
fn grid_vertices(p: &GridParams) -> Vec<LineVertex> {
    let [gray, major, center] = p.colors;
    let major_every = p.major_every.max(1) as i64;
    // (u axis index, v axis index)
    let (u, v) = match p.plane {
        GridPlane::XY => (0, 1),
        GridPlane::XZ => (0, 2),
        GridPlane::YZ => (1, 2),
    };
    let at = |a: usize, va: f32, b: usize, vb: f32| -> [f32; 3] {
        let mut q = [0.0; 3];
        q[a] = va;
        q[b] = vb;
        q
    };
    let n = p.cells.max(1) as i64;
    let s = p.spacing as f64;
    let [cu, cv] = p.center.map(|c| c as i64);
    let (u0, u1) = (((cu - n) as f64 * s) as f32, ((cu + n) as f64 * s) as f32);
    let (v0, v1) = (((cv - n) as f64 * s) as f32, ((cv + n) as f64 * s) as f32);
    let color = |i: i64| if i == 0 { center } else if i % major_every == 0 { major } else { gray };
    let mut verts = Vec::with_capacity((4 * (2 * n + 1)) as usize);
    for k in -n..=n {
        // Lines along u at v = (cv + k) * s: the v == 0 line is the u axis.
        let (iv, iu) = (cv + k, cu + k);
        verts.push(LineVertex { position: at(u, u0, v, (iv as f64 * s) as f32), color: color(iv) });
        verts.push(LineVertex { position: at(u, u1, v, (iv as f64 * s) as f32), color: color(iv) });
        verts.push(LineVertex { position: at(v, v0, u, (iu as f64 * s) as f32), color: color(iu) });
        verts.push(LineVertex { position: at(v, v1, u, (iu as f64 * s) as f32), color: color(iu) });
    }
    verts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centre_lines_are_black_and_there_is_no_z_line() {
        let v = grid_vertices(&GridParams { plane: GridPlane::XY, spacing: 1.0, major_every: 10, cells: 2, center: [0, 0], colors: GRID_COLORS });
        // 5 lines each way, 2 vertices per line, nothing else.
        assert_eq!(v.len(), 5 * 2 * 2);
        let black: Vec<_> = v.iter().filter(|l| l.color == [0.0; 3]).collect();
        assert_eq!(black.len(), 4, "one black line along X and one along Y");
        assert!(black.iter().all(|l| l.position[2] == 0.0));
    }

    #[test]
    fn an_off_centre_grid_keeps_its_axes_and_major_lines_where_the_origin_puts_them() {
        let p = GridParams { plane: GridPlane::XY, spacing: 10.0, major_every: 10, cells: 15, center: [10, 0], colors: GRID_COLORS };
        let v = grid_vertices(&p);
        let x_of = |c: [f32; 3]| v.chunks(2).filter(|l| l[0].position[0] == l[1].position[0] && l[0].color == c).map(|l| l[0].position[0]).collect::<Vec<_>>();
        assert_eq!(x_of(GRID_COLORS[2]), vec![0.0], "the Y axis line stays at x = 0");
        assert_eq!(x_of(GRID_COLORS[1]), vec![100.0, 200.0], "major lines every 100 units from the origin");
        let xs: Vec<f32> = v.iter().map(|l| l.position[0]).collect();
        assert_eq!(xs.iter().cloned().fold(f32::MAX, f32::min), -50.0);
        assert_eq!(xs.iter().cloned().fold(f32::MIN, f32::max), 250.0);
    }
}
