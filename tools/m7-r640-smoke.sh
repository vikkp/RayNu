#!/usr/bin/env bash
# M7.5 host/CI smoke: R640 boot **scaffold** → RAYNU-V-M7-R640-SCAFFOLD-OK.
#
# Proves runbook + evidence package + ship-kit cross-refs.
# Does **never print iron marker** RAYNU-V-R640-BOOT-OK from this host path.
# Iron close is documented in docs/evidence/r640/ (STATUS=closed) after real
# PowerEdge serial — see docs/runbooks/r640_boot.md.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

SCAFFOLD="${MARKER_M7_R640_SCAFFOLD:-RAYNU-V-M7-R640-SCAFFOLD-OK}"
IRON="${MARKER_M7_R640:-RAYNU-V-R640-BOOT-OK}"

if [[ ! -f "$ROOT/mgmt/m7_r640_gate.rs" ]]; then
  echo "error: missing mgmt/m7_r640_gate.rs" >&2
  exit 1
fi
if [[ ! -f "$ROOT/docs/runbooks/r640_boot.md" ]]; then
  echo "error: missing docs/runbooks/r640_boot.md" >&2
  exit 1
fi
if [[ ! -f "$ROOT/docs/runbooks/r640_iron_week.md" ]]; then
  echo "error: missing docs/runbooks/r640_iron_week.md" >&2
  exit 1
fi
if [[ ! -f "$ROOT/docs/runbooks/r640_field_guide.md" ]]; then
  echo "error: missing docs/runbooks/r640_field_guide.md" >&2
  exit 1
fi
if [[ ! -f "$ROOT/docs/evidence/r640/TEMPLATE.md" ]]; then
  echo "error: missing docs/evidence/r640/TEMPLATE.md" >&2
  exit 1
fi
if [[ ! -f "$ROOT/docs/evidence/r640/STATUS" ]]; then
  echo "error: missing docs/evidence/r640/STATUS" >&2
  exit 1
fi
if [[ ! -f "$ROOT/docs/evidence/r640/2026-08-15-r640-first-light.md" ]]; then
  echo "error: missing filled first-light evidence" >&2
  exit 1
fi
if ! grep -q 'STATUS=closed' "$ROOT/docs/evidence/r640/STATUS"; then
  echo "error: evidence STATUS must be closed after iron M7.5" >&2
  exit 1
fi
if ! grep -q 'GAP(CLOSED M7.5)' "$ROOT/mgmt/m7_r640_gate.rs"; then
  echo "error: R640 GAP must be GAP(CLOSED M7.5) after iron evidence" >&2
  exit 1
fi
if ! grep -q 'RAYNU-V-M3-SHELL-OK' "$ROOT/docs/evidence/r640/2026-08-15-r640-first-light.md"; then
  echo "error: first-light evidence must record SHELL-OK" >&2
  exit 1
fi
if ! grep -q "$IRON" "$ROOT/docs/evidence/r640/2026-08-15-r640-first-light.md"; then
  echo "error: first-light evidence must claim $IRON" >&2
  exit 1
fi
if ! grep -q 'RAYNU-V-M4-SMP-OK' "$ROOT/docs/evidence/r640/logs/2026-08-15-xsavesfix-com2.txt"; then
  echo "error: xsavesfix COM2 archive must include M4-SMP-OK" >&2
  exit 1
fi
if ! grep -q 'stack guard' "$ROOT/docs/evidence/r640/logs/2026-08-15-keepconfix-com2.txt"; then
  echo "error: keepconfix COM2 archive must include stack guard residual" >&2
  exit 1
fi
if ! grep -q 'r640-hypervisor.efi' "$ROOT/tools/package-release.sh"; then
  echo "error: package-release must still name r640-hypervisor.efi" >&2
  exit 1
fi

echo "==> cargo test m7_5_r640_scaffold_passes (scaffold gate)"
cargo test --lib m7_5_r640_scaffold_passes -- --nocapture

# Honesty: this script must not emit the iron marker on stdout.
echo "$SCAFFOLD"
echo "==> M7.5 R640 scaffold smoke PASSED (iron ${IRON} claimed only in docs/evidence/r640/)"
