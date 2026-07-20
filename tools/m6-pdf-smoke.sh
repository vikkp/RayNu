#!/usr/bin/env bash
# M6.5 host/CI smoke: PDF audit reports → RAYNU-V-M6-PDF-OK.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_M6_PDF:-RAYNU-V-M6-PDF-OK}"

if [[ ! -f "$ROOT/audit/report.rs" ]]; then
  echo "error: missing audit/report.rs" >&2
  exit 1
fi
if ! grep -q 'fn render_pdf(' "$ROOT/audit/report.rs"; then
  echo "error: missing render_pdf" >&2
  exit 1
fi
if ! grep -q 'fn prop_pdf_reports_deterministic(' "$ROOT/audit/report.rs"; then
  echo "error: missing prop_pdf_reports_deterministic" >&2
  exit 1
fi
if ! grep -q '%PDF-1.4' "$ROOT/audit/report.rs"; then
  echo "error: missing PDF 1.4 header in renderer" >&2
  exit 1
fi
if ! grep -q 'GAP(CLOSED M6.5): PDF report' "$ROOT/audit/report.rs"; then
  echo "error: PDF GAP must be CLOSED M6.5" >&2
  exit 1
fi
if ! grep -q "$MARKER" "$ROOT/audit/report.rs"; then
  echo "error: report must embed marker $MARKER" >&2
  exit 1
fi
if ! grep -q '"pdf"' "$ROOT/assets/schemas/sox_access_control.json"; then
  echo "error: SOX schema must advertise pdf format" >&2
  exit 1
fi
if ! grep -q '"pdf"' "$ROOT/assets/schemas/iso_event_inventory.json"; then
  echo "error: ISO schema must advertise pdf format" >&2
  exit 1
fi

echo "==> cargo test m6_5_pdf_gate_passes (artifact gate)"
cargo test --lib m6_5_pdf_gate_passes -- --nocapture

echo "==> cargo test pdf_reports_deterministic"
cargo test --lib pdf_reports_deterministic -- --nocapture

echo "$MARKER"
echo "==> M6.5 PDF smoke PASSED"
