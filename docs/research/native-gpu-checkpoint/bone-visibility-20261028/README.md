# Native bone category verification

Installed local commit `9e0fe7c`; previous binary preserved locally. No scene save was needed or performed. Scene SHA stayed unchanged. Reopen and final readback retained every node record, frame 39 and original selection 4988.

The full-scene screenshot matches all four previous viewport crops pixel for pixel. With Hide Geometry enabled, explicit Display Bones now retains bone geometry in the orthographic views. Helpers are still classified as Geometry, so this test temporarily cleared the selected helper; full display and the original selection were explicitly restored afterward.

The Realistic screenshot still shows the imported bones too faintly to use reliably. This is a remaining product issue, not a completed skeleton-visibility goal. The native test was performed while an isolated GPU correctness sweep was running; it is not a performance measurement.
