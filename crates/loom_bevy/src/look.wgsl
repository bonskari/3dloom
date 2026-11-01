// Over Bevy's standard material: the Falloff map's view-dependent mix of
// its Front (the material's base colour) and Side colours, Ink 'n Paint's
// flat paint bands with an ink outline, and Matte/Shadow, which shows only
// the shadows falling on it. A light that excludes the object from
// illumination has its direct light taken back out.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing, calculate_diffuse_color, calculate_F0},
    mesh_view_bindings as view_bindings,
    lighting,
    lighting::LAYER_BASE,
    clustered_forward as clustering,
    shadows,
    forward_io::{VertexOutput, FragmentOutput},
    mesh_types::MESH_FLAGS_SHADOW_RECEIVER_BIT,
    mesh_view_bindings::{view, lights},
    pbr_types,
}

struct LookParams {
    flags: u32,
    falloff_type: u32,
    falloff_dir: u32,
    levels: u32,
    side: vec4<f32>,
    dir: vec4<f32>,
    near_far: vec4<f32>,
    lighted: vec4<f32>,
    shaded: vec4<f32>,
    highlight: vec4<f32>,
    ink: vec4<f32>,
    matte: vec4<f32>,
    matte_flags: vec4<f32>,
    excluded: array<vec4<f32>, 8>,
};

@group(2) @binding(100) var<uniform> look: LookParams;
// A render's ray-traced Ambient Occlusion per pixel (a 1x1 white fallback without).
@group(2) @binding(101) var ao_texture: texture_2d<f32>;

const FALLOFF: u32 = 1u;
const TOON: u32 = 2u;
const MATTE: u32 = 4u;
const COLOR_SCALE: u32 = 8u;

// Whether light key `k` (position or direction to the light) is one the object is excluded from.
fn is_excluded(k: vec3<f32>, kind: f32) -> bool {
    for (var j = 0u; j < 8u; j = j + 1u) {
        let e = look.excluded[j];
        if (e.w == kind && distance(e.xyz, k) <= 1e-3 * max(1.0, length(k))) {
            return true;
        }
    }
    return false;
}

// The direct light of the lights that exclude the object (as Bevy lights it).
fn excluded_light(in: pbr_types::PbrInput) -> vec3<f32> {
    var any = false;
    for (var j = 0u; j < 8u; j = j + 1u) {
        any = any || look.excluded[j].w > 0.0;
    }
    if (!any || (in.material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT) != 0u) {
        return vec3<f32>(0.0);
    }
    let roughness = lighting::perceptualRoughnessToRoughness(in.material.perceptual_roughness);
    let NdotV = max(dot(in.N, in.V), 0.0001);
    var li: lighting::LightingInput;
    li.layers[LAYER_BASE].NdotV = NdotV;
    li.layers[LAYER_BASE].N = in.N;
    li.layers[LAYER_BASE].R = reflect(-in.V, in.N);
    li.layers[LAYER_BASE].perceptual_roughness = in.material.perceptual_roughness;
    li.layers[LAYER_BASE].roughness = roughness;
    li.P = in.world_position.xyz;
    li.V = in.V;
    li.diffuse_color = calculate_diffuse_color(in.material.base_color.rgb, in.material.metallic, in.material.specular_transmission, in.material.diffuse_transmission);
    li.F0_ = calculate_F0(in.material.base_color.rgb, in.material.metallic, in.material.reflectance);
    li.F_ab = lighting::F_AB(in.material.perceptual_roughness, NdotV);
#ifdef STANDARD_MATERIAL_ANISOTROPY
    li.anisotropy = in.anisotropy_strength;
    li.Ta = in.anisotropy_T;
    li.Ba = in.anisotropy_B;
#endif
    let receives = (in.flags & MESH_FLAGS_SHADOW_RECEIVER_BIT) != 0u;
    var light = vec3<f32>(0.0);
    let view_z = dot(vec4<f32>(view.view_from_world[0].z, view.view_from_world[1].z, view.view_from_world[2].z, view.view_from_world[3].z), in.world_position);
    let ranges = clustering::unpack_clusterable_object_index_ranges(clustering::fragment_cluster_index(in.frag_coord.xy, view_z, in.is_orthographic));
    for (var i = ranges.first_point_light_index_offset; i < ranges.first_reflection_probe_index_offset; i = i + 1u) {
        let id = clustering::get_clusterable_object_id(i);
        let o = &view_bindings::clusterable_objects.data[id];
        if (!is_excluded((*o).position_radius.xyz, 1.0)) {
            continue;
        }
        let spot = i >= ranges.first_spot_light_index_offset;
        var shadow = 1.0;
        if (receives && ((*o).flags & 1u) != 0u) {
            if (spot) {
                shadow = shadows::fetch_spot_shadow(id, in.world_position, in.world_normal, (*o).shadow_map_near_z);
            } else {
                shadow = shadows::fetch_point_shadow(id, in.world_position, in.world_normal);
            }
        }
        if (spot) {
            light += lighting::spot_light(id, &li, true) * shadow;
        } else {
            light += lighting::point_light(id, &li, true) * shadow;
        }
    }
    for (var i = 0u; i < lights.n_directional_lights; i = i + 1u) {
        if (!is_excluded(lights.directional_lights[i].direction_to_light, 2.0)) {
            continue;
        }
        var shadow = 1.0;
        if (receives && (lights.directional_lights[i].flags & 1u) != 0u) {
            shadow = shadows::fetch_directional_shadow(i, in.world_position, in.world_normal, view_z);
        }
        light += lighting::directional_light(i, &li, true) * shadow;
    }
    return light * view.exposure;
}

// Bevy's lighting less the lights that exclude the object.
fn lighting_less_excluded(in: pbr_types::PbrInput) -> vec4<f32> {
    let c = apply_pbr_lighting(in);
    return vec4<f32>(max(c.rgb - excluded_light(in), vec3<f32>(0.0)), c.a);
}

fn luminance(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

// The diffuse light reaching the point (a white, matte, unlit-free surface).
fn diffuse_light(input: pbr_types::PbrInput) -> f32 {
    var p = input;
    p.material.base_color = vec4<f32>(1.0, 1.0, 1.0, 1.0);
    p.material.emissive = vec4<f32>(0.0);
    p.material.metallic = 0.0;
    p.material.reflectance = vec3<f32>(0.0);
    p.material.perceptual_roughness = 1.0;
    p.material.specular_transmission = 0.0;
    p.material.diffuse_transmission = 0.0;
    p.material.clearcoat = 0.0;
    return luminance(lighting_less_excluded(p).rgb);
}

// The share of the light that is not shadowed, 0–1.
fn unshadowed(input: pbr_types::PbrInput) -> f32 {
    let shadowed = diffuse_light(input);
    var free = input;
    free.flags = free.flags & ~MESH_FLAGS_SHADOW_RECEIVER_BIT;
    return clamp(shadowed / max(diffuse_light(free), 1e-5), 0.0, 1.0);
}

// How lit the point is, 0–1: facing the main (first directional) light, out of shadow.
fn lit_level(input: pbr_types::PbrInput) -> f32 {
    var facing = 1.0;
    if (lights.n_directional_lights > 0u) {
        facing = max(dot(input.N, lights.directional_lights[0].direction_to_light), 0.0);
    }
    return facing * unshadowed(input);
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    var out: FragmentOutput;

    if ((look.flags & FALLOFF) != 0u) {
        var d = pbr_input.V;
        if (look.falloff_dir == 1u) {
            d = normalize(look.dir.xyz);
        }
        let c = dot(pbr_input.N, d);
        var t = 0.0;
        switch look.falloff_type {
            // Towards/Away: facing the direction Front, facing away Side.
            case 0u: { t = (1.0 - c) * 0.5; }
            // Perpendicular/Parallel: facing (either way) Front, edge-on Side.
            case 1u: { t = 1.0 - abs(c); }
            // Fresnel: Schlick's reflectance at the IOR.
            case 2u: {
                let r0 = pow((look.near_far.z - 1.0) / (look.near_far.z + 1.0), 2.0);
                t = r0 + (1.0 - r0) * pow(1.0 - clamp(abs(c), 0.0, 1.0), 5.0);
            }
            // Shadow/Light: lit Side, in shadow Front.
            case 3u: { t = lit_level(pbr_input); }
            // Distance Blend: Near Distance Front, Far Distance Side.
            default: {
                let dist = distance(view.world_position.xyz, pbr_input.world_position.xyz);
                t = clamp((dist - look.near_far.x) / max(look.near_far.y - look.near_far.x, 1e-4), 0.0, 1.0);
            }
        }
        let front = pbr_input.material.base_color;
        pbr_input.material.base_color = vec4<f32>(mix(front.rgb, look.side.rgb, t), front.a);
    }

    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);
    if (textureDimensions(ao_texture).x > 1u) {
        pbr_input.diffuse_occlusion *= textureLoad(ao_texture, vec2<i32>(in.position.xy), 0).r;
    }

    if ((look.flags & MATTE) != 0u) {
        // The shadows alone: the light with shadows against the light without them.
        var lit = 1.0;
        if (look.matte_flags.x > 0.5) {
            lit = unshadowed(pbr_input);
        }
        let shadow = (1.0 - lit) * (1.0 - look.matte.w);
        out.color = vec4<f32>(look.matte.rgb, shadow);
        return out;
    }

    if ((look.flags & TOON) != 0u) {
        // Paint Levels flat bands from Shaded to Lighted, by the light reaching the point.
        let l = lit_level(pbr_input);
        let n = f32(max(look.levels, 1u));
        var band = 1.0;
        if (n > 1.0) {
            band = min(floor(l * n), n - 1.0) / (n - 1.0);
        }
        var c = mix(look.shaded.rgb, look.lighted.rgb, band);
        if (look.highlight.w > 0.5 && l > 0.97) {
            c = look.highlight.rgb;
        }
        // Ink: the outline where the surface turns away from the view.
        if (look.ink.w > 0.0 && abs(dot(pbr_input.N, pbr_input.V)) < 0.08 * look.ink.w) {
            c = look.ink.rgb;
        }
        out.color = vec4<f32>(c, pbr_input.material.base_color.a);
        out.color = main_pass_post_lighting_processing(pbr_input, out.color);
        return out;
    }

    // An unlit material shows its colour as it is (as Bevy's own fragment does).
    if ((pbr_input.material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u) {
        out.color = lighting_less_excluded(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    if ((look.flags & COLOR_SCALE) != 0u) {
        // Color Range Limiting Scale: the largest component comes down to 1, the hue stays.
        let m = max(out.color.r, max(out.color.g, out.color.b));
        if (m > 1.0) {
            out.color = vec4<f32>(out.color.rgb / m, out.color.a);
        }
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
