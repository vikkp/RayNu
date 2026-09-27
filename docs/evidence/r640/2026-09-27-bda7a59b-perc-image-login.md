# 2026-09-27 — `bda7a59b`: unattended login from the spare image

EFI `bda7a59bd597` (`cursor/m8-7-perc-hide-vdc-8366`). Front USB 2 UDisk.
H740P Mini `18:00.0`. UBUNTU0 was not written. The H840 was not mapped.

## What COM2 showed

```
build: sha=bda7a59bd597
boot: perc copy skip present alt=16777215
boot: perc virtio ro hidden (vda is the 8 GiB image; not PERC-BOOT-OK)
RAYNU-V-M8-PERC-WRITE-OK
RAYNU-V-M8-PERC-BOOT-OK
boot: RayNu-F found \EFI\BOOT\BOOTX64.EFI bytes=139264
image=DISK-BOOTX64
RAYNU-V-RAYNU-F-DISK-BOOT-OK
[vda] 16777216 512-byte logical blocks (8.59 GB/8.00 GiB)
 vda: vda1 vda2
[vdb] 2035712 512-byte logical blocks (1.04 GB/994 MiB)
EXT4-fs (vda2): recovery complete
EXT4-fs (vda2): mounted filesystem a0ad99ac-ca25-4458-ade1-cd8599ca4928 ro
Mounting root: ok.
Welcome to Alpine Linux 3.21
localhost login: root
localhost:~# cat /proc/cmdline
BOOT_IMAGE=/boot/vmlinuz-lts root=UUID=a0ad99ac-ca25-4458-ade1-cd8599ca4928
```

PCI enumeration listed virtio at `00:02.0` and `00:03.0`. It did not list
`00:04.0`. There was no `[vdc]`.

## What this is

The guest disk is the copied 8 GiB window at the start of RAYNU-SPARE.
The rest of the 2.9 TB VD stayed hidden. `fsck` cleared a vfat dirty bit
on `vda1` and reported `vda2` clean (`6380/521216` files). The operator
reached a root shell without a hand mount.

## What this is not

Not a boot of the whole spare. Not a format of UBUNTU0. Not
`setup-disk`. Not `setup-alpine`. Not an unmodified ISO. Not Everest
reopened. Host and CI do not print `RAYNU-V-M8-PERC-BOOT-OK`.
