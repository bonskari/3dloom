import json,math,struct,pathlib,collections
root=pathlib.Path('/tmp/loom-conditioning-capture')
def add(a,b):return [x+y for x,y in zip(a,b)]
def sub(a,b):return [x-y for x,y in zip(a,b)]
def mul(a,s):return [x*s for x in a]
def dot(a,b):return sum(x*y for x,y in zip(a,b))
def cross(a,b):return [a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]]
def length(a):return math.sqrt(dot(a,a))
def unit(a):return mul(a,1/length(a)) if any(abs(x)>1.17549435e-38 for x in a)else a

def angle(a,b):return math.atan2(length(cross(a,b)),dot(a,b)) if any(a)and any(b)else math.pi/2

def angular(a,b):return angle(a,b)<math.pi

def f32(x):return struct.unpack('f',struct.pack('f',x))[0]

def sums(terms):
 s=[0.,0.,0.];c=[0.,0.,0.];naive=[0.,0.,0.]
 for v in terms:
  for i,x in enumerate(v):
   naive[i]=f32(naive[i]+x)
   t=f32(s[i]+x)
   c[i]=f32(c[i]+(f32(f32(s[i]-t)+x) if abs(s[i])>=abs(x)else f32(f32(x-t)+s[i])))
   s[i]=t
 return naive,[f32(x+y)for x,y in zip(s,c)],[math.fsum(v[i]for v in terms)for i in range(3)]

def distance(a,b):return length(sub(unit(a),unit(b)))
reports=[]
for path in sorted(root.glob('*-*-*.json')):
 p=json.load(open(path))
 if 'members'not in p:continue
 n=p['normal'];proj=lambda v:unit(sub(v,mul(n,dot(n,v))))
 rows=[]
 for m in p['members']:
  a,b,c=m['position'];x=sub(m['uv'][1],m['uv'][0]);y=sub(m['uv'][2],m['uv'][0]);d1=sub(b,a);d2=sub(c,a);area=x[0]*y[1]-x[1]*y[0];sign=1 if area>0 else -1
  A=mul(d1,y[1]);B=mul(d2,x[1]);C=mul(d1,-y[0]);D=mul(d2,x[0]);ds=sub(A,B);dt=add(C,D)
  os=mul(unit(ds),sign)if abs(area)>1.17549435e-38 else [0]*3;ot=mul(unit(dt),sign)if abs(area)>1.17549435e-38 else [0]*3
  ps=proj(os);pt=proj(ot);q=m['position'];i=m['corner'];ea=proj(sub(q[(i+2)%3],q[i]));eb=proj(sub(q[(i+1)%3],q[i]));weight=angle(ea,eb)
  f=m['debug_floats'];gpu_ps=f[4:7];gpu_pt=f[8:11];gpu_a=f[16:19];gpu_b=f[20:23]
  raw_proj=sub(os,mul(n,dot(n,os)))
  rows.append(dict(member=m,os=os,ot=ot,ps=ps,pt=pt,weight=weight,gpu_ps=gpu_ps,gpu_pt=gpu_pt,gpu_weight=f[3],gpu_include=m['debug_words'][7]!=0,gpu_term=f[12:15],gpu_prefix=f[24:27],gpu_edges_angle=angle(gpu_a,gpu_b),deriv_condition=(length(A)+length(B))/length(ds)if length(ds)else math.inf,projection_condition=length(os)/length(raw_proj)if length(raw_proj)else math.inf,deriv_error=length(sub(m['derivatives'][0][:3],os)),projection_only_error=length(sub(gpu_ps,proj(m['derivatives'][0][:3])))))
 own=next(r for r in rows if r['member']['prepared']==p['target_prepared']);own_flags=own['member']['flags']
 for r in rows:
  m=r['member'];r['include']=bool((own_flags|m['flags'])&4)or m['original']==p['target_original_face']or(angular(own['ps'],r['ps'])and angular(own['pt'],r['pt']))
 selected=[r for r in rows if r['gpu_include']and not(r['member']['flags']&4)]
 truth_terms=[mul(r['ps'],r['weight'])for r in rows if r['include']and not(r['member']['flags']&4)]
 gpu_terms=[r['gpu_term']for r in selected]
 naive,comp,exact=sums(gpu_terms);truth=[math.fsum(v[i]for v in truth_terms)for i in range(3)];gpu_actual=p['baseline'][:3]
 def assembled(weights='gpu',directions='gpu',membership='gpu'):
  terms=[]
  for r in rows:
   inc=r['gpu_include']if membership=='gpu'else r['include']
   if not inc or r['member']['flags']&4:continue
   w=r['gpu_weight']if weights=='gpu'else r['gpu_edges_angle']if weights=='edges'else r['weight']
   d=r['gpu_ps']if directions=='gpu'else proj(r['member']['derivatives'][0][:3])if directions=='gpu_deriv'else proj(r['member']['baseline_derivatives'][0][:3])if directions=='baseline_deriv'else r['ps']
   terms.append(mul(d,w))
  return [math.fsum(v[i]for v in terms)for i in range(3)]
 normsum=length(truth);termsize=sum(length(v)for v in truth_terms)
 e=dict(node=p['node'],frame=p['frame'],time=p['time'],vertex=p['vertex'],file=path.name,group=p['group'],members=len(rows),gpu_f64=distance(gpu_actual,truth),reference_check=distance(truth,p['truth'][:3]),debug_delta=p['debug_output_delta'],baseline_delta=p['instrumented_baseline_max_delta'],membership_mismatches=sum(r['include']!=r['gpu_include']for r in rows),deriv_condition=max(r['deriv_condition']for r in rows),projection_condition=max(r['projection_condition']for r in rows),sum_condition=termsize/normsum if normsum else math.inf,sum_length=normsum,term_norms=termsize,naive_same_gpu_terms=distance(naive,truth),compensated_same_gpu_terms=distance(comp,truth),f64_same_gpu_terms=distance(exact,truth),f64_product_gpu_angle_gpu_direction=distance(assembled(),truth),f64_angle_on_gpu_edges=distance(assembled(weights='edges'),truth),f64_angle_gpu_direction=distance(assembled(weights='truth'),truth),gpu_angle_f64_direction=distance(assembled(directions='truth'),truth),f64_angle_f64_projection_gpu_derivative=distance(assembled(weights='truth',directions='gpu_deriv'),truth),derivative_error=max(r['deriv_error']for r in rows),projection_only_error=max(r['projection_only_error']for r in rows),atan2_only_error=max(abs(r['gpu_weight']-r['gpu_edges_angle'])for r in selected)if selected else 0)
 e['baseline_derivative_f64_rest']=distance(assembled(weights='truth',directions='baseline_deriv'),truth)
 e['derivative_instrumentation_max_delta']=max(length(sub(r['member']['derivatives'][0][:3],r['member']['baseline_derivatives'][0][:3]))for r in rows)
 e['target_instrumentation_delta']=length(sub(p['candidate'][:3],p['baseline'][:3]))
 e['gpu_sum_condition']=sum(length(r['gpu_term'])for r in selected)/length(selected[-1]['gpu_prefix']) if selected and length(selected[-1]['gpu_prefix'])else 1.0
 baseline_sum=assembled(weights='truth',directions='baseline_deriv')
 e['baseline_derivative_sum_condition']=sum(r['weight']*length(proj(r['member']['baseline_derivatives'][0][:3]))for r in selected)/length(baseline_sum)
 e['baseline_projection_condition']=max(length(r['member']['baseline_derivatives'][0][:3])/length(sub(r['member']['baseline_derivatives'][0][:3],mul(n,dot(n,r['member']['baseline_derivatives'][0][:3]))))for r in rows)
 e['baseline_conditioning_indicator']=2**-24*e['baseline_derivative_sum_condition']*(e['deriv_condition']+e['baseline_projection_condition']+len(selected))

 # A diagnostic conditioning heuristic, not a proved WGSL error bound.
 e['conditioning_indicator']=2**-24*e['sum_condition']*(e['deriv_condition']+e['projection_condition']+len(selected))
 reports.append(e)
(root/'analysis.json').write_text(json.dumps(reports,indent=2))
for r in sorted(reports,key=lambda r:-r['gpu_f64'])[:12]:print(json.dumps(r))
print('CAPTURE_REPORTS',len(reports),'maxreference',max((r['reference_check']for r in reports),default=0),'maxbaseline',max((r['baseline_delta']for r in reports),default=0),'maxdebug',max((r['debug_delta']for r in reports),default=0))
