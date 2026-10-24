#!/usr/bin/env bash
# Run 3dLoom on a virtual X display only, never on the user's Wayland screen.
# Usage: tools/run-virtual.sh [port] — uses Xvnc :90 if alive, else xvfb-run.
set -euo pipefail
port="${1:-7833}"
cd "$(dirname "$0")/.."
export LOOM_CONTROL_PORT="$port" XDG_SESSION_TYPE=x11 WINIT_UNIX_BACKEND=x11
unset WAYLAND_DISPLAY
if DISPLAY=:90 xdpyinfo >/dev/null 2>&1; then
  DISPLAY=:90 exec target/debug/loom
else
  exec xvfb-run -a -s "-screen 0 1600x1000x24" target/debug/loom
fi
