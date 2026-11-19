#!/usr/bin/env bash
# Plan section 6, stage 1, rows 1–5 (docs/plans/b21_plan.md).
# The five WP0 baselines, then the same five with HOST_SMP=4, then the
# threads row, the ONECORE row, and row 2's product run with cpu-pm=on.
#
# A GitHub-hosted runner cannot run every row. This script prints
# `cannot-run` and does not start QEMU for those. It exits 0 only when
# every row that it was able to start also showed its lines. A cannot-run
# row keeps the exit non-zero, so a green dispatch means rows 1–5 all ran.
#
# Cannot run on ubuntu-latest (4 CPUs, 16 GB, no product ISO in the tree):
# - Any HOST_SMP whose CPU count is greater than nproc-1. The plan's WP0
#   note: the ticket lock waits forever if the outer host deschedules the
#   core whose turn it is. HOST_SMP=4 needs nproc >= 5. That is rows 2, 3,
#   4 and 5 on a 4-CPU runner.
# - A product row whose stop line is typed at the guest shell
#   (`localhost:~# cat /proc/cmdline`, or a line that is only `2`). An
#   unattended file serial does not contain them. That is base-prod and
#   base-smp2 inside rows 1 and 2, and row 5.
# - A product row when target/alpine-extended-3.21.3-x86_64.iso is absent.
#   The file is not in the repo.
# - QEMU_MEM=8192M when MemAvailable is under 10 GiB.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
ISO="${ISO:-$ROOT/target/alpine-extended-3.21.3-x86_64.iso}"
NPROC="$(nproc)"
FAIL=0

if ! grep -qw vmx /proc/cpuinfo 2>/dev/null || [[ ! -w /dev/kvm ]]; then
  echo "cannot-run all: nested VT-x is not usable on this machine" >&2
  exit 1
fi
if [[ -f /sys/module/kvm_intel/parameters/enable_shadow_vmcs ]]; then
  shadow="$(cat /sys/module/kvm_intel/parameters/enable_shadow_vmcs)"
  echo "==> kvm_intel.enable_shadow_vmcs=${shadow}"
  if [[ "$shadow" != "0" && "$shadow" != "N" && "$shadow" != "n" ]]; then
    echo "cannot-run all: shadow VMCS is on. Reload kvm_intel with enable_shadow_vmcs=0 (tools/enable-nested-kvm.sh). A built-in kvm_intel cannot." >&2
    exit 1
  fi
fi

mem_kib() {
  awk '/MemAvailable/ { print $2; exit }' /proc/meminfo
}

# CPU count encoded by a HOST_SMP value. Empty means QEMU's default of 1.
smp_cpus() {
  local s="${1:-}"
  if [[ -z "$s" ]]; then
    echo 1
    return
  fi
  if [[ "$s" =~ ^[0-9]+ ]]; then
    echo "${BASH_REMATCH[0]}"
    return
  fi
  echo 1
}

# Plan WP0: HOST_SMP <= nproc-1.
smp_ok() {
  local cpus
  cpus="$(smp_cpus "$1")"
  (( NPROC >= cpus + 1 ))
}

run_boot() {
  local name="$1" secs="$2" mode="$3" pat="$4" cnt="$5"
  shift 5
  local -a need=("$@")
  local log="$ROOT/target/b21-smp-${name}.log"
  local err="$ROOT/target/b21-smp-${name}.err"
  rm -rf "/tmp/b21-esp-${name}"
  rm -f "$log" "$err"
  : >"$log"
  echo "==> start ${name} (${secs}s)"
  set +e
  if [[ "$mode" == lab ]]; then
    setsid timeout --signal=KILL "$secs" \
      env RAYNU_F=1 QEMU_ACCEL=kvm ESP="/tmp/b21-esp-${name}" \
      SERIAL_CHARDEV="file:${log}" \
      ./tools/run-qemu.sh >"$ROOT/target/b21-smp-${name}.out" 2>"$err" &
  else
    setsid timeout --signal=KILL "$secs" \
      env PRODUCT_ISO="$ISO" RAYNU_F=1 QEMU_ACCEL=kvm \
      ESP="/tmp/b21-esp-${name}" SERIAL_CHARDEV="file:${log}" \
      ./tools/run-qemu.sh >"$ROOT/target/b21-smp-${name}.out" 2>"$err" &
  fi
  local wrap=$!
  local found=0
  while kill -0 "$wrap" 2>/dev/null; do
    local n
    n=$(grep -c -E "$pat" "$log" 2>/dev/null || true)
    local ok=1
    if (( n < cnt )); then
      ok=0
    fi
    local line
    for line in "${need[@]}"; do
      if [[ -n "$line" ]] && ! grep -qF "$line" "$log" 2>/dev/null; then
        ok=0
      fi
    done
    if [[ "$ok" -eq 1 ]]; then
      found=1
      kill -TERM -- "-${wrap}" 2>/dev/null || kill -TERM "$wrap" 2>/dev/null || true
      sleep 1
      kill -KILL -- "-${wrap}" 2>/dev/null || kill -KILL "$wrap" 2>/dev/null || true
      break
    fi
    sleep 2
  done
  wait "$wrap" || true
  set -e
  if [[ "$found" -ne 1 ]]; then
    echo "fail ${name}: pattern or required line missing" >&2
    tail -n 20 "$log" >&2 || true
    return 1
  fi
  echo "pass ${name}"
}

# kind: lab | product | shell | smp4 | ram
# shell rows are typed at the guest and are not started.
consider() {
  local name="$1" why="$2"
  echo "cannot-run ${name}: ${why}"
  FAIL=1
}

row_env_ok() {
  local smp="$1" mem="$2" mode="$3" shell="$4" name="$5"
  if [[ "$shell" == 1 ]]; then
    consider "$name" "stop line is typed at the guest shell; an unattended runner serial does not contain it (plan section 6)"
    return 1
  fi
  if ! smp_ok "$smp"; then
    consider "$name" "HOST_SMP=$(smp_cpus "$smp") needs nproc >= $(( $(smp_cpus "$smp") + 1 )); this runner has ${NPROC} (plan WP0 nested note)"
    return 1
  fi
  if [[ "$mode" == product && ! -f "$ISO" ]]; then
    consider "$name" "product ISO is not on the runner ($ISO is not in the repo)"
    return 1
  fi
  if [[ "$mem" == 8192M ]]; then
    local kib
    kib="$(mem_kib)"
    if [[ -z "$kib" || "$kib" -lt $((10 * 1024 * 1024)) ]]; then
      consider "$name" "QEMU_MEM=8192M needs 10 GiB MemAvailable"
      return 1
    fi
  fi
  return 0
}

one() {
  local name="$1" secs="$2" mode="$3" pat="$4" cnt="$5" smp="$6" mem="$7" shell="$8"
  shift 8
  if ! row_env_ok "$smp" "$mem" "$mode" "$shell" "$name"; then
    return 0
  fi
  if [[ -n "$smp" ]]; then
    export HOST_SMP="$smp"
  else
    unset HOST_SMP
  fi
  export QEMU_MEM="${mem:-512M}"
  if ! run_boot "$name" "$secs" "$mode" "$pat" "$cnt" "$@"; then
    FAIL=1
  fi
  unset HOST_SMP ONECORE TWO_GUESTS VCPUS
}

echo "==> nproc=${NPROC}"

# Row 1. Five WP0 baselines, HOST_SMP unset. online=1 of 1.
one row1-lab 600 lab "RAYNU-V-RAYNU-F-DISK-BOOT-OK|RayNu-F stop" 3 "" 512M 0 \
  "boot: B21 host cores online=1 of 1"
one row1-prod 600 product "localhost:~# cat /proc/cmdline" 1 "" 4096M 1
export TWO_GUESTS=1
one row1-two 900 product "guest\\(s\\) still on|is started again" 2 "" 8192M 0 \
  "boot: B21 host cores online=1 of 1"
unset TWO_GUESTS
export VCPUS=2
one row1-smp2 900 product "^2\\r?$" 2 "" 4096M 1
unset VCPUS
export TWO_GUESTS=1 VCPUS=2
one row1-two-smp 1200 product "guest\\(s\\) still on|is started again" 2 "" 8192M 0 \
  "boot: B21 host cores online=1 of 1"
unset TWO_GUESTS VCPUS

# Row 2. The same five with HOST_SMP=4.
one row2-lab 600 lab "RAYNU-V-RAYNU-F-DISK-BOOT-OK|RayNu-F stop" 3 4 512M 0 \
  "boot: B21 host cores online=4 of 4" \
  "boot: B21 host core 1 " \
  "boot: B21 host core 2 " \
  "boot: B21 host core 3 "
one row2-prod 600 product "localhost:~# cat /proc/cmdline" 1 4 4096M 1
export TWO_GUESTS=1
one row2-two 900 product "guest\\(s\\) still on|is started again" 2 4 8192M 0 \
  "boot: B21 host cores online=4 of 4"
unset TWO_GUESTS
export VCPUS=2
one row2-smp2 900 product "^2\\r?$" 2 4 4096M 1
unset VCPUS
export TWO_GUESTS=1 VCPUS=2
one row2-two-smp 1200 product "guest\\(s\\) still on|is started again" 2 4 8192M 0 \
  "boot: B21 host cores online=4 of 4"
unset TWO_GUESTS VCPUS

# Row 3. Threads. Product. threads_skipped=2 and online=2.
one row3-threads 600 product "threads_skipped=2" 1 "4,sockets=1,cores=2,threads=2" 4096M 0 \
  " threads_skipped=2" \
  "boot: B21 host cores online=2 of 2"

# Row 4. ONECORE=1 HOST_SMP=4 VCPUS=2 product. online=1.
export ONECORE=1 VCPUS=2
one row4-onecore 900 product "boot: B21 host cores online=1 of 1" 1 4 4096M 0 \
  "boot: B21 host cores stay 1: onecore.txt" \
  "boot: B21 host cores online=1 of 1"
unset ONECORE VCPUS

# Row 5. Row 2's product run with -overcommit cpu-pm=on.
# The extra QEMU argument is row 2's base-prod, which is a typed shell line,
# and it also needs HOST_SMP=4. Both keep it off a 4-CPU runner.
if ! smp_ok 4; then
  consider row5-cpupm "HOST_SMP=4 needs nproc >= 5; this runner has ${NPROC}. Also the stop line is typed (plan section 6 row 5)"
else
  consider row5-cpupm "stop line is the typed base-prod shell line (plan section 6 row 5); cpu-pm is -overcommit cpu-pm=on on that same run"
fi

if [[ "$FAIL" -ne 0 ]]; then
  echo "stage 1 incomplete: a row failed or cannot run on this runner"
  exit 1
fi
echo "stage 1 rows 1-5 passed"
