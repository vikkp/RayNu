#!/usr/bin/env bash
# M7.5 host/CI smoke: R640 boot **scaffold** → RAYNU-V-M7-R640-SCAFFOLD-OK.
#
# Proves runbook + evidence template + ship-kit cross-refs.
# Does **never print iron marker** RAYNU-V-R640-BOOT-OK from this host path.
# Real PowerEdge R640 serial evidence is required to close M7.5 (see
# docs/runbooks/r640_boot.md and docs/evidence/r640/).
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
if ! grep -q 'STATUS=open' "$ROOT/docs/evidence/r640/STATUS"; then
  echo "error: evidence STATUS must remain open until real R640 close" >&2
  exit 1
fi
if ! grep -q 'GAP: Real R640 boot' "$ROOT/mgmt/m7_r640_gate.rs"; then
  echo "error: R640 GAP must stay open (not CLOSED) on scaffold" >&2
  exit 1
fi
if grep -q 'GAP(CLOSED M7.5)' "$ROOT/mgmt/m7_r640_gate.rs"; then
  echo "error: must not claim GAP(CLOSED M7.5) without iron evidence" >&2
  exit 1
fi
if ! grep -q 'r640-hypervisor.efi' "$ROOT/tools/package-release.sh"; then
  echo "error: package-release must still name r640-hypervisor.efi" >&2
  exit 1
fi

echo "==> cargo test m7_5_r640_scaffold_passes (scaffold gate)"
cargo test --lib m7_5_r640_scaffold_passes -- --nocapture

# Honesty: this script must not emit the iron marker on stdout.
if grep -q "^${IRON}$" <<<"$(printf '%s\n' "$SCAFFOLD")" 2>/dev/null; then
  :
fi
# Ensure we only print scaffold marker below — never IRON from host smoke.
echo "$SCAFFOLD"
echo "==> M7.5 R640 scaffold smoke PASSED (iron ${IRON} not claimed)"
echo "SKIP: no R640 iron evidence — fill docs/evidence/r640/ on real PowerEdge"
