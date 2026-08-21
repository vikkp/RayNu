# R640 — 618e89e2 SPA marker then G0 VMPTRLD loop (2026-08-21)

**Claim:** relocate EFI `618e89e2` on Cruzer `RAYNUV` reached SPA start on
real PowerEdge R640. COM2 printed `E4 G0 VMCS relocated HPA=0x10a00000`,
`RAYNU-V-M7-E4-SPA-LAUNCH-OK` (SHELL CPUID in the G1 2 MiB slab), then
`sched VMPTRLD failed slot=00000000` in a loop. Fail-soft resumed slot 1
(`VMX on; no VMXOFF`). No `boot gate failed`.

**Not claimed:** E4 closed. Mount Everest. Dual-guest coexist after SPA start.
Distro installer. TLS / `auth.token`. Marker + fail-soft ≠ a working G0 resume.

## Kit

| Field | Value |
|-------|--------|
| EFI SHA256 | `618e89e2acf852e463c17dce0e33337e452caee6d43a6e3907cb18f392ff68b3` |
| EFI size | **1229312** |
| Artifact | **`9432035922`** (commit `acba27b` / relocate `7b750ab`) |
| Media | Cruzer Micro, front USB 2, label `RAYNUV` |
| Lease | `10.99.99.126:8443` (native BCM5720 after `BOOT-OK`) |
| Station | `b0:26:28:5c:5a:38` (`01:00.0` / `:38`) |
| COM2 | [`logs/2026-08-21-e4-g0reloc-spa-vmptrld-loop-com2.txt`](logs/2026-08-21-e4-g0reloc-spa-vmptrld-loop-com2.txt) |

Keep `ape-nophylock=yes`. Bind LOM `:38`. Do not write PERC.
Mac: `curl -4 --noproxy '*'`. GET `/` may `curl: (56)` after HTTP 200 (abort+re-listen).

## What this proves

| Gate | Status | Evidence |
|------|--------|----------|
| Spec + start HTTP | **OK on iron** | `VmCreated` / `VmStarted` + `HOST-NIC-HTTP-OK` (`method_tag=2`) |
| G0 VMCS memcpy relocate | **printed** | `E4 G0 VMCS relocated HPA=0x0000000010a00000` |
| E4 SHELL VMLAUNCH | **marker printed** | `E4 SPA VMLAUNCH slot=1 private 2M EPT` then `RAYNU-V-M7-E4-SPA-LAUNCH-OK` |
| Fail-soft | **held VMX** | `park VMPTRLD fail; resume slot=1 (VMX on; no VMXOFF)` — unlike hang-fix `f413a9fc` |
| G0 resume | **FAIL** | `VMPTRLD` of the memcpy'd page at `0x10a00000` fails every tick (COM2 flood) |
| E4 closed | **no** | SPA guest runs; G0 is not resumable; COM2 is unusable |

## Cause

Intel SDM: VMCS data format is implementation-specific. `VMCLEAR` at the
identity-pool HPA then `memcpy` 4 KiB to `0x10a00000` does not produce a
region this CPU will `VMPTRLD`. Because `G0_VMCS_RELOCATED` was set, the
scheduler kept picking slot 0.

Follow-up (this tree, not on Cruzer `618e89e2`): clone G0 with VMREAD/VMWRITE
into the host slab, `VMPTRLD` verify `GUEST_RIP`, sticky-park slot 0 on the
first failure, latch ERROR/WARN once.

## Serial excerpt

```text
RAYNU-V-AUDIT: VmStarted guest_id=1
boot: E4 G0 VMCS relocated HPA=0x0000000010a00000 (host slab; punched from G0 identity)
boot: E4 SPA VMLAUNCH slot=1 private 2M EPT (VMCS in slab; not G0 identity)
boot: VMLAUNCH → E4 SPA SHELL CPUID
RAYNU-V-M7-E4-SPA-LAUNCH-OK
boot: ERROR — sched VMPTRLD failed slot=00000000
boot: WARN — park VMPTRLD fail; resume slot=00000001 (VMX on; no VMXOFF)
```

## Next (new EFI — do not reuse `618e89e2` for this fix)

Flash Cruzer **by SHA** of the VMREAD/VMWRITE clone EFI. Leave `installdisk.bin`
and `auth.token`. F11 Cruzer. No SPA/browser. spec → `sleep 2` → start.

WANT: marker **and** no repeating `VMPTRLD failed slot=0`. Either G0
`VMLAUNCH` after clone, or a single park HINT and quiet COM2 with HTTP-OK.
No `boot gate failed`. Fail-soft must not VMXOFF.
