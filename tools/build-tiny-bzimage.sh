#!/usr/bin/env bash
# Build a tinyconfig x86_64 bzImage with earlyprintk/serial (M3.8).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-$ROOT/assets/bzImage}"
BUILD="${BUILD_DIR:-$ROOT/target/linux-build}"
VER="${KERNEL_VER:-6.12.40}"
SRC="$BUILD/linux-$VER"

mkdir -p "$BUILD"
cd "$BUILD"

if [[ ! -f "linux-$VER.tar.xz" ]]; then
  echo "==> downloading linux-$VER"
  curl -L --fail -o "linux-$VER.tar.xz" \
    "https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-$VER.tar.xz"
fi
if [[ ! -d "$SRC" ]]; then
  echo "==> extracting"
  tar -xf "linux-$VER.tar.xz"
fi

cd "$SRC"
echo "==> tinyconfig + serial/earlyprintk"
make tinyconfig
./scripts/config --enable 64BIT
./scripts/config --enable X86_64
./scripts/config --disable SMP
./scripts/config --enable RELOCATABLE
./scripts/config --disable RANDOMIZE_BASE
./scripts/config --enable PRINTK
./scripts/config --enable EARLY_PRINTK
./scripts/config --enable SERIAL_EARLYCON
./scripts/config --enable SERIAL_8250
./scripts/config --enable SERIAL_8250_CONSOLE
./scripts/config --enable TTY
./scripts/config --enable BINFMT_ELF
./scripts/config --enable BLK_DEV_INITRD
./scripts/config --enable RD_GZIP
./scripts/config --enable DEVTMPFS
./scripts/config --enable DEVTMPFS_MOUNT
./scripts/config --enable TMPFS
./scripts/config --enable PROC_FS
./scripts/config --enable SYSFS
./scripts/config --set-str DEFAULT_HOSTNAME raynu-v
make olddefconfig
echo "==> make bzImage"
make -j"$(nproc)" bzImage
mkdir -p "$(dirname "$OUT")"
cp -f arch/x86/boot/bzImage "$OUT"
ls -la "$OUT"
echo "==> wrote $OUT"
# Companion initrd (M3.10) when requested.
if [[ "${BUILD_INITRD:-1}" == "1" ]]; then
  "$ROOT/tools/build-tiny-initrd.sh" "$ROOT/assets/initrd"
fi
