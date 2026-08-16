#!/usr/bin/env bash
# M7.7 host/CI smoke: ISO install-to-disk **scaffold** → RAYNU-V-M7-ISO-INSTALL-SCAFFOLD-OK.
# Proves runbook + phase machine + virtio install capacity + REST begin.
# Does NOT claim RAYNU-V-M7-ISO-INSTALL-OK (QEMU/iron only) — never print iron marker.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

SCAFFOLD="${MARKER_M7_ISO_INSTALL_SCAFFOLD:-RAYNU-V-M7-ISO-INSTALL-SCAFFOLD-OK}"
IRON="${MARKER_M7_ISO_INSTALL_OK:-RAYNU-V-M7-ISO-INSTALL-OK}"

if [[ ! -f "$ROOT/mgmt/iso_install.rs" ]]; then
  echo "error: missing mgmt/iso_install.rs" >&2
  exit 1
fi
if ! grep -q 'fn prop_iso_install_package(' "$ROOT/mgmt/iso_install.rs"; then
  echo "error: missing prop_iso_install_package" >&2
  exit 1
fi
if ! grep -q 'GAP(CLOSED M7.7): ISO install-to-disk' "$ROOT/mgmt/iso_install.rs"; then
  echo "error: ISO install GAP must be CLOSED M7.7 after iron reboot-to-disk" >&2
  exit 1
fi
if ! grep -q 'reboot-to-disk' "$ROOT/mgmt/iso_install.rs"; then
  echo "error: reboot-to-disk MVP note required" >&2
  exit 1
fi
if [[ ! -f "$ROOT/docs/runbooks/iso_install.md" ]]; then
  echo "error: missing docs/runbooks/iso_install.md" >&2
  exit 1
fi
if [[ ! -f "$ROOT/docs/evidence/r640/TEMPLATE-iso-install.md" ]]; then
  echo "error: missing docs/evidence/r640/TEMPLATE-iso-install.md" >&2
  exit 1
fi
if [[ ! -f "$ROOT/docs/evidence/r640/STATUS-iso-install" ]]; then
  echo "error: missing docs/evidence/r640/STATUS-iso-install" >&2
  exit 1
fi
if ! grep -q 'STATUS=closed' "$ROOT/docs/evidence/r640/STATUS-iso-install"; then
  echo "error: STATUS-iso-install must be closed after iron BOOTED-FROM-DISK" >&2
  exit 1
fi

echo "==> cargo test m7_7_iso_install_scaffold_passes (scaffold gate)"
cargo test --lib m7_7_iso_install_scaffold_passes -- --nocapture

echo "==> cargo test iso_install_package"
cargo test --lib iso_install_package -- --nocapture

echo "$SCAFFOLD"
echo "==> M7.7 ISO install-to-disk scaffold smoke PASSED (iron close is COM2 BOOTED-FROM-DISK; never print ${IRON})"
# never print iron marker from host scaffold smoke
true
