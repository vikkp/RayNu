#!/usr/bin/env bash
# Section 6 stage 1, rows 1–5. The plan text is not in the tree. These are
# the five HOST_SMP checks recorded for nested `0b5df509` in docs/hda.md
# (2026-11-18, wave-7-b21-cores). Stage 2 is not in this script. It is added
# as WP6–WP9 land.
#
# Requires nested VT-x with shadow VMCS off. GitHub Actions runs
# tools/enable-nested-kvm.sh first. This VM cannot: kvm_intel is built in
# and enable_shadow_vmcs is Y and read-only.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if ! grep -qw vmx /proc/cpuinfo 2>/dev/null; then
  echo "error: host CPUID has no vmx; this job needs a runner that can reload kvm_intel" >&2
  exit 1
fi
if [[ ! -w /dev/kvm ]]; then
  echo "error: /dev/kvm is not writable" >&2
  exit 1
fi
if [[ -f /sys/module/kvm_intel/parameters/enable_shadow_vmcs ]]; then
  shadow="$(cat /sys/module/kvm_intel/parameters/enable_shadow_vmcs)"
  echo "==> kvm_intel.enable_shadow_vmcs=${shadow}"
  if [[ "$shadow" != "0" && "$shadow" != "N" && "$shadow" != "n" ]]; then
    echo "error: shadow VMCS is on. Run sudo ./tools/enable-nested-kvm.sh on a runner where kvm_intel is a module." >&2
    exit 1
  fi
fi

mkdir -p target
if [[ ! -f target/x86_64-unknown-uefi/release/r640-hypervisor.efi ]]; then
  echo "==> building EFI"
  ./tools/build.sh
fi

run_row() {
  local name="$1" secs="$2"
  shift 2
  local log="$ROOT/target/b21-smp-${name}.log"
  local out="$ROOT/target/b21-smp-${name}.stdout"
  local err="$ROOT/target/b21-smp-${name}.stderr"
  rm -f "$log" "$out" "$err"
  : >"$log"
  echo "==> row ${name} (${secs}s)"
  set +e
  # Own process group so the timeout and QEMU die together. Not pkill -f.
  setsid timeout --signal=KILL "$secs" \
    env SERIAL_CHARDEV="file:${log}" QEMU_ACCEL=kvm \
    "$ROOT/tools/run-qemu.sh" >"$out" 2>"$err" &
  local wrap=$!
  local found=0
  while kill -0 "$wrap" 2>/dev/null; do
    local ok=1
    local pat
    for pat in "$@"; do
      if ! grep -qF "$pat" "$log" 2>/dev/null; then
        ok=0
        break
      fi
    done
    if [[ "$ok" -eq 1 ]]; then
      found=1
      kill -TERM -- "-${wrap}" 2>/dev/null || kill -TERM "$wrap" 2>/dev/null || true
      sleep 1
      kill -KILL -- "-${wrap}" 2>/dev/null || kill -KILL "$wrap" 2>/dev/null || true
      break
    fi
    sleep 1
  done
  wait "$wrap" || true
  set -e
  if [[ "$found" -ne 1 ]]; then
    echo "error: row ${name} did not print every expected line" >&2
    echo "----- expected -----" >&2
    printf '  %s\n' "$@" >&2
    echo "----- serial tail -----" >&2
    tail -n 40 "$log" >&2 || true
    echo "----- qemu stderr tail -----" >&2
    tail -n 20 "$err" >&2 || true
    exit 1
  fi
  echo "==> row ${name} ok"
}

# 1. Three host CPUs, all started.
HOST_SMP=3 ONECORE=0 run_row 1 180 \
  "boot: B21 host core 1 " \
  "boot: B21 host core 2 " \
  " online (VMXON ok)" \
  "boot: B21 host cores online=3 of 3"

# 2. QEMU's default one CPU.
env -u HOST_SMP ONECORE=0 run_row 2 180 \
  "boot: B21 host cores online=1 of 1"

# 3. Cores are offered and the flag keeps one.
HOST_SMP=3 ONECORE=1 run_row 3 180 \
  "boot: B21 host cores stay 1: onecore.txt"

# 4. Two threads per core. One thread is started.
HOST_SMP="sockets=1,cores=2,threads=2" ONECORE=0 run_row 4 180 \
  " threads_skipped=2" \
  "boot: B21 host cores online=2 of 2"

# 5. The lab path still reaches the boot marker. No HOST_SMP.
env -u HOST_SMP ONECORE=0 run_row 5 480 \
  "RAYNU-V-R640-BOOT-OK"

echo "==> stage 1 rows 1-5 ok"
