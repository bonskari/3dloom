import glob,json,os,subprocess,pathlib
root=pathlib.Path('/home/b1s/dev/github/3dloom');os.chdir(root)
f=max(glob.glob('target/release/.fingerprint/loom_bevy-*/lib-loom_bevy.json'),key=os.path.getmtime)
def resolve(name, expected):
 for p in glob.glob('target/release/.fingerprint/'+name+'-*/lib-'+name):
  raw=open(p).read()
  if int(raw,16)==expected or int.from_bytes(bytes.fromhex(raw),'little')==expected:
   suffix=pathlib.Path(p).parent.name[len(name)+1:];return root/('target/release/deps/lib'+name+'-'+suffix+'.rlib'), p+'.json'
 raise Exception((name,expected))
deps={n:resolve(n,v) for _,n,_,v in json.load(open(f))['deps']}
scene=json.load(open(deps['loom_scene'][1]));j=next(d for d in scene['deps'] if d[1]=='serde_json');deps['serde_json']=resolve(j[1],j[3])
deps['pollster']=resolve('pollster',7443006572381805701)
args=['nice','-n','10','rustc','--edition=2021','-C','opt-level=3','-C','codegen-units=4','/tmp/loom_gpu_edge_probe/probe.rs','-o','/tmp/loom_gpu_edge_probe/probe','-L',str(root/'target/release/deps')]
for name,(p,_) in deps.items():args+=['--extern',name+'='+str(p)]
env=os.environ.copy();env['CARGO_MANIFEST_DIR']=str(root/'crates/loom_bevy');env['CARGO_PKG_NAME']='loom_async_probe'
open('/tmp/loom_async_probe/build-args.json','w').write(json.dumps(args,indent=2))
subprocess.run(args,env=env,check=True)
