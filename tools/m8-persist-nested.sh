#!/usr/bin/env bash
# M8.0 nested persist: install → kill/restart the *hypervisor* process →
# second Linux without setup-disk. Mechanism proof, not iron.
#
# The virtio HPA is QEMU initial RAM backed by `M8_PERSIST_IMG` (share=on).
# Distro OVMF_CODE_4M ignores nvdimm and pc-dimm hotplug. Guest F7 is not
# this test. Nested QEMU ≠ R640. Host/CI cargo tests must never print
# RAYNU-V-M8-DISK-PERSIST-NESTED-OK or RAYNU-V-M7-ISO-INSTALL-OK.
# Iron COM2 marker RAYNU-V-M8-DISK-PERSIST-OK is forbidden here.
#
# file-RAM backend: QEMU initial RAM is M8_PERSIST_IMG (share=on).
# MODE=smoke  one boot, persist reserve only. TCG ok. Not nested-OK.
# MODE=keep   two boots, TCG ok. Boot 1 persist reserve, plant GPT+ESP+ext4
#             into M8_PERSIST_IMG, kill HV, boot 2 keep=1. Not Alpine.
#             Does not print nested-OK.
# MODE=lun    one boot, QEMU NVMe Identify + I/O ready. TCG ok. Not nested-OK.
#             Not iron.
# MODE=usb    one boot, QEMU qemu-xhci + usb-storage BOT I/O ready. TCG ok.
#             Not nested-OK. Not iron. Do not F11.
# MODE=full   two boots (default). Boot 1 stops after Alpine install.
#             Needs nested KVM (VMLAUNCH). TCG is smoke/keep only.
#
# Usage:
#   MODE=smoke ./tools/m8-persist-nested.sh
#   MODE=keep ./tools/m8-persist-nested.sh
#   MODE=lun ./tools/m8-persist-nested.sh
#   MODE=usb ./tools/m8-persist-nested.sh
#   ./tools/m8-persist-nested.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MODE="${MODE:-full}"
TIMEOUT_BOOT1="${TIMEOUT_BOOT1:-1800}"
TIMEOUT_BOOT2="${TIMEOUT_BOOT2:-900}"
TIMEOUT_SMOKE="${TIMEOUT_SMOKE:-180}"
WAIT_QEMU_START="${WAIT_QEMU_START:-300}"
SERIAL1="${SERIAL1:-$ROOT/target/m8-persist-boot1.log}"
SERIAL2="${SERIAL2:-$ROOT/target/m8-persist-boot2.log}"
ESP="${ESP:-$ROOT/target/m8-persist-esp}"
ALPINE_FLAVOR="${ALPINE_FLAVOR:-extended}"
ISO_URL="${ALPINE_ISO_URL:-https://dl-cdn.alpinelinux.org/alpine/v3.21/releases/x86_64/alpine-${ALPINE_FLAVOR}-3.21.3-x86_64.iso}"
ALPINE_ISO="${ALPINE_ISO:-$ROOT/target/alpine-${ALPINE_FLAVOR}-3.21.3-x86_64.iso}"
SMOKE_ISO="${SMOKE_ISO:-$ROOT/target/m8-smoke-window.iso}"
M8_PERSIST_IMG="${M8_PERSIST_IMG:-$ROOT/target/m8-persist.img}"
M8_PERSIST_SIZE="${M8_PERSIST_SIZE:-2560M}"
# 2560M so leftover above PRECISE exists; the file *is* that RAM.
QEMU_MEM="${QEMU_MEM:-2560M}"
RAYNU_F="${RAYNU_F:-1}"
NESTED_OK="RAYNU-V-M8-DISK-PERSIST-NESTED-OK"
IRON_OK="RAYNU-V-M8-DISK-PERSIST-OK"
ISO_OK="RAYNU-V-M7-ISO-INSTALL-OK"

if [[ "$MODE" == "smoke" || "$MODE" == "keep" || "$MODE" == "lun" || "$MODE" == "usb" ]] && [[ -z "${PRODUCT_ISO:-}" ]]; then
  ISO_PATH="$SMOKE_ISO"
else
  ISO_PATH="${PRODUCT_ISO:-$ALPINE_ISO}"
fi

mkdir -p "$(dirname "$SERIAL1")" "$ESP" "$(dirname "$ISO_PATH")" "$(dirname "$M8_PERSIST_IMG")"

forbid_markers() {
  local log="$1"
  if grep -qF "$ISO_OK" "$log"; then
    echo "error: nested printed iron $ISO_OK" >&2
    exit 1
  fi
  if grep -qF "$IRON_OK" "$log"; then
    echo "error: nested printed iron $IRON_OK" >&2
    exit 1
  fi
}

require_persist_reserved() {
  local log="$1"
  if ! grep -qF 'persist install disk hpa=' "$log"; then
    echo "error: no Stage 46 persist install disk (file-RAM not leftover?)" >&2
    grep -E 'leftover install disk|persist install disk|report-RAM extra|pc-dimm|hypervisor' "$log" | head -n 20 >&2 || true
    exit 1
  fi
  if ! grep -qF 'leftover install disk skip persist' "$log"; then
    echo "error: leftover disk carve was not skipped for persist" >&2
    exit 1
  fi
}

scan() {
  local log="$1"
  echo "==> marker scan $log (not ISO-INSTALL-OK):"
  grep -E -n 'persist install disk|leftover install disk|virtio-blk install disk|VMLAUNCH-OK|VMXON-SKIP|Linux version|setup-disk|Installation is complete|GPT ESP|root=UUID|DISK-BOOT-OK|ISO-INSTALL-OK|report-RAM extra' \
    "$log" | head -n 80 || true
}

ensure_smoke_iso() {
  if [[ -f "$ISO_PATH" ]]; then
    return 0
  fi
  echo "==> window-sized smoke ISO $ISO_PATH (not alpine; not ISO-INSTALL-OK)"
  dd if=/dev/zero of="$ISO_PATH" bs=1024 count=80 status=none
}

fetch_iso() {
  if [[ "$ISO_PATH" == "$SMOKE_ISO" ]]; then
    ensure_smoke_iso
  elif [[ ! -f "$ISO_PATH" ]]; then
    echo "==> fetching alpine-${ALPINE_FLAVOR} ISO to $ISO_PATH"
    curl -fsSL -o "${ISO_PATH}.part" "$ISO_URL"
    mv "${ISO_PATH}.part" "$ISO_PATH"
  fi
  local psz
  psz=$(wc -c <"$ISO_PATH" | tr -d ' ')
  if (( psz <= 73728 )); then
    echo "error: PRODUCT_ISO is lab-stub sized ($psz)" >&2
    exit 1
  fi
  echo "==> ISO $ISO_PATH ($psz bytes) (not ISO-INSTALL-OK)"
}

kvm_wedged() {
  local log
  log="$(dmesg 2>/dev/null || true)"
  if [[ -z "$log" ]]; then
    log="$(sudo dmesg 2>/dev/null || true)"
  fi
  [[ "$log" == *kvm_spurious_fault* ]]
}

kvm_usable() {
  [[ -e /dev/kvm && -r /dev/kvm && -w /dev/kvm ]]
}

pick_accel() {
  if [[ -n "${QEMU_ACCEL:-}" && "${QEMU_ACCEL}" != "auto" ]]; then
    echo "==> QEMU_ACCEL=$QEMU_ACCEL (user)"
    return 0
  fi
  if [[ "$MODE" == "full" ]]; then
    if ! kvm_usable; then
      echo "error: MODE=full needs nested KVM (VMLAUNCH); /dev/kvm not usable" >&2
      exit 1
    fi
    if kvm_wedged; then
      echo "error: MODE=full needs nested KVM; host kvm_spurious_fault (builtin kvm_intel, cannot reload)" >&2
      exit 1
    fi
    QEMU_ACCEL=kvm
  elif kvm_usable && ! kvm_wedged; then
    QEMU_ACCEL=kvm
  else
    QEMU_ACCEL=tcg
    echo "==> accel tcg (kvm missing or kvm_spurious_fault; persist scan is PRE-VMLAUNCH)"
  fi
  echo "==> QEMU_ACCEL=$QEMU_ACCEL"
}

prepare_host() {
  if [[ -e /dev/kvm ]]; then
    sudo chmod a+rw /dev/kvm || true
  fi
  if [[ "${QEMU_ACCEL}" == "kvm" && -x "$ROOT/tools/enable-nested-kvm.sh" ]]; then
    # Builtin kvm_intel cannot unload; do not treat as fatal for smoke.
    sudo "$ROOT/tools/enable-nested-kvm.sh" || true
  fi
  echo "==> host virt flags: $(grep -m1 '^flags' /proc/cpuinfo | grep -oE 'vmx|svm' | tr '\n' ' ' || true)"
}

build_efi() {
  if [[ "${REBUILD_EFI:-1}" == "1" ]]; then
    echo "==> building EFI (REBUILD_EFI=0 to skip)"
    cargo build --release --features uefi-bin --target x86_64-unknown-uefi \
      >"$ROOT/target/m8-persist-build.log" 2>&1 || {
      echo "error: EFI build failed; see target/m8-persist-build.log" >&2
      tail -n 40 "$ROOT/target/m8-persist-build.log" >&2 || true
      exit 1
    }
  fi
}

reset_persist_img() {
  rm -f "$M8_PERSIST_IMG"
  truncate -s "$M8_PERSIST_SIZE" "$M8_PERSIST_IMG"
  echo "==> persist img $M8_PERSIST_IMG ($M8_PERSIST_SIZE, empty) (not ISO-INSTALL-OK)"
}

# Linux comm is 15 chars (`qemu-system-x86`). Walk timeout children.
# Does not use pkill -f.
find_qemu_pid() {
  local tpid="$1"
  local pid g comm
  while read -r pid; do
    [[ -z "$pid" ]] && continue
    comm=$(ps -p "$pid" -o comm= 2>/dev/null || true)
    if [[ "$comm" == qemu-system* ]]; then
      echo "$pid"
      return 0
    fi
    while read -r g; do
      [[ -z "$g" ]] && continue
      comm=$(ps -p "$g" -o comm= 2>/dev/null || true)
      if [[ "$comm" == qemu-system* ]]; then
        echo "$g"
        return 0
      fi
    done < <(pgrep -P "$pid" || true)
  done < <(pgrep -P "$tpid" || true)
  pgrep -u "$(id -u)" qemu-system | head -n 1 || true
}

start_qemu() {
  local serial="$1"
  local timeout_secs="$2"
  local stdout="$3"
  local stderr="$4"
  rm -f "$serial"
  : >"$serial"
  timeout --signal=KILL "$timeout_secs" \
    env PRODUCT_ISO="$ISO_PATH" ESP="$ESP" SERIAL_CHARDEV="file:$serial" \
    QEMU_ACCEL="${QEMU_ACCEL}" RAYNU_F="$RAYNU_F" QEMU_MEM="$QEMU_MEM" \
    M8_PERSIST_IMG="$M8_PERSIST_IMG" M8_PERSIST_SIZE="$M8_PERSIST_SIZE" \
    M8_NVME_IMG="${M8_NVME_IMG:-}" M8_USB_IMG="${M8_USB_IMG:-}" \
    REBUILD_EFI=0 \
    "$ROOT/tools/run-qemu.sh" \
    >"$stdout" 2>"$stderr" &
  local tpid=$!
  echo "$tpid" >"$ROOT/target/m8-persist-timeout.pid"
  local qpid=""
  local i
  for i in $(seq 1 "$WAIT_QEMU_START"); do
    sleep 1
    qpid=$(find_qemu_pid "$tpid")
    if [[ -n "$qpid" ]]; then
      echo "$qpid" >"$ROOT/target/m8-persist-qemu.pid"
      echo "==> qemu pid=$qpid timeout pid=$tpid serial=$serial"
      return 0
    fi
    if ! kill -0 "$tpid" 2>/dev/null; then
      echo "error: QEMU wrapper exited before qemu-system" >&2
      cat "$stderr" >&2 || true
      exit 1
    fi
  done
  echo "error: qemu-system did not start within ${WAIT_QEMU_START}s" >&2
  cat "$stderr" >&2 || true
  exit 1
}

stop_qemu() {
  local qpid tpid
  qpid=$(cat "$ROOT/target/m8-persist-qemu.pid" 2>/dev/null || true)
  tpid=$(cat "$ROOT/target/m8-persist-timeout.pid" 2>/dev/null || true)
  if [[ -n "$qpid" ]] && kill -0 "$qpid" 2>/dev/null; then
    kill "$qpid" 2>/dev/null || true
    sleep 2
    if kill -0 "$qpid" 2>/dev/null; then
      kill -KILL "$qpid" 2>/dev/null || true
    fi
  fi
  if [[ -n "$tpid" ]]; then
    kill "$tpid" 2>/dev/null || true
    wait "$tpid" 2>/dev/null || true
  fi
  rm -f "$ROOT/target/m8-persist-qemu.pid" "$ROOT/target/m8-persist-timeout.pid"
}

wait_qemu() {
  local tpid
  tpid=$(cat "$ROOT/target/m8-persist-timeout.pid" 2>/dev/null || true)
  if [[ -n "$tpid" ]]; then
    wait "$tpid" 2>/dev/null || true
  fi
  rm -f "$ROOT/target/m8-persist-qemu.pid" "$ROOT/target/m8-persist-timeout.pid"
}

wait_serial_needle() {
  local serial="$1"
  local needle="$2"
  local tpid
  tpid=$(cat "$ROOT/target/m8-persist-timeout.pid")
  while kill -0 "$tpid" 2>/dev/null; do
    if grep -qF "$needle" "$serial" 2>/dev/null; then
      return 0
    fi
    sleep 2
  done
  return 1
}

parse_persist_hpa() {
  local log="$1"
  grep -oE 'persist install disk hpa=0x[0-9a-fA-F]+' "$log" \
    | tail -n1 \
    | grep -oE '0x[0-9a-fA-F]+'
}

plant_keep_fixture() {
  local log="$1"
  local hpa
  hpa=$(parse_persist_hpa "$log")
  if [[ -z "$hpa" ]]; then
    echo "error: no persist hpa in $log (not ISO-INSTALL-OK)" >&2
    exit 1
  fi
  echo "==> plant GPT+ESP+ext4 at $hpa in $M8_PERSIST_IMG (not nested-OK; not iron; not ISO-INSTALL-OK)"
  local plant_log="$ROOT/target/m8-persist-plant.log"
  # Do not use --exact: the test lives under mgmt::disk_persist::disk_persist_test::
  if ! M8_PLANT_PATH="$M8_PERSIST_IMG" M8_PLANT_OFFSET="$hpa" \
    cargo test --no-default-features plant_m8_persist_fixture -- --nocapture \
    >"$plant_log" 2>&1; then
    echo "error: plant_m8_persist_fixture failed" >&2
    tail -n 40 "$plant_log" >&2 || true
    exit 1
  fi
  if ! grep -qE 'test result: ok\. 1 passed' "$plant_log"; then
    echo "error: plant_m8_persist_fixture did not run (0 tests?)" >&2
    tail -n 20 "$plant_log" >&2 || true
    exit 1
  fi
  python3 - "$M8_PERSIST_IMG" "$hpa" <<'PY'
import sys
from pathlib import Path
path, hpa_s = Path(sys.argv[1]), sys.argv[2]
off = int(hpa_s, 16) if hpa_s.lower().startswith("0x") else int(hpa_s)
with path.open("rb") as f:
    f.seek(off + 512)
    sig = f.read(8)
if sig != b"EFI PART":
    raise SystemExit(f"error: no EFI PART at {hpa_s}+512 (got {sig!r})")
print(f"==> plant GPT EFI PART at {hpa_s} (not ISO-INSTALL-OK)")
PY
}

run_smoke() {
  echo "==> MODE=smoke — one boot, persist reserve only (not NESTED-OK, not iron)"
  reset_persist_img
  start_qemu "$SERIAL1" "$TIMEOUT_SMOKE" \
    "$ROOT/target/m8-persist-smoke-stdout.log" \
    "$ROOT/target/m8-persist-smoke-stderr.log"
  # KVM_CREATE_VCPU can hang after kvm_spurious_fault with empty serial.
  local waited=0
  while (( waited < 25 )); do
    if [[ -s "$SERIAL1" ]]; then
      break
    fi
    sleep 1
    waited=$((waited + 1))
  done
  if [[ ! -s "$SERIAL1" && "$QEMU_ACCEL" == "kvm" ]]; then
    echo "==> kvm serial empty after ${waited}s; retry tcg (persist scan is PRE-VMLAUNCH)"
    stop_qemu
    QEMU_ACCEL=tcg
    reset_persist_img
    start_qemu "$SERIAL1" "$TIMEOUT_SMOKE" \
      "$ROOT/target/m8-persist-smoke-stdout.log" \
      "$ROOT/target/m8-persist-smoke-stderr.log"
  fi
  if wait_serial_needle "$SERIAL1" "persist install disk hpa="; then
    sleep 2
    stop_qemu
  else
    wait_qemu || true
  fi
  if [[ ! -s "$SERIAL1" ]]; then
    echo "error: smoke serial empty" >&2
    cat "$ROOT/target/m8-persist-smoke-stderr.log" >&2 || true
    exit 1
  fi
  scan "$SERIAL1"
  forbid_markers "$SERIAL1"
  require_persist_reserved "$SERIAL1"
  echo "==> nested File RAM persist reserved (not $NESTED_OK; not iron $IRON_OK; not $ISO_OK)"
}

run_full() {
  echo "==> MODE=full — install, kill HV, second boot keep=1 (not iron)"
  reset_persist_img
  start_qemu "$SERIAL1" "$TIMEOUT_BOOT1" \
    "$ROOT/target/m8-persist-boot1-stdout.log" \
    "$ROOT/target/m8-persist-boot1-stderr.log"
  local tpid
  tpid=$(cat "$ROOT/target/m8-persist-timeout.pid")
  local saw_install=0
  while kill -0 "$tpid" 2>/dev/null; do
    if grep -qF 'Installation is complete. Please reboot.' "$SERIAL1" 2>/dev/null; then
      saw_install=1
      echo "==> boot1 install complete; flushing persist file then killing HV (not guest F7)"
      sleep 8
      stop_qemu
      break
    fi
    sleep 5
  done
  if [[ "$saw_install" != "1" ]]; then
    wait_qemu || true
    scan "$SERIAL1"
    forbid_markers "$SERIAL1"
    echo "error: boot1 did not reach Alpine Installation is complete" >&2
    exit 1
  fi
  scan "$SERIAL1"
  forbid_markers "$SERIAL1"
  require_persist_reserved "$SERIAL1"
  if grep -qF 'RAYNU-V-M1-VMXON-SKIP' "$SERIAL1"; then
    echo "error: VMXON-SKIP on boot1; nested VT-x did not run" >&2
    exit 1
  fi

  echo "==> boot2: same $M8_PERSIST_IMG ($(stat -c%s "$M8_PERSIST_IMG") bytes)"
  start_qemu "$SERIAL2" "$TIMEOUT_BOOT2" \
    "$ROOT/target/m8-persist-boot2-stdout.log" \
    "$ROOT/target/m8-persist-boot2-stderr.log"
  tpid=$(cat "$ROOT/target/m8-persist-timeout.pid")
  local saw_disk=0
  while kill -0 "$tpid" 2>/dev/null; do
    if grep -qF 'RAYNU-V-RAYNU-F-DISK-BOOT-OK' "$SERIAL2" 2>/dev/null \
      || grep -qE 'root=UUID=' "$SERIAL2" 2>/dev/null; then
      if grep -qF 'Linux version' "$SERIAL2" 2>/dev/null; then
        saw_disk=1
        echo "==> boot2 reached installed Linux; stopping HV"
        sleep 5
        stop_qemu
        break
      fi
    fi
    sleep 5
  done
  if [[ "$saw_disk" != "1" ]]; then
    wait_qemu || true
  fi
  if [[ ! -s "$SERIAL2" ]]; then
    echo "error: boot2 serial empty" >&2
    cat "$ROOT/target/m8-persist-boot2-stderr.log" >&2 || true
    exit 1
  fi
  scan "$SERIAL2"
  forbid_markers "$SERIAL2"
  require_persist_reserved "$SERIAL2"
  if ! grep -qE 'virtio-blk install disk bytes=.* keep=1' "$SERIAL2"; then
    echo "error: boot2 did not attach_disk_keep (keep=1)" >&2
    grep -n 'virtio-blk install disk' "$SERIAL2" >&2 || true
    exit 1
  fi
  if grep -qF 'setup-disk' "$SERIAL2"; then
    echo "error: boot2 ran setup-disk; persist did not boot the installed disk" >&2
    exit 1
  fi
  if ! grep -qF 'RAYNU-V-RAYNU-F-DISK-BOOT-OK' "$SERIAL2" \
    && ! grep -qE 'root=UUID=' "$SERIAL2"; then
    echo "error: boot2 missing DISK-BOOT-OK / root=UUID=" >&2
    exit 1
  fi
  echo "==> nested install → kill HV → second Linux without setup-disk (not iron, not ISO-INSTALL-OK)"
  echo "$NESTED_OK"
}

run_keep() {
  echo "==> MODE=keep — plant GPT, kill HV, boot2 keep=1 (not $NESTED_OK, not iron, not $ISO_OK)"
  reset_persist_img
  start_qemu "$SERIAL1" "$TIMEOUT_SMOKE" \
    "$ROOT/target/m8-persist-keep1-stdout.log" \
    "$ROOT/target/m8-persist-keep1-stderr.log"
  local waited=0
  while (( waited < 25 )); do
    if [[ -s "$SERIAL1" ]]; then
      break
    fi
    sleep 1
    waited=$((waited + 1))
  done
  if [[ ! -s "$SERIAL1" && "$QEMU_ACCEL" == "kvm" ]]; then
    echo "==> kvm serial empty after ${waited}s; retry tcg (persist scan is PRE-VMLAUNCH)"
    stop_qemu
    QEMU_ACCEL=tcg
    reset_persist_img
    start_qemu "$SERIAL1" "$TIMEOUT_SMOKE" \
      "$ROOT/target/m8-persist-keep1-stdout.log" \
      "$ROOT/target/m8-persist-keep1-stderr.log"
  fi
  if wait_serial_needle "$SERIAL1" "persist install disk hpa="; then
    sleep 2
    stop_qemu
  else
    wait_qemu || true
  fi
  if [[ ! -s "$SERIAL1" ]]; then
    echo "error: keep boot1 serial empty" >&2
    cat "$ROOT/target/m8-persist-keep1-stderr.log" >&2 || true
    exit 1
  fi
  scan "$SERIAL1"
  forbid_markers "$SERIAL1"
  require_persist_reserved "$SERIAL1"
  if grep -qE 'virtio-blk install disk bytes=.* keep=1' "$SERIAL1"; then
    echo "error: keep boot1 attached keep=1 on empty persist" >&2
    exit 1
  fi

  plant_keep_fixture "$SERIAL1"

  echo "==> boot2: same $M8_PERSIST_IMG ($(stat -c%s "$M8_PERSIST_IMG") bytes) after plant (not nested-OK)"
  start_qemu "$SERIAL2" "$TIMEOUT_SMOKE" \
    "$ROOT/target/m8-persist-keep2-stdout.log" \
    "$ROOT/target/m8-persist-keep2-stderr.log"
  waited=0
  while (( waited < 25 )); do
    if [[ -s "$SERIAL2" ]]; then
      break
    fi
    sleep 1
    waited=$((waited + 1))
  done
  if [[ ! -s "$SERIAL2" && "$QEMU_ACCEL" == "kvm" ]]; then
    echo "==> kvm serial empty after ${waited}s; retry tcg without wiping persist"
    stop_qemu
    QEMU_ACCEL=tcg
    start_qemu "$SERIAL2" "$TIMEOUT_SMOKE" \
      "$ROOT/target/m8-persist-keep2-stdout.log" \
      "$ROOT/target/m8-persist-keep2-stderr.log"
  fi
  if wait_serial_needle "$SERIAL2" "keep=1"; then
    sleep 2
    stop_qemu
  else
    wait_qemu || true
  fi
  if [[ ! -s "$SERIAL2" ]]; then
    echo "error: keep boot2 serial empty" >&2
    cat "$ROOT/target/m8-persist-keep2-stderr.log" >&2 || true
    exit 1
  fi
  scan "$SERIAL2"
  forbid_markers "$SERIAL2"
  require_persist_reserved "$SERIAL2"
  if ! grep -qE 'virtio-blk install disk bytes=.* keep=1' "$SERIAL2"; then
    echo "error: keep boot2 did not attach_disk_keep (keep=1)" >&2
    grep -n 'virtio-blk install disk' "$SERIAL2" >&2 || true
    grep -n 'VMXON-SKIP' "$SERIAL2" >&2 || true
    exit 1
  fi
  if grep -qF "$NESTED_OK" "$SERIAL2"; then
    echo "error: keep boot2 serial printed $NESTED_OK" >&2
    exit 1
  fi
  echo "==> nested File RAM keep=1 after HV kill + plant (not $NESTED_OK; not iron $IRON_OK; not $ISO_OK)"
}

run_lun() {
  echo "==> MODE=lun — QEMU NVMe Identify + I/O ready (not $NESTED_OK, not iron, not $ISO_OK)"
  M8_PERSIST_IMG=""
  M8_NVME_IMG="${M8_NVME_IMG:-$ROOT/target/m8-nvme.img}"
  rm -f "$M8_NVME_IMG"
  truncate -s 1G "$M8_NVME_IMG"
  start_qemu "$SERIAL1" "$TIMEOUT_SMOKE" \
    "$ROOT/target/m8-persist-lun-stdout.log" \
    "$ROOT/target/m8-persist-lun-stderr.log"
  local waited=0
  while (( waited < 25 )); do
    if [[ -s "$SERIAL1" ]]; then
      break
    fi
    sleep 1
    waited=$((waited + 1))
  done
  if wait_serial_needle "$SERIAL1" "durable LUN nvme I/O ready"; then
    sleep 2
    stop_qemu
  else
    wait_qemu || true
  fi
  if [[ ! -s "$SERIAL1" ]]; then
    echo "error: lun serial empty" >&2
    cat "$ROOT/target/m8-persist-lun-stderr.log" >&2 || true
    exit 1
  fi
  scan "$SERIAL1"
  grep -n 'durable LUN' "$SERIAL1" | head -n 20 || true
  forbid_markers "$SERIAL1"
  if ! grep -qF 'durable LUN nvme I/O ready' "$SERIAL1"; then
    echo "error: no NVMe I/O ready (leftover still the attach?)" >&2
    grep -E 'durable LUN|nvme' "$SERIAL1" | head -n 20 >&2 || true
    exit 1
  fi
  echo "==> DurableLun NVMe I/O ready (not $NESTED_OK; not iron $IRON_OK; not $ISO_OK)"
}

run_usb() {
  echo "==> MODE=usb — QEMU qemu-xhci + usb-storage BOT I/O ready (not $NESTED_OK, not iron, not $ISO_OK)"
  M8_PERSIST_IMG=""
  M8_NVME_IMG=""
  M8_USB_IMG="${M8_USB_IMG:-$ROOT/target/m8-usb.img}"
  rm -f "$M8_USB_IMG"
  truncate -s 1G "$M8_USB_IMG"
  start_qemu "$SERIAL1" "$TIMEOUT_SMOKE" \
    "$ROOT/target/m8-persist-usb-stdout.log" \
    "$ROOT/target/m8-persist-usb-stderr.log"
  local waited=0
  while (( waited < 25 )); do
    if [[ -s "$SERIAL1" ]]; then
      break
    fi
    sleep 1
    waited=$((waited + 1))
  done
  if wait_serial_needle "$SERIAL1" "durable LUN usb I/O ready"; then
    sleep 2
    stop_qemu
  else
    wait_qemu || true
  fi
  if [[ ! -s "$SERIAL1" ]]; then
    echo "error: usb serial empty" >&2
    cat "$ROOT/target/m8-persist-usb-stderr.log" >&2 || true
    exit 1
  fi
  scan "$SERIAL1"
  grep -n 'durable LUN' "$SERIAL1" | head -n 20 || true
  forbid_markers "$SERIAL1"
  if ! grep -qF 'durable LUN usb I/O ready' "$SERIAL1"; then
    echo "error: no USB I/O ready (leftover still the attach?)" >&2
    grep -E 'durable LUN|xhci|usb' "$SERIAL1" | head -n 30 >&2 || true
    exit 1
  fi
  echo "==> DurableLun USB BOT I/O ready (not $NESTED_OK; not iron $IRON_OK; not $ISO_OK)"
}

pick_accel
prepare_host
fetch_iso
build_efi

case "$MODE" in
  smoke) run_smoke ;;
  keep) run_keep ;;
  lun) run_lun ;;
  usb) run_usb ;;
  full) run_full ;;
  *)
    echo "error: MODE=$MODE (want smoke|keep|lun|usb|full)" >&2
    exit 1
    ;;
esac
