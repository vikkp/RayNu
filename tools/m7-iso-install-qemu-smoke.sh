#!/usr/bin/env bash
# E5 / M7.7 QEMU lab two-boot loop:
#   boot1: isoinstall.txt → 1MiB disk write → LAB-OK + REBOOT-PENDING
#   synth: target/e5-lab-install.img (host LBA markers; RAM disk is not persisted)
#   boot2: isoreboot.txt + installdisk.bin → BOOTED-FROM-DISK
#
# Lab markers only — never print iron RAYNU-V-M7-ISO-INSTALL-OK.
# Without nested VT-x: soft-pass after proving ESP arm notes on both boots.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

LAB_ARM="${MARKER_E5_LAB_ARM:-boot: E5 lab isoinstall.txt armed (1MiB)}"
REBOOT_ARM="${MARKER_E5_REBOOT_ARM:-boot: E5 lab isoreboot.txt armed (1MiB persist)}"
LAB_OK="${MARKER_M7_ISO_INSTALL_LAB_OK:-RAYNU-V-M7-ISO-INSTALL-LAB-OK}"
DISK_WRITTEN="${MARKER_M7_ISO_DISK_WRITTEN:-RAYNU-V-M7-ISO-DISK-WRITTEN}"
REBOOT_PENDING="${MARKER_M7_ISO_REBOOT_PENDING:-RAYNU-V-M7-ISO-REBOOT-PENDING}"
BOOTED="${MARKER_M7_ISO_BOOTED_FROM_DISK:-RAYNU-V-M7-ISO-BOOTED-FROM-DISK}"
BLK_OK="${MARKER_M4_BLK:-RAYNU-V-M4-BLK-OK}"
IRON="${MARKER_M7_ISO_INSTALL_OK:-RAYNU-V-M7-ISO-INSTALL-OK}"
TIMEOUT_SECS="${TIMEOUT_SECS:-300}"
SERIAL1="${SERIAL_LOG:-$ROOT/target/e5-iso-install-serial.log}"
SERIAL2="${SERIAL_LOG2:-$ROOT/target/e5-iso-reboot-serial.log}"
ESP1="${ESP:-$ROOT/target/e5-iso-install-esp}"
ESP2="${ESP2:-$ROOT/target/e5-iso-reboot-esp}"
IMG="${INSTALL_DISK_IMG:-$ROOT/target/e5-lab-install.img}"

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

mkdir -p "$(dirname "$SERIAL1")" "$ESP1/EFI/BOOT" "$ESP2/EFI/BOOT"

echo "==> cargo test iso_install_lab_package (host)"
cargo test --lib iso_install_lab_package -- --nocapture

echo "==> cargo test iso_reboot_lab_package (host)"
cargo test --lib iso_reboot_lab_package -- --nocapture

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

# --- boot1: install write path ---
echo "==> QEMU boot1-install (timeout ${TIMEOUT_SECS}s, REQUIRE_VMX=${REQUIRE_VMX})"
rm -f "$SERIAL1"
: >"$SERIAL1"
set +e
ISO_INSTALL_LAB=1 ESP="$ESP1" SERIAL_CHARDEV="file:$SERIAL1" \
  timeout --signal=KILL "${TIMEOUT_SECS}" "$ROOT/tools/run-qemu.sh" \
  >"$ROOT/target/e5-iso-install-qemu-stdout.log" 2>"$ROOT/target/e5-iso-install-qemu-stderr.log"
qrc1=$?
set -e
echo "==> QEMU boot1-install exit=$qrc1 (serial: $SERIAL1)"

if ! grep -qF "$LAB_ARM" "$SERIAL1"; then
  echo "error: missing lab arm note: $LAB_ARM" >&2
  tail -n 80 "$SERIAL1" >&2 || true
  exit 1
fi
echo "==> boot1 lab arm OK: $LAB_ARM"

boot1_full=0
if grep -qF "RAYNU-V-M1-VMXON-OK" "$SERIAL1"; then
  for m in "bytes=1048576" "boot: E5 install-sized virtio-blk armed" "$BLK_OK" "$DISK_WRITTEN" "$LAB_OK" "$REBOOT_PENDING"; do
    if ! grep -qF "$m" "$SERIAL1"; then
      echo "error: missing VMX boot1 marker: $m" >&2
      tail -n 120 "$SERIAL1" >&2 || true
      exit 1
    fi
    echo "==> boot1 found: $m"
  done
  boot1_full=1
elif [[ "$REQUIRE_VMX" == "1" ]]; then
  echo "error: REQUIRE_VMX=1 but no RAYNU-V-M1-VMXON-OK in boot1 serial" >&2
  exit 1
else
  echo "==> boot1 VMXON skipped — arm proved; sized-disk path needs nested VT-x"
fi

# --- synthesize persisted install image (host; RAM disk does not survive) ---
echo "==> synthesize lab install image → $IMG"
"$ROOT/tools/synth-e5-lab-install-img.sh" "$IMG"

# --- boot2: reboot-from-disk detect ---
echo "==> QEMU boot2-reboot (timeout ${TIMEOUT_SECS}s, REQUIRE_VMX=${REQUIRE_VMX})"
rm -f "$SERIAL2"
: >"$SERIAL2"
set +e
ISO_REBOOT_LAB=1 INSTALL_DISK_IMG="$IMG" ESP="$ESP2" SERIAL_CHARDEV="file:$SERIAL2" \
  timeout --signal=KILL "${TIMEOUT_SECS}" "$ROOT/tools/run-qemu.sh" \
  >"$ROOT/target/e5-iso-reboot-qemu-stdout.log" 2>"$ROOT/target/e5-iso-reboot-qemu-stderr.log"
qrc2=$?
set -e
echo "==> QEMU boot2-reboot exit=$qrc2 (serial: $SERIAL2)"

if ! grep -qF "$REBOOT_ARM" "$SERIAL2"; then
  echo "error: missing reboot arm note: $REBOOT_ARM" >&2
  tail -n 80 "$SERIAL2" >&2 || true
  exit 1
fi
echo "==> boot2 reboot arm OK: $REBOOT_ARM"

if grep -qF "RAYNU-V-M1-VMXON-OK" "$SERIAL2"; then
  for m in "bytes=1048576" "boot: E5 install disk preload (reboot detect)" "$BLK_OK" "$BOOTED"; do
    if ! grep -qF "$m" "$SERIAL2"; then
      echo "error: missing VMX boot2 marker: $m" >&2
      tail -n 120 "$SERIAL2" >&2 || true
      exit 1
    fi
    echo "==> boot2 found: $m"
  done
  echo "$BOOTED"
  if [[ "$boot1_full" == "1" ]]; then
    echo "$LAB_OK"
  fi
  echo "==> M7.7 ISO install QEMU lab PASSED (two-boot; iron ${IRON} still open)"
  # never print iron marker from this smoke
  exit 0
fi

if [[ "$REQUIRE_VMX" == "1" ]]; then
  echo "error: REQUIRE_VMX=1 but no RAYNU-V-M1-VMXON-OK in boot2 serial" >&2
  exit 1
fi

echo "==> boot2 VMXON skipped — reboot arm proved; BOOTED-FROM-DISK needs nested VT-x"
echo "==> M7.7 ISO install QEMU lab SOFT-PASS (arm-only both boots; full lab when VMX available)"
# never print iron marker
exit 0
