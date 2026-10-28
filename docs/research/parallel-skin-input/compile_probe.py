"""Copy current Scene into ignored target; link exact built release dependencies."""
import json
import os
from pathlib import Path
import shutil
import subprocess

root = Path(__file__).resolve().parents[3]
research = Path(__file__).resolve().parent
output = root / 'target/parallel-skin-input-probe'
source = output / 'crates/loom_scene/src'
shutil.copytree(root / 'crates/loom_scene/src', source, dirs_exist_ok=True)
(output / 'docs/reference').mkdir(parents=True, exist_ok=True)
shutil.copyfile(root / 'docs/reference/modifiers.tsv', output / 'docs/reference/modifiers.tsv')
baseline = (research / 'baseline.rs').read_text().split('#[cfg(test)]')[0]
baseline = baseline.replace('pub fn evaluated_batch<', 'pub fn evaluated_batch_baseline<')
(source / 'skin_batch_baseline.rs').write_text(baseline)
shutil.copyfile(research / 'probe.rs', source / 'parallel_skin_input_probe.rs')
with (source / 'scene.rs').open('a') as file:
    file.write('\n#[cfg(test)]\n#[path = "skin_batch_baseline.rs"]\nmod skin_batch_baseline;\n#[cfg(test)]\n#[path = "parallel_skin_input_probe.rs"]\nmod parallel_skin_input_probe;\n')
release = root / 'target/release'

def dependency(name, expected):
    matches = [p for p in (release / '.fingerprint').glob('*/lib-' + name)
               if int.from_bytes(bytes.fromhex(p.read_text().strip()), 'little') == expected]
    assert len(matches) == 1, (name, matches)
    suffix = matches[0].parent.name.rsplit('-', 1)[1]
    library = release / 'deps' / f'lib{name}-{suffix}.rlib'
    assert library.is_file(), library
    return matches[0], library

scene_fp = max((release / '.fingerprint').glob('loom_scene-*/lib-loom_scene.json'), key=lambda p: p.stat().st_mtime_ns)
ui_fp = max((release / '.fingerprint').glob('loom_ui-*/lib-loom_ui.json'), key=lambda p: p.stat().st_mtime_ns)
args = ['rustc', '--edition=2021', '--crate-name', 'loom_scene', '--test', str(source / 'lib.rs'),
        '-C', 'opt-level=3', '-C', 'debuginfo=0', '-L', 'dependency=' + str(release / 'deps'), '-o', str(output / 'tests')]
for _, name, _, expected in json.loads(scene_fp.read_text())['deps']:
    _, library = dependency(name, expected)
    args += ['--extern', name + '=' + str(library)]
row = next(d for d in json.loads(ui_fp.read_text())['deps'] if d[1] == 'bevy_tasks')
_, library = dependency(row[1], row[3])
args += ['--extern', 'bevy_tasks=' + str(library)]
(output / 'args.json').write_text(json.dumps(args, indent=2))
env = os.environ.copy()
env.update(CARGO_MANIFEST_DIR=str(root / 'crates/loom_scene'), CARGO_PKG_NAME='loom_scene', CARGO_PKG_VERSION='0.1.0')
subprocess.run(args, cwd=root, env=env, check=True)
