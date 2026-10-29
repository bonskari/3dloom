import ast,json,socket,hashlib
from pathlib import Path
source=Path('/home/b1s/dev/github/3dloom/docs/research/contact-target-native-71883fd/verify.py')
g={'json':json,'socket':socket}
t=ast.parse(source.read_text());exec(compile(ast.Module(body=[n for n in t.body if isinstance(n,ast.FunctionDef) and n.name=='call'],type_ignores=[]),str(source),'exec'),g);call=g['call']
original=Path('/mnt/2tb/hollowreach-ue/Hollowreach/SourceArt/RayLoom20261025/RayAnimationTest_LoomRig.loom')
prepared=Path('/home/b1s/dev/scratch-scripts/loom-native-71883fd-contacts-v2/free-before.loom')
output=Path('/home/b1s/dev/scratch-scripts/loom-native-71883fd-free-gizmo');output.mkdir(exist_ok=False)
before=call('State');assert before['file']==str(original) and not before['dirty'] and not before['undo_labels'] and before['frame']==39
assert not any(before['animation'][k] for k in ['playing','auto_key','set_key_mode'])
assert call('ping')['commit']=='71883fd71cfc4fc7949a3bc8dfca406afddc276a'
free=json.loads(prepared.read_text())['scene']
def snapshot(name):
 p=output/(name+'.loom');assert not p.exists();call('Save File As Copy',path=str(p));return json.loads(p.read_text())['scene']
def shot(name):
 call('input',events=[{'type':'leave'}]);call('screenshot',path=str(output/(name+'.png')))
try:
 call('Open File',path=str(prepared));call('Select Nodes',ids=[4975],op='replace');call('Select and Rotate')
 history=call('State')['undo_labels'];shot('before')
 call('input',events=[{'type':'drag','from':[804,530],'to':[828,526],'button':'left','steps':8}])
 after=call('State');assert after['selection']['selected']==[4975] and len(after['undo_labels'])==len(history)+1
 posed=snapshot('posed');changed=[k for k in free['nodes'] if posed['nodes'][k]!=free['nodes'][k]]
 assert changed==['4975'],changed
 assert posed['loom_rigs']==free['loom_rigs'] and posed['materials']==free['materials']
 shot('posed');gpu=call('diagnostics')['playback']['gpu_pose'];assert (gpu['committed_objects'],gpu['committed_parts'])==(5,13) and gpu['last_error'] is None and gpu['recovered_recipes']==0
 call('Undo Scene Operation');assert snapshot('restored')==free
 call('Open File',path=str(original));call('Select and Rotate');restored=call('State')
 assert restored['nodes']==before['nodes'] and restored['frame']==39 and restored['selection']==before['selection'] and not restored['dirty'] and not restored['undo_labels']
 assert hashlib.sha256(original.read_bytes()).hexdigest()=='84fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935'
 print(json.dumps({'kind':'matched_free_fk_pointer_gizmo','changed_node_ids':[4975],'one_gesture_one_undo':True,'matching_and_source_records_retained':True,'full_scene_undo_exact':True,'original_runtime_restored':True}),flush=True)
except Exception:
 snapshot('unresolved-preserved');raise
