#!/usr/bin/env bash
# M7.3 host/CI smoke: ISO deploy path → RAYNU-V-M7-ISO-OK.
# Proves ISO register + documented kernel-extract boot + virtio-blk install target.
# El Torito / CD-ROM attach remains stubbed (see docs/runbooks/iso.md).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_M7_ISO:-RAYNU-V-M7-ISO-OK}"

if [[ ! -f "$ROOT/mgmt/iso.rs" ]]; then
  echo "error: missing mgmt/iso.rs" >&2
  exit 1
fi
if ! grep -q 'fn prop_iso_deploy_package(' "$ROOT/mgmt/iso.rs"; then
  echo "error: missing prop_iso_deploy_package" >&2
  exit 1
fi
if ! grep -q 'GAP(CLOSED M7.3): Linux ISO deploy path' "$ROOT/mgmt/iso.rs"; then
  echo "error: ISO GAP must be CLOSED M7.3" >&2
  exit 1
fi
if ! grep -q 'kernel-extract' "$ROOT/mgmt/iso.rs"; then
  echo "error: documented kernel-extract note required" >&2
  exit 1
fi
if ! grep -q 'UnsupportedOnFirmware' "$ROOT/mgmt/iso.rs"; then
  echo "error: CD-ROM stub must be honest" >&2
  exit 1
fi
if [[ ! -f "$ROOT/docs/runbooks/iso.md" ]]; then
  echo "error: missing docs/runbooks/iso.md" >&2
  exit 1
fi

echo "==> cargo test m7_3_iso_gate_passes (artifact gate)"
cargo test --lib m7_3_iso_gate_passes -- --nocapture

echo "==> cargo test iso_deploy_package"
cargo test --lib iso_deploy_package -- --nocapture

echo "==> cargo test register_bind_install_roundtrip"
cargo test --lib register_bind_install_roundtrip -- --nocapture

echo "$MARKER"
echo "==> M7.3 ISO deploy smoke PASSED"
