#!/usr/bin/env bash
# ADR-021 D11. Port roles are written down: iDRAC Dedicated or Shared LOM,
# port 3 management, port 4 guests. Fixed for Shipment 1.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DOC="${ROOT}/docs/customer/port-roles.md"
fail=0

if [[ ! -f "$DOC" ]]; then
  echo "error: missing docs/customer/port-roles.md" >&2
  exit 1
fi

need() {
  local pat="$1"
  if ! grep -q -F -- "$pat" "$DOC"; then
    echo "error: docs/customer/port-roles.md missing: $pat" >&2
    fail=1
  fi
}

need "port 3"
need "port 4"
need "Dedicated"
need "LOM3"
need "01:00.0"
need "01:00.1"
need "b0:26:28:5c:5a:38"
need "b0:26:28:5c:5a:39"
need "10.99.99.201"
need "10.99.99.24"
need "4e327820e039"
need "Guest 3"
need "fixed"

if grep -q -F -- "the operator picks the management jack" "$DOC"; then
  echo "error: D11 must keep the Shipment 1 roles fixed" >&2
  fail=1
fi

if [[ "$fail" -ne 0 ]]; then
  exit 1
fi
echo "D11 port roles OK"
