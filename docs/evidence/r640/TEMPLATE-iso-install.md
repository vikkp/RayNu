# R640 / QEMU ISO install-to-disk evidence template (E5 / M7.7)

Copy to `docs/evidence/r640/YYYY-MM-DD-iso-install.md` (or `…-qemu-iso-install.md`)
and fill every field. Empty templates do **not** close M7.7 / E5.

| Field | Value |
|-------|-------|
| Date (UTC) | |
| Operator | |
| Platform | QEMU nested KVM / PowerEdge R640 (circle one) |
| Host (service tag / hostname) | |
| Boot method | USB / iDRAC vMedia / QEMU |
| EFI path on media | `\EFI\BOOT\BOOTX64.EFI` (or note) |
| `r640-hypervisor.efi` SHA256 | |
| Release kit version | |
| Install disk size (bytes) | (default 67108864) |
| ISO id / name | |
| Extract-boot path | PE `.askern` / ESP `BZIMAGE` + `INITRD` / other |

## Required markers / proofs

- [ ] Deploy / install REST (or in-process) issued launch contract
- [ ] Guest extract-boot progressed (bzImage / SHELL or installer)
- [ ] Virtio-blk install disk write proof (marker sector or filesystem)
- [ ] Reboot-to-disk / second boot from install disk
- [ ] Serial shows `RAYNU-V-M7-ISO-INSTALL-OK` (or documented equivalent)

## Serial excerpt

```text
(paste COM2 / QEMU serial here)
```

## REST / operator transcript (optional)

```text
(paste curl deploy/install + responses)
```

## Close claim

Only after the checklist above is real evidence may a close PR claim:

```text
RAYNU-V-M7-ISO-INSTALL-OK
```

and flip `docs/evidence/r640/STATUS-iso-install` to `STATUS=closed`, and set
`GAP(CLOSED M7.7): ISO install-to-disk + reboot-to-disk`.

Scaffold smoke (`RAYNU-V-M7-ISO-INSTALL-SCAFFOLD-OK`) alone is **invalid** as E5 close.
Do **not** claim Mount Everest until E4 + E5 are both green.
