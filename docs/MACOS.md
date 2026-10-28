# Native macOS build

**BLOCKER: no native Mac build or Metal runtime has yet been verified.** The available registered test host is Linux. This document and the build script do not establish completed Mac support.

On a Mac with Rust and Xcode Command Line Tools, from this repository:

```sh
bash tools/build_macos.sh
```

The script builds the actual `loom_app` executable for the execution architecture reported by `uname -m` (arm64 or x86_64), checks the produced Mach-O architecture, and puts it in a new timestamped `.app` bundle under `target/macos/`. It reads the version from Cargo metadata and records the real Git commit and dirty state. Previous bundles are retained. The bundle identifier chosen for this project is `io.github.bonskari.3dloom`.

The output has a local ad-hoc code signature, a ZIP and a SHA-256 checksum. It is not a Developer ID signed/notarized public release. Signed bundle updating is not implemented or enabled by this local build. No release is uploaded or published.

Native verification must still cover launching the app, Metal rendering of the real Ray scene with its materials, loading/saving/reopening, Command shortcuts, local IPC, physical touchpad input and playback performance. The existing precise GPU pose path requires adapter capabilities that must be checked on the actual Mac; Linux performance cannot certify Mac performance.

Touchpad navigation uses two-finger pan, Alt/Option orbit and pinch or Ctrl/Command scrolling for zoom. Preferences → Viewports → Input device offers Automatic, Mouse and Touchpad. Choose Touchpad if the window system reports two-finger scrolling as ordinary wheel steps; choose Mouse to retain scroll-to-zoom for a precision mouse. Mac hardware input verification remains pending.

Apple references: [bundle configuration](https://developer.apple.com/documentation/bundleresources/bundle-configuration), [code signature verification](https://developer.apple.com/library/archive/documentation/Security/Conceptual/CodeSigningGuide/Procedures/Procedures.html), [signing without deep signing](https://developer.apple.com/documentation/xcode/creating-distribution-signed-code-for-the-mac/).
