#!/usr/bin/env bash
# Boot r640-hypervisor.efi under QEMU+OVMF (serial on stdio).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

TARGET="${TARGET:-x86_64-unknown-uefi}"
PROFILE="${PROFILE:-release}"
EFI="target/${TARGET}/${PROFILE}/r640-hypervisor.efi"

if [[ ! -f "$EFI" ]]; then
  echo "==> EFI missing; building first"
  "$ROOT/tools/build.sh"
fi

OVMF_CODE="${OVMF_CODE:-}"
if [[ -z "$OVMF_CODE" ]]; then
  for c in \
    /usr/share/OVMF/OVMF_CODE.fd \
    /usr/share/OVMF/OVMF_CODE_4M.fd \
    /usr/share/edk2/ovmf/OVMF_CODE.fd \
    /usr/share/edk2-ovmf/x64/OVMF_CODE.fd
  do
    if [[ -f "$c" ]]; then OVMF_CODE="$c"; break; fi
  done
fi

if [[ -z "${OVMF_CODE}" || ! -f "$OVMF_CODE" ]]; then
  echo "error: OVMF firmware not found; set OVMF_CODE=/path/to/OVMF_CODE.fd" >&2
  exit 1
fi

mkdir -p esp/EFI/BOOT
cp "$EFI" esp/EFI/BOOT/BOOTX64.EFI

echo "==> QEMU boot (serial stdio); Ctrl-A X to exit"
exec qemu-system-x86_64 \
  -bios "$OVMF_CODE" \
  -drive format=raw,file=fat:rw:esp \
  -serial stdio \
  -display none \
  -m 512M \
  -cpu qemu64 \
  "$@"
