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

param() {
  local f="/sys/module/kvm_intel/parameters/$1"
  if [[ -f "$f" ]]; then
    cat "$f"
  else
    echo "(missing)"
  fi
}

echo "==> current kvm_intel params"
echo "    nested=$(param nested)"
echo "    enable_shadow_vmcs=$(param enable_shadow_vmcs)"

if [[ ! -f /sys/module/kvm_intel/parameters/enable_shadow_vmcs ]]; then
  echo "error: this kernel has no enable_shadow_vmcs module param" >&2
  echo "       M1.2 under nested QEMU needs a kernel that exposes it, or bare-metal boot." >&2
  exit 1
fi

shadow_before="$(param enable_shadow_vmcs)"
if [[ "$shadow_before" == "0" || "$shadow_before" == "N" || "$shadow_before" == "n" ]]; then
  echo "==> shadow VMCS already off — nothing to reload"
else
  echo "==> reloading kvm_intel with nested=1 enable_shadow_vmcs=0"
  echo "    (all QEMU/VMs using KVM must be quit first)"

  # Do NOT swallow unload failure: if the module stays loaded, modprobe
  # will not apply new parameters and M1.2 will keep failing with error 12.
  if ! modprobe -r kvm_intel; then
    echo "error: could not unload kvm_intel (is QEMU still running?)" >&2
    echo "       quit every VM, then re-run: sudo $0" >&2
    fuser -v /dev/kvm 2>&1 || true
    exit 1
  fi

  modprobe kvm_intel nested=1 enable_shadow_vmcs=0
fi

echo "==> new kvm_intel params"
echo "    nested=$(param nested)"
echo "    enable_shadow_vmcs=$(param enable_shadow_vmcs)"

shadow_after="$(param enable_shadow_vmcs)"
if [[ "$shadow_after" != "0" && "$shadow_after" != "N" && "$shadow_after" != "n" ]]; then
  echo "error: enable_shadow_vmcs is still '${shadow_after}' (want 0/N)" >&2
  echo "       check /etc/modprobe.d for overrides, then retry." >&2
  exit 1
fi

nested_after="$(param nested)"
if [[ "$nested_after" != "Y" && "$nested_after" != "y" && "$nested_after" != "1" ]]; then
  echo "error: nested is '${nested_after}' (want Y/1)" >&2
  exit 1
fi

if [[ -e /dev/kvm ]]; then
  chmod a+rw /dev/kvm 2>/dev/null || true
  ls -l /dev/kvm
fi

conf=/etc/modprobe.d/kvm-raynu.conf
if [[ ! -f "$conf" ]] || ! grep -q 'enable_shadow_vmcs=0' "$conf" 2>/dev/null; then
  echo
  echo "Persistent (optional, survives reboot):"
  echo "  echo 'options kvm_intel nested=1 enable_shadow_vmcs=0' | sudo tee $conf"
fi

echo
echo "OK — host ready for M1.2. Then:"
echo "  cd ~/raynu && ./tools/qemu-boot-test.sh"
