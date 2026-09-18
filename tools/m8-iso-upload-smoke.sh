#!/usr/bin/env bash
# M8.4 host/CI smoke: ISO bytes → datastore blob → RAYNU-V-M8-ISO-UPLOAD-HOST-OK.
# Firmware path is ESP-staged linux.iso. Never prints RAYNU-V-M8-ISO-UPLOAD-OK.
# Iron close is a network ISO PUT on coexist after BOOT-OK. ESP-staged stays valid.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_M8_ISO_UPLOAD_HOST:-RAYNU-V-M8-ISO-UPLOAD-HOST-OK}"

if [[ ! -f "$ROOT/mgmt/iso_upload.rs" ]]; then
  echo "error: missing mgmt/iso_upload.rs" >&2
  exit 1
fi
if ! grep -q 'fn prop_iso_upload_host_package(' "$ROOT/mgmt/iso_upload.rs"; then
  echo "error: missing prop_iso_upload_host_package" >&2
  exit 1
fi
if ! grep -q 'HostReady' "$ROOT/mgmt/iso_upload.rs"; then
  echo "error: UploadMode::HostReady required" >&2
  exit 1
fi
if grep -q 'println!("RAYNU-V-M8-ISO-UPLOAD-OK")' "$ROOT/mgmt/iso_upload.rs" "$ROOT/mgmt/http.rs"; then
  echo "error: host must never println iron ISO-UPLOAD-OK" >&2
  exit 1
fi
if grep -q '/blob' "$ROOT/mgmt/http.rs"; then
  echo "error: firmware HTTP must not grow a coexist blob PUT" >&2
  exit 1
fi

OUT="$(cargo test --lib m8_iso_upload_host_gate_passes -- --nocapture 2>&1)"
echo "$OUT"
if ! echo "$OUT" | grep -q "$MARKER"; then
  echo "error: missing $MARKER" >&2
  exit 1
fi
# HOST-OK is a different string from ISO-UPLOAD-OK.
if echo "$OUT" | grep -F 'RAYNU-V-M8-ISO-UPLOAD-OK' | grep -vF 'RAYNU-V-M8-ISO-UPLOAD-HOST-OK' | grep -q .; then
  echo "error: iron ISO-UPLOAD-OK printed" >&2
  exit 1
fi
echo "$MARKER"
