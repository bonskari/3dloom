# Internal current-position GPU runtime

The runtime is test-registered and not activated in the native renderer. It accepts actual CPU final world-baked positions, reconstructs current GPU normals, batches material preparation, and evaluates current GPU recipe math. Initial/rejected structural recipes are compiled by the real early-stop CPU compiler from the current GPU intermediate position/normal/UV readback. No CPU final tangent oracle is consumed by runtime construction or updates.

Candidates expose output/guard buffers, original current CPU corner-gather positions and local triangle indices for the separately validated atomic pose commit. Guard recovery recompiles rejected CURRENT structures, reruns their math, and rejects the pose if fresh guards still fail. Descriptor changes create a complete new candidate cache. The caller must still use atomic guard-checked commit; exposing a candidate never authorizes an unchecked render copy.

Synthetic GPU checks passed cached updates, a forced guard failure, a natural current-position weld/degenerate change, same-count faceted descriptor change, and byte-for-byte comparison with a fresh current runtime. Actual five-skin/13-part updates at0,39.25 and149 also matched fresh current runtime output bytes for every part. These prove wrapper/cache behavior; they do not independently establish shading quality. The separate GPU numerical/image audits retain that responsibility.

The initial actual test observed16.174ms per pose for cache-hit descriptor comparison plus CPU position upload/gather. This was measured inside a correctness run, not a quiet/native FPS benchmark. The current descriptor implementation reconstructs Topology and fans on every comparison; phase profiling and an exact descriptor comparison investigation follow. No native performance claim is made.

The default Bevy suite passed61 unit tests, one bake and seven render tests, with28 opt-in tests ignored.
