#!/usr/bin/env bash
# ADR-021 H5. The four Shipment 1 customer documents exist and say the
# lived facts. Lab runbooks under docs/runbooks/ are not these files.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DOC="${ROOT}/docs/customer"
fail=0

need() {
  local f="$1"
  local pat="$2"
  if [[ ! -f "$DOC/$f" ]]; then
    echo "error: missing docs/customer/$f" >&2
    fail=1
    return
  fi
  if ! grep -q -F -- "$pat" "$DOC/$f"; then
    echo "error: docs/customer/$f missing: $pat" >&2
    fail=1
  fi
}

need README.md "Install"
need README.md "Cabling"
need README.md "Operator runbook"
need README.md "Support matrix"
need install.md "BOOTX64.EFI"
need install.md "sbsign"
need install.md "8443"
need install.md "4e327820e039"
need cabling.md "Dedicated"
need cabling.md "LOM port 3"
need cabling.md "LOM port 4"
need cabling.md "10GbE"
need operator.md "durable yes"
need operator.md "saved"
need operator.md "Keep this slot"
need operator.md "Guest 3"
need support-matrix.md "PowerEdge R640"
need support-matrix.md "H740P"
need support-matrix.md "BCM5720"
need support-matrix.md "Not in this shipment"

# Later rows. These documents do not close them.
forbid() {
  local f="$1"
  local pat="$2"
  if [[ -f "$DOC/$f" ]] && grep -q -F -- "$pat" "$DOC/$f"; then
    echo "error: docs/customer/$f must not say: $pat" >&2
    fail=1
  fi
}

forbid support-matrix.md "R650 is supported"
forbid support-matrix.md "Shared LOM is supported"
forbid cabling.md "Shared LOM is the install path"

if [[ "$fail" -ne 0 ]]; then
  exit 1
fi
echo "H5 customer documents OK"
