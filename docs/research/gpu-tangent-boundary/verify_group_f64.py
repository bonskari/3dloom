import json,math,sys
p=json.load(open(sys.argv[1] if len(sys.argv)>1 else '/tmp/loom-gpu-hybrid-worst.json'))
def add(a,b):return tuple(x+y for x,y in zip(a,b))
def sub(a,b):return tuple(x-y for x,y in zip(a,b))
def mul(a,b):return tuple(x*b for x in a)
def dot(a,b):return sum(x*y for x,y in zip(a,b))
def length(a):return math.sqrt(dot(a,a))
def unit(a):return mul(a,1/length(a)) if any(abs(x)>1.17549435e-38 for x in a) else a
for normalize_normal in [False,True]:
 n=tuple(p['normal']);n=unit(n) if normalize_normal else n
 def projected(v):return unit(sub(v,mul(n,dot(n,v))))
 rows=[]
 for m in p['members']:
  a,b,c=map(tuple,m['position']);uv=m['uv'];x=sub(uv[1],uv[0]);y=sub(uv[2],uv[0]);d1=sub(b,a);d2=sub(c,a);area=x[0]*y[1]-x[1]*y[0]
  os=sub(mul(d1,y[1]),mul(d2,x[1]));ot=add(mul(d1,-y[0]),mul(d2,x[0]));ls=length(os);lt=length(ot)
  sign=1 if area>0 else -1
  os=mul(os,sign/ls) if ls>1.17549435e-38 and abs(area)>1.17549435e-38 else (0,0,0)
  ot=mul(ot,sign/lt) if lt>1.17549435e-38 and abs(area)>1.17549435e-38 else (0,0,0)
  rows.append((m,projected(os),projected(ot),ls/abs(area) if area else 0,lt/abs(area) if area else 0))
 own=next(r for r in rows if r[0]['original']==p['target_original_face'])
 s=t=(0,0,0);ms=mt=angles=0;selected=[]
 for m,os,ot,fs,ft in rows:
  include=(m['flags']|own[0]['flags'])&4 or m['original']==own[0]['original'] or (dot(os,own[1])>-1 and dot(ot,own[2])>-1)
  if include:selected.append(m['prepared'])
  if not include or m['flags']&4:continue
  corner=next(i for i,v in enumerate(m['position']) if v==p['representative_position'])
  center=m['position'][corner];a=projected(sub(m['position'][(corner+2)%3],center));b=projected(sub(m['position'][(corner+1)%3],center))
  angle=math.acos(max(-1,min(1,dot(a,b))));s=add(s,mul(os,angle));t=add(t,mul(ot,angle));ms+=angle*fs;mt+=angle*ft;angles+=angle
 s=unit(s);t=unit(t)
 print('F64 normalize_input_normal',normalize_normal,'node',p['node'],'time',p['time'],'part',p['part'],'vertex',p['vertex'],'normal_length',length(p['normal']))
 print('members',selected,'s',s,'t',t,'magnitudes',ms/angles if angles else 0,mt/angles if angles else 0)
 print('gpu_error',length(sub(s,p['gpu'][:3])),'cpu_error',length(sub(s,p['cpu'][:3])),'gpu',p['gpu'],'cpu',p['cpu'])
