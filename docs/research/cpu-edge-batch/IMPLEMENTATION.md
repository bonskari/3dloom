# Experimental exact CPU-edge batching

Candidate only, default **off**. Set `LOOM_EXPERIMENTAL_CPU_EDGE_BATCH=1` before starting the app to enable the candidate. No user preference, shader, skin math, visibility rule or geometry suppression was added. Native performance/default-promotion evidence is pending.

`ViewportCallback::prepare` defers ordinary per-MeshBuffer CPU-edge writes, then calls one persistent `CpuEdgeBatch::update`. This concatenates complete existing 24-byte LineVertex records in current mesh/staging order and uploads one shared VBO. No position/color arithmetic is introduced. First-frame MeshBuffer constructors still create their ordinary buffers; steady updates omit their edge writes. The batch holds reusable CPU staging and one grow-as-needed VBO, and each current CPU MeshBuffer retains a buffer handle and draw range. Empty batches do not write. Oversized combined batches retain the full ordinary per-node upload path.

GPU edge streams remain separate and keep their existing current-source proof. Failed/missing/mismatched GPU sources use ordinary staging and join the CPU batch. Face vertices/indices, material-wire lines, AABBs, picking meshes and helper lines remain unchanged.

Draw ranges use staging length for storage offsets and public edge_vertex_count for each draw length. This preserves truncated public draw counts without aliasing the next node. Default and selected passes independently merge only consecutive adjacent ranges of the same actual buffer, within their existing pipeline, with an even previous vertex count. An odd trailing vertex remains discarded within its node's LineList; it cannot pair with the next node. Selected traversal order, including reversed/repeated indices, is preserved. GPU buffers interrupt a CPU run. Draw range firstVertex can be odd; its own primitive grouping begins there.

## Gates run

- Initial TDD compile RED: missing CpuEdgeBatch API (`initial-red.log`). This is an API-missing RED, not a rendered-regression reproduction.
- Final serial Render suite: **36 passed**, including real GPU byte readback through grow/shrink/remove/reorder/empty/±0/color/diagonal/invisible-edge changes.
- Pixel reference compares candidate against ordinary per-node default draws and legacy white selection buffers, with material-wire faces and blue subobject lines. Linear format covers CPU batching with an odd public count; sRGB covers mixed CPU/GPU streams. Selection permutations include reversed and repeated nodes.
- Final UI targeted GPU callback test: **1 passed**, covering current poses, source withdrawal/mismatch, diagonal CPU fallback, source-buffer replacement, visibility edits, color changes and multi-view face readiness, with batching enabled explicitly in the test resources. Render dependency was rebuilt from the final frozen sources.
- `git diff --check` passed.

The odd-count fixture initially failed its old white-reference full-count assertion (20 vs19) before drawing. That was fixture setup, not evidence of a pixel failure; the reference now permits intentionally truncated draws. Final green logs correspond to complete-staging storage offsets and the explicit even-length merge guard.

Exact frozen sources: `source-sha256.txt`. No native app or scene was touched by these tests. No FPS, queue-write reduction measurement or default enablement is claimed yet. The parent's `benchmark.py` is a separately owned native harness.
