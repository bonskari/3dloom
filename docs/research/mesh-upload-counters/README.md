# Actual mesh queue-write counters

Diagnostic-only instrumentation in `loom_render/src/profile.rs` and `mesh_buffer.rs`; no changes to geometry, shader layouts, drawing or upload decisions.

When `LOOM_PROFILE_FRAMES` is present, each actual nonempty `queue.write_buffer` in `mesh_buffer::update_buffer` records one write plus `contents.len()` bytes. Labels separate streams:

- `loom_mesh_edge_buffer`: ordinary CPU-generated unique-edge stream (GPU-gather objects omit this write).
- `loom_mesh_vertex_buffer`: shaded triangle vertices.
- `loom_mesh_index_buffer`: shaded triangle indices.
- `loom_mesh_wire_buffer`: material-wire face-edge stream.

Existing five-second reporting drains counters and emits `[loom_profile_counter] <label> writes=<count> bytes=<bytes>` separately from timed `[loom_profile]` scopes. Disabled recording checks the existing cached flag without locking or reading the clock. Enabled recording adds one mutex acquisition/map update per measured write; profiling overhead must not be represented as normal-production FPS.

Empty streams and allocation/growth via `create_buffer_init` are intentionally excluded: these counters count this exact queue-write call site, not all GPU transfers. GPU compute/gather, edge style-uniform writes and other queue-write call sites are outside this metric. Scope counts such as `render.upload.queue` remain MeshBuffer update counts and must not be interpreted as write counts.

Verification: `cargo test -p loom_render --lib -j4 -- --test-threads=1` passed all 35 tests, including real GPU byte-readback and selected-line pixel regression checks. Log: `render-tests.log`. Native write totals/bytes have not yet been collected with this instrumentation; no batching savings or FPS claim is made.
