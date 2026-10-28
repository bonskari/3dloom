import pathlib,json,subprocess,os
root=pathlib.Path('/home/b1s/dev/github/3dloom');release=root/'target/release';directory=root/'target/autosave-ray-probe'
app_path=max((release/'.fingerprint').glob('loom_app-*/bin-loom.json'),key=lambda p:p.stat().st_mtime);app=json.loads(app_path.read_text())
def dependency(name,expected):
 paths=[p for p in(release/'.fingerprint').glob('*/lib-'+name)if int.from_bytes(bytes.fromhex(p.read_text().strip()),'little')==expected]
 assert len(paths)==1,(name,paths)
 suffix=paths[0].parent.name.rsplit('-',1)[1];library=release/'deps'/f'lib{name}-{suffix}.rlib';assert library.is_file();return paths[0],library
rows={d[1]:d for d in app['deps']};command_path,_=dependency('loom_commands',rows['loom_commands'][3]);commands=json.loads(command_path.with_suffix('.json').read_text());rows.update({d[1]:d for d in commands['deps']})
args=['rustc','--edition=2021','--crate-name','autosave_ray_probe',str(directory/'probe.rs'),'-C','opt-level=3','-C','codegen-units=4','-L','dependency='+str(release/'deps'),'-o',str(directory/'probe')]
for name in['loom_commands','loom_scene','serde_json','serde','sha2']:
 _,library=dependency(name,rows[name][3]);args+=['--extern',name+'='+str(library)]
for path in(release/'build').glob('*/out'):
 if path.is_dir():args+=['-L','native='+str(path)]
env=os.environ.copy();env['PROBE_ROOT']=str(directory/'homes');(directory/'args.json').write_text(json.dumps(args,indent=2));(directory/'fingerprint.txt').write_text(str(app_path)+'\n'+str(command_path)+'\n');print('COMPILE_RELEASE_PROBE',flush=True);subprocess.run(args,cwd=root,env=env,check=True);print('COMPILE_OK',flush=True)
