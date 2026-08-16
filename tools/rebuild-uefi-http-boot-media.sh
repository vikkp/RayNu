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
  'falling back to SNP residual' \
  'PRE-EBS SNP window' \
  'RAYNU-V-R640-BOOT-OK'
do
  if ! efi_has "$EFI" "$needle"; then
    echo "error: built EFI missing bytes: $needle" >&2
    exit 1
  fi
done
echo "==> built EFI contains M7.6 Tcp4+SNP residual + R640-BOOT-OK markers"

# Refresh dist/ so make-boot-media --kit cannot pack the old xsavesfix EFI.
SKIP_BUILD=1 ./tools/package-release.sh
./tools/make-boot-media.sh --kit dist/raynu-v-0.1.0
./tools/verify-boot-img-uefi-http.sh

IMG="$(ls -1t dist/*/raynu-v-*-uefi-boot.img dist/*-uefi-boot.img 2>/dev/null | head -1)"
echo
echo "Remap iDRAC Virtual Floppy to:"
echo "  ${IMG}"
echo "Expect on COM2 before ExitBootServices:"
echo "  boot: uefi-net probe snp=… mnp=… ip4=… dhcp4=… tcp4=… pci=…"
echo "  boot: uefi-net connect — starting PCI/UNDI drivers   # if tcp4 was 0"
echo "  boot: uefi-net after-pci …                           # re-probe"
echo "  boot: uefi-net after-snp …                           # if snp>0 still no tcp4"
echo "  boot: falling back to SNP residual (ADR-012)         # if tcp4=0"
echo "  boot: SNP DHCP discover…"
echo "  boot: mgmt HTTP listening on a.b.c.d:8443 (PRE-EBS SNP window)"
echo "  boot: SNP lease a.b.c.d/nn router=…"
echo "  boot: CURL NOW → http://a.b.c.d:8443/"
echo "  boot: PING NOW (same LAN as HOST NIC, not iDRAC) before curl"
echo "  boot: SNP listen window_ms=45000"
echo "During the SNP window (~45s after bind), from a laptop on the SAME subnet:"
echo "  ping -c 2 HOST_NIC_IP"
echo "  curl -sS --connect-timeout 2 http://HOST_NIC_IP:8443/"
echo "  curl -sS -H 'Authorization: Bearer raynu-v-bringup' http://HOST_NIC_IP:8443/vms"
echo "After a successful curl exchange:"
echo "  RAYNU-V-M7-UEFI-HTTP-OK"
echo "Soft-fail still ends with RAYNU-V-R640-BOOT-OK (E2)."
echo "HOST NIC IP ≠ iDRAC IP (iron example: 10.99.99.127)."
echo "If ping fails: Mac is not on the LOM/DHCP subnet — fix LAN path first."
