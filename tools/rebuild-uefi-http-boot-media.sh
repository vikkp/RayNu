#!/usr/bin/env bash
# One-shot: pull main, prove M7.6 source, clean-rebuild, pack, verify floppy.
# Avoids the stale-dist trap (make-boot-media prefers dist/ over a fresh target).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

efi_has() {
  # Byte search — macOS strings|grep is unreliable on PE/COFF blobs.
  python3 -c 'import sys; d=open(sys.argv[1],"rb").read(); sys.exit(0 if sys.argv[2].encode() in d else 1)' "$1" "$2"
}

echo "==> fetch + checkout main"
git fetch origin
git checkout main
git pull origin main
echo "==> HEAD=$(git rev-parse --short HEAD) $(git log -1 --pretty=%s)"

if ! grep -Fq 'run_pre_ebs_mgmt_listen' src/main.rs; then
  echo "error: source missing run_pre_ebs_mgmt_listen — pull did not land M7.6 tip" >&2
  echo "       need main >= 5b3a6ce (Merge M7.6 PRE-EBS UEFI HTTP scaffold)" >&2
  exit 1
fi
if ! grep -Fq 'RAYNU-V-M7-UEFI-HTTP-OK' mgmt/http_listen.rs; then
  echo "error: source missing RAYNU-V-M7-UEFI-HTTP-OK marker string" >&2
  exit 1
fi
echo "==> source M7.6 OK (main.rs + http_listen.rs)"

echo "==> clean UEFI release artifacts + rebuild"
rm -rf target/x86_64-unknown-uefi/release
./tools/build.sh
EFI=target/x86_64-unknown-uefi/release/r640-hypervisor.efi
EFI_SHA="$(shasum -a 256 "$EFI" | awk '{print $1}')"
echo "==> built efi_sha256=${EFI_SHA}"
echo "==> built mtime=$(stat -f '%Sm' -t '%Y-%m-%d %H:%M:%S' "$EFI" 2>/dev/null || stat -c '%y' "$EFI")"

for needle in \
  'RAYNU-V-M7-UEFI-HTTP-OK' \
  'mgmt HTTP listening' \
  'PRE-EBS Tcp4 window' \
  'RAYNU-V-R640-BOOT-OK'
do
  if ! efi_has "$EFI" "$needle"; then
    echo "error: built EFI missing bytes: $needle" >&2
    exit 1
  fi
done
echo "==> built EFI contains M7.6 + R640-BOOT-OK markers"

# Refresh dist/ so make-boot-media --kit cannot pack the old xsavesfix EFI.
SKIP_BUILD=1 ./tools/package-release.sh
./tools/make-boot-media.sh --kit dist/raynu-v-0.1.0
./tools/verify-boot-img-uefi-http.sh

IMG="$(ls -1t dist/*/raynu-v-*-uefi-boot.img dist/*-uefi-boot.img 2>/dev/null | head -1)"
echo
echo "Remap iDRAC Virtual Floppy to:"
echo "  ${IMG}"
echo "Expect on COM2 before ExitBootServices:"
echo "  boot: mgmt HTTP listening on 0.0.0.0:8443 (PRE-EBS Tcp4 window)"
echo "After a successful curl exchange:"
echo "  RAYNU-V-M7-UEFI-HTTP-OK"
