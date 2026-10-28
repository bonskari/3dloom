# Full-u16 material-ID bitset: isolated real-Ray probe

No production edits, GPU jobs or native FPS claim. The candidate uses `[u64;1024]`, filters hidden faces, then emits `Vec<Option<u16>>` in ascending ID order. It does not assume IDs below64 and does not filter missing material-library entries.

The independent std::collections::BTreeSet reference passed empty input;0/63/64/65535 boundaries/duplicates/hidden entries; and every u16 value with all-visible, all-hidden, modulo3-hidden, and alternating-hidden patterns. Each actual skin's ascending result was also exact.

Input frozen real Ray SHA256:84fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935. 461nodes, frame39, actual evaluated geometry; setup/load/evaluation outside collection timing. Source hashes, exact release compiler dependency arguments, original GPU-loop source, candidate and raw results are retained. No diagnostics or preferences were archived.

Classification is computed from **both actual paths**, not inferred from face IDs: UI wanted uses scene.node_surfaces filtered by frozen-gray and shaded ColorSource::Material; actual Bevy source uses scene.node_surfaces_for(...ShellView::Viewport). Both paths agreed for this document:

| Object | Node | Actual surfaces | Faces | Visible IDs | Raw collection old/new ms |
|---|---:|---|---:|---|---:|
| Body001 |4543|Single|23560|0|0.028772 /0.009791 (diagnostic only)|
| Face001 |4544|PerId|63308|0–8|0.074619 /0.026242|
| SkeletalMesh001 |4545|PerId|5438|0|0.006851 /0.002705|
| SkeletalMesh002 |4546|PerId|14199|0|0.017094 /0.006066|
| SkeletalMesh003 |4547|PerId|4818|0|0.006112 /0.002482|

All five have zero hidden faces in this source pose. Body001's raw-collector timing is **not production work**: the actual branch returns vec![None] without inspecting its faces. It must not be counted as a bitset saving.

The final actual-five benchmark includes precisely that Single branch and the four real PerId collectors. Warm50 calls per variant,4ABBAcycles,600five-object calls per block:

- Original actual-five branch:0.104960821ms/call (block range0.104264092–0.105412557).
- Candidate actual-five branch:0.037457499ms/call (0.037243072–0.037622953).
- Saving:0.067503323ms/call. Per-cycle differences0.067598635,0.067240720,0.067663077,0.067510860ms.

Per-object collectors above were measured separately with warm30 and4ABBAcycles×300iterations per object/block; differences between those and the final combined branch are retained as separate measurements. This small isolated saving is not a prediction that native playback crosses60FPS. Root's live Scope measurement of the original five branches was approximately0.136–0.138ms/frame; these are distinct measurement contexts.

Root release74269 and native profile89191 were terminal before permission to compile. App remained paused with idle repaints. Direct compile74985/run93129 passed; supplemental actual-five compile24022/run5731 passed. All jobs terminated and the quiet window was released to the next agent. No subsequent prototype expansion.

Reproduce: put these files in target/material-id-bitset-probe; after a coherent completed release build, run compile.py and only run probe if compilation succeeds. The compiler resolves exact release app/UI/commands dependency fingerprints. The probe only reads the frozen document and prints JSON; it does not write scenes or backups. `result.json` is the first per-object sweep; `result-actual.json` contains the final per-object sweep plus actual-five branch rows.

## Production checkpoint

After explicit approval, only gpu_pose_renderer.rs was changed: a private visible_material_ids face-collector helper replaces BTreeSet in the existing PerId branch. Single vec![None] and gpu_pose.material_ids Scope are unchanged. No UI/runtime/shader/API/data-model change.

TDD: production RED compile3503 failed solely because visible_material_ids did not yet exist. GREEN23678 passed both boundary/full-u16 reference tests. The opt-in actual-Ray test99966 passed all five evaluated meshes at0/39/149 against independent std BTreeSet. Full cargo test -p loom_bevy -j4 -- --test-threads=1 (44790) exited0:81library tests passed,35ignored;1bake and7render integration tests passed;0doctest failures. git diff --check passed. No native FPS claim is made; combined release/native verification belongs to the parent task.
