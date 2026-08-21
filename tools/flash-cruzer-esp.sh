#!/usr/bin/env bash
# Replace-only flash of EFI/BOOT/BOOTX64.EFI onto the lab Cruzer Micro.
#
# Pillar: [Z] [D]
# Proven Core: outside
#
# Run on raynuvsrv1 (Ubuntu on the R640 PERC), with the Cruzer left in
# front USB 2. Identifies the stick by FAT label RAYNUV + USB + Cruzer
# model. Never uses a hardcoded /dev/sdc. Never dd, never format, never
# touches PERC volumes (sda/sdb).
#
# Usage:
#   ./tools/flash-cruzer-esp.sh --self-test
#   sudo ./tools/flash-cruzer-esp.sh --efi /path/to/r640-hypervisor.efi
#   sudo ./tools/flash-cruzer-esp.sh --efi ./r640-hypervisor.efi --sha256 <hex>
#
# Optional: CRUZER_SERIAL (default 200524441218e7503e33) must match lsblk
# SERIAL when the device reports one.
set -euo pipefail

LABEL="${CRUZER_LABEL:-RAYNUV}"
EXPECT_SERIAL="${CRUZER_SERIAL:-200524441218e7503e33}"
MIN_BYTES=$((256 * 1024 * 1024))
MAX_BYTES=$((4 * 1024 * 1024 * 1024))
MNT_DEFAULT="/mnt/usb"
EFI_PATH=""
EXPECT_SHA=""
SELFTEST=0
DRY=0

usage() {
  sed -n '2,22p' "$0" | sed 's/^# \{0,1\}//'
  exit "${1:-0}"
}

# Host-testable: lab Cruzer vs PERC / vMedia / random USB.
# INVARIANTS:
# - Label must be RAYNUV
# - Transport must be usb
# - Model must contain Cruzer (case-insensitive)
# - Model must not contain PERC / H740 / Virtual
# - Size in [256 MiB, 4 GiB]
# - Never panics
target_is_lab_cruzer() {
  local model="$1" tran="$2" size_bytes="$3" label="$4"
  local lc
  [[ "$label" == "$LABEL" ]] || return 1
  [[ "$tran" == "usb" ]] || return 1
  [[ "$size_bytes" =~ ^[0-9]+$ ]] || return 1
  if (( size_bytes < MIN_BYTES || size_bytes > MAX_BYTES )); then
    return 1
  fi
  lc=$(printf '%s' "$model" | tr '[:upper:]' '[:lower:]')
  [[ "$lc" == *cruzer* ]] || return 1
  [[ "$lc" != *perc* && "$lc" != *h740* && "$lc" != *virtual* ]] || return 1
  return 0
}

self_test() {
  target_is_lab_cruzer "Cruzer Micro" usb 1024966656 RAYNUV
  target_is_lab_cruzer "Cruzer Micro" usb 1024966656 WRONG && return 1
  target_is_lab_cruzer "PERC H740P Mini" usb 1024966656 RAYNUV && return 1
  target_is_lab_cruzer "Cruzer Micro" sas 1024966656 RAYNUV && return 1
  target_is_lab_cruzer "Cruzer Micro" usb $((200 * 1024 * 1024 * 1024)) RAYNUV && return 1
  target_is_lab_cruzer "Virtual Floppy" usb 0 RAYNUV && return 1
  target_is_lab_cruzer "Virtual CD" usb $((1024 * 1024 * 1024)) RAYNUV && return 1
  echo "RAYNU-V-CRUZER-FLASH-SELFTEST-OK"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage 0 ;;
    --self-test) SELFTEST=1; shift ;;
    --dry-run) DRY=1; shift ;;
    --efi) EFI_PATH="${2:-}"; shift 2 ;;
    --sha256) EXPECT_SHA="${2:-}"; shift 2 ;;
    --label) LABEL="${2:-}"; shift 2 ;;
    *)
      echo "error: unknown arg: $1" >&2
      usage 1
      ;;
  esac
done

if [[ "$SELFTEST" == "1" ]]; then
  self_test
  exit 0
fi

if [[ -z "$EFI_PATH" ]]; then
  echo "error: --efi is required (or pass --self-test)" >&2
  usage 1
fi
if [[ ! -f "$EFI_PATH" ]]; then
  echo "error: EFI not found: $EFI_PATH" >&2
  exit 1
fi

OS="$(uname -s)"
if [[ "$OS" != "Linux" ]]; then
  echo "error: flash path is Linux-only (run on raynuvsrv1). Use --self-test on CI." >&2
  exit 1
fi

EFI_ABS="$(readlink -f "$EFI_PATH")"
EFI_BYTES="$(wc -c <"$EFI_ABS" | tr -d ' ')"
if (( EFI_BYTES < 262144 || EFI_BYTES > 20971520 )); then
  echo "error: EFI size $EFI_BYTES is outside 256KiB–20MiB" >&2
  exit 1
fi
GOT_SHA="$(sha256sum "$EFI_ABS" | awk '{print $1}')"
if [[ -n "$EXPECT_SHA" && "$GOT_SHA" != "$EXPECT_SHA" ]]; then
  echo "error: SHA256 mismatch: got $GOT_SHA want $EXPECT_SHA" >&2
  exit 1
fi

LABEL_DEV="/dev/disk/by-label/${LABEL}"
if [[ ! -e "$LABEL_DEV" ]]; then
  echo "error: no block device for label ${LABEL} ($LABEL_DEV)" >&2
  echo "       expect Cruzer Micro in front USB 2; lsusb 0781:5151" >&2
  exit 1
fi

RAW="$(readlink -f "$LABEL_DEV")"
if [[ ! -b "$RAW" ]]; then
  echo "error: not a block device: $RAW" >&2
  exit 1
fi
if [[ "$EFI_ABS" == /mnt/usb/* || "$EFI_ABS" == "$RAW"* ]]; then
  echo "error: --efi must not live on the Cruzer itself" >&2
  exit 1
fi

# Whole-disk FAT on iron (no partition). If this is a partition, query the
# partition; parent must still classify as Cruzer.
MODEL="$(lsblk -bdno MODEL "$RAW" | sed 's/[[:space:]]*$//')"
TRAN="$(lsblk -bdno TRAN "$RAW" | sed 's/[[:space:]]*$//')"
SIZE_BYTES="$(lsblk -bdno SIZE "$RAW" | tr -d ' ')"
SERIAL="$(lsblk -bdno SERIAL "$RAW" | tr -d ' ')"
FSTYPE="$(lsblk -bdno FSTYPE "$RAW" | tr -d ' ')"
TYPE="$(lsblk -bdno TYPE "$RAW" | tr -d ' ')"
LS_LABEL="$(lsblk -bdno LABEL "$RAW" | tr -d ' ')"

echo "==> resolved $LABEL_DEV -> $RAW"
echo "==> model=${MODEL:-?} tran=${TRAN:-?} type=${TYPE:-?} fstype=${FSTYPE:-?} size=${SIZE_BYTES} serial=${SERIAL:-?}"

if [[ "$LS_LABEL" != "$LABEL" ]]; then
  echo "error: lsblk LABEL is '${LS_LABEL}', expected $LABEL" >&2
  exit 1
fi
if ! target_is_lab_cruzer "$MODEL" "$TRAN" "$SIZE_BYTES" "$LS_LABEL"; then
  echo "error: $RAW is not the lab Cruzer (refusing PERC / vMedia / random USB)" >&2
  exit 1
fi
if [[ "$TYPE" != "disk" && "$TYPE" != "part" ]]; then
  echo "error: unexpected lsblk TYPE=$TYPE" >&2
  exit 1
fi
if [[ -n "$SERIAL" && -n "$EXPECT_SERIAL" && "$SERIAL" != "$EXPECT_SERIAL" ]]; then
  echo "error: serial $SERIAL != expected $EXPECT_SERIAL" >&2
  exit 1
fi
if [[ "$MODEL" == *PERC* ]]; then
  echo "error: PERC refused" >&2
  exit 1
fi
ROOT_SRC="$(findmnt -n -o SOURCE / 2>/dev/null || true)"
if [[ -n "$ROOT_SRC" && ( "$ROOT_SRC" == "$RAW" || "$ROOT_SRC" == "$RAW"* ) ]]; then
  echo "error: $RAW is the OS root disk — refusing" >&2
  exit 1
fi

if [[ "$DRY" == "1" ]]; then
  echo "==> dry-run: would copy $EFI_ABS -> ${RAW} EFI/BOOT/BOOTX64.EFI"
  echo "RAYNU-V-CRUZER-FLASH-DRY-OK"
  exit 0
fi

MNT="$MNT_DEFAULT"
EXISTING_MNT="$(findmnt -n -o TARGET "$RAW" 2>/dev/null || true)"
DID_MOUNT=0
if [[ -n "$EXISTING_MNT" ]]; then
  MNT="$EXISTING_MNT"
  echo "==> already mounted at $MNT"
  case "$MNT" in
    /|/boot|/boot/efi|/home)
      echo "error: refusing to use OS mount $MNT" >&2
      exit 1
      ;;
  esac
else
  sudo mkdir -p "$MNT"
  sudo mount -t vfat -o rw,flush "$RAW" "$MNT"
  DID_MOUNT=1
fi

cleanup() {
  if [[ "$DID_MOUNT" == "1" ]]; then
    sudo umount "$MNT" || true
  fi
}
trap cleanup EXIT

if [[ ! -d "$MNT/EFI/BOOT" ]]; then
  echo "error: $MNT/EFI/BOOT missing — not the RayNu-V Cruzer layout" >&2
  exit 1
fi
if [[ ! -f "$MNT/EFI/RayNu/installdisk.bin" ]]; then
  echo "error: $MNT/EFI/RayNu/installdisk.bin missing — refusing (wrong volume)" >&2
  exit 1
fi
INSTALL_BEFORE="$(wc -c <"$MNT/EFI/RayNu/installdisk.bin" | tr -d ' ')"

echo "==> installdisk.bin bytes=$INSTALL_BEFORE (unchanged)"
echo "==> copying $EFI_ABS ($EFI_BYTES bytes, sha256=$GOT_SHA)"
sudo cp --remove-destination "$EFI_ABS" "$MNT/EFI/BOOT/BOOTX64.EFI"
sudo sync -f "$MNT/EFI/BOOT/BOOTX64.EFI" 2>/dev/null || sudo sync

DEST_SHA="$(sha256sum "$MNT/EFI/BOOT/BOOTX64.EFI" | awk '{print $1}')"
DEST_BYTES="$(wc -c <"$MNT/EFI/BOOT/BOOTX64.EFI" | tr -d ' ')"
INSTALL_AFTER="$(wc -c <"$MNT/EFI/RayNu/installdisk.bin" | tr -d ' ')"
if [[ "$DEST_SHA" != "$GOT_SHA" ]]; then
  echo "error: destination SHA256 $DEST_SHA != $GOT_SHA" >&2
  exit 1
fi
if [[ "$INSTALL_AFTER" != "$INSTALL_BEFORE" ]]; then
  echo "error: installdisk.bin size changed ($INSTALL_BEFORE -> $INSTALL_AFTER)" >&2
  exit 1
fi

echo "==> BOOTX64.EFI bytes=$DEST_BYTES sha256=$DEST_SHA"
echo "==> leave $MNT/EFI/RayNu/installdisk.bin and auth.token alone — done"
echo "Next: BIOS boot order stays Ubuntu on PERC; one-time F11 boot the Cruzer."
echo "RAYNU-V-CRUZER-FLASH-OK"
