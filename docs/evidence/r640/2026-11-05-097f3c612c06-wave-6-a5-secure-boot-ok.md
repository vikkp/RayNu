# 2026-11-05 — `097f3c612c06`: signed EFI under Secure Boot (ADR-021 A5 lived)

**EFI:** branch `40a746e4` (PR #431), pull-request merge stamp. COM2 `build: sha=097f3c612c06 run=42062748313`.
**File:** unsigned CI artifact SHA256 `67dbe9ff4a3f75849f1840282e1a84afef105ee45ea5866f0678935d2b3accb1` (2,579,456 bytes). Operator certificate `CN=RayNu-V Secure Boot` signed it. `sbverify` printed `Signature verification OK`. Signed file SHA256 `b78b0fa68276a6407bcda7802976e2498466e15e3ca636f52af41b848a424470` (2,581,024 bytes). The private key stayed in `~/raynu-keys`. It is not in git.
**Flash:** Lexar, label `RAYNUV`, serial `A144053100000F82`, front USB 2. `installdisk.bin` and `auth.token` were left alone. DER certificate `raynu-v-db.cer` was copied to the stick root and to `EFI/RayNu/`.
**Rollback:** unsigned stick of the same stamp, and Secure Boot set back to Disabled. Older wave-2 pin remains `v0.1.0-m8-9-lan`.

## BIOS

System Setup, System Security. Secure Boot Policy set to Custom. Secure Boot Mode was Deployed Mode. Authorized Signature Database (`db`) after Import:

- Microsoft Corporation UEFI CA 2011
- Microsoft Windows Production PCA 2011
- VMware, Inc. VMware Secure Boot Signing
- RayNu-V Secure Boot

The Microsoft and VMware entries were left in place. Secure Boot was then set to Enabled. One-shot F11, Boot Manager, One-shot UEFI Boot Menu, front USB 2.

## COM2

```text
Booting from Disk connected to front USB 2: USB Flash Drive
RAYNU-V-M0-BOOT-OK
build: sha=097f3c612c06 run=42062748313
boot: E4 cert rd lba=6190268891 operator
boot: HOST-NIC TLS wrap=tls13
boot: D9 host addr static 10.99.99.201/24
boot: B5 autostart start off=19327352832
[    0.000000] secureboot: Secure boot disabled
Welcome to Ubuntu 26.04 LTS!
raynusrv2 login:
```

`secureboot: Secure boot disabled` is the guest. Linux was booted by RayNu-F, and RayNu-F does not publish the host firmware's SecureBoot variable. The same line was on the earlier boot with Secure Boot off. Dell's check is that this signed file reached `RAYNU-V-M0-BOOT-OK` after the certificate was in `db` and Secure Boot was Enabled. An unsigned file is rejected before that line.

`boot: D10 ntp none` on this boot after `boot: D10 ntp query 162.159.200.123`. D10 stays lived on the earlier reply. Not an A5 failure.

**A5 lived.** Wave 6 stays open on A7. Not a new `ISO-INSTALL-OK`. HDA stays 99%.
