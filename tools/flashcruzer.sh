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
LINUX_ISO=""
NO_LINUX_ISO=0
REFAT=0

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
  --branch BRANCH     fetch origin/BRANCH and git checkout -B (default: current)
  --wait              poll until CI for HEAD finishes (default 20 min)
  --download-only     write ~/r640-hypervisor.efi; do not flash
  --dry-run           pick the CI run; do not download or flash
  --ovmf PATH         stage this 1-4 MiB _FVH OVMF.fd onto EFI/RayNu/OVMF.fd
  --no-ovmf           do not stage OVMF.fd (guest-UEFI will skip if missing)
  --linux-iso PATH    stage EFI/RayNu/linux.iso (Stage 46; size must exceed 73728)
  --no-linux-iso      remove leftover product ISO so E4 LINUX-EARLY still runs
  --refat-cruzer      mkfs.vfat -I -F 32 -n RAYNUV on identified whole-disk Cruzer (64MiB FAT)
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
    --linux-iso) LINUX_ISO="${2:-}"; shift 2 ;;
    --no-linux-iso) NO_LINUX_ISO=1; shift ;;
    --refat-cruzer) REFAT=1; shift ;;
    --ovmf) OVMF_PATH="${2:-}"; shift 2 ;;
    --no-git) NO_GIT=1; shift ;;
    *) echo "error: unknown arg: $1" >&2; usage; exit 2 ;;
  esac
done

if [[ -z "$LINUX_ISO" && -n "${PRODUCT_ISO:-}" ]]; then
  LINUX_ISO="$PRODUCT_ISO"
fi

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
  grep -q '^6fc742b0$' "$REJECT_FILE"
  grep -q 'flashcruzer reject 2d6b109 dest skip' "$SCRIPT_PATH"
  grep -q '33389381409' "$SCRIPT_PATH"
  grep -q '33391068937' "$SCRIPT_PATH"
  grep -q '33392055961' "$SCRIPT_PATH"
  grep -q '33394776080' "$SCRIPT_PATH"
  grep -q '33397104645' "$SCRIPT_PATH"
  grep -q '33387614559' "$SCRIPT_PATH"
  grep -q '33349142609' "$SCRIPT_PATH"
  grep -q '33347766697' "$SCRIPT_PATH"
  grep -q '33345731636' "$SCRIPT_PATH"
  grep -q '33337287432' "$SCRIPT_PATH"
  grep -q '33333506987' "$SCRIPT_PATH"
  grep -q '33321642509' "$SCRIPT_PATH"
  grep -qi 'never hardcode' "$SCRIPT_PATH"
  grep -q '0781:5151' "$SCRIPT_PATH"
  grep -q 'RAYNU-V-CRUZER-FLASH-OK' "$ESP"
  grep -q 'installdisk.bin' "$ESP"
  grep -q 'target_is_lab_cruzer' "$ESP"
  grep -q 'EFI/RayNu/OVMF.fd' "$ESP"
  grep -q 'ovmf_has_fvh' "$ESP"
  grep -q -- '--no-ovmf' "$ESP"
  grep -q -- '--linux-iso' "$ESP"
  grep -q -- '--no-linux-iso' "$ESP"
  grep -q 'EFI/RayNu/linux.iso' "$ESP"
  grep -q 'pruning leftover ESP ISOs' "$ESP"
  grep -q 'ESP free=' "$ESP"
  grep -q 'fsck.vfat -a' "$ESP"
  grep -q 'not format' "$ESP"
  grep -q -- '--refat-cruzer' "$ESP"
  grep -q 'mkfs.vfat -I -F 32 -n' "$ESP"
  grep -q 'fat_bytes_too_small' "$ESP"
  grep -q 'do not git checkout a SHA' "$SCRIPT_PATH"
  grep -q 'checkout -B' "$SCRIPT_PATH"
  grep -q 'refs/heads/${br}:refs/remotes/origin/${br}' "$SCRIPT_PATH"
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
echo "==> flashcruzer $SCRIPT_PATH"
echo "==> pre-git $(git rev-parse --abbrev-ref HEAD) $(git rev-parse --short=8 HEAD)"

# When the operator `git checkout <sha>`, HEAD is detached. Infer the origin
# branch that *points at* this commit and stay there (do not pull the tip).
infer_detached_branch() {
  local infer
  infer="$(git for-each-ref --format='%(refname:short)' refs/remotes/origin \
    --points-at HEAD 2>/dev/null | sed 's|^origin/||' | grep -v '^HEAD$' | head -1 || true)"
  if [[ -z "$infer" ]]; then
    echo "error: detached HEAD; pass --branch (do not git checkout a SHA)" >&2
    echo "example: ./tools/flashcruzer.sh --wait --branch <branch> --linux-iso /path/to.iso" >&2
    exit 1
  fi
  echo "==> detached HEAD $(git rev-parse --short=8 HEAD); CI branch origin/$infer (no checkout)"
  BRANCH="$infer"
}

# `git fetch origin <branch>` only writes FETCH_HEAD. checkout -B from the
# remote-tracking ref so a dirty/old working tree still gets origin/BRANCH.
checkout_origin_branch() {
  local br="$1"
  echo "==> fetching refs/heads/${br}"
  git fetch origin "refs/heads/${br}:refs/remotes/origin/${br}"
  echo "==> checkout -B ${br} origin/${br}"
  if ! git checkout -B "$br" "origin/${br}"; then
    echo "error: cannot checkout origin/${br}" >&2
    echo "       uncommitted changes in this clone? git status -sb" >&2
    git status -sb >&2 || true
    echo "hint: git stash push -u -m wip-before-flash" >&2
    echo "      then retry, or checkout origin/${br} yourself and pass --no-git" >&2
    exit 1
  fi
}

if [[ "$NO_GIT" -eq 0 ]]; then
  if [[ -n "$BRANCH" ]]; then
    checkout_origin_branch "$BRANCH"
  else
    git fetch origin
    BRANCH="$(git rev-parse --abbrev-ref HEAD)"
    if [[ "$BRANCH" == "HEAD" ]]; then
      infer_detached_branch
    else
      git pull --ff-only origin "$BRANCH"
    fi
  fi
else
  BRANCH="$(git rev-parse --abbrev-ref HEAD)"
  if [[ "$BRANCH" == "HEAD" ]]; then
    infer_detached_branch
  fi
fi

HEAD="$(git rev-parse HEAD)"
HEAD_SHORT="$(git rev-parse --short=8 HEAD)"
REPO="$(gh repo view --json nameWithOwner -q .nameWithOwner)"
echo "==> repo=$REPO branch=$BRANCH HEAD=$HEAD_SHORT"

# 2d6b109 dest skip: IoReadFifo8 still skips dest 0x205f18 inside identity
# 0x200000. Operator FLASHCRUZER-OK on e5-stage46-iso-a623 / run 33321642509
# Pin e70a295 (firmware HLT skip after ataio) run 33397104645 is F11.
# skip-without-inject + El Torito ide@ first + skip HLT after PACKET ataio>0.
# do not F11 90da03d / --run 33394776080 (ataio==0 skip parks PACKET HLT at RET).
# do not F11 56f31d3 / --run 33392055961 (scsi@3 first, no El Torito boot).
# Do not F11 ea30da1 / a2acfc8 / --run 33389381409 / 33391068937.
# Do not F11 b824789 / run 33387614559 (hide-IDE skip-after-inject raw pci_ide).
# Do not F11 d61dc7e / run 33349142609 (ConnectAll IdeBus CpuSleep).
# Do not F11 5c0f7a2 / run 33347766697 (ATAPI-first bootorder).
# Do not F11 2ae4544 / run 33345731636 (wakeup without ATA-over-PIT).
# Iron COM2 8663f56 dest_ok then IN EAX,DX Delay — do not F11 8663f56 /
# run 33333506987. Iron COM2 084430f Delay then HLT stall — do not F11
# 084430f / run 33337287432. flashcruzer reject 2d6b109 dest skip.
# Not ISO-INSTALL-OK.
refuse_2d6b109_dest_skip() {
  if [[ "$ALLOW_REJECTED" -ne 0 ]]; then
    return 0
  fi
  if [[ "$PIN_RUN" == "33321642509" ]]; then
    echo "error: run 33321642509 is 2d6b109 dest skip (identity 0x200000)" >&2
    echo "       ACPI cannot install. product ISO HLT stall before n=16384;" >&2
    echo "       wait for this SHA CI. Do not F11 ea30da1 / --run 33389381409." >&2
    echo "       FLASHCRUZER-OK for 2d6b109 is not F11." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33333506987" ]]; then
    echo "error: run 33333506987 is 8663f56 dest_ok then 0xAF00 Delay" >&2
    echo "       product ISO fw_cfg bootorder El Torito ide@ first; flash 90da03d / --run 33394776080." >&2
    echo "       firmware HLT skip after ataio; flash e70a295 / --run 33397104645." >&2
    echo "       firmware HLT skip without inject; flash 56f31d3 / --run 33392055961." >&2
    echo "       do not F11 56f31d3." >&2
    echo "       do not F11 90da03d." >&2
    echo "       product ISO HLT stall before n=16384; wait for this SHA CI." >&2
    echo "       do not F11 8663f56 again." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33337287432" ]]; then
    echo "error: run 33337287432 is 084430f Delay then HLT stall" >&2
    echo "       product ISO fw_cfg bootorder El Torito ide@ first; flash 90da03d / --run 33394776080." >&2
    echo "       firmware HLT skip after ataio; flash e70a295 / --run 33397104645." >&2
    echo "       firmware HLT skip without inject; flash 56f31d3 / --run 33392055961." >&2
    echo "       do not F11 56f31d3." >&2
    echo "       do not F11 90da03d." >&2
    echo "       product ISO HLT stall before n=16384; wait for this SHA CI." >&2
    echo "       do not F11 084430f again." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33345731636" ]]; then
    echo "error: run 33345731636 is 2ae4544 LAPIC expiry without I/O-over-PIT" >&2
    echo "       product ISO fw_cfg bootorder El Torito ide@ first; flash 90da03d / --run 33394776080." >&2
    echo "       firmware HLT skip after ataio; flash e70a295 / --run 33397104645." >&2
    echo "       firmware HLT skip without inject; flash 56f31d3 / --run 33392055961." >&2
    echo "       do not F11 56f31d3." >&2
    echo "       do not F11 90da03d." >&2
    echo "       product ISO HLT stall before n=16384; wait for this SHA CI." >&2
    echo "       do not F11 2ae4544 again." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33347766697" ]]; then
    echo "error: run 33347766697 is 5c0f7a2 ATAPI-first bootorder" >&2
    echo "       product ISO fw_cfg bootorder El Torito ide@ first; flash 90da03d / --run 33394776080." >&2
    echo "       firmware HLT skip after ataio; flash e70a295 / --run 33397104645." >&2
    echo "       firmware HLT skip without inject; flash 56f31d3 / --run 33392055961." >&2
    echo "       do not F11 56f31d3." >&2
    echo "       do not F11 90da03d." >&2
    echo "       product ISO HLT stall before n=16384; wait for this SHA CI." >&2
    echo "       do not F11 5c0f7a2 again." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33349142609" ]]; then
    echo "error: run 33349142609 is d61dc7e ConnectAll IdeBus CpuSleep" >&2
    echo "       product ISO fw_cfg bootorder El Torito ide@ first; flash 90da03d / --run 33394776080." >&2
    echo "       firmware HLT skip after ataio; flash e70a295 / --run 33397104645." >&2
    echo "       firmware HLT skip without inject; flash 56f31d3 / --run 33392055961." >&2
    echo "       do not F11 56f31d3." >&2
    echo "       do not F11 90da03d." >&2
    echo "       product ISO HLT stall before n=16384; wait for this SHA CI." >&2
    echo "       do not F11 d61dc7e again." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33387614559" ]]; then
    echo "error: run 33387614559 is b824789 skip-after-inject raw pci_ide" >&2
    echo "       product ISO fw_cfg bootorder El Torito ide@ first; flash 90da03d / --run 33394776080." >&2
    echo "       firmware HLT skip after ataio; flash e70a295 / --run 33397104645." >&2
    echo "       firmware HLT skip without inject; flash 56f31d3 / --run 33392055961." >&2
    echo "       do not F11 56f31d3." >&2
    echo "       do not F11 90da03d." >&2
    echo "       product ISO HLT stall before n=16384; wait for this SHA CI." >&2
    echo "       do not F11 b824789." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33389381409" ]]; then
    echo "error: run 33389381409 is ea30da1 hide-IDE inject vec=0x20 timer ISR" >&2
    echo "       product ISO fw_cfg bootorder El Torito ide@ first; flash 90da03d / --run 33394776080." >&2
    echo "       firmware HLT skip after ataio; flash e70a295 / --run 33397104645." >&2
    echo "       firmware HLT skip without inject; flash 56f31d3 / --run 33392055961." >&2
    echo "       do not F11 56f31d3." >&2
    echo "       do not F11 90da03d." >&2
    echo "       do not F11 ea30da1." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33391068937" ]]; then
    echo "error: run 33391068937 is a2acfc8 n>16384 after hide-IDE timer ISR" >&2
    echo "       product ISO fw_cfg bootorder El Torito ide@ first; flash 90da03d / --run 33394776080." >&2
    echo "       firmware HLT skip after ataio; flash e70a295 / --run 33397104645." >&2
    echo "       firmware HLT skip without inject; flash 56f31d3 / --run 33392055961." >&2
    echo "       do not F11 56f31d3." >&2
    echo "       do not F11 90da03d." >&2
    echo "       do not F11 a2acfc8 / ea30da1." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33392055961" ]]; then
    echo "error: run 33392055961 is 56f31d3 scsi@3 first with no El Torito boot option" >&2
    echo "       product ISO fw_cfg bootorder El Torito ide@ first; flash 90da03d / --run 33394776080." >&2
    echo "       firmware HLT skip after ataio; flash e70a295 / --run 33397104645." >&2
    echo "       firmware HLT skip without inject; flash 56f31d3 / --run 33392055961." >&2
    echo "       do not F11 56f31d3." >&2
    echo "       do not F11 90da03d." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33394776080" ]]; then
    echo "error: run 33394776080 is 90da03d skip-after-inject ataio==0 parks PACKET HLT" >&2
    echo "       firmware HLT skip after ataio; flash e70a295 / --run 33397104645." >&2
    echo "       product ISO fw_cfg bootorder El Torito ide@ first; flash 90da03d / --run 33394776080." >&2
    echo "       do not F11 90da03d." >&2
    echo "       do not F11 56f31d3." >&2
    exit 1
  fi
  case "$HEAD_SHORT" in
    2d6b109*|8663f56*|084430f*|2ae4544*|5c0f7a2*|d61dc7e*|b824789*|2cf313e*|ea30da1*|c587ba7*|a2acfc8*|56f31d3*|b8a726d*|90da03d*|82c0fd4*)
      echo "error: HEAD $HEAD_SHORT is not the F11 pin" >&2
      echo "       firmware HLT skip after ataio; flash e70a295 / --run 33397104645." >&2
      echo "       product ISO fw_cfg bootorder El Torito ide@ first; flash 90da03d / --run 33394776080." >&2
      echo "       firmware HLT skip without inject; flash 56f31d3 / --run 33392055961." >&2
      echo "       do not F11 56f31d3." >&2
      echo "       do not F11 90da03d." >&2
      echo "       product ISO HLT stall before n=16384; do not F11 ea30da1." >&2
      echo "       do not F11 ea30da1 / a2acfc8 / --run 33389381409 / 33391068937." >&2
      echo "       do not checkout cursor/e5-stage46-iso-a623 for F11." >&2
      echo "       git checkout -B cursor/e5-pm1-sci-a623 origin/cursor/e5-pm1-sci-a623" >&2
      echo "       ./tools/flashcruzer.sh --no-git --run 33397104645 --linux-iso ..." >&2
      exit 1
      ;;
  esac
}
refuse_2d6b109_dest_skip

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
    if [[ "$PREFIX" == "6fc742b0" ]]; then
      echo "       2d6b109 dest skip cannot install ACPI; do not F11 ea30da1 / --run 33389381409." >&2
    fi
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
if [[ "$NO_LINUX_ISO" -eq 1 && -n "$LINUX_ISO" ]]; then
  echo "error: --linux-iso and --no-linux-iso are mutually exclusive" >&2
  exit 1
fi
if [[ "$NO_OVMF" -eq 1 ]]; then
  ESP_ARGS+=(--no-ovmf)
elif [[ -n "$OVMF_PATH" ]]; then
  ESP_ARGS+=(--ovmf "$OVMF_PATH")
fi
if [[ "$NO_LINUX_ISO" -eq 1 ]]; then
  ESP_ARGS+=(--no-linux-iso)
elif [[ -n "$LINUX_ISO" ]]; then
  ESP_ARGS+=(--linux-iso "$LINUX_ISO")
fi
if [[ "$REFAT" -eq 1 ]]; then
  ESP_ARGS+=(--refat-cruzer)
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
