"""Actual native cage selection/manipulation; real scene copies, no State dump."""
import ast
import hashlib
import json
import math
from pathlib import Path
import socket
import sys
from PIL import Image,ImageChops

helper=Path('docs/research/contact-target-native-71883fd/verify.py')
tree=ast.parse(helper.read_text())
exec(compile(ast.Module(body=[node for node in tree.body if isinstance(node,ast.FunctionDef) and node.name in ['call','project']],type_ignores=[]),str(helper),'exec'))
SOURCE=Path('/mnt/2tb/hollowreach-ue/Hollowreach/SourceArt/RayLoom20261025/RayAnimationTest_LoomRig.loom')
DIGEST='84fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935'
output=Path(sys.argv[1]);output.mkdir(exist_ok=False)
assert call('ping')['commit']=='c4708d049792d8322d211b0d59c58626df7cd8cc'
pid=call('ping')['pid'];state=call('State')
assert state['file']==str(SOURCE) and state['frame']==39 and all(label=='Select' for label in state['undo_labels'])
assert not any(state['animation'][key] for key in ['playing','auto_key','set_key_mode'])
assert hashlib.sha256(SOURCE.read_bytes()).hexdigest()==DIGEST
baseline=json.loads(SOURCE.read_text())['scene']
def emit(**value):print(json.dumps(value),flush=True)
def event(**value):call('input',events=[value])
def copy(name):
 path=output/(name+'.loom');assert not path.exists();call('Save File As Copy',path=str(path));return json.loads(path.read_text())['scene']
def shot(name):
 event(type='leave');call('screenshot',path=str(output/(name+'.png')))
 gpu=call('diagnostics')['playback']['gpu_pose'];assert (gpu['committed_objects'],gpu['committed_parts'])==(5,13)
 assert gpu['last_error'] is None and gpu['recovered_recipes']==0
assert copy('preflight')==baseline
call('Open File',path=str(SOURCE));before=call('State');assert not before['dirty'] and not before['undo_labels']
call('Select Object');shot('baseline')
try:
 for role,control,point,origin in [('foot',4986,[670,587],[681,583]),('pelvis',4968,[760,526],[743,512]),('master',4967,[770,613],None)]:
  call('Select Object');event(type='click',pos=[550,400],button='left');assert call('State')['selection']['selected']==[]
  event(type='click',pos=point,button='left');assert call('State')['selection']['selected']==[control],role
  call('Select and Move');shot(role+'-before');history=call('State')['undo_labels']
  if origin is None:
   node=next(node for node in call('State')['nodes'] if node['id']==control);assert node['parent'] is None;origin=project(node['position'])
  event(type='drag',**{'from':origin,'to':[origin[0]+15,origin[1]-12],'button':'left','steps':8})
  state=call('State');assert state['selection']['selected']==[control] and len(state['undo_labels'])==len(history)+1,role
  posed=copy(role+'-posed');changed=[identifier for identifier in baseline['nodes'] if posed['nodes'][identifier]!=baseline['nodes'][identifier]]
  assert changed==[str(control)],(role,changed)
  assert posed['loom_rigs']==baseline['loom_rigs'] and posed['materials']==baseline['materials']
  shot(role+'-posed');call('Undo Scene Operation');assert copy(role+'-undo')==baseline
  call('Open File',path=str(SOURCE));assert not call('State')['dirty']
  emit(kind='native_shape_pointer',role=role,control=control,only_input_changed=True,source_nodes_skin_materials_retained=True,one_gesture_one_undo=True,full_scene_undo_exact=True)
 call('Set LoomRig Contact Preset',rig=1,limb='Left Arm',time=39.,preset='planted')
 call('Select Nodes',ids=[4987],op='replace');call('Move Selection',delta=[0.,0.,5.],system='World')
 prepared=copy('ik-prepared');contact=prepared['loom_rigs'][0]['limbs'][0]['contacts'];reference=contact['references'][str(contact['keys'][-1]['reference'])];assert reference['space']=='world'
 origin=project(reference['position']);call('Select Nodes',ids=[],op='replace');call('Select Object');event(type='click',pos=origin,button='left')
 assert call('State')['selection']['selected']==[4987],'actual IK hand cage click'
 call('Select and Move');shot('ik-before');history=call('State')['undo_labels']
 event(type='drag',**{'from':origin,'to':[origin[0]+15,origin[1]-12],'button':'left','steps':8})
 state=call('State');assert state['selection']['selected']==[4987] and len(state['undo_labels'])==len(history)+1
 posed=copy('ik-posed');assert posed['nodes']==prepared['nodes'] and posed['materials']==prepared['materials']
 assert posed['loom_rigs'][0]['limbs'][0]['contacts']!=contact
 assert posed['loom_rigs'][0]['limbs'][1:]==prepared['loom_rigs'][0]['limbs'][1:]
 shot('ik-posed');call('Undo Scene Operation');assert copy('ik-undo')==prepared
 for _ in range(16):
  history=call('State')['undo_labels']
  if not history:break
  call('Undo Scene Operation');assert len(call('State')['undo_labels'])==len(history)-1
 assert not call('State')['undo_labels'] and copy('final-undo')==baseline
 emit(kind='native_shape_pointer',role='IK hand',control=4987,only_contact_target_changed=True,source_nodes_skin_materials_retained=True,one_gesture_one_undo=True,full_scene_undo_exact=True)
 call('Open File',path=str(SOURCE));call('Select Object');restored=call('State')
 assert restored['nodes']==before['nodes'] and restored['selection']==before['selection'] and restored['frame']==39
 assert not restored['dirty'] and not restored['undo_labels'] and call('ping')['pid']==pid
 assert hashlib.sha256(SOURCE.read_bytes()).hexdigest()==DIGEST
 shot('restored');crop=(0,74,1515,1043)
 a=Image.open(output/'baseline.png').convert('RGB').crop(crop);b=Image.open(output/'restored.png').convert('RGB').crop(crop)
 assert ImageChops.difference(a,b).getbbox() is None,'all four restored viewports must match this new version baseline'
 call('Select and Rotate')
 emit(kind='restored',all_four_viewport_pixels_identical=True,original_source_hash=DIGEST,nodes_frame_selection_modes_and_history_restored=True)
except Exception:
 copy('unresolved-preserved');emit(kind='stopped',pending_scene_preserved=True,source_untouched=hashlib.sha256(SOURCE.read_bytes()).hexdigest()==DIGEST);raise
