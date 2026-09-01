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
  grep -q '33399209557' "$SCRIPT_PATH"
  grep -q '33399991049' "$SCRIPT_PATH"
  grep -q 'firmware force IF for inject' "$SCRIPT_PATH"
  grep -q 'do not F11 77f5866' "$SCRIPT_PATH"
  grep -q 'retrigger 9df52c5' "$SCRIPT_PATH"
  grep -q '33402411199' "$SCRIPT_PATH"
  grep -q '33404368817' "$SCRIPT_PATH"
  grep -q 'flash 5227ad9' "$SCRIPT_PATH"
  grep -q '33408594472' "$SCRIPT_PATH"
  grep -q 'flash 489d938' "$SCRIPT_PATH"
  grep -q 'firmware arm ATA GSI 14' "$SCRIPT_PATH"
  grep -q 'firmware prefer ATA IRR' "$SCRIPT_PATH"
  grep -q 'firmware ATA over PIC' "$SCRIPT_PATH"
  grep -q '33411580450' "$SCRIPT_PATH"
  grep -q 'flash bce5bbb' "$SCRIPT_PATH"
  grep -q 'do not F11 489d938' "$SCRIPT_PATH"
  grep -q '33413425759' "$SCRIPT_PATH"
  grep -q 'flash eaa580d' "$SCRIPT_PATH"
  grep -q 'do not F11 bce5bbb' "$SCRIPT_PATH"
  grep -q '33415083012' "$SCRIPT_PATH"
  grep -q 'flash 12926eb' "$SCRIPT_PATH"
  grep -q 'do not F11 eaa580d' "$SCRIPT_PATH"
  grep -q '33418246409' "$SCRIPT_PATH"
  grep -q 'flash 0bb06a2' "$SCRIPT_PATH"
  grep -q 'do not F11 12926eb' "$SCRIPT_PATH"
  grep -q '33422323257' "$SCRIPT_PATH"
  grep -q 'flash 30b78a0' "$SCRIPT_PATH"
  grep -q 'do not F11 0bb06a2' "$SCRIPT_PATH"
  grep -q '33424573770' "$SCRIPT_PATH"
  grep -q 'flash 8e581c7' "$SCRIPT_PATH"
  grep -q 'do not F11 30b78a0' "$SCRIPT_PATH"
  grep -q '33426291731' "$SCRIPT_PATH"
  grep -q 'flash d7d63ca' "$SCRIPT_PATH"
  grep -q 'do not F11 8e581c7' "$SCRIPT_PATH"
  grep -q '33429494930' "$SCRIPT_PATH"
  grep -q 'flash e4faceb' "$SCRIPT_PATH"
  grep -q 'do not F11 d7d63ca' "$SCRIPT_PATH"
  grep -q '33433126839' "$SCRIPT_PATH"
  grep -q 'flash 3b7bbac' "$SCRIPT_PATH"
  grep -q 'do not F11 e4faceb' "$SCRIPT_PATH"
  grep -q '33436232227' "$SCRIPT_PATH"
  grep -q 'flash a14223f' "$SCRIPT_PATH"
  grep -q 'do not F11 3b7bbac' "$SCRIPT_PATH"
  grep -q '33440050729' "$SCRIPT_PATH"
  grep -q 'flash b5c3a9c' "$SCRIPT_PATH"
  grep -q 'do not F11 a14223f' "$SCRIPT_PATH"
  grep -q '33430294210' "$SCRIPT_PATH"
  grep -q 'retrigger 5a69de2' "$SCRIPT_PATH"
  grep -q '33437881901' "$SCRIPT_PATH"
  grep -q 'retrigger 0d36b53' "$SCRIPT_PATH"
  grep -q '33438918646' "$SCRIPT_PATH"
  grep -q '33440951898' "$SCRIPT_PATH"
  grep -q '33443188019' "$SCRIPT_PATH"
  grep -q '33444677681' "$SCRIPT_PATH"
  grep -q '33445476540' "$SCRIPT_PATH"
  grep -q '33446918467' "$SCRIPT_PATH"
  grep -q '33448452364' "$SCRIPT_PATH"
  grep -q '33449291916' "$SCRIPT_PATH"
  grep -q '33450139765' "$SCRIPT_PATH"
  grep -q '33451734183' "$SCRIPT_PATH"
  grep -q '33452659198' "$SCRIPT_PATH"
  grep -q '33453324709' "$SCRIPT_PATH"
  grep -q '33454130069' "$SCRIPT_PATH"
  grep -q '33454767329' "$SCRIPT_PATH"
  grep -q '33455373334' "$SCRIPT_PATH"
  grep -q '33455903058' "$SCRIPT_PATH"
  grep -q '33456465331' "$SCRIPT_PATH"
  grep -q '33457132491' "$SCRIPT_PATH"
  grep -q '33458084140' "$SCRIPT_PATH"
  grep -q '33459130885' "$SCRIPT_PATH"
  grep -q '33459800906' "$SCRIPT_PATH"
  grep -q '33460343555' "$SCRIPT_PATH"
  grep -q '33460640154' "$SCRIPT_PATH"
  grep -q '33461311226' "$SCRIPT_PATH"
  grep -q '33461867968' "$SCRIPT_PATH"
  grep -q '33462312015' "$SCRIPT_PATH"
  grep -q '33462988233' "$SCRIPT_PATH"
  grep -q '33463584633' "$SCRIPT_PATH"
  grep -q '33463983585' "$SCRIPT_PATH"
  grep -q '33463955237' "$SCRIPT_PATH"
  grep -q 'nested iso=0 firmware LAPIC timer' "$SCRIPT_PATH"
  grep -q 'nested iso=0 EDK2 IRQ0' "$SCRIPT_PATH"
  grep -q 'product ISO firmware HLT wake' "$SCRIPT_PATH"
  grep -q 'nested iso=0 firmware HLT ATA' "$SCRIPT_PATH"
  grep -q 'product ISO firmware HLT ATA' "$SCRIPT_PATH"
  grep -q 'product ISO firmware HLT ATA IOAPIC' "$SCRIPT_PATH"
  grep -q 'nested iso=0 firmware HLT ATA LAPIC' "$SCRIPT_PATH"
  grep -q 'product ISO firmware HLT ATA LAPIC' "$SCRIPT_PATH"
  grep -q 'product ISO firmware HLT wake LAPIC' "$SCRIPT_PATH"
  grep -q 'firmware HLT skip only after inject' "$SCRIPT_PATH"
  grep -q 'product ISO firmware HLT wake LAPIC timer' "$SCRIPT_PATH"
  grep -q 'product ISO firmware HLT wake IDT 0x20' "$SCRIPT_PATH"
  grep -q 'product ISO firmware HLT wake IDT 0x20 only' "$SCRIPT_PATH"
  grep -q 'product ISO firmware HLT wake LVT unmask' "$SCRIPT_PATH"
  grep -q 'product ISO firmware LVT timer inject' "$SCRIPT_PATH"
  grep -q 'product ISO firmware no LVT inject I/O' "$SCRIPT_PATH"
  grep -q 'product ISO firmware wake preempt' "$SCRIPT_PATH"
  grep -q 'product ISO firmware no preempt inject' "$SCRIPT_PATH"
  grep -q 'product ISO firmware wake Delay I/O' "$SCRIPT_PATH"
  grep -q 'product ISO firmware Delay I/O no inject' "$SCRIPT_PATH"
  grep -q 'product ISO firmware wake IDE cmd' "$SCRIPT_PATH"
  grep -q 'product ISO firmware IDE cmd reset 0' "$SCRIPT_PATH"
  grep -q 'product ISO firmware IDE cmd ATA IRQ' "$SCRIPT_PATH"
  grep -q 'product ISO firmware IDE cmd inject ATA' "$SCRIPT_PATH"
  grep -q 'product ISO firmware IDE cmd ATA on HLT' "$SCRIPT_PATH"
  grep -q 'product ISO firmware IDE cmd I/O no inject' "$SCRIPT_PATH"
  grep -q 'product ISO firmware IDE cmd HLT 0x20' "$SCRIPT_PATH"
  grep -q 'firmware PIC ATA' "$SCRIPT_PATH"
  grep -q 'firmware OVMF ATA vector' "$SCRIPT_PATH"
  grep -q 'do not clobber IOAPIC ATA vector' "$SCRIPT_PATH"
  grep -q 'do not inject leftover 0x2E' "$SCRIPT_PATH"
  grep -q 'do not clobber PIC ICW2' "$SCRIPT_PATH"
  grep -q 'PIC ATA vector follows ICW2' "$SCRIPT_PATH"
  grep -q 'firmware HLT insn_len 0 skip' "$SCRIPT_PATH"
  grep -q 'nested iso=0 firmware HLT PIT' "$SCRIPT_PATH"
  grep -q 'nested iso=0 firmware HLT no PIT inject' "$SCRIPT_PATH"
  grep -q 'nested iso=0 firmware HLT EDK2 0x68' "$SCRIPT_PATH"
  grep -q 'product ISO firmware HLT EDK2 0x68' "$SCRIPT_PATH"
  grep -q 'nested iso=0 firmware HLT 0x68 miss' "$SCRIPT_PATH"
  grep -q 'firmware HLT inject cap' "$SCRIPT_PATH"
  grep -q 'nested iso=0 firmware HLT skip after inject' "$SCRIPT_PATH"
  grep -q 'nested iso=0 firmware HLT inject cap' "$SCRIPT_PATH"
  grep -q '33466890874' "$SCRIPT_PATH"
  grep -q '33468177902' "$SCRIPT_PATH"
  grep -q '33469144799' "$SCRIPT_PATH"
  grep -q '33470144235' "$SCRIPT_PATH"
  grep -q '33470837613' "$SCRIPT_PATH"
  grep -q '33471631130' "$SCRIPT_PATH"
  grep -q '33473305422' "$SCRIPT_PATH"
  grep -q 'nested iso=0 firmware HLT skip after cap' "$SCRIPT_PATH"
  grep -q 'nested iso=0 firmware HLT PM1 SCI' "$SCRIPT_PATH"
  grep -q 'nested iso=0 firmware HLT 0x71' "$SCRIPT_PATH"
  grep -q 'i440FX slot-0 Header Type single function' "$SCRIPT_PATH"
  grep -q 'nested iso=0 firmware IdeBus PCI' "$SCRIPT_PATH"
  grep -q 'guest-UEFI stop inj' "$SCRIPT_PATH"
  grep -q '33464757885' "$SCRIPT_PATH"
  grep -q '33465649406' "$SCRIPT_PATH"
  grep -q '33466397855' "$SCRIPT_PATH"
  grep -q 'firmware take IOAPIC ATA' "$SCRIPT_PATH"
  grep -q 'IOAPIC edge no remote IRR' "$SCRIPT_PATH"
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
# Pin b5c3a9c (firmware HLT insn_len 0 skip) run 33440050729 is F11. EFI 9775891845.
# flash a14223f is not F11. do not F11 a14223f / --run 33436232227.
# flash 4730397 is not F11. do not F11 4730397 / --run 33436822494.
# firmware HLT insn_len 0 skip (nested CpuSleep f4c3 ataio=0).
# nested iso=0 firmware HLT PIT (CI 33440951898 skip-without-inject ataio=0).
# nested iso=0 firmware HLT no PIT inject (CI 33464757885 VMXON inject 0x20 timer ISR CPUID ataio=0).
# do not F11 7f199db / --run 33464757885.
# nested iso=0 firmware HLT EDK2 0x68 (CI 33465649406 VMXON-SKIP; leftover LVT 0x20 stole EDK2 IRQ0).
# do not F11 739eb8a / --run 33465649406.
# product ISO firmware HLT EDK2 0x68 (CI 33466397855 VMXON-SKIP; iron ea30da1 leftover 0x20 timer ISR).
# do not F11 13052e7 / --run 33466397855.
# nested iso=0 firmware HLT 0x68 miss (CI 33466890874 VMXON inject 0x68 CR livelock ataio=0).
# firmware HLT inject cap (stop after 8; CI 33466890874 print-only cap).
# do not F11 dd0096b / --run 33466890874.
# nested iso=0 firmware HLT skip after inject (CI 33468177902 VMXON 8x 0x20 then skip-HLT ataio=0).
# do not F11 65b94c1 / --run 33468177902.
# nested iso=0 firmware HLT inject cap (CI 33469144799 skip-after-inject hlt=0 8x 0x20 ataio=0).
# do not F11 cfabb62 / --run 33469144799.
# guest-UEFI stop inj (CI 33470144235 VMXON-SKIP; inj= on POST_DXE_TAIL stop).
# do not F11 ee90aad / --run 33470144235.
# nested iso=0 firmware HLT skip after cap (CI 33470837613 VMXON inj=1487 CPUID ataio=0).
# do not F11 e416806 / --run 33470837613.
# nested iso=0 firmware HLT PM1 SCI (CI 33471631130 VMXON-SKIP skip-after-cap; inject FADT SCI 0x71 not leftover LVT 0x20).
# nested iso=0 firmware HLT 0x71.
# do not F11 1c7ff1c / --run 33471631130.
# i440FX slot-0 Header Type single function (CI 33473305422 VMXON-SKIP SCI unproven; duplicate 00:00.1 IDE ataio=0).
# nested iso=0 firmware IdeBus PCI.
# do not F11 68aff41 / --run 33473305422.
# nested iso=0 EDK2 IRQ0 (CI 33443188019 VMXON-SKIP; take-None unproven on VMX).
# nested iso=0 firmware LAPIC timer (CI 33444677681 VMXON-SKIP; 33440951898 pic=0 gsi2=0).
# product ISO firmware HLT wake (skip_pit leftover 0x20; inject EDK2 0x68 on firmware HLT ataio==0).
# nested iso=0 firmware HLT ATA (CI 33446918467 VMXON-SKIP; IDENTIFY WaitForInterrupt IRQ 14).
# firmware SRST ATA IRQ (product SRST deassert raises IRQ 14).
# product ISO firmware HLT ATA (CI 33448452364 VMXON-SKIP; IDENTIFY WaitForInterrupt 0x76).
# product ISO firmware HLT ATA IOAPIC (CI 33449291916 VMXON-SKIP; pic=0 take pin 14).
# nested iso=0 firmware HLT ATA LAPIC (CI 33450139765 VMXON-SKIP; pic=0 latch 0x76).
# product ISO firmware HLT ATA LAPIC (CI 33451734183 VMXON-SKIP; pic=0 latch 0x76).
# product ISO firmware HLT wake LAPIC (iron COM2 b5c3a9c ataio=0 inj=0 pic=0).
# firmware HLT skip only after inject (CI 33452659198 VMXON-SKIP; iron COM2 b5c3a9c inj=0).
# do not F11 77d84d3 / --run 33452659198.
# product ISO firmware HLT wake LAPIC timer (CI 33453324709 VMXON-SKIP; pic=0 force LVT).
# do not F11 c4cd522 / --run 33453324709.
# product ISO firmware HLT wake IDT 0x20 (CI 33454130069 VMXON-SKIP; skip-only-after-inject IRET to RET).
# do not F11 37320ad / --run 33454130069.
# product ISO firmware HLT wake IDT 0x20 only (CI 33454767329 VMXON-SKIP; ignore unmasked LVT 0x27).
# do not F11 d454545 / --run 33454767329.
# product ISO firmware HLT wake LVT unmask (CI 33455373334 VMXON-SKIP; inject 0x20 with LVT unmasked).
# do not F11 f37674f / --run 33455373334.
# product ISO firmware LVT timer inject (CI 33455903058 VMXON-SKIP; skip_pit must not drop periodic LVT 0x20).
# product ISO firmware no LVT inject I/O (unmasked LVT 0x20 must not inject on CF8/Delay/preempt).
# CI 33463983585 VMXON-SKIP. do not F11 89bba8f / --run 33463983585.
# CI 33463955237 VMXON-SKIP. do not F11 4b11843 / --run 33463955237.
# CI 33463584633 curl 35. do not F11 4e98f27 / --run 33463584633.
# do not F11 91f15b3 / --run 33455903058.
# product ISO firmware wake preempt (CI 33456465331 VMXON-SKIP; HLT only, not VMX preemption 52; skip RIP stays HLT-only).
# product ISO firmware no preempt inject (CF8 walk must finish; inject on CpuSleep HLT).
# CI 33461867968 VMXON-SKIP. do not F11 c7e4638 / --run 33461867968.
# CI 33462312015 VMXON-SKIP. do not F11 90569fd / --run 33462312015.
# product ISO firmware Delay I/O no inject (PM timer IN already ticks; inject on CpuSleep HLT).
# CI 33462988233 VMXON-SKIP. do not F11 b670993 / --run 33462988233.
# do not F11 8f04fa6 / --run 33456465331.
# product ISO firmware wake Delay I/O (CI 33457132491 VMXON-SKIP; ACPI PM timer I/O Delay; skip RIP stays HLT-only; do not wake CF8).
# do not F11 1b758d2 / --run 33457132491.
# product ISO firmware wake IDE cmd (CI 33458084140 VMXON-SKIP; IdeBus Start PCI command write; skip RIP stays HLT-only; empty CF8 does not wake).
# do not F11 ce11fda / --run 33458084140.
# product ISO firmware IDE cmd reset 0 (PIIX/QEMU command is 0 at reset so IdeBus Start writes offset 0x04; reset 0x0005 skipped that write).
# CI 33459130885 VMXON-SKIP. do not F11 8851af8 / --run 33459130885.
# product ISO firmware IDE cmd ATA IRQ (IdeBus Start PCI command write raises IRQ 14; BAR writes do not).
# CI 33459800906 VMXON-SKIP. do not F11 7d02e96 / --run 33459800906.
# product ISO firmware IDE cmd inject ATA (IdeBus Start PCI command write injects 0x76 not timer 0x20).
# CI 33460343555 VMXON-SKIP. do not F11 72885fa / --run 33460343555.
# product ISO firmware IDE cmd ATA on HLT (defer 0x76 to CpuSleep after IdeBus Start; not mid-PciIo).
# CI 33460640154 VMXON-SKIP. do not F11 244750c / --run 33460640154.
# product ISO firmware IDE cmd I/O no inject (PCI command OUT does not inject; ATA 0x76 waits for CpuSleep).
# CI 33461311226 VMXON-SKIP. do not F11 2d64091 / --run 33461311226.
# product ISO firmware IDE cmd HLT 0x20 (IdeBus Start CpuSleep injects IDT 0x20 not ATA 0x76; iron cmd=0x00 ataio=0 pic=0).
# do not F11 c0c9810 / --run 33440951898.
# do not F11 3ff3cf9 / --run 33443188019.
# do not F11 deb64f5 / --run 33444677681.
# do not F11 a83c51c / --run 33445476540.
# do not F11 2b1433f / --run 33446918467.
# do not F11 61eef92 / --run 33448452364.
# do not F11 05938ac / --run 33449291916.
# do not F11 fe05f78 / --run 33450139765.
# do not F11 74ba1de / --run 33451734183.
# do not F11 77d84d3 / --run 33452659198.
# retrigger 0d36b53 after nested ATAPI miss (33437881901 ataio=0 packet=0).
# flash 3b7bbac is not F11. do not F11 3b7bbac / --run 33433126839.
# flash e4faceb is not F11. do not F11 e4faceb / --run 33429494930.
# retrigger 5a69de2 after nested-KVM kill-init (33430294210 iso=0 after GTIMER2).
# firmware OVMF ATA vector. do not clobber IOAPIC ATA vector. do not inject leftover 0x2E.
# do not clobber PIC ICW2. PIC ATA vector follows ICW2.
# flash d7d63ca is not F11. do not F11 d7d63ca / --run 33426291731.
# firmware PIC ATA: take PIC 0x2E when the 8259 can deliver it.
# flash 8e581c7 is not F11. do not F11 8e581c7 / --run 33424573770.
# IOAPIC edge no remote IRR: PACKET after IDENTIFY without IOAPIC EOI.
# flash 30b78a0 is not F11. do not F11 30b78a0 / --run 33422323257.
# firmware take IOAPIC ATA: do not latch virtio/UART into IRR that ata_irr_only
# will not inject. flash 0bb06a2 is not F11. do not F11 0bb06a2 / --run 33418246409.
# firmware ATA IRR only: do not take_highest_irr LVT 0xEF before PACKET.
# flash 12926eb is not F11. do not F11 12926eb / --run 33415083012.
# retrigger cdbee39 after nested-KVM kill-init (33417361559 iso=0 after GTIMER2).
# Do not F11 eaa580d / --run 33413425759 (same-cycle only). Do not F11 bce5bbb
# / --run 33411580450 (prefer ATA IRR; PIC IRQ 0 starves 0x2E). Do not F11 489d938
# / --run 33408594472 (TPR-stuck 0x2E). wait_for_irq stays false.
# 5227ad9 force-IF cannot inject 0x2E. Do not F11 5227ad9 / --run 33404368817.
# retrigger 9df52c5 after nested-KVM SHELL flake (33402411199 iso=0 5/5).
# do not F11 77f5866
# / 388149b / --run 33399209557 / 33399991049 (skip-PIT IF=0 after PACKET).
# flash 77f5866 is not F11. firmware skip PIT inject.
# skip-without-inject plus El Torito ide@ first plus skip HLT after PACKET
# plus ATA 14 / virtio INTx (not PIT 0x20) plus IF after ataio.
# do not F11 e70a295 / --run 33397104645 (skip-without-inject blocked ATA 14).
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
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 5227ad9 / --run 33404368817." >&2
    echo "       do not F11 489d938 / --run 33408594472." >&2
    echo "       do not F11 77f5866 / --run 33399209557." >&2
    echo "       firmware HLT skip after ataio; flash e70a295 / --run 33397104645." >&2
    echo "       do not F11 e70a295." >&2
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
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 5227ad9 / --run 33404368817." >&2
    echo "       do not F11 489d938 / --run 33408594472." >&2
    echo "       do not F11 77f5866 / --run 33399209557." >&2
    echo "       firmware HLT skip after ataio; flash e70a295 / --run 33397104645." >&2
    echo "       do not F11 e70a295." >&2
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
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 5227ad9 / --run 33404368817." >&2
    echo "       do not F11 489d938 / --run 33408594472." >&2
    echo "       do not F11 77f5866 / --run 33399209557." >&2
    echo "       firmware HLT skip after ataio; flash e70a295 / --run 33397104645." >&2
    echo "       do not F11 e70a295." >&2
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
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 5227ad9 / --run 33404368817." >&2
    echo "       do not F11 489d938 / --run 33408594472." >&2
    echo "       do not F11 77f5866 / --run 33399209557." >&2
    echo "       firmware HLT skip after ataio; flash e70a295 / --run 33397104645." >&2
    echo "       do not F11 e70a295." >&2
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
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 5227ad9 / --run 33404368817." >&2
    echo "       do not F11 489d938 / --run 33408594472." >&2
    echo "       do not F11 77f5866 / --run 33399209557." >&2
    echo "       firmware HLT skip after ataio; flash e70a295 / --run 33397104645." >&2
    echo "       do not F11 e70a295." >&2
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
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 5227ad9 / --run 33404368817." >&2
    echo "       do not F11 489d938 / --run 33408594472." >&2
    echo "       do not F11 77f5866 / --run 33399209557." >&2
    echo "       firmware HLT skip after ataio; flash e70a295 / --run 33397104645." >&2
    echo "       do not F11 e70a295." >&2
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
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 5227ad9 / --run 33404368817." >&2
    echo "       do not F11 489d938 / --run 33408594472." >&2
    echo "       do not F11 77f5866 / --run 33399209557." >&2
    echo "       firmware HLT skip after ataio; flash e70a295 / --run 33397104645." >&2
    echo "       do not F11 e70a295." >&2
    echo "       firmware HLT skip without inject; flash 56f31d3 / --run 33392055961." >&2
    echo "       do not F11 56f31d3." >&2
    echo "       do not F11 90da03d." >&2
    echo "       do not F11 ea30da1." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33391068937" ]]; then
    echo "error: run 33391068937 is a2acfc8 n>16384 after hide-IDE timer ISR" >&2
    echo "       product ISO fw_cfg bootorder El Torito ide@ first; flash 90da03d / --run 33394776080." >&2
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 5227ad9 / --run 33404368817." >&2
    echo "       do not F11 489d938 / --run 33408594472." >&2
    echo "       do not F11 77f5866 / --run 33399209557." >&2
    echo "       firmware HLT skip after ataio; flash e70a295 / --run 33397104645." >&2
    echo "       do not F11 e70a295." >&2
    echo "       firmware HLT skip without inject; flash 56f31d3 / --run 33392055961." >&2
    echo "       do not F11 56f31d3." >&2
    echo "       do not F11 90da03d." >&2
    echo "       do not F11 a2acfc8 / ea30da1." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33392055961" ]]; then
    echo "error: run 33392055961 is 56f31d3 scsi@3 first with no El Torito boot option" >&2
    echo "       product ISO fw_cfg bootorder El Torito ide@ first; flash 90da03d / --run 33394776080." >&2
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 5227ad9 / --run 33404368817." >&2
    echo "       do not F11 489d938 / --run 33408594472." >&2
    echo "       do not F11 77f5866 / --run 33399209557." >&2
    echo "       firmware HLT skip after ataio; flash e70a295 / --run 33397104645." >&2
    echo "       do not F11 e70a295." >&2
    echo "       firmware HLT skip without inject; flash 56f31d3 / --run 33392055961." >&2
    echo "       do not F11 56f31d3." >&2
    echo "       do not F11 90da03d." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33394776080" ]]; then
    echo "error: run 33394776080 is 90da03d skip-after-inject ataio==0 parks PACKET HLT" >&2
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 5227ad9 / --run 33404368817." >&2
    echo "       do not F11 489d938 / --run 33408594472." >&2
    echo "       do not F11 77f5866 / --run 33399209557." >&2
    echo "       firmware HLT skip after ataio; flash e70a295 / --run 33397104645." >&2
    echo "       do not F11 e70a295." >&2
    echo "       product ISO fw_cfg bootorder El Torito ide@ first; flash 90da03d / --run 33394776080." >&2
    echo "       do not F11 90da03d." >&2
    echo "       do not F11 56f31d3." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33397104645" ]]; then
    echo "error: run 33397104645 is e70a295 skip-without-inject blocked ATA 14" >&2
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 5227ad9 / --run 33404368817." >&2
    echo "       do not F11 489d938 / --run 33408594472." >&2
    echo "       do not F11 77f5866 / --run 33399209557." >&2
    echo "       firmware HLT skip after ataio; flash e70a295 / --run 33397104645." >&2
    echo "       do not F11 e70a295." >&2
    echo "       do not F11 90da03d." >&2
    echo "       do not F11 56f31d3." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33399209557" ]]; then
    echo "error: run 33399209557 is 77f5866 skip-PIT IF=0 after PACKET" >&2
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 5227ad9 / --run 33404368817." >&2
    echo "       do not F11 489d938 / --run 33408594472." >&2
    echo "       do not F11 77f5866 / --run 33399209557." >&2
    echo "       do not F11 e70a295." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33399991049" ]]; then
    echo "error: run 33399991049 is 388149b pin of 77f5866 skip-PIT IF=0 after PACKET" >&2
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 5227ad9 / --run 33404368817." >&2
    echo "       do not F11 489d938 / --run 33408594472." >&2
    echo "       do not F11 77f5866 / --run 33399209557." >&2
    echo "       do not F11 e70a295." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33402411199" ]]; then
    echo "error: run 33402411199 is 9df52c5 nested-KVM SHELL flake 5/5" >&2
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 5227ad9 / --run 33404368817." >&2
    echo "       do not F11 489d938 / --run 33408594472." >&2
    echo "       do not F11 77f5866 / --run 33399209557." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33417361559" ]]; then
    echo "error: run 33417361559 is cdbee39 nested-KVM kill-init after GTIMER2" >&2
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 never takes try_inject. Do not F11 cdbee39 / --run 33417361559." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33430294210" ]]; then
    echo "error: run 33430294210 is 5a69de2 nested-KVM kill-init after GTIMER2" >&2
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 never takes try_inject. Do not F11 5a69de2 / --run 33430294210." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33433126839" ]]; then
    echo "error: run 33433126839 is 3b7bbac PIC ICW2 clobber (IRQ 14 0x26)" >&2
    echo "       do not clobber PIC ICW2; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 3b7bbac / --run 33433126839." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33435849693" ]]; then
    echo "error: run 33435849693 is 010403c host-test fail (residual needle)" >&2
    echo "       do not clobber PIC ICW2; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 010403c / --run 33435849693." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33436232227" ]]; then
    echo "error: run 33436232227 is a14223f superseded (missing ICW2-follows + insn_len 0 skip)" >&2
    echo "       firmware HLT insn_len 0 skip; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 a14223f / --run 33436232227." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33436822494" ]]; then
    echo "error: run 33436822494 is 4730397 pin of a14223f (superseded F11)" >&2
    echo "       firmware HLT insn_len 0 skip; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 4730397 / --run 33436822494." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33437881901" ]]; then
    echo "error: run 33437881901 is 0d36b53 nested ATAPI miss (ataio=0 packet=0)" >&2
    echo "       PIC ATA vector follows ICW2; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 0d36b53 / --run 33437881901." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33438918646" ]]; then
    echo "error: run 33438918646 is 9299888 retrigger nested ATAPI miss (ataio=0 packet=0)" >&2
    echo "       firmware HLT insn_len 0 skip; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 9299888 / --run 33438918646." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33440951898" ]]; then
    echo "error: run 33440951898 is c0c9810 pin-docs nested ATAPI miss (ataio=0)" >&2
    echo "       nested iso=0 firmware HLT PIT; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 c0c9810 / --run 33440951898." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33443188019" ]]; then
    echo "error: run 33443188019 is 3ff3cf9 nested PIT VMXON-SKIP (not ATAPI-OK)" >&2
    echo "       nested iso=0 EDK2 IRQ0; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 3ff3cf9 / --run 33443188019." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33444677681" ]]; then
    echo "error: run 33444677681 is deb64f5 nested EDK2 IRQ0 VMXON-SKIP (not ATAPI-OK)" >&2
    echo "       nested iso=0 firmware LAPIC timer; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 deb64f5 / --run 33444677681." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33445476540" ]]; then
    echo "error: run 33445476540 is a83c51c nested LAPIC VMXON-SKIP (not ATAPI-OK)" >&2
    echo "       product ISO firmware HLT wake; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 a83c51c / --run 33445476540." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33446918467" ]]; then
    echo "error: run 33446918467 is 2b1433f product firmware HLT wake VMXON-SKIP (not ATAPI-OK)" >&2
    echo "       nested iso=0 firmware HLT ATA; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 2b1433f / --run 33446918467." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33448452364" ]]; then
    echo "error: run 33448452364 is 61eef92 nested ATA VMXON-SKIP (not ATAPI-OK)" >&2
    echo "       product ISO firmware HLT ATA; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 61eef92 / --run 33448452364." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33449291916" ]]; then
    echo "error: run 33449291916 is 05938ac product HLT ATA VMXON-SKIP (not ATAPI-OK)" >&2
    echo "       product ISO firmware HLT ATA IOAPIC; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 05938ac / --run 33449291916." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33450139765" ]]; then
    echo "error: run 33450139765 is fe05f78 product ATA IOAPIC VMXON-SKIP (not ATAPI-OK)" >&2
    echo "       nested iso=0 firmware HLT ATA LAPIC; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 fe05f78 / --run 33450139765." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33451734183" ]]; then
    echo "error: run 33451734183 is 74ba1de nested ATA LAPIC VMXON-SKIP (not ATAPI-OK)" >&2
    echo "       product ISO firmware HLT ATA LAPIC; product ISO firmware HLT wake LAPIC;" >&2
    echo "       flash b5c3a9c / --run 33440050729 (iron COM2 ataio=0 inj=0; do not re-flash)." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 74ba1de / --run 33451734183." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33452659198" ]]; then
    echo "error: run 33452659198 is 77d84d3 wake+ATA LAPIC VMXON-SKIP (not ATAPI-OK)" >&2
    echo "       firmware HLT skip only after inject; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 77d84d3 / --run 33452659198." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33453324709" ]]; then
    echo "error: run 33453324709 is c4cd522 skip-only-after-inject VMXON-SKIP (not ATAPI-OK)" >&2
    echo "       product ISO firmware HLT wake LAPIC timer; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 c4cd522 / --run 33453324709." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33454130069" ]]; then
    echo "error: run 33454130069 is 37320ad wake LAPIC timer VMXON-SKIP (not ATAPI-OK)" >&2
    echo "       product ISO firmware HLT wake IDT 0x20; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 37320ad / --run 33454130069." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33454767329" ]]; then
    echo "error: run 33454767329 is d454545 wake IDT 0x20 VMXON-SKIP (not ATAPI-OK)" >&2
    echo "       product ISO firmware HLT wake IDT 0x20 only; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 d454545 / --run 33454767329." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33455373334" ]]; then
    echo "error: run 33455373334 is f37674f wake IDT 0x20 only VMXON-SKIP (not ATAPI-OK)" >&2
    echo "       product ISO firmware HLT wake LVT unmask; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 f37674f / --run 33455373334." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33455903058" ]]; then
    echo "error: run 33455903058 is 91f15b3 LVT unmask VMXON-SKIP (not ATAPI-OK)" >&2
    echo "       product ISO firmware LVT timer inject; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 91f15b3 / --run 33455903058." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33456465331" ]]; then
    echo "error: run 33456465331 is 8f04fa6 LVT timer inject VMXON-SKIP (not ATAPI-OK)" >&2
    echo "       product ISO firmware wake preempt; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 8f04fa6 / --run 33456465331." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33457132491" ]]; then
    echo "error: run 33457132491 is 1b758d2 wake preempt VMXON-SKIP (not ATAPI-OK)" >&2
    echo "       product ISO firmware wake Delay I/O; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 1b758d2 / --run 33457132491." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33458084140" ]]; then
    echo "error: run 33458084140 is ce11fda Delay I/O VMXON-SKIP (not ATAPI-OK)" >&2
    echo "       product ISO firmware IDE cmd reset 0; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 ce11fda / --run 33458084140." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33459130885" ]]; then
    echo "error: run 33459130885 is 8851af8 residual VMXON-SKIP (not ATAPI-OK)" >&2
    echo "       product ISO firmware IDE cmd inject ATA; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 8851af8 / --run 33459130885." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33459800906" ]]; then
    echo "error: run 33459800906 is 7d02e96 IDE cmd reset 0 VMXON-SKIP (not ATAPI-OK)" >&2
    echo "       product ISO firmware IDE cmd inject ATA; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 7d02e96 / --run 33459800906." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33460343555" ]]; then
    echo "error: run 33460343555 is 72885fa IDE cmd ATA IRQ VMXON-SKIP (not ATAPI-OK)" >&2
    echo "       product ISO firmware IDE cmd ATA on HLT; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 72885fa / --run 33460343555." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33460640154" ]]; then
    echo "error: run 33460640154 is 244750c IDE cmd inject ATA VMXON-SKIP (not ATAPI-OK)" >&2
    echo "       product ISO firmware IDE cmd I/O no inject; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 244750c / --run 33460640154." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33461311226" ]]; then
    echo "error: run 33461311226 is 2d64091 IDE cmd ATA on HLT VMXON-SKIP (not ATAPI-OK)" >&2
    echo "       product ISO firmware no preempt inject; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 2d64091 / --run 33461311226." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33461867968" ]]; then
    echo "error: run 33461867968 is c7e4638 IDE cmd I/O no inject VMXON-SKIP (not ATAPI-OK)" >&2
    echo "       product ISO firmware no preempt inject; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 c7e4638 / --run 33461867968." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33462312015" ]]; then
    echo "error: run 33462312015 is 90569fd IDE cmd HLT 0x20 VMXON-SKIP (not ATAPI-OK)" >&2
    echo "       product ISO firmware Delay I/O no inject; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 90569fd / --run 33462312015." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33462988233" ]]; then
    echo "error: run 33462988233 is b670993 no preempt inject VMXON-SKIP (not ATAPI-OK)" >&2
    echo "       product ISO firmware no LVT inject I/O; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 b670993 / --run 33462988233." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33463983585" ]]; then
    echo "error: run 33463983585 is 89bba8f docs/HDA VMXON-SKIP (not ATAPI-OK)" >&2
    echo "       product ISO firmware no LVT inject I/O; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 89bba8f / --run 33463983585." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33463955237" ]]; then
    echo "error: run 33463955237 is 4b11843 Verus curl retry VMXON-SKIP (not ATAPI-OK)" >&2
    echo "       product ISO firmware no LVT inject I/O; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 4b11843 / --run 33463955237." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33463584633" ]]; then
    echo "error: run 33463584633 is 4e98f27 Delay I/O no inject M4.8 curl 35 (not ATAPI-OK)" >&2
    echo "       product ISO firmware no LVT inject I/O; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 4e98f27 / --run 33463584633." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33464757885" ]]; then
    echo "error: run 33464757885 is 7f199db nested VMXON inject 0x20 timer ISR (ATAPI-OK missing)" >&2
    echo "       nested iso=0 firmware HLT no PIT inject; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 7f199db / --run 33464757885." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33465649406" ]]; then
    echo "error: run 33465649406 is 739eb8a nested no PIT inject VMXON-SKIP (not ATAPI-OK)" >&2
    echo "       nested iso=0 firmware HLT EDK2 0x68; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 739eb8a / --run 33465649406." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33466397855" ]]; then
    echo "error: run 33466397855 is 13052e7 nested EDK2 0x68 VMXON-SKIP (not ATAPI-OK)" >&2
    echo "       product ISO firmware HLT EDK2 0x68; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 13052e7 / --run 33466397855." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33466890874" ]]; then
    echo "error: run 33466890874 is dd0096b nested VMXON inject 0x68 CR livelock (ATAPI-OK missing)" >&2
    echo "       nested iso=0 firmware HLT 0x68 miss; firmware HLT inject cap; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 dd0096b / --run 33466890874." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33468177902" ]]; then
    echo "error: run 33468177902 is 65b94c1 nested VMXON 8x inject 0x20 then skip-HLT (ATAPI-OK missing)" >&2
    echo "       nested iso=0 firmware HLT skip after inject; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 65b94c1 / --run 33468177902." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33469144799" ]]; then
    echo "error: run 33469144799 is cfabb62 nested VMXON skip-after-inject 8x 0x20 hlt=0 (ATAPI-OK missing)" >&2
    echo "       nested iso=0 firmware HLT inject cap; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 cfabb62 / --run 33469144799." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33470144235" ]]; then
    echo "error: run 33470144235 is ee90aad nested VMXON-SKIP (not ATAPI-OK)" >&2
    echo "       guest-UEFI stop inj; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 ee90aad / --run 33470144235." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33470837613" ]]; then
    echo "error: run 33470837613 is e416806 nested VMXON inj=1487 CPUID livelock (ATAPI-OK missing)" >&2
    echo "       nested iso=0 firmware HLT skip after cap; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 e416806 / --run 33470837613." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33471631130" ]]; then
    echo "error: run 33471631130 is 1c7ff1c nested VMXON-SKIP skip-after-cap (not ATAPI-OK)" >&2
    echo "       nested iso=0 firmware HLT PM1 SCI; flash b5c3a9c / --run 33440050729." >&2
    echo "       iso=0 E4 SHELL held. Do not F11 1c7ff1c / --run 33471631130." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33473305422" ]]; then
    echo "error: run 33473305422 is 68aff41 nested VMXON-SKIP (SCI unproven; not ATAPI-OK)" >&2
    echo "       i440FX slot-0 Header Type single function; flash b5c3a9c / --run 33440050729." >&2
    echo "       nested iso=0 firmware IdeBus PCI. Do not F11 68aff41 / --run 33473305422." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33429494930" ]]; then
    echo "error: run 33429494930 is e4faceb leftover IOAPIC 0x2E after PIC remap" >&2
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 e4faceb / --run 33429494930." >&2
    echo "       do not F11 d7d63ca / --run 33426291731." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33431369645" ]]; then
    echo "error: run 33431369645 is 25e6596 retrigger of 5a69de2 leftover 0x2E EFI" >&2
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 e4faceb / --run 33429494930." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33418246409" ]]; then
    echo "error: run 33418246409 is 0bb06a2 ATA IRR only without take IOAPIC ATA" >&2
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 0bb06a2 / --run 33418246409." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33419049836" ]]; then
    echo "error: run 33419049836 is 6498158 pin of 0bb06a2 ATA IRR only" >&2
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 0bb06a2 / --run 33418246409." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33422323257" ]]; then
    echo "error: run 33422323257 is 30b78a0 take IOAPIC ATA with edge remote IRR" >&2
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 30b78a0 / --run 33422323257." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33422962160" ]]; then
    echo "error: run 33422962160 is 8a125b9 pin of 30b78a0 edge remote IRR" >&2
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 30b78a0 / --run 33422323257." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33424452815" ]]; then
    echo "error: run 33424452815 is ff1faeb host-test fail before PIC IRQ 14 assert" >&2
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 ff1faeb / --run 33424452815." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33424573770" ]]; then
    echo "error: run 33424573770 is 8e581c7 PIC unmask never reached take_pic" >&2
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 8e581c7 / --run 33424573770." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33425259123" ]]; then
    echo "error: run 33425259123 is d9b062d pin of 8e581c7 PIC unmask dead in try_inject" >&2
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 8e581c7 / --run 33424573770." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33426291731" ]]; then
    echo "error: run 33426291731 is d7d63ca PIC ATA that clobbers IOAPIC to 0x2E" >&2
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 d7d63ca / --run 33426291731." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33427171899" ]]; then
    echo "error: run 33427171899 is 6457ec2 pin of d7d63ca IOAPIC 0x2E clobber" >&2
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 d7d63ca / --run 33426291731." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33415083012" ]]; then
    echo "error: run 33415083012 is 12926eb take_highest_irr LVT 0xEF before PACKET" >&2
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 12926eb / --run 33415083012." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33415638447" ]]; then
    echo "error: run 33415638447 is 6792eb7 pin of 12926eb LVT 0xEF fallthrough" >&2
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 12926eb / --run 33415083012." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33404368817" ]]; then
    echo "error: run 33404368817 is 5227ad9 force-IF with pin 14 still masked" >&2
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 5227ad9 / --run 33404368817." >&2
    echo "       do not F11 489d938 / --run 33408594472." >&2
    echo "       do not F11 77f5866 / --run 33399209557." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33405102333" ]]; then
    echo "error: run 33405102333 is 807831c pin of 5227ad9 force-IF pin 14 masked" >&2
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 5227ad9 / --run 33404368817." >&2
    echo "       do not F11 489d938 / --run 33408594472." >&2
    echo "       do not F11 77f5866 / --run 33399209557." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33408594472" ]]; then
    echo "error: run 33408594472 is 489d938 arm GSI 14 with TPR-stuck 0x2E" >&2
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 489d938 / --run 33408594472." >&2
    echo "       do not F11 5227ad9 / --run 33404368817." >&2
    echo "       do not F11 77f5866 / --run 33399209557." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33409711971" ]]; then
    echo "error: run 33409711971 is 6b94350 pin of 489d938 TPR-stuck 0x2E" >&2
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 489d938 / --run 33408594472." >&2
    echo "       do not F11 5227ad9 / --run 33404368817." >&2
    echo "       do not F11 77f5866 / --run 33399209557." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33411580450" ]]; then
    echo "error: run 33411580450 is bce5bbb prefer ATA IRR; PIC IRQ 0 starves 0x2E" >&2
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 bce5bbb / --run 33411580450." >&2
    echo "       do not F11 489d938 / --run 33408594472." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33412462849" ]]; then
    echo "error: run 33412462849 is fcad250 pin of bce5bbb PIC-starve 0x2E" >&2
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 bce5bbb / --run 33411580450." >&2
    echo "       do not F11 489d938 / --run 33408594472." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33413425759" ]]; then
    echo "error: run 33413425759 is eaa580d ATA over PIC without latched 0x2E" >&2
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 eaa580d / --run 33413425759." >&2
    echo "       do not F11 bce5bbb / --run 33411580450." >&2
    exit 1
  fi
  if [[ "$PIN_RUN" == "33414038523" ]]; then
    echo "error: run 33414038523 is 5fdcafa pin of eaa580d same-cycle only" >&2
    echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
    echo "       do not F11 eaa580d / --run 33413425759." >&2
    echo "       do not F11 bce5bbb / --run 33411580450." >&2
    exit 1
  fi
  case "$HEAD_SHORT" in
    2d6b109*|8663f56*|084430f*|2ae4544*|5c0f7a2*|d61dc7e*|b824789*|2cf313e*|ea30da1*|c587ba7*|a2acfc8*|56f31d3*|b8a726d*|90da03d*|82c0fd4*|e70a295*|0541ef0*|77f5866*|388149b*|9df52c5*|5227ad9*|807831c*|489d938*|6b94350*|bce5bbb*|fcad250*|eaa580d*|5fdcafa*|12926eb*|6792eb7*|cdbee39*|0bb06a2*|6498158*|30b78a0*|8a125b9*|ff1faeb*|8e581c7*|d9b062d*|d7d63ca*|6457ec2*)
      echo "error: HEAD $HEAD_SHORT is not the F11 pin" >&2
      echo "       firmware OVMF ATA vector; flash b5c3a9c / --run 33440050729." >&2
      echo "       do not F11 eaa580d / --run 33413425759." >&2
      echo "       do not F11 bce5bbb / --run 33411580450." >&2
      echo "       do not F11 5227ad9 / --run 33404368817." >&2
      echo "       do not F11 489d938 / --run 33408594472." >&2
      echo "       do not F11 77f5866 / --run 33399209557." >&2
      echo "       firmware HLT skip after ataio; flash e70a295 / --run 33397104645." >&2
      echo "       do not F11 e70a295." >&2
      echo "       product ISO fw_cfg bootorder El Torito ide@ first; flash 90da03d / --run 33394776080." >&2
      echo "       firmware HLT skip without inject; flash 56f31d3 / --run 33392055961." >&2
      echo "       do not F11 56f31d3." >&2
      echo "       do not F11 90da03d." >&2
      echo "       product ISO HLT stall before n=16384; do not F11 ea30da1." >&2
      echo "       do not F11 ea30da1 / a2acfc8 / --run 33389381409 / 33391068937." >&2
      echo "       do not checkout cursor/e5-stage46-iso-a623 for F11." >&2
      echo "       git checkout -B cursor/e5-pm1-sci-a623 origin/cursor/e5-pm1-sci-a623" >&2
      echo "       ./tools/flashcruzer.sh --no-git --run 33440050729 --linux-iso ..." >&2
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
