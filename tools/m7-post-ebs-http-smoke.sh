#!/usr/bin/env bash
# Post-EBS SNP HTTP listen **scaffold** → RAYNU-V-M7-POST-EBS-HTTP-SCAFFOLD-OK.
#
# Proves parked SNP + post-EBS probe/idle wiring (ADR-012 residual).
# Does **never print iron/firmware marker** RAYNU-V-M7-POST-EBS-HTTP-OK from this path.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

SCAFFOLD="${MARKER_M7_POST_EBS_HTTP_SCAFFOLD:-RAYNU-V-M7-POST-EBS-HTTP-SCAFFOLD-OK}"
IRON="${MARKER_M7_POST_EBS_HTTP:-RAYNU-V-M7-POST-EBS-HTTP-OK}"

if [[ ! -f "$ROOT/mgmt/m7_post_ebs_http_gate.rs" ]]; then
  echo "error: missing mgmt/m7_post_ebs_http_gate.rs" >&2
  exit 1
fi
if ! grep -q 'run_post_ebs_mgmt_listen' "$ROOT/src/main.rs"; then
  echo "error: main.rs must probe post-EBS SNP after ExitBootServices" >&2
  exit 1
fi
if ! grep -q 'run_pre_ebs_mgmt_listen' "$ROOT/src/main.rs"; then
  echo "error: PRE-EBS fallback must remain" >&2
  exit 1
fi
if ! grep -q 'park_snp_http' "$ROOT/mgmt/snp_listen_uefi.rs"; then
  echo "error: SNP session must be parked before EBS" >&2
  exit 1
fi
if ! grep -q 'do not chase' "$ROOT/docs/runbooks/mgmt_http.md"; then
  echo "error: runbook must say do not chase firmware Tcp4" >&2
  exit 1
fi

echo "==> cargo test m7_post_ebs_http_scaffold_passes"
cargo test --lib m7_post_ebs_http_scaffold_passes -- --nocapture

echo "$SCAFFOLD"
echo "==> post-EBS HTTP scaffold smoke PASSED (firmware ${IRON} never claimed from host; SNP after EBS is dead on iron)"
echo "never print iron marker from host smoke"
