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
| Later #229 SHAs are docs only | **False.** dest_ok, HLT policy, COMMAND, EnableAttributes, last CF8 are code + iron COM2. |
| Fail at step 3 → one dest / `rep insb` fix | dest / MADT is **DONE**. Step 3 split: 3a MADT closed; 3b Linux `ACPI=` never reached because `ataio=0`. |
| Nested F11 `--run 33440050729` stays on the parked fork | That pin is **#229** `b5c3a9c` (skip-after-inject). Refused. Not a #231 flash. |
| Flashable EFI is `2d6b109` | **Refused.** Then `61991be` / `--run 33573126367` printed `cf8=0x0`. Also refused. |

What the first note still got right: Option 2 is the path. Park #231.
Everest E5 is iron `ISO-INSTALL-OK`, not stamp persist, not nested
ParseBar. HDA iso ~99% is scaffolding. This pod has no VMX / Cruzer.
Host/CI never prints the iron marker. One SHA per failed COM2 step.

---

## Stop rules

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

---

## Ladder (tick only the proof column)

| # | Status | Do | Proof |
|---|--------|----|-------|
| 0 | **DONE** | Park #231. Path is #229. | ADR-015 |
| 1 | **DONE** | Green CI on the live pin | 49/49 on `61991be` (`--run 33573126367`) — **refused after COM2** |
| 2 | **DONE** (many pins) | Flash Cruzer from clone, `--no-git --run <id>` | `FLASH-OK` on `61991be` / `33573126367` (EFI `12f84c66`). Never PERC. Never `8024439`. |
| 3a | **DONE** | COM2: fw_cfg + ACPI tables | `dest_ok fill dest=0x81ec98` **and** `product ISO fw_cfg ACPI MADT` (held through `61991be`) |
| 3b | **FAIL** | COM2: firmware starts ATA / Linux sees ACPI | Need `ataio>0` then Linux `efi:` contains `ACPI=`. Last COM2 (`61991be`): HLT `cf8=0x0` `pcicmd=0x5` `seq=0,0,0,0,0,0`, then CpuSleep `rip=0x7f0680d0` `ataio=0`. Never reached Linux. |
| 3c | **FAIL** | COM2 of `61991be` last CF8 | HLT `cf8=0x0` — firmware wrote CONFIG_ADDRESS 0 after the PCI walk (not stuck mid-ParseBar). Still `ataio=0`. |
| 3d | **IN PROGRESS** | Print last enabled CF8 (`cf8en=`) | HLT `cf8=0x0 cf8en=0x8000xxxx` (last live BDF), then `ataio>0` or the same hang. |
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
| `61991be` | HLT `cf8=0x0`. Firmware deselected CONFIG_ADDRESS after the walk. Still `ataio=0`. |

HLT policy is exhausted. COMMAND is closed. Last CF8 is 0 (deselect). Nested
`cmdwr=0` / ParseBar is not the iron picture (iron writes COMMAND six times).
Further #231 IdeBus PCI slices cannot close E5.

---

## Current wall

Firmware enumerates IDE (CDROM-OK, BOTH-OK), dest_ok + ACPI MADT land,
COMMAND on the wire is IO+BM (`pcicmd=0x5`). Firmware finishes the PCI
walk, writes CONFIG_ADDRESS 0, then CpuSleep. `ataio=0` `pin14=0`
`cmd=0x00` (last ATA 0x1F7, not PCI COMMAND). ATA I/O is not gated on
`pci_cmd`. `cf8=0x0` is the last write, not the last enabled BDF.

This #229 HEAD prints `cf8en=` (last CF8 with bit 31). Wait CI, then
flash. Do not F11 `61991be`. Do not flash `3b1cf51` (docs only).

Next proof: HLT `cf8=0x0 cf8en=0x8000xxxx`, then `ataio>0` or the same
hang. Still not `ISO-INSTALL-OK`. After step 6, E5 can close. TLS and
guest console stay residual; they are not this ladder.

---

## How we stay honest

- Tick only the proof column. Latitude / QEMU / nested ≠ R640 COM2.
- Fail at 3b → one SHA that is not dest, not HLT policy, not COMMAND.IO,
  not #231.
- Fail at 5 → virtio, not #231.
- Host/CI never prints `RAYNU-V-M7-ISO-INSTALL-OK`.
- Cruzer only: `0781:5151` / `RAYNUV` / `/dev/sdc`. Flash from
  `~/projects/raynu` with `--no-git --run <id>`. `artifact commit=` is
  leftover #231 HEAD — ignore it.

HDA: months_to_everest 0.5 held · overall 95% · ETA 2026-09.
