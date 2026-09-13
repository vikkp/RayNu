# Nested M8.0 File persist two-boot (`ce3d8a09`) — not ISO-INSTALL-OK

- **Host:** `raynuvsrv1` nested QEMU (KVM nested VT-x, `nested=Y`, `enable_shadow_vmcs=N`)
- **Commit:** `ce3d8a09` (`fix(m8): do not abort MODE=full on success skip persist`)
- **Harness:** `REBUILD_EFI=0 MODE=full PRODUCT_ISO=~/projects/raynuv/alpine-extended-3.21.3-x86_64.iso ./tools/m8-persist-nested.sh`
- **ISO:** alpine-extended 3.21.3 `1042284544` bytes
- **File-RAM:** `target/m8-persist.img` `3584M` (`3758096384` bytes), share=on
- **Honesty:** Nested QEMU ≠ R640. Harness printed `RAYNU-V-M8-DISK-PERSIST-NESTED-OK`. Do **not** print `RAYNU-V-M7-ISO-INSTALL-OK`. Do **not** print iron `RAYNU-V-M8-DISK-PERSIST-OK`. Do not F11.

## Boot 1 (empty persist, ISO wins)

```
==> leftover/File persist attached (not 64 MiB pool) (not ISO-INSTALL-OK)
boot: Stage 46 persist install disk hpa=0x20000000 bytes=536870912 (nested File RAM; not ISO-INSTALL-OK)
boot: Stage 46 leftover install disk skip persist (not ISO-INSTALL-OK)
boot: report-RAM extra hpa=0x40000000 bytes=1005510656 (Stage 46; not ISO-INSTALL-OK)
boot: Stage 46 virtio-blk install disk bytes=536870912 keep=0 (not ISO-INSTALL-OK)
RAYNU-V-M7-E5-OVMF-VMLAUNCH-OK
boot: RayNu-F VMLAUNCH … image=ISO-BOOTX64
[    0.000000] Linux version 6.12.13-0-lts
==> boot1 install complete; flushing persist file then killing HV (not guest F7)
==> persist file GPT EFI PART at 0x20000000 (not ISO-INSTALL-OK)
```

`keep=0` is boot1 empty File persist. Success skip persist has no `avail=` (carve-fail). Not the 64 MiB pool (`67108864`).

## Boot 2 (same File-RAM after HV kill)

```
boot: Stage 46 persist install disk hpa=0x20000000 bytes=536870912 (nested File RAM; not ISO-INSTALL-OK)
boot: Stage 46 virtio-blk install disk bytes=536870912 keep=1 (not ISO-INSTALL-OK)
boot: RayNu-F GPT ESP lba=2048 sectors=98304 part=1 (F7; not ISO-INSTALL-OK)
boot: RayNu-F disk whole-disk path (F7; not ISO-INSTALL-OK)
boot: RayNu-F found \EFI\BOOT\BOOTX64.EFI bytes=139264 (F7 disk; not ISO-INSTALL-OK)
boot: RayNu-F VMLAUNCH … image=DISK-BOOTX64
RAYNU-V-RAYNU-F-DISK-BOOT-OK
==> nested install → kill HV → second Linux without setup-disk (not iron, not ISO-INSTALL-OK)
RAYNU-V-M8-DISK-PERSIST-NESTED-OK
```

Harness required boot2 `keep=1`, no `setup-disk`, and `DISK-BOOT-OK` plus `Linux version`. Nested-OK is QEMU on `raynuvsrv1`, not flashcruzer, not iron Force Off.

## Not this

- Iron `RAYNU-V-M8-DISK-PERSIST-OK` (Force Off / reboot RayNu-V on the real R640)
- `RAYNU-V-M7-ISO-INSTALL-OK` (Everest iron-only)
- M8.1 TLS
