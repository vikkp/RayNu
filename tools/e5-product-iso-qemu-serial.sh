#!/usr/bin/env bash
# Nested QEMU serial capture of Stage 46 product ISO (alpine-standard).
# alpine-virt on-media repo lacks grub-efi + dosfstools needed for
# USE_EFI=1 BOOTLOADER=grub. Nested proof uses alpine-standard (245 MiB;
# grub.cfg "Linux lts" / vmlinuz-lts, Data Length 140 -> 299 after the
# ISO_GRUB_LINUX_LTS grow). Cruzer ESP still stages alpine-virt (63 MiB):
# the stick is 977.5 MiB but --refat-cruzer made a 64 MiB FAT; refat
# larger before staging alpine-standard on iron.
#
# Not ISO-INSTALL-OK. Nested product-ISO HOLDS and seeds leftover DRAM
# above PRECISE (run-qemu.sh defaults QEMU_MEM=2560M). iso=0 stays 512M
# and does not seed. Host/CI must never print RAYNU-V-M7-ISO-INSTALL-OK.
# Iron close stays Cruzer flash of cursor/e5-stage46-iso-a623.
#
# GHA ubuntu-latest is mixed Intel/AMD. AMD cannot expose VMX (RayNu-V
# is VT-x). VMXON-SKIP + leftover extra hpa= is a skip, not a red X.
# Intel nested still requires VMLAUNCH so the Linux walk is captured.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

TIMEOUT_SECS="${TIMEOUT_SECS:-480}"
SERIAL_LOG="${SERIAL_LOG:-$ROOT/target/e5-iso-serial.log}"
ESP="${ESP:-$ROOT/target/e5-iso-esp}"
ISO_URL="${ALPINE_ISO_URL:-https://dl-cdn.alpinelinux.org/alpine/v3.21/releases/x86_64/alpine-standard-3.21.3-x86_64.iso}"
ISO_PATH="${PRODUCT_ISO:-$ROOT/target/alpine-standard-3.21.3-x86_64.iso}"

mkdir -p "$(dirname "$SERIAL_LOG")" "$ESP" "$(dirname "$ISO_PATH")"

if [[ ! -f "$ISO_PATH" ]]; then
  echo "==> fetching alpine-standard ISO to $ISO_PATH"
  curl -fsSL -o "$ISO_PATH" "$ISO_URL"
fi
psz=$(wc -c <"$ISO_PATH" | tr -d ' ')
if (( psz <= 73728 )); then
  echo "error: PRODUCT_ISO is lab-stub sized ($psz)" >&2
  exit 1
fi

if [[ -e /dev/kvm ]]; then
  sudo chmod a+rw /dev/kvm || true
fi
if [[ -x "$ROOT/tools/enable-nested-kvm.sh" ]]; then
  sudo "$ROOT/tools/enable-nested-kvm.sh" || true
fi
cpu_virt=$(grep -m1 '^flags' /proc/cpuinfo 2>/dev/null | grep -oE 'vmx|svm' | tr '\n' ' ' || true)
echo "==> host virt flags: ${cpu_virt:-none}"

rm -f "$SERIAL_LOG"
: >"$SERIAL_LOG"

# ADR-016: the product-ISO Linux walk runs on RayNu-F. Without raynuf.txt
# run-qemu.sh boots the parked retained-OVMF leg, which stops at
# reason=0x30 rip=0xfffdxxxx (n~1043) before any kernel runs, and the
# grub.cfg patch is never exercised. RAYNU_F=0 only for that OVMF leg.
RAYNU_F="${RAYNU_F:-1}"
if [[ "$RAYNU_F" != "1" ]]; then
  echo "==> RAYNU_F=$RAYNU_F: retained-OVMF leg only; Linux walk will NOT run"
fi
# run-qemu.sh only builds when the EFI is missing; a stale target/ EFI
# would test the previous commit's patcher.
if [[ "${REBUILD_EFI:-1}" == "1" ]]; then
  echo "==> building EFI (REBUILD_EFI=0 to skip)"
  cargo build --release --features uefi-bin --target x86_64-unknown-uefi \
    >"$ROOT/target/e5-iso-build.log" 2>&1 || {
    echo "error: EFI build failed; see target/e5-iso-build.log" >&2
    tail -n 30 "$ROOT/target/e5-iso-build.log" >&2 || true
    exit 1
  }
fi

echo "==> product ISO nested serial timeout=${TIMEOUT_SECS}s iso=$psz bytes RAYNU_F=$RAYNU_F (not ISO-INSTALL-OK)"
set +e
timeout --signal=KILL "$TIMEOUT_SECS" \
  env PRODUCT_ISO="$ISO_PATH" ESP="$ESP" SERIAL_CHARDEV="file:$SERIAL_LOG" \
  QEMU_ACCEL="${QEMU_ACCEL:-kvm}" RAYNU_F="$RAYNU_F" \
  "$ROOT/tools/run-qemu.sh" \
  >"$ROOT/target/e5-iso-qemu-stdout.log" 2>"$ROOT/target/e5-iso-qemu-stderr.log"
QEMU_STATUS=$?
set -e

echo "==> QEMU exit status: $QEMU_STATUS (124/137 = timeout; expected)"
if [[ ! -s "$SERIAL_LOG" ]]; then
  echo "error: serial log empty" >&2
  cat "$ROOT/target/e5-iso-qemu-stderr.log" || true
  exit 1
fi

echo "==> marker scan (not ISO-INSTALL-OK):"
grep -E -n 'VMLAUNCH-OK|OVMF-ELTORITO-OK|RN-ELT|Loaded initrd|linux deliver|linux cpuid|linux skip-|invlpg miss|Linux version|Kernel command line|Freeing initrd|Welcome to Alpine|setup-disk|invalid opcode|Oops:|ISO-INSTALL-OK|report-RAM extra|stop n=|#PF linux|preempt noskip|RAYNU-F' \
  "$SERIAL_LOG" | head -n 80 || true

if grep -qF 'RAYNU-V-M7-ISO-INSTALL-OK' "$SERIAL_LOG"; then
  echo "error: nested/host printed iron ISO-INSTALL-OK" >&2
  exit 1
fi
if grep -qF 'RAYNU-V-M7-E5-OVMF-VMLAUNCH-OK' "$SERIAL_LOG"; then
  echo "==> VMLAUNCH seen; nested serial captured (not ISO-INSTALL-OK)"
  exit 0
fi
# AMD GHA / no nested VT-x: leftover seed still runs (extra
# hpa=0x20000000 bytes=1947164672) then VMXON-SKIP. Not a Linux walk.
if grep -qF 'RAYNU-V-M1-VMXON-SKIP' "$SERIAL_LOG" \
  && grep -qF 'report-RAM extra hpa=' "$SERIAL_LOG"; then
  echo "==> host CPUID.VMX clear; leftover DRAM seeded; no VMLAUNCH (not ISO-INSTALL-OK)"
  exit 0
fi
echo "error: no guest-UEFI VMLAUNCH in serial" >&2
exit 1
