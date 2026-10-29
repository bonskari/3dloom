"""Measure native committed pose times, excluding clip-tail/wrap windows.

Run only with the native app already open and other GPU jobs stopped.
The log records observed ready-pose throughput, not an independent per-frame
vertex fingerprint. Full geometry correctness is verified by separate gates.
"""
import hashlib
import json
import os
from pathlib import Path
import socket
import sys
import time

output = Path(sys.argv[1])
expected_mode = sys.argv[2]
expected_commit = sys.argv[3]
expected_parallel = sys.argv[4] if len(sys.argv) > 4 else 'unset'
assert expected_parallel in ['unset', '0', '1']

def call(command, **args):
    with socket.socket(socket.AF_UNIX) as sock:
        sock.settimeout(30)
        sock.connect('/run/user/1000/3dloom/current')
        sock.sendall((json.dumps({'cmd': command, 'args': args}) + '\n').encode())
        result = json.loads(sock.makefile().readline())
    if not result.get('ok'):
        raise RuntimeError(result)
    return result

def emit(record):
    with output.open('a') as log:
        log.write(json.dumps(record) + '\n')
    print(json.dumps(record), flush=True)

def diag():
    result = call('diagnostics')
    gpu = result['playback']['gpu_pose']
    assert gpu['committed_objects'] == 5 and gpu['committed_parts'] == 13, gpu
    assert gpu['last_error'] is None and gpu['recovered_recipes'] == 0, gpu
    return result

assert not output.exists(), 'Never overwrite an earlier measurement'
ping = call('ping')
pid = ping['pid']
assert ping['commit'] == expected_commit, 'Wrong binary for production acceptance'
env = Path(f'/proc/{pid}/environ').read_bytes().split(b'\0')
if expected_mode == 'default':
    assert not any(e.startswith(b'LOOM_EXPERIMENTAL_GPU_POSES=') for e in env)
else:
    assert ('LOOM_EXPERIMENTAL_GPU_POSES=' + expected_mode).encode() in env
assert not any(e.startswith(b'LOOM_PROFILE_FRAMES=') for e in env)
assert not any(e.startswith((b'LOOM_EXPERIMENTAL_CPU_EDGE_BATCH=', b'LOOM_EXPERIMENTAL_FUSED_POSE_POSITIONS=')) for e in env), 'Production acceptance requires unset feature overrides'
if expected_parallel != 'unset':
    assert ('LOOM_EXPERIMENTAL_PARALLEL_POSE_PREPARATION=' + expected_parallel).encode() in env
else:
    assert not any(e.startswith(b'LOOM_EXPERIMENTAL_PARALLEL_POSE_PREPARATION=') for e in env)
call('input', events=[{'type': 'leave'}])
before = call('State')
initial = diag()
assert initial['playback']['gpu_pose']['parallel_preparation_enabled'] == (expected_parallel != '0'), 'Wrong active preparation configuration'
assert not initial['modes']['playing'], 'Start from a paused scene'
assert not before['dirty'], 'Preserve unsaved work before benchmarking'
scene = Path(before['file'])
scene_hash = hashlib.sha256(scene.read_bytes()).hexdigest()
original_shading = initial['viewports']['active_view']['shading']
original_selection = before['selection']
emit({'kind': 'start', 'pid': pid, 'commit': ping['commit'], 'mode': expected_mode, 'parallel_preparation': expected_parallel,
      'parallel_preparation_enabled': initial['playback']['gpu_pose']['parallel_preparation_enabled'],
      'experimental_overrides_unset': not any(e.startswith(b'LOOM_EXPERIMENTAL_') for e in env),
      'rate_clocks': ['client_ipc_wall_seconds', 'app_distinct_submission_seconds'],
      'frame': before['frame'], 'scene_sha256': scene_hash})
try:
    for shading in ['Realistic', 'Wireframe']:
        call('Viewport Shading', shading=shading)
        assert diag()['viewports']['active_view']['shading'] == shading
        call('Play Animation')
        time.sleep(8)
        rates = []
        event_rates = []
        for index in range(24):
            a = diag()['playback']
            t0 = time.perf_counter()
            time.sleep(1)
            b = diag()['playback']
            elapsed = time.perf_counter() - t0
            assert call('ping')['pid'] == pid, 'Application changed during measurement'
            count = b['submitted_distinct_pose_times'] - a['submitted_distinct_pose_times']
            assert count > 0, 'No new ready pose times'
            start, end = a['submitted_pose_time'], b['submitted_pose_time']
            active = 0 <= start < end < 149
            rate = count / elapsed
            assert a.get('submitted_distinct_seconds') is not None and b.get('submitted_distinct_seconds') is not None, 'Requires the submission-event clock verification build'
            event_elapsed = b['submitted_distinct_seconds'] - a['submitted_distinct_seconds']
            assert event_elapsed > 0, 'Ready submission clock did not advance'
            event_rate = count / event_elapsed
            if active:
                rates.append((count, elapsed))
                event_rates.append((count, event_elapsed))
            emit({'kind': 'sample', 'shading': shading, 'sample': index,
                  'ready_pose_times': count, 'seconds': elapsed, 'fps': rate,
                  'event_seconds': event_elapsed, 'event_fps': event_rate,
                  'start_time': start, 'end_time': end, 'active_clip_window': active})
        assert len(rates) >= 12, 'Not enough moving-clip windows'
        emit({'kind': 'summary', 'shading': shading, 'active_windows': len(rates),
              'active_ready_pose_fps': sum(n for n, _ in rates) / sum(t for _, t in rates),
              'min_active_window_fps': min(n/t for n, t in rates),
              'active_event_ready_pose_fps': sum(n for n, _ in event_rates) / sum(t for _, t in event_rates),
              'min_active_event_window_fps': min(n/t for n, t in event_rates)})
        call('Play Animation')
finally:
    current = call('State')
    if call('ping')['pid'] == pid and current['file'] == before['file'] and current['selection'] == original_selection:
        if call('diagnostics')['modes']['playing']:
            call('Play Animation')
        call('Set Frame', frame=before['frame'])
        call('Viewport Shading', shading=original_shading)
        after = call('State')
        assert after['nodes'] == before['nodes'], 'Node state changed'
        assert after['selection'] == original_selection and after['dirty'] == before['dirty']
        assert hashlib.sha256(scene.read_bytes()).hexdigest() == scene_hash
        emit({'kind': 'restored', 'frame': after['frame'], 'nodes_unchanged': True,
              'scene_file_unchanged': True})
    else:
        emit({'kind': 'human_takeover', 'automatic_restore_skipped': True})
