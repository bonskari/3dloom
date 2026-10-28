# macOS and touchpad scope — 2026-10-28

User requested macOS support and explicitly confirmed touchpad support for both macOS and Linux. This is a source audit and implementation scope, not a claim of delivered Mac support. Changes stay local main with no push.

## Touchpad behavior

Implementation choice stated to the user: pixel/two-finger scroll pans in both axes; Alt plus pixel scroll orbits; native pinch zooms; line-based mouse wheel retains zoom behavior. Ctrl/Command plus two-finger scroll provides zoom where the window system does not deliver native pinch. Only the hovered enabled viewport responds. Axis views become free orthographic views on orbit, just like mouse orbit. Scene-camera navigation must use the existing command/undo route.

The baseline viewport_widget.rs read only raw_scroll_delta.y, losing horizontal precision scrolling and ignoring Event::Zoom. The implementation now routes raw events through the existing viewport/scene-camera navigation path and offers persisted Automatic/Mouse/Touchpad modes for platforms that lose device identity. The pinned egui-winit0.31 adapter already maps native PinchGesture to Event::Zoom and PixelDelta to MouseWheelUnit::Point. Its Linux Wayland backend has no pinch handling in the inspected source; receiving physical Linux pinch must not be claimed from synthetic Event::Zoom tests. Preserve a usable Ctrl-scroll zoom alternative and verify physical devices separately.

Regression scope: actual headless application event routing for two-axis pan, pinch applied once, Alt-orbit including fixed-axis views, modifier zoom without double zoom, side-panel exclusion, ordinary mouse-wheel behavior, and scene-camera navigation/undo. Follow with installed-app IPC events and visual checks; these do not replace physical hardware validation.

## macOS findings

- eframe0.31/wgpu24 are the existing UI/render dependencies; no replacement GUI framework is planned. Runtime GPU capability checks must remain authoritative; the precise GPU pose program needs SHADER_F64, and unsupported adapters retain the accurate CPU path. Mac animation performance is unmeasured.
- .github/workflows/build.yml currently builds/releases only Linux and Windows. update::platform likewise has only linux-x64 and windows-x64 entries. There is no existing Mac app bundle packaging in the inspected build workflow.
- Common keyboard shortcuts already normalize ctrl OR command in hotkeys.rs. Do not rewrite this working mapping as an assumed missing feature.
- Unix IPC exists, but spawn_unix infers UID from /proc/self and skips that comparison if unavailable. live::process_alive returns true on every non-Linux platform. Audit portable OS-backed user/process checks for Mac rather than copying Linux filesystem assumptions.
- A bundle update cannot be implemented by blindly replacing an inner executable and assuming its outer code signature remains valid. Verify the actual bundle/update/signing behavior before enabling Mac release updates.

Native build/package/test work must cover Apple Silicon and any requested Intel support, Metal rendering, loading the real scene/materials, saving/reopening, controls/clipboard/dialogs and IPC. Actual Mac hardware/CI verification is required before completion. User has been asked which Mac and macOS version are available; no answer recorded yet. Gateway server discovery returned one Linux server and no Mac (no next page), so no registered Mac test target was found there.

GitHub currently documents native arm64 macos-15 and Intel macos-15-intel runners: https://docs.github.com/en/actions/reference/runners/github-hosted-runners . These are possible future native checks, not jobs that have been run. Do not push the local changes merely to trigger them without user authorization.

## Implementation checkpoint

The15focused touchpad tests and full433passed/2ignored UI lib suite pass. The local native Mac app builder is tools/build_macos.sh; only its Linux guard, Bash syntax and real-Cargo-metadata plist generation have been verified here. Installed-app/hardware verification remains separate.
