#!/usr/bin/env bash
# M8.7 host/CI smoke: frame pack + fwstate gate → RAYNU-V-M8-PERC-HOST-OK.
# Never prints RAYNU-V-M8-PERC-LUN-OK. No doorbell. durable_lun still skips PERC.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_M8_PERC_HOST:-RAYNU-V-M8-PERC-HOST-OK}"

if [[ ! -f "$ROOT/mgmt/megaraid.rs" ]]; then
  echo "error: missing mgmt/megaraid.rs" >&2
  exit 1
fi
if ! grep -q 'fn pick_spare(' "$ROOT/mgmt/megaraid.rs"; then
  echo "error: missing pick_spare" >&2
  exit 1
fi
if ! grep -q 'fn adapter_reset_is_allowed(' "$ROOT/mgmt/megaraid.rs"; then
  echo "error: reset policy required" >&2
  exit 1
fi
if ! grep -q 'fn perc_fwstate_probe(' "$ROOT/mgmt/megaraid.rs"; then
  echo "error: missing perc_fwstate_probe" >&2
  exit 1
fi
if ! grep -q 'fn h740p_mini_subsys(' "$ROOT/mgmt/megaraid.rs"; then
  echo "error: missing H740P Mini subsystem pick" >&2
  exit 1
fi
if ! grep -q 'fn harpoon_fw_status_offset(' "$ROOT/mgmt/megaraid.rs"; then
  echo "error: missing Harpoon status offset" >&2
  exit 1
fi
if ! grep -q 'fn fusion_post_is_allowed(status:' "$ROOT/mgmt/megaraid.rs"; then
  echo "error: fusion post must stay an explicit policy" >&2
  exit 1
fi
if ! grep -q 'fn pack_mfa_descriptor(' "$ROOT/mgmt/megaraid.rs"; then
  echo "error: missing MFA descriptor pack" >&2
  exit 1
fi
if ! grep -q 'fn pci_cmd_for_fusion_post(' "$ROOT/mgmt/megaraid.rs"; then
  echo "error: missing fusion bus-master policy" >&2
  exit 1
fi
if ! grep -q 'boot: perc fusion' "$ROOT/mgmt/megaraid.rs"; then
  echo "error: missing fusion status line" >&2
  exit 1
fi
if ! grep -q 'boot: perc ioc' "$ROOT/mgmt/megaraid.rs"; then
  echo "error: missing IOC post line" >&2
  exit 1
fi
if ! grep -q 'fn pack_ld_read16_polled(' "$ROOT/mgmt/megaraid.rs"; then
  echo "error: missing polled spare READ" >&2
  exit 1
fi
if ! grep -q 'boot: perc read' "$ROOT/mgmt/megaraid.rs"; then
  echo "error: missing spare READ line" >&2
  exit 1
fi
if ! grep -q 'boot: perc read2' "$ROOT/mgmt/megaraid.rs"; then
  echo "error: missing spare last-LBA READ line" >&2
  exit 1
fi
if ! grep -q 'boot: perc virtio ro' "$ROOT/mgmt/megaraid.rs"; then
  echo "error: missing read-only spare virtio line" >&2
  exit 1
fi
if ! grep -q 'write_line_nowait(M8_PERC_LUN_OK_MARKER)' "$ROOT/mgmt/megaraid.rs"; then
  echo "error: guest marker must use write_line_nowait (earlycon hushes write_line)" >&2
  exit 1
fi
if ! grep -q 'fn pack_ld_write16_last_polled(' "$ROOT/mgmt/megaraid.rs"; then
  echo "error: missing polled spare last-LBA WRITE" >&2
  exit 1
fi
if ! grep -q 'boot: perc write' "$ROOT/mgmt/megaraid.rs"; then
  echo "error: missing spare WRITE line" >&2
  exit 1
fi
if ! grep -q 'write_line_nowait(M8_PERC_WRITE_OK_MARKER)' "$ROOT/mgmt/megaraid.rs"; then
  echo "error: write marker must use write_line_nowait" >&2
  exit 1
fi
if ! grep -q 'LBA 0 is not written' "$ROOT/mgmt/megaraid.rs"; then
  echo "error: LBA 0 must stay unwritten" >&2
  exit 1
fi
if ! grep -q 'fn pci_cmd_for_fwstate_load(' "$ROOT/mgmt/megaraid.rs"; then
  echo "error: missing memory-space command policy" >&2
  exit 1
fi
if grep -q 'pci_write32(bus, dev, func, 0x00' "$ROOT/mgmt/megaraid.rs"; then
  echo "error: fwstate must not store a doorbell" >&2
  exit 1
fi
if ! grep -q 'perc_fwstate_probe()' "$ROOT/boot/handoff.rs"; then
  echo "error: fwstate probe is not on the post-EBS path" >&2
  exit 1
fi
if ! grep -q 'perc_fusion_post_low' "$ROOT/boot/handoff.rs"; then
  echo "error: IOC post must use the frame pool" >&2
  exit 1
fi
if ! grep -q 'fn fusion_dma_base_ok(' "$ROOT/mgmt/megaraid.rs"; then
  echo "error: missing 32-bit DMA address check" >&2
  exit 1
fi
if grep -q 'PERC_FUSION_DMA' "$ROOT/mgmt/megaraid.rs"; then
  echo "error: DMA must not be a BSS static" >&2
  exit 1
fi
if grep -q 'println!("RAYNU-V-M8-PERC-LUN-OK")' "$ROOT/mgmt/megaraid.rs" "$ROOT/mgmt/durable_lun.rs" "$ROOT/mgmt/m8_perc_gate.rs"; then
  echo "error: host must never println iron PERC-LUN-OK" >&2
  exit 1
fi
if grep -q 'println!("RAYNU-V-M8-PERC-WRITE-OK")' "$ROOT/mgmt/megaraid.rs" "$ROOT/mgmt/durable_lun.rs" "$ROOT/mgmt/m8_perc_gate.rs"; then
  echo "error: host must never println iron PERC-WRITE-OK" >&2
  exit 1
fi

OUT="$(cargo test --lib m8_perc_host_gate_passes -- --nocapture 2>&1)"
echo "$OUT"
if ! echo "$OUT" | grep -q "$MARKER"; then
  echo "error: missing $MARKER" >&2
  exit 1
fi
if echo "$OUT" | grep -F 'RAYNU-V-M8-PERC-LUN-OK' | grep -q .; then
  echo "error: iron PERC-LUN-OK printed" >&2
  exit 1
fi
if echo "$OUT" | grep -F 'RAYNU-V-M8-PERC-WRITE-OK' | grep -q .; then
  echo "error: iron PERC-WRITE-OK printed" >&2
  exit 1
fi
echo "$MARKER"
