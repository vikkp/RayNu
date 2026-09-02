# Iron COM2 — `118edcf` HLT `romwr=0xfffffffe` still `ataio=0`

- **Date:** 2026-09-02
- **Operator:** vikkp @ raynuvsrv1
- **Boot:** F11 Cruzer Micro (front USB 2), iDRAC `console com2`
- **EFI:** CI `--run 33630723649` (`118edcf`, sha256 `6c359fea…`)
- **Media:** `OVMF.fd` 3653632 · `linux.iso` 66060288
- **Does not claim:** `RAYNU-V-M7-ISO-INSTALL-OK` · Linux `ACPI=` · El Torito · Everest E5

## What moved

| Line | Meaning |
|------|---------|
| `Stage 46 IDE pci rombar wr=0xfffffffe` | First write to IDE Expansion ROM |
| `HLT … romwr=0xfffffffe` | Same latch on the stall / wait-for-irq lines |

`0xfffffffe` is the standard PciBus Expansion ROM **size probe** (all-1s with enable bit 0 clear). Reads stay 0 (RAZ/WI). Firmware did not enable or map a ghost ROM. Last IDE CF8 is still `00:01.1+30`. Last enabled CF8 is still host `00:08.0+08`. Then CONFIG_ADDRESS 0 and CpuSleep.

## What did not

No `Linux version`, no `efi:` / `ACPI=`, no `ataio>0`, no `sectors>0`.

After skip-HLT, ticks stay on CpuSleep:

- `reason=0xc` `rip=0x7f0680d0` `insn=f4c3` `inj=1`
- `ataio=0` `cmd=0x00` `pin14=0` through `n>1.7M`

`cmd=0x00` is last ATA 0x1F7, not PCI COMMAND. EnableAttributes `pcicmd=0x5` still holds.

## CF8 / ROM ladder closed

| Pin | Last print | `ataio` |
|-----|------------|---------|
| `c144001` | `pcicmd=0x5` | 0 |
| `61991be` | `cf8=0x0` | 0 |
| `5de9e1c` | `cf8en=0x80004008` (`00:08.0+08`) | 0 |
| `7ba1ccf` | `cf8ide=0x80000930` (`00:01.1+30` ROM) | 0 |
| `118edcf` | `romwr=0xfffffffe` (size probe) | 0 |

Do not OR `0x0001`. Do not resume #231 for `COMMAND.IO`. HLT policy is exhausted. CF8/ROM prints are closed. Do not F11 `118edcf` / `--run 33630723649` again.

## Next SHA

Product ISO hides duplicate slot-0 IDE (`00:00.1` → `0xFFFFFFFF`). Keep PIIX `00:01.1` for El Torito. iso=0 keeps both. Both functions share the same I/O BARs; the duplicate can block IdeBus Start. Proof: COM2 `product ISO hides duplicate slot0 IDE`, CDROM-OK/BOTH-OK via PIIX, then `ataio>0` or the same hang. Still not `ISO-INSTALL-OK`.

## Ladder

| Step | Result |
|------|--------|
| last PCI CF8 on HLT | **DONE** (`cf8=0x0`) |
| last enabled CF8 | **DONE** (`00:08.0+08` host class) |
| last IDE CF8 | **DONE** (`00:01.1+30` Expansion ROM) |
| last IDE ROM write | **DONE** (`romwr=0xfffffffe` size probe) |
| 3b ATA / El Torito / Linux `ACPI=` | **FAIL** (`ataio=0`, same hang) |
| product ISO hide slot-0 | **THIS SHA** |
