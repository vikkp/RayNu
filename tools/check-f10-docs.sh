#!/usr/bin/env bash
# ADR-021 F10. Both NIC Selection rows are recorded from the 2026-11-07 iron pass.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DOC="${ROOT}/docs/customer/idrac-nic.md"
EV="${ROOT}/docs/evidence/r640/2026-11-07-4e327820e039-wave-6-f10-idrac-nic-ok.md"
fail=0

if [[ ! -f "$DOC" ]]; then
  echo "error: missing docs/customer/idrac-nic.md" >&2
  exit 1
fi
if [[ ! -f "$EV" ]]; then
  echo "error: missing F10 evidence" >&2
  exit 1
fi

need() {
  local f="$1"
  local pat="$2"
  if ! grep -q -F -- "$pat" "$f"; then
    echo "error: $f missing: $pat" >&2
    fail=1
  fi
}

need "$DOC" "Dedicated"
need "$DOC" "LOM3"
need "$DOC" "NIC.Integrated.1-3-1"
need "$DOC" "10.99.99.24"
need "$DOC" "10.99.99.201"
need "$DOC" "4e327820e039"
need "$DOC" "Active NIC Interface"
need "$DOC" "100 Mbps"
need "$DOC" "Guest 3"
need "$DOC" "ape-nophylock=yes"
need "$EV" "F10 lived"

if grep -q -F -- "Not tested." "$DOC"; then
  echo "error: docs/customer/idrac-nic.md still says Not tested." >&2
  fail=1
fi

if [[ "$fail" -ne 0 ]]; then
  exit 1
fi
echo "F10 NIC Selection matrix OK"
