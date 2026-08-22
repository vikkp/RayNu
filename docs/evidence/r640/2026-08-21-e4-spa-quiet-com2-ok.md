# R640 — E4 COM2 quiet after first re-entry (2026-08-21)

**Claim:** Cruzer flashed the `v0.1.0-e4-spa-launch` quiet-COM2 EFI
(source `832ea32` / kit SHA256 `00443957…`). Spec + start HTTP printed
`VmCreated` / `VmStarted` and `HOST-NIC-HTTP-OK`. First SPA `VMLAUNCH`
printed `RAYNU-V-M7-E4-SPA-LAUNCH-OK`. Scheduler then logged **one** G0
clear-state re-entry, **one** SPA clear-state re-entry, **one** shadow
restore per slot (98 fields), then:

```text
boot: HINT — COM2 quiet after first E4 re-entry (HTTP/WARN only; switch loop continues)
```

No further `E4 G0 VMLAUNCH` / `E4 restore` / `E4 SPA VMLAUNCH` lines in
the paste. Switch loop continues (`VMLAUNCH` after `VMCLEAR`). **No**
`insn_error` 7 or 11, **no** `VMXOFF`.

This closes the “in-tree only” caveat on quiet COM2. P0-14 itself was
already closed on `2b795a0` the same day
([shadow re-entry](2026-08-21-e4-spa-shadow-reentry-ok.md)).

**Not claimed:** Mount Everest. Distro guest. TLS. `VMRESUME`. HTTP
exchanges *during* the quiet switch loop (paste ends at the HINT).

## Kit

| Field | Value |
|-------|--------|
| Tree / EFI | `v0.1.0-e4-spa-launch` (`832ea32`) |
| Release | https://github.com/vikkp/RayNu/releases/tag/v0.1.0-e4-spa-launch |
| CI EFI SHA256 | `0044395754d942545507e75cdd0fecf702241a36cd8b9bd60c27e2149eba4906` |
| Media | Cruzer Micro, front USB 2, label `RAYNUV` |
| Station | BCM5720 `:38` / `01:00.0` (same LOM as E3b / P0-14) |
| COM2 | [`logs/2026-08-21-e4-spa-quiet-com2-ok-com2.txt`](logs/2026-08-21-e4-spa-quiet-com2-ok-com2.txt) |

Keep `ape-nophylock=yes`. Bind LOM `:38`. Do not write PERC.

## What this proves

| Gate | Status | Evidence |
|------|--------|----------|
| Spec + start HTTP | **OK** | `AuthAllowed method_tag=2` (POST) + `VmCreated` / `VmStarted` + HTTP-OK |
| G0 VMCS clone | **OK** | `fields=98 rip=0xffffffff81160299`; relocated `0x10a00000` |
| First SPA VMLAUNCH | **OK** | `RAYNU-V-M7-E4-SPA-LAUNCH-OK` |
| First-only switch logs | **OK** | one G0 re-entry, one SPA re-entry, one restore per slot |
| Quiet HINT | **OK on iron** | `COM2 quiet after first E4 re-entry` |
| Repeating quantum spam | **stopped** | paste ends at HINT |
| Everest E4 / M7 | **no** | SHELL stub; TLS/console/distro residual |

## Serial excerpt

```text
RAYNU-V-M7-E4-SPA-LAUNCH-OK
boot: E4 G0 VMLAUNCH (VMCS relocated; was VMCLEAR)
boot: E4 restore VMCS shadow slot=00000000 fields=98
boot: E4 SPA VMLAUNCH (VMCS was VMCLEAR; clear-state re-entry)
boot: E4 restore VMCS shadow slot=00000001 fields=98
boot: HINT — COM2 quiet after first E4 re-entry (HTTP/WARN only; switch loop continues)
```

## Honesty

Quiet COM2 ≠ `VMRESUME`. Guests still switch every quantum. Product next
stays [ADR-014](../../adr/ADR-014.md) installer + TLS/console.
