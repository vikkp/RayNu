# R640 — firmware SNP unusable after ExitBootServices (2026-08-17)

**Not claimed:** `RAYNU-V-M7-POST-EBS-HTTP-OK`. Do **not** claim Mount Everest.

**Still true on the same boot (skip-immediate-poll EFI):** PRE-EBS
`RAYNU-V-M7-UEFI-HTTP-OK`, E5 `RAYNU-V-M7-ISO-BOOTED-FROM-DISK`, E2
`RAYNU-V-R640-BOOT-OK`.

Media: Cruzer Micro, front USB 2. Do **not** re-image Cruzer for this residual.
Replace only `EFI/BOOT/BOOTX64.EFI`; leave `EFI/RayNu/installdisk.bin`.

## What we tried

Parked SNP + smoltcp across EBS (`Box::leak`, no CloseProtocol), then listen
after VMXOFF.

| Step | Result |
|------|--------|
| PRE-EBS SNP `:8443` | Worked — Mac SPA + Bearer `/vms` |
| SNP poll immediately after EBS | **Hung** — COM2 stuck at `boot: smoke frame phys=0x1000000` |
| Skip immediate poll; guest path | Worked — SHELL / M4 / `BOOTED-FROM-DISK` / `R640-BOOT-OK` |
| Idle listen after `BOOT-OK` | Banner printed; Mac curl to parked lease → **timeout (28)** |
| After that idle poll | Dell UEFI **RSOD** (BIOS 2.2.11) |

Lease/MAC unchanged from PRE-EBS: `10.99.99.133:8443` / `b0:26:28:5c:5a:3a`.

## Immediate-poll hang (do not re-test)

EFI SHA-256 `924af89408622cd34ede3aec848d61b6149de03564c10a592036b08dc47a9c94`
(1 178 624 bytes) polled SNP immediately after EBS. First `iface.poll` never
returned. Guest path never started. **Do not flash this EFI again.**

## Skip-poll idle (curl timeout, then RSOD)

After `a41dd71` (serial-only probe: `not polling SNP yet`), COM2 reached:

```text
boot: mgmt HTTP listening on 10.99.99.133:8443 (POST-EBS SNP idle)
boot: CURL NOW (post-EBS) → http://10.99.99.133:8443/
```

Those lines printed **before** the idle SNP poll loop. Mac on the same LAN that
worked PRE-EBS:

```text
curl: (28) Connection timed out after 5002 milliseconds
```

Both `GET /` and Bearer `/vms` timed out.

Then firmware exception (red screen, BIOS 2.2.11):

- Message: exception during UEFI pre-boot; restart required
- Type: Invalid opcode (06)
- Source: Software (UEFI0004) on BSP
- **RIP = `0000000000000017`** (jump to garbage)
- RSP `0000000005BFF970`, RAX `00000000481A1A90`, Flags `00010002`
- CurrentTPL `04`; LBR not available

This is a firmware exception after the post-EBS idle SNP path was entered —
not a clean hang.

## Honesty

Firmware SNP Transmit/Receive is **not** a durable post-EBS NIC on this boot
method. Tcp4 was already a platform limit. Do not chase either after EBS.

PRE-EBS 45s window remains the working management UI.

Durable post-EBS HTTP needs a **host-owned** NIC path (MMIO/DMA after EBS),
not `SimpleNetwork` protocol calls.

This tree: after `BOOT-OK`, print WARN and return. Never `iface.poll` on
firmware SNP. Never print a fake `CURL NOW (post-EBS)`.

## Retest — WARN-only, no RSOD (2026-08-17)

Vignesh flashed the skip-poll / WARN-only EFI (Cruzer, same lease). COM2
reached M4.5 → VMXOFF → `RAYNU-V-R640-BOOT-OK` (×3) then:

```text
boot: WARN — POST-EBS SNP idle skipped; firmware SNP dead after EBS lease=10.99.99.133:8443 (PRE-EBS was the mgmt window; do not chase SNP/Tcp4)
```

**No red screen.** Guests and E2 stayed green. That closes ADR-013 Phase A
on iron. Preserve kit: `releases/v0.1.0-adr013-baseline/`.

Log: [`logs/2026-08-17-snp-warn-no-rsod-com2.txt`](logs/2026-08-17-snp-warn-no-rsod-com2.txt).
