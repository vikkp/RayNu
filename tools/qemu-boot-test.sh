#!/usr/bin/env bash
# Boot integration gate: build EFI, boot under QEMU, require serial markers.
# M0:   RAYNU-V-M0-BOOT-OK
# M1.0: RAYNU-V-M1-EBS-OK
# M1.1: RAYNU-V-M1-VMXON-OK (or SKIP without usable KVM unless REQUIRE_VMX=1)
# M1.2: RAYNU-V-M1-VMEXIT-OK (required when VMXON succeeds / REQUIRE_VMX=1)
# M2.0: RAYNU-V-M2-EPT-OK   (required when VMXON succeeds / REQUIRE_VMX=1)
# M2.1: RAYNU-V-M2-GUEST-OK (required when VMXON succeeds / REQUIRE_VMX=1)
# M2.2: RAYNU-V-M2-OWN-OK   (required when VMXON succeeds / REQUIRE_VMX=1)
# M2.3: RAYNU-V-M2-ALLOC-OK (required when VMXON succeeds / REQUIRE_VMX=1)
# M2.4: RAYNU-V-M2-IRQ-OK   (required when VMXON succeeds / REQUIRE_VMX=1)
# M2.5: RAYNU-V-M2-TIMER-OK (required when VMXON succeeds / REQUIRE_VMX=1)
# M3.0: RAYNU-V-M3-IO-OK    (required when VMXON succeeds / REQUIRE_VMX=1)
# M3.1: RAYNU-V-M3-CPUID-OK (required when VMXON succeeds / REQUIRE_VMX=1)
# M3.2: RAYNU-V-M3-LOAD-OK  (required when VMXON succeeds / REQUIRE_VMX=1)
# M3.3: RAYNU-V-M3-EARLY-OK (required when VMXON succeeds / REQUIRE_VMX=1)
# M3.4: RAYNU-V-M3-GTIMER-OK (required when VMXON succeeds / REQUIRE_VMX=1)
# M3.5: RAYNU-V-M3-SHELL-OK (required when VMXON succeeds / REQUIRE_VMX=1)
# M3.6: RAYNU-V-M3-LOOP-OK (required when VMXON succeeds / REQUIRE_VMX=1)
# M3.7: RAYNU-V-M3-BZIMAGE-OK (required when VMXON succeeds / REQUIRE_VMX=1)
# M3.8: RAYNU-V-M3-LINUX-EARLY-OK (required for real Linux; proto keeps EARLY..LOOP)
# M3.9: RAYNU-V-M3-GTIMER2-OK (required for real Linux after earlyprintk)
# M3.10: RAYNU-V-M3-SHELL-OK (required for real Linux after GTIMER2)
# M3.11: RAYNU-V-M3-GTIMER3-OK (virtual APIC timer)
# M3.12: RAYNU-V-M3-APIC-OK (IRR/ISR LVT inject; drop IRQ0 crutch)
# M3.13: RAYNU-V-M3-EPT2-OK (precise EPT identity + range claims)
# M3.19: RAYNU-V-M3-NOIRQ-OK (no IRQ4 inject; IRQ0 only until SHELL)
# M3.20: RAYNU-V-M3-EPT3-OK (tight EPT [0,512MiB); QEMU -m 512M)
# M3.22: RAYNU-V-M3-ASSETS-OK (PE .askern/.asinit embed; ESP fallback)
# M4.0: RAYNU-V-M4-2VM-OK (G0 Linux SHELL + G1 private EPT SHELL)
# M4.1: RAYNU-V-M4-SCHED-OK (credit scheduler time-slices G0↔G1)
# M4.2: RAYNU-V-M4-NVM-OK (G0 + G1–G3 ≥4 concurrent under scheduler)
# M4.3: RAYNU-V-M4-BLK-OK (virtio-blk MMIO handshake + host write/readback)
# M4.4: RAYNU-V-M4-NET-OK (virtio-net dual-port + vSwitch exchange)
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MARKER_M0="${MARKER_M0:-RAYNU-V-M0-BOOT-OK}"
MARKER_M10="${MARKER_M10:-RAYNU-V-M1-EBS-OK}"
MARKER_VMXON="${MARKER_VMXON:-RAYNU-V-M1-VMXON-OK}"
MARKER_VMX_SKIP="${MARKER_VMX_SKIP:-RAYNU-V-M1-VMXON-SKIP}"
MARKER_VMEXIT="${MARKER_VMEXIT:-RAYNU-V-M1-VMEXIT-OK}"
MARKER_EPT="${MARKER_EPT:-RAYNU-V-M2-EPT-OK}"
MARKER_GUEST="${MARKER_GUEST:-RAYNU-V-M2-GUEST-OK}"
MARKER_OWN="${MARKER_OWN:-RAYNU-V-M2-OWN-OK}"
MARKER_ALLOC="${MARKER_ALLOC:-RAYNU-V-M2-ALLOC-OK}"
MARKER_IRQ="${MARKER_IRQ:-RAYNU-V-M2-IRQ-OK}"
MARKER_TIMER="${MARKER_TIMER:-RAYNU-V-M2-TIMER-OK}"
MARKER_IO="${MARKER_IO:-RAYNU-V-M3-IO-OK}"
MARKER_CPUID="${MARKER_CPUID:-RAYNU-V-M3-CPUID-OK}"
MARKER_LOAD="${MARKER_LOAD:-RAYNU-V-M3-LOAD-OK}"
MARKER_EARLY="${MARKER_EARLY:-RAYNU-V-M3-EARLY-OK}"
MARKER_GTIMER="${MARKER_GTIMER:-RAYNU-V-M3-GTIMER-OK}"
MARKER_SHELL="${MARKER_SHELL:-RAYNU-V-M3-SHELL-OK}"
MARKER_LOOP="${MARKER_LOOP:-RAYNU-V-M3-LOOP-OK}"
MARKER_BZIMAGE="${MARKER_BZIMAGE:-RAYNU-V-M3-BZIMAGE-OK}"
MARKER_LINUX_EARLY="${MARKER_LINUX_EARLY:-RAYNU-V-M3-LINUX-EARLY-OK}"
MARKER_GTIMER2="${MARKER_GTIMER2:-RAYNU-V-M3-GTIMER2-OK}"
MARKER_GTIMER3="${MARKER_GTIMER3:-RAYNU-V-M3-GTIMER3-OK}"
MARKER_APIC="${MARKER_APIC:-RAYNU-V-M3-APIC-OK}"
MARKER_EPT2="${MARKER_EPT2:-RAYNU-V-M3-EPT2-OK}"
MARKER_EPT3="${MARKER_EPT3:-RAYNU-V-M3-EPT3-OK}"
MARKER_NOIRQ="${MARKER_NOIRQ:-RAYNU-V-M3-NOIRQ-OK}"
MARKER_ASSETS="${MARKER_ASSETS:-RAYNU-V-M3-ASSETS-OK}"
MARKER_2VM="${MARKER_2VM:-RAYNU-V-M4-2VM-OK}"
MARKER_SCHED="${MARKER_SCHED:-RAYNU-V-M4-SCHED-OK}"
MARKER_NVM="${MARKER_NVM:-RAYNU-V-M4-NVM-OK}"
MARKER_BLK="${MARKER_BLK:-RAYNU-V-M4-BLK-OK}"
MARKER_NET="${MARKER_NET:-RAYNU-V-M4-NET-OK}"
TIMEOUT_SECS="${TIMEOUT_SECS:-300}"
SERIAL_LOG="${SERIAL_LOG:-$ROOT/target/m0-serial.log}"
ESP="${ESP:-$ROOT/target/m0-esp}"

kvm_usable() {
  [[ -e /dev/kvm && -r /dev/kvm && -w /dev/kvm ]]
}

# Nested Intel VT-x needs host CPUID.VMX — /dev/kvm alone is not enough
# (GitHub-hosted runners often lack nested virtualization).
host_has_vmx() {
  grep -qw vmx /proc/cpuinfo 2>/dev/null
}

# Auto-require VMX when nested VT-x is actually available unless overridden.
if [[ -z "${REQUIRE_VMX:-}" ]]; then
  if kvm_usable && host_has_vmx && [[ "${QEMU_ACCEL:-auto}" != "tcg" ]]; then
    REQUIRE_VMX=1
  else
    REQUIRE_VMX=0
  fi
fi

mkdir -p "$(dirname "$SERIAL_LOG")" "$ESP/EFI/BOOT"

if [[ "$REQUIRE_VMX" == "1" ]] && [[ -f /sys/module/kvm_intel/parameters/enable_shadow_vmcs ]]; then
  shadow="$(cat /sys/module/kvm_intel/parameters/enable_shadow_vmcs)"
  echo "==> kvm_intel.enable_shadow_vmcs=${shadow}"
  if [[ "$shadow" != "0" && "$shadow" != "N" && "$shadow" != "n" ]]; then
    echo "error: kvm_intel shadow VMCS is ON — nested VMWRITE fails with insn error 12" >&2
    echo "error: fix on this host (quit QEMU first):" >&2
    echo "error:   sudo $ROOT/tools/enable-nested-kvm.sh" >&2
    echo "error: then re-run ./tools/qemu-boot-test.sh" >&2
    exit 1
  fi
fi

echo "==> Building EFI"
"$ROOT/tools/build.sh"

# M3.8: prefer real tinyconfig asset; proto fixture only if nothing present.
if [[ ! -f "$ROOT/assets/bzImage" ]]; then
  if [[ -f "$ROOT/assets/bzImage.real" ]]; then
    cp "$ROOT/assets/bzImage.real" "$ROOT/assets/bzImage"
  else
    echo "==> Generating minimal bzImage asset"
    "$ROOT/tools/gen-minimal-bzimage.sh" "$ROOT/assets/bzImage"
  fi
fi
if [[ ! -f "$ROOT/assets/initrd" ]]; then
  echo "==> Building tiny initrd"
  "$ROOT/tools/build-tiny-initrd.sh" "$ROOT/assets/initrd"
fi

echo "==> Running QEMU boot test (timeout ${TIMEOUT_SECS}s, REQUIRE_VMX=${REQUIRE_VMX})"
rm -f "$SERIAL_LOG"
: >"$SERIAL_LOG"

set +e
timeout --signal=KILL "$TIMEOUT_SECS" \
  env ESP="$ESP" SERIAL_CHARDEV="file:$SERIAL_LOG" \
  "$ROOT/tools/run-qemu.sh" \
  >"$ROOT/target/m0-qemu-stdout.log" 2>"$ROOT/target/m0-qemu-stderr.log"
QEMU_STATUS=$?
set -e

echo "==> QEMU exit status: $QEMU_STATUS"
echo "==> Serial log: $SERIAL_LOG"
if [[ ! -s "$SERIAL_LOG" ]]; then
  echo "error: serial log empty or missing" >&2
  echo "----- qemu stderr -----"
  cat "$ROOT/target/m0-qemu-stderr.log" || true
  echo "----- qemu stdout -----"
  cat "$ROOT/target/m0-qemu-stdout.log" || true
  exit 1
fi

echo "----- serial begin -----"
cat "$SERIAL_LOG" || true
echo "----- serial end -----"

fail=0
for m in "$MARKER_M0" "$MARKER_M10"; do
  if ! grep -qF "$m" "$SERIAL_LOG"; then
    echo "error: marker '$m' not found on serial output" >&2
    fail=1
  fi
done

if grep -qF "$MARKER_ASSETS" "$SERIAL_LOG"; then
  echo "==> M3.22 PE assets marker found"
else
  echo "error: marker '$MARKER_ASSETS' not found (need PE .askern/.asinit embed)" >&2
  fail=1
fi

if grep -qF "$MARKER_VMXON" "$SERIAL_LOG"; then
  echo "==> M1.1 VMXON marker found"
  if grep -qF "$MARKER_VMEXIT" "$SERIAL_LOG"; then
    echo "==> M1.2 VMEXIT marker found"
  else
    echo "error: marker '$MARKER_VMEXIT' not found after successful VMXON" >&2
    fail=1
  fi
  if grep -qF "$MARKER_EPT" "$SERIAL_LOG"; then
    echo "==> M2.0 EPT marker found"
  else
    echo "error: marker '$MARKER_EPT' not found after successful VMXON" >&2
    fail=1
  fi
  if grep -qF "$MARKER_EPT2" "$SERIAL_LOG"; then
    echo "==> M3.13 precise EPT marker found"
  else
    echo "error: marker '$MARKER_EPT2' not found after successful VMXON" >&2
    fail=1
  fi
  if grep -qF "$MARKER_EPT3" "$SERIAL_LOG"; then
    echo "==> M3.20 tight EPT marker found"
  else
    echo "error: marker '$MARKER_EPT3' not found (need EPT [0,512MiB))" >&2
    fail=1
  fi
  if grep -qF "$MARKER_GUEST" "$SERIAL_LOG"; then
    echo "==> M2.1 guest-store marker found"
  else
    echo "error: marker '$MARKER_GUEST' not found after successful VMXON" >&2
    fail=1
  fi
  if grep -qF "$MARKER_OWN" "$SERIAL_LOG"; then
    echo "==> M2.2 ownership marker found"
  else
    echo "error: marker '$MARKER_OWN' not found after successful VMXON" >&2
    fail=1
  fi
  if grep -qF "$MARKER_ALLOC" "$SERIAL_LOG"; then
    echo "==> M2.3 frame-allocator marker found"
  else
    echo "error: marker '$MARKER_ALLOC' not found after successful VMXON" >&2
    fail=1
  fi
  if grep -qF "$MARKER_IRQ" "$SERIAL_LOG"; then
    echo "==> M2.4 IRQ-inject marker found"
  else
    echo "error: marker '$MARKER_IRQ' not found after successful VMXON" >&2
    fail=1
  fi
  if grep -qF "$MARKER_TIMER" "$SERIAL_LOG"; then
    echo "==> M2.5 LAPIC-timer marker found"
  else
    echo "error: marker '$MARKER_TIMER' not found after successful VMXON" >&2
    fail=1
  fi
  if grep -qF "$MARKER_IO" "$SERIAL_LOG"; then
    echo "==> M3.0 guest-I/O marker found"
  else
    echo "error: marker '$MARKER_IO' not found after successful VMXON" >&2
    fail=1
  fi
  if grep -qF "$MARKER_CPUID" "$SERIAL_LOG"; then
    echo "==> M3.1 CPUID-filter marker found"
  else
    echo "error: marker '$MARKER_CPUID' not found after successful VMXON" >&2
    fail=1
  fi
  if grep -qF "$MARKER_LOAD" "$SERIAL_LOG"; then
    echo "==> M3.2 kernel-load marker found"
  else
    echo "error: marker '$MARKER_LOAD' not found after successful VMXON" >&2
    fail=1
  fi
  if grep -qF "$MARKER_BZIMAGE" "$SERIAL_LOG"; then
    echo "==> M3.7 bzImage-load marker found"
  else
    echo "error: marker '$MARKER_BZIMAGE' not found after successful VMXON" >&2
    fail=1
  fi
  # M3.8 real Linux vs proto fixture: mutually exclusive post-entry markers.
  if grep -qF "$MARKER_LINUX_EARLY" "$SERIAL_LOG"; then
    echo "==> M3.8 Linux earlyprintk marker found"
    if grep -qF "$MARKER_GTIMER2" "$SERIAL_LOG"; then
      echo "==> M3.9 Linux GTIMER2 marker found"
    else
      echo "error: marker '$MARKER_GTIMER2' not found after LINUX-EARLY" >&2
      fail=1
    fi
    if grep -qF "$MARKER_SHELL" "$SERIAL_LOG"; then
      echo "==> M3.10 real shell/init marker found"
    else
      echo "error: marker '$MARKER_SHELL' not found after GTIMER2 (need real /init)" >&2
      fail=1
      echo "==> M3.10 diagnostics (e820 / panic / init clues):" >&2
      grep -E "e820_entries=|BIOS-e820|BIOS-e801|alloc_low_pages|Kernel panic|Run /init|No init|Failed to execute|Freeing|initramfs|host-tick=|linux unhandled|waiting for real init|nolapic|lpj=" \
        "$SERIAL_LOG" | head -n 80 >&2 || true
      echo "==> M3.10 last 40 serial lines:" >&2
      tail -n 40 "$SERIAL_LOG" >&2 || true
    fi
    if grep -qF "$MARKER_GTIMER3" "$SERIAL_LOG"; then
      echo "==> M3.11 guest APIC timer marker found"
    else
      echo "error: marker '$MARKER_GTIMER3' not found (need virtual APIC timer)" >&2
      fail=1
    fi
    if grep -qF "$MARKER_APIC" "$SERIAL_LOG"; then
      echo "==> M3.12 faithful APIC inject marker found"
    else
      echo "error: marker '$MARKER_APIC' not found (need IRR/ISR LVT inject)" >&2
      fail=1
    fi
    if grep -qF "$MARKER_NOIRQ" "$SERIAL_LOG"; then
      echo "==> M3.19 NOIRQ marker found (no IRQ4; IRQ0 until SHELL)"
    else
      echo "error: marker '$MARKER_NOIRQ' not found (need no IRQ4 + SHELL)" >&2
      fail=1
    fi
    if grep -qF "$MARKER_2VM" "$SERIAL_LOG"; then
      echo "==> M4.0 two-VM marker found (G0 + G1 SHELL under distinct EPT)"
    else
      echo "error: marker '$MARKER_2VM' not found (need second guest under private EPT)" >&2
      fail=1
    fi
    if grep -qF "$MARKER_SCHED" "$SERIAL_LOG"; then
      echo "==> M4.1 scheduler marker found (G0↔G1 time-slices)"
    else
      echo "error: marker '$MARKER_SCHED' not found (need credit scheduler ≥2 VMs)" >&2
      fail=1
    fi
    if grep -qF "$MARKER_NVM" "$SERIAL_LOG"; then
      echo "==> M4.2 NVM marker found (≥4 concurrent guests under scheduler)"
    else
      echo "error: marker '$MARKER_NVM' not found (need ≥4 guests G0+G1–G3)" >&2
      fail=1
    fi
    if grep -qF "$MARKER_BLK" "$SERIAL_LOG"; then
      echo "==> M4.3 BLK marker found (virtio-blk MMIO + write/readback)"
    else
      echo "error: marker '$MARKER_BLK' not found (need virtio-blk DRIVER_OK readback)" >&2
      fail=1
    fi
    if grep -qF "$MARKER_NET" "$SERIAL_LOG"; then
      echo "==> M4.4 NET marker found (virtio-net dual-port vSwitch exchange)"
    else
      echo "error: marker '$MARKER_NET' not found (need virtio-net port0→port1 exchange)" >&2
      fail=1
    fi
    echo "==> real Linux path — skipping synthetic EARLY/GTIMER/LOOP checks"
  else
    if grep -qF "$MARKER_EARLY" "$SERIAL_LOG"; then
      echo "==> M3.3 earlyprintk marker found (proto)"
    else
      echo "error: neither '$MARKER_LINUX_EARLY' nor '$MARKER_EARLY' found" >&2
      fail=1
    fi
    if grep -qF "$MARKER_GTIMER" "$SERIAL_LOG"; then
      echo "==> M3.4 guest-timer marker found"
    else
      echo "error: marker '$MARKER_GTIMER' not found after successful VMXON" >&2
      fail=1
    fi
    if grep -qF "$MARKER_SHELL" "$SERIAL_LOG"; then
      echo "==> M3.5 shell/init marker found"
    else
      echo "error: marker '$MARKER_SHELL' not found after successful VMXON" >&2
      fail=1
    fi
    if grep -qF "$MARKER_LOOP" "$SERIAL_LOG"; then
      echo "==> M3.6 exit-loop marker found"
    else
      echo "error: marker '$MARKER_LOOP' not found after successful VMXON" >&2
      fail=1
    fi
  fi
elif grep -qF "$MARKER_VMX_SKIP" "$SERIAL_LOG"; then
  if [[ "$REQUIRE_VMX" == "1" ]]; then
    echo "error: VMXON skipped but REQUIRE_VMX=1 (need nested KVM / VT-x)" >&2
    fail=1
  else
    echo "==> M1.1 VMXON skipped (no guest VMX; OK without REQUIRE_VMX)"
  fi
else
  echo "error: neither $MARKER_VMXON nor $MARKER_VMX_SKIP found" >&2
  fail=1
fi

if [[ "$fail" -ne 0 ]]; then
  echo "----- qemu stderr -----"
  cat "$ROOT/target/m0-qemu-stderr.log" || true
  exit 1
fi

echo "==> Boot gate PASSED (M0 → M4.4; qemu status=$QEMU_STATUS)"
exit 0
