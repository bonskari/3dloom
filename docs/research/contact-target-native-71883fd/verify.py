"""Native contact Auto Key input and matched Free FK verification.

No operating-system input; only app-owned IPC. Real scene copies and filtered
evidence are retained, never State/preferences. Does not overwrite the source.
"""
import hashlib
import json
import math
from pathlib import Path
import socket
import sys

SOURCE=Path('/mnt/2tb/hollowreach-ue/Hollowreach/SourceArt/RayLoom20261025/RayAnimationTest_LoomRig.loom')
DIGEST='84fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935'
COMMIT='71883fd71cfc4fc7949a3bc8dfca406afddc276a'

def call(cmd,**args):
    with socket.socket(socket.AF_UNIX) as sock:
        sock.settimeout(30);sock.connect('/run/user/1000/3dloom/current')
        sock.sendall((json.dumps({'cmd':cmd,'args':args})+'\n').encode())
        result=json.loads(sock.makefile().readline())
    assert result.get('ok'),result.get('error')
    return result

def event(**value):call('input',events=[value])
def emit(**value):print(json.dumps(value),flush=True)
def snapshot(name):
    path=output/(name+'.loom');assert not path.exists()
    call('Save File As Copy',path=str(path))
    return json.loads(path.read_text())['scene']
def shot(name):
    event(type='leave');call('screenshot',path=str(output/(name+'.png')))
    gpu=call('diagnostics')['playback']['gpu_pose']
    assert (gpu['committed_objects'],gpu['committed_parts'])==(5,13)
    assert gpu['last_error'] is None and gpu['recovered_recipes']==0
def select(control):call('Select Nodes',ids=[control],op='replace')
def undo_to(history):
    for _ in range(16):
        current=call('State')['undo_labels']
        if current==history:return
        assert len(current)>len(history) and current[:len(history)]==history,'unexpected Undo history'
        call('Undo Scene Operation')
        assert len(call('State')['undo_labels'])==len(current)-1
    raise AssertionError('unexpectedly long native test history')

def project(point):
    """Project through the actual unrolled perspective viewport camera."""
    view=call('diagnostics')['viewports'];camera=view['active_view']['camera']
    assert abs(call('State')['camera']['roll_deg'])<1e-6
    assert view['active_view']['kind']=='Perspective'
    rect=view['rects'][view['active']];x,y,w,h=rect[0],rect[1],rect[2]-rect[0],rect[3]-rect[1]
    yaw,pitch=camera['yaw'],camera['pitch']
    sy,cy,sp,cp=math.sin(yaw),math.cos(yaw),math.sin(pitch),math.cos(pitch)
    right=(-sy,cy,0.);up=(-sp*cy,-sp*sy,cp);back=(cp*cy,cp*sy,sp)
    offset=[point[i]-camera['target'][i]-back[i]*camera['distance'] for i in range(3)]
    dot=lambda a,b:sum(q*r for q,r in zip(a,b))
    depth=-dot(offset,back);assert depth>0
    scale=h/(2*depth*math.tan(view['active_view']['fov_y_radians']/2))
    result=[x+w/2+dot(offset,right)*scale,y+h/2-dot(offset,up)*scale]
    assert x+25<result[0]<x+w-25 and y+25<result[1]<y+h-25,result
    return result

assert call('ping')['commit']==COMMIT
before=call('State');pid=call('ping')['pid']
assert before['file']==str(SOURCE) and before['frame']==39
assert not before['dirty'] and not before['undo_labels']
assert before['selection']['selected']==[4988]
assert not any(before['animation'][key] for key in ['playing','auto_key','set_key_mode'])
assert not call('ui_tree')['floating']
assert hashlib.sha256(SOURCE.read_bytes()).hexdigest()==DIGEST
output=Path(sys.argv[1]);output.mkdir(exist_ok=False)
source_document=json.loads(SOURCE.read_text());baseline=source_document['scene']
assert snapshot('baseline')==baseline
limb=baseline['loom_rigs'][0]['limbs'][0];goal,pole=limb['goal'],limb['pole']
try:
    call('Set LoomRig Contact Preset',rig=1,limb='Left Arm',time=39.,preset='planted')
    call('Set Frame',frame=40)
    start=snapshot('planted-before')
    start_history=call('State')['undo_labels']
    original=start['loom_rigs'][0]['limbs'][0]['contacts']
    call('Auto Key Mode Toggle');call('Select and Move')
    prior=start
    for role,control in [('goal',goal),('pole',pole)]:
        select(control)
        contact=prior['loom_rigs'][0]['limbs'][0]['contacts']
        key=next((key for key in contact['keys'] if key['time']==40.),contact['keys'][-1])
        reference=contact['references'][str(key['reference'])]
        assert reference['space']=='world'
        origin=project(reference['position' if role=='goal' else 'pole_position'])
        history=call('State')['undo_labels']
        event(type='drag',**{'from':origin,'to':[origin[0]+15,origin[1]-12],'button':'left','steps':8})
        current=call('State');assert current['selection']['selected']==[control]
        assert len(current['undo_labels'])==len(history)+1
        after=snapshot(role+'-after')
        track=after['loom_rigs'][0]['limbs'][0]['contacts']
        assert any(key['time']==40. for key in track['keys'])
        for key in original['keys']:assert key in track['keys']
        for identifier,pose in original['references'].items():assert track['references'][identifier]==pose
        assert len(track['references'])==len(original['references'])+1
        current_key=next(key for key in track['keys'] if key['time']==40.)
        assert track['references'][str(current_key['reference'])][('position' if role=='goal' else 'pole_position')]!=reference[('position' if role=='goal' else 'pole_position')]
        assert after['nodes']==start['nodes'] and after['materials']==start['materials']
        shot(role+'-after');emit(kind='pointer_auto_key',role=role,current_frame=40,prior_anchors_retained=True,one_gesture_one_undo=True,raw_nodes_materials_retained=True)
        prior=after
    undo_to(start_history)
    assert snapshot('planted-restored')==start
    call('Auto Key Mode Toggle');call('Set Frame',frame=39)
    select(goal)
    call('Move Selection',delta=[0.,0.,2.],system='World')
    call('Set LoomRig Contact Preset',rig=1,limb='Left Arm',time=39.,preset='free')
    free=snapshot('free-before')
    matches=free['loom_rigs'][0]['limbs'][0]['pose_matches']
    assert matches.get('keys') or matches.get('constant'),'Free pose has no matching data'
    offsets=matches['keys'][-1]['offsets'] if matches.get('keys') else matches['constant']
    identity=[1. if index in [0,5,10,15] else 0. for index in range(16)]
    assert any(abs(value-expected)>1e-4 for matrix in offsets for value,expected in zip(matrix,identity)),'fixture must contain a nonidentity match'
    binding=next(binding for binding in free['loom_rigs'][0]['bindings'] if binding['bone']==limb['mapping']['start'])
    control=binding['control'];select(control);call('Select and Rotate')
    shot('free-before')
    call('Rotate Selection',axis='x',degrees=15.,system='World')
    rotated=snapshot('free-rotated')
    changed=[identifier for identifier in free['nodes'] if free['nodes'][identifier]!=rotated['nodes'][identifier]]
    assert changed==[str(control)],changed
    assert rotated['loom_rigs']==free['loom_rigs'] and rotated['materials']==free['materials']
    shot('free-rotated');call('Undo Scene Operation');assert snapshot('free-restored')==free
    emit(kind='matched_free_fk',world_x_degrees=15,changed_node_ids=[control],source_tracks_and_matching_retained=True,undo_restored=True)
    undo_to(before['undo_labels'])
    assert snapshot('original-restored')==baseline
    call('Open File',path=str(SOURCE));call('Select and Rotate');event(type='leave')
    restored=call('State')
    assert restored['nodes']==before['nodes'] and restored['selection']==before['selection']
    assert restored['frame']==39 and not restored['dirty'] and not restored['undo_labels']
    assert call('ping')['pid']==pid and hashlib.sha256(SOURCE.read_bytes()).hexdigest()==DIGEST
    shot('restored');emit(kind='restored',original_scene_hash=DIGEST,original_nodes_selection_frame_and_history=True)
except Exception:
    snapshot('unresolved-preserved')
    emit(kind='stopped',pending_scene_preserved=True,original_file_untouched=hashlib.sha256(SOURCE.read_bytes()).hexdigest()==DIGEST)
    raise
