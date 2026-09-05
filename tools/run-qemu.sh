#!/usr/bin/env bash
# Boot r640-hypervisor.efi under QEMU+OVMF with COM1 on a chardev.
# Prefers KVM when /dev/kvm is usable (required for M1.1 VMXON).
# SERIAL_CHARDEV defaults to stdio; CI sets file:/path/to/log.
# Force TCG: QEMU_ACCEL=tcg ./tools/run-qemu.sh
# ADR-011 evidence mode: EVIDENCE_MODE=1 stages paperverbose.txt on the ESP.
# E5 ISO install lab: ISO_INSTALL_LAB=1 stages isoinstall.txt (1MiB virtio disk).
# Stage 46: PRODUCT_ISO=/path/to/distro.iso stages EFI/RayNu/linux.iso (not default).
# Product ISO defaults QEMU_MEM=2560M so leftover DRAM exists above PRECISE
# (512MiB). iso=0 / boot gate stay 512M. Not ISO-INSTALL-OK.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

TARGET="${TARGET:-x86_64-unknown-uefi}"
PROFILE="${PROFILE:-release}"
EFI="target/${TARGET}/${PROFILE}/r640-hypervisor.efi"
SERIAL_CHARDEV="${SERIAL_CHARDEV:-stdio}"
QEMU_ACCEL="${QEMU_ACCEL:-auto}"
EVIDENCE_MODE="${EVIDENCE_MODE:-0}"

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
# M3.8: prefer real tinyconfig bzImage; fall back to minimal proto fixture.
BZIMAGE_SRC="${BZIMAGE_SRC:-}"
if [[ -z "$BZIMAGE_SRC" ]]; then
  if [[ -f "$ROOT/assets/bzImage.real" ]]; then
    BZIMAGE_SRC="$ROOT/assets/bzImage.real"
  elif [[ -f "$ROOT/assets/bzImage" ]] && [[ "$(wc -c <"$ROOT/assets/bzImage")" -gt 32768 ]]; then
    BZIMAGE_SRC="$ROOT/assets/bzImage"
  else
    BZIMAGE_SRC="$ROOT/assets/bzImage"
  fi
fi
if [[ ! -f "$BZIMAGE_SRC" ]]; then
  echo "==> generating minimal bzImage fixture"
  "$ROOT/tools/gen-minimal-bzimage.sh" "$BZIMAGE_SRC"
fi
cp "$BZIMAGE_SRC" "$ESP/EFI/BOOT/BZIMAGE"
echo "==> ESP BZIMAGE: $ESP/EFI/BOOT/BZIMAGE ($(wc -c <"$ESP/EFI/BOOT/BZIMAGE") bytes) from $BZIMAGE_SRC"
# M3.10: optional real initrd (static /init → SHELL-OK).
INITRD_SRC="${INITRD_SRC:-$ROOT/assets/initrd}"
if [[ -f "$INITRD_SRC" ]]; then
  cp "$INITRD_SRC" "$ESP/EFI/BOOT/INITRD"
  echo "==> ESP INITRD: $ESP/EFI/BOOT/INITRD ($(wc -c <"$ESP/EFI/BOOT/INITRD") bytes)"
fi

# ADR-014 Presence rule: stage a real 1-4 MiB system OVMF.fd onto the ESP
# (split-mode, not PE embed). Skip images without _FVH at offset 0x28 —
# those fail accept_real_ovmf_bytes.
mkdir -p "$ESP/EFI/RayNu"
rm -f "$ESP/EFI/RayNu/OVMF.fd"
ovmf_has_fvh() {
  # EFI_FIRMWARE_VOLUME_HEADER.Signature at 0x28 is "_FVH".
  local sig
  sig=$(od -An -tx1 -N 4 -j 40 "$1" 2>/dev/null | tr -d ' \n')
  [[ "$sig" == "5f465648" ]]
}
GUEST_OVMF_STAGED=0
for f in ${GUEST_OVMF:-} ${OVMF_BIOS:-} ${OVMF_CODE:-} \
  /usr/share/OVMF/OVMF.fd \
  /usr/share/ovmf/OVMF.fd \
  /usr/share/OVMF/OVMF_CODE.fd \
  /usr/share/OVMF/OVMF_CODE_4M.fd \
  /usr/share/edk2/ovmf/OVMF.fd \
  /usr/share/edk2/ovmf/OVMF_CODE.fd \
  /usr/share/edk2-ovmf/x64/OVMF_CODE.fd
do
  [[ -n "$f" && -f "$f" ]] || continue
  sz=$(wc -c <"$f" | tr -d ' ')
  if (( sz >= 1048576 && sz <= 4194304 )) && ovmf_has_fvh "$f"; then
    cp "$f" "$ESP/EFI/RayNu/OVMF.fd"
    echo "==> Guest ESP OVMF.fd: $ESP/EFI/RayNu/OVMF.fd ($sz bytes) from $f"
    GUEST_OVMF_STAGED=1
    break
  fi
done
if [[ "$GUEST_OVMF_STAGED" != "1" ]]; then
  echo "error: no real 1-4 MiB _FVH OVMF.fd to stage at EFI/RayNu/OVMF.fd" >&2
  exit 1
fi

# ADR-011: stage paperverbose.txt so the EFI activates evidence mode.
# Default path stays clean when EVIDENCE_MODE is unset/0.
rm -f "$ESP/paperverbose.txt" "$ESP/EFI/RayNu/paperverbose.txt" 2>/dev/null || true
if [[ "$EVIDENCE_MODE" == "1" ]]; then
  mkdir -p "$ESP/EFI/RayNu"
  : >"$ESP/EFI/RayNu/paperverbose.txt"
  echo "==> ADR-011 evidence mode: staged $ESP/EFI/RayNu/paperverbose.txt"
fi

# ADR-016 F2b: RAYNU_F=1 stages EFI/RayNu/raynuf.txt so the EFI launches the
# RayNu-F test app on the private VMCS after the OVMF leg stops. Default clean.
rm -f "$ESP/EFI/RayNu/raynuf.txt" 2>/dev/null || true
if [[ "${RAYNU_F:-0}" == "1" ]]; then
  mkdir -p "$ESP/EFI/RayNu"
  : >"$ESP/EFI/RayNu/raynuf.txt"
  echo "==> ADR-016 RayNu-F: staged $ESP/EFI/RayNu/raynuf.txt"
fi

# Stage 46: leftover product ISO would HOLD guest-UEFI instead of E4 LINUX-EARLY.
rm -f "$ESP/linux.iso" "$ESP/install.iso" \
  "$ESP/EFI/RayNu/linux.iso" "$ESP/EFI/RayNu/install.iso" 2>/dev/null || true
PRODUCT_ISO="${PRODUCT_ISO:-}"
if [[ -n "$PRODUCT_ISO" ]]; then
  if [[ ! -f "$PRODUCT_ISO" ]]; then
    echo "error: PRODUCT_ISO not found: $PRODUCT_ISO" >&2
    exit 1
  fi
  psz=$(wc -c <"$PRODUCT_ISO" | tr -d ' ')
  if (( psz <= 73728 )); then
    echo "error: PRODUCT_ISO is lab-stub sized ($psz); need >73728" >&2
    exit 1
  fi
  mkdir -p "$ESP/EFI/RayNu"
  cp "$PRODUCT_ISO" "$ESP/EFI/RayNu/linux.iso"
  echo "==> Stage 46 product ISO: $ESP/EFI/RayNu/linux.iso ($psz bytes) (not ISO-INSTALL-OK)"
fi

# Leftover DRAM for report-RAM extras lives above PRECISE (512MiB). Product
# ISO HOLDS nested guest-UEFI, so seed those HPAs; -m 512M has none.
if [[ -n "$PRODUCT_ISO" ]]; then
  QEMU_MEM="${QEMU_MEM:-2560M}"
else
  QEMU_MEM="${QEMU_MEM:-512M}"
fi

# E5 lab: stage isoinstall.txt → arm 1MiB install-sized virtio-blk (no curl).
ISO_INSTALL_LAB="${ISO_INSTALL_LAB:-0}"
ISO_REBOOT_LAB="${ISO_REBOOT_LAB:-0}"
HOST_NIC_LAB="${HOST_NIC_LAB:-0}"
rm -f "$ESP/isoinstall.txt" "$ESP/EFI/RayNu/isoinstall.txt" 2>/dev/null || true
rm -f "$ESP/isoreboot.txt" "$ESP/EFI/RayNu/isoreboot.txt" 2>/dev/null || true
rm -f "$ESP/installdisk.bin" "$ESP/EFI/RayNu/installdisk.bin" 2>/dev/null || true
rm -f "$ESP/hostnic.txt" "$ESP/EFI/RayNu/hostnic.txt" 2>/dev/null || true
if [[ "$ISO_INSTALL_LAB" == "1" ]]; then
  mkdir -p "$ESP/EFI/RayNu"
  : >"$ESP/EFI/RayNu/isoinstall.txt"
  echo "==> E5 ISO install lab: staged $ESP/EFI/RayNu/isoinstall.txt (1MiB disk)"
fi
if [[ "$ISO_REBOOT_LAB" == "1" ]]; then
  mkdir -p "$ESP/EFI/RayNu"
  : >"$ESP/EFI/RayNu/isoreboot.txt"
  INSTALL_DISK_IMG="${INSTALL_DISK_IMG:-$ROOT/target/e5-lab-install.img}"
  if [[ ! -f "$INSTALL_DISK_IMG" ]]; then
    echo "error: ISO_REBOOT_LAB=1 requires INSTALL_DISK_IMG ($INSTALL_DISK_IMG)" >&2
    exit 1
  fi
  cp "$INSTALL_DISK_IMG" "$ESP/EFI/RayNu/installdisk.bin"
  echo "==> E5 ISO reboot lab: staged isoreboot.txt + installdisk.bin ($(wc -c <"$INSTALL_DISK_IMG") bytes)"
fi

# ADR-013 Phase C: QEMU e1000 + user-net hostfwd; ESP flag exits after GET /.
HOST_NIC_ARGS=()
if [[ "$HOST_NIC_LAB" == "1" ]]; then
  mkdir -p "$ESP/EFI/RayNu"
  : >"$ESP/EFI/RayNu/hostnic.txt"
  HOST_NIC_FWD="${HOST_NIC_FWD:-18443}"
  echo "==> ADR-013 Phase C lab: staged $ESP/EFI/RayNu/hostnic.txt (e1000 hostfwd :${HOST_NIC_FWD} -> :8443)"
  HOST_NIC_ARGS+=(
    -netdev "user,id=n0,hostfwd=tcp:127.0.0.1:${HOST_NIC_FWD}-:8443"
    -device e1000,netdev=n0
  )
fi

FW_ARGS=()
if [[ -n "${OVMF_BIOS}" ]]; then
  echo "==> OVMF (bios): $OVMF_BIOS"
  FW_ARGS+=(-bios "$OVMF_BIOS")
else
  echo "==> OVMF (pflash): $OVMF_CODE"
  FW_ARGS+=(-drive if=pflash,format=raw,readonly=on,file="$OVMF_CODE")
fi

kvm_usable() {
  [[ -e /dev/kvm && -r /dev/kvm && -w /dev/kvm ]]
}

ACCEL_ARGS=()
if [[ "$QEMU_ACCEL" == "tcg" ]]; then
  echo "==> accel: tcg (VMXON will SKIP)"
  ACCEL_ARGS+=(-machine q35,accel=tcg -cpu qemu64)
elif [[ "$QEMU_ACCEL" == "kvm" ]]; then
  if ! kvm_usable; then
    echo "error: QEMU_ACCEL=kvm but /dev/kvm is not usable (permission?)" >&2
    ls -l /dev/kvm 2>&1 || true
    exit 1
  fi
  echo "==> accel: kvm (nested VT-x for M1.1/M1.2)"
  # Only request +vmx when the host CPU advertises it; many cloud runners
  # expose /dev/kvm without nested VT-x (QEMU then clears guest CPUID.VMX).
  if grep -qw vmx /proc/cpuinfo 2>/dev/null; then
    ACCEL_ARGS+=(-machine q35,accel=kvm -enable-kvm -cpu host,+vmx)
  else
    echo "==> note: host CPUID lacks vmx — guest VMXON will SKIP"
    ACCEL_ARGS+=(-machine q35,accel=kvm -enable-kvm -cpu host)
  fi
elif [[ "$QEMU_ACCEL" == "auto" ]] && kvm_usable; then
  echo "==> accel: kvm (nested VT-x for M1.1/M1.2)"
  if grep -qw vmx /proc/cpuinfo 2>/dev/null; then
    ACCEL_ARGS+=(-machine q35,accel=kvm -enable-kvm -cpu host,+vmx)
  else
    echo "==> note: host CPUID lacks vmx — guest VMXON will SKIP"
    ACCEL_ARGS+=(-machine q35,accel=kvm -enable-kvm -cpu host)
  fi
else
  echo "==> accel: tcg fallback (/dev/kvm missing or not writable; VMXON will SKIP)"
  ACCEL_ARGS+=(-machine q35,accel=tcg -cpu qemu64)
fi

# QEMU vvfat (`fat:rw:`) is capped at ~516 MB ("Directory does not fit in
# FAT16/FAT32 (capacity 516.06 MB)"), so a product ISO above that
# (alpine-extended 994 MiB, the only official x86_64 ISO with on-media
# grub-efi + dosfstools) must ride a real FAT32 image. Built fresh per run;
# mtools when present, else sudo loop mount (the harness already sudo's kvm).
ESP_DRIVE="fat:rw:$ESP"
esp_bytes=$(du -sb "$ESP" | cut -f1)
if (( esp_bytes > 480 * 1024 * 1024 )); then
  ESP_IMG="${ESP_IMG:-$ROOT/target/esp-fat32.img}"
  img_mib=$(( esp_bytes / 1048576 + 128 ))
  rm -f "$ESP_IMG"
  truncate -s "${img_mib}M" "$ESP_IMG"
  mkfs.vfat -F 32 -n RAYNUV "$ESP_IMG" >/dev/null
  if command -v mcopy >/dev/null 2>&1; then
    MTOOLS_SKIP_CHECK=1 mcopy -i "$ESP_IMG" -s "$ESP"/* ::/
  else
    mnt=$(mktemp -d)
    sudo mount -o loop,uid="$(id -u)",gid="$(id -g)" "$ESP_IMG" "$mnt"
    cp -r "$ESP"/. "$mnt"/
    sync
    sudo umount "$mnt"
    rmdir "$mnt"
  fi
  ESP_DRIVE="$ESP_IMG"
  echo "==> ESP ${esp_bytes} bytes exceeds vvfat; FAT32 image $ESP_IMG (${img_mib} MiB)"
fi

echo "==> QEMU boot (COM1 → ${SERIAL_CHARDEV}); mem=${QEMU_MEM}; guest exits via isa-debug-exit"

exec qemu-system-x86_64 \
  "${ACCEL_ARGS[@]}" \
  -m "$QEMU_MEM" \
  -display none \
  -serial "$SERIAL_CHARDEV" \
  -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
  "${FW_ARGS[@]}" \
  -drive format=raw,file="$ESP_DRIVE" \
  "${HOST_NIC_ARGS[@]}" \
  "$@"
