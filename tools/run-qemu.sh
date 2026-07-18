#!/usr/bin/env bash
# Boot r640-hypervisor.efi under QEMU+OVMF with COM1 on a chardev.
# SERIAL_CHARDEV defaults to stdio; CI sets file:/path/to/log.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

TARGET="${TARGET:-x86_64-unknown-uefi}"
PROFILE="${PROFILE:-release}"
EFI="target/${TARGET}/${PROFILE}/r640-hypervisor.efi"
SERIAL_CHARDEV="${SERIAL_CHARDEV:-stdio}"

if [[ ! -f "$EFI" ]]; then
  echo "==> EFI missing; building first"
  "$ROOT/tools/build.sh"
fi

OVMF_BIOS="${OVMF_BIOS:-}"
OVMF_CODE="${OVMF_CODE:-}"
if [[ -z "$OVMF_BIOS" && -z "$OVMF_CODE" ]]; then
  for c in \
    /usr/share/OVMF/OVMF.fd \
    /usr/share/OVMF/OVMF_CODE_4M.fd \
    /usr/share/OVMF/OVMF_CODE.fd \
    /usr/share/edk2/ovmf/OVMF_CODE.fd \
    /usr/share/edk2-ovmf/x64/OVMF_CODE.fd
  do
    if [[ -f "$c" ]]; then
      if [[ "$(basename "$c")" == "OVMF.fd" ]]; then
        OVMF_BIOS="$c"
      else
        OVMF_CODE="$c"
      fi
      break
    fi
  done
fi

if [[ -z "${OVMF_BIOS}" && -z "${OVMF_CODE}" ]]; then
  echo "error: OVMF firmware not found; set OVMF_BIOS or OVMF_CODE" >&2
  exit 1
fi

ESP="${ESP:-$ROOT/esp}"
mkdir -p "$ESP/EFI/BOOT"
cp "$EFI" "$ESP/EFI/BOOT/BOOTX64.EFI"

FW_ARGS=()
if [[ -n "${OVMF_BIOS}" ]]; then
  echo "==> OVMF (bios): $OVMF_BIOS"
  FW_ARGS+=(-bios "$OVMF_BIOS")
else
  echo "==> OVMF (pflash): $OVMF_CODE"
  FW_ARGS+=(-drive if=pflash,format=raw,readonly=on,file="$OVMF_CODE")
fi

echo "==> QEMU boot (COM1 → ${SERIAL_CHARDEV}); guest exits via isa-debug-exit"

exec qemu-system-x86_64 \
  -machine q35,accel=tcg \
  -cpu qemu64 \
  -m 512M \
  -display none \
  -serial "$SERIAL_CHARDEV" \
  -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
  "${FW_ARGS[@]}" \
  -drive format=raw,file=fat:rw:"$ESP" \
  "$@"
