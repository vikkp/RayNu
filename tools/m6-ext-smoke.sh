#!/usr/bin/env bash
# M6.9 host/CI smoke: external audit + spec review → RAYNU-V-M6-EXT-OK.
# Auditor path: frozen ADR-008 pin + ept_model verify + review artifacts.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_M6_EXT:-RAYNU-V-M6-EXT-OK}"
VERUS_HOME="${VERUS_HOME:-$ROOT/target/verus}"
PIN="$ROOT/verus-version.toml"

if [[ ! -f "$ROOT/mgmt/ext.rs" ]]; then
  echo "error: missing mgmt/ext.rs" >&2
  exit 1
fi
if ! grep -q 'fn prop_external_audit_package(' "$ROOT/mgmt/ext.rs"; then
  echo "error: missing prop_external_audit_package" >&2
  exit 1
fi
if ! grep -q 'GAP(CLOSED M6.9): External audit + spec review' "$ROOT/mgmt/ext.rs"; then
  echo "error: EXT GAP must be CLOSED M6.9" >&2
  exit 1
fi
if ! grep -q "$MARKER" "$ROOT/mgmt/ext.rs"; then
  echo "error: ext must embed marker $MARKER" >&2
  exit 1
fi
if [[ ! -f "$ROOT/docs/reviews/m6_spec_review.md" ]]; then
  echo "error: missing docs/reviews/m6_spec_review.md" >&2
  exit 1
fi
if [[ ! -f "$ROOT/docs/findings/m6_external.md" ]]; then
  echo "error: missing docs/findings/m6_external.md" >&2
  exit 1
fi
if ! grep -q 'Open critical findings: \*\*0\*\*' "$ROOT/docs/findings/m6_external.md"; then
  echo "error: findings must report Open critical findings: 0" >&2
  exit 1
fi
if [[ ! -f "$ROOT/docs/reviews/m6_proof_maintenance.md" ]]; then
  echo "error: missing docs/reviews/m6_proof_maintenance.md" >&2
  exit 1
fi
if [[ ! -f "$ROOT/docs/runbooks/external_audit.md" ]]; then
  echo "error: missing docs/runbooks/external_audit.md" >&2
  exit 1
fi
if ! grep -q 'never releases/latest' "$PIN"; then
  echo "error: verus-version.toml must forbid latest" >&2
  exit 1
fi

echo "==> cargo test m6_9_ext_gate_passes (artifact gate)"
cargo test --lib m6_9_ext_gate_passes -- --nocapture

echo "==> cargo test external_audit_package"
cargo test --lib external_audit_package -- --nocapture

echo "==> auditor path: install frozen Verus pin + verify ept_model"
"$ROOT/tools/install-verus.sh"
export PATH="$VERUS_HOME:$PATH"
if [[ ! -x "$VERUS_HOME/verus" || ! -x "$VERUS_HOME/cargo-verus" ]]; then
  echo "error: verus / cargo-verus missing under $VERUS_HOME" >&2
  exit 1
fi
version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$PIN" | head -1)"
vout="$("$VERUS_HOME/verus" --version)"
echo "$vout"
if ! grep -q "$version" <<<"$vout"; then
  echo "error: verus --version did not report pinned $version" >&2
  exit 1
fi
cargo clean -p ept_model >/dev/null 2>&1 || true
out="$(cargo verus verify -p ept_model 2>&1)"
echo "$out"
if ! grep -q '0 errors' <<<"$out"; then
  echo "error: ept_model verification reported errors" >&2
  exit 1
fi
if ! grep -qE '[1-9][0-9]* verified' <<<"$out"; then
  echo "error: ept_model verification produced no positive verified count" >&2
  exit 1
fi

echo "$MARKER"
echo "==> M6.9 external audit smoke PASSED"
