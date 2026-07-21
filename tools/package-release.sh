#!/usr/bin/env bash
# Package a versioned EFI release kit (M7.0 / ADR-009 / ADR-003).
# Layout:
#   dist/raynu-v-<version>/
#     r640-hypervisor.efi
#     r640-hypervisor.efi.sha256
#     VERSION
#     SHA256SUMS
#     MANIFEST.txt
#   dist/raynu-v-<version>.tar.gz
#   dist/raynu-v-<version>.tar.gz.sha256
#
# Env:
#   SKIP_BUILD=1  — require existing EFI (do not invoke tools/build.sh)
#   PROFILE / TARGET — same as tools/build.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

TARGET="${TARGET:-x86_64-unknown-uefi}"
PROFILE="${PROFILE:-release}"
SKIP_BUILD="${SKIP_BUILD:-0}"
EFI="target/${TARGET}/${PROFILE}/r640-hypervisor.efi"

version_from_cargo() {
  sed -n 's/^version = "\([^"]*\)"/\1/p' "$ROOT/Cargo.toml" | head -1
}

VERSION="$(version_from_cargo)"
if [[ -z "$VERSION" ]]; then
  echo "error: could not read version from Cargo.toml" >&2
  exit 1
fi

GIT_SHORT="nogit"
if command -v git >/dev/null 2>&1 && git -C "$ROOT" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  GIT_SHORT="$(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null || echo nogit)"
fi

STAMP="raynu-v-${VERSION}"
DIST_ROOT="$ROOT/dist"
OUT_DIR="$DIST_ROOT/$STAMP"
TARBALL="$DIST_ROOT/${STAMP}.tar.gz"

if [[ "$SKIP_BUILD" != "1" ]]; then
  echo "==> build EFI"
  "$ROOT/tools/build.sh"
else
  echo "==> SKIP_BUILD=1 — using existing EFI"
fi

if [[ ! -f "$EFI" ]]; then
  echo "error: missing $EFI — run ./tools/build.sh or unset SKIP_BUILD" >&2
  exit 1
fi

echo "==> size budget (ADR-003)"
"$ROOT/tools/check-size.sh"

rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

cp "$EFI" "$OUT_DIR/r640-hypervisor.efi"

# Sidecar checksum for the EFI (sha256sum format: HASH  FILENAME).
(
  cd "$OUT_DIR"
  sha256sum r640-hypervisor.efi | tee r640-hypervisor.efi.sha256
)

{
  echo "name=raynu-v"
  echo "version=${VERSION}"
  echo "git=${GIT_SHORT}"
  echo "efi=r640-hypervisor.efi"
  echo "target=${TARGET}"
  echo "profile=${PROFILE}"
  echo "marker=RAYNU-V-M7-SHIP-OK"
  echo "built_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
} >"$OUT_DIR/VERSION"

{
  echo "RayNu-V EFI release kit"
  echo "Version: ${VERSION} (git ${GIT_SHORT})"
  echo "Marker: RAYNU-V-M7-SHIP-OK"
  echo "Contents:"
  echo "  r640-hypervisor.efi"
  echo "  r640-hypervisor.efi.sha256"
  echo "  VERSION"
  echo "  SHA256SUMS"
  echo "  MANIFEST.txt"
  echo "Deploy: see docs/runbooks/usb_idrac.md"
} >"$OUT_DIR/MANIFEST.txt"

(
  cd "$OUT_DIR"
  sha256sum r640-hypervisor.efi r640-hypervisor.efi.sha256 VERSION MANIFEST.txt \
    | tee SHA256SUMS
)

echo "==> tarball"
rm -f "$TARBALL" "${TARBALL}.sha256"
tar -C "$DIST_ROOT" -czf "$TARBALL" "$STAMP"
(
  cd "$DIST_ROOT"
  sha256sum "$(basename "$TARBALL")" | tee "$(basename "$TARBALL").sha256"
)

echo "==> release kit ready"
echo "  dir:     $OUT_DIR"
echo "  tarball: $TARBALL"
ls -la "$OUT_DIR"
ls -la "$TARBALL" "${TARBALL}.sha256"
echo "RAYNU-V-M7-SHIP-OK (package)"
