import json,pathlib,subprocess,os,shutil
root=pathlib.Path(__file__).resolve().parents[3];release=root/'target/release';directory=root/'target/effective-weights-probe'
research=pathlib.Path(__file__).resolve().parent
source=directory/'crates/loom_scene/src'
shutil.copytree(root/'crates/loom_scene/src',source,dirs_exist_ok=True)
reference=directory/'docs/reference';reference.mkdir(parents=True,exist_ok=True)
shutil.copyfile(root/'docs/reference/modifiers.tsv',reference/'modifiers.tsv')
shutil.copyfile(research/'probe.rs',source/'effective_weights_stage_tests.rs')
shutil.copyfile(research/'cache.rs',source/'effective_weights_probe.rs')
subprocess.run(['patch','-p1','--directory',str(source),'--input',str(research/'prototype.patch')],check=True)
bevy=json.loads((release/'.fingerprint/loom_bevy-d37bfa3ba707912c/test-lib-loom_bevy.json').read_text())
def dependency(name,expected):
    paths=[p for p in (release/'.fingerprint').glob('*/lib-'+name) if int.from_bytes(bytes.fromhex(p.read_text().strip()),'little')==expected]
    assert len(paths)==1,(name,paths)
    suffix=paths[0].parent.name.rsplit('-',1)[1]
    library=release/'deps'/f'lib{name}-{suffix}.rlib';assert library.is_file()
    return paths[0],library
scene_row=next(d for d in bevy['deps'] if d[1]=='loom_scene')
scene_path,_=dependency(scene_row[1],scene_row[3]);fingerprint=json.loads(scene_path.with_suffix('.json').read_text())
args=['rustc','--edition=2021','--crate-name','loom_scene','--test',str(directory/'crates/loom_scene/src/lib.rs'),'-C','opt-level=3','-C','debuginfo=0','-L','dependency='+str(release/'deps'),'-o',str(directory/'loom_scene_tests')]
for _,name,_,expected in fingerprint['deps']:
    _,library=dependency(name,expected);args+=['--extern',name+'='+str(library)]
row=next(d for d in bevy['deps'] if d[1]=='bevy_tasks');_,library=dependency(row[1],row[3]);args+=['--extern','bevy_tasks='+str(library)]
env=os.environ.copy();env.update(CARGO_MANIFEST_DIR=str(root/'crates/loom_scene'),CARGO_PKG_VERSION='0.1.0',CARGO_PKG_NAME='loom_scene')
(directory/'args.json').write_text(json.dumps(args,indent=2));print('COMPILING',flush=True)
subprocess.run(args,cwd=root,env=env,check=True);print('COMPILED',flush=True)
