"""Verify selection saved-state in the native app before any cleanup reload."""
import ast
import hashlib
import json
from pathlib import Path
import socket
import sys

helper=Path('docs/research/contact-target-native-71883fd/verify.py');tree=ast.parse(helper.read_text())
exec(compile(ast.Module(body=[node for node in tree.body if isinstance(node,ast.FunctionDef) and node.name=='call'],type_ignores=[]),str(helper),'exec'))
source=Path('/mnt/2tb/hollowreach-ue/Hollowreach/SourceArt/RayLoom20261025/RayAnimationTest_LoomRig.loom')
digest='84fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935'
ping=call('ping');assert ping['commit']==sys.argv[1]
before=call('State');assert before['file']==str(source) and before['frame']==39
assert not before['dirty'] and not before['undo_labels'] and before['selection']['selected']==[4988]
assert not any(before['animation'][key] for key in ['playing','auto_key','set_key_mode'])
assert hashlib.sha256(source.read_bytes()).hexdigest()==digest

def emit(**value):print(json.dumps(value),flush=True)
def check_dirty(expected,phase):
 state=call('State');emit(kind='saved_state',phase=phase,dirty=state['dirty'],history_length=len(state['undo_labels']))
 assert state['dirty']==expected,(phase,state['dirty'],expected)

try:
 for index,ids in enumerate([[4967],[4968],[]]):
  call('Select Nodes',ids=ids,op='replace');check_dirty(False,'select'+str(index))
 for index in range(3):call('Undo Scene Operation');check_dirty(False,'undo_select'+str(index))
 state=call('State');assert state['nodes']==before['nodes'] and state['selection']==before['selection'] and not state['undo_labels']
 for index in range(3):call('Redo Scene Operation');check_dirty(False,'redo_select'+str(index))
 for _ in range(3):call('Undo Scene Operation')
 call('Select Nodes',ids=[4967],op='replace');check_dirty(False,'select_for_real_move')
 call('Move Selection',delta=[0.,0.,1.],system='World');check_dirty(True,'real_move')
 call('Select Nodes',ids=[4968],op='replace');check_dirty(True,'selection_after_real_move')
 call('Undo Scene Operation');check_dirty(True,'undo_selection_keeps_real_move_dirty')
 call('Undo Scene Operation');check_dirty(False,'undo_real_move')
 call('Undo Scene Operation');check_dirty(False,'undo_initial_selection')
 state=call('State');assert state['nodes']==before['nodes'] and state['selection']==before['selection'] and not state['undo_labels']
 assert call('ping')['pid']==ping['pid'] and hashlib.sha256(source.read_bytes()).hexdigest()==digest
 emit(kind='passed',clean_without_reload=True,real_edit_remained_dirty_until_undone=True,all_node_records_restored=True)
 # The assertions above prove the correction without reloading; clear only test redo history now.
 call('Open File',path=str(source));call('Select and Rotate')
 state=call('State');assert not state['dirty'] and not state['undo_labels'] and state['nodes']==before['nodes'] and state['selection']==before['selection']
 emit(kind='restored',source_hash=digest,frame=39,selection=[4988],clean=True)
except Exception:
 state=call('State')
 # A failing pre-fix run has selection markers only. Never discard a real edit.
 if state['nodes']==before['nodes'] and all(label=='Select' for label in state['undo_labels']):
  for _ in range(len(state['undo_labels'])):call('Undo Scene Operation')
  state=call('State');assert state['nodes']==before['nodes'] and state['selection']==before['selection']
  assert hashlib.sha256(source.read_bytes()).hexdigest()==digest
  call('Open File',path=str(source));call('Select and Rotate')
  emit(kind='old_run_cleanup',only_selection_markers_discarded=True,source_unchanged=True)
 else:emit(kind='stopped',unresolved_real_edit_retained=True)
 raise
