import json,pathlib,subprocess,os,shutil
root=pathlib.Path(__file__).resolve().parents[3];release=root/'target/release'
fingerprint=json.loads((release/'.fingerprint/loom_bevy-d37bfa3ba707912c/test-lib-loom_bevy.json').read_text())
directory=root/'target/rigid-source-probe';binary=directory/'loom_bevy_tests'
shutil.copytree(root/'crates/loom_bevy/src',directory/'src',dirs_exist_ok=True)
shutil.copyfile(pathlib.Path(__file__).with_name('probe.rs'),directory/'src/rigid_source_probe.rs')
with (directory/'src/lib.rs').open('a') as f:f.write('\n#[cfg(test)] mod rigid_source_probe;\n')
args=['rustc','--edition=2021','--crate-name','loom_bevy','--test',str(directory/'src/lib.rs'),'-C','opt-level=3','-C','debuginfo=0','-L','dependency='+str(release/'deps'),'-o',str(binary)]
for _,name,_,expected in fingerprint['deps']:
 matches=[]
 for path in (release/'.fingerprint').glob('*/lib-'+name):
  value=path.read_text().strip()
  if int.from_bytes(bytes.fromhex(value),'little')==expected:matches.append(path)
 if len(matches)!=1:raise RuntimeError((name,expected,matches))
 suffix=matches[0].parent.name.rsplit('-',1)[1];library=release/'deps'/f'lib{name}-{suffix}.rlib'
 if not library.is_file():raise RuntimeError(library)
 args+=['--extern',name+'='+str(library)]
env=os.environ.copy();env.update(CARGO_MANIFEST_DIR=str(root/'crates/loom_bevy'),CARGO_PKG_VERSION='0.1.0',CARGO_PKG_NAME='loom_bevy')
(directory/'args.json').write_text(json.dumps(args,indent=2));print('DIRECT_BINARY='+str(binary),flush=True)
result=subprocess.run(args,cwd=root,env=env)
if result.returncode:raise SystemExit(result.returncode)
print('DIRECT_COMPILE_OK',flush=True)
