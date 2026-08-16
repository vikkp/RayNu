#!/usr/bin/env bash
# Synthesize QEMU E5 lab install disk (1 MiB) with host LBA0/LBA1 patterns.
# Matches devices/virtio_blk.rs DISK_PATTERN / INSTALL_DISK_PATTERN trails.
# Not iron evidence — used between boot1 (write) and boot2 (reboot detect).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-$ROOT/target/e5-lab-install.img}"
mkdir -p "$(dirname "$OUT")"

python3 - "$OUT" <<'PY'
import struct, sys
out = sys.argv[1]
DISK_PATTERN = 0xB10C_0B01
INSTALL_DISK_PATTERN = 0xE5D1_5C00
sector = 512
size = 1024 * 1024
img = bytearray(size)
for i in range(sector // 4):
    struct.pack_into("<I", img, i * 4, DISK_PATTERN ^ i)
struct.pack_into("<I", img, 0, DISK_PATTERN)
for i in range(sector // 4):
    struct.pack_into("<I", img, sector + i * 4, INSTALL_DISK_PATTERN ^ i)
struct.pack_into("<I", img, sector, INSTALL_DISK_PATTERN)
open(out, "wb").write(img)
print(f"wrote {out} ({size} bytes) LBA0=0x{DISK_PATTERN:08X} LBA1=0x{INSTALL_DISK_PATTERN:08X}")
PY
