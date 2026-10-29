# 2026-10-29 — `4ac1a629`: the saved vNIC is on the spare and comes back after a hypervisor reboot (ADR-021 C8 lived)

**EFI:** `4ac1a629` (PR #420). The `match` line exists only in this commit. Push CI `40911446044` stamps `build: sha=4ac1a6291d5e`. Pull-request CI `40911456697` stamps the merge `6bd02af06633`. The operator did not paste `build: sha=` on these boots. The first C8 stick, `94d0d369`, had printed `mismatch` and was not the reboot under test.
**Flash:** Lexar, label `RAYNUV`, front USB 2. One flash, then two F11 boots of that same stick. No second flash between them. Ubuntu 26.04 standing on UBUNTU0. No guest was started for the check.
**Rollback:** `v0.1.0-m8-9-lan`. Disks unchanged. No doorbell. The write is one sector at the high end of RAYNU-SPARE, LBA `6190268926`, the sector before the probe LBA `6190268927`. Not LBA 0. Not UBUNTU0.

## Boot 1 — Save, guest still off

The Guests row for the 10 GiB tail was `INSTALLED`. vNIC Save, nothing started. COM2, after the SPA was up and before `PercSpaChoice`:

```text
boot: perc nic catalog wr lba=6190268926 match
RAYNU-V-AUDIT: GuestNicSaved off=19327352832 mode=2
```

The page showed MAC `02:52:4e:38:0b:0d`, manual `10.99.99.200/24`, gateway `10.99.99.1`, DNS `1.1.1.1`.

`94d0d369` had printed `mismatch (staged, not acked)` on this same LBA. That WRITE frame was not in poll mode, so the READ won. `4ac1a629` posts the WRITE the same way as the spare-probe write.

## Boot 2 — same stick, hypervisor reboot

Power off host from the SPA, Ubuntu up, F11 the same Lexar. No reflash. In the PERC section, after the probe read of LBA `6190268927` and before that probe is written again:

```text
boot: perc read2 post … lba=6190268927 …
boot: perc read2 status=0x00 to=0 ok (not PERC-LUN-OK)
boot: perc nic catalog rd lba=6190268926 n=1
boot: perc write post … lba=6190268927 …
```

`n=1` is one NIC record loaded into the panel before the guest starts. The operator did not click Start.

vNIC on `10240 MiB@18432`, opened and not typed: manual, `10.99.99.200`, prefix `24`, gateway `10.99.99.1`, DNS `1.1.1.1`, MAC `02:52:4e:38:0b:0d`. The row stayed `INSTALLED`. **C8 lived** for the NIC catalog.

## Not claimed

Guest settings other than the NIC catalog are still RAM. G4 (audit across reboot) and C7 (virtio-blk FLUSH) are still planned, so wave 3 stays open. No new `RAYNU-V-*-OK` marker. Not ISO-INSTALL-OK, not 1.0, not GA. HDA stays 99%.
