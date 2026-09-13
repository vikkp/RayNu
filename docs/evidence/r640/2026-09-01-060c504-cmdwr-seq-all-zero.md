# Iron COM2 — `060c504` COMMAND seq all zeros

- **Date:** 2026-09-01
- **Operator:** vikkp @ raynuvsrv1
- **Boot:** F11 Cruzer Micro (front USB 2), iDRAC `console com2`
- **EFI:** CI `--run 33569757025` (`060c504`, sha256 `cafaafd1…`)
- **Media:** `OVMF.fd` 3653632 · `linux.iso` 66060288
- **Does not claim:** `RAYNU-V-M7-ISO-INSTALL-OK` · Linux `ACPI=` · El Torito · Everest E5

## What moved

| Line | Meaning |
|------|---------|
| `cmdwr honor wr=0x0000 n=0` … `n=5` | All six COMMAND writes are `0` |
| `seq=0,0,0,0,0,0` | EnableAttributes never wrote `0x5` |

COMMAND is closed. Nested `cmdwr=0` is still not iron. Iron writes COMMAND, but only disable. No IO+BusMaster enable.

## What did not

No `Linux version`, no `efi:` / `ACPI=`, no `ataio>0`, no `sectors>0`.

After skip-HLT, ticks stay on CpuSleep:

- `reason=0xc` `rip=0x7f0680d0` `insn=f4c3` `inj=1`
- `ataio=0` `cmd=0x00` `pin14=0` through `n>3.4M`

`cmd=0x00` is last ATA 0x1F7, not PCI COMMAND. HLT policy is exhausted. `OR 0x0001` already failed (`184ee61`). Honor already failed (`abba969`).

## Next SHA

ADR-015: after this dump, apply EnableAttributes `0x0005` (IO+BM) on write-0. Do not OR `0x0001`. Do not F11 this pin again. Still not `ISO-INSTALL-OK`.

## Ladder

| Step | Result |
|------|--------|
| honor COMMAND as written | **DONE** (`abba969`) |
| six-write sequence | **DONE** (all `0`) |
| 3b ATA / El Torito / Linux `ACPI=` | **FAIL** (`ataio=0`, same hang) |
