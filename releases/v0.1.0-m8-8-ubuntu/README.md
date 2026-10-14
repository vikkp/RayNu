# v0.1.0-m8-8-ubuntu — M8.8 close rollback

Flash this file to return to the EFI that closed M8.8. It is **not**
GitHub Latest. Latest stays `v0.1.0-everest-closed` (`f72b4276`).

This is the rollback when an M8.9 EFI misbehaves. The older PERC SPA pin
`v0.1.0-m8-perc-spa` does not have this Ubuntu disk.

## What this freezes

| Area | State |
|------|--------|
| Login | `vikkp@raynusrv2:~$` on Ubuntu 26.04, kernel `7.0.0-14-generic` |
| Disk | 10 GiB tail, `root=/dev/mapper/ubuntu--vg-ubuntu--lv`, `vda1` `vda2` `vda3` |
| Initrd | `initrd.img-7.0.0-14-generic`, 34338680 bytes, copied from ext4 |
| Clear ISO | `boot: M8.8 library ISO cleared (signature sector only; disks unchanged)` |
| Untouched | UBUNTU0, the 8 GiB window, and the Ubuntu disk |

## Provenance

| Field | Value |
|-------|--------|
| EFI source | `c3a70e623aec376e548d2553932ce3fe54026468` |
| CI | `38454398693` |
| EFI bytes | 2,404,352 |
| EFI SHA256 | `08ef31ba87136577102ac378d8e0a512327994e8794c41c64444f5be5f8812c4` |
| COM2 | `build: sha=c3a70e623aec` |
| Branch | `cursor/m8-8-initrd-ext4-8366` |

The bytes in this directory are the CI artifact. A later documentation
commit does not change them. A local rebuild will stamp a different
`build: sha=` and is not this rollback.

## Verify

```bash
( cd releases/v0.1.0-m8-8-ubuntu && sha256sum -c r640-hypervisor.efi.sha256 )
```

Cruzer: replace only `EFI/BOOT/BOOTX64.EFI`. Leave `raynuf.txt`. Do not
write the Toshiba. Do not pass `--init-new-cruzer`.
