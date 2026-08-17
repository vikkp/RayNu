#!/usr/bin/env bash
# M7.8 host/CI smoke: host-owned NIC **scaffold** → RAYNU-V-M7-HOST-NIC-SCAFFOLD-OK.
#
# Proves ADR-013 Phase C wiring (e1000 Device, bounded poll, post-EBS entry).
# Does **never print iron/firmware markers** RAYNU-V-M7-HOST-NIC-HTTP-OK or
# RAYNU-V-M7-HOST-NIC-QEMU-OK from this path.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

SCAFFOLD="${MARKER_M7_HOST_NIC_SCAFFOLD:-RAYNU-V-M7-HOST-NIC-SCAFFOLD-OK}"
IRON="${MARKER_M7_HOST_NIC_HTTP:-RAYNU-V-M7-HOST-NIC-HTTP-OK}"

if [[ ! -f "$ROOT/mgmt/m7_host_nic_gate.rs" ]]; then
  echo "error: missing mgmt/m7_host_nic_gate.rs" >&2
  exit 1
fi
if [[ ! -f "$ROOT/mgmt/e1000_mmio.rs" ]]; then
  echo "error: missing mgmt/e1000_mmio.rs (single NIC unsafe module)" >&2
  exit 1
fi
if ! grep -q 'run_post_ebs_host_nic_listen' "$ROOT/mgmt/http_listen.rs"; then
  echo "error: post-EBS must call native HOST-NIC listen" >&2
  exit 1
fi
if ! grep -q 'probe_host_nic_lab_flag' "$ROOT/src/main.rs"; then
  echo "error: main.rs must probe hostnic.txt before EBS" >&2
  exit 1
fi
if grep -q 'CURL NOW (post-EBS)' "$ROOT/mgmt/host_nic_listen.rs"; then
  echo "error: native listen must not print CURL NOW (post-EBS)" >&2
  exit 1
fi

echo "==> cargo test m7_8_host_nic_scaffold_passes"
cargo test --lib m7_8_host_nic_scaffold_passes -- --nocapture

echo "$SCAFFOLD"
echo "==> M7.8 HOST-NIC scaffold smoke PASSED (firmware ${IRON} is iron Phase D only)"
echo "never print iron marker from host smoke"
