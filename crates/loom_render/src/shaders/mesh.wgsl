struct ViewUniform {
    view_proj: mat4x4<f32>,
    /// Camera position in world space (xyz). w is unused.
    camera_pos: vec4<f32>,
    /// x = lighting mode: 0 headlight, 1 flat, 2 fixed key light.
    params: vec4<f32>,
    /// Hidden Line colour override when a > 0.
    override_color: vec4<f32>,
};
@group(0) @binding(0) var<uniform> view: ViewUniform;
@group(1) @binding(0) var map_tex: texture_2d<f32>;
@group(1) @binding(1) var map_samp: sampler;

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) normal:   vec3<f32>,
    @location(2) uv:       vec2<f32>,
    @location(3) color:    vec4<f32>,
    @location(4) spec:     vec4<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) world_pos:    vec3<f32>,
    @location(2) uv:           vec2<f32>,
    @location(3) color:        vec4<f32>,
    @location(4) spec:         vec4<f32>,
};

@vertex
fn vs(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = view.view_proj * vec4<f32>(in.position, 1.0);
    out.world_normal = in.normal;
    out.world_pos = in.position;
    out.uv = in.uv;
    out.color = in.color;
    out.spec = in.spec;
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    // Object colour, replaced by the map (checker / bitmap) where it is opaque.
    let t = textureSample(map_tex, map_samp, vec2<f32>(in.uv.x, 1.0 - in.uv.y));
    var base = mix(in.color.rgb, t.rgb, t.a);
    if (view.override_color.a > 0.0) {
        base = view.override_color.rgb;
    } else if (in.spec.a < 0.0) {
        // Unlit colours (vertex colours without Shaded).
        return vec4<f32>(base, in.color.a);
    }

    let n = normalize(in.world_normal);
    let mode = view.params.x;
    let to_cam = normalize(view.camera_pos.xyz - in.world_pos);
    var diff = 1.0;
    var light = to_cam;
    if (mode < 0.5) {
        // Headlight: light from the camera; abs() keeps two-sided faces lit.
        diff = clamp(abs(dot(n, to_cam)), 0.0, 1.0);
    } else if (mode > 1.5) {
        // Scene light stand-in: a fixed key light from above-left-front.
        light = normalize(vec3<f32>(-0.4, -0.6, 0.7));
        diff = clamp(abs(dot(n, light)), 0.0, 1.0);
    }
    let ambient = select(0.42, 1.0, mode > 0.5 && mode < 1.5);
    var color = base * (ambient + (1.0 - ambient) * diff);
    // Blinn highlight of the material (none in Flat or without a material).
    if (in.spec.a > 0.0 && !(mode > 0.5 && mode < 1.5)) {
        let h = normalize(light + to_cam);
        color = color + in.spec.rgb * pow(clamp(abs(dot(n, h)), 0.0, 1.0), in.spec.a);
    }
    return vec4<f32>(color, in.color.a);
}
