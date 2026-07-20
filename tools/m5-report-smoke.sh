#!/usr/bin/env bash
# M5.4 host/CI smoke: SOX/ISO audit reports → RAYNU-V-M5-REPORT-OK.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_M5_REPORT:-RAYNU-V-M5-REPORT-OK}"

if [[ ! -f "$ROOT/assets/schemas/sox_access_control.json" ]]; then
  echo "error: missing SOX schema" >&2
  exit 1
fi
if [[ ! -f "$ROOT/assets/schemas/iso_event_inventory.json" ]]; then
  echo "error: missing ISO schema" >&2
  exit 1
fi
if [[ ! -f "$ROOT/audit/report.rs" ]]; then
  echo "error: missing audit/report.rs" >&2
  exit 1
fi
if ! grep -q 'fn render_report(' "$ROOT/audit/report.rs"; then
  echo "error: missing render_report" >&2
  exit 1
fi
if ! grep -q 'fn prop_reports_deterministic(' "$ROOT/audit/report.rs"; then
  echo "error: missing prop_reports_deterministic" >&2
  exit 1
fi
if ! grep -q 'link_section = ".aschema"' "$ROOT/audit/report.rs"; then
  echo "error: missing PE section .aschema" >&2
  exit 1
fi
if ! grep -q "$MARKER" "$ROOT/audit/report.rs"; then
  echo "error: report must embed marker $MARKER" >&2
  exit 1
fi
if ! grep -qE 'GAP(\(CLOSED M6\.5\))?: PDF report' "$ROOT/audit/report.rs"; then
  echo "error: PDF GAP note missing (open or CLOSED M6.5)" >&2
  exit 1
fi

echo "==> cargo test m5_4_report_gate_passes (artifact gate)"
out="$(cargo test --lib m5_4_report_gate_passes -- --nocapture 2>&1)"
echo "$out"
echo "$out" | grep -q 'm5_4_report_gate_passes ... ok'
echo "$out" | grep -q "$MARKER"

echo "==> cargo test reports_deterministic"
out2="$(cargo test --lib reports_deterministic -- --nocapture 2>&1)"
echo "$out2"
echo "$out2" | grep -q 'reports_deterministic ... ok'

echo "$MARKER"
echo "==> M5.4 report smoke PASSED"
