# Runbook — Real PowerEdge R640 boot (M7.5)

**Iron marker:** `RAYNU-V-R640-BOOT-OK` (real R640 only)  
**Scaffold smoke:** `./tools/m7-r640-smoke.sh` → `RAYNU-V-M7-R640-SCAFFOLD-OK`  
**Evidence:** [`docs/evidence/r640/`](../evidence/r640/)  
**Operator checklist:** [`r640_iron_week.md`](r640_iron_week.md)  
**Printable field guide:** [`r640_field_guide.md`](r640_field_guide.md)  
**Prior ship kit:** [`usb_idrac.md`](usb_idrac.md) · `./tools/package-release.sh`

## Story

M7.5 is the **hard gate** for Mount Everest (ADR-009): boot `r640-hypervisor.efi`
on a **real Dell PowerEdge R640** via USB or iDRAC virtual media, capture COM1 /
iDRAC serial, and observe VMX + EPT + Linux shell path (or document residual).

**Latitude / QEMU cannot close this gate.** Host scaffold smoke only proves the
runbook and evidence template exist — it does **not** print
`RAYNU-V-R640-BOOT-OK`.

## Prerequisites

1. Release kit from M7.0 (`./tools/package-release.sh`) with verified SHA256.  
2. PowerEdge R640 with iDRAC (virtual console + virtual media).  
3. Operator laptop with serial capture (iDRAC virtual console or COM1 redirect).

## Boot procedure

Follow [`usb_idrac.md`](usb_idrac.md) to place `r640-hypervisor.efi` as
`\EFI\BOOT\BOOTX64.EFI` on USB or iDRAC vMedia, then:

1. Open iDRAC **virtual console** (serial/COM1) **before** reboot.  
2. One-time boot to USB / virtual media.  
3. Capture the full serial log to a file.  
4. Confirm at minimum:
   - `RAYNU-V-M0-BOOT-OK`
   - VMX path markers (`RAYNU-V-M1-VMXON-OK` / `RAYNU-V-M1-VMEXIT-OK` when VT-x available)
   - EPT / guest path as far as iron reaches (document residual if short of shell)
5. Record EFI SHA256, iDRAC method (USB vs vMedia), date, and operator in the
   evidence template.

## Evidence package (required to claim iron marker)

Copy [`docs/evidence/r640/TEMPLATE.md`](../evidence/r640/TEMPLATE.md) to a dated
file (e.g. `2026-MM-DD-r640-first-light.md`), fill every field, attach or paste
a serial excerpt that shows the required markers, then open a close PR that:

1. Sets `docs/evidence/r640/STATUS` to `STATUS=closed` with a pointer to the
   filled evidence file.  
2. Changes the GAP to `GAP(CLOSED M7.5): Real R640 boot`.  
3. Updates `docs/progress.md` / HDA E2 / site — **only after** real iron proof.

Until then, `STATUS=open` and scaffold smoke stays green without claiming boot.

## Acceptance (iron)

```text
RAYNU-V-R640-BOOT-OK
==> M7.5 real R640 boot PASSED
```

Printed only from a close path backed by filled evidence — **never** from
`./tools/m7-r640-smoke.sh` on a laptop/CI host.

## Limits

- Nested KVM on Latitude is **not** R640.  
- Live Tier‑1 Redfish health is a follow-on (P0-3), not required for first light.  
- Console/TLS/firmware NIC residuals from M7.1–M7.4 remain lab-path residuals.
