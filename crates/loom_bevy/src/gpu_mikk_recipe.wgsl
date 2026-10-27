// Hybrid current structural recipe; ALL tangent floating-point math is current
// GPU work. Recipe contains no tangent, derivative, angle or normal values.
struct Vertex {position:vec4<f32>,normal:vec4<f32>,tangent:vec4<f32>,uv:vec4<f32>}
struct Params {counts:vec4<u32>,prep:vec4<u32>,recipe0:vec4<u32>,recipe1:vec4<u32>,scratch:vec4<u32>,debug:vec4<u32>}
@group(0) @binding(0) var<storage,read> vertices:array<Vertex>;
@group(0) @binding(1) var<storage,read> triangles:array<vec4<u32>>;
@group(0) @binding(2) var<storage,read_write> prep:array<atomic<u32>>;
@group(0) @binding(3) var<storage,read> recipe:array<u32>;
@group(0) @binding(4) var<storage,read_write> scratch:array<atomic<u32>>;
@group(0) @binding(5) var<storage,read_write> spaces:array<vec4<f32>>;
@group(0) @binding(6) var<storage,read_write> output:array<Vertex>;
@group(0) @binding(7) var<storage,read_write> derivatives:array<vec4<f32>>;
@group(0) @binding(8) var<uniform> params:Params;
fn original(f:u32)->u32{return atomicLoad(&prep[params.prep.y+f]);}
fn canonical(corner:u32)->u32{return atomicLoad(&prep[params.prep.x+corner]);}
fn value(corner:u32)->Vertex{return vertices[params.prep.w+triangles[corner/3u][corner%3u]];}
fn welded(f:u32,c:u32)->Vertex{return value(canonical(original(f)*3u+c));}
fn nonzero(v:vec3<f32>)->bool{return any(abs(v)>vec3<f32>(1.17549435e-38));}
fn not_zero(v:f32)->bool{return abs(v)>1.17549435e-38;}
fn unit(v:vec3<f32>)->vec3<f32>{return (1.0/sqrt(dot(v,v)))*v;}
fn projected(v:vec3<f32>,n:vec3<f32>)->vec3<f32>{var p=v-dot(n,v)*n;if nonzero(p){p=unit(p);}return p;}
// At the renderer's fixed 180-degree threshold, normalized dot>-1 means
// every direction except exactly antiparallel. Testing geometric directions
// avoids a rounded unit-vector dot falling BELOW -1 (the actual Face failure).
fn angular_member(a:vec3<f32>,b:vec3<f32>)->bool {
    if !nonzero(a)||!nonzero(b){return true;}
    if all(a == -b){return false;}
    let perpendicular=cross(a,b);
    return dot(a,b)>=0.0 || any(perpendicular!=vec3<f32>(0.0));
}
fn corner_angle(a:vec3<f32>,b:vec3<f32>)->f32 {
    // Original acos(dot(0,v)) is pi/2 when a projected edge is zero.
    if !nonzero(a)||!nonzero(b){return 1.5707963267948966;}
    if all(a==b){return 0.0;}
    let perpendicular=cross(a,b);
    return atan2(sqrt(dot(perpendicular,perpendicular)),dot(a,b));
}
fn descriptor(group:u32)->vec4<u32>{let i=params.recipe1.y+group*4u;return vec4<u32>(recipe[i],recipe[i+1u],recipe[i+2u],recipe[i+3u]);}
@compute @workgroup_size(64)
fn guard_init(@builtin(global_invocation_id) id:vec3<u32>){
    let i=id.x;let corners=params.counts.y*3u;
    if i>=corners{return;}
    if canonical(i)!=recipe[params.recipe0.x+i]{atomicOr(&scratch[0],1u);}
    if i<params.counts.y {
        if original(i)!=recipe[params.recipe0.y+i] || atomicLoad(&prep[params.prep.z+original(i)])!=recipe[params.recipe0.z+i] {atomicOr(&scratch[0],2u);}
    }
    if i==0u {if atomicLoad(&prep[0])!=params.counts.z {atomicOr(&scratch[0],4u);}if atomicLoad(&prep[1])!=0u {atomicOr(&scratch[0],16u);}}
    let f=i/3u;let c=i%3u;let source=original(f)*3u+c;
    atomicStore(&scratch[params.scratch.w+source],i);
    let vertex=triangles[source/3u][source%3u];
    atomicMax(&scratch[params.scratch.x+vertex],source+1u);
    if f<atomicLoad(&prep[0]) {atomicMax(&scratch[params.scratch.y+canonical(source)],0xffffffffu-i);}
    spaces[source*2u]=vec4<f32>(1.0,0.0,0.0,1.0);
    spaces[source*2u+1u]=vec4<f32>(0.0,1.0,0.0,1.0);
}
@compute @workgroup_size(64)
fn derive(@builtin(global_invocation_id) id:vec3<u32>){
    let f=id.x;if params.debug.w!=0u || f>=params.counts.z || atomicLoad(&scratch[0])!=0u{return;}
    let a=welded(f,0u);let b=welded(f,1u);let c=welded(f,2u);
    let x=b.uv.xy-a.uv.xy;let y=c.uv.xy-a.uv.xy;
    let d1=b.position.xyz-a.position.xyz;let d2=c.position.xyz-a.position.xyz;
    let area=x.x*y.y-x.y*y.x;
    var s=vec4<f32>(0.0);var t=vec4<f32>(0.0);
    if not_zero(area){
        let os=y.y*d1-x.y*d2;let ot=(-y.x)*d1+x.x*d2;
        let ls=sqrt(dot(os,os));let lt=sqrt(dot(ot,ot));let sign=select(-1.0,1.0,area>0.0);
        if not_zero(ls){s=vec4<f32>((sign/ls)*os,0.0);}
        if not_zero(lt){t=vec4<f32>((sign/lt)*ot,0.0);}
        s.w=ls/abs(area);t.w=lt/abs(area);
    }
    derivatives[f*2u]=s;derivatives[f*2u+1u]=t;
}
@compute @workgroup_size(64)
fn evaluate(@builtin(global_invocation_id) id:vec3<u32>){
    let i=id.x;if i>=params.counts.z*3u || atomicLoad(&scratch[0])!=0u{return;}
    let assigned=recipe[params.recipe1.x+i];if assigned==0u{return;}
    let group=descriptor(assigned-1u);let face=i/3u;
    let n=value(group.z).normal.xyz;let representative=canonical(group.z);
    let os=projected(derivatives[face*2u].xyz,n);let ot=projected(derivatives[face*2u+1u].xyz,n);
    let own_flags=recipe[params.recipe0.w+face];
    var s=vec3<f32>(0.0);var t=vec3<f32>(0.0);var mag_s=0.0;var mag_t=0.0;var angles=0.0;
    // Compiler sorts current group members by prepared face, matching the CPU
    // evaluator's ordered subgroup sum. Membership tests remain dynamic here.
    for(var j=0u;j<group.y;j++){
        let member=recipe[params.recipe1.z+group.x+j];
        let flags=recipe[params.recipe0.w+member];
        let ms=derivatives[member*2u];let mt=derivatives[member*2u+1u];
        let ps=projected(ms.xyz,n);let pt=projected(mt.xyz,n);
        let cosine_s=dot(os,ps);let cosine_t=dot(ot,pt);
        let include=((own_flags|flags)&4u)!=0u || original(face)==original(member) || (angular_member(os,ps) && angular_member(ot,pt));
        if params.debug.x==i+1u {let out=params.debug.y+params.debug.z+j*4u;atomicStore(&scratch[out],bitcast<u32>(cosine_s));atomicStore(&scratch[out+1u],bitcast<u32>(cosine_t));}
        if include && params.debug.x==i+1u {atomicOr(&scratch[params.debug.y+j/32u],1u<<(j%32u));}
        if (flags&4u)!=0u {continue;}
        var corner=3u;
        for(var c=0u;c<3u;c++){if canonical(original(member)*3u+c)==representative {corner=c;break;}}
        if corner==3u {atomicOr(&scratch[0],8u);return;}
        let center=welded(member,corner).position.xyz;
        let before=welded(member,(corner+2u)%3u).position.xyz;
        let after=welded(member,(corner+1u)%3u).position.xyz;
        let a=projected(before-center,n);let b=projected(after-center,n);
        // Native GPU acos is measured against CPU f64-acos-rounded-to-f32;
        // bit parity is explicitly not assumed.
        let angle_cosine=clamp(dot(a,b),-1.0,1.0);let angle=corner_angle(a,b);
        if params.debug.x==i+1u {let out=params.debug.y+params.debug.z+j*4u;atomicStore(&scratch[out+2u],bitcast<u32>(angle_cosine));atomicStore(&scratch[out+3u],bitcast<u32>(angle));}
        if !include {continue;}
        s=s+angle*ps;t=t+angle*pt;mag_s+=angle*ms.w;mag_t+=angle*mt.w;angles+=angle;
    }
    if nonzero(s){s=unit(s);}if nonzero(t){t=unit(t);}
    if angles>0.0 {mag_s/=angles;mag_t/=angles;}
    let destination=original(face)*3u+i%3u;
    spaces[destination*2u]=vec4<f32>(s,mag_s);spaces[destination*2u+1u]=vec4<f32>(t,mag_t);
    atomicStore(&scratch[params.scratch.z+destination],group.w);
}
@compute @workgroup_size(64)
fn epilogue(@builtin(global_invocation_id) id:vec3<u32>){
    let i=id.x;if i>=params.counts.y*3u || i<params.counts.z*3u || atomicLoad(&scratch[0])!=0u{return;}
    let destination=original(i/3u)*3u+i%3u;
    let first=atomicLoad(&scratch[params.scratch.y+canonical(destination)]);
    if first==0u{return;}
    let source_prepared=0xffffffffu-first;
    let source=original(source_prepared/3u)*3u+source_prepared%3u;
    spaces[destination*2u]=spaces[source*2u];spaces[destination*2u+1u]=spaces[source*2u+1u];
    atomicStore(&scratch[params.scratch.z+destination],atomicLoad(&scratch[params.scratch.z+source]));
}
@compute @workgroup_size(64)
fn pack(@builtin(global_invocation_id) id:vec3<u32>){
    let i=id.x;if i>=params.counts.x || atomicLoad(&scratch[0])!=0u{return;}
    var result=vertices[params.prep.w+i];let owner=atomicLoad(&scratch[params.scratch.x+i]);
    result.tangent=vec4<f32>(0.0,0.0,0.0,-0.0);
    if owner!=0u {let source=owner-1u;result.tangent=vec4<f32>(spaces[source*2u].xyz,select(1.0,-1.0,atomicLoad(&scratch[params.scratch.z+source])!=0u));}
    output[i]=result;
}
