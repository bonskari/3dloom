// Over Bevy's standard material: the Falloff map's view-dependent mix of
// its Front (the material's base colour) and Side colours, Ink 'n Paint's
// flat paint bands with an ink outline, and Matte/Shadow, which shows only
// the shadows falling on it.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
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
};

@group(2) @binding(100) var<uniform> look: LookParams;

const FALLOFF: u32 = 1u;
const TOON: u32 = 2u;
const MATTE: u32 = 4u;

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
    return luminance(apply_pbr_lighting(p).rgb);
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

    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
