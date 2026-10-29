# Saved state through selection Undo/Redo

The native c4708d0 app reproduced the fault: three selection-only operations stayed clean, but their first Undo became dirty despite unchanged scene nodes. The retained native RED verifier records that failure and safely restores its selection-only test state. Core regressions independently reproduced the saved-boundary failure.

Selection markers now carry an explicit runtime-only tag through Undo/Redo, including failed-apply recovery. Traversing a selection marker from a saved boundary moves that boundary with it. Actual edits, ordinary empty change groups (which can carry timing sidecars), unsaved scenes and branch invalidation retain their previous meanings. There is no content hashing, dirty-flag override or file/schema change.

All 13 Undo tests pass, including repeated markers, Undo/Redo, unsaved state, real edits/empty groups, branch invalidation, limits and retained pose history. All 9 command selection tests pass, including unchanged serialized scene data and a real Move remaining dirty until undone. Read-only source review found no concrete correctness issue. Full Scene/Commands/UI/Bevy lib+integration verification passed; the UI subset passed 460/0 failed/3 ignored in 72.15s. Logs are `/tmp/loom-selection-saved-core.log`, `/tmp/loom-selection-saved-commands.log` and `/tmp/loom-selection-saved-regressions.log`.

The installed native app still runs c4708d0; acceptance of this correction in the replacement binary remains pending. `docs/research/selection-saved-native/verify.py` asserts clean selection Undo/Redo and proper real-edit dirty status before any cleanup reload. The full goal remains active, including performance/stability dips and the remaining animation/Mac workflow. No original Ray write or public push.
