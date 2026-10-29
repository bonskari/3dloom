import hashlib,json,os,socket,sys,time
from pathlib import Path

def call(cmd,**args):
 with socket.socket(socket.AF_UNIX) as s:
  s.settimeout(30);s.connect('/run/user/1000/3dloom/current');s.sendall((json.dumps({'cmd':cmd,'args':args})+'\n').encode());r=json.loads(s.makefile().readline())
 assert r.get('ok'),r
 return r

def emit(r):print(json.dumps(r),flush=True)
def memory(pid):
 return {k:int(v.split()[0]) for line in Path(f'/proc/{pid}/status').read_text().splitlines() if ':' in line for k,v in [line.split(':',1)] if k in ['VmRSS','VmSwap','Threads']}
def filehash(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()

ping=call('ping');assert ping['commit']==sys.argv[1], 'wrong binary for stability test';pid=ping['pid'];before=call('State');d=call('diagnostics')
parallel=sys.argv[2] if len(sys.argv)>2 else 'unset';assert parallel in ['unset','1']
env=Path(f'/proc/{pid}/environ').read_bytes().split(b'\0')
assert not any(e.startswith((b'LOOM_PROFILE_FRAMES=',b'LOOM_EXPERIMENTAL_GPU_POSES=')) for e in env)
assert not any(e.startswith((b'LOOM_EXPERIMENTAL_CPU_EDGE_BATCH=',b'LOOM_EXPERIMENTAL_FUSED_POSE_POSITIONS=')) for e in env)
if parallel=='1':assert b'LOOM_EXPERIMENTAL_PARALLEL_POSE_PREPARATION=1' in env
else:assert not any(e.startswith(b'LOOM_EXPERIMENTAL_PARALLEL_POSE_PREPARATION=') for e in env)
assert not before['dirty'] and not before['animation']['playing'] and not before['animation']['auto_key']
assert not before['undo_labels']
prefs=d['modes']['prefs'];assert prefs['auto_backup_enable'] and prefs['auto_backup_interval_min']<=10
original_file_hash=filehash(before['file']);assert original_file_hash=='84fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935'
# Two nondirty selection changes exercise the real backup clock without altering any model data.
selected=before['selection']['selected'];alternate=4967 if selected!=[4967] else 4988
emit({'kind':'start','app_pid':pid,'test_pid':os.getpid(),'commit':ping['commit'],'parallel_preparation':parallel,'memory':memory(pid),'context':'long playback/backup reliability with explicit recorded configuration; possible concurrent correctness work, not a quiet FPS acceptance benchmark','backup_interval_min':prefs['auto_backup_interval_min']})
call('Select Nodes',ids=[alternate],op='replace');call('Select Nodes',ids=selected,op='replace')
call('Viewport Shading',shading='Realistic');play_start=time.time();call('Play Animation')
backup_paths=set();complete=False
try:
 time.sleep(10);last=call('diagnostics');start=time.monotonic();last_wall=start
 for i in range(30):
  time.sleep(30);current=call('diagnostics');now=time.monotonic()
  assert call('ping')['pid']==pid,'native process restarted'
  gpu=current['playback']['gpu_pose'];assert gpu and (gpu['committed_objects'],gpu['committed_parts'])==(5,13),gpu
  assert gpu['parallel_preparation_enabled'] is True,'wrong active preparation configuration'
  assert gpu['last_error'] is None and gpu['recovered_recipes']==0,gpu
  count=current['playback']['submitted_distinct_pose_times']-last['playback']['submitted_distinct_pose_times'];assert count>0
  prompt=current['ui']['status_prompt']
  if prompt.startswith('Auto Backup: '):backup_paths.add(prompt.removeprefix('Auto Backup: '))
  emit({'kind':'sample','sample':i+1,'seconds':now-start,'ready_pose_times':count,'observed_fps':count/(now-last_wall),'gpu_pose':gpu,'memory':memory(pid)})
  last=current;last_wall=now
 complete=True
finally:
 state=call('State')
 takeover=state['selection']!=before['selection'] or state['file']!=before['file'] or state['dirty'] or state['viewports']!=before['viewports'] or filehash(before['file'])!=original_file_hash
 if not takeover:
  if state['animation']['playing']:call('Play Animation')
  # Initial history was empty. Reload restores it after the two test selections.
  call('Open File',path=before['file']);state=call('State')
  assert state['nodes']==before['nodes'] and state['selection']==before['selection']
  assert state['frame']==before['frame'] and not state['dirty'] and not state['undo_labels']
  assert filehash(before['file'])==original_file_hash
  emit({'kind':'restored','all_node_records_equal':True,'original_undo_history_restored':True,'source_file_unchanged':True,'memory':memory(pid)})
 else:emit({'kind':'human_takeover','automatic_restore_skipped':True})
if complete:
 source=json.loads(Path(before['file']).read_text());verified=[]
 for name in sorted(backup_paths):
  path=Path(name)
  if path.exists() and path.stat().st_mtime>=play_start:
   saved=json.loads(path.read_text());assert saved['scene']==source['scene'],'backup changed scene data'
   verified.append({'path':name,'bytes':path.stat().st_size,'finished_after_play_start':True,'scene_equal':True})
 assert verified,'no real automatic backup completed after playback began'
 emit({'kind':'complete','backup_verified':verified,'memory':memory(pid)})
