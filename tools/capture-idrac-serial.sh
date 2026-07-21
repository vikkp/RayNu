#!/usr/bin/env bash
# Capture RayNu-V / iDRAC serial (and optional BMC logs) for R640 evidence.
#
# What this can and cannot do:
#   CAN  — save everything RayNu-V prints on COM1 (gate markers, audit lines)
#          if you attach before reboot (SOL, or tee while pasting from Virtual Console)
#   CAN  — best-effort pull of iDRAC SEL via Redfish when credentials work
#   CANNOT — magically tap every internal iDRAC UI click; BMC has its own logs
#
# Modes:
#   tee       Read stdin → timestamped file (paste from Virtual Console, or pipe)
#   sol       ipmitool Serial-over-LAN → file (needs ipmitool + iDRAC SOL enabled)
#   redfish   Pull SEL (and LC if present) JSON/text next to the serial file
#
# Usage:
#   ./tools/capture-idrac-serial.sh tee --out docs/evidence/r640/2026-08-15-serial.txt
#   ./tools/capture-idrac-serial.sh sol --host 10.0.0.50 --user root --out ./r640-serial.txt
#   ./tools/capture-idrac-serial.sh redfish --host 10.0.0.50 --user root --out-dir ./capture/
#
# Field guide: docs/runbooks/r640_field_guide.md §1c / §4
# Logging model: docs/runbooks/idrac_logging.md
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MODE="${1:-}"
shift || true

HOST=""
USER_NAME="${IDRAC_USER:-root}"
PASS="${IDRAC_PASS:-}"
OUT=""
OUT_DIR=""
INSECURE="${INSECURE:-1}"

usage() {
  sed -n '2,22p' "$0" | sed 's/^# \{0,1\}//'
  exit "${1:-0}"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage 0 ;;
    --host) HOST="${2:-}"; shift 2 ;;
    --user) USER_NAME="${2:-}"; shift 2 ;;
    --pass) PASS="${2:-}"; shift 2 ;;
    --out) OUT="${2:-}"; shift 2 ;;
    --out-dir) OUT_DIR="${2:-}"; shift 2 ;;
    --insecure) INSECURE=1; shift ;;
    --secure) INSECURE=0; shift ;;
    *)
      echo "error: unknown arg: $1" >&2
      usage 1
      ;;
  esac
done

stamp_utc() { date -u +%Y-%m-%dT%H:%M:%SZ; }

write_header() {
  local path="$1"
  local how="$2"
  {
    echo "# RayNu-V iDRAC/COM1 capture"
    echo "# started_utc=$(stamp_utc)"
    echo "# mode=${how}"
    echo "# host=${HOST:-local}"
    echo "# note=Attach BEFORE reboot into r640-hypervisor.efi"
    echo "# --- begin transcript ---"
  } >>"$path"
}

mode_tee() {
  if [[ -z "$OUT" ]]; then
    OUT="docs/evidence/r640/$(date -u +%Y-%m-%d)-r640-serial.txt"
  fi
  mkdir -p "$(dirname "$OUT")"
  : >"$OUT"
  write_header "$OUT" "tee-stdin"
  echo "==> writing COM1 transcript to $OUT"
  echo "    Paste / pipe Virtual Console serial here. Ctrl-D when done."
  echo "    Tip: open this BEFORE rebooting into RayNu-V."
  # tee preserves operator view while saving
  tee -a "$OUT"
  {
    echo
    echo "# --- end transcript ---"
    echo "# ended_utc=$(stamp_utc)"
  } >>"$OUT"
  echo "==> saved $OUT"
  echo "RAYNU-V-M7-SERIAL-CAPTURE-OK"
}

mode_sol() {
  if [[ -z "$HOST" ]]; then
    echo "error: sol mode requires --host" >&2
    exit 1
  fi
  if [[ -z "$PASS" ]]; then
    echo "error: set --pass or IDRAC_PASS for SOL" >&2
    exit 1
  fi
  if ! command -v ipmitool >/dev/null 2>&1; then
    echo "error: ipmitool not found (apt/brew install ipmitool)" >&2
    echo "       fallback: ./tools/capture-idrac-serial.sh tee --out …" >&2
    exit 1
  fi
  if [[ -z "$OUT" ]]; then
    OUT="docs/evidence/r640/$(date -u +%Y-%m-%d)-r640-serial-sol.txt"
  fi
  mkdir -p "$(dirname "$OUT")"
  : >"$OUT"
  write_header "$OUT" "ipmitool-sol"
  echo "==> SOL activate $HOST → $OUT (Ctrl-C / ~. to disconnect per ipmitool)"
  # shellcheck disable=SC2029
  ipmitool -I lanplus -H "$HOST" -U "$USER_NAME" -P "$PASS" sol activate 2>>"$OUT" \
    | tee -a "$OUT" || true
  {
    echo
    echo "# --- end transcript ---"
    echo "# ended_utc=$(stamp_utc)"
  } >>"$OUT"
  echo "==> saved $OUT"
  echo "RAYNU-V-M7-SERIAL-CAPTURE-OK"
}

mode_redfish() {
  if [[ -z "$HOST" || -z "$PASS" ]]; then
    echo "error: redfish mode needs --host and --pass (or IDRAC_PASS)" >&2
    exit 1
  fi
  if ! command -v curl >/dev/null 2>&1; then
    echo "error: curl required" >&2
    exit 1
  fi
  if [[ -z "$OUT_DIR" ]]; then
    OUT_DIR="docs/evidence/r640/$(date -u +%Y-%m-%d)-idrac-logs"
  fi
  mkdir -p "$OUT_DIR"
  BASE="https://${HOST}"
  : >"$OUT_DIR/FETCH.txt"

  echo "==> Redfish log pull from $HOST → $OUT_DIR"
  # Best-effort paths used by many iDRAC 8/9 builds; may 404 — recorded honestly.
  for path in \
    /redfish/v1/Managers/iDRAC.Embedded.1/LogServices/Sel/Entries \
    /redfish/v1/Managers/iDRAC.Embedded.1/Logs/Sel \
    /redfish/v1/Systems/System.Embedded.1/LogServices/Sel/Entries \
    /redfish/v1/Managers/iDRAC.Embedded.1/LogServices/Lclog/Entries \
    /redfish/v1/Managers/iDRAC.Embedded.1/Logs/Lclog
  do
    safe="$(echo "$path" | tr '/.' '__')"
    code="$(curl -sS -k -u "${USER_NAME}:${PASS}" -o "$OUT_DIR/${safe}.json" -w '%{http_code}' \
      "${BASE}${path}" || true)"
    echo "  ${path} → HTTP ${code}" | tee -a "$OUT_DIR/FETCH.txt"
  done
  {
    echo "started_utc=$(stamp_utc)"
    echo "host=${HOST}"
    echo "note=Best-effort SEL/LC pull; 404 means this firmware path differs — keep COM1 serial as primary"
  } >"$OUT_DIR/README.txt"
  echo "==> Redfish fetch notes in $OUT_DIR/FETCH.txt"
  echo "RAYNU-V-M7-IDRAC-LOG-PULL-OK"
}

case "$MODE" in
  ""|-h|--help) usage 0 ;;
  tee) mode_tee ;;
  sol) mode_sol ;;
  redfish) mode_redfish ;;
  *)
    echo "error: mode must be tee|sol|redfish (got '$MODE')" >&2
    usage 1
    ;;
esac
