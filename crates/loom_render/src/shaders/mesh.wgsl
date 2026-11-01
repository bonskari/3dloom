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
    /// Each shadow map layer's view-projection.
    shadow_vps: array<mat4x4<f32>, 16>,
    /// Per light: x first layer (-1 none), y layer count (6: a point light's cube).
    light_shadow: array<vec4<f32>, 8>,
    /// x shadows on, y soft, z depth bias, w intensity.
    shadow: vec4<f32>,
    inv_view_proj: mat4x4<f32>,
    /// x AO on, y radius (world units), z intensity.
    ao: vec4<f32>,
    /// The view's rectangle in framebuffer pixels: x, y, width, height.
    viewport_px: vec4<f32>,
};
@group(0) @binding(0) var<uniform> view: ViewUniform;
@group(1) @binding(0) var map_tex: texture_2d<f32>;
@group(1) @binding(1) var map_samp: sampler;
@group(2) @binding(0) var shadow_map: texture_depth_2d_array;
@group(2) @binding(1) var shadow_samp: sampler_comparison;
@group(2) @binding(2) var ao_depth: texture_depth_2d;

fn screen_of(p: vec3<f32>) -> vec2<f32> {
    let c = view.view_proj * vec4<f32>(p, 1.0);
    let ndc = c.xy / max(c.w, 1e-6);
    return vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5) * view.viewport_px.zw;
}

/// Ambient occlusion at `p` (normal `n`, framebuffer pixel `px`): how open
/// the hemisphere above it is (1 open), from the view's depth around it.
fn ambient_occlusion(p: vec3<f32>, n: vec3<f32>, px: vec2<f32>) -> f32 {
    let radius = view.ao.y;
    let local = px - view.viewport_px.xy;
    // The radius on screen: the longest of the world axes projected.
    let o = screen_of(p);
    let r_px = clamp(max(max(length(screen_of(p + vec3<f32>(radius, 0.0, 0.0)) - o), length(screen_of(p + vec3<f32>(0.0, radius, 0.0)) - o)), length(screen_of(p + vec3<f32>(0.0, 0.0, radius)) - o)), 2.0, 96.0);
    let size = vec2<i32>(textureDimensions(ao_depth));
    var occ = 0.0;
    let count = 16;
    for (var k = 0; k < count; k = k + 1) {
        // A spiral of samples out to the radius, turned per pixel.
        let t = (f32(k) + 0.5) / f32(count);
        let a = f32(k) * 2.39996 + fract(sin(dot(px, vec2<f32>(12.9898, 78.233))) * 43758.5453) * 6.2831853;
        let q = local + vec2<f32>(cos(a), sin(a)) * r_px * t;
        let qi = vec2<i32>(q);
        if (qi.x < 0 || qi.y < 0 || qi.x >= size.x || qi.y >= size.y) {
            continue;
        }
        let d = textureLoad(ao_depth, qi, 0);
        if (d >= 1.0) {
            continue;
        }
        // The depth is the sampled texel's, at its centre.
        let qc = vec2<f32>(qi) + vec2<f32>(0.5);
        let ndc = vec2<f32>(qc.x / view.viewport_px.z * 2.0 - 1.0, 1.0 - qc.y / view.viewport_px.w * 2.0);
        let w = view.inv_view_proj * vec4<f32>(ndc, d, 1.0);
        let s = w.xyz / w.w;
        let v = s - p;
        let dist = length(v);
        if (dist < radius * 0.05 || dist > radius) {
            continue;
        }
        occ = occ + max(dot(n, v / dist) - 0.25, 0.0) / 0.75 * (1.0 - dist / radius);
    }
    return clamp(1.0 - view.ao.z * 4.0 * occ / f32(count), 0.0, 1.0);
}

/// How much of light `i` reaches `p` (1 lit, 0 in shadow): its layer (a
/// point light's cube face by the major axis from the light), 3×3 PCF when soft.
fn shadow_factor(i: u32, p: vec3<f32>) -> f32 {
    let info = view.light_shadow[i];
    if (info.x < 0.0) {
        return 1.0;
    }
    var layer = i32(info.x);
    if (info.y > 1.5) {
        let d = p - view.lights[i].pos_kind.xyz;
        let a = abs(d);
        var face = 0;
        if (a.x >= a.y && a.x >= a.z) {
            face = select(1, 0, d.x >= 0.0);
        } else if (a.y >= a.z) {
            face = select(3, 2, d.y >= 0.0);
        } else {
            face = select(5, 4, d.z >= 0.0);
        }
        layer = layer + face;
    }
    let c = view.shadow_vps[layer] * vec4<f32>(p, 1.0);
    if (c.w <= 0.0) {
        return 1.0;
    }
    let ndc = c.xyz / c.w;
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 || ndc.z > 1.0) {
        return 1.0;
    }
    let depth = ndc.z - view.shadow.z;
    var lit = 0.0;
    if (view.shadow.y > 0.5) {
        let texel = 1.0 / 1024.0;
        for (var y = -1; y <= 1; y = y + 1) {
            for (var x = -1; x <= 1; x = x + 1) {
                lit = lit + textureSampleCompareLevel(shadow_map, shadow_samp, uv + vec2<f32>(f32(x), f32(y)) * texel, layer, depth);
            }
        }
        lit = lit / 9.0;
    } else {
        lit = textureSampleCompareLevel(shadow_map, shadow_samp, uv, layer, depth);
    }
    // Intensity: how much of the light the shadow takes away.
    return 1.0 - view.shadow.w * (1.0 - lit);
}

@vertex
fn vs_depth(in: VsIn) -> @builtin(position) vec4<f32> {
    return view.view_proj * vec4<f32>(in.position, 1.0);
}

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
    // The face's own normal, for Facets (derivatives need uniform control flow: here, first).
    let face_n = normalize(cross(dpdx(in.world_pos), dpdy(in.world_pos)));
    let flags = u32(view.params.z);
    var a = t.a * in.extra.y;
    if ((flags & 16u) != 0u) {
        a = 0.0;
    }
    // Transparency None: opaque; Simple: every other pixel of a see-through face.
    var alpha = in.color.a;
    if ((flags & 4u) != 0u) {
        alpha = 1.0;
    }
    if ((flags & 8u) != 0u && alpha < 0.99) {
        let p = vec2<u32>(in.clip.xy);
        if (((p.x + p.y) & 1u) == 0u) {
            discard;
        }
        alpha = 1.0;
    }
    let highlights = (flags & 1u) == 0u;
    if ((in.extra.z > 0.5 && (in.uv.x < 0.0 || in.uv.x > 1.0)) || (in.extra.w > 0.5 && (in.uv.y < 0.0 || in.uv.y > 1.0))) {
        a = 0.0;
    }
    var base = mix(in.color.rgb, t.rgb, a);
    if (view.override_color.a > 0.0) {
        base = view.override_color.rgb;
    } else if (in.spec.a < 0.0) {
        // Unlit colours (vertex colours without Shaded).
        return vec4<f32>(base, alpha);
    }

    var n = normalize(in.world_normal);
    if ((flags & 2u) != 0u) {
        n = face_n;
    }
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
            var k = falloff(l, dist, dot(-to_light, l.dir_hot.xyz));
            if (view.shadow.x > 0.5) {
                k = k * shadow_factor(i, in.world_pos);
            }
            lit = lit + l.color_fall.rgb * k * clamp(abs(dot(n, to_light)), 0.0, 1.0);
            if (in.spec.a > 0.0 && highlights) {
                let h = normalize(to_light + to_cam);
                spec = spec + l.color_fall.rgb * k * pow(clamp(abs(dot(n, h)), 0.0, 1.0), in.spec.a);
            }
        }
        if (view.ao.x > 0.5) {
            // Facing the viewer: two-sided faces turn their normal to the camera.
            let nf = select(-n, n, dot(n, to_cam) >= 0.0);
            lit = lit * ambient_occlusion(in.world_pos, nf, in.clip.xy);
        }
        // Self-Illumination: the colour shows unlit by that much.
        return vec4<f32>(mix(base * lit, base, in.extra.x) + in.spec.rgb * spec, alpha);
    }
    let ambient = select(0.42, 1.0, mode > 0.5 && mode < 1.5);
    var shade = ambient + (1.0 - ambient) * diff;
    if (view.ao.x > 0.5) {
        let nf = select(-n, n, dot(n, to_cam) >= 0.0);
        shade = shade * ambient_occlusion(in.world_pos, nf, in.clip.xy);
    }
    // Self-Illumination: the colour shows unlit by that much.
    var color = mix(base * shade, base, in.extra.x);
    // Blinn highlight of the material (none in Flat or without a material).
    if (in.spec.a > 0.0 && highlights && !(mode > 0.5 && mode < 1.5)) {
        let h = normalize(light + to_cam);
        color = color + in.spec.rgb * pow(clamp(abs(dot(n, h)), 0.0, 1.0), in.spec.a);
    }
    return vec4<f32>(color, alpha);
}

/// Lit Wireframes: an edge's colour lit like a surface with its corner
/// normal (two-sided), by the headlight or the view's lights.
@fragment
fn fs_line_lit(in: VsOut) -> @location(0) vec4<f32> {
    let n = normalize(in.world_normal);
    let to_cam = normalize(view.camera_pos.xyz - in.world_pos);
    let base = in.color.rgb;
    if (view.params.x > 1.5) {
        var lit = view.ambient.rgb;
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
            var k = falloff(l, dist, dot(-to_light, l.dir_hot.xyz));
            if (view.shadow.x > 0.5) {
                k = k * shadow_factor(i, in.world_pos);
            }
            lit = lit + l.color_fall.rgb * k * clamp(abs(dot(n, to_light)), 0.0, 1.0);
        }
        return vec4<f32>(base * lit, 1.0);
    }
    let diff = clamp(abs(dot(n, to_cam)), 0.0, 1.0);
    return vec4<f32>(base * (0.42 + 0.58 * diff), 1.0);
}
