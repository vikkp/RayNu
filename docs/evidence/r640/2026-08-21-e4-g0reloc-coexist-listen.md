# R640 — 618e89e2 coexist listen, curl SYN timeout (2026-08-21)

**Claim:** relocate EFI `618e89e2` booted from Cruzer `RAYNUV`. Hang-fix remap
held (`SLICE-G0` then `sched switch → slot=00000001`). M4 ladder +
`RAYNU-V-R640-BOOT-OK`. Native coexist listen on **`10.99.99.126:8443`**
(VMX on; G0 scheduled).

**Not claimed:** `RAYNU-V-M7-E4-SPA-LAUNCH-OK` this boot (no SPA start).
Mac `POST spec` / `POST start` both `curl: (28)` 20s. COM2 had **no**
`HOST-NIC TCP accept`. First RX dumps were LAN multicast `to=other` only.

## Kit

| Field | Value |
|-------|--------|
| EFI prefix | `618e89e2` (artifact `9432035922`, `acba27b`) |
| Media | Cruzer `RAYNUV` front USB 2 |
| SNP/native lease | `10.99.99.126:8443` (same LOM MAC `:38` Ubuntu used) |
| Station | `b0:26:28:5c:5a:38` (`01:00.0`) |
| COM2 | [`logs/2026-08-21-e4-g0reloc-coexist-listen-com2.txt`](logs/2026-08-21-e4-g0reloc-coexist-listen-com2.txt) |

## Honesty

`.126` is the HV lease **this boot**, not Ubuntu (Ubuntu is off after Force Off).
Curl `(28)` is SYN timeout, not RST `(7)`. Do not Force Off while coexist is
still printing RX — probe ARP/ping/`GET /` from the Mac on the LOM LAN.
