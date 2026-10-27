// Unwired EvalTspace arithmetic experiment. Group membership/current triangle
// derivatives are CPU-oracle fixtures; this is not a production Mikk pipeline.
struct Face { normal: vec4<f32>, prev: vec4<f32>, position: vec4<f32>, next: vec4<f32>, os: vec4<f32>, ot: vec4<f32>, probe: vec4<f32>, flags: vec4<u32> }
struct Result { s: vec4<f32>, t: vec4<f32> }
@group(0) @binding(0) var<storage,read> faces: array<Face>;
@group(0) @binding(1) var<storage,read> groups: array<vec2<u32>>;
@group(0) @binding(2) var<storage,read_write> results: array<Result>;
@group(0) @binding(3) var<storage,read_write> angles: array<vec4<f32>>;
fn nonzero(v:vec3<f32>)->bool { return any(abs(v)>vec3<f32>(1.17549435e-38)); }
fn unit(v:vec3<f32>)->vec3<f32> { return (1.0/sqrt(dot(v,v)))*v; }
@compute @workgroup_size(64)
fn evaluate(@builtin(global_invocation_id) id:vec3<u32>) {
    let i=id.x;
    if i>=arrayLength(&groups) {return;}
    let group=groups[i];
    var s=vec3<f32>(0.0);var t=vec3<f32>(0.0);var mag_s=0.0;var mag_t=0.0;var angle_sum=0.0;
    for(var j=0u;j<group.y;j+=1u) {
        let index=group.x+j;
        let face=faces[index];
        if face.flags.x!=0u {continue;}
        let n=face.normal.xyz;
        var os=face.os.xyz-dot(n,face.os.xyz)*n;
        var ot=face.ot.xyz-dot(n,face.ot.xyz)*n;
        if nonzero(os) {os=unit(os);}
        if nonzero(ot) {ot=unit(ot);}
        var a=face.prev.xyz-face.position.xyz;
        var b=face.next.xyz-face.position.xyz;
        a=a-dot(n,a)*n;b=b-dot(n,b)*n;
        if nonzero(a) {a=unit(a);}
        if nonzero(b) {b=unit(b);}
        let cosine=clamp(dot(a,b),-1.0,1.0);
        // CPU reference explicitly uses f64 acos then rounds to f32.
        // WGSL native acos is measured here, never claimed bit-identical.
        let angle=acos(cosine);
        angles[index]=vec4<f32>(cosine,angle,acos(face.probe.x),face.probe.x);
        s=s+angle*os;t=t+angle*ot;
        mag_s+=angle*face.os.w;mag_t+=angle*face.ot.w;angle_sum+=angle;
    }
    if nonzero(s) {s=unit(s);}
    if nonzero(t) {t=unit(t);}
    if angle_sum>0.0 {mag_s/=angle_sum;mag_t/=angle_sum;}
    results[i].s=vec4<f32>(s,mag_s);results[i].t=vec4<f32>(t,mag_t);
}
