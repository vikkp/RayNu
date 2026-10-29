# 2026-10-29 — `33488fb7`: the audit hash chain is on the spare and the next boot continues it (ADR-021 G4 lived)

**EFI:** `33488fb7e578` (PR #421, CI run `40923163642`). COM2 `build: sha=33488fb7e578 run=40923163642` on both boots.
**Flash:** Lexar, label `RAYNUV`, front USB 2. One flash, then two F11 boots of that same stick. No second flash between them. Ubuntu 26.04 standing on UBUNTU0. No guest was started. Save was not clicked.
**Rollback:** `v0.1.0-m8-9-lan`. Disks unchanged. No doorbell. The image is sixteen sectors at LBA `6190268910`, immediately before the NIC catalog at `6190268926`. Not LBA 0. Not UBUNTU0.

## Boot 1 — nothing saved yet

`0421c2fd` had printed `boot: audit chain rd none` and then served the SPA without a `wr` line. The flush ran only from the coexist idle, and a latched spare image never enters that idle.

`33488fb7` writes from the SPA wait. After the probe read of LBA `6190268927`:

```text
boot: perc nic catalog rd lba=6190268926 n=1
boot: audit chain rd none
```

Once the page was waiting, and again after `AuthAllowed`:

```text
boot: audit chain wr lba=6190268910 match
```

Each `match` is sixteen WRITE(16)s and sixteen readbacks of the `RAYNUAUD` image. The hash still ignores payloads.

## Boot 2 — same stick, hypervisor reboot

Power through Ubuntu, F11 the same Lexar. No reflash. Same place in the PERC section:

```text
boot: perc read2 status=0x00 to=0 ok (not PERC-LUN-OK)
boot: perc nic catalog rd lba=6190268926 n=1
boot: audit chain rd lba=6190268910 n=9
```

`n=9` is the joined ring at that read. This boot had already recorded `BootStarted` and `OvmfLiveEspBytesRetained`. The spare supplied the other seven links. `adopt_image` returns only when the spare chain verifies from genesis and the join still verifies. A torn image prints `boot: audit chain rd none`. **G4 lived.**

The NIC catalog line is still `n=1`. C8 stays lived.

## Not claimed

C7 (virtio-blk FLUSH) is still planned, so wave 3 stays open. No new `RAYNU-V-*-OK` marker. Not ISO-INSTALL-OK, not 1.0, not GA. HDA stays 99%.
