#!/usr/bin/env bash
# Build iDRAC / USB boot media from a RayNu-V EFI release kit.
#
# Produces (under dist/ by default):
#   raynu-v-<ver>-uefi-boot.img   — FAT32 image with \EFI\BOOT\BOOTX64.EFI
#                                   (map as iDRAC Virtual Media *USB*)
#   raynu-v-<ver>-uefi-boot.iso   — El Torito UEFI ISO wrapping the same FAT
#                                   (map as iDRAC Virtual Media *CD*)
#   *.sha256                      — checksums for evidence
#
# Usage:
#   ./tools/make-boot-media.sh
#   ./tools/make-boot-media.sh --efi path/to/r640-hypervisor.efi
#   ./tools/make-boot-media.sh --kit dist/raynu-v-0.1.0
#   IMG_ONLY=1 ./tools/make-boot-media.sh
#
# Requires: dosfstools (mkfs.vfat), mtools (mmd/mcopy). xorriso optional for ISO.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

IMG_MIB="${IMG_MIB:-64}"
IMG_ONLY="${IMG_ONLY:-0}"
EFI=""
KIT=""
OUT_DIR=""

usage() {
  sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
  exit "${1:-0}"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage 0 ;;
    --efi) EFI="${2:-}"; shift 2 ;;
    --kit) KIT="${2:-}"; shift 2 ;;
    --out) OUT_DIR="${2:-}"; shift 2 ;;
    --img-only) IMG_ONLY=1; shift ;;
    *)
      echo "error: unknown arg: $1" >&2
      usage 1
      ;;
  esac
done

need_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "error: missing command '$1' — install: $2" >&2
    exit 1
  fi
}

need_cmd mkfs.vfat "dosfstools"
need_cmd mmd "mtools"
need_cmd mcopy "mtools"
need_cmd truncate "coreutils"

# Resolve EFI + kit metadata
if [[ -n "$KIT" ]]; then
  if [[ ! -d "$KIT" ]]; then
    echo "error: --kit is not a directory: $KIT" >&2
    exit 1
  fi
  EFI="${EFI:-$KIT/r640-hypervisor.efi}"
elif [[ -z "$EFI" ]]; then
  if [[ -f dist/raynu-v-*/r640-hypervisor.efi ]]; then
    # shellcheck disable=SC2012
    EFI="$(ls -1d dist/raynu-v-*/r640-hypervisor.efi 2>/dev/null | sort | tail -1)"
    KIT="$(dirname "$EFI")"
  elif [[ -f target/x86_64-unknown-uefi/release/r640-hypervisor.efi ]]; then
    EFI="target/x86_64-unknown-uefi/release/r640-hypervisor.efi"
  else
    echo "error: no EFI found — run ./tools/package-release.sh or pass --efi / --kit" >&2
    exit 1
  fi
fi

if [[ ! -f "$EFI" ]]; then
  echo "error: EFI not found: $EFI" >&2
  exit 1
fi

if [[ -z "$KIT" && -f "$(dirname "$EFI")/r640-hypervisor.efi.sha256" ]]; then
  KIT="$(dirname "$EFI")"
fi

# Verify checksum when sidecar exists
if [[ -n "$KIT" && -f "$KIT/r640-hypervisor.efi.sha256" ]]; then
  echo "==> verify EFI checksum"
  (
    cd "$KIT"
    sha256sum -c r640-hypervisor.efi.sha256
  )
elif [[ -f "${EFI}.sha256" ]]; then
  echo "==> verify EFI checksum (sidecar next to EFI)"
  sha256sum -c "${EFI}.sha256"
else
  echo "==> warn: no .sha256 sidecar — computing hash only (not verifying a known-good kit)"
fi

EFI_SHA="$(sha256sum "$EFI" | awk '{print $1}')"
VERSION="unknown"
if [[ -n "$KIT" && -f "$KIT/VERSION" ]]; then
  VERSION="$(sed -n 's/^version=//p' "$KIT/VERSION" | head -1)"
fi
if [[ "$VERSION" == "unknown" || -z "$VERSION" ]]; then
  VERSION="$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
fi
VERSION="${VERSION:-0.0.0}"
STAMP="raynu-v-${VERSION}"

if [[ -z "$OUT_DIR" ]]; then
  OUT_DIR="$ROOT/dist/${STAMP}-boot-media"
fi
mkdir -p "$OUT_DIR"

IMG="$OUT_DIR/${STAMP}-uefi-boot.img"
ISO="$OUT_DIR/${STAMP}-uefi-boot.iso"
STAGE="$OUT_DIR/.stage-fat"
rm -f "$IMG" "$ISO"
rm -rf "$STAGE"
mkdir -p "$STAGE"

echo "==> FAT image (${IMG_MIB} MiB) → $IMG"
truncate -s "${IMG_MIB}M" "$IMG"
# -C creates the file; we already truncated — format in place.
mkfs.vfat -F 32 -n RAYNUV "$IMG" >/dev/null

export MTOOLS_SKIP_CHECK=1
mmd -i "$IMG" ::/EFI
mmd -i "$IMG" ::/EFI/BOOT
mcopy -i "$IMG" "$EFI" ::/EFI/BOOT/BOOTX64.EFI

# Operator convenience copies (not required for UEFI boot)
mcopy -i "$IMG" "$EFI" ::/r640-hypervisor.efi
if [[ -n "$KIT" && -f "$KIT/VERSION" ]]; then
  mcopy -i "$IMG" "$KIT/VERSION" ::/VERSION
fi
if [[ -n "$KIT" && -f "$KIT/r640-hypervisor.efi.sha256" ]]; then
  mcopy -i "$IMG" "$KIT/r640-hypervisor.efi.sha256" ::/r640-hypervisor.efi.sha256
fi

{
  echo "RayNu-V UEFI boot media"
  echo "version=${VERSION}"
  echo "efi_sha256=${EFI_SHA}"
  echo "layout=EFI/BOOT/BOOTX64.EFI"
  echo "img=$(basename "$IMG")"
  echo "built_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "use_idrac=Map .img as Virtual Media USB (preferred) or .iso as CD"
  echo "field_guide=docs/runbooks/r640_field_guide.md"
} >"$OUT_DIR/MEDIA.txt"

(
  cd "$OUT_DIR"
  sha256sum "$(basename "$IMG")" MEDIA.txt | tee "$(basename "$IMG").sha256"
)

echo "==> FAT layout check"
mdir -i "$IMG" ::/EFI/BOOT

if [[ "$IMG_ONLY" != "1" ]]; then
  if command -v xorriso >/dev/null 2>&1; then
    echo "==> UEFI El Torito ISO → $ISO"
    # EFI system partition image is the FAT we just built.
    xorriso -as mkisofs \
      -o "$ISO" \
      -V "RAYNU-V" \
      -e "$(basename "$IMG")" \
      -no-emul-boot \
      -isohybrid-gpt-basdat \
      -append_partition 2 0xef "$IMG" \
      "$OUT_DIR" >/dev/null 2>&1 || {
        # Fallback: simpler graft without hybrid extras (still UEFI-bootable on many BMC maps)
        echo "==> xorriso hybrid path failed; trying simple EFI El Torito"
        SIMPLE_FAT="$OUT_DIR/efiboot.img"
        cp "$IMG" "$SIMPLE_FAT"
        mkdir -p "$STAGE/EFI/BOOT"
        mcopy -i "$IMG" ::/EFI/BOOT/BOOTX64.EFI "$STAGE/EFI/BOOT/BOOTX64.EFI"
        xorriso -as mkisofs \
          -o "$ISO" \
          -V "RAYNU-V" \
          -e efiboot.img \
          -no-emul-boot \
          -graft-points \
          "EFI/BOOT/BOOTX64.EFI=$STAGE/EFI/BOOT/BOOTX64.EFI" \
          "efiboot.img=$SIMPLE_FAT" \
          "MEDIA.txt=$OUT_DIR/MEDIA.txt"
        rm -f "$SIMPLE_FAT"
      }
    (
      cd "$OUT_DIR"
      sha256sum "$(basename "$ISO")" | tee "$(basename "$ISO").sha256"
    )
  else
    echo "==> warn: xorriso not installed — skipped ISO (IMG is enough for iDRAC USB map)"
    echo "    install: apt install xorriso   /   brew install xorriso"
  fi
fi

rm -rf "$STAGE"

cat <<EOF

==> boot media ready
  dir:  $OUT_DIR
  img:  $IMG
  img_sha256: $EFI_SHA (EFI payload); see $(basename "$IMG").sha256 for image hash
  iso:  ${ISO:-"(skipped)"}

iDRAC Virtual Media (preferred):
  1. Open Virtual Console → Virtual Media
  2. Map the .img as a virtual USB stick  (or .iso as CD)
  3. Next boot → virtual USB/CD → reboot with serial console open

USB stick (physical):
  ./tools/make-boot-usb.sh --img "$IMG" --disk /dev/diskN

Marker: RAYNU-V-M7-BOOT-MEDIA-OK
EOF
