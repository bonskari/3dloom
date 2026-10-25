// The environment pass: after the main pass it draws the environment
// background (colour or map) behind the scene, the atmospheres (standard,
// layered and volume fog) over it in list order, and the Depth of Field
// blur (two passes: horizontal, then vertical).
//
// Colours are premultiplied: the main pass clears to transparent black, so
// an edge pixel is `coverage * colour`; the background fills what the
// geometry leaves uncovered and keeps alpha 0, as the reference's renders do.

#import bevy_render::view::View
#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

struct EnvPass {
    // rgb linear; w: 1 = draw the background.
    background: vec4<f32>,
    // x: 1 = a map; y: mapping (0 screen, 1 spherical, 2 cylindrical, 3 shrink-wrap).
    map: vec4<f32>,
    // The part of the frame this view shows: u0, v0, u1, v1.
    frame: vec4<f32>,
    // Per atmosphere: rgb linear colour, w: kind (0 none, 1 standard, 2 layered, 3 volume).
    fog_color: array<vec4<f32>, 4>,
    // Standard: near distance, far distance, near density, far density.
    // Layered: bottom, top, extinction, falloff (0 none, 1 top, 2 bottom).
    // Volume: extinction.
    fog_a: array<vec4<f32>, 4>,
    // x: fog background, y: exponential (standard).
    fog_b: array<vec4<f32>, 4>,
    // x: 1 = depth of field; y: focal distance; z: focal range; w: focal limit.
    dof: vec4<f32>,
    // x: horizontal, y: vertical focal loss in pixels.
    dof_loss: vec4<f32>,
    // x: exposure multiplier, y: contrast, z: 1 = grade here.
    grade: vec4<f32>,
    // Volume Light (omni / spot): position + kind (0 none, 1 omni, 2 spot),
    // direction + cos(falloff / 2), in-scattering colour + cos(hotspot / 2),
    // x: range.
    vlight_pos: array<vec4<f32>, 4>,
    vlight_dir: array<vec4<f32>, 4>,
    vlight_color: array<vec4<f32>, 4>,
    vlight_range: array<vec4<f32>, 4>,
}

@group(0) @binding(0) var<uniform> view: View;
#ifdef MULTISAMPLED
@group(0) @binding(1) var depth_texture: texture_depth_multisampled_2d;
#else
@group(0) @binding(1) var depth_texture: texture_depth_2d;
#endif
@group(0) @binding(2) var color_texture: texture_2d<f32>;
@group(0) @binding(3) var<uniform> env: EnvPass;
@group(0) @binding(4) var map_texture: texture_2d<f32>;
@group(0) @binding(5) var map_sampler: sampler;

const PI: f32 = 3.14159265358979;

fn depth_at(p: vec2<i32>) -> f32 {
    return textureLoad(depth_texture, p, 0);
}

fn world_at(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let ndc = vec2(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let w = view.world_from_clip * vec4(ndc, depth, 1.0);
    return w.xyz / w.w;
}

// Distance along the view direction (environment ranges and focus are depths).
fn view_depth(p: vec3<f32>) -> f32 {
    let v = view.view_from_world * vec4(p, 1.0);
    return -v.z;
}

fn background_color(uv: vec2<f32>, dir: vec3<f32>) -> vec3<f32> {
    if env.map.x < 0.5 {
        return env.background.rgb;
    }
    var muv: vec2<f32>;
    let mapping = i32(env.map.y + 0.5);
    if mapping == 0 {
        muv = mix(env.frame.xy, env.frame.zw, uv);
    } else if mapping == 1 {
        // Spherical: longitude around world Z, latitude from the horizon.
        muv = vec2(0.5 - atan2(dir.y, dir.x) / (2.0 * PI), 0.5 - asin(clamp(dir.z, -1.0, 1.0)) / PI);
    } else if mapping == 2 {
        // Cylindrical: around world Z, the height over the unit cylinder.
        let h = dir.z / max(length(dir.xy), 1e-4);
        muv = vec2(0.5 - atan2(dir.y, dir.x) / (2.0 * PI), 0.5 - clamp(h, -1.0, 1.0) * 0.5);
    } else {
        // Shrink-wrap: the map's centre at the zenith, its edge gathered at the nadir.
        let a = acos(clamp(dir.z, -1.0, 1.0)) / PI;
        let az = atan2(dir.y, dir.x);
        muv = vec2(0.5 + 0.5 * a * cos(az), 0.5 - 0.5 * a * sin(az));
    }
    let c = textureSampleLevel(map_texture, map_sampler, muv, 0.0);
    return c.rgb;
}

// Density of a layered fog at height z (0 outside the layer).
fn layer_density(i: i32, z: f32) -> f32 {
    let a = env.fog_a[i];
    if z < a.x || z > a.y {
        return 0.0;
    }
    let t = (z - a.x) / max(a.y - a.x, 1e-4);
    let falloff = i32(a.w + 0.5);
    let k = 3.0;
    let e = exp(-k);
    if falloff == 1 {
        return a.z * (exp(-k * t) - e) / (1.0 - e);
    } else if falloff == 2 {
        return a.z * (exp(-k * (1.0 - t)) - e) / (1.0 - e);
    }
    return a.z;
}

// Opacity of atmosphere i along the ray o + t d, t in [0, len] (len < 0: to
// infinity, the background), with `depth` the hit point's view depth.
fn fog_amount(i: i32, o: vec3<f32>, d: vec3<f32>, len: f32, depth: f32) -> f32 {
    let kind = i32(env.fog_color[i].w + 0.5);
    let a = env.fog_a[i];
    let b = env.fog_b[i];
    let background = len < 0.0;
    if background && b.x < 0.5 {
        return 0.0;
    }
    if kind == 1 {
        if background {
            return a.w;
        }
        var t = clamp((depth - a.x) / max(a.y - a.x, 1e-4), 0.0, 1.0);
        if b.y > 0.5 {
            t = (exp(3.0 * t) - 1.0) / (exp(3.0) - 1.0);
        }
        return mix(a.z, a.w, t);
    }
    if kind == 3 {
        if background {
            return 1.0;
        }
        return 1.0 - exp(-a.x * len);
    }
    if kind == 2 {
        // The part of the ray inside the layer.
        var t0 = 0.0;
        var t1 = 0.0;
        if abs(d.z) < 1e-5 {
            if o.z < a.x || o.z > a.y {
                return 0.0;
            }
            if background {
                return 1.0;
            }
            t1 = len;
        } else {
            let ta = (a.x - o.z) / d.z;
            let tb = (a.y - o.z) / d.z;
            t0 = max(min(ta, tb), 0.0);
            t1 = max(ta, tb);
            if !background {
                t1 = min(t1, len);
            }
        }
        if t1 <= t0 {
            return 0.0;
        }
        let steps = 16;
        let dt = (t1 - t0) / f32(steps);
        var optical = 0.0;
        for (var s = 0; s < steps; s += 1) {
            let p = o + d * (t0 + (f32(s) + 0.5) * dt);
            optical += layer_density(i, p.z) * dt;
        }
        return 1.0 - exp(-optical);
    }
    return 0.0;
}

// Light a Volume Light scatters towards the eye along o + t d, t in
// [0, len] (len < 0: the background, as far as the light reaches). Single
// scattering without shadows: inverse-square light, the spot's cone, Bevy's
// range fade.
fn volume_light(o: vec3<f32>, d: vec3<f32>, len: f32) -> vec3<f32> {
    var sum = vec3(0.0);
    for (var i = 0; i < 4; i += 1) {
        let kind = i32(env.vlight_pos[i].w + 0.5);
        if kind == 0 {
            continue;
        }
        let pos = env.vlight_pos[i].xyz;
        let range = env.vlight_range[i].x;
        // The part of the ray within the light's range.
        let oc = o - pos;
        let b = dot(oc, d);
        let disc = b * b - (dot(oc, oc) - range * range);
        if disc <= 0.0 {
            continue;
        }
        let s = sqrt(disc);
        var t0 = max(-b - s, 0.0);
        var t1 = -b + s;
        if len >= 0.0 {
            t1 = min(t1, len);
        }
        if t1 <= t0 {
            continue;
        }
        let steps = 48;
        let dt = (t1 - t0) / f32(steps);
        var acc = 0.0;
        for (var k = 0; k < steps; k += 1) {
            let p = o + d * (t0 + (f32(k) + 0.5) * dt);
            let v = p - pos;
            let dist2 = max(dot(v, v), 0.25);
            var att = 1.0 / dist2;
            let fade = clamp(1.0 - pow(dist2 / (range * range), 2.0), 0.0, 1.0);
            att *= fade * fade;
            if kind == 2 {
                let c = dot(v * inverseSqrt(dist2), env.vlight_dir[i].xyz);
                let cone = smoothstep(env.vlight_dir[i].w, env.vlight_color[i].w, c);
                att *= cone * cone;
            }
            acc += att * dt;
        }
        sum += env.vlight_color[i].rgb * acc;
    }
    return sum;
}

@fragment
fn environment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let p = vec2<i32>(in.position.xy);
    var c = textureLoad(color_texture, p, 0);
    let depth = depth_at(p);
    let near = world_at(in.uv, 1.0);
    let dir = normalize(world_at(in.uv, 0.5) - near);
    let geometry = depth > 0.0;
    if geometry {
        let hit = world_at(in.uv, depth);
        let len = length(hit - near);
        let vd = view_depth(hit);
        for (var i = 0; i < 4; i += 1) {
            let f = fog_amount(i, near, dir, len, vd);
            if f > 0.0 {
                c = vec4(mix(c.rgb, env.fog_color[i].rgb * c.a, f), c.a);
            }
        }
    }
    if env.background.w > 0.5 {
        var bg = background_color(in.uv, dir);
        for (var i = 0; i < 4; i += 1) {
            let f = fog_amount(i, near, dir, -1.0, 0.0);
            if f > 0.0 {
                bg = mix(bg, env.fog_color[i].rgb, f);
            }
        }
        c = vec4(c.rgb + (1.0 - c.a) * bg, c.a);
    }
    if env.vlight_pos[0].w > 0.5 {
        var len = -1.0;
        if geometry {
            len = length(world_at(in.uv, depth) - near);
        }
        c = vec4(c.rgb + volume_light(near, dir, len), c.a);
    }
    if env.grade.z > 0.5 {
        // Contrast around mid grey on the unpremultiplied colour, then exposure.
        let cover = select(c.a, 1.0, env.background.w > 0.5);
        if cover > 0.0 {
            let u = c.rgb / cover;
            c = vec4(max(0.5 + (u - 0.5) * env.grade.y, vec3(0.0)) * cover * env.grade.x, c.a);
        }
    }
    return c;
}

// Blur radius in pixels at view depth `d` for focal loss `loss`: none
// within the focal range, the full loss from the focal limit on.
fn circle(d: f32, loss: f32) -> f32 {
    let off = abs(d - env.dof.y);
    let t = clamp((off - env.dof.z) / max(env.dof.w - env.dof.z, 1e-4), 0.0, 1.0);
    return t * loss;
}

fn depth_of(uv: vec2<f32>, p: vec2<i32>) -> f32 {
    let z = depth_at(p);
    if z <= 0.0 {
        return 1e30;
    }
    return view_depth(world_at(uv, z));
}

fn dof_blur(in: FullscreenVertexOutput, axis: vec2<i32>, loss: f32) -> vec4<f32> {
    let p = vec2<i32>(in.position.xy);
    let size = vec2<i32>(textureDimensions(color_texture));
    let r = circle(depth_of(in.uv, p), loss);
    let centre = textureLoad(color_texture, p, 0);
    if r < 0.5 {
        return centre;
    }
    // Up to 32 taps either side, spread over the radius.
    let taps = min(i32(ceil(r)), 32);
    let stride = r / f32(taps);
    let sigma = max(r * 0.5, 0.5);
    var sum = vec4(0.0);
    var weight = 0.0;
    for (var k = -taps; k <= taps; k += 1) {
        let off = f32(k) * stride;
        let q = clamp(p + axis * i32(round(off)), vec2(0), size - vec2(1));
        let quv = (vec2<f32>(q) + 0.5) / vec2<f32>(size);
        // A sharp sample nearer than its offset does not spread into this pixel.
        let rq = circle(depth_of(quv, q), loss);
        let reach = select(1.0, 0.0, rq < abs(off) && rq < r);
        let w = exp(-0.5 * off * off / (sigma * sigma)) * max(reach, select(0.0, 1.0, k == 0));
        sum += textureLoad(color_texture, q, 0) * w;
        weight += w;
    }
    return sum / max(weight, 1e-6);
}

@fragment
fn dof_horizontal(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    return dof_blur(in, vec2(1, 0), env.dof_loss.x);
}

@fragment
fn dof_vertical(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    return dof_blur(in, vec2(0, 1), env.dof_loss.y);
}
