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
// Current geometric upper-half-plane angle: y is cross length, x is dot.
const ATAN_COEFF:array<f64,21> = array<f64,21>(1.0lf,0.33333333333333331lf,0.20000000000000001lf,0.14285714285714285lf,0.1111111111111111lf,0.090909090909090912lf,0.076923076923076927lf,0.066666666666666666lf,0.058823529411764705lf,0.052631578947368418lf,0.047619047619047616lf,0.043478260869565216lf,0.040000000000000001lf,0.037037037037037035lf,0.034482758620689655lf,0.032258064516129031lf,0.030303030303030304lf,0.028571428571428571lf,0.027027027027027029lf,0.02564102564102564lf,0.024390243902439025lf);
fn atan_reduced(z:f64)->f64{
    let zz=z*z;
    var p=ATAN_COEFF[20];
    for(var k:i32=19;k>=0;k--){p=ATAN_COEFF[u32(k)]-zz*p;}
    return z*p;
}
fn angle_upper(y_input:f64,x:f64)->f64{
    let y=abs(y_input);
    let ax=abs(x);
    let pi:f64=3.1415926535897932384626433832795lf;
    let half:f64=1.5707963267948966192313216916398lf;
    let quarter:f64=0.78539816339744830961566084581988lf;
    if y==0.0lf && ax==0.0lf{return half;}
    let invert=y>ax;
    var r:f64;
    if invert {r=ax/y;}else{r=y/ax;}
    var z=r;var offset:f64=0.0lf;
    if r>0.4142135623730950488016887242097lf {z=(r-1.0lf)/(r+1.0lf);offset=quarter;}
    var result=offset+atan_reduced(z);
    if invert {result=half-result;}
    if x<0.0lf {result=pi-result;}
    return result;
}
// Independent current group arithmetic in native Float64. No captured terms.
fn d_dot(a:vec3<f64>,b:vec3<f64>)->f64{return a.x*b.x+a.y*b.y+a.z*b.z;}
fn d_cross(a:vec3<f64>,b:vec3<f64>)->vec3<f64>{return vec3<f64>(a.y*b.z-a.z*b.y,a.z*b.x-a.x*b.z,a.x*b.y-a.y*b.x);}
fn d_length(v:vec3<f64>)->f64{return sqrt(d_dot(v,v));}
fn d_nonzero(v:vec3<f64>)->bool{return any(abs(v)>vec3<f64>(1.1754943508222875e-38lf));}
fn d_unit(v:vec3<f64>)->vec3<f64>{if d_nonzero(v){return v/d_length(v);}return v;}
fn d_project(v:vec3<f64>,n:vec3<f64>)->vec3<f64>{return d_unit(v-d_dot(n,v)*n);}
fn d_angle(a:vec3<f64>,b:vec3<f64>)->f64{
    if all(a==vec3<f64>(0.0lf)) || all(b==vec3<f64>(0.0lf)){return 1.57079632679489661923lf;}
    return angle_upper(d_length(d_cross(a,b)),d_dot(a,b));
}
struct DSpace{s:vec3<f64>,t:vec3<f64>,mag_s:f64,mag_t:f64}
fn d_derive(f:u32)->DSpace{
    let a=welded(f,0u);let b=welded(f,1u);let c=welded(f,2u);
    let x=vec2<f64>(b.uv.xy)-vec2<f64>(a.uv.xy);let y=vec2<f64>(c.uv.xy)-vec2<f64>(a.uv.xy);
    let d1=vec3<f64>(b.position.xyz)-vec3<f64>(a.position.xyz);let d2=vec3<f64>(c.position.xyz)-vec3<f64>(a.position.xyz);
    let area=x.x*y.y-x.y*y.x;
    if abs(area)<=1.1754943508222875e-38lf{return DSpace(vec3<f64>(0.0lf),vec3<f64>(0.0lf),0.0lf,0.0lf);}
    let s=y.y*d1-x.y*d2;let t=x.x*d2-y.x*d1;
    let sign=select(f64(-1.0),1.0lf,area>0.0lf);
    return DSpace(d_unit(s)*sign,d_unit(t)*sign,d_length(s)/abs(area),d_length(t)/abs(area));
}
fn member_corner(member:u32,representative:u32)->u32{
    for(var c=0u;c<3u;c++){if canonical(original(member)*3u+c)==representative{return c;}}
    return 3u;
}
@group(0) @binding(9) var<storage,read_write> precision_risks:array<atomic<u32>>;
fn finite1(v:f32)->bool{return (bitcast<u32>(v)&0x7f800000u)!=0x7f800000u;}
fn finite2(v:vec2<f32>)->bool{return all((bitcast<vec2<u32>>(v)&vec2<u32>(0x7f800000u))!=vec2<u32>(0x7f800000u));}
fn finite3(v:vec3<f32>)->bool{return all((bitcast<vec3<u32>>(v)&vec3<u32>(0x7f800000u))!=vec3<u32>(0x7f800000u));}
fn finite4(v:vec4<f32>)->bool{return all((bitcast<vec4<u32>>(v)&vec4<u32>(0x7f800000u))!=vec4<u32>(0x7f800000u));}
fn valid_inputs(v:Vertex)->bool{return finite4(v.position)&&finite3(v.normal.xyz)&&finite2(v.uv.xy);}
// Separate risk pass leaves the original evaluate arithmetic untouched.
@compute @workgroup_size(64)
fn risk(@builtin(global_invocation_id) id:vec3<u32>){
    let gid=id.x;if gid>=params.counts.w || atomicLoad(&scratch[0])!=0u{return;}
    let g=descriptor(gid);let n=value(g.z).normal.xyz;let representative=canonical(g.z);
    if !finite3(n){atomicOr(&scratch[0],32u);return;}
    var d_condition=0.0;var p_condition=0.0;
    // A cone certificate bounds every nonnegative-weight subgroup, not final
    // tangent error. Wide/ill-conditioned/zero-weight cases keep the old path.
    let normal_squared=dot(n,n);
    var cone_ok=g.y<=256u && finite1(normal_squared) && normal_squared<=4.0;
    var axis=vec3<f32>(0.0);var axis_ready=false;
    var min_dot=3.402823466e38;var max_dot_terms=0.0;var max_s_norm=0.0;

    for(var j=0u;j<g.y;j++){
        let member=recipe[params.recipe1.z+g.x+j];let flags=recipe[params.recipe0.w+member];if (flags&4u)!=0u{continue;}
        let a=welded(member,0u);let b=welded(member,1u);let c=welded(member,2u);
        if !valid_inputs(a)||!valid_inputs(b)||!valid_inputs(c){atomicOr(&scratch[0],32u);return;}
        let x=b.uv.xy-a.uv.xy;let y=c.uv.xy-a.uv.xy;let e1=b.position.xyz-a.position.xyz;let e2=c.position.xyz-a.position.xyz;
        if !finite2(x)||!finite2(y)||!finite3(e1)||!finite3(e2){atomicStore(&precision_risks[gid],1u);return;}
        let sa=y.y*e1;let sb=x.y*e2;let ta=x.x*e2;let tb=y.x*e1;
        let ds=sa-sb;let dt=ta-tb;
        if !finite3(sa)||!finite3(sb)||!finite3(ta)||!finite3(tb)||!finite3(ds)||!finite3(dt){atomicStore(&precision_risks[gid],1u);return;}
        let ls=sqrt(dot(ds,ds));let lt=sqrt(dot(dt,dt));
        if !finite1(ls)||!finite1(lt){atomicStore(&precision_risks[gid],1u);return;}
        if ls<=1.17549435e-38 || lt<=1.17549435e-38{atomicStore(&precision_risks[gid],1u);return;}
        let cs=(length(sa)+length(sb))/ls;let ct=(length(ta)+length(tb))/lt;
        if !finite1(cs)||!finite1(ct){atomicStore(&precision_risks[gid],1u);return;}
        d_condition=max(d_condition,max(cs,ct));
        let s=derivatives[member*2u].xyz;let t=derivatives[member*2u+1u].xyz;
        if !finite3(s)||!finite3(t){atomicStore(&precision_risks[gid],1u);return;}
        let ps=s-dot(n,s)*n;let pt=t-dot(n,t)*n;
        if !finite3(ps)||!finite3(pt){atomicStore(&precision_risks[gid],1u);return;}
        let lp=sqrt(dot(ps,ps));let lq=sqrt(dot(pt,pt));
        if !finite1(lp)||!finite1(lq){atomicStore(&precision_risks[gid],1u);return;}
        if lp<=1.17549435e-38 || lq<=1.17549435e-38{atomicStore(&precision_risks[gid],1u);return;}
        let cps=length(s)/lp;let cpt=length(t)/lq;
        if !finite1(cps)||!finite1(cpt){atomicStore(&precision_risks[gid],1u);return;}
        p_condition=max(p_condition,max(cps,cpt));
        if cone_ok {
            // Keep the rounding estimate in normal-range arithmetic.
            if ls<9.094947017729282e-13 || ls>1.099511627776e12 || lt<9.094947017729282e-13 || lt>1.099511627776e12 || lp<9.094947017729282e-13 || lq<9.094947017729282e-13 {cone_ok=false;}
            if cone_ok {
                let direction=projected(s,n);
                let direction_length=length(direction);
                if !finite3(direction)||!finite1(direction_length)||direction_length==0.0 {cone_ok=false;}else{
                    if !axis_ready {axis=direction;axis_ready=true;}
                    let product_terms=abs(axis.x*direction.x)+abs(axis.y*direction.y)+abs(axis.z*direction.z);
                    let alignment=dot(axis,direction);
                    if !finite1(product_terms)||!finite1(alignment){cone_ok=false;}else{
                        min_dot=min(min_dot,alignment);max_dot_terms=max(max_dot_terms,product_terms);max_s_norm=max(max_s_norm,direction_length);
                    }
                }
                // Every non-Any target includes its own corner. Requiring a
                // positive, well-conditioned own weight rules out zero sums.
                let corner=member_corner(member,representative);
                if corner==3u {atomicOr(&scratch[0],8u);return;}
                var center=a.position.xyz;var before=c.position.xyz;var after=b.position.xyz;
                if corner==1u {center=b.position.xyz;before=a.position.xyz;after=c.position.xyz;}
                if corner==2u {center=c.position.xyz;before=b.position.xyz;after=a.position.xyz;}
                let ea=before-center;let eb=after-center;
                let pa=ea-dot(n,ea)*n;let pb=eb-dot(n,eb)*n;
                let la=length(pa);let lb=length(pb);let lea=length(ea);let leb=length(eb);
                if !finite3(ea)||!finite3(eb)||!finite3(pa)||!finite3(pb)||!finite1(la)||!finite1(lb)||!finite1(lea)||!finite1(leb)||la<9.094947017729282e-13||lb<9.094947017729282e-13||la>1.099511627776e12||lb>1.099511627776e12 {cone_ok=false;}else{
                    let edge_condition=max(lea/la,leb/lb);
                    let weight=corner_angle(projected(ea,n),projected(eb,n));
                    if !finite1(edge_condition)||edge_condition>4.0||!finite1(weight)||weight<0.001*(1.0+normal_squared){cone_ok=false;}
                }
            }
        }

    }

    if cone_ok && axis_ready {
        let eps=1.1920928955078125e-7;
        // Covers dot/norm rounding plus projection amplification; eligibility
        // excludes subnormal/wide intermediate ranges and ill-conditioned edges.
        let margin=256.0*eps*max(1.0,max_dot_terms)*(1.0+p_condition*(1.0+normal_squared));
        let axis_upper=length(axis)+margin;let dot_lower=min_dot-margin;
        let n_eps=(f32(g.y)+8.0)*eps;
        if finite1(margin)&&margin<0.25&&finite1(axis_upper)&&axis_upper>0.0&&dot_lower>0.0&&n_eps<0.015625 {
            let gamma=n_eps/(1.0-n_eps);
            let denominator=dot_lower/axis_upper-4.0*gamma*(max_s_norm+margin);
            if finite1(denominator)&&denominator>0.0 {
                let k_upper=(1.0+gamma)/denominator;
                let indicator_upper=5.960464477539063e-8*k_upper*(d_condition+p_condition+f32(g.y))*(1.0+64.0*eps);
                // This guard band only controls shortcut eligibility. The full
                // classifier's 2e-5 precision threshold below is unchanged.
                if finite1(indicator_upper)&&indicator_upper<1.0e-5 {return;}
            }
        }
    }
    for(var k=0u;k<g.y;k++){
        let face=recipe[params.recipe1.z+g.x+k];let own_flags=recipe[params.recipe0.w+face];
        let os=projected(derivatives[face*2u].xyz,n);let ot=projected(derivatives[face*2u+1u].xyz,n);
        var sum=vec3<f32>(0.0);var angles=0.0;var contributing=0.0;
        for(var j=0u;j<g.y;j++){
            let member=recipe[params.recipe1.z+g.x+j];let flags=recipe[params.recipe0.w+member];if (flags&4u)!=0u{continue;}
            let ps=projected(derivatives[member*2u].xyz,n);let pt=projected(derivatives[member*2u+1u].xyz,n);
            let include=((own_flags|flags)&4u)!=0u || original(face)==original(member) || (angular_member(os,ps)&&angular_member(ot,pt));
            if !include{continue;}
            let corner=member_corner(member,representative);if corner==3u{atomicOr(&scratch[0],8u);return;}
            let center=welded(member,corner).position.xyz;
            let ea=welded(member,(corner+2u)%3u).position.xyz-center;let eb=welded(member,(corner+1u)%3u).position.xyz-center;
            let pa=ea-dot(n,ea)*n;let pb=eb-dot(n,eb)*n;
            let aa=dot(pa,pa);let bb=dot(pb,pb);
            if !finite3(ea)||!finite3(eb)||!finite3(pa)||!finite3(pb)||!finite1(aa)||!finite1(bb)||aa==0.0||bb==0.0{atomicStore(&precision_risks[gid],1u);return;}
            let a=projected(ea,n);let b=projected(eb,n);
            let angle=corner_angle(a,b);
            if !finite3(a)||!finite3(b)||!finite1(angle){atomicStore(&precision_risks[gid],1u);return;}
            sum+=angle*ps;angles+=angle;contributing+=1.0;
            if !finite3(sum)||!finite1(angles){atomicStore(&precision_risks[gid],1u);return;}
        }
        if contributing==0.0{continue;}
        let magnitude=sqrt(dot(sum,sum));
        if !finite1(magnitude){atomicStore(&precision_risks[gid],1u);return;}
        if magnitude<=1.17549435e-38{atomicStore(&precision_risks[gid],1u);return;}
        let indicator=5.960464477539063e-8*(angles/magnitude)*(d_condition+p_condition+contributing);
        if !finite1(indicator)||indicator>=2.0e-5{atomicStore(&precision_risks[gid],1u);return;}
    }
}
@compute @workgroup_size(64)
fn refine_group(@builtin(global_invocation_id) id:vec3<u32>){
    let gid=id.x;if gid>=params.counts.w || atomicLoad(&scratch[0])!=0u || atomicLoad(&precision_risks[gid])==0u{return;}
    let g=descriptor(gid);let n=vec3<f64>(value(g.z).normal.xyz);let representative=canonical(g.z);
    for(var k=0u;k<g.y;k++){
        let face=recipe[params.recipe1.z+g.x+k];let own_flags=recipe[params.recipe0.w+face];
        let own=d_derive(face);let os=d_project(own.s,n);let ot=d_project(own.t,n);
        var s=vec3<f64>(0.0lf);var t=vec3<f64>(0.0lf);var mag_s:f64=0.0lf;var mag_t:f64=0.0lf;var angles:f64=0.0lf;
        for(var j=0u;j<g.y;j++){
            let member=recipe[params.recipe1.z+g.x+j];let flags=recipe[params.recipe0.w+member];
            let d=d_derive(member);let ps=d_project(d.s,n);let pt=d_project(d.t,n);
            let include=((own_flags|flags)&4u)!=0u || original(face)==original(member) || (d_angle(os,ps)<3.14159265358979323846lf && d_angle(ot,pt)<3.14159265358979323846lf);
            if !include || (flags&4u)!=0u{continue;}
            let corner=member_corner(member,representative);if corner==3u{atomicOr(&scratch[0],8u);return;}
            let center=vec3<f64>(welded(member,corner).position.xyz);
            let a=d_project(vec3<f64>(welded(member,(corner+2u)%3u).position.xyz)-center,n);let b=d_project(vec3<f64>(welded(member,(corner+1u)%3u).position.xyz)-center,n);
            let angle=d_angle(a,b);s+=angle*ps;t+=angle*pt;mag_s+=angle*d.mag_s;mag_t+=angle*d.mag_t;angles+=angle;
        }
        if angles>0.0lf{mag_s/=angles;mag_t/=angles;}
        let corner=member_corner(face,representative);if corner==3u{atomicOr(&scratch[0],8u);return;}
        let destination=original(face)*3u+corner;
        spaces[destination*2u]=vec4<f32>(vec3<f32>(d_unit(s)),f32(mag_s));spaces[destination*2u+1u]=vec4<f32>(vec3<f32>(d_unit(t)),f32(mag_t));
        // Original evaluate already stored the validated group's handedness.
    }
}

@compute @workgroup_size(64)
fn validate_output(@builtin(global_invocation_id) id:vec3<u32>){
    let i=id.x;if i>=params.counts.x || atomicLoad(&scratch[0])!=0u{return;}
    let v=output[i];
    if !finite4(v.tangent)||!valid_inputs(v){atomicOr(&scratch[0],32u);}
}
