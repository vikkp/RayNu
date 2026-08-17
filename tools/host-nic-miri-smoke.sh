#!/usr/bin/env bash
# Optional Miri smoke for the host-owned NIC parse path (ADR-013 Phase D).
#
# Miri is not a CI hard gate (toolchain may be absent). Host unit tests on
# parse_mocked_rx_desc_bytes remain the required parse-path check.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if ! command -v cargo >/dev/null 2>&1; then
  echo "skip: cargo not found"
  exit 0
fi

if ! rustup component list --installed 2>/dev/null | grep -q '^miri'; then
  echo "skip: rustup component miri is not installed"
  exit 0
fi

echo "==> cargo miri test parse_mocked_rx_desc (host parse path)"
cargo miri test --lib parse_mocked_rx_desc -- --nocapture
echo "ok: miri parse_mocked_rx_desc"
