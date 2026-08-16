#!/usr/bin/env bash
# E5 / M7.7 QEMU lab: ESP isoinstall.txt → 1MiB virtio-blk + BLK-OK + disk written.
# Marker: RAYNU-V-M7-ISO-INSTALL-LAB-OK (not iron RAYNU-V-M7-ISO-INSTALL-OK).
#
# Always requires pre-EBS lab arm note. Full sized-disk + BLK path needs nested VT-x;
# without VMX the script soft-passes after proving the ESP flag armed.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

LAB_ARM="${MARKER_E5_LAB_ARM:-boot: E5 lab isoinstall.txt armed (1MiB)}"
LAB_OK="${MARKER_M7_ISO_INSTALL_LAB_OK:-RAYNU-V-M7-ISO-INSTALL-LAB-OK}"
DISK_WRITTEN="${MARKER_M7_ISO_DISK_WRITTEN:-RAYNU-V-M7-ISO-DISK-WRITTEN}"
REBOOT_PENDING="${MARKER_M7_ISO_REBOOT_PENDING:-RAYNU-V-M7-ISO-REBOOT-PENDING}"
BLK_OK="${MARKER_M4_BLK:-RAYNU-V-M4-BLK-OK}"
IRON="${MARKER_M7_ISO_INSTALL_OK:-RAYNU-V-M7-ISO-INSTALL-OK}"
TIMEOUT_SECS="${TIMEOUT_SECS:-300}"
SERIAL_LOG="${SERIAL_LOG:-$ROOT/target/e5-iso-install-serial.log}"
ESP="${ESP:-$ROOT/target/e5-iso-install-esp}"

kvm_usable() {
  [[ -e /dev/kvm && -r /dev/kvm && -w /dev/kvm ]]
}
host_has_vmx() {
  grep -qw vmx /proc/cpuinfo 2>/dev/null
}

if [[ -z "${REQUIRE_VMX:-}" ]]; then
  if kvm_usable && host_has_vmx && [[ "${QEMU_ACCEL:-auto}" != "tcg" ]]; then
    REQUIRE_VMX=1
  else
    REQUIRE_VMX=0
  fi
fi

mkdir -p "$(dirname "$SERIAL_LOG")" "$ESP/EFI/BOOT"

echo "==> cargo test iso_install_lab_package (host)"
cargo test --lib iso_install_lab_package -- --nocapture

echo "==> Building EFI"
"$ROOT/tools/build.sh"

if [[ ! -f "$ROOT/assets/bzImage" ]]; then
  if [[ -f "$ROOT/assets/bzImage.real" ]]; then
    cp "$ROOT/assets/bzImage.real" "$ROOT/assets/bzImage"
  else
    "$ROOT/tools/gen-minimal-bzimage.sh" "$ROOT/assets/bzImage"
  fi
fi
if [[ ! -f "$ROOT/assets/initrd" ]]; then
  "$ROOT/tools/build-tiny-initrd.sh" "$ROOT/assets/initrd"
fi

echo "==> QEMU ISO install lab (timeout ${TIMEOUT_SECS}s, REQUIRE_VMX=${REQUIRE_VMX})"
rm -f "$SERIAL_LOG"
: >"$SERIAL_LOG"
set +e
ISO_INSTALL_LAB=1 ESP="$ESP" SERIAL_CHARDEV="file:$SERIAL_LOG" \
  timeout --signal=KILL "${TIMEOUT_SECS}" "$ROOT/tools/run-qemu.sh" \
  >"$ROOT/target/e5-iso-install-qemu-stdout.log" 2>"$ROOT/target/e5-iso-install-qemu-stderr.log"
qrc=$?
set -e
echo "==> QEMU exit=$qrc (serial: $SERIAL_LOG)"

if ! grep -qF "$LAB_ARM" "$SERIAL_LOG"; then
  echo "error: missing lab arm note: $LAB_ARM" >&2
  tail -n 80 "$SERIAL_LOG" >&2 || true
  exit 1
fi
echo "==> lab arm OK: $LAB_ARM"

if grep -qF "RAYNU-V-M1-VMXON-OK" "$SERIAL_LOG"; then
  for m in "bytes=1048576" "boot: E5 install-sized virtio-blk armed" "$BLK_OK" "$DISK_WRITTEN" "$LAB_OK" "$REBOOT_PENDING"; do
    if ! grep -qF "$m" "$SERIAL_LOG"; then
      echo "error: missing VMX lab marker: $m" >&2
      tail -n 120 "$SERIAL_LOG" >&2 || true
      exit 1
    fi
    echo "==> found: $m"
  done
  echo "$LAB_OK"
  echo "==> M7.7 ISO install QEMU lab PASSED (iron ${IRON} still open)"
  # never print iron marker from this smoke
  exit 0
fi

if [[ "$REQUIRE_VMX" == "1" ]]; then
  echo "error: REQUIRE_VMX=1 but no RAYNU-V-M1-VMXON-OK in serial" >&2
  exit 1
fi

echo "==> VMXON skipped — lab arm proved; sized-disk/BLK path needs nested VT-x"
echo "==> M7.7 ISO install QEMU lab SOFT-PASS (arm only; full lab when VMX available)"
# never print iron marker
exit 0
