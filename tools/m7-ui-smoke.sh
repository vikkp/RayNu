#!/usr/bin/env bash
# M7.4 host/CI smoke: Ops Web UI MVP → RAYNU-V-M7-UI-OK.
# Proves create-VM fields (CPU/RAM/disk/ISO) + media wiring in SPA.
# Console / TLS / firmware NIC remain residual (see docs/runbooks/ops_ui.md).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_M7_UI:-RAYNU-V-M7-UI-OK}"

if [[ ! -f "$ROOT/assets/webui.html" ]]; then
  echo "error: missing assets/webui.html" >&2
  exit 1
fi
if ! grep -q 'data-raynu-m7-ui' "$ROOT/assets/webui.html"; then
  echo "error: SPA must mark M7.4 UI" >&2
  exit 1
fi
if ! grep -q '/spec/' "$ROOT/assets/webui.html"; then
  echo "error: create-VM spec path required" >&2
  exit 1
fi
if ! grep -q 'GAP(CLOSED M7.4): Network create-VM + ISO UI' "$ROOT/mgmt/m7_ui_gate.rs"; then
  echo "error: UI GAP must be CLOSED M7.4" >&2
  exit 1
fi
if [[ ! -f "$ROOT/docs/runbooks/ops_ui.md" ]]; then
  echo "error: missing docs/runbooks/ops_ui.md" >&2
  exit 1
fi

echo "==> cargo test m7_4_ui_gate_passes (artifact gate)"
cargo test --lib m7_4_ui_gate_passes -- --nocapture

echo "==> cargo test prop_create_vm_spec"
cargo test --lib prop_create_vm_spec -- --nocapture

echo "$MARKER"
echo "==> M7.4 Ops UI smoke PASSED"
