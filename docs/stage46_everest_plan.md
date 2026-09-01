# Stage 46 Option 2 — Everest move plan

Living tracker for [PR #229](https://github.com/vikkp/RayNu/pull/229).
Decision: [ADR-015](adr/ADR-015.md).

**Do not claim `RAYNU-V-M7-ISO-INSTALL-OK`.** HDA `last_commit` stays `2b795a0`.

---

## Stop rules

- Do not F11 `c144001` / `--run 33571164257` again (`pcicmd=0x5` still `ataio=0`).
- Do not F11 `060c504` / `--run 33569757025` again (`seq=0,0,0,0,0,0`).
- Do not F11 `abba969` / `--run 33567464001` again (honor `pcicmd=0` `wr=0` still `ataio=0`).
- Do not F11 `184ee61` / `--run 33562028442` again (`cmdwr=6` last `wr=0x0` stored `0x1`).
- Do not F11 `21dc562` / `--run 33559849096` again (skip-HLT then same CpuSleep).
- Do not F11 `e3cbfa5` / `--run 33558261624` again (one-shot then HLT hang).
- Do not F11 `24c5fa6` / `--run 33555104832` again (PIT livelock).
- Do not F11 `b5c3a9c` / `--run 33440050729` again (skip-after-inject CpuSleep).
- Do not flash `2d6b109` (dest skip) or `8024439` (later IdeBus).
- No new EFI SHA without a COM2 line that fails the current step.
- One SHA per fail. Not an IdeBus PCI farm. Host/CI never prints the iron marker.
- Do not OR PCI command `0x0001`. COMMAND path is closed.

---

## Ladder

| # | Status | Proof | This COM2 (`c144001`) |
|---|--------|-------|------------------------|
| 0 | DONE | ADR-015 | — |
| 1–2 | DONE | Flash pin `--run 33571164257` | Cruzer `FLASH-OK`, iso 63 MiB |
| 3a | **DONE** | dest_ok + ACPI MADT + wait-for-irq + one-shot + skip-HLT | unchanged |
| 3b | **FAIL** | Linux `efi:` contains `ACPI=` | `pcicmd=0x5` then same `rip=0x7f0680d0` `ataio=0` |
| 4–6 | BLOCKED | `/init` · disk · `ISO-INSTALL-OK` | — |

Iron notes:

- [`docs/evidence/r640/2026-09-01-c144001-enableattr-still-ataio0.md`](evidence/r640/2026-09-01-c144001-enableattr-still-ataio0.md) — EnableAttributes `pcicmd=0x5`, still `ataio=0`

---

## Current wall

COMMAND is closed. EnableAttributes `0x0005` is on the wire. Firmware still never issues ATA. Same CpuSleep. `ataio=0`.

This #229 HEAD: print last PCI CF8 on the HLT stall. Do not F11 `c144001` / `33571164257`. Stay on #229. Do not flash `8024439`. Do not resume #231 for `COMMAND.IO`.

Next proof: COM2 HLT `cf8=0x…` (BDF Connect parked on), then `ataio>0` or the same hang. Still not `ISO-INSTALL-OK`.
