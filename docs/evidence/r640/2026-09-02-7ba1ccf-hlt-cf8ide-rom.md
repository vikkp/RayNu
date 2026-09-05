# Iron COM2 — `7ba1ccf` HLT `cf8ide=0x80000930` still `ataio=0`

- **Date:** 2026-09-02
- **Operator:** vikkp @ raynuvsrv1
- **Boot:** F11 Cruzer Micro (front USB 2), iDRAC `console com2`
- **EFI:** CI `--run 33627470674` (`7ba1ccf`, sha256 `f7c2f14b…`)
- **Media:** `OVMF.fd` 3653632 · `linux.iso` 66060288
- **Does not claim:** `RAYNU-V-M7-ISO-INSTALL-OK` · Linux `ACPI=` · El Torito · Everest E5

## What moved

| Line | Meaning |
|------|---------|
| `HLT … cf8=0x0 cf8en=0x80004008 cf8ide=0x80000930` | Last IDE CONFIG_ADDRESS is `00:01.1` offset `0x30` |
| `HLT wait-for-irq … cf8ide=0x80000930` | Same latch on the wait line |

`0x80000930` = bus 0, device 1, function 1, register `0x30` (PIIX IDE **Expansion ROM BAR**). Firmware finished ParseBar including the optional ROM, last live IDE poke was ROM, last enabled CF8 overall stayed host `00:08.0+08`, then wrote CONFIG_ADDRESS 0. Not stuck mid-COMMAND or I/O BAR.

Reads of `0x30` were already 0 (`config_dword` default). No option ROM.

## What did not

No `Linux version`, no `efi:` / `ACPI=`, no `ataio>0`, no `sectors>0`.

After skip-HLT, ticks stay on CpuSleep:

- `reason=0xc` `rip=0x7f0680d0` `insn=f4c3` `inj=1`
- `ataio=0` `cmd=0x00` `pin14=0` through `n>3.0M`

`cmd=0x00` is last ATA 0x1F7, not PCI COMMAND. EnableAttributes `pcicmd=0x5` still holds.

## CF8 ladder closed through last IDE register

| Pin | Last print | `ataio` |
|-----|------------|---------|
| `c144001` | `pcicmd=0x5` | 0 |
| `61991be` | `cf8=0x0` | 0 |
| `5de9e1c` | `cf8en=0x80004008` (`00:08.0+08`) | 0 |
| `7ba1ccf` | `cf8ide=0x80000930` (`00:01.1+30` ROM) | 0 |

Do not OR `0x0001`. Do not resume #231 for `COMMAND.IO`. HLT policy is still exhausted. Do not F11 `7ba1ccf` / `--run 33627470674` again.

## Next SHA

Latch last write to IDE Expansion ROM (`LAST_PCI_ROM_WR`). Print `romwr=` next to `cf8ide=`. Reads stay 0 (RAZ/WI, no ghost ROM). Proof: HLT `cf8ide=0x80000930 romwr=0x……`, then `ataio>0` or the same hang. Still not `ISO-INSTALL-OK`.

## Ladder

| Step | Result |
|------|--------|
| last PCI CF8 on HLT | **DONE** (`cf8=0x0`) |
| last enabled CF8 | **DONE** (`00:08.0+08` host class) |
| last IDE CF8 | **DONE** (`00:01.1+30` Expansion ROM) |
| 3b ATA / El Torito / Linux `ACPI=` | **FAIL** (`ataio=0`, same hang) |
| last IDE ROM write | **THIS SHA** |
