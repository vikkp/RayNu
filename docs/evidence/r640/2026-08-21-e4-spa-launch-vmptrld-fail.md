# R640 — E4 SPA VMLAUNCH then G0 VMPTRLD fail (2026-08-21)

**Claim:** hang-fix EFI `f413a9fc` on Cruzer `RAYNUV` reached SPA start on
real PowerEdge R640. COM2 printed `RAYNU-V-M7-E4-SPA-LAUNCH-OK` (SHELL CPUID
in the G1 2 MiB slab), then `sched VMPTRLD failed slot=00000000`, `VMXOFF ok`,
`boot gate failed`.

**Not claimed:** E4 closed. Mount Everest. Distro installer. TLS / `auth.token`.
The marker printed and the boot gate then failed — that is not an iron close.

## Kit

| Field | Value |
|-------|--------|
| EFI SHA256 | `f413a9fc9a4c2b15b64259e863d3984a771eb3fa6b70e9270fe4748db0a4ecc1` |
| EFI size | **1226240** |
| Artifact | **`9429378906`** (commit `1a29e33` / hang fix `a32232c`) |
| Media | Cruzer Micro, front USB 2, label `RAYNUV` |
| Lease | `10.99.99.149:8443` (native BCM5720 after `BOOT-OK`) |
| Station | `b0:26:28:5c:5a:38` (`01:00.0` / `:38`) |
| COM2 | [`logs/2026-08-21-e4-spa-vmcs-vmptrld-fail-com2.txt`](logs/2026-08-21-e4-spa-vmcs-vmptrld-fail-com2.txt) |

Keep `ape-nophylock=yes`. Bind LOM `:38`. Do not write PERC.

## What this proves

| Gate | Status | Evidence |
|------|--------|----------|
| Spec + start HTTP | **OK on iron** | `VmCreated` / `VmStarted` + `HOST-NIC-HTTP-OK` (`method_tag=2`) |
| E4 SHELL VMLAUNCH | **marker printed** | `E4 SPA VMLAUNCH slot=1 private 2M EPT` then `RAYNU-V-M7-E4-SPA-LAUNCH-OK` |
| Coexist after E4 | **FAIL** | `sched VMPTRLD failed slot=00000000` → VMXOFF → `boot gate failed` |
| E4 closed | **no** | mgmt plane died; do not flash this as a close |

## Cause (in-tree follow-up)

G0 VMCS lives in G0 precise identity (~92 MiB, guest RAM). Linux can scribble
it. Phase F only schedules G0, so the CPU keeps a cached current VMCS and
coexist HTTP-OK still works. E4 `VMPTRLD`s the G1 slab VMCS; the next
`pick_next_fair` can select slot 0 and `VMPTRLD` the scribbled G0 page.

Follow-up (this tree, not on Cruzer `f413a9fc`): `VMCLEAR` G0, copy 4 KiB to a
host-only 2 MiB slab punched from G0 EPT, `VMLAUNCH` G0 on first return;
scheduler fail-soft must not `VMXOFF` the mgmt plane.

## Serial excerpt

```text
RAYNU-V-AUDIT: VmStarted guest_id=1
boot: E4 SPA VMLAUNCH slot=1 private 2M EPT (VMCS in slab; not G0 identity)
boot: VMLAUNCH → E4 SPA SHELL CPUID
RAYNU-V-M7-E4-SPA-LAUNCH-OK
boot: HOST-NIC TCP accept — client connected
boot: ERROR — sched VMPTRLD failed slot=00000000
boot: VMXOFF ok
boot: boot gate failed
```

## Next (new EFI — do not reuse `f413a9fc` for this fix)

Flash Cruzer **by SHA** of the relocate+fail-soft EFI. Leave `installdisk.bin`
and `auth.token`. F11 Cruzer. No SPA/browser. spec → `sleep 2` → start.

WANT: marker **and** continued coexist (`HOST-NIC-HTTP-OK` after SHELL). No
`VMPTRLD failed slot=0`. No `boot gate failed`.
