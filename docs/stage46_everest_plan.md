# Stage 46 Option 2 — Everest move plan

Living tracker for [PR #229](https://github.com/vikkp/RayNu/pull/229).
Decision: [ADR-015](adr/ADR-015.md).

**Do not claim `RAYNU-V-M7-ISO-INSTALL-OK`.** HDA `last_commit` stays `2b795a0`.

---

## Stop rules

- Do not F11 `abba969` / `--run 33567464001` again (honor `pcicmd=0` `wr=0` still `ataio=0`).
- Do not F11 `184ee61` / `--run 33562028442` again (`cmdwr=6` last `wr=0x0` stored `0x1`).
- Do not F11 `21dc562` / `--run 33559849096` again (skip-HLT then same CpuSleep).
- Do not F11 `e3cbfa5` / `--run 33558261624` again (one-shot then HLT hang).
- Do not F11 `24c5fa6` / `--run 33555104832` again (PIT livelock).
- Do not F11 `b5c3a9c` / `--run 33440050729` again (skip-after-inject CpuSleep).
- Do not flash `2d6b109` (dest skip) or `8024439` (later IdeBus).
- No new EFI SHA without a COM2 line that fails the current step.
- One SHA per fail. Not an IdeBus PCI farm. Host/CI never prints the iron marker.
- Do not OR PCI command `0x0001`.

---

## Ladder

| # | Status | Proof | This COM2 (`abba969`) |
|---|--------|-------|------------------------|
| 0 | DONE | ADR-015 | — |
| 1–2 | DONE | Flash pin `--run 33567464001` | Cruzer `FLASH-OK`, iso 63 MiB |
| 3a | **DONE** | dest_ok + ACPI MADT + wait-for-irq + one-shot + skip-HLT | unchanged |
| 3b | **FAIL** | Linux `efi:` contains `ACPI=` | honor `cmdwr=6 wr=0x0 pcicmd=0x0` then same `rip=0x7f0680d0` `ataio=0` |
| 4–6 | BLOCKED | `/init` · disk · `ISO-INSTALL-OK` | — |

Iron notes:

- [`docs/evidence/r640/2026-09-01-b5c3a9c-acpi-madt-hlt.md`](evidence/r640/2026-09-01-b5c3a9c-acpi-madt-hlt.md) — dest_ok + skip-HLT hang
- [`docs/evidence/r640/2026-09-01-24c5fa6-hlt-wait-pit-livelock.md`](evidence/r640/2026-09-01-24c5fa6-hlt-wait-pit-livelock.md) — wait-for-irq, stacked PIT livelock
- [`docs/evidence/r640/2026-09-01-e3cbfa5-pit-oneshot-hlt-hang.md`](evidence/r640/2026-09-01-e3cbfa5-pit-oneshot-hlt-hang.md) — one-shot stops storm; HLT hang
- [`docs/evidence/r640/2026-09-01-21dc562-skip-oneshot-hlt-hang.md`](evidence/r640/2026-09-01-21dc562-skip-oneshot-hlt-hang.md) — skip fired; same hang
- [`docs/evidence/r640/2026-09-01-184ee61-ide-cmdwr-disable.md`](evidence/r640/2026-09-01-184ee61-ide-cmdwr-disable.md) — `cmdwr=6` last `wr=0x0` stored `0x1`
- [`docs/evidence/r640/2026-09-01-abba969-honor-cmd-still-disabled.md`](evidence/r640/2026-09-01-abba969-honor-cmd-still-disabled.md) — honor stuck `pcicmd=0`

---

## Current wall

Honor COMMAND is proved: last write `0` reads back `0`. Firmware still left IO disabled. EnableAttributes did not write `0x5` after that disable. Same CpuSleep hang. `ataio=0` `sectors=0` `cmd=0x00` (ATA 0x1F7). HLT policy is exhausted.

This #229 HEAD: print the six COMMAND write values (`wr=` per write + HLT `seq=`). Do not OR `0x0001`. Do not F11 `abba969` / `33567464001`. Stay on #229 (ADR-015). Do not flash `8024439`.

Next proof: COM2 `wr=` sequence (whether any write was `0x5` / EnableAttributes). Still not `ISO-INSTALL-OK`.
