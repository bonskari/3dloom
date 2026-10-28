# Automatic backup responsiveness: frozen Ray snapshot

The original synchronous `autosave::backup_now` release path took **547.731 ms** in this isolated child-process run. Load (929.697 ms) and reopen (673.133 ms) were outside that interval. Independent snapshot clone was 3.935 ms; independent pretty serialization was 356.025 ms. These separate measurements are not additive stage timings.

The run overlapped release build **10945**. Stability test 32851 had already ended. This is not a quiet benchmark or an observed native UI freeze; it establishes the duration of the synchronous source path in this run, not the cause of every reported pause. `result-original.json` preserves the incorrectly emitted context label; `result.json` corrects only its metadata.

Input: `/tmp/loom-conditioning-capture/input.loom`, SHA256 `84fe5289bf2227d474b37e30d04e3267f03a8c0a37be2e30bc8da1117af0f935`. All 461 nodes, five full Skin weight arrays, settings, Ray_OfficeDesk and Ray_OfficeChair node data/world matrices were verified after reopening the private backup. No source scene or existing backup was written. Saved size: 271346303 bytes.

The child used a fresh private `LOOM_HOME` recorded in the result. Compile-time checks prove Document and Scene are Send; the probe moved an owned cloned document to a thread and verified editing an actual animation key in the clone did not alter the source. Scene contains RefCell caches and is not shared live with a worker.

Provenance: successful direct release compile session 67306; child session 97973, PID 2968365, exit 0. A subsequent relink attempt failed while release fingerprints were changing; the shell then ran the previously successful binary. The executed source differed from archived `probe.rs` only by an unused `hash::Hasher` import and the original stale context string. Dependency arguments/fingerprints are preserved. Reproduction must compile successfully before running; never fall back to an old binary after a failed compile.

Reproduce after a coherent release build: run `python3 docs/research/autosave-responsiveness/compile_probe.py` (script currently expects the original target probe directory), then **only after success** launch the binary with the frozen input and a newly generated private `LOOM_HOME` under its compiled `PROBE_ROOT`. Never point it at normal user data. The retained source performs its own input hash and private-directory assertions.

Source audit: UI frame directly calls synchronous tick; tick calls backup_now; file::write pretty-serializes, writes a temporary file, sync_all on the file, renames, and attempts parent-directory sync. The latter remains best effort in the original writer.

Proposed change: capture one owned Document on the UI thread, run the unchanged durable writer on one bounded worker, and acknowledge the captured scene instance/version only after success. Serialize manual AutoBackupNow and any Save target/temp-path collision with that writer; unrelated Save/Hold stay synchronous without waiting. Open/Reset must not acknowledge a replacement scene. Normal close and restart must drain started work before process exit/relaunch. Lifecycle regressions precede production edits.
