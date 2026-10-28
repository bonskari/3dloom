// Bitwise gather: vertex position and colour are six tightly packed u32 words.
// A struct with vec3 fields would have a 32-byte storage stride, not LineVertex24.
struct Params { counts:vec4<u32>, color:vec4<u32> }
@group(0) @binding(0) var<storage,read> source:array<u32>;
@group(0) @binding(1) var<storage,read> endpoints:array<u32>;
@group(0) @binding(2) var<storage,read_write> output:array<u32>;
@group(0) @binding(3) var<uniform> params:Params;
@compute @workgroup_size(64)
fn gather(@builtin(global_invocation_id) invocation:vec3<u32>){
    let i=invocation.x;
    if i>=params.counts.x{return;}
    let src=endpoints[i]*4u;
    let dst=i*6u;
    output[dst]=source[src];
    output[dst+1u]=source[src+1u];
    output[dst+2u]=source[src+2u];
    output[dst+3u]=params.color.x;
    output[dst+4u]=params.color.y;
    output[dst+5u]=params.color.z;
}
