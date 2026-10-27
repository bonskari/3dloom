// Exact position equation from loom_scene::skin::apply. All effective
// influences are retained, including weights attached to missing bones in sum.
struct Bone { matrix: mat4x4<f32>, valid: vec4<u32> }
struct Pose { mesh_bind: mat4x4<f32>, object_world: mat4x4<f32>, back: mat4x4<f32>, count: vec4<u32> }
@group(0) @binding(0) var<storage, read> positions: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read> ranges: array<vec2<u32>>;
// y holds the weight's f32 bits.
@group(0) @binding(2) var<storage, read> influences: array<vec2<u32>>;
@group(0) @binding(3) var<storage, read> bones: array<Bone>;
@group(0) @binding(4) var<uniform> pose: Pose;
@group(0) @binding(5) var<storage, read_write> output: array<vec4<f32>>;

@compute @workgroup_size(64)
fn skin(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if i >= pose.count.x { return; }
    let p = positions[i];
    let pw = pose.mesh_bind * p;
    let range = ranges[i];
    var sum = 0.0;
    var total = 0.0;
    var acc = vec3<f32>(0.0);
    for (var j = 0u; j < range.y; j += 1u) {
        let influence = influences[range.x + j];
        let weight = bitcast<f32>(influence.y);
        sum += weight;
        let bone = bones[influence.x];
        if bone.valid.x != 0u {
            acc += (bone.matrix * pw).xyz * weight;
            total += weight;
        }
    }
    var result = p;
    if sum > 1e-6 && total > 1e-6 {
        let k = min(sum, 1.0);
        let world = acc / total * k + (pose.object_world * p).xyz * (1.0 - k);
        result = pose.back * vec4<f32>(world, 1.0);
    }
    output[i] = result;
}
