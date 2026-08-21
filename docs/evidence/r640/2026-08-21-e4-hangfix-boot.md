# R640 — E4 hang-fix boot (2026-08-21)

**Claim:** hang-fix EFI `f413a9fc` booted on real PowerEdge R640 from Cruzer
`RAYNUV`. After `RAYNU-V-M4-SLICE-G0`, COM2 printed
`boot: sched switch → slot=00000001` then `RAYNU-V-M4-SLICE-G1` (the
`67b0acde` hang). M4 ladder, `RAYNU-V-R640-BOOT-OK`, and Phase F coexist
HTTP on `10.99.99.149:8443` all printed. Native `HOST-NIC-HTTP-OK` while
VMX on; G1–G3 parked.

**Not claimed:** `RAYNU-V-M7-E4-SPA-LAUNCH-OK`. This paste is GET-only
(`AuthAllowed method_tag=1`). No `E4 SPA VMLAUNCH slot=1 private 2M EPT`.
Not a distro installer. Not Mount Everest.

## Kit

| Field | Value |
|-------|--------|
| EFI SHA256 | `f413a9fc9a4c2b15b64259e863d3984a771eb3fa6b70e9270fe4748db0a4ecc1` |
| EFI size | **1226240** |
| Artifact | **`9429378906`** (commit `1a29e33` / hang fix `a32232c`) |
| Media | Cruzer Micro, front USB 2, label `RAYNUV` |
| Lease | `10.99.99.149:8443` (native BCM5720 after `BOOT-OK`) |
| Station | `b0:26:28:5c:5a:38` (`01:00.0` / `:38`) |
| COM2 | [`logs/2026-08-21-e4-hangfix-boot-com2.txt`](logs/2026-08-21-e4-hangfix-boot-com2.txt) |

Keep `ape-nophylock=yes`. Bind LOM `:38`. Do not write PERC.

## What this proves

| Gate | Status | Evidence |
|------|--------|----------|
| Hang-fix remap | **OK on iron** | `SLICE-G0` then `sched switch → slot=00000001` |
| E2 / M4 ladder | **OK** | SHELL → 2VM → SCHED → NVM/BLK/NET/SMP → `R640-BOOT-OK` |
| Phase F coexist | **OK** | `HOST-NIC coexist listening` + repeated `HOST-NIC-HTTP-OK` |
| E4 SPA VMLAUNCH | **open** | no `RAYNU-V-M7-E4-SPA-LAUNCH-OK` in this paste |

## Serial excerpt (hang-fix + coexist)

```text
RAYNU-V-M4-SLICE-G0
boot: sched switch → slot=00000001
RAYNU-V-M4-SLICE-G1
…
RAYNU-V-R640-BOOT-OK
boot: HOST-NIC coexist listening on 10.99.99.149:8443 (VMX on; ADR-013 Phase F)
boot: CURL NOW → http://10.99.99.149:8443/  (native BCM5720; G0 still scheduled; SNP is dead)
boot: HOST-NIC coexist — resume G0 (VMX on; G1–G3 parked)
RAYNU-V-M7-HOST-NIC-HTTP-OK
RAYNU-V-AUDIT: AuthAllowed method_tag=1
```

## Next (same boot — do not power off)

`POST /vms/1/spec/1/512/1024/0` then `POST /vms/1/start` with Bearer.
WANT COM2: `AuthAllowed method_tag=2`, `E4 SPA VMLAUNCH slot=1 private 2M EPT`,
`RAYNU-V-M7-E4-SPA-LAUNCH-OK`.

That start **ran**. Marker printed, then `VMPTRLD failed slot=0` / VMXOFF.
See [`2026-08-21-e4-spa-launch-vmptrld-fail.md`](2026-08-21-e4-spa-launch-vmptrld-fail.md).
Chassis is down. Do not claim E4 closed. Flash the relocate+fail-soft EFI next.
