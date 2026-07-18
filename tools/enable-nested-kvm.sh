#!/usr/bin/env bash
# Prepare the host for RayNu-V M1.1/M1.2 under QEMU nested VT-x.
#
# Symptom this fixes:
#   boot: VM_INSTRUCTION_ERROR=12
#   (VMWRITE fails for every field while VMPTRST shows a valid current VMCS)
#
# Cause: kvm_intel VMCS shadowing — hardware VMWRITE into the shadow VMCS
# rejects fields that should be emulated by KVM (Intel error 12).
# Disabling shadow VMCS makes L1 VMWRITE exit to L0 for software emulation.
set -euo pipefail

if [[ "$(id -u)" -ne 0 ]]; then
  echo "re-running with sudo…"
  exec sudo -- "$0" "$@"
fi

if [[ ! -d /sys/module/kvm_intel ]]; then
  echo "error: kvm_intel is not loaded (AMD host? use kvm_amd nested=1 instead)" >&2
  exit 1
fi

echo "==> current kvm_intel params"
for p in nested enable_shadow_vmcs; do
  f="/sys/module/kvm_intel/parameters/$p"
  if [[ -f "$f" ]]; then
    echo "    $p=$(cat "$f")"
  else
    echo "    $p=(not present on this kernel)"
  fi
done

echo "==> reloading kvm_intel with nested=1 enable_shadow_vmcs=0"
echo "    (quit any QEMU/VMs first if modprobe -r fails)"

modprobe -r kvm_intel 2>/dev/null || true
# Keep kvm core if other modules depend on it; reload intel with flags.
modprobe kvm_intel nested=1 enable_shadow_vmcs=0

echo "==> new kvm_intel params"
for p in nested enable_shadow_vmcs; do
  f="/sys/module/kvm_intel/parameters/$p"
  if [[ -f "$f" ]]; then
    echo "    $p=$(cat "$f")"
  fi
done

if [[ -e /dev/kvm ]]; then
  chmod a+rw /dev/kvm 2>/dev/null || true
  ls -l /dev/kvm
fi

echo
echo "Persistent (optional):"
echo "  echo 'options kvm_intel nested=1 enable_shadow_vmcs=0' | sudo tee /etc/modprobe.d/kvm-raynu.conf"
echo
echo "Then: cd ~/raynu && ./tools/qemu-boot-test.sh"
