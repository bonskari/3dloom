import pathlib,json,subprocess,hashlib
root=pathlib.Path('/home/b1s/dev/github/3dloom');r=root/'target/release';d=root/'target/material-id-bitset-probe'
app=max((r/'.fingerprint').glob('loom_app-*/bin-loom.json'),key=lambda p:p.stat().st_mtime)
def dep(name,value):
 ps=[p for p in (r/'.fingerprint').glob('*/lib-'+name) if int.from_bytes(bytes.fromhex(p.read_text().strip()),'little')==value];assert len(ps)==1,(name,ps)
 p=ps[0];return p,r/'deps'/('lib'+name+'-'+p.parent.name.rsplit('-',1)[1]+'.rlib')
rows={a[1]:a for a in json.loads(app.read_text())['deps']};ui,_=dep('loom_ui',rows['loom_ui'][3]);rows.update({a[1]:a for a in json.loads(ui.with_suffix('.json').read_text())['deps']})
cp,_=dep('loom_commands',rows['loom_commands'][3]);rows.update({a[1]:a for a in json.loads(cp.with_suffix('.json').read_text())['deps']})
args=['rustc','--edition=2021',str(d/'probe.rs'),'-C','opt-level=3','-C','codegen-units=4','-L','dependency='+str(r/'deps'),'-o',str(d/'probe')]
for name in ['loom_ui','loom_commands','loom_scene','eframe','glam','serde_json','sha2','loom_mesh']:
 p,lib=dep(name,rows[name][3]);assert lib.is_file();args+=['--extern',name+'='+str(lib)]
for p in (r/'build').glob('*/out'):args+=['-L','native='+str(p)]
(d/'args.json').write_text(json.dumps(args,indent=2));subprocess.run(args,cwd=root,check=True)
