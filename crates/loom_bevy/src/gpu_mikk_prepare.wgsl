// Test-only current Mikk preparation. Corrected undirected edge buckets are
// processed in prepared-face order, preserving nonmanifold opposing pairing.
struct Vertex { position: vec4<f32>, normal: vec4<f32>, tangent: vec4<f32>, uv: vec4<f32> }
struct Params { counts: vec4<u32>, offsets0: vec4<u32>, offsets1: vec4<u32> }
@group(0) @binding(0) var<storage,read> vertices: array<Vertex>;
@group(0) @binding(1) var<storage,read> triangles: array<vec4<u32>>;
@group(0) @binding(2) var<storage,read_write> weld_table: array<atomic<u32>>;
@group(0) @binding(3) var<storage,read_write> edge_table: array<atomic<u32>>;
@group(0) @binding(4) var<storage,read_write> arena: array<atomic<u32>>;
@group(0) @binding(5) var<uniform> params: Params;
fn current(corner: u32) -> Vertex { return vertices[triangles[corner / 3u][corner % 3u]]; }
fn bits(v: f32) -> u32 { return select(bitcast<u32>(v),0u,v == 0.0); }
fn mix(h: u32,v: u32) -> u32 { return (h ^ v) * 16777619u; }
fn key_hash(v: Vertex) -> u32 {
    var h = 2166136261u;
    for(var i=0u;i<3u;i++){h=mix(h,bits(v.position[i]));}
    for(var i=0u;i<3u;i++){h=mix(h,bits(v.normal[i]));}
    for(var i=0u;i<2u;i++){h=mix(h,bits(v.uv[i]));}
    return h;
}
fn same(a: Vertex,b: Vertex) -> bool {
    return all(a.position.xyz==b.position.xyz) && all(a.normal.xyz==b.normal.xyz) && all(a.uv.xy==b.uv.xy);
}
@compute @workgroup_size(64)
fn weld(@builtin(global_invocation_id) id: vec3<u32>) {
    let corner=id.x;
    if corner>=params.counts.y*3u{return;}
    let value=current(corner);
    if !all(abs(value.position.xyz)<=vec3<f32>(3.402823466e38)) || !all(abs(value.normal.xyz)<=vec3<f32>(3.402823466e38)) || !all(abs(value.uv.xy)<=vec2<f32>(3.402823466e38)) {atomicAdd(&arena[1],1u);}
    var slot=key_hash(value)&params.counts.z;
    loop {
        let candidate=atomicCompareExchangeWeak(&weld_table[slot],0u,corner+1u);
        if candidate.exchanged {break;}
        if candidate.old_value!=0u && same(value,current(candidate.old_value-1u)) {
            atomicMin(&weld_table[slot],corner+1u);break;
        }
        // Weak CAS may fail spuriously with old_value zero: retry same slot.
        if candidate.old_value!=0u {slot=(slot+1u)&params.counts.z;}
    }
    atomicStore(&arena[params.offsets0.x+corner],slot);
}
@compute @workgroup_size(64)
fn canonical(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x>=params.counts.y*3u{return;}
    let slot=atomicLoad(&arena[params.offsets0.x+id.x]);
    atomicStore(&arena[params.offsets0.y+id.x],atomicLoad(&weld_table[slot])-1u);
}
// Exact triangle-only DegenPrologue ordering. This serial stage is research,
// has no timing claim, and preserves all degenerate triangles in the tail.
@compute @workgroup_size(1)
fn degen_prologue() {
    var good_count=0u;
    for(var t=0u;t<params.counts.y;t++){
        let a=current(t*3u).position.xyz;let b=current(t*3u+1u).position.xyz;let c=current(t*3u+2u).position.xyz;
        let degen=all(a==b)||all(a==c)||all(b==c);
        atomicStore(&arena[params.offsets0.z+t],t);
        atomicStore(&arena[params.offsets0.w+t],select(0u,1u,degen));
        good_count+=select(1u,0u,degen);
    }
    atomicStore(&arena[0],good_count);
    var next=1u;
    for(var t=0u;t<good_count;t++){
        let original=atomicLoad(&arena[params.offsets0.z+t]);
        if atomicLoad(&arena[params.offsets0.w+original])==0u {next=max(next,t+2u);}
        else {
            loop {
                if next>=params.counts.y {return;}
                let found=atomicLoad(&arena[params.offsets0.z+next]);
                if atomicLoad(&arena[params.offsets0.w+found])==0u {break;}
                next++;
            }
            let found=atomicLoad(&arena[params.offsets0.z+next]);
            atomicStore(&arena[params.offsets0.z+t],found);
            atomicStore(&arena[params.offsets0.z+next],original);
            next++;
        }
    }
}
fn not_zero(value:f32)->bool{return abs(value)>1.17549435e-38;}
@compute @workgroup_size(64)
fn flags(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x>=atomicLoad(&arena[0]){return;}
    let t=atomicLoad(&arena[params.offsets0.z+id.x]);
    let a=current(t*3u);let b=current(t*3u+1u);let c=current(t*3u+2u);
    let x=b.uv.xy-a.uv.xy;let y=c.uv.xy-a.uv.xy;
    let d1=b.position.xyz-a.position.xyz;let d2=c.position.xyz-a.position.xyz;
    let area=x.x*y.y-x.y*y.x;
    var value=4u|select(0u,8u,area>0.0);
    if not_zero(area){
        let os=y.y*d1-x.y*d2;let ot=(-y.x)*d1+x.x*d2;
        if not_zero(length(os)/abs(area)) && not_zero(length(ot)/abs(area)){value=value&~4u;}
    }
    atomicStore(&arena[params.offsets0.w+t],value);
}
fn endpoints(edge:u32)->vec2<u32>{
    let original=atomicLoad(&arena[params.offsets0.z+edge/3u]);
    return vec2<u32>(atomicLoad(&arena[params.offsets0.y+original*3u+edge%3u]),atomicLoad(&arena[params.offsets0.y+original*3u+(edge%3u+1u)%3u]));
}
@compute @workgroup_size(64)
fn edges(@builtin(global_invocation_id) id:vec3<u32>){
    let edge=id.x;if edge>=atomicLoad(&arena[0])*3u{return;}
    let ends=endpoints(edge);let key=vec2<u32>(min(ends.x,ends.y),max(ends.x,ends.y));
    var slot=mix(mix(2166136261u,key.x),key.y)&params.counts.w;
    loop {
        let candidate=atomicCompareExchangeWeak(&edge_table[slot*2u],0u,edge+1u);
        if candidate.exchanged {break;}
        if candidate.old_value!=0u {
            let other=endpoints(candidate.old_value-1u);
            if all(key==vec2<u32>(min(other.x,other.y),max(other.x,other.y))) {atomicMin(&edge_table[slot*2u],edge+1u);break;}
            slot=(slot+1u)&params.counts.w;
        }
    }
    atomicStore(&arena[params.offsets1.x+edge],slot);
    let previous=atomicExchange(&edge_table[slot*2u+1u],edge+1u);
    atomicStore(&arena[params.offsets1.y+edge],previous);
}
@compute @workgroup_size(64)
fn pair(@builtin(global_invocation_id) id:vec3<u32>){
    let edge=id.x;if edge>=atomicLoad(&arena[0])*3u{return;}
    let slot=atomicLoad(&arena[params.offsets1.x+edge]);
    if atomicLoad(&edge_table[slot*2u])!=edge+1u{return;}
    let head=atomicLoad(&edge_table[slot*2u+1u]);
    loop {
        var first=0xffffffffu;var item=head;
        loop {if item==0u{break;}let candidate=item-1u;
            if atomicLoad(&arena[params.offsets1.z+candidate])==0u {first=min(first,candidate);}
            item=atomicLoad(&arena[params.offsets1.y+candidate]);
        }
        if first==0xffffffffu{break;}
        atomicStore(&arena[params.offsets1.z+first],1u);
        let direction=endpoints(first);var opposite=0xffffffffu;item=head;
        loop {if item==0u{break;}let candidate=item-1u;
            if atomicLoad(&arena[params.offsets1.z+candidate])==0u && all(endpoints(candidate)==direction.yx){opposite=min(opposite,candidate);}
            item=atomicLoad(&arena[params.offsets1.y+candidate]);
        }
        if opposite!=0xffffffffu {
            atomicStore(&arena[params.offsets1.z+opposite],1u);
            atomicStore(&arena[params.offsets1.w+first],opposite/3u+1u);
            atomicStore(&arena[params.offsets1.w+opposite],first/3u+1u);
        }
    }
}
