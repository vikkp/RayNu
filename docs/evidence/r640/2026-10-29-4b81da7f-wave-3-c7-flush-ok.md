# 2026-10-29 — `4b81da7f`: virtio FLUSH waited for the spare cache (ADR-021 C7 lived)

**EFI:** `4b81da7fe8e3` (PR #422, CI run `40930163881`). COM2 `build: sha=4b81da7fe8e3 run=40930163881`.
**Flash:** Lexar, label `RAYNUV`, front USB 2. One F11. Ubuntu 26.04 standing on UBUNTU0. The operator started the installed tail guest. No reinstall. No `sync` typed.
**Rollback:** `v0.1.0-m8-9-lan`. Disks unchanged. No doorbell. The command is SYNCHRONIZE CACHE (10) on the spare LD, IMMED clear, whole LD. UBUNTU0 is a different target and was not posted.

## Boot

The spare image was latched. The NIC catalog and the audit chain were still there:

```text
boot: perc nic catalog rd lba=6190268926 n=1
boot: audit chain rd lba=6190268910 n=16
```

Guests Start of the free-tail disk:

```text
RAYNU-V-AUDIT: PercSpaChoice choice=4
boot: perc SPA choice tail boot (free tail is vda; setup-disk withheld; 8 GiB window stays; not ISO-INSTALL-OK)
boot: Stage 46 virtio-blk install disk bytes=10737418240 keep=0 tail_off=19327352832
```

Linux 7.0.0-38 came up on `vda` (10.7 GB). While the initrd was mounting `/sysroot`, after virtio notify `n=2241` and before `Mounted sysroot.mount`:

```text
boot: C7 virtio flush sync status=0x00
```

That line is the first successful SYNCHRONIZE CACHE. Status `0x00` is the controller completing the command. The guest status byte is written after that return. There is no `boot: C7 virtio flush sync fail` line.

The boot continued through switch-root to:

```text
Welcome to Ubuntu 26.04 LTS!
Ubuntu 26.04 LTS raynusrv2 ttyS0
raynusrv2 login:
```

**C7 lived. Wave 3 closed** (C8, G4, C7).

## Not claimed

USB and NVMe device caches are not this command. A power cut still leaves the guest off until the next wave (B5). Not ISO-INSTALL-OK, not 1.0, not GA. HDA stays 99%.
