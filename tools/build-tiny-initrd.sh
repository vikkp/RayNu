#!/usr/bin/env bash
# Build a gzip+cpio initrd with a static /init that prints SHELL-OK (M3.10).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-$ROOT/assets/initrd}"
WORKDIR="${BUILD_DIR:-$ROOT/target/initrd-build}"

mkdir -p "$WORKDIR/root"
echo "==> compiling static init"
gcc -static -nostdlib -fno-asynchronous-unwind-tables -fno-ident \
  -Os -s -o "$WORKDIR/root/init" "$ROOT/tools/init/init.c"
chmod 755 "$WORKDIR/root/init"
# Optional: empty /dev so paths exist before devtmpfs (kernel may mount).
mkdir -p "$WORKDIR/root/dev" "$WORKDIR/root/proc" "$WORKDIR/root/sys"

echo "==> packing gzip cpio"
(
  cd "$WORKDIR/root"
  find . -print0 | cpio --null -o -H newc --owner=0:0
) | gzip -n -9 >"$OUT"

ls -la "$OUT"
echo "==> wrote $OUT ($(wc -c <"$OUT") bytes)"
file "$OUT" || true
