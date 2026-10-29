# 2026-10-29 — `ddbaf11cd11c`: the page says the guest is running, then why it stopped (ADR-021 B20 lived; wave 2 closed)

**EFI:** pull-request build of `cff61509` (PR #419). `GITHUB_SHA` on that run is the merge onto `main`, `ddbaf11cd11c1fe0da4b337942a5c8b8cd9450b1`. CI pull-request run `40900796962`, green. The build log prints `sha=ddbaf11cd11c run=40900796962`.
**Flash:** Lexar, label `RAYNUV`, front USB 2, F11 one-shot. Ubuntu 26.04 standing on UBUNTU0. Guest: the 10 GiB tail on RAYNU-SPARE, vNIC saved, Start, then Stop from the Guests page.
**Rollback:** `v0.1.0-m8-9-lan`. Disks unchanged. No doorbell.

## Build line

Under the tiles, on both screenshots:

```
build ddbaf11cd11c · CI 40900796962
```

Same SHA and run as COM2 `build: sha=ddbaf11cd11c run=40900796962`.

## Guest running

The Guest tile read `Guest 3 is running`. The Guests row for the 10240 MiB disk at `@18432` read `RUNNING`, with Stop enabled. The 8 GiB window stayed `STOPPED`. The spare note read `Booting the free-tail guest. setup-disk stays withheld.`

## Guest stopped

After Stop, the tile read `Guest is stopped`. All three rows read `STOPPED`. The spare note read `Guest stopped. Start boots it again. The hypervisor stays up.` The line under the tiles:

```
Stopped from the Guests page. reason=0x34 rip=0xffffffff85b2bebd
```

`0x34` is a preemption-timer exit (52). That is the exit the vCPU was in when Stop landed, not a fault. The sentence is the operator stop. **B20 lived.** With D4, D5, D6 and A3, wave 2 is closed.

## The line that was already there while the guest ran

Before Stop, the same place read:

```
Exit cap reached. reason=0x1c rip=0xfffffe94
```

`0x1c` is a control-register access (28). `rip=0xfffffe94` is the first exit of every boot, the OVMF leg that RayNu-F bypasses (`guest-UEFI stop n=1 reason=0x1c rip=0xfffffe94`, then `RayNu-F direct`). The collapse cap on that leg is 1, so `n >= cap` labeled it an exit cap. The guest was not stopped. Stop replaced that record with the operator sentence. Noted. Not a second stop of guest 3.

## Not claimed

No new `RAYNU-V-*-OK` marker. Not ISO-INSTALL-OK, not 1.0, not GA. HDA stays 99%.
