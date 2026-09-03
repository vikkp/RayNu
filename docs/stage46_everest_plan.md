# Stage 46 Option 2 — Everest move plan

Living tracker for draft [PR #229](https://github.com/vikkp/RayNu/pull/229)
(`cursor/e5-stage46-iso-a623`). Decision: [ADR-015](adr/ADR-015.md).

**Do not claim `RAYNU-V-M7-ISO-INSTALL-OK`.** HDA `last_commit` stays `2b795a0`.
#231 tip stays `8024439` (parked). #229 stays draft.

---

## Where this plan was wrong (2026-09-01 note)

The first Option 2 note said Everest was waiting on a Cruzer flash of
`2d6b109` (fw_cfg `rep insb`) and that later #229 SHAs were docs only.

That is stale:

| Old claim | Lived |
|-----------|--------|
| Flash `2d6b109` | **Refused.** Dest skip at HV identity `0x200000`. Never F11 again. |
| Iron last reached `Freeing initrd` without `ACPI=` | That dump was the unflashed-`2d6b109` era. Later #229 pins print `fw_cfg dest_ok fill` and `product ISO fw_cfg ACPI MADT`. |
| Later #229 SHAs are docs only | **False.** dest_ok, HLT policy, COMMAND, EnableAttributes, last CF8, ROM write are code + iron COM2. |
| Fail at step 3 → one dest / `rep insb` fix | dest / MADT is **DONE**. Step 3 split: 3a MADT closed; 3b Linux `ACPI=` never reached because `ataio=0`. |
| Nested F11 `--run 33440050729` stays on the parked fork | That pin is **#229** `b5c3a9c` (skip-after-inject). Refused. Not a #231 flash. |
| Flashable EFI is `2d6b109` | **Refused.** Then CF8/ROM pins through `118edcf`. Also refused. |

What the first note still got right: Option 2 is the path. Park #231.
Everest E5 is iron `ISO-INSTALL-OK`, not stamp persist, not nested
ParseBar. HDA iso ~99% is scaffolding. This pod has no VMX / Cruzer.
Host/CI never prints the iron marker. One SHA per failed COM2 step.

---

## Stop rules

- Do not F11 `0b770cd` / `--run 33701350767` (`rethx=0xe056ff41b84d8b48`, still `ataio=0`).
- Do not F11 `6c4bfde` / `--run 33699177232` (ConIn CR fired, still `ataio=0`).
- Do not F11 `2d4ab51` / `--run 33697154185` (HLT `ret=0x7ff0e055` DxeCore Wait, still `ataio=0`).
- Do not F11 `27eda8c` / `--run 33695570769` (hide-slot0, CDROM-OK via PIIX, still `ataio=0`).
- Do not F11 `118edcf` / `--run 33630723649` (`romwr=0xfffffffe` size probe still `ataio=0`).
- Do not F11 `7ba1ccf` / `--run 33627470674` (`cf8ide=0x80000930` PIIX ROM BAR still `ataio=0`).
- Do not F11 `5de9e1c` / `--run 33575888121` (`cf8en=0x80004008` host class still `ataio=0`).
- Do not F11 `61991be` / `--run 33573126367` (`cf8=0x0` still `ataio=0`).
- Do not F11 `c144001` / `--run 33571164257` (`pcicmd=0x5` still `ataio=0`).
- Do not F11 `060c504` / `--run 33569757025` (`seq=0,0,0,0,0,0`).
- Do not F11 `abba969` / `--run 33567464001` (honor `pcicmd=0` still `ataio=0`).
- Do not F11 `184ee61` / `--run 33562028442` (`OR 0x0001` hid disable).
- Do not F11 `21dc562` / `--run 33559849096` (skip-HLT then same CpuSleep).
- Do not F11 `e3cbfa5` / `--run 33558261624` (one-shot then HLT hang).
- Do not F11 `24c5fa6` / `--run 33555104832` (PIT livelock).
- Do not F11 `b5c3a9c` / `--run 33440050729` (skip-after-inject CpuSleep).
- Do not flash `2d6b109` (dest skip), `3b1cf51` (docs only), or `8024439` (later IdeBus).
- No new EFI SHA without a COM2 line that fails the current step.
- One SHA per fail. Not an IdeBus PCI farm. Not another HLT policy.
- Do not OR PCI command `0x0001`. COMMAND path is closed.
- Do not resume #231 for further `COMMAND.IO`.
- Do not print another CF8/ROM field. That ladder is closed.
- Do not hide PIIX. Do not revert hide-slot0 (it is correct, not sufficient).

---

## Ladder (tick only the proof column)

| # | Status | Do | Proof |
|---|--------|----|-------|
| 0 | **DONE** | Park #231. Path is #229. | ADR-015 |
| 1 | **DONE** | Green CI on the live pin | 49/49 on `0b770cd` (`--run 33701350767`) — **refused after COM2** |
| 2 | **DONE** (many pins) | Flash Cruzer from clone, `--no-git --run <id>` | `FLASH-OK` on `0b770cd` / `33701350767` (EFI `b17314b7`). Never PERC. Never `8024439`. |
| 3a | **DONE** | COM2: fw_cfg + ACPI tables | `dest_ok fill dest=0x81ec98` **and** `product ISO fw_cfg ACPI MADT` (held through `0b770cd`) |
| 3b | **FAIL** | COM2: firmware starts ATA / Linux sees ACPI | Need `ataio>0` then Linux `efi:` contains `ACPI=`. Last COM2 (`0b770cd`): `rethx=` then CpuSleep `ataio=0`. Never reached Linux. |
| 3c | **FAIL** | COM2 of `61991be` last CF8 | HLT `cf8=0x0` — firmware wrote CONFIG_ADDRESS 0 after the PCI walk. Still `ataio=0`. |
| 3d | **FAIL** | COM2 of `5de9e1c` last enabled CF8 | HLT `cf8en=0x80004008` = i440FX host `00:08.0+08` (class). Not an IDE BDF. Still `ataio=0`. |
| 3e | **FAIL** | COM2 of `7ba1ccf` last IDE CF8 | HLT `cf8ide=0x80000930` = PIIX `00:01.1+30` (Expansion ROM). Still `ataio=0`. |
| 3f | **FAIL** | COM2 of `118edcf` last IDE ROM write | HLT `romwr=0xfffffffe` = standard ROM size probe (enable bit clear). No ghost ROM. Still `ataio=0`. |
| 3g | **FAIL** | COM2 of `27eda8c` hide slot-0 | Hide printed. CDROM-OK on `00:01.1`. `cmdwr=3`. Same hang `ataio=0`. BAR conflict is not the wall. |
| 3h | **FAIL** | COM2 of `2d4ab51` HLT retaddr | HLT `ret=0x7ff0e055` = DxeCore WaitForEvent (AcpiTimer `0x7ff635d2`), not PciBus. Still `ataio=0`. |
| 3i | **FAIL** | COM2 of `6c4bfde` ConIn CR | CR printed. Same hang `ret=0x7ff0e055` `ataio=0`. Timer wait, not serial. |
| 3j | **FAIL** | COM2 of `0b770cd` callsite | `rethx=0xe056ff41b84d8b48` = `call [r14-0x20]` CpuSleep. Still `ataio=0`. |
| 3k | **IN PROGRESS** | Force WaitForEvent `EFI_SUCCESS` | Unwind frame (`leave; ret` + `RAX=0`). Print `caller=`. |
| 4 | BLOCKED | Linux stays up | `Linux version` then `/init` or `~#` |
| 5 | BLOCKED | `setup-disk` sees the disk | `/dev/vda`, not `No disks available`. Fail here → virtio, not #231. |
| 6 | OPEN | Installer writes GPT | COM2 `RAYNU-V-M7-ISO-INSTALL-OK` |

---

## Closed experiments (do not repeat)

| Pin | What we learned |
|-----|-----------------|
| `2d6b109` | dest skip. Not the ACPI EFI. |
| `b5c3a9c` | dest_ok + MADT, then skip-after-inject CpuSleep `ataio=0`. |
| `24c5fa6` / `e3cbfa5` / `21dc562` | HLT policy exhausted (wait-for-irq, one-shot, skip-after-oneshot). |
| `184ee61` | `OR 0x0001` left `pcicmd=0x1`. |
| `abba969` | Honor COMMAND: `pcicmd=0`. |
| `060c504` | Six COMMAND writes, all `0`. |
| `c144001` | EnableAttributes `pcicmd=0x5`. Still `ataio=0`. **COMMAND closed.** |
| `61991be` | HLT `cf8=0x0`. Firmware deselected CONFIG_ADDRESS after the walk. |
| `5de9e1c` | HLT `cf8en=0x80004008`. Last live BDF is host `00:08.0+08`, not IDE. Still `ataio=0`. |
| `7ba1ccf` | HLT `cf8ide=0x80000930`. Last IDE register is PIIX Expansion ROM. Still `ataio=0`. |
| `118edcf` | HLT `romwr=0xfffffffe`. Standard size probe, enable bit clear, RAZ/WI. **CF8/ROM closed.** |
| `27eda8c` | Hide slot-0. CDROM-OK via PIIX. Same CpuSleep. **BAR conflict closed.** |
| `2d4ab51` | HLT `ret=0x7ff0e055`. DxeCore WaitForEvent, not PciBus. Still `ataio=0`. **Caller named.** |
| `6c4bfde` | ConIn CR fired. Same hang. **Serial ConIn wait closed.** |
| `0b770cd` | `rethx=0xe056ff41b84d8b48` = `call [r14-0x20]` CpuSleep. Event never signals. **Callsite closed.** |

HLT policy is exhausted. COMMAND is closed. CF8/ROM diagnostics are
closed (`romwr=0xfffffffe`). Nested `cmdwr=0` / ParseBar is not the
iron picture. Further #231 IdeBus PCI slices cannot close E5.
Do not hide PIIX (`ea30da1`). Do not start another dest / HLT / COMMAND SHA.

### Gauge (are we on the right track?)

| Question | Answer |
|----------|--------|
| Diagnosis track | **Yes.** Each pin killed one hypothesis. We know the exact call. |
| Product track (`ataio>0`) | **Stalled.** ~15 iron pins, `ataio` still 0. Informed ≠ closer to Linux. |
| Right next move | Change WaitForEvent's outcome (this SHA). Not another print. |
| Wrong next moves | dest, HLT policy, COMMAND, CF8/ROM, hide-slot0/PIIX, ConIn, #231 |

---

## Current wall

Firmware enumerates IDE (CDROM-OK, BOTH-OK), dest_ok + ACPI MADT land,
COMMAND on the wire is IO+BM (`pcicmd=0x5`). Firmware finishes the PCI
walk, last IDE poke is a ROM size probe (`romwr=0xfffffffe`), last
enabled CF8 is i440FX host `00:08.0` offset `0x08`, then writes
CONFIG_ADDRESS 0 and CpuSleep. `ataio=0` `pin14=0` `cmd=0x00`
(last ATA 0x1F7, not PCI COMMAND). ATA I/O is not gated on `pci_cmd`.
`00:00.1` and `00:01.1` share the same I/O BARs.

Hide-slot0 is kept. ConIn CR is kept (fired, not sufficient). Callsite
is `call [r14-0x20]` into CpuSleep (`rethx=0xe056ff41b84d8b48`).
CpuSleep returns; WaitForEvent loops; the event never signals. This
#229 HEAD unwinds that WaitForEvent as `EFI_SUCCESS` and prints
`caller=`. Wait CI, then flash. Do not F11 `0b770cd`. Do not F11
`6c4bfde`. Do not flash `3b1cf51`.

Next proof: `firmware WaitForEvent return caller=0x……`, then `ataio>0`
or a new hang/crash. Still not `ISO-INSTALL-OK`. After step 6, E5 can
close. TLS and guest console stay residual; they are not this ladder.

---

## How we stay honest

- Tick only the proof column. Latitude / QEMU / nested ≠ R640 COM2.
- Fail at 3b → one SHA that is not dest, not HLT policy, not COMMAND.IO,
  not CF8/ROM print, not hide-slot0, not hide-PIIX, not retaddr print,
  not ConIn CR, not callsite print, not #231.
- Fail at 5 → virtio, not #231.
- Host/CI never prints `RAYNU-V-M7-ISO-INSTALL-OK`.
- Cruzer only: `0781:5151` / `RAYNUV` / `/dev/sdc`. Flash from
  `~/projects/raynu` with `--no-git --run <id>`. `artifact commit=` is
  leftover #231 HEAD — ignore it.

HDA: months_to_everest 0.5 held · overall 95% · ETA 2026-09.
