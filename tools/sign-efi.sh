#!/usr/bin/env bash
# ADR-021 A5. Authenticode-sign an EFI for UEFI Secure Boot.
#
# Key decision (2026-11-05): Shipment 1 uses a RayNu-V certificate the
# customer enrolls in the Secure Boot signature database (db). The private
# key is the operator's:
#
#   RAYNU_SIGN_KEY    PEM RSA private key, unencrypted at sign time
#   RAYNU_SIGN_CERT   PEM X.509 certificate (the db enrollment file)
#
# Neither file belongs in this repository. --ephemeral is the CI proof: it
# mints a throwaway key in a temp directory, checks sbverify, and deletes
# the key before returning. It does not replace the input EFI.
#
# A Microsoft-signed shim is the alternative when a customer will not
# enroll a key. That shim is not built here.
#
# This script does not mark A5 lived and does not print an iron marker.
# See docs/runbooks/secure_boot.md.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PE="$ROOT/tools/a5_pe.py"

usage() {
  echo "usage: sign-efi.sh --in EFI (--ephemeral | --key KEY --cert CERT --out EFI)" >&2
  echo "       RAYNU_SIGN_KEY and RAYNU_SIGN_CERT stand in for --key and --cert" >&2
}

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "error: missing $1 (install sbsigntool and openssl)" >&2
    exit 1
  fi
}

sign_copy() {
  local key="$1" cert="$2" src="$3" dst="$4"
  local tmp
  tmp="$(mktemp)"
  # RETURN clears this temp even when sbsign fails. EXIT trap on the
  # ephemeral dir is a different signal and stays in place.
  # shellcheck disable=SC2064
  trap "rm -f '$tmp'" RETURN
  sbsign --key "$key" --cert "$cert" --output "$tmp" "$src"
  sbverify --cert "$cert" "$tmp" >/dev/null
  python3 "$PE" check "$tmp" >/dev/null
  mv "$tmp" "$dst"
}

ephemeral=0
key="${RAYNU_SIGN_KEY:-}"
cert="${RAYNU_SIGN_CERT:-}"
src=""
dst=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --ephemeral) ephemeral=1; shift ;;
    --key) key="${2:-}"; shift 2 ;;
    --cert) cert="${2:-}"; shift 2 ;;
    --in) src="${2:-}"; shift 2 ;;
    --out) dst="${2:-}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *)
      echo "error: unknown argument $1" >&2
      usage
      exit 1
      ;;
  esac
done

need sbsign
need sbverify
need openssl
need python3

if [[ -z "$src" || ! -f "$src" ]]; then
  echo "error: --in must be an existing EFI" >&2
  exit 1
fi

before="$(sha256sum "$src" | awk '{print $1}')"

if [[ "$ephemeral" -eq 1 ]]; then
  if [[ -n "$key" || -n "$cert" ]]; then
    echo "error: --ephemeral does not take a product key" >&2
    exit 1
  fi
  work="$(mktemp -d)"
  trap 'rm -rf "$work"' EXIT
  openssl req -x509 -newkey rsa:2048 \
    -keyout "$work/key.pem" -out "$work/cert.pem" \
    -days 1 -nodes -subj "/CN=RayNu-V A5 ephemeral/" 2>/dev/null
  openssl req -x509 -newkey rsa:2048 \
    -keyout "$work/wrong-key.pem" -out "$work/wrong.pem" \
    -days 1 -nodes -subj "/CN=not-raynu/" 2>/dev/null
  signed="$work/signed.efi"
  sign_copy "$work/key.pem" "$work/cert.pem" "$src" "$signed"
  if sbverify --cert "$work/wrong.pem" "$signed" >/dev/null 2>&1; then
    echo "error: a different certificate verified the signature" >&2
    exit 1
  fi
  if [[ -n "$dst" ]]; then
    cp "$signed" "$dst"
  fi
  after="$(sha256sum "$src" | awk '{print $1}')"
  if [[ "$before" != "$after" ]]; then
    echo "error: --ephemeral modified the input EFI" >&2
    exit 1
  fi
  echo "a5: ephemeral verify ok"
  exit 0
fi

if [[ -z "$key" || -z "$cert" ]]; then
  echo "error: set --key and --cert, or RAYNU_SIGN_KEY and RAYNU_SIGN_CERT" >&2
  exit 1
fi
if [[ -z "$dst" ]]; then
  echo "error: --out is required" >&2
  exit 1
fi
if [[ ! -f "$key" || ! -f "$cert" ]]; then
  echo "error: key or certificate file is missing" >&2
  exit 1
fi

key_pub="$(openssl pkey -in "$key" -pubout 2>/dev/null || true)"
cert_pub="$(openssl x509 -in "$cert" -pubkey -noout 2>/dev/null || true)"
if [[ -z "$key_pub" || "$key_pub" != "$cert_pub" ]]; then
  echo "error: signing key does not match the certificate" >&2
  exit 1
fi

sign_copy "$key" "$cert" "$src" "$dst"
echo "a5: signed $(basename "$dst")"
