#!/usr/bin/env python3
"""Minimal PE32+ EFI image and Authenticode certificate-table check.

ADR-021 A5. Host proof only. This file does not contain a key.
`write` emits an unsigned EFI application small enough for sbsign.
`check` refuses a file whose security data directory is empty.
"""

import struct
import sys


def _u16(n):
    return struct.pack("<H", n)


def _u32(n):
    return struct.pack("<I", n)


def _u64(n):
    return struct.pack("<Q", n)


def write_minimal_pe(path):
    """Write an unsigned PE32+ EFI application. File alignment 512."""
    dos = bytearray(0x80)
    dos[0:2] = b"MZ"
    dos[0x3C:0x40] = _u32(0x80)

    # COFF. EFI applications are DLLs (IMAGE_FILE_DLL) with a large address.
    coff = (
        _u16(0x8664)
        + _u16(1)
        + _u32(0)
        + _u32(0)
        + _u32(0)
        + _u16(240)
        + _u16(0x2022)
    )

    # PE32+ optional header, 16 data directories, security directory empty.
    opt = bytearray(240)
    struct.pack_into("<H", opt, 0, 0x20B)
    struct.pack_into("<I", opt, 4, 0x200)  # SizeOfCode
    struct.pack_into("<I", opt, 16, 0x1000)  # AddressOfEntryPoint
    struct.pack_into("<I", opt, 20, 0x1000)  # BaseOfCode
    struct.pack_into("<Q", opt, 24, 0)  # ImageBase
    struct.pack_into("<I", opt, 32, 0x1000)  # SectionAlignment
    struct.pack_into("<I", opt, 36, 0x200)  # FileAlignment
    struct.pack_into("<I", opt, 56, 0x2000)  # SizeOfImage
    struct.pack_into("<I", opt, 60, 0x200)  # SizeOfHeaders
    struct.pack_into("<H", opt, 68, 10)  # Subsystem: EFI application
    struct.pack_into("<I", opt, 108, 16)  # NumberOfRvaAndSizes

    sec = bytearray(40)
    sec[0:5] = b".text"
    struct.pack_into("<I", sec, 8, 0x200)  # VirtualSize
    struct.pack_into("<I", sec, 12, 0x1000)  # VirtualAddress
    struct.pack_into("<I", sec, 16, 0x200)  # SizeOfRawData
    struct.pack_into("<I", sec, 20, 0x200)  # PointerToRawData
    struct.pack_into("<I", sec, 36, 0x60000020)  # code, execute, read

    headers = bytes(dos) + b"PE\0\0" + coff + bytes(opt) + bytes(sec)
    if len(headers) > 0x200:
        raise SystemExit(f"headers {len(headers)} exceed file alignment")
    blob = headers + bytes(0x200 - len(headers)) + bytes([0xC3]) + bytes(0x1FF)
    with open(path, "wb") as f:
        f.write(blob)


def certificate_table(path):
    """Return (file_offset, size, revision, cert_type) of the WIN_CERTIFICATE."""
    with open(path, "rb") as f:
        data = f.read()
    if len(data) < 0x40 or data[0:2] != b"MZ":
        raise SystemExit("not a PE: missing MZ")
    e_lfanew = struct.unpack_from("<I", data, 0x3C)[0]
    if e_lfanew + 24 + 112 + 5 * 8 > len(data):
        raise SystemExit("PE truncated before the security directory")
    if data[e_lfanew : e_lfanew + 4] != b"PE\0\0":
        raise SystemExit("not a PE: missing signature")
    magic = struct.unpack_from("<H", data, e_lfanew + 24)[0]
    if magic != 0x20B:
        raise SystemExit(f"expected PE32+ (0x20b), got {magic:#x}")
    opt = e_lfanew + 24
    n_dir = struct.unpack_from("<I", data, opt + 108)[0]
    if n_dir < 5:
        raise SystemExit(f"security directory absent (n={n_dir})")
    off, size = struct.unpack_from("<II", data, opt + 112 + 4 * 8)
    if off == 0 or size < 8:
        raise SystemExit("security data directory is empty")
    if off + 8 > len(data):
        raise SystemExit("certificate table points past the file")
    wlen, rev, ctype = struct.unpack_from("<IHH", data, off)
    if rev != 0x0200:
        raise SystemExit(f"WIN_CERTIFICATE revision {rev:#x}, want 0x200")
    if ctype != 0x0002:
        raise SystemExit(f"certificate type {ctype:#x}, want PKCS signed data")
    if wlen < 8 or off + wlen > len(data):
        raise SystemExit("WIN_CERTIFICATE length does not fit the file")
    return off, size, rev, ctype


def main(argv):
    if len(argv) != 3 or argv[1] not in ("write", "check"):
        raise SystemExit("usage: a5_pe.py write|check <path>")
    if argv[1] == "write":
        write_minimal_pe(argv[2])
        return
    off, size, rev, ctype = certificate_table(argv[2])
    print(f"a5: cert table off={off} size={size} rev={rev:#x} type={ctype:#x}")


if __name__ == "__main__":
    main(sys.argv)
