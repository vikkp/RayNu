#!/usr/bin/env bash
# ADR-021 A5 host gate. Proves sbsign/sbverify on a throwaway key.
# RAYNU-V-A5-SIGN-HOST-OK is a host artifact. It does not make A5 lived.
# An R640 boot with Secure Boot enabled is the close. See
# docs/runbooks/secure_boot.md.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

for tool in sbsign sbverify openssl python3; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "error: missing $tool (install sbsigntool and openssl)" >&2
    exit 1
  fi
done

if grep -E -q 'BEGIN (RSA |OPENSSH |EC )?PRIVATE KEY' \
  tools/sign-efi.sh tools/a5-sign-smoke.sh tools/a5_pe.py docs/runbooks/secure_boot.md
then
  echo "error: a private key is in the A5 sources" >&2
  exit 1
fi

grep -q 'RAYNU_SIGN_KEY' tools/sign-efi.sh
grep -q 'Status: lived' docs/runbooks/secure_boot.md
grep -q 'db' docs/runbooks/secure_boot.md
grep -q 'shim' docs/runbooks/secure_boot.md

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

python3 tools/a5_pe.py write "$work/tiny.efi"
before="$(sha256sum "$work/tiny.efi" | awk '{print $1}')"
./tools/sign-efi.sh --ephemeral --in "$work/tiny.efi" --out "$work/tiny.signed"
after="$(sha256sum "$work/tiny.efi" | awk '{print $1}')"
if [[ "$before" != "$after" ]]; then
  echo "error: ephemeral sign changed the unsigned EFI" >&2
  exit 1
fi
python3 tools/a5_pe.py check "$work/tiny.signed"

if ./tools/sign-efi.sh --ephemeral --key "$work/nope" --cert "$work/nope" --in "$work/tiny.efi"; then
  echo "error: --ephemeral accepted a product key" >&2
  exit 1
fi

# Operator path: matching key signs; a crossed pair is refused. These files
# live in the temp dir and are removed with it. They are not a product key.
openssl req -x509 -newkey rsa:2048 \
  -keyout "$work/op.key" -out "$work/op.crt" \
  -days 1 -nodes -subj "/CN=RayNu-V A5 operator-test/" 2>/dev/null
openssl req -x509 -newkey rsa:2048 \
  -keyout "$work/other.key" -out "$work/other.crt" \
  -days 1 -nodes -subj "/CN=RayNu-V A5 other/" 2>/dev/null
if RAYNU_SIGN_KEY="$work/op.key" RAYNU_SIGN_CERT="$work/other.crt" \
  ./tools/sign-efi.sh --in "$work/tiny.efi" --out "$work/bad.efi"
then
  echo "error: a key that does not match the certificate was accepted" >&2
  exit 1
fi
RAYNU_SIGN_KEY="$work/op.key" RAYNU_SIGN_CERT="$work/op.crt" \
  ./tools/sign-efi.sh --in "$work/tiny.efi" --out "$work/op.efi"
python3 tools/a5_pe.py check "$work/op.efi"
sbverify --cert "$work/op.crt" "$work/op.efi" >/dev/null

if [[ -n "${A5_EFI:-}" ]]; then
  if [[ ! -f "$A5_EFI" ]]; then
    echo "error: A5_EFI is not a file: $A5_EFI" >&2
    exit 1
  fi
  orig="$(sha256sum "$A5_EFI" | awk '{print $1}')"
  cp "$A5_EFI" "$work/product.efi"
  ./tools/sign-efi.sh --ephemeral --in "$work/product.efi"
  now="$(sha256sum "$A5_EFI" | awk '{print $1}')"
  if [[ "$orig" != "$now" ]]; then
    echo "error: product EFI was modified" >&2
    exit 1
  fi
  echo "a5: product EFI verified; the tree copy stayed unsigned"
fi

echo "RAYNU-V-A5-SIGN-HOST-OK"
echo "==> A5 host sign smoke PASSED (not lived)"
