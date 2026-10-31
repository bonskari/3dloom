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
    selected_pipeline: RenderPipeline,
    /// Grids for the XY, XZ and YZ construction planes (perspective extent).
    planes: [(Buffer, u32); 3],
    /// The same planes at the larger axis-view extent.
    ortho_planes: [(Buffer, u32); 3],
    params: GridParams,
}

impl GridPipeline {
    /// Build a grid spanning -N..N on the XY plane (Z up), 1-unit cells.
    /// Centre lines black, the others gray (see `grid_vertices`).
    pub fn new(
        device: &Device,
        target_format: TextureFormat,
        layout: &BindGroupLayout,
        n: i32,
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
        let make_pipeline = |entry| device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("grid_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some(entry),
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
            multisample: MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let pipeline = make_pipeline("vs");
        let selected_pipeline = make_pipeline("vs_selected");

        let params = GridParams { spacing: 1.0, major_every: 10, cells: n, ortho_cells: n, colors: GRID_COLORS };
        let (planes, ortho_planes) = build_planes(device, params);
        Self { pipeline, selected_pipeline, planes, ortho_planes, params }
    }

    /// Rebuilds the line buffers when the Home Grid settings change.
    pub fn set_params(&mut self, device: &Device, params: GridParams) {
        if params == self.params {
            return;
        }
        let (planes, ortho_planes) = build_planes(device, params);
        self.planes = planes;
        self.ortho_planes = ortho_planes;
        self.params = params;
    }

    pub fn record(&self, pass: &mut RenderPass<'_>, view_bind_group: &BindGroup, plane: GridPlane, orthographic: bool) {
        let set = if orthographic { &self.ortho_planes } else { &self.planes };
        let (buffer, count) = &set[plane as usize];
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, view_bind_group, &[]);
        pass.set_vertex_buffer(0, buffer.slice(..));
        pass.draw(0..*count, 0..1);
    }

    /// Selection reuses the ordinary edge positions with constant white colour.
    pub fn selected_line_pipeline(&self) -> &RenderPipeline {
        &self.selected_pipeline
    }

    /// The normal line pipeline preserves each vertex colour.
    pub fn line_pipeline(&self) -> &RenderPipeline {
        &self.pipeline
    }
}

/// Home Grid settings the line buffers are built from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridParams {
    pub spacing: f32,
    /// Major Lines every Nth Grid Line.
    pub major_every: u32,
    /// Perspective View Grid Extent, in cells.
    pub cells: i32,
    /// Axis views show a larger grid (drawn without bound).
    pub ortho_cells: i32,
    /// Line colours (Customize User Interface > Colors): grid lines, major lines, world axes.
    pub colors: [[f32; 3]; 3],
}

/// The grid's own colours: gray lines, brighter major lines, black axes.
pub const GRID_COLORS: [[f32; 3]; 3] = [[0.38, 0.38, 0.38], [0.55, 0.55, 0.55], [0.0, 0.0, 0.0]];

type PlaneBuffers = [(Buffer, u32); 3];

fn build_planes(device: &Device, p: GridParams) -> (PlaneBuffers, PlaneBuffers) {
    let build = |cells: i32| {
        [GridPlane::XY, GridPlane::XZ, GridPlane::YZ].map(|plane| {
            let verts = grid_vertices(cells, plane, p.spacing, p.major_every, p.colors);
            let buffer = device.create_buffer_init(&BufferInitDescriptor {
                label: Some("grid_vertex_buffer"),
                contents: bytemuck::cast_slice(&verts),
                usage: BufferUsages::VERTEX,
            });
            (buffer, verts.len() as u32)
        })
    };
    (build(p.cells), build(p.ortho_cells))
}

/// Grid lines spanning -n..n cells of `spacing` on the given plane. The
/// two centre lines are black, every `major_every`th line brighter, the
/// rest gray; there is no Z axis line.
fn grid_vertices(n: i32, plane: GridPlane, spacing: f32, major_every: u32, colors: [[f32; 3]; 3]) -> Vec<LineVertex> {
    let [gray, major, center] = colors;
    let major_every = major_every.max(1) as i32;
    // (u axis index, v axis index)
    let (u, v) = match plane {
        GridPlane::XY => (0, 1),
        GridPlane::XZ => (0, 2),
        GridPlane::YZ => (1, 2),
    };
    let at = |a: usize, va: f32, b: usize, vb: f32| -> [f32; 3] {
        let mut p = [0.0; 3];
        p[a] = va;
        p[b] = vb;
        p
    };
    let mut verts = Vec::new();
    let ext = n as f32 * spacing;
    for i in -n..=n {
        let f = i as f32 * spacing;
        let line = if i % major_every == 0 { major } else { gray };
        // Lines along u (varying v): the v==0 line is the u axis.
        let color = if i == 0 { center } else { line };
        verts.push(LineVertex { position: at(u, -ext, v, f), color });
        verts.push(LineVertex { position: at(u, ext, v, f), color });
        verts.push(LineVertex { position: at(v, -ext, u, f), color });
        verts.push(LineVertex { position: at(v, ext, u, f), color });
    }
    verts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centre_lines_are_black_and_there_is_no_z_line() {
        let v = grid_vertices(2, GridPlane::XY, 1.0, 10, GRID_COLORS);
        // 5 lines each way, 2 vertices per line, nothing else.
        assert_eq!(v.len(), 5 * 2 * 2);
        let black: Vec<_> = v.iter().filter(|l| l.color == [0.0; 3]).collect();
        assert_eq!(black.len(), 4, "one black line along X and one along Y");
        assert!(black.iter().all(|l| l.position[2] == 0.0));
    }
}
