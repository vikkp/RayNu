#!/usr/bin/env bash
# Write a RayNu-V UEFI FAT boot image to a physical USB stick.
#
# DESTRUCTIVE — erases the target disk.
#
# Usage:
#   ./tools/make-boot-usb.sh --img dist/raynu-v-0.1.0-boot-media/raynu-v-0.1.0-uefi-boot.img --disk /dev/disk4
#   ./tools/make-boot-usb.sh --img ... --disk /dev/sdb          # Linux
#
# On macOS, prefer the whole-disk id (diskN), not diskNs1.
# Prefer iDRAC Virtual Media (.img map) when the R640 is remote — see
# docs/runbooks/media_maker.md.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

IMG=""
DISK=""
YES="${YES:-0}"

usage() {
  sed -n '2,14p' "$0" | sed 's/^# \{0,1\}//'
  exit "${1:-0}"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage 0 ;;
    --img) IMG="${2:-}"; shift 2 ;;
    --disk) DISK="${2:-}"; shift 2 ;;
    --yes) YES=1; shift ;;
    *)
      echo "error: unknown arg: $1" >&2
      usage 1
      ;;
  esac
done

if [[ -z "$IMG" || -z "$DISK" ]]; then
  echo "error: --img and --disk are required" >&2
  usage 1
fi
if [[ ! -f "$IMG" ]]; then
  echo "error: image not found: $IMG" >&2
  exit 1
fi

OS="$(uname -s)"
echo "==> OS: $OS"
echo "==> image: $IMG ($(du -h "$IMG" | awk '{print $1}'))"
echo "==> disk:  $DISK"
echo
echo "WARNING: This will ERASE all data on $DISK."
echo

if [[ "$YES" != "1" ]]; then
  printf "Type the disk path again to confirm (%s): " "$DISK"
  read -r CONFIRM
  if [[ "$CONFIRM" != "$DISK" ]]; then
    echo "error: confirmation mismatch — aborted" >&2
    exit 1
  fi
fi

case "$OS" in
  Darwin)
    # Accept disk4 or /dev/disk4
    RAW="$DISK"
    [[ "$RAW" == /dev/* ]] || RAW="/dev/$RAW"
    if [[ "$RAW" == *s[0-9]* ]]; then
      echo "error: pass the whole disk (e.g. /dev/disk4), not a partition" >&2
      exit 1
    fi
    echo "==> unmounting $RAW"
    diskutil unmountDisk "$RAW" || true
    echo "==> writing image (rdisk for speed when available)"
    RDISK="${RAW/disk/rdisk}"
    if [[ -c "$RDISK" ]]; then
      sudo dd if="$IMG" of="$RDISK" bs=4m status=progress
    else
      sudo dd if="$IMG" of="$RAW" bs=4m status=progress
    fi
    sync
    diskutil eject "$RAW" || true
    ;;
  Linux)
    RAW="$DISK"
    [[ "$RAW" == /dev/* ]] || RAW="/dev/$RAW"
    if [[ ! -b "$RAW" ]]; then
      echo "error: not a block device: $RAW" >&2
      exit 1
    fi
    echo "==> writing image"
    sudo dd if="$IMG" of="$RAW" bs=4M status=progress conv=fsync
    sync
    ;;
  *)
    echo "error: unsupported OS '$OS' — write the .img with your platform dd/Etcher" >&2
    exit 1
    ;;
esac

echo
echo "==> USB written. Plug into the R640 (or leave inserted), open iDRAC serial,"
echo "    one-time boot to USB (F11), watch for RAYNU-V-M0-BOOT-OK."
echo "RAYNU-V-M7-BOOT-USB-OK"
