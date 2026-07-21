#!/usr/bin/env bash
# Interactive helper: verify kit → build iDRAC/USB boot media.
# Thin wrapper around package-release + make-boot-media for operators.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "RayNu-V media maker"
echo "==================="
echo "1) Use existing release kit under dist/raynu-v-* (if present)"
echo "2) Build a new release kit (./tools/package-release.sh)"
echo "3) Quit"
printf "Choice [1/2/3]: "
read -r CHOICE

case "$CHOICE" in
  1)
    if ! ls dist/raynu-v-*/r640-hypervisor.efi >/dev/null 2>&1; then
      echo "error: no kit in dist/ — pick 2 to build one" >&2
      exit 1
    fi
    ;;
  2)
    ./tools/package-release.sh
    ;;
  3) exit 0 ;;
  *)
    echo "error: invalid choice" >&2
    exit 1
    ;;
esac

# Prefer newest versioned kit dir (exclude *-boot-media)
# shellcheck disable=SC2012
KIT="$(ls -1d dist/raynu-v-* 2>/dev/null | grep -v boot-media | sort | tail -1 || true)"
if [[ -z "$KIT" || ! -f "$KIT/r640-hypervisor.efi" ]]; then
  echo "error: could not locate release kit" >&2
  exit 1
fi

echo "==> using kit: $KIT"
./tools/make-boot-media.sh --kit "$KIT"

echo
echo "Next:"
echo "  • iDRAC: map the .img (USB) or .iso (CD) in Virtual Media — preferred"
echo "  • Physical USB: ./tools/make-boot-usb.sh --img <path-to-.img> --disk /dev/diskN"
echo "  • Field guide: docs/runbooks/r640_field_guide.md"
echo "  • Media runbook: docs/runbooks/media_maker.md"
