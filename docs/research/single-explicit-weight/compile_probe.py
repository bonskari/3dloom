import pathlib,shutil,json,subprocess,os
root=pathlib.Path(__file__).resolve().parents[3];research=pathlib.Path(__file__).resolve().parent
out=root/'target/single-explicit-weight';src=out/'crates/loom_scene/src'
shutil.copytree(root/'crates/loom_scene/src',src,dirs_exist_ok=True)
(out/'docs/reference').mkdir(parents=True,exist_ok=True)
shutil.copyfile(root/'docs/reference/modifiers.tsv',out/'docs/reference/modifiers.tsv')
subprocess.run(['patch','-p1','--directory',str(src),'--input',str(research/'prototype.patch')],check=True)
shutil.copyfile(research/'probe.rs',src/'single_probe.rs')
release=root/'target/release';bevy=json.loads((release/'.fingerprint/loom_bevy-d37bfa3ba707912c/test-lib-loom_bevy.json').read_text())
def dep(name,expected):
 paths=[p for p in (release/'.fingerprint').glob('*/lib-'+name) if int.from_bytes(bytes.fromhex(p.read_text().strip()),'little')==expected];assert len(paths)==1,(name,paths);suf=paths[0].parent.name.rsplit('-',1)[1];return paths[0],release/'deps'/f'lib{name}-{suf}.rlib'
row=next(d for d in bevy['deps'] if d[1]=='loom_scene');fp,_=dep(row[1],row[3]);f=json.loads(fp.with_suffix('.json').read_text());args=['rustc','--edition=2021','--crate-name','loom_scene','--test',str(src/'lib.rs'),'-C','opt-level=3','-C','debuginfo=0','-L','dependency='+str(release/'deps'),'-o',str(out/'tests')]
for _,name,_,expected in f['deps']:
 _,lib=dep(name,expected);args+=['--extern',name+'='+str(lib)]
row=next(d for d in bevy['deps'] if d[1]=='bevy_tasks');_,lib=dep(row[1],row[3]);args+=['--extern','bevy_tasks='+str(lib)]
env=os.environ.copy();env.update(CARGO_MANIFEST_DIR=str(root/'crates/loom_scene'),CARGO_PKG_VERSION='0.1.0',CARGO_PKG_NAME='loom_scene');(out/'args.json').write_text(json.dumps(args,indent=2));subprocess.run(args,cwd=root,env=env,check=True)
