// The environment pass: after the main pass it draws the environment
// background (colour or map) behind the scene, the atmospheres (standard,
// layered and volume fog) over it in list order, the Fire effects in their
// atmospheric apparatus, and the Depth of Field blur (two passes:
// horizontal, then vertical).
//
// Colours are premultiplied: the main pass clears to transparent black, so
// an edge pixel is `coverage * colour`; the background fills what the
// geometry leaves uncovered and keeps alpha 0, as the reference's renders do.

#import bevy_render::view::View
#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

struct EnvPass {
    // rgb linear; w: 1 = draw the background.
    background: vec4<f32>,
    // x: 1 = a map; y: mapping (0 screen, 1 spherical, 2 cylindrical, 3 shrink-wrap);
    // z: 1 = the map's alpha joins the image's (Use Environment Alpha).
    map: vec4<f32>,
    // The part of the frame this view shows: u0, v0, u1, v1.
    frame: vec4<f32>,
    // Per atmosphere: rgb linear colour, w: kind (0 none, 1 standard, 2 layered, 3 volume, 4 fire).
    fog_color: array<vec4<f32>, 4>,
    // Standard: near distance, far distance, near density, far density.
    // Layered: bottom, top, extinction, falloff (0 none, 1 top, 2 bottom).
    // Volume: extinction, the centre of the sphere it fills.
    fog_a: array<vec4<f32>, 4>,
    // x: fog background, y: exponential (standard) / 1 = in gizmos (volume),
    // z: the volume's radius, w: the gizmos it fills (bit mask, volume and fire).
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
    // Atmospheric apparatus: world to local, x: shape (0 box, 1 sphere,
    // 2 cylinder) and its size (box: width, length, height; sphere: radius,
    // hemisphere; cylinder: radius, height), x: seed.
    gizmo_inv: array<mat4x4<f32>, 8>,
    gizmo_shape: array<vec4<f32>, 8>,
    gizmo_seed: array<vec4<f32>, 8>,
    // Fire: inner colour + tendril, outer colour + stretch, smoke colour +
    // regularity; flame size, detail, density, samples; phase, drift,
    // flags (1 explosion, 2 smoke), fury.
    fire_inner: array<vec4<f32>, 4>,
    fire_outer: array<vec4<f32>, 4>,
    fire_smoke: array<vec4<f32>, 4>,
    fire_shape: array<vec4<f32>, 4>,
    fire_motion: array<vec4<f32>, 4>,
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

fn background_color(uv: vec2<f32>, dir: vec3<f32>) -> vec4<f32> {
    if env.map.x < 0.5 {
        return vec4(env.background.rgb, 0.0);
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
    return vec4(c.rgb, select(0.0, c.a, env.map.z > 0.5));
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
    if kind == 3 && b.y > 0.5 {
        // The fog fills its gizmos: the parts of the ray inside them.
        var inside = 0.0;
        let mask = u32(b.w + 0.5);
        for (var g = 0; g < 8; g += 1) {
            if (mask & (1u << u32(g))) == 0u {
                continue;
            }
            let span = gizmo_span(g, o, d);
            var t1 = span.y;
            if !background {
                t1 = min(t1, len);
            }
            inside += max(t1 - max(span.x, 0.0), 0.0);
        }
        return 1.0 - exp(-a.x * inside);
    }
    if kind == 3 {
        // The fog fills the scene: the part of the ray inside its sphere
        // (centre a.yzw, radius b.z).
        let oc = o - a.yzw;
        let h = dot(oc, d);
        let disc = h * h - (dot(oc, oc) - b.z * b.z);
        if disc <= 0.0 {
            return 0.0;
        }
        let s = sqrt(disc);
        let t0 = max(-h - s, 0.0);
        var t1 = -h + s;
        if !background {
            t1 = min(t1, len);
        }
        return 1.0 - exp(-a.x * max(t1 - t0, 0.0));
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

// Entry and exit of the ray o + t d (world t) through apparatus g; exit <
// entry when it misses.
fn gizmo_span(g: i32, o: vec3<f32>, d: vec3<f32>) -> vec2<f32> {
    let m = env.gizmo_inv[g];
    let lo = (m * vec4(o, 1.0)).xyz;
    let ld = (m * vec4(d, 0.0)).xyz;
    let sh = env.gizmo_shape[g];
    let shape = i32(sh.x + 0.5);
    let miss = vec2(1.0, 0.0);
    if shape == 0 {
        let half = vec3(sh.y, sh.z, sh.w) * 0.5;
        let c = lo - vec3(0.0, 0.0, half.z);
        let inv = 1.0 / select(ld, vec3(1e-9), abs(ld) < vec3(1e-9));
        let ta = (-half - c) * inv;
        let tb = (half - c) * inv;
        let t0 = max(max(min(ta.x, tb.x), min(ta.y, tb.y)), min(ta.z, tb.z));
        let t1 = min(min(max(ta.x, tb.x), max(ta.y, tb.y)), max(ta.z, tb.z));
        return vec2(t0, t1);
    }
    // Sphere and cylinder: the quadratic of the radius (in xyz or xy).
    var p = lo;
    var q = ld;
    if shape == 2 {
        p.z = 0.0;
        q.z = 0.0;
    }
    let qa = dot(q, q);
    var t0 = -1e30;
    var t1 = 1e30;
    if qa > 1e-12 {
        let hb = dot(p, q);
        let disc = hb * hb - qa * (dot(p, p) - sh.y * sh.y);
        if disc <= 0.0 {
            return miss;
        }
        let s = sqrt(disc);
        t0 = (-hb - s) / qa;
        t1 = (-hb + s) / qa;
    } else if dot(p, p) > sh.y * sh.y {
        return miss;
    }
    // The cylinder's height, the hemisphere's base.
    let top = select(1e30, sh.z, shape == 2);
    let cut = shape == 2 || (shape == 1 && sh.z > 0.5);
    if cut {
        if abs(ld.z) < 1e-9 {
            if lo.z < 0.0 || lo.z > top {
                return miss;
            }
        } else {
            let za = (0.0 - lo.z) / ld.z;
            let zb = (top - lo.z) / ld.z;
            t0 = max(t0, min(za, zb));
            t1 = min(t1, max(za, zb));
        }
    }
    return vec2(t0, t1);
}

// Apparatus g's local point at world p: xyz, and w how far out (0 at the
// centre, 1 at the boundary); `zr` (returned in the caller) is the height
// within it.
fn gizmo_local(g: i32, p: vec3<f32>) -> vec4<f32> {
    let lp = (env.gizmo_inv[g] * vec4(p, 1.0)).xyz;
    let sh = env.gizmo_shape[g];
    let shape = i32(sh.x + 0.5);
    var r = 0.0;
    if shape == 0 {
        let half = max(vec3(sh.y, sh.z, sh.w) * 0.5, vec3(1e-4));
        let c = abs(lp - vec3(0.0, 0.0, half.z)) / half;
        r = max(max(c.x, c.y), c.z);
    } else if shape == 1 {
        r = length(lp) / max(sh.y, 1e-4);
    } else {
        let hh = max(sh.z * 0.5, 1e-4);
        r = max(length(lp.xy) / max(sh.y, 1e-4), abs(lp.z - hh) / hh);
    }
    return vec4(lp, r);
}

// Height within apparatus g, 0 at its base (the sphere's centre) to 1 at its top.
fn gizmo_height(g: i32, lp: vec3<f32>) -> f32 {
    let sh = env.gizmo_shape[g];
    let shape = i32(sh.x + 0.5);
    if shape == 1 {
        return clamp(lp.z / max(sh.y, 1e-4), -1.0, 1.0);
    }
    return clamp(lp.z / max(select(sh.w, sh.z, shape == 2), 1e-4), 0.0, 1.0);
}

fn hash3(p: vec3<f32>) -> f32 {
    let q = fract(p * vec3(0.1031, 0.1030, 0.0973));
    let r = q + dot(q, q.yxz + 33.33);
    return fract((r.x + r.y) * r.z);
}

fn value_noise(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let n000 = hash3(i);
    let n100 = hash3(i + vec3(1.0, 0.0, 0.0));
    let n010 = hash3(i + vec3(0.0, 1.0, 0.0));
    let n110 = hash3(i + vec3(1.0, 1.0, 0.0));
    let n001 = hash3(i + vec3(0.0, 0.0, 1.0));
    let n101 = hash3(i + vec3(1.0, 0.0, 1.0));
    let n011 = hash3(i + vec3(0.0, 1.0, 1.0));
    let n111 = hash3(i + vec3(1.0, 1.0, 1.0));
    return mix(mix(mix(n000, n100, u.x), mix(n010, n110, u.x), u.y), mix(mix(n001, n101, u.x), mix(n011, n111, u.x), u.y), u.z);
}

// Fractal noise, `octaves` (fractional) of halving size: 0..1.
fn fbm(p: vec3<f32>, octaves: f32, ridged: bool) -> f32 {
    var sum = 0.0;
    var amp = 0.5;
    var norm = 0.0;
    var x = p;
    let n = i32(ceil(octaves));
    for (var k = 0; k < n; k += 1) {
        var v = value_noise(x);
        if ridged {
            v = 1.0 - abs(2.0 * v - 1.0);
        }
        let w = amp * clamp(octaves - f32(k), 0.0, 1.0);
        sum += v * w;
        norm += w;
        amp *= 0.5;
        x = x * 2.03 + vec3(17.1, 9.3, 4.7);
    }
    return sum / max(norm, 1e-4);
}

// Fire atmosphere i along o + t d up to `len` (< 0: unbounded): emitted
// light (premultiplied) and the transmittance left.
fn fire(i: i32, o: vec3<f32>, d: vec3<f32>, len: f32) -> vec4<f32> {
    let inner = env.fire_inner[i];
    let outer = env.fire_outer[i];
    let smoke = env.fire_smoke[i];
    let shp = env.fire_shape[i];
    let mot = env.fire_motion[i];
    let tendril = inner.w > 0.5;
    let stretch = max(outer.w, 1e-3);
    let regularity = clamp(smoke.w, 0.0, 1.0);
    let size = max(shp.x, 1e-3);
    let octaves = 1.0 + clamp(shp.y, 0.0, 10.0) * 0.5;
    var density = max(shp.z, 0.0);
    let samples = clamp(i32(shp.w + 0.5), 1, 50);
    let flags = u32(mot.z + 0.5);
    let phase = mot.x;
    // Explosion: density grows to its peak by phase 100; between 100 and 200
    // the colours turn to smoke (or stay); between 200 and 300 it clears.
    var smoke_mix = 0.0;
    var scale = 1.0;
    if (flags & 1u) != 0u {
        let grow = clamp(phase / 100.0, 0.0, 1.0);
        density *= grow;
        scale = max(grow, 0.05);
        if (flags & 2u) != 0u {
            smoke_mix = clamp((phase - 100.0) / 100.0, 0.0, 1.0);
        }
        density *= 1.0 - clamp((phase - 200.0) / 100.0, 0.0, 1.0);
    }
    // Churning: the noise moves through a fourth dimension with the phase.
    let churn = phase * max(mot.w, 0.0) * 0.02;
    let slice = floor(churn);
    let blend = churn - slice;
    var emit = vec3(0.0);
    var trans = 1.0;
    let mask = u32(env.fog_b[i].w + 0.5);
    for (var g = 0; g < 8; g += 1) {
        if (mask & (1u << u32(g))) == 0u {
            continue;
        }
        let span = gizmo_span(g, o, d);
        let t0 = max(span.x, 0.0);
        var t1 = span.y;
        if len >= 0.0 {
            t1 = min(t1, len);
        }
        if t1 <= t0 {
            continue;
        }
        let seed = vec3(env.gizmo_seed[g].x * 0.6180339, env.gizmo_seed[g].x * 0.3819660, 0.0) % 1000.0;
        // The noise around the apparatus (its centre and six points half way
        // out): the flames are measured from it, so the seed varies their
        // pattern, not how much fire there is.
        let sh = env.gizmo_shape[g];
        let shape_kind = i32(sh.x + 0.5);
        var ext = vec3(sh.y);
        if shape_kind == 0 {
            ext = vec3(sh.y, sh.z, sh.w) * 0.5;
        } else if shape_kind == 2 {
            ext = vec3(sh.y, sh.y, sh.z * 0.5);
        }
        let centre = vec3(0.0, 0.0, select(ext.z, 0.0, shape_kind == 1));
        var nc = 0.0;
        for (var j = 0; j < 7; j += 1) {
            var q = centre;
            if j > 0 {
                let axis = (j - 1) / 2;
                let sgn = select(-0.5, 0.5, (j & 1) == 1);
                q[axis] += sgn * ext[axis];
            }
            let xq = vec3(q.x, q.y, (q.z - mot.y) / stretch) / size + seed;
            nc += mix(fbm(xq + vec3(0.0, 0.0, slice * 3.7), octaves, tendril), fbm(xq + vec3(0.0, 0.0, (slice + 1.0) * 3.7), octaves, tendril), blend);
        }
        nc /= 7.0;
        let steps = samples * 4;
        let dt = (t1 - t0) / f32(steps);
        for (var k = 0; k < steps; k += 1) {
            let p = o + d * (t0 + (f32(k) + 0.5) * dt);
            let l = gizmo_local(g, p);
            let r = l.w / scale;
            if r >= 1.0 {
                continue;
            }
            // Flames rise along local Z; Stretch lengthens them, Drift moves them up.
            let x = vec3(l.x, l.y, (l.z - mot.y) / stretch) / size + seed;
            let n0 = fbm(x + vec3(0.0, 0.0, slice * 3.7), octaves, tendril);
            let n1 = fbm(x + vec3(0.0, 0.0, (slice + 1.0) * 3.7), octaves, tendril);
            let n = clamp(mix(n0, n1, blend) - nc + 0.5, 0.0, 1.0);
            var shape = 1.0 - r;
            if tendril {
                // Pointed flames: they thin out towards the top.
                let h = max(gizmo_height(g, l.xyz), 0.0);
                shape *= 1.0 - h * h;
            }
            // Regularity 1 fills the apparatus; 0 leaves it to the noise.
            let v = clamp(mix(n * 1.6 - 0.55, 1.0, regularity * 0.5) * mix(1.0, shape * 2.0, regularity) + (shape - 0.5) * (1.0 - regularity), 0.0, 1.0) * smoothstep(0.0, 0.15, 1.0 - r);
            if v <= 0.0 {
                continue;
            }
            let a = 1.0 - exp(-v * density * 0.02 * dt);
            // Dense parts take the inner colour, sparse ones the outer; high
            // densities whiten the core.
            var col = mix(outer.rgb, inner.rgb, clamp(v * 1.5, 0.0, 1.0));
            col = mix(col, vec3(1.0), clamp(v * density / 120.0, 0.0, 0.8) * v);
            col = mix(col, smoke.rgb, smoke_mix);
            emit += trans * a * col * 2.0;
            trans *= 1.0 - a;
            if trans < 0.003 {
                break;
            }
        }
    }
    return vec4(emit, trans);
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
        // Fine enough steps for a narrow cone across the scene.
        let steps = 192;
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
        let bg_a = background_color(in.uv, dir);
        var bg = bg_a.rgb;
        for (var i = 0; i < 4; i += 1) {
            let f = fog_amount(i, near, dir, -1.0, 0.0);
            if f > 0.0 {
                bg = mix(bg, env.fog_color[i].rgb, f);
            }
        }
        // Alpha 0 behind the scene, or with Use Environment Alpha the map's.
        c = vec4(c.rgb + (1.0 - c.a) * bg, c.a + (1.0 - c.a) * bg_a.a);
    }
    // Fire renders in perspective and camera views only.
    if view.clip_from_view[3][3] < 0.5 {
        var flen = -1.0;
        if geometry {
            flen = length(world_at(in.uv, depth) - near);
        }
        for (var i = 0; i < 4; i += 1) {
            if i32(env.fog_color[i].w + 0.5) == 4 {
                let f = fire(i, near, dir, flen);
                c = vec4(c.rgb * f.w + f.rgb, 1.0 - (1.0 - c.a) * f.w);
            }
        }
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
