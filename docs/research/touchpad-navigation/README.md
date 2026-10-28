# Touchpad navigation and native Mac build preparation

Basic UI-event tests reproduced3missing behaviors before the implementation (pan, pinch, Alt-orbit). The first implementation passed4tests. Expanded verification passed15focused tests, then the entire UI lib suite passed433tests/2ignored, using --test-threads=1. Frozen production/test source digests are in source-sha.json. Native installed-app verification and physical device verification were still pending at this checkpoint.

Coverage includes hovered non-active views, orthographic/perspective pan axes, Shift convention, native pinch, Ctrl/Command/Alt priority, native event factors applied once, ordinary Line/Page zoom compatibility, mixed-event ordering at zoom limits, disabled views, scene-camera command/atomic Undo including Fov/PanCamera modes, persisted device preferences/old configuration defaults, Preferences UI and native input parsing/validation.

Auto interprets Point events as touchpad and Line/Page as wheel. Mouse retains scroll-to-zoom even for precision Point events. Touchpad explicitly handles lossy X11 Line events as two-finger navigation. The pinned winit X11 backend uses LineDelta for smooth valuator scrolling as well as wheel buttons; auto-detection cannot infer the missing physical device identity. Native Linux pinch delivery has not been proven by synthetic Zoom-event tests.

The local Mac .app builder passes bash syntax validation, actual Linux platform-guard RED/GREEN, and the production plist generator was executed against real Cargo metadata and Git state. No Mac executable, signature, launch, Metal rendering, physical touchpad or Mac performance is certified. See docs/MACOS.md for the explicit native-verification blocker. No public CI release or push was performed.
