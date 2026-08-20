# R640 — ADR-013 Phase F closed (2026-08-20)

**Claim:** native BCM5720 HTTP beside VMX on real Dell PowerEdge R640.
After `RAYNU-V-R640-BOOT-OK`, COM2 printed `HOST-NIC coexist listening`
`10.99.99.149:8443` **`(VMX on; ADR-013 Phase F)`**, resumed **G0**
(`G1–G3 parked`), then repeated `RAYNU-V-M7-HOST-NIC-HTTP-OK` and
`AuthAllowed` with **no** `sched VMPTRLD failed` / **no** `VMXOFF`.
Operator SPA stayed up (Refresh / logs / other buttons).

A later same-EFI boot held COM2 through **25** `HOST-NIC-HTTP-OK` lines
(full paste: [`logs/2026-08-20-phase-f-coexist-hold-com2.txt`](logs/2026-08-20-phase-f-coexist-hold-com2.txt)).
The first close paste ended mid-accept when SOL went quiet; this hold log is
the confirmation COM2 can stay up beside VMX-on HTTP.

**Not claimed:** Mount Everest. E4 polish (TLS/console) and a real distro
installer remain. SPA create/start is the mgmt table only — it does **not**
VMLAUNCH a new guest. PRE-EBS SNP HTTP does **not** count (timed out this boot).

## Closing kit

| Field | Value |
|-------|--------|
| Git HEAD (pin) | `b5c1605` |
| Feature | `0e94b5b` (park G1–G3 after M4 ladder) |
| Branch | `cursor/m7-8-coexist-a623` (PR #168) |
| CI run | `32424402214` (success, 97 checks) |
| Artifact | `r640-hypervisor.efi` id **`9426883522`** |
| EFI SHA256 | `0d06297b6409908d4e9bff905892df1f161e9f57702ad1728aaf587df0136d04` |
| EFI size | **1223680** |
| Media | Cruzer Micro, front USB 2, label `RAYNUV` |
| Lease | `10.99.99.149:8443` (parked SNP IPv4; native Device after EBS) |
| Station | `b0:26:28:5c:5a:38` (live LOM / `01:00.0`) |

Keep `ape-nophylock=yes`. Do **not** flash take-PHY / skip-CORECLK / `c16cbffd` /
`9fc6a3c2`.

## What closed

| Gate | Marker | Evidence |
|------|--------|----------|
| ADR-013 Phase F iron | coexist + `HOST-NIC-HTTP-OK` while VMX on | [`logs/2026-08-20-phase-f-coexist-ok-com2.txt`](logs/2026-08-20-phase-f-coexist-ok-com2.txt) |
| Phase F COM2 hold | 25× `HOST-NIC-HTTP-OK`; SOL stayed up | [`logs/2026-08-20-phase-f-coexist-hold-com2.txt`](logs/2026-08-20-phase-f-coexist-hold-com2.txt) |
| E2 (same boot) | `RAYNU-V-R640-BOOT-OK` | guest path through M4, VMX **stays on** |
| E5 stamps (same boot) | `RAYNU-V-M7-ISO-BOOTED-FROM-DISK` | persist-detect still green |
| E3b (same path) | `RAYNU-V-M7-HOST-NIC-HTTP-OK` | now with G0 scheduled, not post-`VMXOFF` idle |

## Path (honest)

1. PRE-EBS SNP residual leased `:38` / `10.99.99.149`. PRE-EBS HTTP window
   timed out.
2. Post-EBS: inherit SNP analog, `CORECLK_RESET` for DMA, skip BMCR,
   `ape-nophylock=yes`, `grc=bswap+wswap (Linux LE tg3)`, `link=up`.
3. Guest path green through SHELL / M4.1–M4.5 / E2.
4. After `BOOT-OK`: SNP idle skipped. Native coexist arm **without VMXOFF**.
5. `resume G0 (VMX on; G1–G3 parked)` — G1–G3 SHELL stubs are not scheduled
   (G0 precise EPT identity-maps their VMCS; earlier `c16cbffd` EFI died on
   `sched VMPTRLD failed` after SPA HTTP).
6. Repeated TCP accept / HTTP exchange / `HOST-NIC-HTTP-OK` / `AuthAllowed`.
   Operator SPA remained up. No `VMXOFF`. No boot-gate fail.
7. Hold boot (same EFI): COM2 stayed through 25 HTTP-OK. Mixed
   `AuthAllowed` / `AuthDenied` on GET (`method_tag=1`) is a Bearer miss on
   REST, not a hang. Paste ends on `TCP accept` (operator iDRAC power-off).

## Serial excerpt

```text
RAYNU-V-R640-BOOT-OK
boot: HOST-NIC coexist listening on 10.99.99.149:8443 (VMX on; ADR-013 Phase F)
boot: CURL NOW → http://10.99.99.149:8443/  (native BCM5720; G0 still scheduled; SNP is dead)
boot: HOST-NIC coexist — resume G0 (VMX on; G1–G3 parked)
boot: HOST-NIC TCP accept — client connected
boot: HOST-NIC HTTP exchange ok
RAYNU-V-M7-HOST-NIC-HTTP-OK
RAYNU-V-AUDIT: AuthAllowed method_tag=1
boot: HOST-NIC HTTP exchange ok
RAYNU-V-M7-HOST-NIC-HTTP-OK
```

First close paste: [`logs/2026-08-20-phase-f-coexist-ok-com2.txt`](logs/2026-08-20-phase-f-coexist-ok-com2.txt).  
Hold confirmation: [`logs/2026-08-20-phase-f-coexist-hold-com2.txt`](logs/2026-08-20-phase-f-coexist-hold-com2.txt).

## Honesty

- QEMU `HOST-NIC-QEMU-OK` does not count.
- PRE-EBS `RAYNU-V-M7-UEFI-HTTP-OK` does not count.
- Phase D / E3b (post-`VMXOFF` first-accept) closed earlier the same day.
- Phase G (NIC auto-select / VLAN split) is later. `:38` is the lab default.
- SPA create/start does not VMLAUNCH.
- Preserve kit for rollback: `releases/v0.1.0-adr013-baseline`.
