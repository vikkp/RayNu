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
TIMEOUT_SECS="${TIMEOUT_SECS:-60}"
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

# M3.7 fixture on ESP (run-qemu.sh also stages; ensure asset exists first).
if [[ ! -f "$ROOT/assets/bzImage" ]]; then
  echo "==> Generating minimal bzImage asset"
  "$ROOT/tools/gen-minimal-bzimage.sh" "$ROOT/assets/bzImage"
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
  if grep -qF "$MARKER_EARLY" "$SERIAL_LOG"; then
    echo "==> M3.3 earlyprintk marker found"
  else
    echo "error: marker '$MARKER_EARLY' not found after successful VMXON" >&2
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

echo "==> Boot gate PASSED (M0 → M3.7; qemu status=$QEMU_STATUS)"
exit 0
