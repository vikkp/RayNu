# R640 — E4 SPA VMLAUNCH + clear-state re-entry OK (2026-08-21)

**Claim:** Cruzer `RAYNUV` flashed shadow-restore EFI `2b795a0`. Mac spec
**201** + start **200** on `10.99.99.126:8443`. First slot-1 `VMLAUNCH`
printed `RAYNU-V-M7-E4-SPA-LAUNCH-OK` (SHELL CPUID). Scheduler then
`VMCLEAR`'d the outgoing VMCS, restored the 98-field software shadow, and
`VMLAUNCH`'d the incoming guest. G0 and slot 1 both re-entered that way for
many quanta. **No** `insn_error` 7 or 11, **no** all-zero ctls, **no**
`VMXOFF`, **no** fail-soft park.

**P0-14 / `RAYNU-V-M7-E4-SPA-LAUNCH-OK` closed on iron** for private-EPT SPA
start + clear-state re-entry.

**Not claimed:** Mount Everest. Full Everest E4 (TLS, guest console, distro
installer). HTTP during the G0↔SPA switch loop (re-listen after start only;
no GET in this paste). `VMRESUME` (every switch is `VMLAUNCH` because
outgoing is `VMCLEAR`'d). Dual Linux guests.

## Kit

| Field | Value |
|-------|--------|
| Tree / EFI | shadow restore after `VMCLEAR` (`2b795a0`) |
| Media | Cruzer Micro, front USB 2, label `RAYNUV` |
| Lease | `10.99.99.126:8443` (native BCM5720 after `BOOT-OK`) |
| Station | `b0:26:28:5c:5a:38` (`01:00.0` / `:38`) |
| Mac spec | **201 Created** |
| Mac start | **200 OK** |
| COM2 | [`logs/2026-08-21-e4-spa-shadow-reentry-ok-com2.txt`](logs/2026-08-21-e4-spa-shadow-reentry-ok-com2.txt) |

Keep `ape-nophylock=yes`. Bind LOM `:38`. Do not write PERC.

Prior fail that this EFI closed:
[`2026-08-21-e4-norewrite-spa-zeros.md`](2026-08-21-e4-norewrite-spa-zeros.md)
(re-entry ctls all 0 / error 7).

## What this proves

| Gate | Status | Evidence |
|------|--------|----------|
| Spec + start HTTP | **OK on iron** | Mac 201 then 200; `VmCreated` / `VmStarted` + `HOST-NIC-HTTP-OK` |
| G0 VMCS clone | **verified** | `fields=98 rip=0xffffffff81160299`; relocated `0x10a00000` |
| First SPA VMLAUNCH | **OK** | `RAYNU-V-M7-E4-SPA-LAUNCH-OK` |
| Shadow restore | **OK** | `E4 restore VMCS shadow slot=0/1 fields=98` each switch |
| G0 re-entry after VMCLEAR | **OK** | `E4 G0 VMLAUNCH (VMCS relocated; was VMCLEAR)` many cycles |
| Slot 1 re-entry after VMCLEAR | **OK** | `E4 SPA VMLAUNCH (VMCS was VMCLEAR; clear-state re-entry)` many cycles |
| Fail-soft / VMXOFF | **not taken** | no park, no error 7/11 |
| P0-14 iron close | **yes** | first clean slot-1 re-entry |
| Everest E4 / M7 | **no** | SHELL stub; TLS/console/distro residual |

## Serial excerpt

```text
RAYNU-V-AUDIT: VmStarted guest_id=1
boot: E4 G0 VMCS clone fields=98 rip=0xffffffff81160299
boot: E4 G0 VMCS relocated HPA=0x0000000010a00000
RAYNU-V-M7-E4-SPA-LAUNCH-OK
boot: E4 G0 VMLAUNCH (VMCS relocated; was VMCLEAR)
boot: E4 restore VMCS shadow slot=00000000 fields=98
boot: E4 SPA VMLAUNCH (VMCS was VMCLEAR; clear-state re-entry)
boot: E4 restore VMCS shadow slot=00000001 fields=98
```

Mac:

```text
HTTP/1.1 201 Created
HTTP/1.1 200 OK
```

First HTTP after coexist was `AuthAllowed` without `VmCreated` (likely
`GET /`). Spec then start followed. Operator paste truncated mid-loop on
another G0 `VMLAUNCH`.

## Next (not blocking P0-14)

Optional: skip `VMCLEAR` when launch-state is launched and `VMRESUME`
instead, to stop the VMLAUNCH-every-quantum loop. Optional: `GET /` during
the switch loop to strengthen coexist-during-E4. Product residual is
TLS/console + a real distro installer — not more NIC bring-up.
