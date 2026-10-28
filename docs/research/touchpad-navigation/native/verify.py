import hashlib,json,math,socket
from pathlib import Path

def call(cmd,**args):
 with socket.socket(socket.AF_UNIX) as s:
  s.settimeout(30);s.connect('/run/user/1000/3dloom/current');s.sendall((json.dumps({'cmd':cmd,'args':args})+'\n').encode());r=json.loads(s.makefile().readline())
 assert r.get('ok'),r
 return r

def emit(v):print(json.dumps(v),flush=True)
def camera():return call('diagnostics')['viewports']['active_view']['camera']
def near(a,b):return abs(a-b)<=max(1e-5,abs(b)*2e-6)
def zoomed(distance,factor):return max(0.05,min(10000.0,distance/factor))

ping=call('ping');before=call('State');d=call('diagnostics')
assert ping['commit'].startswith('5f91f1e'),ping
assert not before['dirty'] and not d['modes']['playing']
assert before['frame']==39 and d['viewports']['active_view']['kind']=='Perspective'
source=Path(before['file']);digest=hashlib.sha256(source.read_bytes()).hexdigest()
assert digest=='84fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935'
mode=d['modes']['prefs']['viewport_navigation_device'];restore_mode={'auto':0,'mouse':1,'touchpad':2}[mode]
original_pointer=d['egui']['input']['pointer']['pos']
index=d['viewports']['active'];r=d['viewports']['rects'][index];pos=[(r[0]+r[2])/2,(r[1]+r[3])/2]
call('Set Preference',name='Viewport Navigation Device',value=0)
call('input',events=[{'type':'move','pos':pos}])
emit({'kind':'start','pid':ping['pid'],'commit':ping['commit'],'original_navigation_device':mode})
try:
 a=camera();call('input',events=[{'type':'scroll','unit':'point','delta':[12,0]}]);b=camera()
 assert b['target']!=a['target'] and near(b['distance'],a['distance'])
 emit({'kind':'pixel_pan','passed':True})
 a=b;call('input',events=[{'type':'zoom','factor':1.25}]);b=camera()
 assert near(b['distance'],zoomed(a['distance'],1.25)) and b['target']==a['target']
 emit({'kind':'native_zoom_event','passed':True})
 a=b;call('input',events=[{'type':'scroll','unit':'point','delta':[0,10],'mods':['command']}]);b=camera()
 assert near(b['distance'],zoomed(a['distance'],math.exp(0.02))) and b['target']==a['target']
 emit({'kind':'command_zoom_once','passed':True})
 a=b;call('input',events=[{'type':'scroll','unit':'point','delta':[20,10],'mods':['alt']}]);b=camera()
 assert (b['yaw'],b['pitch'])!=(a['yaw'],a['pitch']) and near(b['distance'],a['distance'])
 emit({'kind':'alt_orbit','passed':True})
 call('Set Preference',name='Viewport Navigation Device',value=2)
 a=camera();call('input',events=[{'type':'scroll','delta':[0,1]}]);b=camera()
 assert b['target']!=a['target'] and near(b['distance'],a['distance'])
 emit({'kind':'x11_line_pan_mode','passed':True})
 call('Set Preference',name='Viewport Navigation Device',value=1)
 a=b;call('input',events=[{'type':'scroll','unit':'point','delta':[0,10]}]);b=camera()
 expected=max(0.05,min(10000.0,a['distance']*(1-0.1*(10/50))))
 assert near(b['distance'],expected) and b['target']==a['target']
 emit({'kind':'precision_mouse_scroll_zoom','passed':True})
finally:
 after=call('State')
 if call('ping')['pid']==ping['pid'] and after['file']==before['file'] and after['nodes']==before['nodes'] and after['selection']==before['selection'] and not after['dirty'] and after['frame']==before['frame']:
  call('Open File',path=before['file']);call('Set Preference',name='Viewport Navigation Device',value=restore_mode)
  call('input',events=[{'type':'move','pos':original_pointer}] if original_pointer is not None else [{'type':'leave'}])
  restored=call('State');assert restored['nodes']==before['nodes'] and restored['selection']==before['selection']
  assert not restored['dirty'] and hashlib.sha256(source.read_bytes()).hexdigest()==digest
  assert call('diagnostics')['modes']['prefs']['viewport_navigation_device']==mode
  emit({'kind':'restored','all_node_records_unchanged':True,'source_file_unchanged':True,'original_navigation_preference_restored':True})
 else:emit({'kind':'takeover','automatic_restore_skipped':True})
