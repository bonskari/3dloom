#!/usr/bin/env bash
# Rebuild and (re)start 3dLoom on the virtual display, waiting for its control port.
# Set LOOM_VK_ICD to a Vulkan ICD json to pin the GPU the app renders on.
set -euo pipefail
port="${1:-7841}"
cd "$(dirname "$0")/.."
cargo build -q -p loom_app
pkill -x loom || true
sleep 1
if [ -n "${LOOM_VK_ICD:-}" ]; then export VK_DRIVER_FILES="$LOOM_VK_ICD"; fi
nohup tools/run-virtual.sh "$port" >/tmp/loom-agent.log 2>&1 &
for _ in $(seq 30); do
  ss -ltn | grep -q ":$port " && { echo "loom up on :$port"; exit 0; }
  sleep 1
done
echo "loom did not open :$port, see /tmp/loom-agent.log" >&2
exit 1
