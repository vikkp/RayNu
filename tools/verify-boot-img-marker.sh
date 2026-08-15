#!/usr/bin/env bash
# Prove the Virtual Floppy image embeds *this* tree's EFI (marker build).
# Usage: ./tools/verify-boot-img-marker.sh [path-to-uefi-boot.img]
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

efi_has() {
  python3 -c 'import sys; d=open(sys.argv[1],"rb").read(); sys.exit(0 if sys.argv[2].encode() in d else 1)' "$1" "$2"
}

IMG="${1:-}"
if [[ -z "$IMG" ]]; then
  IMG="$(ls -1t dist/*/raynu-v-*-uefi-boot.img dist/*-uefi-boot.img 2>/dev/null | head -1 || true)"
fi
if [[ -z "$IMG" || ! -f "$IMG" ]]; then
  echo "error: no boot img found — pass path or run make-boot-media.sh first" >&2
  exit 1
fi

EFI_BUILT="target/x86_64-unknown-uefi/release/r640-hypervisor.efi"
if [[ ! -f "$EFI_BUILT" ]]; then
  echo "error: missing $EFI_BUILT — run ./tools/build.sh first" >&2
  exit 1
fi

need_cmd() { command -v "$1" >/dev/null || { echo "error: need $1" >&2; exit 1; }; }
need_cmd mcopy
need_cmd shasum
need_cmd python3

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mcopy -n -i "$IMG" ::/EFI/BOOT/BOOTX64.EFI "$TMP/BOOTX64.EFI"

BUILT_SHA="$(shasum -a 256 "$EFI_BUILT" | awk '{print $1}')"
IMG_SHA="$(shasum -a 256 "$TMP/BOOTX64.EFI" | awk '{print $1}')"
DIST_SHA=""
if [[ -f dist/raynu-v-0.1.0/r640-hypervisor.efi ]]; then
  DIST_SHA="$(shasum -a 256 dist/raynu-v-0.1.0/r640-hypervisor.efi | awk '{print $1}')"
fi

echo "img:        $IMG"
echo "built EFI:  $BUILT_SHA"
echo "img BOOTX64:$IMG_SHA"
[[ -n "$DIST_SHA" ]] && echo "dist EFI:   $DIST_SHA"

if ! efi_has "$TMP/BOOTX64.EFI" 'E2 marker build=r640-boot-ok-marker'; then
  echo "FAIL: img EFI missing build stamp bytes" >&2
  exit 1
fi
if ! efi_has "$TMP/BOOTX64.EFI" 'RAYNU-V-R640-BOOT-OK'; then
  echo "FAIL: img EFI missing RAYNU-V-R640-BOOT-OK" >&2
  exit 1
fi
if [[ "$BUILT_SHA" != "$IMG_SHA" ]]; then
  echo "FAIL: img BOOTX64.EFI SHA != freshly built EFI" >&2
  echo "      Remap iDRAC after: ./tools/make-boot-media.sh --kit dist/raynu-v-0.1.0" >&2
  exit 1
fi
echo "OK: floppy embeds marker-build EFI (SHA match + stamp present)"
