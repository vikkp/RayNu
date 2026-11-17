# 2026-11-17 — `4f581a4dcd93`: two product guests run side by side (ADR-021 B6)

**EFI:** push build of `4f581a4dcd93640486e68e1a112ec938ffe40619` on branch `cursor/wave-7-b6-seats-37d7` (PR #439). The Guests page read `build 4f581a4dcd93 · CI 44174137515`. The signed SHA256 was not pasted.
**Rollback:** `v0.1.6-wave6-closed`.
**Design:** [ADR-022](../../adr/ADR-022.md), one guest model and guest slots.

## What iron showed

Guest 3 (Ubuntu 26.04 on the 10 GiB tail at 18432 MiB) was started first and reached `raynusrv2 login:`.

The operator then pressed Start on guest 2 (Alpine on the 10 GiB tail at 8192 MiB) on the Guests page. COM2, in order:

```
boot: guest slot 1 disk bytes=10737418240 spare_off=8589934592 keep=0 (its own span of the spare; ADR-022; not ISO-INSTALL-OK)
boot: guest slot 1 machine VMCS=0x1a660b000 EPTP=0x1a660001e slab=0x1a6c00000 report_ram_mib=2016 disk_bytes=10737418240 ioapic_trap=1 hpet=1 dram_left_mib=58704 (ADR-022; not ISO-INSTALL-OK)
RAYNU-V-AUDIT: VmcsCreated vcpu_id=1
RAYNU-V-AUDIT: VmStarted guest_id=1
boot: guest slot 1 start guests_on=1 (one guest model, ADR-022; not ISO-INSTALL-OK)
boot: RayNu-F VMLAUNCH entry=0xb23000 system_table=0x801000 relocs=2 image=DISK-BOOTX64 wall_cap_s=600 (F2b/F5; not ISO-INSTALL-OK)
[g0] boot: guest scheduler: two guests share the CPU (2 ms quantum; ADR-022; not ISO-INSTALL-OK)
```

The second guest booted through the same RayNu-F path as the first: GRUB, `RAYNU-V-RAYNU-F-START-IMAGE-OK`, `RAYNU-V-RAYNU-F-DISK-BOOT-OK`, `RAYNU-V-M8-PERC-BOOT-OK`, `RAYNU-V-RAYNU-F-EBS-OK`. Linux 6.12.13-0-lts printed `IOAPIC[0]: apic_id 0, version 17, address 0xfec00000, GSI 0-23`, `boot: B8 virtio msi-x vec=23`, `vda: vda1 vda2`, mounted `ee851fd5-126e-4e44-9402-22fff99cc728`, and reached:

```
Welcome to Alpine Linux 3.21
Kernel 6.12.13-0-lts on an x86_64 (/dev/ttyS0)

localhost login: root
localhost:~# cat /proc/cmdline; mount | head -3
BOOT_IMAGE=/boot/vmlinuz-lts root=UUID=ee851fd5-126e-4e44-9402-22fff99cc728 ro ...
```

Status lines, 30 seconds apart, with both guests on:

```
boot: guests on=2 switches=14036 g0 runs=7018 cpu_ms=148253 g1 runs=7018 cpu_ms=15669 (ADR-022; not ISO-INSTALL-OK)
boot: guests on=2 switches=28842 g0 runs=14421 cpu_ms=163184 g1 runs=14421 cpu_ms=30737 (ADR-022; not ISO-INSTALL-OK)
boot: guests on=2 switches=43650 g0 runs=21825 cpu_ms=178128 g1 runs=21825 cpu_ms=45794 (ADR-022; not ISO-INSTALL-OK)
boot: guests on=2 switches=58282 g0 runs=29141 cpu_ms=193117 g1 runs=29141 cpu_ms=60808 (ADR-022; not ISO-INSTALL-OK)
boot: guests on=2 switches=73080 g0 runs=36540 cpu_ms=208052 g1 runs=36540 cpu_ms=75872 (ADR-022; not ISO-INSTALL-OK)
```

Both `runs=` counts move. Each guest gains about 15 seconds of CPU per 30 seconds.

With both guests on, from the operator Mac: `ssh vikkp@10.99.99.200` printed `Welcome to Ubuntu 26.04 LTS` and `vikkp@raynusrv2:~$`, and `ls -aul` answered. The Guests page read `2 guests running` and `More than one guest is on. Each row has its own Start and Stop.` Rows 2 and 3 were both `RUNNING`.

The operator then pressed Stop on the Alpine row:

```
boot: RayNu-F stop spa-stop exits=1422870 wall_ms=162684 svc=656742 svc_err=22 conout_ok=1 blk_rd=879 blk_wr=0 allocs=20 free_pages=494080 (F2b; not ISO-INSTALL-OK)
boot: M8.9 net tx=0 rx=0 sent=0 held=0 (not ISO-INSTALL-OK)
RAYNU-V-AUDIT: VmStopped guest_id=1
boot: guest slot 1 is off; 1 guest(s) still on (ADR-022; not ISO-INSTALL-OK)
```

**B6 lived.** Two product guests, each started from the Guests page on its own span of RAYNU-SPARE, ran at the same time and each answered at its own login. Wave 7 stays open on B7. HDA stays 99%.

## What this run did not show

- **Two guest NICs.** The Alpine guest has no saved vNIC (`M8.9 net tx=0 rx=0`). Ubuntu's `ssh` worked with two guests on. A split of the uplink between two guest MACs has not run.
- **After the Stop.** The paste ends at `guest slot 1 is off; 1 guest(s) still on`. Ubuntu answering after the Stop, and a second Start of the stopped row, were not pasted.
- **Shut down.** The row's ACPI Shut down was not pressed. It has not run on iron since Linux moved to the I/O APIC (B8).
- **More than one CPU.** B7 is not on this EFI. Each guest saw one CPU and half of one core.

## Lines that are not new claims

- `RAYNU-V-M7-ISO-INSTALL-OK` printed while Alpine replayed its ext4 journal (`EXT4-fs (vda2): INFO: recovery required on readonly filesystem`). A tail disk attaches with `keep=0`, so the first journal write takes the marker. Same spurious print as `2026-09-18-4af78b43`. It is not a new install.
- `local IPI:TIMEOUT` in the kernel NMI self-test is the same line a single Alpine guest prints (iron `f72b4276`, and the nested single-guest run of this build).
- Host lines printed while guest 0 was on the CPU carry `[g0]` (`[g0] boot: HOST-NIC TCP re-listen after HTTP slot=0`). They are hypervisor lines, not guest output.
- The page showed `Exit cap reached. reason=0x1c rip=0xfffffe94` while both guests ran. That is the first exit of every boot, the OVMF leg that RayNu-F bypasses (see `2026-10-29-ddbaf11c`). The page keeps one "last stop" record. It is not either guest.
