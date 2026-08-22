#!/usr/bin/env bash
# Boot r640-hypervisor.efi under QEMU+OVMF with COM1 on a chardev.
# Prefers KVM when /dev/kvm is usable (required for M1.1 VMXON).
# SERIAL_CHARDEV defaults to stdio; CI sets file:/path/to/log.
# Force TCG: QEMU_ACCEL=tcg ./tools/run-qemu.sh
# ADR-011 evidence mode: EVIDENCE_MODE=1 stages paperverbose.txt on the ESP.
# E5 ISO install lab: ISO_INSTALL_LAB=1 stages isoinstall.txt (1MiB virtio disk).
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

echo "==> QEMU boot (COM1 → ${SERIAL_CHARDEV}); guest exits via isa-debug-exit"

exec qemu-system-x86_64 \
  "${ACCEL_ARGS[@]}" \
  -m 512M \
  -display none \
  -serial "$SERIAL_CHARDEV" \
  -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
  "${FW_ARGS[@]}" \
  -drive format=raw,file=fat:rw:"$ESP" \
  "${HOST_NIC_ARGS[@]}" \
  "$@"
