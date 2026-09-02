# Iron COM2 — `27eda8c` hide-slot0 still `ataio=0`

- **Date:** 2026-09-02
- **Operator:** vikkp @ raynuvsrv1
- **Boot:** F11 Cruzer Micro (front USB 2), iDRAC `console com2`
- **EFI:** CI `--run 33695570769` (`27eda8c`, sha256 `687b3b20…`)
- **Media:** `OVMF.fd` 3653632 · `linux.iso` 66060288
- **Does not claim:** `RAYNU-V-M7-ISO-INSTALL-OK` · Linux `ACPI=` · El Torito · Everest E5

## What moved

Interleaved SOL reconstructed:

`boot: product ISO hides duplicate slot0 IDE (not ISO-INSTALL-OK)`

| Line | Meaning |
|------|---------|
| `pci select 00:00.01` then immediately `00:00.02` | Slot-0 fn1 hidden (no BAR dump) |
| `pci select 00:01.01` then `CDROM-OK` | PIIX is the CD |
| `cmdwr=3` `seq=0,0,0` | Only PIIX COMMAND writes (was 6) |
| BOTH-OK `virtio=1 ide=1` | Hide did not drop PIIX |

## What did not

No `Linux version`, no `efi:` / `ACPI=`, no `ataio>0`, no `sectors>0`.

Same CpuSleep after EnableAttributes + ROM size probe:

- HLT `cf8=0x0 cf8en=0x80004008 cf8ide=0x80000930 romwr=0xfffffffe`
- `reason=0xc` `rip=0x7f0680d0` `insn=f4c3` `ataio=0` `pin14=0`

BAR conflict / duplicate slot-0 is **not** why IdeBus never issues ATA. PIIX-only still hangs.

## Ladder

| Pin | Result | `ataio` |
|-----|--------|---------|
| `118edcf` | `romwr=0xfffffffe` | 0 |
| `27eda8c` | hide slot-0; CDROM-OK via PIIX | 0 |

Do not F11 `27eda8c` / `--run 33695570769` again. Keep the hide (it is correct). Do not hide PIIX. Do not start another HLT/COMMAND/CF8 SHA.

## Next SHA

Print CpuSleep caller: `hlt; ret` so `[RSP]` is the return address (`ret=0x`). Proof: HLT `ret=0x……`, then we know who called CpuSleep. Still not `ISO-INSTALL-OK`.
