#!/usr/bin/env bash
# M5.5 host/CI smoke: VMware import → RAYNU-V-M5-MIGRATE-OK.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_M5_MIGRATE:-RAYNU-V-M5-MIGRATE-OK}"

if [[ ! -f "$ROOT/assets/migrate/sample_inventory.txt" ]]; then
  echo "error: missing sample inventory" >&2
  exit 1
fi
if ! grep -q 'vmdk' "$ROOT/assets/migrate/sample_inventory.txt"; then
  echo "error: inventory missing vmdk" >&2
  exit 1
fi
if ! grep -q 'ovf' "$ROOT/assets/migrate/sample_inventory.txt"; then
  echo "error: inventory missing ovf" >&2
  exit 1
fi
lines=$(grep -vE '^\s*(#|$)' "$ROOT/assets/migrate/sample_inventory.txt" | wc -l | tr -d ' ')
if (( lines < 10 )); then
  echo "error: inventory needs ≥10 guests (got $lines)" >&2
  exit 1
fi
if [[ ! -f "$ROOT/migrate/mod.rs" ]]; then
  echo "error: missing migrate/mod.rs" >&2
  exit 1
fi
if ! grep -q 'fn migrate_one_command(' "$ROOT/migrate/mod.rs"; then
  echo "error: missing migrate_one_command" >&2
  exit 1
fi
if ! grep -q "$MARKER" "$ROOT/migrate/mod.rs"; then
  echo "error: migrate must embed marker $MARKER" >&2
  exit 1
fi
if ! grep -q 'MigrateStarted' "$ROOT/audit/integrity.rs"; then
  echo "error: audit missing MigrateStarted" >&2
  exit 1
fi
if ! grep -q 'MigrateCompleted' "$ROOT/audit/integrity.rs"; then
  echo "error: audit missing MigrateCompleted" >&2
  exit 1
fi
if ! grep -q 'MigrateFailed' "$ROOT/audit/integrity.rs"; then
  echo "error: audit missing MigrateFailed" >&2
  exit 1
fi

echo "==> cargo test m5_5_migrate_gate_passes (artifact gate)"
out="$(cargo test --lib m5_5_migrate_gate_passes -- --nocapture 2>&1)"
echo "$out"
echo "$out" | grep -q 'm5_5_migrate_gate_passes ... ok'
echo "$out" | grep -q "$MARKER"

echo "==> cargo test migrate_ten_plus_one_command"
out2="$(cargo test --lib migrate_ten_plus_one_command -- --nocapture 2>&1)"
echo "$out2"
echo "$out2" | grep -q 'migrate_ten_plus_one_command ... ok'

echo "$MARKER"
echo "==> M5.5 migrate smoke PASSED"
