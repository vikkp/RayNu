#!/usr/bin/env bash
# Package a versioned EFI release kit (M7.0 / ADR-009 / ADR-003).
# Layout:
#   dist/raynu-v-<version>/
#     r640-hypervisor.efi
#     r640-hypervisor.efi.sha256
#     r640-hypervisor.efi.bin          — same bytes; Windows-safe download name
#     r640-hypervisor.efi.bin.sha256
#     WINDOWS.txt
#     VERSION
#     SHA256SUMS
#     MANIFEST.txt
#   dist/raynu-v-<version>.tar.gz
#   dist/raynu-v-<version>-windows.zip — no .efi inside (Defender / SmartScreen)
#   dist/*.sha256
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
WINZIP="$DIST_ROOT/${STAMP}-windows.zip"

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
# Same PE bytes, non-.efi name: Windows Defender often quarantines a naked
# downloaded .efi (unsigned PE + Mark of the Web). Not an AV-evasion trick —
# operator renames to BOOTX64.EFI on the Cruzer/FAT stick.
cp "$EFI" "$OUT_DIR/r640-hypervisor.efi.bin"

# Sidecar checksum for the EFI (sha256sum format: HASH  FILENAME).
(
  cd "$OUT_DIR"
  sha256sum r640-hypervisor.efi | tee r640-hypervisor.efi.sha256
  sha256sum r640-hypervisor.efi.bin | tee r640-hypervisor.efi.bin.sha256
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
  echo "  r640-hypervisor.efi.bin"
  echo "  r640-hypervisor.efi.bin.sha256"
  echo "  WINDOWS.txt"
  echo "  VERSION"
  echo "  SHA256SUMS"
  echo "  MANIFEST.txt"
  echo "Deploy: see docs/runbooks/usb_idrac.md"
  echo "Windows: send ${STAMP}-windows.zip — do not email/Teams a naked .efi"
} >"$OUT_DIR/MANIFEST.txt"

cat >"$OUT_DIR/WINDOWS.txt" <<'WIN'
RayNu-V — Windows download (Defender / SmartScreen)

Windows often deletes a naked r640-hypervisor.efi on download ("virus
detected"). That is a false positive on an unsigned UEFI PE, not a signed
Authenticode app. Do not "allow the virus." Do not download the .efi in
Edge/Chrome/Teams.

On Windows:
  1. Download raynu-v-*-windows.zip (this kit), not the .efi.
  2. Right-click → Properties → Unblock if present, then Extract.
  3. Verify: certutil -hashfile r640-hypervisor.efi.bin SHA256
     must match r640-hypervisor.efi.bin.sha256.
  4. Copy r640-hypervisor.efi.bin onto the Cruzer as:
       EFI\BOOT\BOOTX64.EFI
     (rename on the USB). Leave EFI\RayNu\installdisk.bin alone.
  5. Prefer: git clone / copy from a Mac, or map the FAT .img via iDRAC.

Do not add a Defender exclusion for random Downloads folders.
Secure Boot / Authenticode signing is a later optional follow-on (HDA).
WIN

(
  cd "$OUT_DIR"
  sha256sum r640-hypervisor.efi r640-hypervisor.efi.sha256 \
    r640-hypervisor.efi.bin r640-hypervisor.efi.bin.sha256 \
    WINDOWS.txt VERSION MANIFEST.txt \
    | tee SHA256SUMS
)

echo "==> tarball"
rm -f "$TARBALL" "${TARBALL}.sha256" "$WINZIP" "${WINZIP}.sha256"
tar -C "$DIST_ROOT" -czf "$TARBALL" "$STAMP"
(
  cd "$DIST_ROOT"
  sha256sum "$(basename "$TARBALL")" | tee "$(basename "$TARBALL").sha256"
)

echo "==> Windows zip (no .efi member — Defender-safe download)"
python3 - "$OUT_DIR" "$WINZIP" <<'PY'
import hashlib, sys, zipfile
from pathlib import Path

kit = Path(sys.argv[1])
out = Path(sys.argv[2])
# Do not include r640-hypervisor.efi — that is what Windows quarantines.
members = [
    "r640-hypervisor.efi.bin",
    "r640-hypervisor.efi.bin.sha256",
    "WINDOWS.txt",
    "VERSION",
    "MANIFEST.txt",
]
with zipfile.ZipFile(out, "w", compression=zipfile.ZIP_DEFLATED) as z:
    for name in members:
        path = kit / name
        if not path.is_file():
            sys.exit(f"missing {path}")
        z.write(path, arcname=name)
    # SHA256SUMS for the zip members only (hashes of .bin + sidecars).
    lines = []
    for name in members:
        h = hashlib.sha256((kit / name).read_bytes()).hexdigest()
        lines.append(f"{h}  {name}\n")
    z.writestr("SHA256SUMS", "".join(lines))
print(f"wrote {out}")
PY
(
  cd "$DIST_ROOT"
  sha256sum "$(basename "$WINZIP")" | tee "$(basename "$WINZIP").sha256"
)

echo "==> release kit ready"
echo "  dir:     $OUT_DIR"
echo "  tarball: $TARBALL"
echo "  windows: $WINZIP"
ls -la "$OUT_DIR"
ls -la "$TARBALL" "${TARBALL}.sha256" "$WINZIP" "${WINZIP}.sha256"
echo "RAYNU-V-M7-SHIP-OK (package)"
