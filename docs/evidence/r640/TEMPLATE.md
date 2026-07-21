# R640 first-light evidence template

Copy this file to `docs/evidence/r640/YYYY-MM-DD-r640-first-light.md` and fill
every field. Empty templates do **not** close M7.5.

| Field | Value |
|-------|-------|
| Date (UTC) | |
| Operator | |
| Host (service tag / hostname) | |
| Boot method | USB / iDRAC vMedia (circle one) |
| EFI path on media | `\EFI\BOOT\BOOTX64.EFI` (or note) |
| `r640-hypervisor.efi` SHA256 | |
| Release kit version | |
| iDRAC firmware (if known) | |
| BIOS / boot mode | UEFI |
| Serial channel | iDRAC virtual COM1 / physical COM1 |

## Required serial markers

Paste excerpts showing at least:

- [ ] `RAYNU-V-M0-BOOT-OK`
- [ ] VMX path (`RAYNU-V-M1-VMXON-OK` / `RAYNU-V-M1-VMEXIT-OK` or documented residual)
- [ ] EPT / guest progress (list markers seen; note residual if short of shell)

## Serial excerpt

```text
(paste COM1 / iDRAC serial here)
```

## Iron close claim

Only after the checklist above is real R640 evidence may a close PR claim:

```text
RAYNU-V-R640-BOOT-OK
```

and set `GAP(CLOSED M7.5): Real R640 boot`. Latitude/QEMU logs are invalid here.
