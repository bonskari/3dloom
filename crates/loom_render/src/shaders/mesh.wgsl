struct Light {
    /// xyz position, w kind (0 point, 1 spot, 2 directional).
    pos_kind: vec4<f32>,
    /// xyz direction the light travels, w cos(hotspot / 2).
    dir_hot: vec4<f32>,
    /// rgb colour times multiplier, a cos(falloff / 2).
    color_fall: vec4<f32>,
    /// near start, near end, far start, far end.
    atten: vec4<f32>,
    /// decay exponent, decay start, near used, far used.
    decay: vec4<f32>,
};

struct ViewUniform {
    view_proj: mat4x4<f32>,
    /// Camera position in world space (xyz). w is unused.
    camera_pos: vec4<f32>,
    /// x = lighting mode: 0 headlight, 1 flat, 2 the lights; y = light count.
    params: vec4<f32>,
    /// Hidden Line colour override when a > 0.
    override_color: vec4<f32>,
    ambient: vec4<f32>,
    lights: array<Light, 8>,
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
    @location(5) extra:    vec4<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) world_pos:    vec3<f32>,
    @location(2) uv:           vec2<f32>,
    @location(3) color:        vec4<f32>,
    @location(4) spec:         vec4<f32>,
    @location(5) extra:        vec4<f32>,
};

fn ramp(x: f32, s: f32, e: f32) -> f32 {
    if (e <= s) {
        return select(0.0, 1.0, x >= e);
    }
    return clamp((x - s) / (e - s), 0.0, 1.0);
}

/// The fraction of light `l` reaching a point `dist` away at `cos_axis` to
/// its direction: decay, near / far attenuation and the spot cone.
fn falloff(l: Light, dist: f32, cos_axis: f32) -> f32 {
    var k = 1.0;
    let kind = l.pos_kind.w;
    if (kind < 1.5) {
        let r0 = max(l.decay.y, 1e-4);
        if (l.decay.x > 0.5 && dist > r0) {
            k = k * pow(r0 / dist, l.decay.x);
        }
    }
    if (l.decay.z > 0.5) {
        k = k * ramp(dist, l.atten.x, l.atten.y);
    }
    if (l.decay.w > 0.5) {
        k = k * (1.0 - ramp(dist, l.atten.z, l.atten.w));
    }
    if (kind > 0.5 && kind < 1.5) {
        k = k * smoothstep(l.color_fall.a, l.dir_hot.w, cos_axis);
    }
    return k;
}

@vertex
fn vs(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = view.view_proj * vec4<f32>(in.position, 1.0);
    out.world_normal = in.normal;
    out.world_pos = in.position;
    out.uv = in.uv;
    out.color = in.color;
    out.spec = in.spec;
    out.extra = in.extra;
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    // Object colour, replaced by the map (checker / bitmap) where it is
    // opaque, by the map's Amount; a decal axis (Tile off) leaves the colour
    // outside 0-1.
    let t = textureSample(map_tex, map_samp, vec2<f32>(in.uv.x, 1.0 - in.uv.y));
    var a = t.a * in.extra.y;
    if ((in.extra.z > 0.5 && (in.uv.x < 0.0 || in.uv.x > 1.0)) || (in.extra.w > 0.5 && (in.uv.y < 0.0 || in.uv.y > 1.0))) {
        a = 0.0;
    }
    var base = mix(in.color.rgb, t.rgb, a);
    if (view.override_color.a > 0.0) {
        base = view.override_color.rgb;
    }

    let n = normalize(in.world_normal);
    let mode = view.params.x;
    let to_cam = normalize(view.camera_pos.xyz - in.world_pos);
    var diff = 1.0;
    var light = to_cam;
    if (mode < 0.5) {
        // Headlight: light from the camera; abs() keeps two-sided faces lit.
        diff = clamp(abs(dot(n, to_cam)), 0.0, 1.0);
    }
    if (mode > 1.5) {
        // The scene's lights (or the default lights), two-sided like the headlight.
        var lit = view.ambient.rgb;
        var spec = vec3<f32>(0.0);
        let count = u32(view.params.y);
        for (var i = 0u; i < count; i = i + 1u) {
            let l = view.lights[i];
            var to_light = -l.dir_hot.xyz;
            var dist = 0.0;
            if (l.pos_kind.w < 1.5) {
                let d = l.pos_kind.xyz - in.world_pos;
                dist = length(d);
                to_light = d / max(dist, 1e-6);
            }
            let k = falloff(l, dist, dot(-to_light, l.dir_hot.xyz));
            lit = lit + l.color_fall.rgb * k * clamp(abs(dot(n, to_light)), 0.0, 1.0);
            if (in.spec.a > 0.0) {
                let h = normalize(to_light + to_cam);
                spec = spec + l.color_fall.rgb * k * pow(clamp(abs(dot(n, h)), 0.0, 1.0), in.spec.a);
            }
        }
        // Self-Illumination: the colour shows unlit by that much.
        return vec4<f32>(mix(base * lit, base, in.extra.x) + in.spec.rgb * spec, in.color.a);
    }
    let ambient = select(0.42, 1.0, mode > 0.5 && mode < 1.5);
    // Self-Illumination: the colour shows unlit by that much.
    var color = mix(base * (ambient + (1.0 - ambient) * diff), base, in.extra.x);
    // Blinn highlight of the material (none in Flat or without a material).
    if (in.spec.a > 0.0 && !(mode > 0.5 && mode < 1.5)) {
        let h = normalize(light + to_cam);
        color = color + in.spec.rgb * pow(clamp(abs(dot(n, h)), 0.0, 1.0), in.spec.a);
    }
    return vec4<f32>(color, in.color.a);
}
