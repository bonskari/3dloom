// Unwired current-pose normal/packing experiment. No bind-normal transform.
struct Corner { source: vec4<u32>, flags: vec4<u32>, explicit_normal: vec4<f32>, uv: vec4<f32> }
struct Vertex { position: vec4<f32>, normal: vec4<f32>, tangent: vec4<f32>, uv: vec4<f32> }
struct Pose { world: mat4x4<f32>, counts: vec4<u32> }
@group(0) @binding(0) var<storage, read> positions: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read> topology: array<u32>;
@group(0) @binding(2) var<storage, read> corners: array<Corner>;
// Current CPU Mikk results are test-oracle inputs, not a shipped GPU tangent path.
@group(0) @binding(3) var<storage, read> oracle_tangents: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read_write> face_normals: array<vec4<f32>>;
@group(0) @binding(5) var<storage, read_write> output: array<Vertex>;
@group(0) @binding(6) var<uniform> pose: Pose;
@group(0) @binding(7) var<storage, read_write> world_positions: array<vec4<f32>>;

fn unit_or_zero(v: vec3<f32>) -> vec3<f32> {
    let reciprocal = 1.0 / sqrt(dot(v,v));
    if reciprocal > 0.0 && reciprocal <= 3.402823466e38 { return v * reciprocal; }
    return vec3<f32>(0.0);
}
fn transformed_point(index: u32) -> vec3<f32> {
    let p = positions[index].xyz;
    var result = pose.world[0] * p.x;
    result = pose.world[1] * p.y + result;
    result = pose.world[2] * p.z + result;
    return (pose.world[3] + result).xyz;
}

fn point(index: u32) -> vec3<f32> { return world_positions[index].xyz; }
@compute @workgroup_size(64)
fn transform_positions(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x >= pose.counts.z { return; }
    world_positions[invocation.x] = vec4<f32>(transformed_point(invocation.x), 1.0);
}

@compute @workgroup_size(64)
fn faces(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let face = invocation.x;
    if face >= pose.counts.x { return; }
    let offset = topology[face * 4u];
    let count = topology[face * 4u + 1u];
    var n = vec3<f32>(0.0);
    // Same ordered Newell sum as Mesh::face_normal, including hidden faces.
    for (var i = 0u; i < count; i += 1u) {
        let a = point(topology[offset + i]);
        let b = point(topology[offset + (i + 1u) % count]);
        n += vec3<f32>((a.y-b.y)*(a.z+b.z), (a.z-b.z)*(a.x+b.x), (a.x-b.x)*(a.y+b.y));
    }
    face_normals[face] = vec4<f32>(unit_or_zero(n), 0.0);
}

// Algebraically the same Newell polygon area vector, expressed as a
// cross-sum about the first CURRENT posed vertex to avoid world-origin
// cancellation. The legacy world-space entry remains as a diagnostic oracle.
@compute @workgroup_size(64)
fn faces_origin_relative(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let face = invocation.x;
    if face >= pose.counts.x { return; }
    let offset = topology[face * 4u];
    let count = topology[face * 4u + 1u];
    let origin = point(topology[offset]);
    var n = vec3<f32>(0.0);
    for (var i = 0u; i < count; i += 1u) {
        let a = point(topology[offset + i]) - origin;
        let b = point(topology[offset + (i + 1u) % count]) - origin;
        n += cross(a, b);
    }
    face_normals[face] = vec4<f32>(unit_or_zero(n), 0.0);
}

fn pack_geometry(i:u32) {
    if i >= pose.counts.y { return; }
    let corner = corners[i];
    let face = corner.source.y;
    var n = face_normals[face].xyz;
    if corner.flags.y != 0u {
        n = corner.explicit_normal.xyz;
    } else if corner.flags.x != 0u && corner.flags.z == 0u {
        var sum = vec3<f32>(0.0);
        for (var j = 0u; j < corner.source.w; j += 1u) {
            let neighbor = topology[corner.source.z + j];
            if (topology[neighbor * 4u + 2u] & corner.flags.x) != 0u { sum += face_normals[neighbor].xyz; }
        }
        if dot(sum,sum) >= 1e-12 { n = unit_or_zero(sum); }
    }
    output[i].position = vec4<f32>(point(corner.source.x), 1.0);
    output[i].normal = vec4<f32>(n, 0.0);
    // Intermediate geometry is never rendered before the tangent stage.
    output[i].tangent = vec4<f32>(0.0);
    output[i].uv = corner.uv;
}

@compute @workgroup_size(64)
fn pack_geometry_only(@builtin(global_invocation_id) id:vec3<u32>){pack_geometry(id.x);}
@compute @workgroup_size(64)
fn pack_corners(@builtin(global_invocation_id) id:vec3<u32>){if id.x>=pose.counts.y{return;}pack_geometry(id.x);output[id.x].tangent=oracle_tangents[id.x];}
