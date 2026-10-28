"""Restore the tested original/Cow workspace without changing production files."""
import io,pathlib,re,shutil,subprocess,tarfile
root=pathlib.Path(__file__).resolve().parents[3]
research=pathlib.Path(__file__).resolve().parent
work=root/'target/halfedge-validation-repro'
base='aebf742495c006735eb12ae2538c62ccac4dfbda'
work.mkdir(parents=True,exist_ok=True)
paths=['Cargo.toml','Cargo.lock','docs/reference/modifiers.tsv','crates/loom_mesh','crates/loom_shape','crates/loom_scene']
archive=subprocess.check_output(['git','archive','--format=tar',base,*paths],cwd=root)
with tarfile.open(fileobj=io.BytesIO(archive)) as files: files.extractall(work,filter='data')
s=(work/'Cargo.toml').read_text();start=s.index('members = [');end=s.index(']',start)+1
s=s[:start]+'members = ["crates/loom_mesh","crates/loom_shape","crates/loom_scene","crates/loom_mesh_cow","crates/loom_shape_cow","crates/loom_scene_cow","crates/cow_probe"]'+s[end:]
(work/'Cargo.toml').write_text(s)
for name in ['loom_mesh','loom_shape','loom_scene']:
    target=work/'crates'/(name+'_cow');shutil.copytree(work/'crates'/name,target,dirs_exist_ok=True)
    manifest=target/'Cargo.toml';s=manifest.read_text();s=re.sub(r'name\s*=\s*"'+name+'"','name = "'+name+'_cow"',s,count=1)
    for dep in ['loom_mesh','loom_shape']:
        s=re.sub(r'(?m)^'+dep+r'\s*=\s*\{ workspace = true \}',dep+' = { package="'+dep+'_cow", path="../'+dep+'_cow" }',s)
    manifest.write_text(s+'\n[lib]\nname="'+name+'"\n')
subprocess.run(['patch','-p1','--directory',str(work/'crates/loom_mesh_cow'),'--input',str(research/'prototype.patch')],check=True)
probe=work/'crates/cow_probe';(probe/'src').mkdir(parents=True,exist_ok=True)
shutil.copyfile(research/'probe.rs',probe/'src/lib.rs');shutil.copyfile(research/'probe-Cargo.toml',probe/'Cargo.toml')
print(work)
