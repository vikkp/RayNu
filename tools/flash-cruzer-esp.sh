#!/usr/bin/env bash
# Replace-only flash of EFI/BOOT/BOOTX64.EFI onto the lab Cruzer Micro,
# plus EFI/RayNu/OVMF.fd (ADR-014 guest-UEFI retain). Leaves
# installdisk.bin and auth.token alone.
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
#   sudo ./tools/flash-cruzer-esp.sh --efi ./r640-hypervisor.efi --ovmf /path/to/OVMF.fd
#   sudo ./tools/flash-cruzer-esp.sh --efi ./r640-hypervisor.efi --no-ovmf
#   sudo ./tools/flash-cruzer-esp.sh --efi ./r640-hypervisor.efi --linux-iso /path/alpine.iso
#   sudo ./tools/flash-cruzer-esp.sh --efi ./r640-hypervisor.efi --no-linux-iso
#
# Stage 46: alpine-virt linux.iso is ~63 MiB. The 977.5 MiB Cruzer has
# room if leftover/partial ISOs are pruned first (keep installdisk.bin
# and auth.token). --linux-iso unlinks ESP *.iso then checks df.
# ENOSPC can leave FAT32 FSInfo stale (df << 977 MiB minus du). Remount
# and fsck.vfat -a reclaim orphaned clusters. Never mkfs / format.
#
# Optional: CRUZER_SERIAL (default 200524441218e7503e33) must match lsblk
# SERIAL when the device reports one. GUEST_OVMF overrides the host search.
set -euo pipefail

LABEL="${CRUZER_LABEL:-RAYNUV}"
EXPECT_SERIAL="${CRUZER_SERIAL:-200524441218e7503e33}"
MIN_BYTES=$((256 * 1024 * 1024))
MAX_BYTES=$((4 * 1024 * 1024 * 1024))
MNT_DEFAULT="/mnt/usb"
EFI_PATH=""
EXPECT_SHA=""
OVMF_SRC=""
NO_OVMF=0
LINUX_ISO=""
NO_LINUX_ISO=0
SELFTEST=0
DRY=0

usage() {
  sed -n '2,28p' "$0" | sed 's/^# \{0,1\}//'
  exit "${1:-0}"
}

# EFI_FIRMWARE_VOLUME_HEADER.Signature at 0x28 is "_FVH".
# Same accept rule as tools/run-qemu.sh / boot::ovmf_esp::accept_real_ovmf_bytes.
ovmf_has_fvh() {
  local sig
  sig=$(od -An -tx1 -N 4 -j 40 "$1" 2>/dev/null | tr -d ' \n')
  [[ "$sig" == "5f465648" ]]
}

# Prefer 4M CODE then combined OVMF.fd. Size 1-4 MiB + _FVH (not a fixture).
pick_host_ovmf() {
  local f sz
  for f in ${OVMF_SRC:-} ${GUEST_OVMF:-} \
    /usr/share/OVMF/OVMF_CODE_4M.fd \
    /usr/share/OVMF/OVMF.fd \
    /usr/share/ovmf/OVMF.fd \
    /usr/share/OVMF/OVMF_CODE.fd \
    /usr/share/edk2/ovmf/OVMF.fd \
    /usr/share/edk2/ovmf/OVMF_CODE.fd \
    /usr/share/edk2-ovmf/x64/OVMF_CODE.fd
  do
    [[ -n "$f" && -f "$f" ]] || continue
    sz=$(wc -c <"$f" | tr -d ' ')
    if (( sz >= 1048576 && sz <= 4194304 )) && ovmf_has_fvh "$f"; then
      printf '%s\n' "$f"
      return 0
    fi
  done
  return 1
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
  local tmp
  target_is_lab_cruzer "Cruzer Micro" usb 1024966656 RAYNUV
  target_is_lab_cruzer "Cruzer Micro" usb 1024966656 WRONG && return 1
  target_is_lab_cruzer "PERC H740P Mini" usb 1024966656 RAYNUV && return 1
  target_is_lab_cruzer "Cruzer Micro" sas 1024966656 RAYNUV && return 1
  target_is_lab_cruzer "Cruzer Micro" usb $((200 * 1024 * 1024 * 1024)) RAYNUV && return 1
  target_is_lab_cruzer "Virtual Floppy" usb 0 RAYNUV && return 1
  target_is_lab_cruzer "Virtual CD" usb $((1024 * 1024 * 1024)) RAYNUV && return 1
  tmp="$(mktemp)"
  dd if=/dev/zero of="$tmp" bs=64 count=1 status=none
  printf '\x5f\x46\x56\x48' | dd of="$tmp" bs=1 seek=40 conv=notrunc status=none
  ovmf_has_fvh "$tmp" || { rm -f "$tmp"; return 1; }
  printf '\x00\x00\x00\x00' | dd of="$tmp" bs=1 seek=40 conv=notrunc status=none
  if ovmf_has_fvh "$tmp"; then
    rm -f "$tmp"
    return 1
  fi
  rm -f "$tmp"
  echo "RAYNU-V-CRUZER-FLASH-SELFTEST-OK"
}

esp_avail_bytes() {
  df -B1 --output=avail "$MNT" | tail -n1 | tr -d ' '
}

remount_cruzer_vfat() {
  sudo umount "$MNT" || true
  DID_MOUNT=0
  sudo mkdir -p "$MNT"
  sudo mount -t vfat -o rw,flush "$RAW" "$MNT"
  DID_MOUNT=1
}

# Reclaim leaked FAT clusters / stale FSInfo after ENOSPC. Never mkfs.
reclaim_fat_free_if_needed() {
  local need="$1"
  local avail
  avail="$(esp_avail_bytes)"
  if [[ "$avail" =~ ^[0-9]+$ ]] && (( avail >= need )); then
    echo "==> ESP free=$avail need=$need"
    return 0
  fi
  echo "==> ESP free=${avail:-?} need=$need — remount to flush FAT32 FSInfo (not format)"
  remount_cruzer_vfat
  avail="$(esp_avail_bytes)"
  if [[ "$avail" =~ ^[0-9]+$ ]] && (( avail >= need )); then
    echo "==> ESP free=$avail need=$need (after remount)"
    return 0
  fi
  if ! command -v fsck.vfat >/dev/null; then
    echo "error: fsck.vfat missing (apt install dosfstools); not formatting" >&2
    return 1
  fi
  echo "==> fsck.vfat -a $RAW to reclaim orphaned clusters (not format)"
  sudo umount "$MNT" || true
  DID_MOUNT=0
  set +e
  sudo fsck.vfat -a "$RAW"
  fsck_rc=$?
  set -e
  if (( fsck_rc > 1 )); then
    echo "error: fsck.vfat rc=$fsck_rc (not format)" >&2
    return 1
  fi
  remount_cruzer_vfat
  avail="$(esp_avail_bytes)"
  echo "==> ESP free=${avail:-?} need=$need (after fsck.vfat)"
  if [[ ! "$avail" =~ ^[0-9]+$ ]] || (( avail < need )); then
    return 1
  fi
  return 0
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage 0 ;;
    --self-test) SELFTEST=1; shift ;;
    --dry-run) DRY=1; shift ;;
    --efi) EFI_PATH="${2:-}"; shift 2 ;;
    --sha256) EXPECT_SHA="${2:-}"; shift 2 ;;
    --ovmf) OVMF_SRC="${2:-}"; shift 2 ;;
    --no-ovmf) NO_OVMF=1; shift ;;
    --linux-iso) LINUX_ISO="${2:-}"; shift 2 ;;
    --no-linux-iso) NO_LINUX_ISO=1; shift ;;
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

if [[ "$NO_OVMF" == "1" && -n "$OVMF_SRC" ]]; then
  echo "error: --ovmf and --no-ovmf are mutually exclusive" >&2
  exit 1
fi
if [[ "$NO_LINUX_ISO" == "1" && -n "$LINUX_ISO" ]]; then
  echo "error: --linux-iso and --no-linux-iso are mutually exclusive" >&2
  exit 1
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
  if [[ "$NO_OVMF" == "1" ]]; then
    echo "==> dry-run: would skip EFI/RayNu/OVMF.fd"
  else
    echo "==> dry-run: would stage EFI/RayNu/OVMF.fd (1-4 MiB _FVH)"
  fi
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
if [[ "$DEST_SHA" != "$GOT_SHA" ]]; then
  echo "error: destination SHA256 $DEST_SHA != $GOT_SHA" >&2
  exit 1
fi

if [[ "$NO_OVMF" == "1" ]]; then
  echo "==> --no-ovmf: leaving EFI/RayNu/OVMF.fd alone (guest-UEFI skips if missing)"
else
  OVMF_PICK="$(pick_host_ovmf || true)"
  if [[ -z "$OVMF_PICK" ]]; then
    echo "error: no real 1-4 MiB _FVH OVMF.fd to stage at EFI/RayNu/OVMF.fd" >&2
    echo "       apt install ovmf  (or pass --ovmf PATH / --no-ovmf)" >&2
    exit 1
  fi
  OVMF_BYTES="$(wc -c <"$OVMF_PICK" | tr -d ' ')"
  echo "==> staging EFI/RayNu/OVMF.fd ($OVMF_BYTES bytes) from $OVMF_PICK"
  sudo mkdir -p "$MNT/EFI/RayNu"
  sudo cp --remove-destination "$OVMF_PICK" "$MNT/EFI/RayNu/OVMF.fd"
  sudo sync -f "$MNT/EFI/RayNu/OVMF.fd" 2>/dev/null || sudo sync
  DEST_OVMF_BYTES="$(wc -c <"$MNT/EFI/RayNu/OVMF.fd" | tr -d ' ')"
  if (( DEST_OVMF_BYTES < 1048576 || DEST_OVMF_BYTES > 4194304 )); then
    echo "error: staged OVMF.fd size $DEST_OVMF_BYTES is outside 1-4 MiB" >&2
    exit 1
  fi
  if ! ovmf_has_fvh "$MNT/EFI/RayNu/OVMF.fd"; then
    echo "error: staged EFI/RayNu/OVMF.fd missing _FVH at 0x28" >&2
    exit 1
  fi
  echo "==> OVMF.fd bytes=$DEST_OVMF_BYTES _FVH=ok"
fi

if [[ "$NO_LINUX_ISO" == "1" ]]; then
  sudo rm -f "$MNT/linux.iso" "$MNT/EFI/RayNu/linux.iso" "$MNT/EFI/RayNu/install.iso"
  echo "==> --no-linux-iso: removed product ISO (E4 LINUX-EARLY)"
elif [[ -n "$LINUX_ISO" ]]; then
  if [[ ! -f "$LINUX_ISO" ]]; then
    echo "error: --linux-iso not found: $LINUX_ISO" >&2
    exit 1
  fi
  LINUX_ABS="$(readlink -f "$LINUX_ISO")"
  LINUX_BYTES="$(wc -c <"$LINUX_ABS" | tr -d ' ')"
  if (( LINUX_BYTES <= 73728 )); then
    echo "error: --linux-iso is lab-stub sized ($LINUX_BYTES); need >73728" >&2
    exit 1
  fi
  if [[ "$LINUX_ABS" == /mnt/usb/* || "$LINUX_ABS" == "$RAW"* || "$LINUX_ABS" == "$MNT"* ]]; then
    echo "error: --linux-iso must not live on the Cruzer itself" >&2
    exit 1
  fi
  # Failed cp leaves a partial linux.iso that consumes the last free clusters.
  echo "==> pruning leftover ESP ISOs (partial ENOSPC + extras) before linux.iso"
  sudo find "$MNT" -iname '*.iso' -print -delete || true
  sudo sync -f "$MNT" 2>/dev/null || sudo sync
  NEED=$((LINUX_BYTES + 1048576))
  if ! reclaim_fat_free_if_needed "$NEED"; then
    echo "error: Cruzer ESP has $(esp_avail_bytes) bytes free; need $NEED for linux.iso" >&2
    echo "       keep EFI/BOOT/BOOTX64.EFI EFI/RayNu/OVMF.fd EFI/RayNu/installdisk.bin EFI/RayNu/auth.token" >&2
    echo "       stale FAT32 FSInfo after ENOSPC: fsck.vfat -a (not format), then retry" >&2
    sudo du -ah "$MNT" | sort -h | tail -20 >&2 || true
    exit 1
  fi
  echo "==> staging EFI/RayNu/linux.iso ($LINUX_BYTES bytes) from $LINUX_ABS"
  sudo mkdir -p "$MNT/EFI/RayNu"
  sudo cp --remove-destination "$LINUX_ABS" "$MNT/EFI/RayNu/linux.iso"
  sudo sync -f "$MNT/EFI/RayNu/linux.iso" 2>/dev/null || sudo sync
  DEST_LINUX_BYTES="$(wc -c <"$MNT/EFI/RayNu/linux.iso" | tr -d ' ')"
  if [[ "$DEST_LINUX_BYTES" != "$LINUX_BYTES" ]]; then
    echo "error: staged linux.iso size $DEST_LINUX_BYTES != $LINUX_BYTES" >&2
    exit 1
  fi
  echo "==> linux.iso bytes=$DEST_LINUX_BYTES (Stage 46; not ISO-INSTALL-OK)"
fi

INSTALL_AFTER="$(wc -c <"$MNT/EFI/RayNu/installdisk.bin" | tr -d ' ')"
if [[ "$INSTALL_AFTER" != "$INSTALL_BEFORE" ]]; then
  echo "error: installdisk.bin size changed ($INSTALL_BEFORE -> $INSTALL_AFTER)" >&2
  exit 1
fi

echo "==> BOOTX64.EFI bytes=$DEST_BYTES sha256=$DEST_SHA"
echo "==> leave $MNT/EFI/RayNu/installdisk.bin and auth.token alone — done"
echo "Next: BIOS boot order stays Ubuntu on PERC; one-time F11 boot the Cruzer."
echo "Confirm EFI/RayNu/OVMF.fd (1-4 MiB, _FVH) before F11 if Stage 44."
echo "RAYNU-V-CRUZER-FLASH-OK"
