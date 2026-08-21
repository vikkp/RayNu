# R640 — 63cd694f clone + SPA marker + G0 VMLAUNCH, then slot 1 error 11 (2026-08-21)

**Claim:** clone EFI `63cd694f` on Cruzer `RAYNUV` reached SPA start on real
PowerEdge R640. COM2 printed VMREAD/VMWRITE clone verify (98 fields, Linux
RIP `0xffffffff81160299`), `RAYNU-V-M7-E4-SPA-LAUNCH-OK`, then G0 `VMLAUNCH`
of the relocated VMCS at `0x10a00000`. The next scheduler tick `VMPTRLD`
slot 1 at `0x10409000` failed with `VM_INSTRUCTION_ERROR=11` (incorrect VMCS
revision identifier). Fail-soft resumed slot 0 (`VMX on; no VMXOFF`). Mac
spec **201** + start **200** on `10.99.99.126:8443`.

**Not claimed:** E4 closed. Mount Everest. Dual-guest coexist after SPA start.
Distro installer. TLS / `auth.token`. Marker + G0 clone + G0 VMLAUNCH ≠ a
working slot-1 resume.

## Kit

| Field | Value |
|-------|--------|
| EFI SHA256 | `63cd694f4f1f1bd8f8e11641df151811707bb063dd49676bb418b1d778348878` |
| EFI size | **1231872** |
| Artifact | **`9461155533`** (run `32522622376`; clone `33155c1` / pin `7d657b1`) |
| Media | Cruzer Micro, front USB 2, label `RAYNUV` |
| Lease | `10.99.99.126:8443` (native BCM5720 after `BOOT-OK`) |
| Station | `b0:26:28:5c:5a:38` (`01:00.0` / `:38`) |
| COM2 | [`logs/2026-08-21-e4-g0clone-spa-slot1-rev11-com2.txt`](logs/2026-08-21-e4-g0clone-spa-slot1-rev11-com2.txt) |

Keep `ape-nophylock=yes`. Bind LOM `:38`. Do not write PERC.
Mac: `curl -4 --noproxy '*'`. GET `/` may `curl: (56)` after HTTP 200 (abort+re-listen).

## What this proves

| Gate | Status | Evidence |
|------|--------|----------|
| Spec + start HTTP | **OK on iron** | Mac POST spec **201** / start **200**; `VmCreated` / `VmStarted` + `HOST-NIC-HTTP-OK` |
| G0 VMCS clone | **verified** | `E4 G0 VMCS clone fields=98 rip=0xffffffff81160299` (`VMPTRLD verify ok`) |
| G0 relocate | **printed** | `E4 G0 VMCS relocated HPA=0x0000000010a00000` |
| E4 SHELL VMLAUNCH | **marker printed** | `RAYNU-V-M7-E4-SPA-LAUNCH-OK` |
| G0 re-entry | **OK** | `E4 G0 VMLAUNCH (VMCS relocated; was VMCLEAR)` |
| Fail-soft | **held VMX** | `park VMPTRLD fail; resume slot=00000000 (VMX on; no VMXOFF)` |
| Slot 1 resume | **FAIL** | `VMPTRLD` `phys=0x10409000` error **11** (revision) |
| E4 closed | **no** | SPA guest ran once; second `VMPTRLD` of slot 1 is not safe |

## Cause

Intel SDM App. C error 11 = `VMPTRLD` with incorrect VMCS revision identifier.
Leaving slot 1 **current** and `VMPTRLD` G0 implicitly flushes the SPA VMCS
at `0x10409000` in an implementation-specific form (revision dword / shadow
bit 31). Nested VT-x in this tree already rewrites the revision after
`VMCLEAR` before the first `VMPTRLD` (`prepare_vmcs_region`). The scheduler
did not `VMCLEAR` the outgoing VMCS.

Follow-up (this tree, not on Cruzer `63cd694f`): `VMCLEAR` the outgoing E4
VMCS, `rewrite_vmcs_revision` (first dword = `IA32_VMX_BASIC[30:0]`, bit 31
clear, **do not** zero the 4K), `VMLAUNCH` on re-entry (`SPA_NEEDS_VMLAUNCH`),
sticky-park slot 1 on fail (`SPA_VMPTRLD_FAILED` / `SPA_RUNNABLE=false`).

## Serial excerpt

```text
boot: E4 G0 VMCS clone fields=98 rip=0xffffffff81160299 (VMREAD/VMWRITE; VMPTRLD verify ok)
boot: E4 G0 VMCS relocated HPA=0x0000000010a00000 (host slab; VMREAD/VMWRITE clone; punched from G0 identity)
boot: E4 SPA VMLAUNCH slot=1 private 2M EPT (VMCS in slab; not G0 identity)
boot: VMLAUNCH → E4 SPA SHELL CPUID
RAYNU-V-M7-E4-SPA-LAUNCH-OK
boot: E4 G0 VMLAUNCH (VMCS relocated; was VMCLEAR)
boot: ERROR — sched VMPTRLD failed slot=00000001 phys=0x0000000010409000
boot: VM_INSTRUCTION_ERROR=11
boot: WARN — park VMPTRLD fail; resume slot=00000000 (VMX on; no VMXOFF)
```

Mac:

```text
HTTP/1.1 201 Created
HTTP/1.1 200 OK
```

## Next (new EFI — do not reuse `63cd694f` for this fix)

Flash Cruzer **by SHA** of the VMCLEAR+revision-rewrite EFI. Leave
`installdisk.bin` and `auth.token`. F11 Cruzer. spec → `sleep 2` → start.

WANT: clone verify + `RAYNU-V-M7-E4-SPA-LAUNCH-OK` + G0 VMLAUNCH **and**
slot 1 `VMPTRLD`/`VMLAUNCH` without error 11; or one park HINT and quiet
COM2 with HTTP-OK. No repeating `VMPTRLD failed`. No `VMXOFF` / `boot gate failed`.
