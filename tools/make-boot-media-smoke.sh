#!/usr/bin/env bash
# Smoke: boot-media maker → RAYNU-V-M7-BOOT-MEDIA-OK
# Uses a tiny fake EFI payload so CI does not need a full UEFI build.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER="${MARKER_M7_BOOT_MEDIA:-RAYNU-V-M7-BOOT-MEDIA-OK}"

for f in tools/make-boot-media.sh tools/make-boot-usb.sh tools/media-maker.sh \
  docs/runbooks/media_maker.md; do
  if [[ ! -f "$ROOT/$f" ]]; then
    echo "error: missing $f" >&2
    exit 1
  fi
done

if ! grep -q 'EFI/BOOT/BOOTX64.EFI' "$ROOT/tools/make-boot-media.sh"; then
  echo "error: make-boot-media must place BOOTX64.EFI" >&2
  exit 1
fi
if ! grep -q 'Virtual Media' "$ROOT/docs/runbooks/media_maker.md"; then
  echo "error: media_maker runbook must cover iDRAC Virtual Media" >&2
  exit 1
fi
if ! grep -q 'make-boot-media.sh' "$ROOT/docs/runbooks/r640_field_guide.md"; then
  echo "error: field guide must point operators at make-boot-media.sh" >&2
  exit 1
fi

need_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "error: missing '$1' ($2)" >&2
    exit 1
  fi
}
need_cmd mkfs.vfat "apt install dosfstools / brew install dosfstools"
need_cmd mmd "apt install mtools / brew install mtools"
need_cmd mcopy "apt install mtools"

WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/raynu-boot-media.XXXXXX")"
cleanup() { rm -rf "$WORKDIR"; }
trap cleanup EXIT

KIT="$WORKDIR/kit"
mkdir -p "$KIT"
# Tiny fake PE-ish payload is enough to prove FAT layout; not a bootable HV.
dd if=/dev/urandom of="$KIT/r640-hypervisor.efi" bs=4096 count=8 status=none
(
  cd "$KIT"
  sha256sum r640-hypervisor.efi | tee r640-hypervisor.efi.sha256
  printf 'version=0.0.0-test\n' >VERSION
)

OUT="$WORKDIR/out"
echo "==> make-boot-media (fixture kit)"
IMG_ONLY=1 ./tools/make-boot-media.sh --kit "$KIT" --out "$OUT"

IMG="$(echo "$OUT"/*-uefi-boot.img)"
if [[ ! -f "$IMG" ]]; then
  echo "error: expected FAT img in $OUT" >&2
  exit 1
fi

export MTOOLS_SKIP_CHECK=1
LISTING="$(mdir -i "$IMG" ::/EFI/BOOT)"
echo "$LISTING"
if ! grep -q 'BOOTX64' <<<"$LISTING"; then
  echo "error: BOOTX64.EFI missing from FAT image" >&2
  exit 1
fi
if [[ ! -f "$OUT/MEDIA.txt" ]]; then
  echo "error: missing MEDIA.txt" >&2
  exit 1
fi
if ! grep -q 'layout=EFI/BOOT/BOOTX64.EFI' "$OUT/MEDIA.txt"; then
  echo "error: MEDIA.txt missing layout line" >&2
  exit 1
fi

echo "$MARKER"
echo "==> M7 boot-media smoke PASSED"
