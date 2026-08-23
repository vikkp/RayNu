#!/usr/bin/env bash
# Refresh Cruzer RAYNUV with the latest green CI r640-hypervisor.efi.
#
# Pillar: [Z] [D]
# Proven Core: outside
#
# Run on raynuvsrv1 (Ubuntu on the R640 PERC) with the Cruzer left in
# front USB 2. This is the operator one-liner after an agent push:
#
#   ~/projects/raynuv/flashcruzer.sh
#
# Equivalent of: git pull → download CI artifact for HEAD → verify SHA256 →
# sudo ./tools/flash-cruzer-esp.sh --efi ~/r640-hypervisor.efi --sha256 …
#
# Identifies the stick by label RAYNUV + USB + Cruzer. Never hardcodes
# /dev/sdc. Never writes PERC sda/sdb. Never formats. Leaves installdisk.bin
# and auth.token alone. Stages EFI/RayNu/OVMF.fd from the host OVMF package
# (required for guest-UEFI / Stage 44; pass --no-ovmf to skip).
set -euo pipefail

# `. ~/projects/raynuv/flashcruzer.sh` must not `exit` the login shell.
if [[ "${BASH_SOURCE[0]}" != "$0" ]]; then
  exec bash "${BASH_SOURCE[0]}" "$@"
fi

SCRIPT_PATH="$(readlink -f "${BASH_SOURCE[0]}")"
TOOLS_DIR="$(cd "$(dirname "$SCRIPT_PATH")" && pwd)"
ROOT="$(cd "$TOOLS_DIR/.." && pwd)"
PICK="$TOOLS_DIR/flashcruzer_pick.py"
ESP="$TOOLS_DIR/flash-cruzer-esp.sh"
REJECT_FILE="$TOOLS_DIR/flashcruzer-reject-prefixes.txt"
LAUNCHER="${RAYNU_FLASHCRUZER_LAUNCHER:-$HOME/projects/raynuv/flashcruzer.sh}"
EFI_OUT="${RAYNU_EFI_OUT:-$HOME/r640-hypervisor.efi}"
USB_VIDPID="${RAYNU_CRUZER_USB:-0781:5151}"
WORKFLOW="${RAYNU_WORKFLOW:-ci.yml}"
WAIT_SECS="${RAYNU_WAIT_SECS:-1200}"
POLL_SECS="${RAYNU_POLL_SECS:-20}"
TMPJSON=""
PICK_FILE=""
WORKDIR=""
RUN_ID=""
MATCH=""
RUN_SHA=""
RUN_EVENT=""
RUN_URL=""

cleanup_all() {
  rm -rf "${WORKDIR:-}"
  rm -f "${TMPJSON:-}" "${PICK_FILE:-}"
}
trap cleanup_all EXIT

BRANCH=""
SELFTEST=0
INSTALL_LAUNCHER=0
NO_GIT=0
NO_FLASH=0
DRY=0
WAIT=0
ALLOW_UEFI_ONLY=0
REQUIRE_HEAD=0
ALLOW_REJECTED=0
PIN_RUN=""
PIN_SHA=""
NO_OVMF=0
OVMF_PATH=""

usage() {
  cat <<'EOF'
Usage:
  ~/projects/raynuv/flashcruzer.sh
  ./tools/flashcruzer.sh [--branch BRANCH] [--wait] [--download-only]
  ./tools/flashcruzer.sh --self-test
  ./tools/flashcruzer.sh --install-launcher

Downloads the latest green CI r640-hypervisor.efi for this clone's branch
and copies it to Cruzer RAYNUV (EFI/BOOT/BOOTX64.EFI plus EFI/RayNu/OVMF.fd).

Options:
  --branch BRANCH     git fetch + checkout + ff-only pull (default: current)
  --wait              poll until CI for HEAD finishes (default 20 min)
  --download-only     write ~/r640-hypervisor.efi; do not flash
  --dry-run           pick the CI run; do not download or flash
  --ovmf PATH         stage this 1-4 MiB _FVH OVMF.fd onto EFI/RayNu/OVMF.fd
  --no-ovmf           do not stage OVMF.fd (guest-UEFI will skip if missing)
  --run ID            pin a GitHub Actions run id
  --sha256 HEX        extra pin after download
  --require-head      refuse branch-fallback (artifact must match HEAD)
  --allow-uefi-only   allow a failed overall run if the UEFI artifact exists
  --allow-rejected    flash a SHA256 prefix on the known-bad list (emergency)
  --no-git            skip fetch/pull
  --install-launcher  mkdir -p ~/projects/raynuv and symlink this script
  --self-test         host/CI checks (no USB, no gh download)
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage; exit 0 ;;
    --self-test) SELFTEST=1; shift ;;
    --install-launcher) INSTALL_LAUNCHER=1; shift ;;
    --branch) BRANCH="${2:-}"; shift 2 ;;
    --wait) WAIT=1; shift ;;
    --download-only|--no-flash) NO_FLASH=1; shift ;;
    --dry-run) DRY=1; shift ;;
    --run) PIN_RUN="${2:-}"; shift 2 ;;
    --sha256) PIN_SHA="${2:-}"; shift 2 ;;
    --require-head) REQUIRE_HEAD=1; shift ;;
    --allow-uefi-only) ALLOW_UEFI_ONLY=1; shift ;;
    --allow-rejected) ALLOW_REJECTED=1; shift ;;
    --no-ovmf) NO_OVMF=1; shift ;;
    --ovmf) OVMF_PATH="${2:-}"; shift 2 ;;
    --no-git) NO_GIT=1; shift ;;
    *) echo "error: unknown arg: $1" >&2; usage; exit 2 ;;
  esac
done

install_launcher() {
  mkdir -p "$(dirname "$LAUNCHER")"
  ln -sfn "$SCRIPT_PATH" "$LAUNCHER"
  echo "==> launcher $LAUNCHER -> $SCRIPT_PATH"
}

# Last tab-separated pick line whose first field is a numeric run id.
# Ignores --wait progress that must never be captured into RUN_ID.
parse_pick_file() {
  local f="$1"
  local line
  if [[ ! -s "$f" ]]; then
    echo "error: empty CI pick file" >&2
    return 1
  fi
  line="$(awk -F'\t' '/^[0-9]+\t/ { line=$0 } END { print line }' "$f")"
  RUN_ID="$(printf '%s\n' "$line" | awk -F'\t' '{print $1}')"
  RUN_EVENT="$(printf '%s\n' "$line" | awk -F'\t' '{print $2}')"
  RUN_SHA="$(printf '%s\n' "$line" | awk -F'\t' '{print $3}')"
  MATCH="$(printf '%s\n' "$line" | awk -F'\t' '{print $4}')"
  RUN_URL="$(printf '%s\n' "$line" | awk -F'\t' '{print $5}')"
  if [[ ! "$RUN_ID" =~ ^[0-9]+$ ]]; then
    echo "error: could not parse numeric run id from pick file:" >&2
    cat "$f" >&2 || true
    return 1
  fi
}

self_test() {
  local tmp
  command -v python3 >/dev/null
  [[ -x "$ESP" ]] || chmod +x "$ESP"
  [[ -x "$SCRIPT_PATH" ]] || chmod +x "$SCRIPT_PATH"
  "$ESP" --self-test
  python3 "$PICK" --self-test
  grep -q '^26db0610$' "$REJECT_FILE"
  grep -qi 'never hardcode' "$SCRIPT_PATH"
  grep -q '0781:5151' "$SCRIPT_PATH"
  grep -q 'RAYNU-V-CRUZER-FLASH-OK' "$ESP"
  grep -q 'installdisk.bin' "$ESP"
  grep -q 'target_is_lab_cruzer' "$ESP"
  grep -q 'EFI/RayNu/OVMF.fd' "$ESP"
  grep -q 'ovmf_has_fvh' "$ESP"
  grep -q -- '--no-ovmf' "$ESP"
  tmp="$(mktemp)"
  printf '%s\n' \
    '==> waiting for CI on 68452b0b (PENDING' \
    $'32529969011\tpull_request\t68452b0bdeadbeef\thead-pr\thttps://github.com/vikkp/RayNu/actions/runs/32529969011' \
    >"$tmp"
  RUN_ID=""; RUN_EVENT=""; RUN_SHA=""; MATCH=""; RUN_URL=""
  parse_pick_file "$tmp"
  rm -f "$tmp"
  [[ "$RUN_ID" == "32529969011" ]]
  [[ "$RUN_EVENT" == "pull_request" ]]
  [[ "$MATCH" == "head-pr" ]]
  echo "RAYNU-V-FLASHCRUZER-SELFTEST-OK"
}

if [[ "$SELFTEST" -eq 1 ]]; then
  self_test
  exit 0
fi

if [[ "$INSTALL_LAUNCHER" -eq 1 ]]; then
  install_launcher
  exit 0
fi

need() {
  command -v "$1" >/dev/null || {
    echo "error: missing $1" >&2
    exit 1
  }
}
need git
need python3
need gh
need sha256sum
need unzip

if [[ ! -x "$ESP" ]]; then
  chmod +x "$ESP"
fi

cd "$ROOT"

if [[ "$NO_GIT" -eq 0 ]]; then
  git fetch origin
  if [[ -n "$BRANCH" ]]; then
    git checkout "$BRANCH"
    git pull --ff-only origin "$BRANCH"
  else
    BRANCH="$(git rev-parse --abbrev-ref HEAD)"
    if [[ "$BRANCH" == "HEAD" ]]; then
      echo "error: detached HEAD; pass --branch" >&2
      exit 1
    fi
    git pull --ff-only origin "$BRANCH"
  fi
else
  BRANCH="$(git rev-parse --abbrev-ref HEAD)"
fi

HEAD="$(git rev-parse HEAD)"
HEAD_SHORT="$(git rev-parse --short=8 HEAD)"
REPO="$(gh repo view --json nameWithOwner -q .nameWithOwner)"
echo "==> repo=$REPO branch=$BRANCH HEAD=$HEAD_SHORT"

install_launcher || true

gh_runs_json() {
  local extra=()
  if [[ -n "${1:-}" ]]; then
    extra+=(--commit "$1")
  fi
  gh run list --repo "$REPO" --workflow "$WORKFLOW" --branch "$BRANCH" \
    --limit 40 --json databaseId,headSha,status,conclusion,event,url,displayTitle,createdAt,workflowName \
    "${extra[@]}"
}

pick_from_json() {
  local json_file="$1"
  local args=(--json-file "$json_file" --head "$HEAD")
  if [[ "$ALLOW_UEFI_ONLY" -eq 1 ]]; then
    args+=(--allow-uefi-only)
  fi
  if [[ "$REQUIRE_HEAD" -eq 1 ]]; then
    args+=(--no-branch-fallback)
  fi
  python3 "$PICK" "${args[@]}"
}

wait_for_head() {
  local out="$1"
  local deadline=$((SECONDS + WAIT_SECS))
  local tmp saved_require
  tmp="$(mktemp)"
  saved_require="$REQUIRE_HEAD"
  REQUIRE_HEAD=1
  while (( SECONDS < deadline )); do
    # HEAD-only: never fall back to an older branch artifact while waiting.
    if ! gh_runs_json "$HEAD" >"$tmp"; then
      : >"$tmp"
    fi
    if pick_from_json "$tmp" >"$out" 2>/tmp/flashcruzer.pick.err; then
      REQUIRE_HEAD="$saved_require"
      rm -f "$tmp"
      return 0
    fi
    # Progress MUST stay on stderr — stdout is the pick line / command substitution trap.
    echo "==> waiting for CI on $HEAD_SHORT ($(tr '\n' ' ' </tmp/flashcruzer.pick.err)) — ${POLL_SECS}s" >&2
    sleep "$POLL_SECS"
  done
  REQUIRE_HEAD="$saved_require"
  rm -f "$tmp"
  echo "error: timed out waiting for CI on $HEAD" >&2
  return 1
}

if [[ -n "$PIN_RUN" ]]; then
  RUN_ID="$PIN_RUN"
  MATCH="pinned"
  RUN_SHA="$HEAD"
  RUN_EVENT="pinned"
  RUN_URL="https://github.com/${REPO}/actions/runs/${RUN_ID}"
  echo "==> pinned run $RUN_ID"
else
  TMPJSON="$(mktemp)"
  PICK_FILE="$(mktemp)"
  gh_runs_json >"$TMPJSON"
  set +e
  pick_from_json "$TMPJSON" >"$PICK_FILE" 2>/tmp/flashcruzer.pick.err
  PICK_RC=$?
  set -e
  if [[ "$PICK_RC" -eq 3 ]]; then
    if [[ "$WAIT" -eq 1 ]]; then
      wait_for_head "$PICK_FILE"
    else
      echo "error: CI for HEAD $HEAD_SHORT is still running" >&2
      cat /tmp/flashcruzer.pick.err >&2 || true
      echo "hint: re-run with --wait, or wait for the green check" >&2
      exit 1
    fi
  elif [[ "$PICK_RC" -ne 0 ]]; then
    if [[ "$WAIT" -eq 1 ]]; then
      wait_for_head "$PICK_FILE"
    else
      echo "error: no green CI artifact for $BRANCH $HEAD_SHORT" >&2
      cat /tmp/flashcruzer.pick.err >&2 || true
      echo "hint: --wait if CI is still queued; --run ID to pin" >&2
      exit 1
    fi
  fi
  parse_pick_file "$PICK_FILE"
fi

if [[ -z "$RUN_ID" ]]; then
  echo "error: could not resolve a CI run id" >&2
  exit 1
fi

echo "==> run=$RUN_ID event=$RUN_EVENT match=$MATCH"
echo "==> artifact commit=${RUN_SHA:0:8} url=$RUN_URL"
if [[ "$MATCH" == "branch-fallback" ]]; then
  echo "WARN: artifact is not HEAD $HEAD_SHORT (docs pin or CI still running?)"
  echo "      flashing EFI built at ${RUN_SHA:0:8}"
fi
if [[ "$MATCH" == "head-uefi-only" ]]; then
  echo "WARN: overall CI failed; using UEFI artifact only (--allow-uefi-only)"
fi

if [[ "$DRY" -eq 1 ]]; then
  echo "==> dry-run: would download run $RUN_ID -> $EFI_OUT and flash Cruzer RAYNUV"
  echo "RAYNU-V-FLASHCRUZER-DRY-OK"
  exit 0
fi

WORKDIR="$(mktemp -d /tmp/flashcruzer.XXXXXX)"

echo "==> downloading artifact r640-hypervisor.efi from run $RUN_ID"
set +e
gh run download "$RUN_ID" --repo "$REPO" -n r640-hypervisor.efi -D "$WORKDIR" >/tmp/flashcruzer.dl.out 2>/tmp/flashcruzer.dl.err
DL_RC=$?
set -e
if [[ "$DL_RC" -ne 0 ]]; then
  echo "==> gh run download failed; trying artifact zip API"
  ART="$(gh api "repos/${REPO}/actions/runs/${RUN_ID}/artifacts" \
    --jq '.artifacts[] | select(.name=="r640-hypervisor.efi") | .id' | head -1)"
  if [[ -z "$ART" ]]; then
    echo "error: no artifact named r640-hypervisor.efi on run $RUN_ID" >&2
    cat /tmp/flashcruzer.dl.err >&2 || true
    exit 1
  fi
  echo "==> artifact id=$ART"
  gh api -H "Accept: application/vnd.github+json" \
    "/repos/${REPO}/actions/artifacts/${ART}/zip" >"$WORKDIR/e4-efi.zip"
  unzip -o "$WORKDIR/e4-efi.zip" -d "$WORKDIR" >/dev/null
fi

FOUND="$(find "$WORKDIR" -type f -name 'r640-hypervisor.efi' | head -1)"
if [[ -z "$FOUND" ]]; then
  echo "error: unzip/download did not contain r640-hypervisor.efi" >&2
  find "$WORKDIR" -type f >&2 || true
  exit 1
fi

cp -f "$FOUND" "$EFI_OUT"
test -f "$EFI_OUT"
GOT="$(sha256sum "$EFI_OUT" | awk '{print $1}')"
BYTES="$(wc -c <"$EFI_OUT" | tr -d ' ')"
PREFIX="${GOT:0:8}"
echo "$BYTES $GOT"
echo "EFI ok prefix=$PREFIX"

if [[ "$BYTES" -lt 262144 || "$BYTES" -gt 20971520 ]]; then
  echo "error: EFI size $BYTES outside 256KiB–20MiB" >&2
  exit 1
fi
if [[ -n "$PIN_SHA" && "$GOT" != "$PIN_SHA" ]]; then
  echo "error: sha256 mismatch want=$PIN_SHA got=$GOT" >&2
  exit 1
fi

if [[ "$ALLOW_REJECTED" -eq 0 && -f "$REJECT_FILE" ]]; then
  if grep -Eiq "^${PREFIX}$" "$REJECT_FILE"; then
    echo "error: EFI prefix $PREFIX is on the known-bad list ($REJECT_FILE)" >&2
    echo "       refusing to flash. Pass --allow-rejected only if you mean it." >&2
    exit 1
  fi
fi

if [[ "$NO_FLASH" -eq 1 ]]; then
  echo "==> download-only: $EFI_OUT ($BYTES bytes, sha256=$GOT)"
  echo "RAYNU-V-FLASHCRUZER-DOWNLOAD-OK"
  exit 0
fi

if ! lsusb | grep -qi "$USB_VIDPID"; then
  echo "error: lsusb did not show $USB_VIDPID (SanDisk Cruzer) — plug front USB 2" >&2
  lsusb >&2 || true
  exit 1
fi
lsusb | grep -i "$USB_VIDPID" || true
if ! lsblk -o NAME,MODEL,TRAN,SIZE,LABEL,SERIAL,FSTYPE | grep -qi cruzer; then
  echo "error: lsblk did not show a Cruzer — refusing (never guess /dev/sdc)" >&2
  lsblk -o NAME,MODEL,TRAN,SIZE,LABEL,SERIAL,FSTYPE >&2 || true
  exit 1
fi
lsblk -o NAME,MODEL,TRAN,SIZE,LABEL,SERIAL,FSTYPE | grep -i cruzer || true

ESP_ARGS=(--efi "$EFI_OUT" --sha256 "$GOT")
if [[ "$NO_OVMF" -eq 1 && -n "$OVMF_PATH" ]]; then
  echo "error: --ovmf and --no-ovmf are mutually exclusive" >&2
  exit 1
fi
if [[ "$NO_OVMF" -eq 1 ]]; then
  ESP_ARGS+=(--no-ovmf)
elif [[ -n "$OVMF_PATH" ]]; then
  ESP_ARGS+=(--ovmf "$OVMF_PATH")
fi
if [[ "$(id -u)" -eq 0 ]]; then
  "$ESP" "${ESP_ARGS[@]}"
else
  sudo "$ESP" "${ESP_ARGS[@]}"
fi
sync
if findmnt /mnt/usb >/dev/null 2>&1; then
  if [[ "$(id -u)" -eq 0 ]]; then
    umount /mnt/usb
  else
    sudo umount /mnt/usb
  fi
fi
echo "Next: BIOS boot order stays Ubuntu on PERC; one-time F11 boot the Cruzer."
echo "RAYNU-V-FLASHCRUZER-OK"
