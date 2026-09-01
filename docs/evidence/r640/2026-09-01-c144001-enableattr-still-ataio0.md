# Iron COM2 — `c144001` EnableAttributes `pcicmd=0x5` still `ataio=0`

- **Date:** 2026-09-01
- **Operator:** vikkp @ raynuvsrv1
- **Boot:** F11 Cruzer Micro (front USB 2), iDRAC `console com2`
- **EFI:** CI `--run 33571164257` (`c144001`, sha256 `2389db6a…`)
- **Media:** `OVMF.fd` 3653632 · `linux.iso` 66060288
- **Does not claim:** `RAYNU-V-M7-ISO-INSTALL-OK` · Linux `ACPI=` · El Torito · Everest E5

## What moved

| Line | Meaning |
|------|---------|
| `IDE pci EnableAttributes` | Write-0 restored IO+BM |
| `cmdwr honor wr=0x0000 n=0` … `n=5` | Firmware still writes only `0` |
| `HLT … cmdwr=6 pcicmd=0x5 wr=0x0 seq=0,0,0,0,0,0` | Register is IO+BM |

EnableAttributes on the wire is proved. Firmware never wrote `0x5`. Same CpuSleep hang.

## What did not

No `Linux version`, no `efi:` / `ACPI=`, no `ataio>0`, no `sectors>0`.

After skip-HLT, ticks stay on CpuSleep:

- `reason=0xc` `rip=0x7f0680d0` `insn=f4c3` `inj=1`
- `ataio=0` `cmd=0x00` `pin14=0` through `n>3.0M`

`cmd=0x00` is last ATA 0x1F7, not PCI COMMAND. ATA I/O is not gated on `pci_cmd`.

## COMMAND closed

| Pin | Stored COMMAND | `ataio` |
|-----|----------------|---------|
| `184ee61` | `OR 0x0001` → `pcicmd=0x1` | 0 |
| `abba969` | honor → `pcicmd=0` | 0 |
| `060c504` | seq all `0` | 0 |
| `c144001` | EnableAttributes → `pcicmd=0x5` | 0 |

Do not OR `0x0001`. Do not resume #231 for further `COMMAND.IO` (ADR-015). HLT policy is still exhausted.

## Next SHA

Print last PCI CF8 on the HLT stall (stop line already has `cf8=`; this dump never reached stop). Still not `ISO-INSTALL-OK`.

## Ladder

| Step | Result |
|------|--------|
| honor COMMAND as written | **DONE** (`abba969`) |
| six-write sequence | **DONE** (all `0`) |
| EnableAttributes `0x0005` after write-0 | **DONE** (`pcicmd=0x5`) |
| 3b ATA / El Torito / Linux `ACPI=` | **FAIL** (`ataio=0`, same hang) |
