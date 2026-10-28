# Standalone GPU edge gather: actual Ray parity evidence

This records a completed correctness experiment for the intermediate Render
component in commit `35e58f0`. It is not an end-to-end performance measurement,
production activation, or proof of the subsequently implemented renderer/UI
lifecycle. No test/build/GPU job was run while archiving this evidence.

## Input and provenance limitation

The experiment read the real saved scene at:

```
/mnt/2tb/hollowreach-ue/Hollowreach/SourceArt/RayLoom20261025/RayAnimationTest_LoomRig.loom
```

The wrapper JSON's `scene` was deserialized through the actual Scene type.
Every one of the five Skin-bearing nodes was evaluated through Scene at each
of 317 times `149.0 * frame / 316.0` for integer frames 0 through 316. Each
result was CPU-transformed with the current object matrix, matching the source
position contract. All source vertices were uploaded, including vertices not
referenced by visible triangles. The probe used the saved display wire colour.
It asserted that each actual skin's diagonal style was unsupported=false; no
style was silently disabled to obtain a passing experiment.

**Historical input SHA was not recorded.** The application saved this same
path after the experiment. The probe source/executable have local file time
02:33 on 28 October 2026; the input later showed 02:41. No historical input
snapshot was retained. A current-file hash cannot identify the bytes used by
this run, and is deliberately not substituted. Reproducing with the current
scene is a new experiment, not reconstruction of an attested historical input.

## What was compared

For every skin at every time:

1. Construct the component once on the first pose with its ordered endpoint
   recipe and shared GPU program; retain source/output buffers on later poses.
2. Check source identity/range and exact topology/effective visibility guards.
3. Upload actual current CPU world positions and update current wire colour.
4. Encode gather for all five objects, then copy their outputs into one staging
   buffer and map/read it after submission.
5. Compare every byte against CPU `Mesh::unique_edges()` positions and the same
   current colour, encoded as 24-byte `LineVertex` records. Counts must agree.

The source asserts five Skin nodes and checks every output byte; it does not
compare a sampled subset or truncate to the shorter stream. The combined
five-object staging buffer is read on every pose. Zero tolerances apply.

The preserved log reports:

```
GPU_EDGES_ACTUAL317 exact_streams=1585 vertices=110466258 descriptor_hits=1580
```

Thus all 317 × 5 streams matched, covering 110,466,258 emitted line vertices.
The 316 × 5 subsequent-pose descriptor checks hit without rebuilding. These
numbers concern edge vertices, not the skinned mesh's unique vertex count.

## Source, command and artifact hashes

`probe.rs` and `build.py` are the actual standalone source/build script used.
The script resolves matching already-built release dependency fingerprints and
runs optimized rustc; it does not invoke Cargo or modify repository artifacts.
It contains this host's absolute repository/temp paths and a cached pollster
fingerprint. Those caches are prerequisites, not a portable test harness.
Its build-argument diagnostic also writes under `/tmp/loom_async_probe`.

The original commands were:

```bash
cd /home/b1s/dev/github/3dloom
python3 /tmp/loom_gpu_edge_probe/build.py > /tmp/loom_gpu_edge_probe/build.log 2>&1
/tmp/loom_gpu_edge_probe/probe /mnt/2tb/hollowreach-ue/Hollowreach/SourceArt/RayLoom20261025/RayAnimationTest_LoomRig.loom > /tmp/loom-gpu-edges-ray317.log 2>&1
```

To rerun this exact script on the same host, copy the archived source/script
back to those temp paths first and ensure the matching release fingerprints
exist. Run only after coordinating the native/GPU measurement slot. Source
is imported from the working tree, so record its revision and input hash
**before** any new run. Do not label a new run as the historical one.

SHA-256:

| Artifact | SHA-256 |
| --- | --- |
| `probe.rs` | `41b26e7338086836d57161b78d105be1d468466ae1b1194f53e032f2e0b0064b` |
| `build.py` | `720d581ad5ecb3bb3d034c6a0a0dca7d5609e981d71ff5d80674ade67d44e9d0` |
| `ray317.log` | `526d69ce4d8eaca1765932ae6cdf55523424e38312d2ec399c3ddf42901c205b` |
| Audited `/tmp/loom_gpu_edge_probe/probe` executable (not committed) | `6ddaa25d37f989284416acca665bcd94d844ccfd623bfe9a7f9ac38498932ba8` |

The standalone binary was compiled before the final output COPY_DST usage flag
and additional unit-test cases were added. Those later edits do not change the
shader/gather arithmetic, but the historical binary is not claimed to be an
exact build of every file byte in commit `35e58f0`. Final current-source GPU
regressions were separately run after that usage change.

## Separate regression coverage

The Render component's three real-GPU tests cover position/colour signed zeros,
pose/colour updates, source-buffer replacement, aligned nonzero ranges, open and
empty meshes, unused source vertices, invisible-edge edits, same-size topology
reordering and invalid storage ranges. Diagonal styles return an explicit error
for the ordinary full-fidelity path. The full Render suite then passed 34 unit
and one opacity integration test. These tests do not establish the UI's pose
version/NodeId proof or failed-sync source withdrawal; integration owns those.
