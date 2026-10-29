"""Native buffered FK authoring on the unchanged original Ray scene.

Run after the long stability process has finished. Uses the app's own IPC,
never desktop input. Keeps real scene copies/images, never State/preferences.
The fixed input geometry belongs to the retained1920x1200 physical screenshot
and1280x800 logical layout; unexpected window placement stops the test.
"""
import hashlib
import json
from pathlib import Path
import socket
import sys
from PIL import Image, ImageChops

SOURCE=Path('/mnt/2tb/hollowreach-ue/Hollowreach/SourceArt/RayLoom20261025/RayAnimationTest_LoomRig.loom')
DIGEST='84fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935'

def call(cmd,**args):
    with socket.socket(socket.AF_UNIX) as sock:
        sock.settimeout(30)
        sock.connect('/run/user/1000/3dloom/current')
        sock.sendall((json.dumps({'cmd':cmd,'args':args})+'\n').encode())
        result=json.loads(sock.makefile().readline())
    assert result.get('ok'),result.get('error')
    return result

def event(**value):call('input',events=[value])
def emit(value):print(json.dumps(value),flush=True)
def shot(name):
    event(type='leave')
    call('screenshot',path=str(output/(name+'.png')))
def snapshot(name):
    path=output/(name+'.loom')
    call('Save File As Copy',path=str(path))
    return json.loads(path.read_text())['scene']

assert not Path('/proc/859214').exists(),'the recorded stability process is still live'
assert call('ping')['commit']=='eb7b2f096b213cacaa96dd1ee9f5b525096376db'
before=call('State')
assert before['file']==str(SOURCE) and before['frame']==39
assert not before['dirty'] and not before['undo_labels']
assert before['selection']['selected']==[4988]
assert not before['animation']['playing'] and not before['animation']['auto_key'] and not before['animation']['set_key_mode']
assert not call('ui_tree')['floating']
assert hashlib.sha256(SOURCE.read_bytes()).hexdigest()==DIGEST
output=Path(sys.argv[1]);output.mkdir(exist_ok=False)
baseline=json.loads(SOURCE.read_text())['scene']
try:
    call('Set Frame',frame=0)
    call('Key LoomRig Pose',rig=1)
    call('Set Frame',frame=39)
    call('Set Key Mode')
    event(type='move',pos=[757,210]);event(type='zoom',factor=2.5)
    event(type='click',pos=[720,229],button='left')
    assert call('State')['selection']['selected']==[4975]
    call('Select and Rotate')
    event(type='drag',**{'from':[725,142],'to':[758,151],'button':'left','steps':8})
    posed=call('State')
    angle=next(node for node in posed['nodes'] if node['id']==4975)['rotation_deg']
    assert any(abs(value)>1e-4 for value in angle),'gizmo did not pose the FK control'
    shot('before-key')
    event(type='click',pos=[355,11],button='left')
    event(type='click',pos=[380,39],button='left')
    tree=call('ui_tree')
    windows=[item['rect'] for item in tree['floating'] if item.get('rect')]
    assert any(abs(rect[0]-16)<6 and abs(rect[1]-64)<6 for rect in windows),'editor moved: inspect the actual UI before proceeding'
    event(type='click',pos=[195,114],button='left')
    event(type='click',pos=[80,250],button='left')
    accepted=call('State')
    assert accepted['undo_labels']==['Key Rig Pose','Key Rig Pose'],accepted['undo_labels']
    assert accepted['animation']['set_key_mode']
    scene=snapshot('accepted39')
    rotation=scene['nodes']['4975']['transform']['rotation']['controller']['Keys']['tracks']
    for index,track in enumerate(rotation):
        assert next(key['value'] for key in track['keys'] if key['time']==0.)==0.
        assert abs(next(key['value'] for key in track['keys'] if key['time']==39.)-angle[index])<1e-4
    rig=scene['loom_rigs'][0]
    assert rig['pose_frames']==[0.,39.]
    owned={rig['master']}|{binding['control'] for binding in rig['bindings']}|{limb[role] for limb in rig['limbs'] for role in ['goal','pole']}
    for key,node in baseline['nodes'].items():
        if int(key) not in owned:assert scene['nodes'][key]==node
    assert scene['materials']==baseline['materials']
    event(type='click',pos=[862,82],button='left');shot('after-key')
    box=(0,74,1515,1043)
    a=Image.open(output/'before-key.png').convert('RGB').crop(box)
    b=Image.open(output/'after-key.png').convert('RGB').crop(box)
    assert ImageChops.difference(a,b).getbbox() is None,'accepting the key changed the visible pose'
    call('Set Frame',frame=40)
    held=call('State')
    assert next(node for node in held['nodes'] if node['id']==4975)['rotation_deg']==angle
    call('Set Key Mode')
    call('Undo Scene Operation');call('Undo Scene Operation')
    assert snapshot('restored')==baseline
    state=call('State');assert not state['dirty'] and not state['undo_labels']
    call('Open File',path=str(SOURCE))
    restored=call('State')
    assert restored['nodes']==before['nodes'] and restored['selection']==before['selection']
    assert restored['frame']==39 and not restored['dirty'] and not restored['undo_labels']
    assert not restored['animation']['set_key_mode']
    assert hashlib.sha256(SOURCE.read_bytes()).hexdigest()==DIGEST
    emit({'kind':'complete','actual_fk_gizmo':True,'buffered_pose_pixels_preserved':True,'prior_zero_key_preserved':True,'accepted_key_survives_frame_change':True,'one_undo_for_accepted_pose':True,'all_unowned_source_nodes_and_materials_unchanged':True,'full_scene_undo_equality':True,'original_runtime_restored':True})
except BaseException:
    # Retain a forensic real-scene copy; never overwrite/reload the original on failure.
    try:call('Save File As Copy',path=str(output/'failed-draft.loom'))
    except Exception:pass
    raise
