# Iron COM2 — `5de9e1c` HLT `cf8en=0x80004008` still `ataio=0`

- **Date:** 2026-09-02
- **Operator:** vikkp @ raynuvsrv1
- **Boot:** F11 Cruzer Micro (front USB 2), iDRAC `console com2`
- **EFI:** CI `--run 33575888121` (`5de9e1c` / `21f3407` bytes, sha256 `59e0a391…`)
- **Media:** `OVMF.fd` 3653632 · `linux.iso` 66060288
- **Does not claim:** `RAYNU-V-M7-ISO-INSTALL-OK` · Linux `ACPI=` · El Torito · Everest E5

## What moved

| Line | Meaning |
|------|---------|
| `HLT … cf8=0x0 cf8en=0x80004008` | Last enabled CONFIG_ADDRESS is `00:08.0` offset `0x08` |
| `HLT wait-for-irq … cf8en=0x80004008` | Same latch on the wait line |

`0x80004008` = bus 0, device 8, function 0, register 0x08 (i440FX host Class/Revision). Firmware finished the PCI walk, last live poke was the **host bridge**, then wrote CONFIG_ADDRESS 0. Not stuck mid-ParseBar on IDE.

## What did not

No `Linux version`, no `efi:` / `ACPI=`, no `ataio>0`, no `sectors>0`.

After skip-HLT, ticks stay on CpuSleep:

- `reason=0xc` `rip=0x7f0680d0` `insn=f4c3` `inj=1`
- `ataio=0` `cmd=0x00` `pin14=0` through `n>2.0M`

`cmd=0x00` is last ATA 0x1F7, not PCI COMMAND. EnableAttributes `pcicmd=0x5` still holds.

## COMMAND + CF8 still closed

| Pin | Last print | `ataio` |
|-----|------------|---------|
| `c144001` | `pcicmd=0x5` | 0 |
| `61991be` | `cf8=0x0` | 0 |
| `5de9e1c` | `cf8en=0x80004008` (`00:08.0+08`) | 0 |

Do not OR `0x0001`. Do not resume #231 for `COMMAND.IO`. HLT policy is still exhausted. Do not F11 `5de9e1c` / `--run 33575888121` again.

## Next SHA

Latch last enabled CF8 that selected IDE `00:00.1` / `00:01.1` (`LAST_CF8_IDE`). Print `cf8ide=` next to `cf8en=`. Proof: HLT `cf8en=0x80004008 cf8ide=0x80000xxx` (last IDE register), then `ataio>0` or the same hang. Still not `ISO-INSTALL-OK`.

## Ladder

| Step | Result |
|------|--------|
| last PCI CF8 on HLT | **DONE** (`cf8=0x0`) |
| last enabled CF8 | **DONE** (`00:08.0+08` host class) |
| 3b ATA / El Torito / Linux `ACPI=` | **FAIL** (`ataio=0`, same hang) |
| last IDE CF8 | **THIS SHA** |
