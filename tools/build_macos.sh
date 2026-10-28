#!/usr/bin/env bash
set -euo pipefail

if [[ $(uname -s) != Darwin ]]; then
    printf '%s\n' 'This native build requires macOS with Xcode Command Line Tools and Rust.' >&2
    exit 1
fi

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"
arch=$(uname -m)
case "$arch" in
    arm64) rust_target=aarch64-apple-darwin ;;
    x86_64) rust_target=x86_64-apple-darwin ;;
    *) printf 'Unsupported Mac architecture: %s\n' "$arch" >&2; exit 1 ;;
esac
for tool in cargo python3 xcrun codesign plutil lipo ditto shasum; do
    command -v "$tool" >/dev/null
done
xcrun --find clang >/dev/null

mkdir -p "$root/target/macos"
out="$root/target/macos/$(date -u +%Y%m%dT%H%M%SZ)-$arch"
mkdir "$out"
cargo metadata --locked --no-deps --format-version 1 > "$out/metadata.json"
build_target_dir=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["target_directory"])' "$out/metadata.json")

# Mac signed-release bundle updates are not implemented; this is a local build.
unset LOOM_UPDATE_CHANNEL
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" cargo build --locked --release -p loom_app --bin loom --target "$rust_target"
binary="$build_target_dir/$rust_target/release/loom"
[[ $(lipo -archs "$binary") == "$arch" ]]
bundle="$out/3dLoom.app"
mkdir -p "$bundle/Contents/MacOS"
install -m 755 "$binary" "$bundle/Contents/MacOS/loom"
python3 - "$out/metadata.json" "$bundle/Contents/Info.plist" <<'PY'
import json
import plistlib
import subprocess
import sys

with open(sys.argv[1]) as source:
    metadata = json.load(source)
package = next(p for p in metadata["packages"] if p["name"] == "loom_app")
version = package["version"]
release_version = version.split("-", 1)[0].split("+", 1)[0]
info = {
    "CFBundleExecutable": "loom",
    "CFBundleIdentifier": "io.github.bonskari.3dloom",
    "CFBundleInfoDictionaryVersion": "6.0",
    "CFBundleName": "3dLoom",
    "CFBundleDisplayName": "3dLoom",
    "CFBundlePackageType": "APPL",
    "CFBundleShortVersionString": release_version,
    "CFBundleVersion": release_version,
    "NSHighResolutionCapable": True,
    "NSPrincipalClass": "NSApplication",
    "LoomVersion": version,
    "LoomGitCommit": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
    "LoomGitDirty": bool(subprocess.check_output(["git", "status", "--porcelain"], text=True).strip()),
}
with open(sys.argv[2], "wb") as target:
    plistlib.dump(info, target)
PY
plutil -lint "$bundle/Contents/Info.plist"
codesign --force --sign - "$bundle"
codesign --verify --deep --strict --verbose=2 "$bundle"
archive="3dLoom-macos-$arch.zip"
ditto -c -k --sequesterRsrc --keepParent "$bundle" "$out/$archive"
(cd "$out" && shasum -a 256 "$archive" > "$archive.sha256")
printf 'App: %s\nArchive: %s\n' "$bundle" "$out/$archive"
