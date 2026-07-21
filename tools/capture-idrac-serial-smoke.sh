#!/usr/bin/env bash
# Smoke: iDRAC/COM1 capture helper → RAYNU-V-M7-SERIAL-CAPTURE-OK
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_M7_SERIAL_CAPTURE:-RAYNU-V-M7-SERIAL-CAPTURE-OK}"

for f in tools/capture-idrac-serial.sh docs/runbooks/idrac_logging.md; do
  if [[ ! -f "$ROOT/$f" ]]; then
    echo "error: missing $f" >&2
    exit 1
  fi
done

if ! grep -q 'RAYNU-V-AUDIT' "$ROOT/audit/integrity.rs"; then
  echo "error: UEFI audit→COM1 mirror missing" >&2
  exit 1
fi
if ! grep -q 'capture-idrac-serial.sh' "$ROOT/docs/runbooks/r640_field_guide.md"; then
  echo "error: field guide must reference capture-idrac-serial.sh" >&2
  exit 1
fi
if ! grep -q 'Three channels' "$ROOT/docs/runbooks/idrac_logging.md"; then
  echo "error: idrac_logging.md must explain A/B/C channels" >&2
  exit 1
fi

WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/raynu-serial-cap.XXXXXX")"
cleanup() { rm -rf "$WORKDIR"; }
trap cleanup EXIT

OUT="$WORKDIR/serial.txt"
printf 'RAYNU-V-M0-BOOT-OK\nRAYNU-V-AUDIT: VmStarted guest_id=1\n' \
  | ./tools/capture-idrac-serial.sh tee --out "$OUT" >/dev/null

if ! grep -q 'RAYNU-V-M0-BOOT-OK' "$OUT"; then
  echo "error: tee capture lost M0 marker" >&2
  exit 1
fi
if ! grep -q 'RAYNU-V-AUDIT: VmStarted' "$OUT"; then
  echo "error: tee capture lost audit line" >&2
  exit 1
fi
if ! grep -q 'begin transcript' "$OUT"; then
  echo "error: missing capture header" >&2
  exit 1
fi

echo "$MARKER"
echo "==> iDRAC serial capture smoke PASSED"
