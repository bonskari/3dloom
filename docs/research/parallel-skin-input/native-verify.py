"""Verify the installed Ray scene, viewport pixels, direct FK edit and Undo.

Arguments: expected binary commit, prior-version screenshot, new artifact directory.
Only filtered results/screenshots are written, never State or local preferences.
"""
import hashlib
import json
from pathlib import Path
import socket
import sys

from PIL import Image, ImageChops


def call(command, **args):
    with socket.socket(socket.AF_UNIX) as sock:
        sock.settimeout(30)
        sock.connect('/run/user/1000/3dloom/current')
        sock.sendall((json.dumps({'cmd': command, 'args': args}) + '\n').encode())
        result = json.loads(sock.makefile().readline())
    assert result.get('ok'), result
    return result


def emit(value):
    print(json.dumps(value), flush=True)


def input_event(**event):
    call('input', events=[event])


ping = call('ping')
assert ping['commit'] == sys.argv[1], 'wrong installed binary'
before = call('State')
assert before['frame'] == 39 and not before['dirty']
assert not before['animation']['playing'] and not before['animation']['auto_key']
assert before['selection']['selected'] == [4988] and not before['undo_labels']
source = Path(before['file'])
digest = hashlib.sha256(source.read_bytes()).hexdigest()
assert digest == '84fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935'
output = Path(sys.argv[3])
output.mkdir(exist_ok=False)
rotated = False
try:
    input_event(type='leave')
    call('screenshot', path=str(output / 'default.png'))
    gpu = call('diagnostics')['playback']['gpu_pose']
    assert (gpu['committed_objects'], gpu['committed_parts']) == (5, 13)
    assert gpu['last_error'] is None and gpu['recovered_recipes'] == 0
    prior = Image.open(sys.argv[2]).convert('RGB')
    current = Image.open(output / 'default.png').convert('RGB')
    assert prior.size == current.size == (1920, 1200)
    # All four viewport interiors; excludes menus, command panel and status/timeline.
    crop = (0, 74, 1515, 1043)
    assert ImageChops.difference(prior.crop(crop), current.crop(crop)).getbbox() is None, 'viewport pixels changed'
    emit({'kind': 'viewport_pixels', 'all_four_identical': True, 'frame': 39})
    input_event(type='move', pos=[757, 210])
    input_event(type='zoom', factor=2.5)
    input_event(type='click', pos=[720, 229], button='left')
    assert call('State')['selection']['selected'] == [4975], 'direct limb click missed'
    call('Select and Rotate')
    input_event(type='drag', **{'from': [725, 142], 'to': [758, 151], 'button': 'left', 'steps': 8})
    after = call('State')
    old_nodes = {node['id']: node for node in before['nodes']}
    changed = [node['id'] for node in after['nodes'] if old_nodes[node['id']] != node]
    assert changed == [4975] and after['dirty'], ('unexpected edit', changed)
    rotated = True
    call('screenshot', path=str(output / 'rotated.png'))
    emit({'kind': 'direct_fk_edit', 'selected': 4975, 'changed_node_ids': changed})
    call('Undo Scene Operation')
    rotated = False
    restored = call('State')
    assert restored['nodes'] == before['nodes'] and not restored['dirty'], 'Undo did not restore all nodes'
    emit({'kind': 'undo', 'all_node_records_restored': True})
finally:
    state = call('State')
    if not rotated and state['nodes'] == before['nodes'] and not state['dirty'] and state['frame'] == before['frame'] and state['file'] == before['file'] and state['selection']['selected'] in [[4988], [4975]]:
        call('Open File', path=before['file'])
        restored = call('State')
        assert restored['nodes'] == before['nodes'] and restored['selection'] == before['selection']
        assert restored['frame'] == before['frame'] and not restored['dirty'] and not restored['undo_labels']
        assert hashlib.sha256(source.read_bytes()).hexdigest() == digest
        emit({'kind': 'restored', 'nodes_selection_frame_undo_source_preserved': True})
    else:
        emit({'kind': 'restore_skipped', 'unresolved_edit_or_possible_human_takeover': True})
        raise RuntimeError('native verification left an unresolved edit or detected a possible takeover')
