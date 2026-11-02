# 2026-10-29 — `e6ed42a3`: the saved guest started with no SPA click (ADR-021 B5 lived)

**EFI:** `e6ed42a31ba4` (PR #423, CI run `40938025112`). COM2 `build: sha=e6ed42a31ba4 run=40938025112`.
**Flash:** Lexar, label `RAYNUV`, front USB 2. Two F11s of this stick. Ubuntu 26.04 standing on UBUNTU0. No reinstall. No doorbell. UBUNTU0 was not written.
**Rollback:** `v0.1.0-m8-9-lan`.

## Boot 1 — the operator saved the list

The spare image was latched. The NIC catalog and the audit chain were already there. The autostart sector was empty:

```text
boot: perc nic catalog rd lba=6190268926 n=1
boot: audit chain rd lba=6190268910 n=23
boot: B5 autostart none
```

The operator opened Guests and clicked **Boot next**. COM2:

```text
boot: B5 autostart wr lba=6190268909 match
```

That LBA is the sector immediately before the audit image (`RAYNUAUT`).

## Boot 2 — no SPA, no click

The operator F11'd the same stick and did not open the SPA. COM2 read the list and started the installed tail:

```text
boot: perc nic catalog rd lba=6190268926 n=1
boot: audit chain rd lba=6190268910 n=32
boot: B5 autostart rd lba=6190268909 n=1
boot: B5 autostart start off=19327352832
boot: perc SPA choice tail boot (free tail is vda; setup-disk withheld; 8 GiB window stays; not ISO-INSTALL-OK)
boot: Stage 46 virtio-blk install disk bytes=10737418240 keep=0 tail_off=19327352832
```

`19327352832` is the Ubuntu tail (18 GiB). While the initrd mounted `/sysroot`:

```text
boot: C7 virtio flush sync status=0x00
```

The guest reached:

```text
Welcome to Ubuntu 26.04 LTS!
Ubuntu 26.04 LTS raynusrv2 ttyS0
raynusrv2 login:
```

**B5 lived.** Wave 4 stays open (B4, D9, D10, H2).

## What this boot did not do

The PCI probe listed virtio-blk at `00:02.0` and did not list virtio-net `00:05.0`. Later:

```text
[FAILED] Failed to start systemd-networkd-wait-online.service - Wait for Network to be Online.
```

A Guests click calls `on_accepted_start` before the guest starts. This autostart path accepted the tail choice and did not. The saved address stayed on the spare and was not handed to the guest. The follow-up on this branch calls that hook and prints `boot: B5 autostart nic` or `boot: B5 autostart nic none`. Neither line is on this COM2. The address handoff is not part of this close.
