#!/usr/bin/env bash
# M7.6 host/CI smoke: UEFI HTTP listen **scaffold** → RAYNU-V-M7-UEFI-HTTP-SCAFFOLD-OK.
#
# Proves ADR-012 wiring (PRE-EBS entry, Tcp4 bindings, runbook).
# Does **never print iron/firmware marker** RAYNU-V-M7-UEFI-HTTP-OK from this path.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

SCAFFOLD="${MARKER_M7_UEFI_HTTP_SCAFFOLD:-RAYNU-V-M7-UEFI-HTTP-SCAFFOLD-OK}"
IRON="${MARKER_M7_UEFI_HTTP:-RAYNU-V-M7-UEFI-HTTP-OK}"

if [[ ! -f "$ROOT/mgmt/m7_uefi_http_gate.rs" ]]; then
  echo "error: missing mgmt/m7_uefi_http_gate.rs" >&2
  exit 1
fi
if [[ ! -f "$ROOT/mgmt/tcp4_uefi.rs" ]]; then
  echo "error: missing mgmt/tcp4_uefi.rs" >&2
  exit 1
fi
if ! grep -q 'run_pre_ebs_mgmt_listen' "$ROOT/src/main.rs"; then
  echo "error: main.rs must call run_pre_ebs_mgmt_listen before EBS" >&2
  exit 1
fi
if ! grep -q 'GAP(CLOSED M7.6)' "$ROOT/mgmt/http_listen.rs"; then
  echo "error: M7.6 GAP must be CLOSED in http_listen.rs" >&2
  exit 1
fi
if ! grep -q 'ADR-012' "$ROOT/docs/runbooks/mgmt_http.md"; then
  echo "error: runbook must cite ADR-012" >&2
  exit 1
fi
if ! grep -q 'PRE-EBS' "$ROOT/docs/runbooks/mgmt_http.md"; then
  echo "error: runbook must document PRE-EBS Tcp4 constraint" >&2
  exit 1
fi

echo "==> cargo test m7_6_uefi_http_scaffold_passes"
cargo test --lib m7_6_uefi_http_scaffold_passes -- --nocapture

echo "$SCAFFOLD"
echo "==> M7.6 UEFI HTTP scaffold smoke PASSED (firmware ${IRON} only from PRE-EBS Tcp4 serve)"
echo "never print iron marker from host smoke"
