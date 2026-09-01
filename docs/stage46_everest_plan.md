# Stage 46 Option 2 — Everest move plan

Living tracker for [PR #229](https://github.com/vikkp/RayNu/pull/229).
Decision: [ADR-015](adr/ADR-015.md).

**Do not claim `RAYNU-V-M7-ISO-INSTALL-OK`.** HDA `last_commit` stays `2b795a0`.

---

## Stop rules

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
- Do not OR PCI command `0x0001`.

---

## Ladder

| # | Status | Proof | This COM2 (`060c504`) |
|---|--------|-------|------------------------|
| 0 | DONE | ADR-015 | — |
| 1–2 | DONE | Flash pin `--run 33569757025` | Cruzer `FLASH-OK`, iso 63 MiB |
| 3a | **DONE** | dest_ok + ACPI MADT + wait-for-irq + one-shot + skip-HLT | unchanged |
| 3b | **FAIL** | Linux `efi:` contains `ACPI=` | `seq=0,0,0,0,0,0` then same `rip=0x7f0680d0` `ataio=0` |
| 4–6 | BLOCKED | `/init` · disk · `ISO-INSTALL-OK` | — |

Iron notes:

- [`docs/evidence/r640/2026-09-01-060c504-cmdwr-seq-all-zero.md`](evidence/r640/2026-09-01-060c504-cmdwr-seq-all-zero.md) — six COMMAND writes all `0`

---

## Current wall

Firmware wrote IDE COMMAND six times. Every write is `0`. EnableAttributes never set IO. Honor and `OR 0x0001` are closed. Same CpuSleep hang. `ataio=0`.

This #229 HEAD: after write-0, restore EnableAttributes `0x0005` (IO+BM). Do not OR `0x0001`. Do not F11 `060c504` / `33569757025`. Stay on #229. Do not flash `8024439`.

Next proof: COM2 `IDE pci EnableAttributes` and HLT `pcicmd=0x5` with `seq=0,0,0,0,0,0`, then `ataio>0`. Still not `ISO-INSTALL-OK`.
