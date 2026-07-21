#!/usr/bin/env bash
# M7.0 host/CI smoke: EFI release kit → RAYNU-V-M7-SHIP-OK.
# Host gate validates scripts/runbook/props. If an EFI already exists, also
# run package-release.sh (SKIP_BUILD=1) to emit dist/ checksums + tarball.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_M7_SHIP:-RAYNU-V-M7-SHIP-OK}"
TARGET="${TARGET:-x86_64-unknown-uefi}"
PROFILE="${PROFILE:-release}"
EFI="target/${TARGET}/${PROFILE}/r640-hypervisor.efi"

if [[ ! -f "$ROOT/mgmt/ship.rs" ]]; then
  echo "error: missing mgmt/ship.rs" >&2
  exit 1
fi
if ! grep -q 'fn prop_release_kit_package(' "$ROOT/mgmt/ship.rs"; then
  echo "error: missing prop_release_kit_package" >&2
  exit 1
fi
if ! grep -q 'GAP(CLOSED M7.0): EFI release kit' "$ROOT/mgmt/ship.rs"; then
  echo "error: SHIP GAP must be CLOSED M7.0" >&2
  exit 1
fi
if ! grep -q "$MARKER" "$ROOT/mgmt/ship.rs"; then
  echo "error: ship must embed marker $MARKER" >&2
  exit 1
fi
if [[ ! -f "$ROOT/tools/package-release.sh" ]]; then
  echo "error: missing tools/package-release.sh" >&2
  exit 1
fi
if [[ ! -f "$ROOT/docs/runbooks/usb_idrac.md" ]]; then
  echo "error: missing docs/runbooks/usb_idrac.md" >&2
  exit 1
fi
if ! grep -q 'virtual media' "$ROOT/docs/runbooks/usb_idrac.md"; then
  echo "error: runbook must cover iDRAC virtual media" >&2
  exit 1
fi
if [[ ! -x "$ROOT/tools/package-release.sh" ]]; then
  chmod +x "$ROOT/tools/package-release.sh"
fi

echo "==> cargo test m7_0_ship_gate_passes (artifact gate)"
cargo test --lib m7_0_ship_gate_passes -- --nocapture

echo "==> cargo test release_kit_package"
cargo test --lib release_kit_package -- --nocapture

if [[ -f "$EFI" ]]; then
  echo "==> EFI present — packaging release kit (SKIP_BUILD=1)"
  SKIP_BUILD=1 "$ROOT/tools/package-release.sh"
  VERSION="$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
  STAMP="raynu-v-${VERSION}"
  if [[ ! -f "$ROOT/dist/${STAMP}/r640-hypervisor.efi.sha256" ]]; then
    echo "error: missing EFI sha256 sidecar in dist/" >&2
    exit 1
  fi
  if [[ ! -f "$ROOT/dist/${STAMP}.tar.gz" ]]; then
    echo "error: missing release tarball" >&2
    exit 1
  fi
  echo "==> packaged $STAMP"
else
  echo "==> no EFI at $EFI — host artifact gate only (CI m7-ship)"
  echo "    full kit: ./tools/build.sh && ./tools/package-release.sh"
fi

echo "$MARKER"
echo "==> M7.0 EFI release kit smoke PASSED"
