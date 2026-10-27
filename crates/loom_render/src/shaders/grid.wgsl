struct ViewUniform {
    view_proj: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> view: ViewUniform;

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) color:    vec3<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec3<f32>,
};

fn line_vertex(in: VsIn) -> VsOut {
    var o: VsOut;
    o.clip = view.view_proj * vec4<f32>(in.position, 1.0);
    o.color = in.color;
    return o;
}

@vertex
fn vs(in: VsIn) -> VsOut {
    return line_vertex(in);
}

@vertex
fn vs_selected(in: VsIn) -> VsOut {
    var out = line_vertex(in);
    out.color = vec3<f32>(1.0);
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
