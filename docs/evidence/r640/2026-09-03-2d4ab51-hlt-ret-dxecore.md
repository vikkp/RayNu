# Iron COM2 — `2d4ab51` HLT `ret=0x7ff0e055` still `ataio=0`

- **Date:** 2026-09-03
- **Operator:** vikkp @ raynuvsrv1
- **Boot:** F11 Cruzer Micro (front USB 2), iDRAC `console com2`
- **EFI:** CI `--run 33697154185` (`2d4ab51`, sha256 `e9a5fd01…`)
- **Media:** `OVMF.fd` 3653632 · `linux.iso` 66060288
- **Does not claim:** `RAYNU-V-M7-ISO-INSTALL-OK` · Linux `ACPI=` · El Torito · Everest E5

## What moved

`ret=0x7ff0e055` names the CpuSleep caller. CpuSleep is `hlt; ret` at
`rip=0x7f0680d0`. The caller sits in the same image as AcpiTimerLib
`IN EAX,DX` at `0x7ff635d2` (~350 KiB later) — **DxeCore WaitForEvent**,
not PciBus (`0x7f01fc0f` CF8 OUT).

Hide-slot0 still printed. CDROM-OK / BOTH-OK via PIIX. EnableAttributes
`cmdwr=3` `pcicmd=0x5`. ROM size probe `romwr=0xfffffffe`. Then the same
HLT.

## What did not

No `Linux version`, no `efi:` / `ACPI=`, no `ataio>0`, no `sectors>0`.

PIT one-shot + `inject vec=0x20` still `inj=1` at `rip=0x7f0680d0`
through `n>1.3M`. HLT policy stays closed.

## Ladder

| Pin | Result | `ataio` |
|-----|--------|---------|
| `27eda8c` | hide slot-0; CDROM-OK via PIIX | 0 |
| `2d4ab51` | HLT `ret=0x7ff0e055` (DxeCore) | 0 |

Do not F11 `2d4ab51` / `--run 33697154185` again. Keep hide-slot0. Do
not hide PIIX. Do not start another dest / HLT / COMMAND / CF8 / ROM /
hide-slot0 SHA.

## Next SHA

Firmware ConIn CR: one `\r` into guest COM1 on the product-ISO HLT
after `pci_ide` so a serial WaitForEvent can return and IdeBus can
issue ATA. Proof: `firmware ConIn CR` then `ataio>0` or the same hang.
Still not `ISO-INSTALL-OK`.
