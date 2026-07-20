#!/usr/bin/env bash
# M5.2 host/CI smoke: embedded Web UI → RAYNU-V-M5-WEBUI-OK.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_M5_WEBUI:-RAYNU-V-M5-WEBUI-OK}"

if [[ ! -f "$ROOT/assets/webui.html" ]]; then
  echo "error: missing assets/webui.html" >&2
  exit 1
fi
if ! grep -q 'data-raynu-webui' "$ROOT/assets/webui.html"; then
  echo "error: webui.html missing data-raynu-webui marker" >&2
  exit 1
fi
if ! grep -q 'startVm' "$ROOT/assets/webui.html"; then
  echo "error: webui.html missing startVm" >&2
  exit 1
fi
if [[ ! -f "$ROOT/mgmt/webui.rs" ]]; then
  echo "error: missing mgmt/webui.rs" >&2
  exit 1
fi
if ! grep -q 'link_section = ".aswebui"' "$ROOT/mgmt/webui.rs"; then
  echo "error: missing PE section .aswebui" >&2
  exit 1
fi
if ! grep -q 'fn load_webui(' "$ROOT/mgmt/webui.rs"; then
  echo "error: missing load_webui lazy path" >&2
  exit 1
fi
if ! grep -q "$MARKER" "$ROOT/mgmt/webui.rs"; then
  echo "error: webui must embed marker $MARKER" >&2
  exit 1
fi
if ! grep -q 'GAP: webui zstd' "$ROOT/mgmt/webui.rs"; then
  echo "error: zstd GAP note missing" >&2
  exit 1
fi

echo "==> cargo test m5_2_webui_gate_passes (artifact gate)"
out="$(cargo test --lib m5_2_webui_gate_passes -- --nocapture 2>&1)"
echo "$out"
echo "$out" | grep -q 'm5_2_webui_gate_passes ... ok'
echo "$out" | grep -q "$MARKER"

echo "==> cargo test webui_list_start_stop"
out2="$(cargo test --lib webui_list_start_stop -- --nocapture 2>&1)"
echo "$out2"
echo "$out2" | grep -q 'webui_list_start_stop ... ok'

echo "$MARKER"
echo "==> M5.2 Web UI smoke PASSED"
