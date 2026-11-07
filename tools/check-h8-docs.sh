#!/usr/bin/env bash
# ADR-021 H8. The support policy states a tag, a twelve-month term, and
# the mailbox. It does not promise a reply clock.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DOC="${ROOT}/docs/customer/support-policy.md"
fail=0

if [[ ! -f "$DOC" ]]; then
  echo "error: missing docs/customer/support-policy.md" >&2
  exit 1
fi

need() {
  local pat="$1"
  if ! grep -q -F -- "$pat" "$DOC"; then
    echo "error: docs/customer/support-policy.md missing: $pat" >&2
    fail=1
  fi
}

need "twelve months"
need "v0.1.6-wave6-closed"
need "v0.1.7-wave7-closed"
need "v0.1.0-everest-closed"
need "v0.1.0-m8-8-ubuntu"
need "v0.1.0-m8-9-lan"
need "namaskaar@raynuv.com"
need "BOOTX64.OLD"
need "support matrix"
need "terms were accepted"

if grep -q -F -- "24/7" "$DOC"; then
  echo "error: support policy must not promise 24/7" >&2
  fail=1
fi
if grep -q -F -- "one-hour" "$DOC"; then
  echo "error: support policy must not promise a one-hour reply" >&2
  fail=1
fi

if [[ "$fail" -ne 0 ]]; then
  exit 1
fi
echo "H8 support policy OK"
