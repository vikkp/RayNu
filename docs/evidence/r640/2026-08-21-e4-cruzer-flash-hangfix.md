# R640 — E4 hang-fix EFI on Cruzer (2026-08-21)

**Claim:** Cruzer Micro `RAYNUV` now holds hang-fix `r640-hypervisor.efi`
(`f413a9fc…`, size `1226240`) from artifact `9429378906` (commit `1a29e33` /
hang fix `a32232c`). Helper printed `RAYNU-V-CRUZER-FLASH-OK`.
`installdisk.bin` (1024 bytes) and `auth.token` were left alone.

**Not claimed:** `RAYNU-V-M7-E4-SPA-LAUNCH-OK`. Hang-fix iron boot is
recorded in [`2026-08-21-e4-hangfix-boot.md`](2026-08-21-e4-hangfix-boot.md).
This is not a distro installer and not Mount Everest.

Hung first E4 EFI `67b0acde` must not be reflashed. Phase F `0d06297b` is the
last closed coexist kit, not this boot.

## Kit

| Field | Value |
|-------|--------|
| Git HEAD (pin) | `1a29e33` |
| Hang-fix feature | `a32232c` (remap slot 1 only after `M4_LADDER_DONE`) |
| Branch | `cursor/e4-spa-launch-a623` (PR #169) |
| Artifact | `r640-hypervisor.efi` id **`9429378906`** |
| EFI SHA256 | `f413a9fc9a4c2b15b64259e863d3984a771eb3fa6b70e9270fe4748db0a4ecc1` |
| EFI size | **1226240** |
| Media | Cruzer Micro, front USB 2, label `RAYNUV` |
| Block device | `/dev/sdc` (`/dev/disk/by-label/RAYNUV`) |
| Serial | `200524441218e7503e33` |
| Helper | `tools/flash-cruzer-esp.sh` from `cursor/raynuvsrv1-cruzer-flash-a623` |

Keep `ape-nophylock=yes`. Bind LOM `:38` / `01:00.0`. Do **not** write PERC
`sda`/`sdb`. Do **not** flash `67b0acde`, `0d06297b`, `c16cbffd`, `9fc6a3c2`,
`26573eb1`, `42b42c99`, `ec08c00f`, or `1404f055`.

## What this proves

| Gate | Status | Evidence |
|------|--------|----------|
| Cruzer ESP write | **OK** | [`logs/2026-08-21-e4-cruzer-flash-hangfix.txt`](logs/2026-08-21-e4-cruzer-flash-hangfix.txt) |
| Hang-fix remap | in the flashed binary | `a32232c` — do not remap G1→G0 during the M4.2 ladder |
| Iron boot of `f413a9fc` | **OK** | [`2026-08-21-e4-hangfix-boot.md`](2026-08-21-e4-hangfix-boot.md) |
| E4 SPA VMLAUNCH | **open** | COM2 `RAYNU-V-M7-E4-SPA-LAUNCH-OK` not claimed |

## Next on iron

1. iDRAC SOL `console com2` **before** power.
2. One-time F11 boot Cruzer `RAYNUV`. BIOS order stays Ubuntu on PERC.
3. Ignore PRE-EBS SNP `CURL NOW` / 45s `mgmt HTTP accept timeout`.
4. After `RAYNU-V-M4-SLICE-G0`, COM2 **must** show `boot: sched switch → slot=00000001` (hang-fix proof). Then NVM / BLK / NET / SMP / `RAYNU-V-R640-BOOT-OK`.
5. Curl **only** after native:
   `HOST-NIC coexist listening on 10.99.99.149:8443 (VMX on; ADR-013 Phase F)`
   (use the lease COM2 prints; it may not be `.149`).
6. Then `POST /vms/1/spec/1/512/1024/0` and `POST /vms/1/start`.
7. WANT COM2: `E4 SPA VMLAUNCH slot=1 private 2M EPT` then `RAYNU-V-M7-E4-SPA-LAUNCH-OK`.

Runbook: [`docs/runbooks/ops_ui.md`](../../runbooks/ops_ui.md).
