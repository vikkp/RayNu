# Iron COM2 — `6c4bfde` firmware ConIn CR still `ataio=0`

- **Date:** 2026-09-03
- **Operator:** vikkp @ raynuvsrv1
- **Boot:** F11 Cruzer Micro (front USB 2), iDRAC `console com2`
- **EFI:** CI `--run 33699177232` (`6c4bfde`, sha256 `8dd46b1c…`)
- **Media:** `OVMF.fd` 3653632 · `linux.iso` 66060288
- **Does not claim:** `RAYNU-V-M7-ISO-INSTALL-OK` · Linux `ACPI=` · El Torito · Everest E5

## What moved

`boot: guest-UEFI firmware ConIn CR` printed. One `\r` landed in guest
COM1 after `pci_ide=1`. Hide-slot0, CDROM-OK/BOTH-OK via PIIX,
EnableAttributes, dest_ok + ACPI MADT all held.

## What did not

No `Linux version`, no `efi:` / `ACPI=`, no `ataio>0`, no `sectors>0`.

Same CpuSleep after the CR:

- HLT `ret=0x7ff0e055` `rip=0x7f0680d0` `insn=f4c3` `ataio=0`
- PIT one-shot + `inject vec=0x20` still `inj=1` through `n>1.4M`

DxeCore WaitForEvent is a **timer** wait, not serial ConIn. ConIn CR
is closed.

## Ladder

| Pin | Result | `ataio` |
|-----|--------|---------|
| `2d4ab51` | HLT `ret=0x7ff0e055` (DxeCore) | 0 |
| `6c4bfde` | ConIn CR fired; same hang | 0 |

Do not F11 `6c4bfde` / `--run 33699177232` again. Keep hide-slot0. Keep
the one-shot CR (harmless). Do not hide PIIX. Do not start another dest
/ HLT / COMMAND / CF8 / ROM / hide-slot0 / ConIn SHA.

## Next SHA

Print 8 bytes at `ret-8` as `rethx=0x` so the DxeCore call site is
named. Then we can poke that WaitForEvent. Still not `ISO-INSTALL-OK`.
