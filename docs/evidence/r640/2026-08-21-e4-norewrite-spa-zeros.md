# R640 — no-incoming-rewrite first SPA OK, re-entry zeros (2026-08-21)

**Claim:** After dropping the incoming-VMCS revision rewrite, Cruzer `RAYNUV`
reached SPA spec **201** + start **200** on `10.99.99.126:8443`. First slot-1
`VMLAUNCH` printed `RAYNU-V-M7-E4-SPA-LAUNCH-OK` (SHELL CPUID). Scheduler
`VMCLEAR`'d slot 1, `VMLAUNCH`'d G0 (OK), then clear-state re-entry of slot 1
failed `insn_error=0x7` with **all-zero** pin/primary/secondary/exit/entry/
EPTP/RIP. Fail-soft parked SPA and `VMLAUNCH`'d G0 (VMX on, no `VMXOFF`).

**Not claimed:** E4 closed. Mount Everest. Dual-guest coexist after SPA start.
Distro installer. TLS. Marker + first VMLAUNCH ≠ a working slot-1 resume.

## Kit

| Field | Value |
|-------|--------|
| Tree | no-incoming-rewrite (`564d39d` + flashcruzer) |
| Media | Cruzer Micro, front USB 2, label `RAYNUV` |
| Lease | `10.99.99.126:8443` (native BCM5720 after `BOOT-OK`) |
| Station | `b0:26:28:5c:5a:38` (`01:00.0` / `:38`) |
| Mac spec | **201 Created** |
| Mac start | **200 OK** |
| COM2 | [`logs/2026-08-21-e4-norewrite-spa-zeros-com2.txt`](logs/2026-08-21-e4-norewrite-spa-zeros-com2.txt) |

Keep `ape-nophylock=yes`. Bind LOM `:38`. Do not write PERC.

## What this proves

| Gate | Status | Evidence |
|------|--------|----------|
| Spec + start HTTP | **OK on iron** | Mac 201 then 200; `VmCreated` / `VmStarted` + `HOST-NIC-HTTP-OK` |
| G0 VMCS clone | **verified** | `fields=98 rip=0xffffffff81160299` |
| First SPA VMLAUNCH | **OK** | `RAYNU-V-M7-E4-SPA-LAUNCH-OK` |
| G0 re-entry after VMCLEAR | **OK** | `E4 G0 VMLAUNCH (VMCS relocated; was VMCLEAR)` |
| Slot 1 re-entry | **FAIL** | `VMLAUNCH(gprs) insn_error=0x00000007`; ctls all 0 |
| Fail-soft | **held VMX** | park slot 1; `E4 G0 VMLAUNCH after SPA entry fail` |
| E4 closed | **no** | second `VMLAUNCH` of slot 1 is not safe |

## Cause

SDM: software must not assume `VMCLEAR` leaves VMCS data unmodified. After
`VMCLEAR` of slot 1 the next `VMPTRLD`+`VMLAUNCH` saw an empty current VMCS
(error 7 = invalid control field). G1's 2 MiB slab (VMCS at `0x10409000`) was
still in G0's precise identity map while Linux ran between the two launches.

Follow-up (this tree): capture [`VMCS_CLONE_FIELDS`] into a software shadow
**before** `VMCLEAR`; `VMWRITE` that shadow after `VMPTRLD` before any
clear-state `VMLAUNCH`. Punch the SPA 2 MiB slab out of G0 EPT. Do not
`VMLAUNCH` a `VMCLEAR`'d VMCS without restoring fields.

## Serial excerpt

```text
RAYNU-V-M7-E4-SPA-LAUNCH-OK
boot: E4 G0 VMLAUNCH (VMCS relocated; was VMCLEAR)
boot: E4 SPA VMLAUNCH (VMCS was VMCLEAR; clear-state re-entry)
boot: ERROR — VMLAUNCH(gprs) failed insn_error=0x00000007
boot: VMCS ctls pin=0x00000000 primary=0x00000000 secondary=0x00000000 exit=0x00000000 entry=0x00000000
boot: EPTP=0x0000000000000000 link=0x0000000000000000 rip=0x0000000000000000
boot: HINT — SPA VMLAUNCH fail; park slot=1 resume G0
boot: E4 G0 VMLAUNCH after SPA entry fail (VMX on; no VMXOFF)
```

Mac:

```text
HTTP/1.1 201 Created
HTTP/1.1 200 OK
```

## Next

On `raynuvsrv1`: `~/projects/raynuv/flashcruzer.sh` (WANT `RAYNU-V-CRUZER-FLASH-OK`).
F11 Cruzer. spec → `sleep 2` → start. WANT COM2: `E4 restore VMCS shadow slot=`
non-zero ctls on re-entry; no error 7/11. No `VMXOFF`.
