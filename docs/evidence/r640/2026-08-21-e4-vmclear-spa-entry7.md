# R640 — eb456eec VMCLEAR re-entry then SPA VMLAUNCH error 7 (2026-08-21)

**Claim:** VMCLEAR+revision-rewrite EFI on Cruzer `RAYNUV` reached SPA start on
real PowerEdge R640. Error 11 is gone: slot 1 `VMPTRLD` succeeded and the
clear-state re-entry `VMLAUNCH` ran. That `VMLAUNCH` failed with
`insn_error=0x7` (VM-entry with invalid control field(s), SDM App. C).
Fail-soft idled with VMX on (no `VMXOFF`). Mac spec curl `(28)` / start **200**.
HV still printed `VmCreated` then `VmStarted`.

**Not claimed:** E4 closed. Mount Everest. Dual-guest coexist after SPA start.
Distro installer. TLS / `auth.token`. Marker + G0 VMLAUNCH + error 7 ≠ a working
slot-1 resume.

## Kit

| Field | Value |
|-------|--------|
| EFI prefix | **`eb456eec`** (green PR run `32527757679`) |
| EFI SHA256 | `eb456eec2eca04e6a52ad2b2e5bb31033f7cb112d943d13bf33732afdaacc2ae` |
| EFI size | **1232384** |
| Artifact | **`9462835178`** |
| Media | Cruzer Micro, front USB 2, label `RAYNUV` |
| Lease | `10.99.99.126:8443` (native BCM5720 after `BOOT-OK`) |
| Station | `b0:26:28:5c:5a:38` (`01:00.0` / `:38`) |
| COM2 | [`logs/2026-08-21-e4-vmclear-spa-entry7-com2.txt`](logs/2026-08-21-e4-vmclear-spa-entry7-com2.txt) |

Keep `ape-nophylock=yes`. Bind LOM `:38`. Do not write PERC.
Mac: `curl -4 --noproxy '*'`. Spec `(28)` with 0 bytes can still be `VmCreated`
on COM2 (HV `abort()` after HTTP). Start **200** is the launch.

## What this proves

| Gate | Status | Evidence |
|------|--------|----------|
| Spec + start HTTP | **OK on iron** | `VmCreated` / `VmStarted` + `HOST-NIC-HTTP-OK`; Mac start **200** |
| G0 VMCS clone | **verified** | `fields=98 rip=0xffffffff81160299` |
| E4 SHELL VMLAUNCH | **marker printed** | `RAYNU-V-M7-E4-SPA-LAUNCH-OK` |
| G0 re-entry | **OK** | `E4 G0 VMLAUNCH (VMCS relocated; was VMCLEAR)` |
| Slot 1 `VMPTRLD` | **OK** | no error 11; reached `E4 SPA VMLAUNCH (VMCS was VMCLEAR)` |
| Slot 1 re-entry | **FAIL** | `VMLAUNCH(gprs) insn_error=0x00000007` |
| Fail-soft | **held VMX** | `VM-entry fail-soft idle (VMX on; coexist)` |
| E4 closed | **no** | SPA ran once; second `VMLAUNCH` of slot 1 is not safe |

## Cause

SDM App. C error 7 = VM-entry with invalid control field(s). The scheduler
`VMCLEAR`'d the outgoing VMCS (fixes error 11) then also rewrote the
**incoming** region's first dword without `VMCLEAR` of that region. `VMPTRLD`
accepted the revision; VM-entry then rejected the control encoding.

Follow-up (this tree): rewrite revision **only** on the region just
`VMCLEAR`'d. Do not rewrite the incoming VMCS. Dump pin/primary/secondary/
exit/entry/EPTP/link/RIP on `VMLAUNCH` fail. Park slot 1 and `VMLAUNCH` G0
instead of spinning idle.

## Serial excerpt

```text
RAYNU-V-M7-E4-SPA-LAUNCH-OK
boot: E4 G0 VMLAUNCH (VMCS relocated; was VMCLEAR)
boot: E4 SPA VMLAUNCH (VMCS was VMCLEAR; clear-state re-entry)
boot: ERROR — VMLAUNCH(gprs) failed insn_error=0x00000007
boot: WARN — VM-entry fail-soft idle (VMX on; coexist)
```

Mac:

```text
curl: (28) Operation timed out after 20005 milliseconds with 0 bytes received
HTTP/1.1 200 OK
```

## Next (new EFI — do not reuse `eb456eec` / `26db0610` for this fix)

On `raynuvsrv1`: `~/projects/raynuv/flashcruzer.sh` (WANT `RAYNU-V-CRUZER-FLASH-OK`).
Leave `installdisk.bin` and `auth.token`. F11 Cruzer. spec → `sleep 2` → start
(spec `(28)` is OK if COM2 shows `VmCreated`).

WANT: clone verify + marker + G0 VMLAUNCH **and** slot 1 re-entry without
error 7/11; or park HINT + G0 resume + HTTP-OK. No `VMXOFF`. No spinning idle
that drops G0.
