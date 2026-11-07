#!/usr/bin/env bash
# ADR-021 F10. The NIC Selection matrix exists. Shared LOM stays open
# until an iron note says it was tested. Dedicated is the lived install path.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DOC="${ROOT}/docs/customer/idrac-nic.md"
fail=0

if [[ ! -f "$DOC" ]]; then
  echo "error: missing docs/customer/idrac-nic.md" >&2
  exit 1
fi

need() {
  local pat="$1"
  if ! grep -q -F -- "$pat" "$DOC"; then
    echo "error: docs/customer/idrac-nic.md missing: $pat" >&2
    fail=1
  fi
}

need "Dedicated"
need "Shared LOM"
need "10.99.99.201"
need "4e327820e039"
need "Not tested."
need "ape-nophylock=yes"
need "Guest 3"

forbid() {
  local pat="$1"
  if grep -q -F -- "$pat" "$DOC"; then
    echo "error: docs/customer/idrac-nic.md must not say: $pat" >&2
    fail=1
  fi
}

forbid "Shared LOM is tested"
forbid "Shared LOM lived"

if [[ "$fail" -ne 0 ]]; then
  exit 1
fi
echo "F10 NIC Selection matrix OK"
