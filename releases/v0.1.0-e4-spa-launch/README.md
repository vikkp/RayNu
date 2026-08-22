# v0.1.0-e4-spa-launch — P0-14 + ADR-013 Stage 1

**Preserve point** after merging [#169](https://github.com/vikkp/RayNu/pull/169)
to `main`. Freezes SPA private-EPT VMLAUNCH, ADR-013 Phase G accepted-risk
(shared LOM), ADR-014 typed ISO, and COM2 quiet after the first E4 re-entry.

## What this freezes

| Area | State |
|------|--------|
| E2 | `RAYNU-V-R640-BOOT-OK` (closed) |
| E3 | `RAYNU-V-M7-UEFI-HTTP-OK` SNP residual PRE-EBS (closed) |
| E3b | `RAYNU-V-M7-HOST-NIC-HTTP-OK` on BCM5720 `:38` after `BOOT-OK` (closed) |
| Phase F | coexist HTTP while VMX on (closed 2026-08-20) |
| Phase G | **Closed** as accepted-risk: host HTTP + guest virtio-net share LOM `:38` |
| P0-14 / E4 | `RAYNU-V-M7-E4-SPA-LAUNCH-OK` on iron EFI `2b795a0` (SHELL stub + shadow re-entry) |
| ADR-014 | Typed ISO (`linux_iso` / `windows_iso` / `generic_uefi`); bzImage lab-only |
| Open | Mount Everest. Product next: distro installer + TLS/console |

## Provenance

| Field | Value |
|-------|--------|
| Git tip | `b6578f5` — merge #169 |
| EFI source | `832ea32` (COM2 quiet after first E4 re-entry) |
| Iron P0-14 EFI | `2b795a0` (repeating switch-loop serial; same VMLAUNCH path) |
| Path | Native BCM5720 after `BOOT-OK`; keep APE PHY; bind LOM `:38` |
| CI EFI SHA256 | `0044395754d942545507e75cdd0fecf702241a36cd8b9bd60c27e2149eba4906` |
| CI run | [32537177085](https://github.com/vikkp/RayNu/actions/runs/32537177085) |
| Iron evidence | `docs/evidence/r640/2026-08-21-e4-spa-shadow-reentry-ok.md` |

COM2 fingerprint (iron `2b795a0` close — first lines only; this kit quiets after that pair):

```text
RAYNU-V-M7-E4-SPA-LAUNCH-OK
boot: E4 G0 VMLAUNCH (VMCS relocated; was VMCLEAR)
boot: E4 restore VMCS shadow slot=00000000 fields=98
boot: E4 SPA VMLAUNCH (VMCS was VMCLEAR; clear-state re-entry)
boot: E4 restore VMCS shadow slot=00000001 fields=98
```

This kit then prints `COM2 quiet after first E4 re-entry` and stays quiet
except HTTP / WARN / markers. **Closed on iron** 2026-08-21 — evidence:
[`docs/evidence/r640/2026-08-21-e4-spa-quiet-com2-ok.md`](../../docs/evidence/r640/2026-08-21-e4-spa-quiet-com2-ok.md).

## Verify / remap

```bash
( cd releases/v0.1.0-e4-spa-launch && shasum -a 256 -c r640-hypervisor.efi.sha256 )
./tools/make-boot-media.sh --kit releases/v0.1.0-e4-spa-launch
```

Cruzer `RAYNUV`: replace **only** `EFI/BOOT/BOOTX64.EFI`. Leave
`EFI/RayNu/installdisk.bin`. Keep `ape-nophylock=yes`. Bind LOM `:38`.
Do not write PERC. Safe shutdown is iDRAC **Force Off**.

Mac-built iron floppy digests may differ by toolchain from this Linux CI
rebuild of the same source — same caveat as `v0.1.0-e4-spa-arm`.

## Honesty

Does **not** claim Mount Everest, a Linux distro guest, TLS, `VMRESUME`,
802.1Q, or a dedicated mgmt NIC. Quiet COM2 is on iron; switches are
still `VMLAUNCH` after `VMCLEAR`. Guest from SPA start is SHELL CPUID.
Preserve kit for NIC rollback remains `releases/v0.1.0-adr013-baseline`.
