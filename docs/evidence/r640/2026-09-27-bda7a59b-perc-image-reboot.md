# 2026-09-27 — `bda7a59b`: same login after a chassis restart

EFI `bda7a59bd597` (`cursor/m8-7-perc-hide-vdc-8366`), second iron boot.
Front USB 2 UDisk. Toshiba `0480:a004` still on xHCI p11. H740P Mini
`18:00.0`. UBUNTU0 was not written. The H840 was not mapped.

## What COM2 showed

```
build: sha=bda7a59bd597
boot: perc copy skip present alt=16777215
boot: perc virtio ro hidden (vda is the 8 GiB image; not PERC-BOOT-OK)
RAYNU-V-M8-PERC-BOOT-OK
[vda] 16777216 512-byte logical blocks (8.59 GB/8.00 GiB)
 vda: vda1 vda2
EXT4-fs (vda2): recovery complete
EXT4-fs (vda2): mounted filesystem a0ad99ac-ca25-4458-ade1-cd8599ca4928 ro
/dev/vda2: clean, 6380/521216 files, 102913/2084352 blocks
Welcome to Alpine Linux 3.21
localhost login: root
root=UUID=a0ad99ac-ca25-4458-ade1-cd8599ca4928
```

PCI enumeration listed virtio at `00:02.0` and `00:03.0`. It did not list
`00:04.0`. There was no `[vdc]`. The copy heartbeat was `skip present`,
not `perc copy start`.

## What this is

The 8 GiB image at the start of RAYNU-SPARE survived a hypervisor power
cycle. The file count matches the first login of this EFI. `fsck` cleared
the vfat dirty bit on `vda1` again.

## What this is not

Not a boot with the Toshiba unplugged. Not a boot of the whole spare.
Not a new install. Not `setup-disk`. Not `setup-alpine`. Not an
unmodified ISO. Host and CI do not print `RAYNU-V-M8-PERC-BOOT-OK`.
