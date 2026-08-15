#!/usr/bin/env bash
# One-shot: pull tip, prove source has stamp, clean-rebuild, pack, verify floppy.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

efi_has() {
  # Byte search — macOS strings|grep is unreliable on PE/COFF blobs.
  python3 -c 'import sys; d=open(sys.argv[1],"rb").read(); sys.exit(0 if sys.argv[2].encode() in d else 1)' "$1" "$2"
}

echo "==> fetch + checkout marker branch"
git fetch origin
git checkout cursor/r640-boot-ok-marker-a623
git pull origin cursor/r640-boot-ok-marker-a623
echo "==> HEAD=$(git rev-parse --short HEAD) $(git log -1 --pretty=%s)"

STAMP='boot: E2 marker build=r640-boot-ok-marker'
if ! grep -Fq "$STAMP" vmx/launch.rs; then
  echo "error: source missing stamp — pull did not land tip (need >= 1b1c87c)" >&2
  exit 1
fi
echo "==> source stamp OK in vmx/launch.rs"

echo "==> clean UEFI release artifacts + rebuild"
rm -rf target/x86_64-unknown-uefi/release
./tools/build.sh
EFI=target/x86_64-unknown-uefi/release/r640-hypervisor.efi
EFI_SHA="$(shasum -a 256 "$EFI" | awk '{print $1}')"
echo "==> built efi_sha256=${EFI_SHA}"
echo "==> built mtime=$(stat -f '%Sm' -t '%Y-%m-%d %H:%M:%S' "$EFI" 2>/dev/null || stat -c '%y' "$EFI")"

if ! efi_has "$EFI" 'E2 marker build=r640-boot-ok-marker'; then
  echo "error: built EFI missing stamp bytes" >&2
  exit 1
fi
if ! efi_has "$EFI" 'RAYNU-V-R640-BOOT-OK'; then
  echo "error: built EFI missing RAYNU-V-R640-BOOT-OK" >&2
  exit 1
fi
echo "==> built EFI contains stamp + R640-BOOT-OK"

SKIP_BUILD=1 ./tools/package-release.sh
./tools/make-boot-media.sh --kit dist/raynu-v-0.1.0
./tools/verify-boot-img-marker.sh

echo
echo "Remap iDRAC to: dist/raynu-v-0.1.0-boot-media/raynu-v-0.1.0-uefi-boot.img"
echo "Expect after VMXOFF:"
echo "  boot: E2 marker build=r640-boot-ok-marker"
echo "  RAYNU-V-R640-BOOT-OK"
