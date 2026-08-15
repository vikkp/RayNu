#!/usr/bin/env bash
# One-shot: pull tip, prove source has stamp, clean-rebuild, pack, verify floppy.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

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

echo "==> clean + rebuild EFI"
cargo clean -p r640-hypervisor
./tools/build.sh
EFI=target/x86_64-unknown-uefi/release/r640-hypervisor.efi
EFI_SHA="$(shasum -a 256 "$EFI" | awk '{print $1}')"
echo "==> built efi_sha256=${EFI_SHA}"

if ! strings "$EFI" | grep -Fq 'E2 marker build=r640-boot-ok-marker'; then
  echo "error: built EFI missing stamp string — wrong tree or stale artifact" >&2
  strings "$EFI" | grep -F 'R640-BOOT' || true
  exit 1
fi
echo "==> built EFI contains stamp"

./tools/package-release.sh
./tools/make-boot-media.sh --kit dist/raynu-v-0.1.0
./tools/verify-boot-img-marker.sh

echo
echo "Remap iDRAC to: dist/raynu-v-0.1.0-boot-media/raynu-v-0.1.0-uefi-boot.img"
echo "Expect after VMXOFF:"
echo "  boot: E2 marker build=r640-boot-ok-marker"
echo "  RAYNU-V-R640-BOOT-OK"
